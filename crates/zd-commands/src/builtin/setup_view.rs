//! A reserves table's setup as the Setup tab shows it (the Build Plan's "Table setup plan", decision 5): what it's run on,
//! its dataflow and add-ons in order with their steps, each field with where it comes from (the table's own marked), its
//! parameter tables in the order read with the columns this table reads from each and those an earlier table gives
//! first, and its table scripts and calculated columns. Read only: an edit is `table.setup`, the table's next revision.

use std::sync::Arc;

use serde::Serialize;
use zd_core::{Body, Dataflow, FieldSource, LayerId, Object, ObjectId, ObjectKind, Project, TableSetup};

use super::read_by::read_through_p;
use crate::CommandError;

/// An object the setup names: its id, its name as it is now and its revision; `missing` where it's been deleted since.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Named {
    pub id: ObjectId,
    pub name: String,
    pub revision: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

/// The blocks the setup runs on: the layers (by name) and the filter, or the blocks named; and how many it picks now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlocksView {
    pub layers: Vec<(LayerId, String)>,
    pub filter: std::collections::BTreeMap<String, String>,
    pub named: usize,
    pub picked: usize,
}

/// One parameter table in the order the table reads it: where it's listed (`table`, `dataflow`, or an add-on's name),
/// the columns this table's steps, fields and scripts name in it, those nothing here names, and those a table read before
/// it gives first where its rows fit (`column`, by which table).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableRead {
    #[serde(flatten)]
    pub table: Named,
    pub from: String,
    pub read: Vec<String>,
    pub unread: Vec<String>,
    pub shadowed: Vec<(String, String)>,
}

/// One field or word field: where the dataflow (or its add-on) takes it from, which of them gives it, and the table's own
/// source where it has one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldView {
    pub name: String,
    pub words: bool,
    pub source: FieldSource,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own: Option<FieldSource>,
}

/// One step in the order it runs: its kind (`built-in`, `formula`, `allocation`, `table script`), whether it's on, and
/// the dataflow it comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetupStep {
    pub step: String,
    pub kind: String,
    pub on: bool,
    pub from: String,
}

/// The table a setup reads in place of the blocks: its rows and columns (words or numbers), for the tab's mapping.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputView {
    pub table: Named,
    pub rows: usize,
    pub columns: Vec<(String, bool)>,
    /// The attributes the run reads off each piece, for the mapping: the dataflow's levels, `horizon`, `seam`,
    /// `material`, `bench` and `bench_floor`, `case`, and each field's block attribute.
    pub reads: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetupView {
    pub table: Named,
    /// Whether the table holds its setup (else it's filled from the run that made it, and saved on the first edit).
    pub held: bool,
    pub setup: TableSetup,
    pub dataflow: Named,
    pub add_ons: Vec<Named>,
    /// Whether the add-ons are the table's own list, not the dataflow's.
    pub own_add_ons: bool,
    pub tables: Vec<TableRead>,
    /// Whether the parameter tables are the table's own list, not the dataflow's.
    pub own_tables: bool,
    /// The tables its scripts look up by name, under the name each is read by.
    pub looked_up: Vec<Named>,
    pub blocks: BlocksView,
    /// A table read in place of the blocks (Tables and scripts plan, decision 12): the table, its rows, and its columns
    /// for the mapping; its volumes are imported, not of record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Named>,
    pub stack: Vec<String>,
    /// How many rows of its own it reads before every table.
    pub own_rows: usize,
    pub fields: Vec<FieldView>,
    pub steps: Vec<SetupStep>,
    pub calculated: Vec<String>,
    /// What stops the setup putting together: an add-on that isn't one or clashes, a source for a field the dataflow
    /// hasn't. Said here so the tab shows it; a run stops on it.
    pub problems: Vec<String>,
    /// The project's main dataflows and add-ons, for the tab's lists.
    pub mains: Vec<Named>,
    pub add_on_choices: Vec<Named>,
}

fn named(project: &Project, id: ObjectId) -> Named {
    match project.object(id) {
        Some(o) => Named { id, name: o.name.clone(), revision: o.revision, missing: false },
        None => Named { id, name: "deleted since".into(), revision: 0, missing: true },
    }
}

