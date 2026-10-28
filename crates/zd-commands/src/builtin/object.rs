use serde::{Deserialize, Serialize};
use zd_core::{Change, ObjectId};

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

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
        Ok(Outcome::new(vec![Change::Delete { id: object.id, base: object.revision }]))
    }
}
