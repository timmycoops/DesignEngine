//! Whether each of a case's dataflows is up to date, and why not (the Build Plan's "Reserves case plan", decision 6,
//! workstream 2, `docs/decisions/0105-reserves-cases.md`). Read from the run log, never kept: each dataflow's last run on
//! the table that wasn't undone, what that run read against what's there now, and the runs of the dataflows above it.
//! Nothing runs by itself (rule 4): a dataflow out of date is said, with why, and the person runs it, or every one out
//! of date in order.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use zd_core::{Body, ObjectId, RunId, TableSetup};

use crate::{Command, CommandError, LogEntry, RunRecord, RunStatus};

/// One of a case's dataflows as it stands: its name, whether it's the reserves dataflow, its range, its last run and
/// when, and whether it's up to date, with each reason it isn't.
#[derive(Debug, Clone, Serialize)]
pub struct DataflowState {
    pub dataflow: ObjectId,
    pub name: String,
    pub reserves: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub range: BTreeMap<String, String>,
    pub run: Option<RunId>,
    /// When its last run started, in UTC words.
    pub ran: Option<String>,
    pub up_to_date: bool,
    pub why: Vec<String>,
    /// The settings tables it reads, by name, in the order read.
    pub tables: Vec<String>,
}

/// A case, a reserves table, and each of its dataflows in run order.
#[derive(Debug, Clone, Serialize)]
pub struct CaseState {
    pub table: ObjectId,
    pub name: String,
    pub dataflows: Vec<DataflowState>,
}

impl CaseState {
    /// The dataflows out of date, in run order.
    pub fn out_of_date(&self) -> Vec<ObjectId> {
        self.dataflows.iter().filter(|d| !d.up_to_date).map(|d| d.dataflow).collect()
    }
}

/// The runs the log has undone and not redone since.
fn undone(log: &[LogEntry]) -> HashSet<RunId> {
    let mut out = HashSet::new();
    for e in log {
        match e {
            LogEntry::Undo { run, .. } => {
                out.insert(*run);
            }
            LogEntry::Redo { run, .. } => {
                out.remove(run);
            }
            _ => {}
        }
    }
    out
}

/// Where a run is in the log.
fn at(log: &[LogEntry], run: RunId) -> usize {
    log.iter().position(|e| matches!(e, LogEntry::Run(r) if r.run == run)).unwrap_or(0)
}

/// Whether a run is a row dataflow's run of this dataflow on this table that wrote it.
fn row_run_of(r: &RunRecord, table: ObjectId, dataflow: ObjectId) -> bool {
    r.command == super::RunRowDataflow::NAME
        && r.status == RunStatus::Ok
        && serde_json::from_value::<super::RunRowDataflowParams>(r.params.clone()).is_ok_and(|p| p.table == table && p.dataflow == dataflow)
        && r.outputs.iter().any(|o| o.id == table)
}

/// The setup a run found on the table: the last `table.setup` of it before the run that wasn't undone; none where the
/// run that made the table set it.
fn setup_before(log: &[LogEntry], table: ObjectId, before: usize, undone: &HashSet<RunId>) -> Option<TableSetup> {
    log[..before].iter().rev().find_map(|e| match e {
        LogEntry::Run(r) if r.command == super::SaveTableSetup::NAME && r.status == RunStatus::Ok && !undone.contains(&r.run) => {
            serde_json::from_value::<super::SaveTableSetupParams>(r.params.clone()).ok().filter(|p| p.table == table).map(|p| p.setup)
        }
        _ => None,
    })
}

