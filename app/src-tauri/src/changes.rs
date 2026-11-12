//! What the app is sent after a change: only the objects added, changed or gone since it was last sent, found by
//! comparing the project against what the shell last sent (the App at scale plan, workstream 1). One rule covers
//! runs, undo, redo, hide, show and open, so no command reports anything new for it. A row carries the attributes the
//! app reads by name; Properties fetches the rest for the object picked.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use zd_core::{Attributes, Body, Display, Layer, LayerId, Object, ObjectId, ObjectKind, Project};

use crate::state::{AppInner, TileId, geometry_key, is_drawn_surface, solid_tile_keys};

/// The attributes the app reads by name, which go with each row. Everything else stays in the shell until asked for.
pub const ROW_ATTRIBUTES: &[&str] = &["build", "build_id", "build_on", "build_record", "build_step", "face_design", "not_a_solid", "road", "road_face", "step", "strip"];

/// Attributes named this way go with a row too: the roads a crest records (decision 0041).
pub const ROW_ATTRIBUTE_FAMILIES: &[&str] = &["road_top "];

/// Whether an attribute goes with a row.
pub fn goes_with_row(name: &str) -> bool {
    ROW_ATTRIBUTES.contains(&name) || ROW_ATTRIBUTE_FAMILIES.iter().any(|family| name.starts_with(family))
}

/// The attributes of `all` that go with a row.
pub fn row_attributes(all: &Attributes) -> Attributes {
    all.iter().filter(|(name, _)| goes_with_row(name)).map(|(name, value)| (name, value.clone())).collect()
}

/// A number no reply has carried before, in this run of the app.
fn next_epoch() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// What the app was last sent: each object's revision as sent, and whether it was hidden.
#[derive(Default)]
pub struct Sent {
    epoch: Option<u64>,
    /// The revision sent, held so its address can't be taken by another while it's compared against.
    rows: HashMap<ObjectId, (Arc<Object>, bool)>,
    order: Vec<ObjectId>,
    origin: Option<[f64; 3]>,
    /// The objects hidden one by one, as sent: a hide or show is told from it without walking every object.
    hidden: Arc<HashSet<ObjectId>>,
}

