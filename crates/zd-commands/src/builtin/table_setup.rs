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
        // What the Setup tab may set (decision 5): add-ons that are add-ons, parameter tables that are, sources for fields
        // the dataflow and its add-ons have, all put together as a run would.
        let Body::Dataflow(main) = &flow.body else { unreachable!() };
        let mut add_ons = Vec::new();
        for &id in s.add_ons.as_ref().unwrap_or(&main.add_ons) {
            let a = inputs.object(id)?;
            let Body::Dataflow(d) = &a.body else { return Err(CommandError::WrongKind { id, expected: ObjectKind::Dataflow, found: a.kind() }) };
            if !d.add_on {
                return Err(CommandError::Invalid(format!("{} is a main dataflow, not an add-on: a setup lists add-ons only", a.name)));
            }
            add_ons.push((a.name.clone(), d.clone()));
        }
        for &id in s.tables.iter().flatten() {
            let t = inputs.object(id)?;
            if !matches!(t.body, Body::ParameterTable(_)) {
                return Err(CommandError::Invalid(format!("{} isn't a parameter table: a setup reads parameter tables only", t.name)));
            }
        }
        let pairs: Vec<(&str, &zd_core::Dataflow)> = add_ons.iter().map(|(n, d)| (n.as_str(), d.as_ref())).collect();
        let merged = main.with_add_ons(&pairs).map_err(CommandError::Invalid)?;
        for name in s.sources.keys() {
            if !merged.fields.iter().any(|f| &f.field.name == name) && !merged.words.iter().any(|w| &w.name == name) {
                return Err(CommandError::Invalid(format!("the setup gives a source for {name}, a field {} and its add-ons haven't", flow.name)));
            }
        }
        // The case's dataflows after its reserves dataflow (Reserves case plan, decisions 2 to 5): each a row dataflow
        // listed once, its tables parameter tables, no column with two writers, none read from a dataflow below.
        let flows = super::row_flows(&|id| inputs.object(id).cloned(), s)?;
        let written = table.setup.as_ref().map(|old| super::row_columns_of(inputs.project(), old)).unwrap_or_default();
        super::check_case(table, &flows, &written).map_err(CommandError::Invalid)?;
        // A table read in place of the blocks (decision 12 of 0099). A mapping not yet whole is kept, so the Setup tab
        // can finish it a column at a time; what it lacks is said here and in the tab, and the run refuses it.
        if let Some(input) = &s.input {
            let read = inputs.object(input.table)?;
            let Body::Table(_) = &read.body else { return Err(CommandError::WrongKind { id: input.table, expected: ObjectKind::Table, found: read.kind() }) };
            let said = match super::reserves_run::pieces_of_table(&|id| inputs.object(id).cloned(), input) {
                Ok((pieces, _, from)) => format!("{} rows of {from}", pieces.len()),
                Err(e) => format!("{}, the mapping not yet whole: {e}", read.name),
            };
            let mut next = (**table).clone();
            next.setup = Some(s.clone());
            let change = Change::Revise { id: p.table, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(next))) };
            return Ok(Outcome::new(vec![change]).message(format!("saved {}'s setup: {said}, run by {}", object.name, flow.name)));
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