/// What a run read that's changed or gone since, by name, the table itself aside.
fn inputs_changed(project: &zd_core::Project, r: &RunRecord, table: ObjectId) -> Vec<String> {
    let mut out = Vec::new();
    for input in r.inputs.iter().filter(|i| i.id != table) {
        match project.object(input.id) {
            Some(o) if o.revision != input.revision => out.push(format!("{} changed", o.name)),
            None => out.push("something it read is gone".to_string()),
            _ => {}
        }
    }
    out.dedup();
    out
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Each of a case's dataflows, up to date or why not (decision 6): the reserves dataflow first, then its row dataflows
/// in order. A dataflow is out of date where it has never run on the table; where something its last run read has
/// changed or gone since (a block, its settings tables, its scripts, the dataflow itself); where the case's setup has
/// changed what it runs on since (its blocks, its range or tables); where a dataflow above it ran after it; and where one
/// above it is out of date, so its numbers are from what that one will change.
pub fn case_state(project: &zd_core::Project, log: &[LogEntry], table: ObjectId) -> Result<CaseState, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::Table(_) = &object.body else { return Err(CommandError::WrongKind { id: table, expected: zd_core::ObjectKind::Table, found: object.kind() }) };
    let setup = super::setup_of(project, log, table)?;
    let undone = undone(log);
    let name_of = |id: ObjectId| project.object(id).map_or_else(|| "a dataflow since deleted".to_string(), |o| o.name.clone());
    // A dataflow's settings tables: those the setup lists for it, else its own.
    let tables_of = |flow: ObjectId, listed: &Option<Vec<ObjectId>>| -> Vec<String> {
        let ids = listed.clone().unwrap_or_else(|| match project.object(flow).map(|o| &o.body) {
            Some(Body::Dataflow(d)) => d.parameters.clone(),
            _ => Vec::new(),
        });
        ids.iter().map(|&id| project.object(id).map_or_else(|| "a table since deleted".to_string(), |o| o.name.clone())).collect()
    };
    let mut out: Vec<DataflowState> = Vec::with_capacity(1 + setup.row_dataflows.len());
    // Where each dataflow's last run is in the log, for "ran after it".
    let mut positions: Vec<Option<usize>> = Vec::new();

    // The reserves dataflow: the run that last wrote the table.
    let record = super::reserves_run::table_run(log, object).filter(|r| !undone.contains(&r.run));
    let mut why = Vec::new();
    match record {
        None => why.push("it hasn't run on this table".to_string()),
        Some(r) => {
            let changed = super::changed_since(project, log, table)?;
            if changed.blocks > 0 {
                why.push(format!("{} block{} changed", changed.blocks, plural(changed.blocks)));
            }
            why.extend(changed.others.iter().map(|o| format!("{o} changed")));
            if changed.gone > 0 {
                why.push(format!("{} of what it read {} gone", changed.gone, if changed.gone == 1 { "is" } else { "are" }));
            }
            if let Ok(p) = serde_json::from_value::<super::RunReservesParams>(r.params.clone()) {
                if setup.input.is_none() {
                    let ran: HashSet<ObjectId> = p.blocks.iter().copied().collect();
                    let added = setup.blocks.resolve(project.objects()).into_iter().filter(|id| !ran.contains(id)).count();
                    if added > 0 {
                        why.push(format!("{added} block{} added to what the setup picks", plural(added)));
                    }
                }
                let same = p.dataflow == setup.dataflow
                    && p.model == setup.model
                    && p.stack == setup.stack
                    && p.parameters == setup.parameters
                    && p.add_ons == setup.add_ons
                    && p.sources == setup.sources
                    && p.tables == setup.tables
                    && p.input == setup.input;
                if !same {
                    why.push("its setup changed since it ran".to_string());
                }
            }
        }
    }
    positions.push(record.map(|r| at(log, r.run)));
    out.push(DataflowState {
        dataflow: setup.dataflow,
        name: name_of(setup.dataflow),
        reserves: true,
        range: BTreeMap::new(),
        run: record.map(|r| r.run),
        ran: record.map(|r| super::reserves_run::utc_words(r.started_unix_ms)),
        up_to_date: why.is_empty(),
        why,
        tables: tables_of(setup.dataflow, &setup.tables),
    });

    // Its row dataflows, each against its own last run and those above it.
    for listed in &setup.row_dataflows {
        let last = log.iter().enumerate().rev().find_map(|(i, e)| match e {
            LogEntry::Run(r) if row_run_of(r, table, listed.dataflow) && !undone.contains(&r.run) => Some((i, r)),
            _ => None,
        });
        let mut why = Vec::new();
        match last {
            None => why.push("it hasn't run on this table".to_string()),
            Some((i, r)) => {
                why.extend(inputs_changed(project, r, table));
                if let Some(then) = setup_before(log, table, i, &undone).and_then(|s| s.row_dataflows.into_iter().find(|d| d.dataflow == listed.dataflow))
                    && (then.range != listed.range || then.tables != listed.tables)
                {
                    why.push(if then.range != listed.range { "its range changed since it ran" } else { "its settings tables changed since it ran" }.to_string());
                }
                for (above, state) in positions.iter().zip(&out) {
                    match above {
                        Some(j) if *j > i => why.push(format!("{} ran after it", state.name)),
                        _ if !state.up_to_date => why.push(format!("{} is out of date", state.name)),
                        _ => {}
                    }
                }
            }
        }
        positions.push(last.map(|(i, _)| i));
        out.push(DataflowState {
            dataflow: listed.dataflow,
            name: name_of(listed.dataflow),
            reserves: false,
            range: listed.range.clone(),
            run: last.map(|(_, r)| r.run),
            ran: last.map(|(_, r)| super::reserves_run::utc_words(r.started_unix_ms)),
            up_to_date: why.is_empty(),
            why,
            tables: tables_of(listed.dataflow, &listed.tables),
        });
    }
    Ok(CaseState { table, name: object.name.clone(), dataflows: out })
}