/// The project as the app needs it after a change. With no `base` it's the whole project, in place of what the app
/// holds; otherwise it's the change from what the app was sent as `base`, and the app takes it only if that's what it
/// holds, asking for the whole project if not.
#[derive(Serialize)]
pub struct ProjectChanges {
    /// What this follows on from; none when it's the whole project.
    base: Option<u64>,
    /// What the app holds once it has taken this.
    epoch: u64,
    path: Option<String>,
    can_undo: bool,
    can_redo: bool,
    origin: Option<[f64; 3]>,
    layers: Vec<Layer>,
    /// The objects added or changed (every object, when it's the whole project).
    objects: Vec<ObjectSummary>,
    /// Objects gone: deleted, undone or redone away.
    removed: Vec<ObjectId>,
    /// Every object's id in order, when the order is other than what the app held, less what's gone, with the new at
    /// the end: an object brought back by an undo goes back to its place.
    order: Option<Vec<ObjectId>>,
    /// The tiles of the layers holding solids, each drawn as one mesh (decision 0060, workstream 4).
    solid_layers: Vec<SolidLayer>,
    /// How objects and layers look: colour by RL or the layer's, opacity, contour bands.
    display: Display,
    /// Who else had the project file open when it was opened ("Tim on PIT-PC since ..."): a save to it is refused.
    locked_by: Option<String>,
    /// The run log as it stands: which session's, and how long, so the app fetches only the runs it hasn't got.
    log: LogMark,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct LogMark {
    pub session: u64,
    pub len: usize,
}

/// A tile of a layer's solids as the viewport draws it.
#[derive(Serialize)]
pub struct SolidLayer {
    layer: LayerId,
    /// The tile, column and row of [`crate::state::TILE_M`] squares from the grid's zero.
    tile: [i64; 2],
    /// Changes only when the geometry of a solid in the tile does, or one comes or goes.
    key: String,
}

#[derive(Serialize)]
pub struct ObjectSummary {
    id: ObjectId,
    revision: u32,
    name: String,
    layer: LayerId,
    kind: ObjectKind,
    /// The attributes the app reads by name ([`goes_with_row`]); `object_details` has them all.
    attributes: Attributes,
    /// Drawn as a surface (surfaces, solids, elevation grids).
    drawn: bool,
    /// Hidden on its own: see [`zd_core::Project::set_objects_visible`].
    hidden: bool,
    /// Changes only when the geometry does; fetch a new render buffer when it changes.
    geometry_key: String,
    /// String vertices, offset from the scene origin. Strings only: left out of other rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    vertices_local: Option<Vec<[f64; 3]>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    closed: Option<bool>,
}

fn row(project: &Project, object: &Object, origin: [f64; 3]) -> ObjectSummary {
    let local = |p: [f64; 3]| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
    let (vertices_local, closed) = match &object.body {
        Body::String(line) => (Some(line.vertices.iter().map(|&p| local(p)).collect()), Some(line.closed)),
        _ => (None, None),
    };
    ObjectSummary {
        id: object.id,
        revision: object.revision,
        name: object.name.clone(),
        layer: object.layer,
        kind: object.kind(),
        attributes: row_attributes(&object.attributes),
        drawn: is_drawn_surface(object),
        hidden: project.is_hidden(object.id),
        geometry_key: geometry_key(&object.body).to_string(),
        vertices_local,
        closed,
    }
}

/// What changed since the app was last sent the project, or the whole project with `whole` (or when the scene origin
/// has moved, as every string's vertices are sent from it). Records what's sent, as the app now holds it.
pub fn changes(app: &mut AppInner, whole: bool) -> ProjectChanges {
    let origin = app.origin();
    let o = origin.unwrap_or([0.0; 3]);
    let project = app.session.project();
    let sent = &mut app.sent;
    let whole = whole || sent.epoch.is_none() || sent.origin != origin;
    if whole {
        sent.rows.clear();
        sent.order.clear();
    }

    let mut objects = Vec::new();
    let mut added = Vec::new();
    let mut kept = 0;
    for object in project.objects() {
        let hidden = project.is_hidden(object.id);
        match sent.rows.get(&object.id) {
            Some((was, was_hidden)) => {
                kept += 1;
                if Arc::ptr_eq(was, object) && *was_hidden == hidden {
                    continue;
                }
            }
            None => added.push(object.id),
        }
        objects.push(row(project, object, o));
        sent.rows.insert(object.id, (object.clone(), hidden));
    }
    // Objects the app holds that the project doesn't: looked for only when some are gone, as is the order.
    let mut removed = Vec::new();
    let mut order = None;
    if kept < sent.order.len() || !added.is_empty() {
        let live: Vec<ObjectId> = project.objects().map(|o| o.id).collect();
        if kept < sent.order.len() {
            let live_set: HashSet<ObjectId> = live.iter().copied().collect();
            removed = sent.order.iter().copied().filter(|id| !live_set.contains(id)).collect();
            for id in &removed {
                sent.rows.remove(id);
            }
        }
        // What the app will make of it on its own: what it held, less what's gone, with the new at the end.
        let gone: HashSet<ObjectId> = removed.iter().copied().collect();
        let expected = sent.order.iter().copied().filter(|id| !gone.contains(id)).chain(added.iter().copied());
        if !whole && !expected.eq(live.iter().copied()) {
            order = Some(live.clone());
        }
        sent.order = live;
    }

    let base = if whole { None } else { sent.epoch };
    sent.origin = origin;
    // Drawn by its solids' geometry alone: hiding one changes its row, and the viewport's table, not the layer's mesh.
    app.solid_keys = solid_tile_keys(project, &mut app.tiles);
    reply(app, base, objects, removed, order)
}

/// What changed when the only change can be which objects are hidden, as after a hide or show (or a layer turned on or
/// off): the objects whose hidden flag is other than was sent, found from the two sets of hidden objects without walking
/// every object. One of 43,000 hidden is then one row found in microseconds.
pub fn changes_after_hiding(app: &mut AppInner) -> ProjectChanges {
    let origin = app.origin();
    if app.sent.epoch.is_none() || app.sent.origin != origin {
        return changes(app, false);
    }
    let o = origin.unwrap_or([0.0; 3]);
    let project = app.session.project();
    let sent = &mut app.sent;
    let now = project.hidden();
    let mut objects = Vec::new();
    for &id in sent.hidden.symmetric_difference(now) {
        let (Some(object), Some(entry)) = (project.object(id), sent.rows.get_mut(&id)) else { continue };
        let hidden = now.contains(&id);
        if entry.1 != hidden {
            entry.1 = hidden;
            objects.push(row(project, object, o));
        }
    }
    let base = sent.epoch;
    reply(app, base, objects, Vec::new(), None)
}

/// The reply around the objects that changed, recording what's sent.
fn reply(app: &mut AppInner, base: Option<u64>, objects: Vec<ObjectSummary>, removed: Vec<ObjectId>, order: Option<Vec<ObjectId>>) -> ProjectChanges {
    let project = app.session.project();
    let epoch = next_epoch();
    app.sent.epoch = Some(epoch);
    app.sent.hidden = project.hidden().clone();
    ProjectChanges {
        base,
        epoch,
        path: app.path.as_ref().map(|p| p.display().to_string()),
        can_undo: project.can_undo(),
        can_redo: project.can_redo(),
        origin: app.sent.origin,
        layers: project.layers().to_vec(),
        objects,
        removed,
        order,
        solid_layers: app.solid_keys.iter().map(|(&TileId { layer, col, row }, key)| SolidLayer { layer, tile: [col, row], key: key.clone() }).collect(),
        display: project.display().clone(),
        locked_by: app.locked_by.clone(),
        log: LogMark { session: app.log_session, len: app.session.log().len() },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zd_commands::Session;

    use super::*;

    fn string(name: &str, x: f64) -> serde_json::Value {
        json!({ "name": name, "layer": "toes", "points": [[650_000.0 + x, 7_450_000.0, 100.0], [650_100.0 + x, 7_450_000.0, 100.0]], "closed": false })
    }

    fn ids(rows: &[ObjectSummary]) -> Vec<ObjectId> {
        rows.iter().map(|r| r.id).collect()
    }

    /// The app's side, as `projectStore.ts` does it: what it holds after taking each reply in turn.
    #[derive(Default)]
    struct Held {
        epoch: Option<u64>,
        rows: HashMap<ObjectId, (u32, bool, String)>,
        order: Vec<ObjectId>,
    }

    impl Held {
        fn take(&mut self, c: &ProjectChanges) {
            match c.base {
                None => {
                    self.rows.clear();
                    self.order.clear();
                }
                Some(base) => assert_eq!(Some(base), self.epoch, "a change follows on from what the app holds"),
            }
            for id in &c.removed {
                self.rows.remove(id);
            }
            self.order.retain(|id| !c.removed.contains(id));
            for r in &c.objects {
                if self.rows.insert(r.id, (r.revision, r.hidden, r.name.clone())).is_none() {
                    self.order.push(r.id);
                }
            }
            if let Some(order) = &c.order {
                self.order = order.clone();
            }
            self.epoch = Some(c.epoch);
        }

        /// The same as being sent the whole project.
        fn matches(&self, app: &mut AppInner) {
            let whole = changes_of_whole(app);
            assert_eq!(self.order, ids(&whole.objects), "the same objects in the same order");
            for r in &whole.objects {
                assert_eq!(self.rows[&r.id], (r.revision, r.hidden, r.name.clone()));
            }
        }
    }

    /// The whole project, without disturbing what the app was sent.
    fn changes_of_whole(app: &mut AppInner) -> ProjectChanges {
        let sent = std::mem::take(&mut app.sent);
        let whole = changes(app, true);
        app.sent = sent;
        whole
    }

    #[test]
    fn only_what_changed_is_sent_and_taken_in_turn_it_gives_the_whole_project() {
        let mut app = AppInner::new(Session::with_builtins(), None);
        let mut held = Held::default();
        let first = changes(&mut app, false);
        assert_eq!(first.base, None, "the first reply is the whole project");
        held.take(&first);

        let a = app.session.run("string.create", string("a", 0.0)).unwrap().outputs[0].id;
        let b = app.session.run("string.create", string("b", 200.0)).unwrap().outputs[0].id;
        let made = changes(&mut app, false);
        // The origin was set by the first string, so this reply is whole; from here on, only changes.
        held.take(&made);
        held.matches(&mut app);

        let c = app.session.run("string.create", string("c", 400.0)).unwrap().outputs[0].id;
        let one = changes(&mut app, false);
        assert_eq!(one.base, Some(made.epoch));
        assert_eq!(ids(&one.objects), vec![c], "a new object, alone");
        assert!(one.removed.is_empty() && one.order.is_none());
        held.take(&one);
        held.matches(&mut app);

        app.session.run("object.rename", json!({ "object": a, "name": "a2" })).unwrap();
        let renamed = changes(&mut app, false);
        assert_eq!(ids(&renamed.objects), vec![a], "a revision, alone");
        held.take(&renamed);
        held.matches(&mut app);

        app.session.set_objects_visible(&[b], false);
        let hidden = changes(&mut app, false);
        assert_eq!(ids(&hidden.objects), vec![b], "hiding sends the row, not the project");
        assert!(hidden.objects[0].hidden);
        held.take(&hidden);

        app.session.run("object.delete", json!({ "object": a })).unwrap();
        let deleted = changes(&mut app, false);
        assert!(deleted.objects.is_empty());
        assert_eq!(deleted.removed, vec![a]);
        assert!(deleted.order.is_none(), "what's left keeps its order");
        held.take(&deleted);
        held.matches(&mut app);

        // Undo brings the string back to its place, first, and the rename undone takes it back a revision.
        app.session.undo();
        let back = changes(&mut app, false);
        assert_eq!(ids(&back.objects), vec![a]);
        assert_eq!(back.order.as_deref(), Some(&[a, b, c][..]), "back where it was, not at the end");
        held.take(&back);
        held.matches(&mut app);
        app.session.undo();
        let unrenamed = changes(&mut app, false);
        assert_eq!(ids(&unrenamed.objects), vec![a]);
        assert_eq!(unrenamed.objects[0].revision, 1);
        held.take(&unrenamed);
        held.matches(&mut app);
        app.session.redo();
        app.session.redo();
        let redone = changes(&mut app, false);
        held.take(&redone);
        held.matches(&mut app);

        let nothing = changes(&mut app, false);
        assert!(nothing.objects.is_empty() && nothing.removed.is_empty() && nothing.order.is_none(), "nothing changed, nothing sent");
        held.take(&nothing);
        held.matches(&mut app);

        let whole = changes(&mut app, true);
        assert_eq!(whole.base, None);
        assert_eq!(ids(&whole.objects), vec![b, c]);
    }

    #[test]
    fn a_hide_or_show_sends_the_rows_whose_flag_changed_as_the_full_diff_would() {
        let mut app = AppInner::new(Session::with_builtins(), None);
        let made: Vec<ObjectId> = (0..6).map(|k| app.session.run("string.create", string(&format!("s{k}"), 200.0 * k as f64)).unwrap().outputs[0].id).collect();
        let mut held = Held::default();
        held.take(&changes(&mut app, false));
        app.session.set_objects_visible(&made[1..3], false);
        let hidden = changes_after_hiding(&mut app);
        assert_eq!(hidden.base, Some(held.epoch.unwrap()));
        let mut got = ids(&hidden.objects);
        got.sort();
        let mut want = made[1..3].to_vec();
        want.sort();
        assert_eq!(got, want, "the two hidden, and nothing else");
        held.take(&hidden);
        held.matches(&mut app);
        // A layer off, then one of its strings shown: it shows alone, the rest of the layer hidden one by one.
        let layer = app.session.project().object(made[0]).unwrap().layer;
        app.session.set_layer_visible(layer, false);
        held.take(&changes_after_hiding(&mut app));
        app.session.set_objects_visible(&made[4..5], true);
        let alone = changes_after_hiding(&mut app);
        assert_eq!(alone.objects.len(), 3, "the three of the rest not hidden before are now: {:?}", ids(&alone.objects));
        held.take(&alone);
        held.matches(&mut app);
        let nothing = changes(&mut app, false);
        assert!(nothing.objects.is_empty(), "the full diff finds nothing more");
    }

    #[test]
    fn a_row_carries_the_attributes_the_app_reads_by_name() {
        let all: Attributes = [
            ("strip", zd_core::AttrValue::Number(7.0)),
            ("road_top A pit", zd_core::AttrValue::Bool(true)),
            ("road_top A width", zd_core::AttrValue::Number(25.0)),
            ("CV_ash", zd_core::AttrValue::Number(12.5)),
            ("roadster", zd_core::AttrValue::Text("no".into())),
        ]
        .into_iter()
        .collect();
        let kept: Vec<&str> = row_attributes(&all).keys().collect();
        assert_eq!(kept, vec!["road_top A pit", "road_top A width", "strip"]);
    }

    /// Times what the app is sent at scale, on a made-up project named by `ZD_SCALE_PROJECT` (the store bench's
    /// `bench/out/store-scale.zdp`, made with `store_scale -- 43000 250`). Counts and timings only.
    #[test]
    #[ignore]
    fn at_scale() {
        use std::time::Instant;
        let Ok(path) = std::env::var("ZD_SCALE_PROJECT") else { return };
        let t = Instant::now();
        let mut app = AppInner::new(Session::open(&path).unwrap(), None);
        println!("open: {} objects in {:.0} ms", app.session.project().objects().count(), t.elapsed().as_secs_f64() * 1e3);
        let sent = |app: &mut AppInner, what: &str, whole: bool| {
            let t = Instant::now();
            let c = changes(app, whole);
            let built = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let json = serde_json::to_vec(&c).unwrap();
            let encoded = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let _: serde_json::Value = serde_json::from_slice(&json).unwrap();
            let parsed = t.elapsed().as_secs_f64() * 1e3;
            println!(
                "{what}: {} rows, {} gone, order {}, {:.1} kB; built {built:.1} ms, encoded {encoded:.1} ms, parsed {parsed:.1} ms",
                c.objects.len(),
                c.removed.len(),
                if c.order.is_some() { "sent" } else { "not sent" },
                json.len() as f64 / 1e3
            );
        };
        sent(&mut app, "open, whole", true);
        let ids: Vec<ObjectId> = app.session.project().objects().map(|o| o.id).collect();
        app.session.set_objects_visible(&ids[100..101], false);
        sent(&mut app, "hide one", false);
        let t = Instant::now();
        app.session.run("object.move", json!({ "objects": [ids[200]], "layer": "moved" })).unwrap();
        println!("move one: run {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
        sent(&mut app, "move one", false);
        app.session.undo();
        sent(&mut app, "undo", false);
        app.session.run("object.delete", json!({ "object": ids[300] })).unwrap();
        sent(&mut app, "delete one", false);
        app.session.undo();
        sent(&mut app, "undo the delete", false);
        app.session.set_objects_visible(&ids, false);
        sent(&mut app, "hide all", false);
        sent(&mut app, "nothing changed", false);
    }

    /// Writes the made-up project's blocks as a zip of solids for the release app's `--bench --solids`, so hiding and
    /// selecting are timed to the frame at scale: `ZD_SCALE_PROJECT` in, `ZD_SCALE_ZIP` out.
    #[test]
    #[ignore]
    fn export_the_scale_blocks() {
        let (Ok(path), Ok(zip)) = (std::env::var("ZD_SCALE_PROJECT"), std::env::var("ZD_SCALE_ZIP")) else { return };
        let mut s = Session::open(&path).unwrap();
        let objects: Vec<ObjectId> = s.project().objects().map(|o| o.id).collect();
        let t = std::time::Instant::now();
        s.run("solid.export_many", json!({ "objects": objects, "path": zip, "overwrite": true })).unwrap();
        println!("{} solids written in {:.0} ms", objects.len(), t.elapsed().as_secs_f64() * 1e3);
    }
}
