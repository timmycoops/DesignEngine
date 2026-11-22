use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Body, Change, Object, ObjectId, ObjectKind, Solid, Surface};
use zd_geom::{Transform, move_grid, transform_crossings, transform_mesh, transform_string};

use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Severity};

/// What deleting these objects takes out of the solids, said as a note measured in m³, so a workflow report can account
/// for solids deleted by hand at a pause (decision 0068). Nothing said when none is a solid.
fn solids_deleted(objects: &[&Object]) -> Option<Note> {
    let volumes: Vec<f64> = objects.iter().filter_map(|o| if let Body::Solid(s) = &o.body { Some(s.volume()) } else { None }).collect();
    if volumes.is_empty() {
        return None;
    }
    let total: f64 = volumes.iter().sum();
    let n = volumes.len();
    let text = format!("{n} solid{} deleted, {total:.3} m³", if n == 1 { "" } else { "s" });
    Some(Note::new(Severity::Info, Some(Code::ObjectSolidsDeleted), text).measured(total, "m3"))
}

/// `object.rename`: the next revision of an object, with a new name.
pub struct RenameObject;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameParams {
    pub object: ObjectId,
    pub name: String,
}

impl Command for RenameObject {
    type Params = RenameParams;
    const NAME: &'static str = "object.rename";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Rename an object",
            inputs: vec![Field::new("object", FieldType::Text, "Id of any object")],
            params: vec![Field::new("name", FieldType::Text, "New name")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.object)?;
        Ok(Outcome::new(vec![Change::Revise {
            id: object.id,
            base: object.revision,
            name: Some(p.name.clone()),
            layer: None,
            attributes: None,
            body: None,
        }]))
    }
}

/// `object.delete`: removes an object from the project; undo brings it back.
pub struct DeleteObject;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectParams {
    pub object: ObjectId,
}

impl Command for DeleteObject {
    type Params = ObjectParams;
    const NAME: &'static str = "object.delete";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Delete an object",
            inputs: vec![Field::new("object", FieldType::Text, "Id of any object")],
            params: vec![],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.object)?;
        let mut outcome = Outcome::new(vec![Change::Delete { id: object.id, base: object.revision }]);
        outcome.notes.extend(solids_deleted(&[object.as_ref()]));
        Ok(outcome)
    }
}

/// The objects of a run, each once, by id.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectsParams {
    pub objects: Vec<ObjectId>,
}

/// `object.delete_many`: removes objects from the project in one run; undo brings them all back.
pub struct DeleteObjects;

impl Command for DeleteObjects {
    type Params = ObjectsParams;
    const NAME: &'static str = "object.delete_many";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Delete objects",
            inputs: vec![Field::new("objects", FieldType::Objects, "Ids of any objects")],
            params: vec![],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let objects = p.objects.iter().map(|&id| inputs.object(id)).collect::<Result<Vec<_>, _>>()?;
        let mut outcome = Outcome::new(objects.iter().map(|o| Change::Delete { id: o.id, base: o.revision }).collect());
        outcome.notes.extend(solids_deleted(&objects.iter().map(|o| o.as_ref()).collect::<Vec<_>>()));
        Ok(outcome)
    }
}

/// `object.move`: the next revision of each object, on another layer, which is created if it's new.
pub struct MoveObjects;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveObjectsParams {
    pub objects: Vec<ObjectId>,
    pub layer: String,
}

impl Command for MoveObjects {
    type Params = MoveObjectsParams;
    const NAME: &'static str = "object.move";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Move objects to another layer",
            inputs: vec![Field::new("objects", FieldType::Objects, "Ids of any objects")],
            params: vec![Field::new("layer", FieldType::Layer, "The layer they go to")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let target = inputs.layer(&p.layer).ok().map(|l| l.id);
        let mut changes = Vec::with_capacity(p.objects.len());
        for &id in &p.objects {
            let object = inputs.object(id)?;
            if Some(object.layer) != target {
                changes.push(Change::Revise { id, base: object.revision, name: None, layer: Some(p.layer.clone()), attributes: None, body: None });
            }
        }
        Ok(Outcome::new(changes))
    }
}

/// How a whole object is moved (String tools plan, workstream 3): by an easting, northing and RL; about a point in plan
/// by an angle, anticlockwise; or across a line in plan.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransformParam {
    Move {
        /// Easting, northing and RL (m).
        by: [f64; 3],
    },
    Rotate {
        /// The point it turns about, in plan.
        about: [f64; 2],
        /// Degrees, anticlockwise seen from above.
        angle: f64,
    },
    Mirror {
        /// The line it's mirrored across, from one point to another in plan.
        from: [f64; 2],
        to: [f64; 2],
    },
}

