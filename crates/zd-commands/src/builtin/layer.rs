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
