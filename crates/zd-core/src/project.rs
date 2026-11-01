//! The project: every revision of every object, the layers, and the history of applied runs.
//!
//! Revisions are shared and never change. An edit adds the next revision of the same object and a
//! delete adds a tombstone, so undo steps back exactly and redo steps forward again without
//! re-running any command (decision 0004).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::{Attributes, Body, Layer, LayerId, Object, ObjectId, RunId};

/// One change a command asks for. A run's changes apply together or not at all.
#[derive(Debug, Clone)]
pub enum Change {
    /// A new object on the named layer; the layer is created if it does not exist.
    Create { name: String, layer: String, attributes: Attributes, body: Body },
    /// The next revision of an object. Fields left `None` carry over. `base` is the revision the
    /// command read, so a change worked out from a stale revision is refused.
    Revise {
        id: ObjectId,
        base: u32,
        name: Option<String>,
        layer: Option<String>,
        attributes: Option<Attributes>,
        body: Option<Body>,
    },
    /// Deletes an object by adding a tombstone revision; undo brings it back.
    Delete { id: ObjectId, base: u32 },
    /// A new layer. Creating a layer that already exists is an error.
    CreateLayer { name: String, colour: Option<[u8; 3]> },
    /// A layer's new name or colour. Fields left `None` carry over.
    ReviseLayer { id: LayerId, name: Option<String>, colour: Option<[u8; 3]> },
    /// Removes a layer, which must hold nothing once this run's changes are counted.
    DeleteLayer { id: LayerId },
}

impl Change {
    /// The next revision of `object` with a new body.
    pub fn revise_body(object: &Object, body: Body) -> Change {
        Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(body) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Revised,
    Deleted,
}

/// One revision a run added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedChange {
    pub id: ObjectId,
    pub revision: u32,
    pub change: ChangeKind,
    pub name: String,
}

/// What applying a run's changes did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    pub changes: Vec<AppliedChange>,
    pub layers_created: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    MissingObject(ObjectId),
    Deleted(ObjectId),
    /// The command read revision `base`, but the object has moved on to `current`.
    Stale { id: ObjectId, base: u32, current: u32 },
    DuplicateLayer(String),
    MissingLayer(LayerId),
    /// A layer can go only when nothing is left on it.
    LayerInUse(String),
    EmptyName,
    /// A revision may not turn a string into a surface, or the like.
    KindChanged(ObjectId),
    /// One run may change each object only once.
    ChangedTwice(ObjectId),
    /// A saved project lists the same object or layer twice, or an object on a missing layer.
    Corrupt(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::MissingObject(id) => write!(f, "no object {id}"),
            ProjectError::Deleted(id) => write!(f, "object {id} has been deleted"),
            ProjectError::Stale { id, base, current } => {
                write!(f, "object {id} is at revision {current}, but the command read revision {base}")
            }
            ProjectError::DuplicateLayer(name) => write!(f, "layer '{name}' already exists"),
            ProjectError::MissingLayer(id) => write!(f, "no layer {id}"),
            ProjectError::LayerInUse(name) => write!(f, "layer '{name}' still has objects on it"),
            ProjectError::EmptyName => write!(f, "names cannot be empty"),
            ProjectError::KindChanged(id) => write!(f, "a revision cannot change the kind of object {id}"),
            ProjectError::ChangedTwice(id) => write!(f, "object {id} is changed twice in one run"),
            ProjectError::Corrupt(message) => write!(f, "saved project is inconsistent: {message}"),
        }
    }
}

impl std::error::Error for ProjectError {}

/// One step of an object's history: a revision, or a tombstone where it was deleted.
#[derive(Debug, Clone)]
struct Entry {
    revision: u32,
    object: Option<Arc<Object>>,
}

/// A layer as it stood and as a run left it (`None` if removed), with its place in the list. Whether
/// it's shown is display state, so undo and redo leave that as it is.
#[derive(Debug, Clone)]
struct LayerEdit {
    place: usize,
    before: Layer,
    after: Option<Layer>,
}

/// Everything one run added, kept so undo can take it off and redo can put it back.
#[derive(Debug, Clone)]
struct Transaction {
    run: RunId,
    layers: Vec<Layer>,
    layer_edits: Vec<LayerEdit>,
    entries: Vec<(ObjectId, Entry)>,
}