fn dataflow_of(project: &Project, id: ObjectId) -> Option<Arc<Dataflow>> {
    match &project.object(id)?.body {
        Body::Dataflow(d) => Some(d.clone()),
        _ => None,
    }
}

fn step_kind(s: &zd_core::DataflowStep) -> &'static str {
    if s.formula.is_some() {
        "formula"
    } else if s.allocation.is_some() {
        "allocation"
    } else if s.table.is_some() {
        "table script"
    } else {
        "built-in"
    }
}

/// The Setup tab's view of a reserves table: its setup held, or filled from the run that made it.
pub fn setup_view(project: &Project, log: &[crate::LogEntry], table: ObjectId) -> Result<SetupView, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::Table(t) = &object.body else { return Err(CommandError::WrongKind { id: table, expected: ObjectKind::Table, found: object.kind() }) };
    let setup = super::setup_of(project, log, table)?;
    let main = dataflow_of(project, setup.dataflow).ok_or_else(|| CommandError::Invalid(format!("{}'s dataflow is gone: pick another in its setup", object.name)))?;
    let flow_name = named(project, setup.dataflow).name;
    let mut problems = Vec::new();

    let add_on_ids = setup.add_ons.clone().unwrap_or_else(|| main.add_ons.clone());
    let add_ons: Vec<(String, Arc<Dataflow>)> = add_on_ids.iter().filter_map(|&id| Some((named(project, id).name, dataflow_of(project, id)?))).collect();
    for &id in &add_on_ids {
        if dataflow_of(project, id).is_none() {
            problems.push(format!("the add-on {} isn't a dataflow in the project", named(project, id).name));
        }
    }
    let pairs: Vec<(&str, &Dataflow)> = add_ons.iter().map(|(n, d)| (n.as_str(), d.as_ref())).collect();
    let merged = main.with_add_ons(&pairs).unwrap_or_else(|e| {
        problems.push(e);
        (*main).clone()
    });
    let from_of = |step: Option<&str>, field: Option<&str>| -> String {
        let in_main = step.is_some_and(|s| main.steps.iter().any(|t| t.step == s)) || field.is_some_and(|f| main.fields.iter().any(|g| g.field.name == f) || main.words.iter().any(|w| w.name == f));
        if in_main {
            return flow_name.clone();
        }
        add_ons
            .iter()
            .find(|(_, a)| step.is_some_and(|s| a.steps.iter().any(|t| t.step == s)) || field.is_some_and(|f| a.fields.iter().any(|g| g.field.name == f) || a.words.iter().any(|w| w.name == f)))
            .map_or_else(|| flow_name.clone(), |(n, _)| n.clone())
    };

    // The parameter tables in the order read: the table's own list or the dataflow's, then its add-ons'.
    let own_tables = setup.tables.is_some();
    let first: Vec<ObjectId> = setup.tables.clone().unwrap_or_else(|| main.parameters.clone());
    let mut order: Vec<(ObjectId, String)> = first.iter().map(|&id| (id, if own_tables { "table".to_string() } else { "dataflow".to_string() })).collect();
    for (n, a) in &add_ons {
        for &id in &a.parameters {
            if !order.iter().any(|(o, _)| *o == id) {
                order.push((id, n.clone()));
            }
        }
    }
    let add_on_flows: Vec<Arc<Dataflow>> = add_ons.iter().map(|(_, d)| d.clone()).collect();
    let names = read_through_p(&main, &add_on_flows);
    let columns_of = |id: ObjectId| -> Vec<String> {
        match project.object(id).map(|o| &o.body) {
            Some(Body::ParameterTable(p)) => p.columns.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        }
    };
    let mut tables = Vec::new();
    for (k, (id, from)) in order.iter().enumerate() {
        let n = named(project, *id);
        if !n.missing && !matches!(project.object(*id).map(|o| &o.body), Some(Body::ParameterTable(_))) {
            problems.push(format!("{} is listed as a parameter table and isn't one", n.name));
        }
        let mut read = Vec::new();
        let mut unread = Vec::new();
        let mut shadowed = Vec::new();
        for c in columns_of(*id) {
            if names.iter().any(|m| m.eq_ignore_ascii_case(&c)) {
                read.push(c.clone());
            } else {
                unread.push(c.clone());
            }
            if let Some((earlier, _)) = order[..k].iter().find(|(e, _)| columns_of(*e).iter().any(|x| x.eq_ignore_ascii_case(&c))) {
                shadowed.push((c, named(project, *earlier).name));
            }
        }
        tables.push(TableRead { table: n, from: from.clone(), read, unread, shadowed });
    }
    let looked_up = merged
        .lookups
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let mut n = named(project, id);
            if let Some(used) = merged.lookup_names.get(i).filter(|u| !u.is_empty() && **u != n.name) {
                n.name = format!("{} (looked up as {used})", n.name);
            }
            n
        })
        .collect();

    let all: Vec<&Arc<Object>> = project.objects().collect();
    let blocks = BlocksView {
        layers: setup.blocks.layers.iter().map(|&l| (l, project.layer(l).map_or_else(|| "a layer since deleted".to_string(), |x| x.name.clone()))).collect(),
        filter: setup.blocks.filter.clone(),
        named: setup.blocks.ids.len(),
        picked: setup.blocks.resolve(all.iter().copied()).len(),
    };

    let mut fields: Vec<FieldView> = Vec::new();
    for f in &merged.fields {
        fields.push(FieldView { name: f.field.name.clone(), words: false, source: f.source.clone(), from: from_of(None, Some(&f.field.name)), own: setup.sources.get(&f.field.name).cloned() });
    }
    for w in &merged.words {
        fields.push(FieldView { name: w.name.clone(), words: true, source: w.source.clone(), from: from_of(None, Some(&w.name)), own: setup.sources.get(&w.name).cloned() });
    }
    for name in setup.sources.keys() {
        if !fields.iter().any(|f| &f.name == name) {
            problems.push(format!("the table gives a source for {name}, a field the dataflow and its add-ons haven't"));
        }
    }
    if let Some(input) = &setup.input
        && let Err(e) = super::reserves_run::pieces_of_table(&|id| project.object(id).cloned().ok_or(CommandError::MissingObject(id)), input)
    {
        problems.push(e.to_string());
    }
    let steps = merged.steps.iter().map(|s| SetupStep { step: s.step.clone(), kind: step_kind(s).into(), on: s.on, from: from_of(Some(&s.step), None) }).collect();

    Ok(SetupView {
        table: named(project, table),
        held: t.setup.is_some(),
        dataflow: named(project, setup.dataflow),
        add_ons: add_on_ids.iter().map(|&id| named(project, id)).collect(),
        own_add_ons: setup.add_ons.is_some(),
        tables,
        own_tables,
        looked_up,
        blocks,
        input: setup.input.as_ref().map(|i| {
            let (rows, columns) = match project.object(i.table).map(|o| &o.body) {
                Some(Body::Table(t)) => (t.rows(), t.columns.iter().map(|c| (c.name.clone(), matches!(c.values, zd_core::ColumnValues::Number(_)))).collect()),
                _ => (0, Vec::new()),
            };
            let mut reads: Vec<String> = merged.levels.iter().map(|l| l.attribute.clone()).collect();
            for a in ["horizon", "seam", "material", "bench", "bench_floor", "case"] {
                reads.push(a.to_string());
            }
            for source in merged.fields.iter().map(|f| setup.sources.get(&f.field.name).unwrap_or(&f.source)).chain(merged.words.iter().map(|w| setup.sources.get(&w.name).unwrap_or(&w.source))) {
                if let FieldSource::Attribute { attribute } = source {
                    reads.push(attribute.clone());
                }
            }
            let mut seen = std::collections::HashSet::new();
            reads.retain(|r| seen.insert(r.to_ascii_lowercase()));
            InputView { table: named(project, i.table), rows, columns, reads }
        }),
        model: setup.model.map(|id| named(project, id)),
        stack: setup.stack.clone(),
        own_rows: setup.parameters.as_ref().map_or(0, |p| p.rows.len()),
        fields,
        steps,
        calculated: t.calculated.iter().map(|c| c.name.clone()).collect(),
        problems,
        setup,
        mains: all.iter().filter(|o| matches!(&o.body, Body::Dataflow(d) if !d.add_on)).map(|o| named(project, o.id)).collect(),
        add_on_choices: all.iter().filter(|o| matches!(&o.body, Body::Dataflow(d) if d.add_on)).map(|o| named(project, o.id)).collect(),
    })
}
