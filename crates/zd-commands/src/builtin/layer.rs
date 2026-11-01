use serde::{Deserialize, Serialize};
use zd_core::Change;

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `layer.create`: a new, empty layer. Commands also create the layer they write to on demand.
pub struct CreateLayer;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateLayerParams {
    pub name: String,
    /// RGB; a colour is picked when left out.
    #[serde(default)]
    pub colour: Option<[u8; 3]>,
}

impl Command for CreateLayer {
    type Params = CreateLayerParams;
    const NAME: &'static str = "layer.create";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "New empty layer",
            inputs: vec![],
            params: vec![
                Field::new("name", FieldType::Text, "Layer name, unique in the project"),
                Field::new("colour", FieldType::Text, "Optional [r, g, b], 0-255"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        Ok(Outcome::new(vec![Change::CreateLayer { name: p.name.clone(), colour: p.colour }]))
    }
}

/// `layer.rename`: a new name for a layer. Objects stay on it, as they hold its id.
pub struct RenameLayer;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameLayerParams {
    pub layer: String,
    pub name: String,
}

impl Command for RenameLayer {
    type Params = RenameLayerParams;
    const NAME: &'static str = "layer.rename";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Rename a layer",
            inputs: vec![],
            params: vec![Field::new("layer", FieldType::Layer, "The layer"), Field::new("name", FieldType::Text, "New name, unique in the project")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let layer = inputs.layer(&p.layer)?;
        Ok(Outcome::new(vec![Change::ReviseLayer { id: layer.id, name: Some(p.name.clone()), colour: None }]).message(format!("{} is now {}", p.layer, p.name)))
    }
}

/// `layer.recolour`: a new colour for a layer.
pub struct RecolourLayer;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecolourLayerParams {
    pub layer: String,
    pub colour: [u8; 3],
}

impl Command for RecolourLayer {
    type Params = RecolourLayerParams;
    const NAME: &'static str = "layer.recolour";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Change a layer's colour",
            inputs: vec![],
            params: vec![Field::new("layer", FieldType::Layer, "The layer"), Field::new("colour", FieldType::Text, "[r, g, b], 0-255")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let layer = inputs.layer(&p.layer)?;
        Ok(Outcome::new(vec![Change::ReviseLayer { id: layer.id, name: None, colour: Some(p.colour) }]).message(format!("{} recoloured", p.layer)))
    }
}

/// `layer.delete`: a layer and everything on it; undo brings them back.
pub struct DeleteLayer;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteLayerParams {
    pub layer: String,
}

impl Command for DeleteLayer {
    type Params = DeleteLayerParams;
    const NAME: &'static str = "layer.delete";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Delete a layer and everything on it",
            inputs: vec![],
            params: vec![Field::new("layer", FieldType::Layer, "The layer")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let layer = inputs.layer(&p.layer)?;
        let objects = inputs.objects_on(layer.id)?;
        let count = objects.len();
        let mut changes: Vec<Change> = objects.into_iter().map(|o| Change::Delete { id: o.id, base: o.revision }).collect();
        changes.push(Change::DeleteLayer { id: layer.id });
        Ok(Outcome::new(changes).message(format!("{} deleted with {count} object{}", p.layer, if count == 1 { "" } else { "s" })))
    }
}