impl TransformParam {
    fn geometry(&self) -> Transform {
        match *self {
            TransformParam::Move { by } => Transform::Move { by },
            TransformParam::Rotate { about, angle } => Transform::Rotate { about, degrees: angle },
            TransformParam::Mirror { from, to } => Transform::Mirror { from, to },
        }
    }

    /// What it does, in words, for the run's message: "100.5 m east, 200.25 m south and 3 m up".
    fn words(&self) -> String {
        match *self {
            TransformParam::Move { by } => move_words(by),
            TransformParam::Rotate { about, angle } => {
                let way = if angle >= 0.0 { "anticlockwise" } else { "clockwise" };
                format!("{}° {way} about {:.3} E, {:.3} N", metres(angle.abs()), about[0], about[1])
            }
            TransformParam::Mirror { from, to } => format!("across the line from {:.3} E, {:.3} N to {:.3} E, {:.3} N", from[0], from[1], to[0], to[1]),
        }
    }
}

/// A move in words: "100.5 m east, 200.25 m south and 3 m up".
pub(crate) fn move_words(by: [f64; 3]) -> String {
    let part = |v: f64, plus: &str, minus: &str| (v != 0.0).then(|| format!("{} m {}", metres(v.abs()), if v > 0.0 { plus } else { minus }));
    let parts: Vec<String> = [part(by[0], "east", "west"), part(by[1], "north", "south"), part(by[2], "up", "down")].into_iter().flatten().collect();
    match parts.as_slice() {
        [] => "nowhere: in place".into(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A number as short as it reads: 100.5, 3, 0.125.
fn metres(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The objects of a move or copy, and how they're moved.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformParams {
    pub objects: Vec<ObjectId>,
    pub transform: TransformParam,
}

/// The objects of a copy, and how the copies are moved from where the objects are: none, a copy in place.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyParams {
    pub objects: Vec<ObjectId>,
    #[serde(default)]
    pub transform: Option<TransformParam>,
}

/// An object's geometry moved whole, or why it can't be: only what's drawn moves, and a grid only moves, its rows
/// running east and north. A surface or solid keeps its crossing check, moved with it; mirrored, its triangles are
/// wound the other way and its flags worked out again, so it still faces out.
fn moved_body(object: &Object, t: &Transform) -> Result<Body, String> {
    match &object.body {
        Body::String(line) => Ok(Body::String(transform_string(line, t))),
        Body::Surface(surface) => {
            let moved = Surface::new(transform_mesh(surface.mesh(), t));
            Ok(Body::Surface(match surface.crossings() {
                Some(c) => moved.with_crossings(transform_crossings(c, t)),
                None => moved,
            }))
        }
        Body::Solid(solid) => {
            let moved = Solid::new(transform_mesh(solid.mesh(), t)).map_err(|_| "it isn't closed once moved, which a rigid move can't do: report it".to_string())?;
            Ok(Body::Solid(match solid.crossings() {
                Some(c) => moved.with_crossings(transform_crossings(c, t)),
                None => moved,
            }))
        }
        Body::Grid(grid) => match *t {
            Transform::Move { by } => Ok(Body::Grid(Arc::new(move_grid(grid, by)))),
            _ => Err("a grid moves but doesn't rotate or mirror: its rows run east and north".into()),
        },
        other => Err(format!("a {} isn't drawn, so it doesn't move: only strings, surfaces, solids and grids do", kind_words(other.kind()))),
    }
}

fn kind_words(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::String => "string",
        ObjectKind::Surface => "surface",
        ObjectKind::Solid => "solid",
        ObjectKind::Grid => "grid",
        ObjectKind::SeamModel => "seam model",
        ObjectKind::FaceDesign => "face design",
        ObjectKind::Workflow => "workflow",
        ObjectKind::ParameterTable => "parameter table",
        ObjectKind::Dataflow => "dataflow",
        ObjectKind::Table => "table",
        ObjectKind::Script => "script",
    }
}

/// Every object of the run moved, in order, or the run refused with each that can't be listed (decision 0078): nothing
/// is changed unless all of them can be.
fn move_all<'a>(inputs: &Inputs<'a>, ids: &[ObjectId], t: &Transform) -> Result<Vec<(&'a Arc<Object>, Body)>, CommandError> {
    if ids.is_empty() {
        return Err(CommandError::Invalid("nothing to move: pick strings, surfaces, solids or grids".into()));
    }
    if let Some(why) = t.problem() {
        return Err(CommandError::Invalid(why.into()));
    }
    let mut seen = HashSet::new();
    if let Some(twice) = ids.iter().find(|id| !seen.insert(**id)) {
        return Err(CommandError::Invalid(format!("object {twice} is in `objects` twice")));
    }
    let objects = ids.iter().map(|&id| inputs.object(id)).collect::<Result<Vec<_>, _>>()?;
    let mut moved = Vec::with_capacity(objects.len());
    let mut failed = Vec::new();
    inputs.progress(0, ids.len(), "objects");
    for object in objects {
        inputs.stop_if_cancelled()?;
        match moved_body(object, t) {
            Ok(body) => moved.push((object, body)),
            Err(why) => failed.push(format!("'{}': {why}", object.name)),
        }
        inputs.step();
    }
    if !failed.is_empty() {
        return Err(CommandError::Invalid(format!("{} of {} objects can't be moved, so nothing was changed. {}", failed.len(), ids.len(), failed.join("; "))));
    }
    Ok(moved)
}