const LAYER_COLOURS: [[u8; 3]; 6] =
    [[168, 145, 107], [91, 107, 125], [140, 154, 166], [201, 162, 39], [111, 143, 95], [176, 96, 80]];

#[derive(Debug, Default)]
pub struct Project {
    layers: Vec<Layer>,
    histories: HashMap<ObjectId, Vec<Entry>>,
    /// Object ids in creation order, for listing.
    order: Vec<ObjectId>,
    done: Vec<Transaction>,
    undone: Vec<Transaction>,
    /// Objects hidden one by one. Display state, saved with the project; shared so a snap can read it
    /// without holding the project.
    hidden: Arc<HashSet<ObjectId>>,
    /// See [`Project::generation`]. Not saved.
    generation: u64,
}

/// A number no project has held before.
fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Project {
    pub fn new() -> Self {
        Project { generation: next_generation(), ..Project::default() }
    }

    /// A number that changes with every change to the project, a layer shown or hidden included,
    /// and that no other project has held: what is worked out from a project can be kept against it.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// A project rebuilt from saved layers and objects (each at its saved revision, in listing
    /// order). The undo history starts empty: it does not survive closing a project.
    pub fn from_saved(layers: Vec<Layer>, objects: Vec<Object>) -> Result<Project, ProjectError> {
        let mut project = Project::new();
        for layer in layers {
            non_empty(&layer.name)?;
            if project.layers.iter().any(|l| l.id == layer.id || l.name == layer.name) {
                return Err(ProjectError::Corrupt(format!("layer '{}' listed twice", layer.name)));
            }
            project.layers.push(layer);
        }
        for object in objects {
            non_empty(&object.name)?;
            if project.layer(object.layer).is_none() {
                return Err(ProjectError::Corrupt(format!("object '{}' is on a missing layer", object.name)));
            }
            if object.revision == 0 || project.histories.contains_key(&object.id) {
                return Err(ProjectError::Corrupt(format!("object {} listed twice or at revision 0", object.id)));
            }
            project.order.push(object.id);
            project.histories.insert(object.id, vec![Entry { revision: object.revision, object: Some(Arc::new(object)) }]);
        }
        Ok(project)
    }

    /// The same project with these objects hidden, as saved.
    pub fn with_hidden(mut self, ids: impl IntoIterator<Item = ObjectId>) -> Project {
        self.hidden = Arc::new(ids.into_iter().collect());
        self
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_named(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// Display state only: not a change, not undone.
    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) -> bool {
        match self.layers.iter_mut().find(|l| l.id == id) {
            Some(layer) => {
                layer.visible = visible;
                self.generation = next_generation();
                true
            }
            None => false,
        }
    }

    /// Shows or hides live objects one by one. Display state only: not a change, not undone. Hidden means
    /// not drawn, snapped to or cut by a section; a command that names the object still takes it. Showing
    /// an object on a layer that's off brings the layer on with only that object showing, as nothing else
    /// on it was showing before. Returns how many changed.
    pub fn set_objects_visible(&mut self, ids: &[ObjectId], visible: bool) -> usize {
        let live: Vec<(ObjectId, LayerId)> = ids.iter().filter_map(|&id| self.object(id).map(|o| (id, o.layer))).collect();
        let off: HashSet<LayerId> = live.iter().map(|&(_, layer)| layer).filter(|&layer| visible && self.layer(layer).is_some_and(|l| !l.visible)).collect();
        let changing: HashSet<ObjectId> = live.iter().filter(|&&(id, layer)| off.contains(&layer) || self.is_hidden(id) == visible).map(|&(id, _)| id).collect();
        if changing.is_empty() {
            return 0;
        }
        let named: HashSet<ObjectId> = live.iter().map(|&(id, _)| id).collect();
        let others: Vec<ObjectId> = self.objects().filter(|o| off.contains(&o.layer) && !named.contains(&o.id)).map(|o| o.id).collect();
        let hidden = Arc::make_mut(&mut self.hidden);
        hidden.extend(others);
        for id in &changing {
            if visible {
                hidden.remove(id);
            } else {
                hidden.insert(*id);
            }
        }
        for layer in self.layers.iter_mut().filter(|l| off.contains(&l.id)) {
            layer.visible = true;
        }
        self.generation = next_generation();
        changing.len()
    }

    pub fn is_hidden(&self, id: ObjectId) -> bool {
        self.hidden.contains(&id)
    }

    /// Every object hidden one by one. An object deleted while hidden stays here, so undoing the delete
    /// brings it back hidden.
    pub fn hidden(&self) -> &Arc<HashSet<ObjectId>> {
        &self.hidden
    }

    /// The current revision of a live object.
    pub fn object(&self, id: ObjectId) -> Option<&Arc<Object>> {
        self.histories.get(&id)?.last()?.object.as_ref()
    }

    /// Any revision still in the history.
    pub fn revision(&self, id: ObjectId, revision: u32) -> Option<&Arc<Object>> {
        self.histories.get(&id)?.iter().find(|e| e.revision == revision)?.object.as_ref()
    }

    /// Live objects at their current revision, in creation order.
    pub fn objects(&self) -> impl Iterator<Item = &Arc<Object>> {
        self.order.iter().filter_map(|&id| self.object(id))
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Applies one run's changes together, or none of them if any is refused. A new run clears
    /// the redo stack; a run that changes nothing (a report, or a file written) adds no undo step.
    pub fn apply(&mut self, run: RunId, changes: Vec<Change>) -> Result<Applied, ProjectError> {
        let mut staged = Staged { project: self, run, layers: Vec::new(), layer_edits: Vec::new(), entries: Vec::new(), touched: HashSet::new(), applied: Applied::default() };
        for change in changes {
            staged.stage(change)?;
        }
        let Staged { layers, layer_edits, entries, mut applied, .. } = staged;
        if layers.is_empty() && layer_edits.is_empty() && entries.is_empty() {
            return Ok(applied);
        }
        applied.layers_created = layers.clone();
        let transaction = Transaction { run, layers, layer_edits, entries };
        self.push(&transaction);
        self.done.push(transaction);
        self.undone.clear();
        self.generation = next_generation();
        Ok(applied)
    }

    /// Takes the last run off; returns its id.
    pub fn undo(&mut self) -> Option<RunId> {
        let transaction = self.done.pop()?;
        let mut gone = HashSet::new();
        for (id, entry) in transaction.entries.iter().rev() {
            let history = self.histories.get_mut(id).expect("undone object has a history");
            let popped = history.pop().expect("undone revision is in the history");
            debug_assert_eq!(popped.revision, entry.revision);
            if history.is_empty() {
                self.histories.remove(id);
                gone.insert(*id);
            }
        }
        self.order.retain(|o| !gone.contains(o));
        for edit in transaction.layer_edits.iter().rev() {
            match self.layers.iter_mut().find(|l| l.id == edit.before.id) {
                Some(layer) => *layer = Layer { visible: layer.visible, ..edit.before.clone() },
                None => self.layers.insert(edit.place.min(self.layers.len()), edit.before.clone()),
            }
        }
        let added: HashSet<LayerId> = transaction.layers.iter().map(|l| l.id).collect();
        self.layers.retain(|l| !added.contains(&l.id));
        let run = transaction.run;
        self.undone.push(transaction);
        self.generation = next_generation();
        Some(run)
    }

    /// Puts the last undone run back; returns its id.
    pub fn redo(&mut self) -> Option<RunId> {
        let transaction = self.undone.pop()?;
        self.push(&transaction);
        let run = transaction.run;
        self.done.push(transaction);
        self.generation = next_generation();
        Some(run)
    }

    fn push(&mut self, transaction: &Transaction) {
        self.layers.extend(transaction.layers.iter().cloned());
        for (id, entry) in &transaction.entries {
            let history = self.histories.entry(*id).or_default();
            if history.is_empty() {
                self.order.push(*id);
            }
            history.push(entry.clone());
        }
        for edit in &transaction.layer_edits {
            match (&edit.after, self.layers.iter_mut().find(|l| l.id == edit.before.id)) {
                (Some(after), Some(layer)) => *layer = Layer { visible: layer.visible, ..after.clone() },
                _ => self.layers.retain(|l| l.id != edit.before.id),
            }
        }
    }

    fn current(&self, id: ObjectId) -> Result<(u32, &Arc<Object>), ProjectError> {
        let last = self.histories.get(&id).and_then(|h| h.last()).ok_or(ProjectError::MissingObject(id))?;
        let object = last.object.as_ref().ok_or(ProjectError::Deleted(id))?;
        Ok((last.revision, object))
    }
}

/// A run's changes checked against the project before anything is applied.
struct Staged<'a> {
    project: &'a Project,
    run: RunId,
    layers: Vec<Layer>,
    layer_edits: Vec<LayerEdit>,
    entries: Vec<(ObjectId, Entry)>,
    touched: HashSet<ObjectId>,
    applied: Applied,
}

impl Staged<'_> {
    fn stage(&mut self, change: Change) -> Result<(), ProjectError> {
        match change {
            Change::CreateLayer { name, colour } => {
                if self.layer_id(&name).is_some() {
                    return Err(ProjectError::DuplicateLayer(name));
                }
                self.add_layer(name, colour)?;
            }
            Change::ReviseLayer { id, name, colour } => {
                let (place, before) = self.existing_layer(id)?;
                if let Some(name) = &name {
                    non_empty(name)?;
                    if *name != before.name && self.layer_id(name).is_some() {
                        return Err(ProjectError::DuplicateLayer(name.clone()));
                    }
                }
                let after = Layer { name: name.unwrap_or_else(|| before.name.clone()), colour: colour.unwrap_or(before.colour), ..before.clone() };
                self.layer_edits.push(LayerEdit { place, before, after: Some(after) });
            }
            Change::DeleteLayer { id } => {
                let (place, before) = self.existing_layer(id)?;
                let staying_off: HashSet<ObjectId> = self.entries.iter().filter(|(_, e)| e.object.as_ref().is_none_or(|o| o.layer != id)).map(|(id, _)| *id).collect();
                let arriving = self.entries.iter().any(|(_, e)| e.object.as_ref().is_some_and(|o| o.layer == id));
                if arriving || self.project.objects().any(|o| o.layer == id && !staying_off.contains(&o.id)) {
                    return Err(ProjectError::LayerInUse(before.name));
                }
                self.layer_edits.push(LayerEdit { place, before, after: None });
            }
            Change::Create { name, layer, attributes, body } => {
                non_empty(&name)?;
                let layer = self.layer_or_create(layer)?;
                let id = ObjectId::new();
                self.touched.insert(id);
                let object = Object { id, revision: 1, name: name.clone(), layer, provenance: self.run, attributes, body };
                self.entries.push((id, Entry { revision: 1, object: Some(Arc::new(object)) }));
                self.applied.changes.push(AppliedChange { id, revision: 1, change: ChangeKind::Created, name });
            }
            Change::Revise { id, base, name, layer, attributes, body } => {
                let old = self.base(id, base)?;
                if body.as_ref().is_some_and(|b| b.kind() != old.kind()) {
                    return Err(ProjectError::KindChanged(id));
                }
                let name = name.unwrap_or_else(|| old.name.clone());
                non_empty(&name)?;
                let layer = match layer {
                    Some(layer) => self.layer_or_create(layer)?,
                    None => old.layer,
                };
                let object = Object {
                    id,
                    revision: base + 1,
                    name: name.clone(),
                    layer,
                    provenance: self.run,
                    attributes: attributes.unwrap_or_else(|| old.attributes.clone()),
                    body: body.unwrap_or_else(|| old.body.clone()),
                };
                self.entries.push((id, Entry { revision: base + 1, object: Some(Arc::new(object)) }));
                self.applied.changes.push(AppliedChange { id, revision: base + 1, change: ChangeKind::Revised, name });
            }
            Change::Delete { id, base } => {
                let name = self.base(id, base)?.name.clone();
                self.entries.push((id, Entry { revision: base + 1, object: None }));
                self.applied.changes.push(AppliedChange { id, revision: base + 1, change: ChangeKind::Deleted, name });
            }
        }
        Ok(())
    }

    /// The object at revision `base`, checking it is still current and untouched by this run.
    fn base(&mut self, id: ObjectId, base: u32) -> Result<Arc<Object>, ProjectError> {
        if !self.touched.insert(id) {
            return Err(ProjectError::ChangedTwice(id));
        }
        let (current, object) = self.project.current(id)?;
        if current != base {
            return Err(ProjectError::Stale { id, base, current });
        }
        Ok(object.clone())
    }

    /// A layer already in the project, with its place in the list.
    fn existing_layer(&self, id: LayerId) -> Result<(usize, Layer), ProjectError> {
        let place = self.project.layers.iter().position(|l| l.id == id).ok_or(ProjectError::MissingLayer(id))?;
        Ok((place, self.project.layers[place].clone()))
    }

    fn layer_id(&self, name: &str) -> Option<LayerId> {
        self.project.layer_named(name).or_else(|| self.layers.iter().find(|l| l.name == name)).map(|l| l.id)
    }

    fn layer_or_create(&mut self, name: String) -> Result<LayerId, ProjectError> {
        match self.layer_id(&name) {
            Some(id) => Ok(id),
            None => self.add_layer(name, None),
        }
    }

    fn add_layer(&mut self, name: String, colour: Option<[u8; 3]>) -> Result<LayerId, ProjectError> {
        non_empty(&name)?;
        let n = self.project.layers.len() + self.layers.len();
        let layer = Layer { id: LayerId::new(), name, colour: colour.unwrap_or(LAYER_COLOURS[n % LAYER_COLOURS.len()]), visible: true };
        let id = layer.id;
        self.layers.push(layer);
        Ok(id)
    }
}

