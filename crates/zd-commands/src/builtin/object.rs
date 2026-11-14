use serde::{Deserialize, Serialize};
use zd_core::{Body, Change, Object, ObjectId};

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