/// How many of each kind, in words: "3 strings and 2 solids".
fn counted(objects: &[(&Arc<Object>, Body)]) -> String {
    let kinds = [ObjectKind::String, ObjectKind::Surface, ObjectKind::Solid, ObjectKind::Grid];
    let parts: Vec<String> = kinds
        .iter()
        .filter_map(|&kind| {
            let n = objects.iter().filter(|(o, _)| o.kind() == kind).count();
            (n > 0).then(|| format!("{n} {}{}", kind_words(kind), if n == 1 { "" } else { "s" }))
        })
        .collect();
    match parts.as_slice() {
        [] => "nothing".into(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `object.transform`: the next revision of each object picked, moved, rotated in plan or mirrored in plan as a whole,
/// in one run and one undo (String tools plan, workstream 3). Strings, surfaces and solids take all three; a grid only
/// moves. Nothing else about an object changes: its name, layer and attributes stay as they were.
pub struct TransformObjects;

impl Command for TransformObjects {
    type Params = TransformParams;
    const NAME: &'static str = "object.transform";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Move, rotate or mirror objects whole",
            inputs: vec![Field::new("objects", FieldType::Objects, "Strings, surfaces, solids and grids (a grid only moves)")],
            params: vec![Field::new("transform", FieldType::Text, "{kind: move, by: [E, N, RL]}, {kind: rotate, about: [E, N], angle} anticlockwise in degrees, or {kind: mirror, from: [E, N], to: [E, N]}")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let t = p.transform.geometry();
        if t.moves_nothing() {
            return Err(CommandError::Invalid(format!("that moves nothing: {}", p.transform.words())));
        }
        let moved = move_all(inputs, &p.objects, &t)?;
        let what = counted(&moved);
        let verb = match p.transform {
            TransformParam::Move { .. } => "moved",
            TransformParam::Rotate { .. } => "rotated",
            TransformParam::Mirror { .. } => "mirrored",
        };
        let changes = moved.into_iter().map(|(o, body)| Change::Revise { id: o.id, base: o.revision, name: None, layer: None, attributes: None, body: Some(body) }).collect();
        Ok(Outcome::new(changes).message(format!("{verb} {what} {}", p.transform.words())))
    }
}

/// `object.copy`: a new object for each one picked, on its layer, named for it with " copy" and carrying its
/// attributes, moved, rotated or mirrored from where it is as `object.transform` would move it, or in place. The
/// objects picked stay as they are (String tools plan, decision 5).
pub struct CopyObjects;

impl Command for CopyObjects {
    type Params = CopyParams;
    const NAME: &'static str = "object.copy";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Copy objects, moved, rotated or mirrored from where they are",
            inputs: vec![Field::new("objects", FieldType::Objects, "Strings, surfaces, solids and grids (a grid's copy only moves)")],
            params: vec![Field::new("transform", FieldType::Text, "As object.transform's; none copies in place")],
            outputs: vec![Field::new("objects", FieldType::Objects, "The copies, in the order picked")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let t = p.transform.map_or(Transform::Move { by: [0.0; 3] }, |t| t.geometry());
        let moved = move_all(inputs, &p.objects, &t)?;
        let what = counted(&moved);
        let changes = moved
            .into_iter()
            .map(|(o, body)| Change::Create { name: format!("{} copy", o.name), layer: inputs.layer_name(o), attributes: o.attributes.clone(), body })
            .collect();
        let whither = match p.transform {
            Some(t) if !t.geometry().moves_nothing() => {
                let verb = match t {
                    TransformParam::Move { .. } => "moved",
                    TransformParam::Rotate { .. } => "rotated",
                    TransformParam::Mirror { .. } => "mirrored",
                };
                format!(", {verb} {}", t.words())
            }
            _ => " in place".into(),
        };
        Ok(Outcome::new(changes).message(format!("copied {what}{whither}")))
    }
}
