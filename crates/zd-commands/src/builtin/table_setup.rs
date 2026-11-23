//! `table.setup`: what fills a reserves table, saved with it (the Build Plan's "Table setup plan", decisions 1 and 6). The
//! dataflow, the blocks (by layer and filter, or exactly these), the seam model or stack, and the table's own parameter
//! rows. Run again runs the setup; Python and workflow steps set it the same way as the app.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Body, Change, ObjectId, ObjectKind, TableSetup};

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct SaveTableSetup;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveTableSetupParams {
    pub table: ObjectId,
    pub setup: TableSetup,
}

impl Command for SaveTableSetup {
    type Params = SaveTableSetupParams;
    const NAME: &'static str = "table.setup";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save what fills a reserves table: its dataflow, its blocks by layer and filter, its seam model or stack, and its own parameter rows; Run again runs it",
            inputs: vec![Field::object("table", ObjectKind::Table, "The reserves table")],
            params: vec![Field::new("setup", FieldType::Text, "The setup: dataflow, blocks {layers, filter, ids}, model or stack, parameters")],
            outputs: vec![Field::object("table", ObjectKind::Table, "The table, its setup saved")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.table)?;
        let Body::Table(table) = &object.body else {
            return Err(CommandError::WrongKind { id: p.table, expected: ObjectKind::Table, found: object.kind() });
        };
        let s = &p.setup;
        let flow = inputs.object(s.dataflow)?;
        if !matches!(flow.body, Body::Dataflow(_)) {
            return Err(CommandError::WrongKind { id: s.dataflow, expected: ObjectKind::Dataflow, found: flow.kind() });
        }
        match s.model {
            Some(id) => {
                let model = inputs.object(id)?;
                if !matches!(model.body, Body::SeamModel(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::SeamModel, found: model.kind() });
                }
            }
            None if s.stack.iter().all(|h| h.trim().is_empty()) => return Err(CommandError::Invalid("a setup orders each column by a seam model, or by a stack of horizons top down: give one".into())),
            None => {}
        }
        if let Some(rows) = &s.parameters {
            rows.check().map_err(|e| CommandError::Invalid(format!("the table's own parameter rows: {e}")))?;
        }
        let picked = s.blocks.resolve(inputs.objects());
        if picked.is_empty() {
            return Err(CommandError::Invalid("the setup picks no blocks: name their layers (and a filter), or the blocks themselves".into()));
        }
        let mut next = (**table).clone();
        next.setup = Some(s.clone());
        let change = Change::Revise { id: p.table, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(next))) };
        let how = if s.blocks.ids.is_empty() { format!("{} layer{}", s.blocks.layers.len(), if s.blocks.layers.len() == 1 { "" } else { "s" }) } else { "the blocks named".to_string() };
        Ok(Outcome::new(vec![change]).message(format!("saved {}'s setup: {} blocks from {how}, run by {}", object.name, picked.len(), flow.name)))
    }
}