fn non_empty(name: &str) -> Result<(), ProjectError> {
    if name.trim().is_empty() { Err(ProjectError::EmptyName) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Polyline;

    fn line(points: &[[f64; 3]]) -> Body {
        Body::String(Polyline { vertices: points.to_vec(), closed: false })
    }

    fn create(name: &str, layer: &str, body: Body) -> Change {
        Change::Create { name: name.into(), layer: layer.into(), attributes: Attributes::new(), body }
    }

    fn vertices(o: &Object) -> Vec<[f64; 3]> {
        match &o.body {
            Body::String(p) => p.vertices.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn create_revise_delete_then_undo_and_redo() {
        let mut p = Project::new();
        let a = p.apply(RunId::new(), vec![create("toe", "design", line(&[[0.0; 3], [1.0, 0.0, 0.0]]))]).unwrap();
        let id = a.changes[0].id;
        assert_eq!(a.layers_created.len(), 1, "layer 'design' made on demand");
        let v1 = p.object(id).unwrap().clone();

        let moved = line(&[[0.0; 3], [2.0, 0.0, 0.0]]);
        p.apply(RunId::new(), vec![Change::revise_body(&v1, moved)]).unwrap();
        assert_eq!(p.object(id).unwrap().revision, 2);
        assert_eq!(vertices(&v1), vec![[0.0; 3], [1.0, 0.0, 0.0]], "revision 1 is untouched");
        assert!(Arc::ptr_eq(p.revision(id, 1).unwrap(), &v1));

        p.apply(RunId::new(), vec![Change::Delete { id, base: 2 }]).unwrap();
        assert!(p.object(id).is_none());
        assert_eq!(p.objects().count(), 0);

        p.undo();
        assert_eq!(p.object(id).unwrap().revision, 2, "delete undone");
        p.undo();
        assert_eq!(p.object(id).unwrap().revision, 1, "edit undone");
        p.undo();
        assert!(p.object(id).is_none() && p.layers().is_empty(), "creation and its layer undone");
        assert!(!p.can_undo());

        p.redo();
        p.redo();
        assert_eq!(vertices(p.object(id).unwrap()), vec![[0.0; 3], [2.0, 0.0, 0.0]]);
        assert_eq!(p.layers().len(), 1);
    }

    #[test]
    fn a_refused_run_changes_nothing() {
        let mut p = Project::new();
        let a = p.apply(RunId::new(), vec![create("toe", "design", line(&[[0.0; 3], [1.0, 0.0, 0.0]]))]).unwrap();
        let id = a.changes[0].id;
        // Second change is stale (base 7): the first must not land either, nor its new layer.
        let err = p
            .apply(
                RunId::new(),
                vec![create("crest", "other", line(&[[0.0; 3], [0.0, 1.0, 0.0]])), Change::Delete { id, base: 7 }],
            )
            .unwrap_err();
        assert_eq!(err, ProjectError::Stale { id, base: 7, current: 1 });
        assert_eq!(p.objects().count(), 1);
        assert_eq!(p.layers().len(), 1);
        assert!(p.apply(RunId::new(), vec![Change::Delete { id, base: 1 }, Change::Delete { id, base: 1 }]).is_err());
        assert!(p.object(id).is_some(), "changed-twice run refused whole");
        let solid_body = Body::Grid(Arc::new(crate::Grid::from_fn([0.0, 0.0], [1.0, 1.0], 2, 2, |_, _| 0.0)));
        assert_eq!(
            p.apply(RunId::new(), vec![Change::revise_body(p.object(id).unwrap(), solid_body)]).unwrap_err(),
            ProjectError::KindChanged(id)
        );
        assert_eq!(p.apply(RunId::new(), vec![Change::CreateLayer { name: "design".into(), colour: None }]).unwrap_err(), ProjectError::DuplicateLayer("design".into()));
    }

    /// Live objects as (id, revision, name, vertices), in creation order.
    fn snapshot(p: &Project) -> Vec<(ObjectId, u32, String, Vec<[f64; 3]>)> {
        p.objects().map(|o| (o.id, o.revision, o.name.clone(), vertices(o))).collect()
    }

    #[test]
    fn undo_and_redo_retrace_one_hundred_random_runs_exactly() {
        let mut p = Project::new();
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let mut snapshots = vec![snapshot(&p)];
        for step in 0..100 {
            let live: Vec<Arc<Object>> = p.objects().cloned().collect();
            let change = match (live.is_empty(), next(3)) {
                (true, _) | (_, 0) => create(&format!("s{step}"), &format!("L{}", next(3)), line(&[[step as f64, 0.0, 0.0], [0.0, step as f64, 0.0]])),
                (_, 1) => {
                    let o = &live[next(live.len())];
                    let mut v = vertices(o);
                    v.push([next(100) as f64, next(100) as f64, 1.5]);
                    Change::revise_body(o, line(&v))
                }
                _ => {
                    let o = &live[next(live.len())];
                    Change::Delete { id: o.id, base: o.revision }
                }
            };
            p.apply(RunId::new(), vec![change]).unwrap();
            snapshots.push(snapshot(&p));
        }
        for expected in snapshots.iter().rev().skip(1) {
            p.undo().unwrap();
            assert_eq!(&snapshot(&p), expected);
        }
        assert!(p.undo().is_none() && p.layers().is_empty());
        for expected in snapshots.iter().skip(1) {
            p.redo().unwrap();
            assert_eq!(&snapshot(&p), expected);
        }
    }

    #[test]
    fn a_new_run_clears_redo() {
        let mut p = Project::new();
        p.apply(RunId::new(), vec![create("a", "L", line(&[[0.0; 3], [1.0; 3]]))]).unwrap();
        p.undo();
        assert!(p.can_redo());
        p.apply(RunId::new(), vec![create("b", "L", line(&[[0.0; 3], [1.0; 3]]))]).unwrap();
        assert!(!p.can_redo());
    }

    #[test]
    fn a_run_that_changes_nothing_adds_no_undo_step() {
        let mut p = Project::new();
        p.apply(RunId::new(), vec![create("a", "L", line(&[[0.0; 3], [1.0; 3]]))]).unwrap();
        p.undo();
        let applied = p.apply(RunId::new(), Vec::new()).unwrap();
        assert!(applied.changes.is_empty());
        assert!(!p.can_undo(), "nothing to undo");
        assert!(p.can_redo(), "and the redo stack is kept");
    }

    #[test]
    fn the_generation_moves_with_every_change_and_with_nothing_else() {
        let mut p = Project::new();
        assert_ne!(p.generation(), Project::new().generation(), "no two projects share one");
        let mut seen = vec![p.generation()];
        let mut moved = |p: &Project, why: &str| {
            assert!(!seen.contains(&p.generation()), "{why}");
            seen.push(p.generation());
        };
        let a = p.apply(RunId::new(), vec![create("a", "L", line(&[[0.0; 3], [1.0; 3]]))]).unwrap();
        moved(&p, "a run");
        let layer = p.object(a.changes[0].id).unwrap().layer;
        p.set_layer_visible(layer, false);
        moved(&p, "a layer hidden");
        p.undo();
        moved(&p, "an undo");
        p.redo();
        moved(&p, "a redo");

        let before = p.generation();
        p.apply(RunId::new(), Vec::new()).unwrap();
        assert_eq!(p.generation(), before, "a run that changes nothing");
        p.objects().count();
        assert_eq!(p.generation(), before, "reading");
    }
}
