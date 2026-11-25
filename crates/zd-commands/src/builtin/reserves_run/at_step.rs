//! A case at a step (the Build Plan's "Reserves case plan", decisions 8 and 9, workstream 3,
//! `docs/decisions/0105-reserves-cases.md`): every block column of a reserves table's case run through its dataflow to one
//! step and kept there, with each column as it stood before that step. The case's totals at the step are read from what
//! is kept: the units by material and what became of them, what the steps so far have written by folder, what the step
//! said, and the balance, no ground made or lost. An edit reruns from the first step that reads it: the step open from the
//! columns kept before it, an edit read earlier from the blocks. Nothing is saved; the table changes only by a run.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use zd_core::{Attributes, Body, Dataflow, FieldSource, ObjectId, ObjectKind, ParameterTable, Table};
use zd_reserves::{Column, ColumnKey, add_up_values};

use super::{BALANCE_TOLERANCE, ColumnField, ColumnRun, NAMED, Piece, Placement, Prepared, Ran, RunReservesParams, Said, StepRun, StepUnits, Stopped, Unsaved, UnitView};
use crate::CommandError;

/// A column as it stands after some steps, kept to run on from: what a column in work holds, but for what it borrows
/// from the run's prepared dataflow, and its volume as input.
#[derive(Clone)]
struct ColumnState {
    column: Column,
    attributes: Vec<Attributes>,
    rows: Vec<Piece>,
    said: Said,
    added_volume: f64,
    block_area: f64,
    moves: Vec<Piece>,
    finalised: bool,
    placed: Vec<Placement>,
    input_volume: f64,
}

impl ColumnState {
    fn of(run: ColumnRun<'_>, input_volume: f64) -> Self {
        ColumnState {
            column: run.column,
            attributes: run.attributes,
            rows: run.rows,
            said: run.said,
            added_volume: run.added_volume,
            block_area: run.block_area,
            moves: run.moves,
            finalised: run.finalised,
            placed: run.placed,
            input_volume,
        }
    }

    /// The column in work again, on `prepared`, noting the parameter columns its steps look up.
    fn resume(self, prepared: &Prepared) -> ColumnRun<'_> {
        ColumnRun {
            column: self.column,
            horizons: prepared.horizons.clone(),
            attributes: self.attributes,
            tables: &prepared.tables,
            dataflow: &prepared.dataflow,
            rows: self.rows,
            lower: &prepared.lower,
            upper: &prepared.upper,
            said: self.said,
            added_volume: self.added_volume,
            block_area: self.block_area,
            reads: None,
            workings: None,
            moves: self.moves,
            finalised: self.finalised,
            placed: self.placed,
            asked: Some(std::cell::RefCell::new(BTreeSet::new())),
        }
    }

    /// Roughly what it holds in memory, in bytes: each unit's and piece's values and their keeping.
    fn bytes(&self) -> usize {
        let fields = self.column.fields().len();
        let piece = |p: &Piece| 96 + 8 * p.values.len() + p.leaf.iter().map(|l| 24 + l.len()).sum::<usize>() + p.folder.len();
        self.column.len() * (200 + 8 * fields) + self.rows.iter().chain(&self.moves).map(piece).sum::<usize>() + self.attributes.len() * 256
    }
}

/// A column kept, or why it stopped on the way: a column that stops stays out, said, and the rest go on.
type Kept = Result<ColumnState, String>;

/// The case's columns as input: each block column made units, its imported rows written.
fn begin(prepared: &Prepared, key: &ColumnKey) -> Kept {
    let (run, input_volume) = super::begin_column(prepared, key, &prepared.columns[key]).map_err(|s| s.why)?;
    Ok(ColumnState::of(run, input_volume))
}

/// One step on one column: the column as kept, the step run on it, kept again; and the parameter columns it looked up.
fn step_column(prepared: &Prepared, kept: Kept, i: usize) -> (Kept, BTreeSet<String>) {
    let state = match kept {
        Ok(state) => state,
        Err(why) => return (Err(why), BTreeSet::new()),
    };
    let (name, step, settings) = &prepared.steps[i];
    let input_volume = state.input_volume;
    let words = state.column.key.words();
    let mut run = state.resume(prepared);
    let done = super::run_step(&mut run, step, settings, &prepared.declared);
    let asked = run.asked.take().map(|a| a.into_inner()).unwrap_or_default();
    match done {
        Ok(()) => (Ok(ColumnState::of(run, input_volume)), asked),
        Err(why) => (Err(format!("{words} at {name}: {why}")), asked),
    }
}

/// `work` on each of `items` across the cores, each taken whole: the results in the items' order.
fn each<T: Send, R: Send>(items: Vec<T>, work: impl Fn(T) -> R + Sync) -> Vec<R> {
    let slots: Vec<Mutex<Option<T>>> = items.into_iter().map(|t| Mutex::new(Some(t))).collect();
    crate::builtin::in_parallel(&slots, |slot| work(slot.lock().expect("a slot").take().expect("each item taken once")))
}

/// What a step did to one block column (decision 8, the block map): its words (`Pit`, `3`, `B5`), why it stopped if it
/// did, the volume the step said it moved (its losses, its dilution), the volume it wrote to the table, and the column's
/// coal before and after it (coal nothing has become of yet).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColumnDid {
    pub path: Vec<String>,
    pub stopped: Option<String>,
    pub moved: f64,
    pub written: f64,
    pub coal_before: f64,
    pub coal_after: f64,
}

/// Units of one material and what became of them, over the whole case at a step (decision 10, the flow): `coal` with
/// nothing become of it, `coal` lost to `loss` (`roof`), `waste` let in as `dilution` (`roof`); how many, their volume and
/// their in situ tonnes (none where nothing weighs).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowGroup {
    pub material: String,
    pub became: Option<String>,
    pub detail: String,
    pub units: usize,
    pub volume: f64,
    pub tonnes: Option<f64>,
}

/// The case after a step (0 for its blocks as read), its units by material and what became of them, for the flow
/// across the Reserves case tab: the steps before the one open as they ran, those after it run on from it and not kept.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlowStep {
    pub step: usize,
    pub name: String,
    pub groups: Vec<FlowGroup>,
    /// Block columns stopped by this step or one before it.
    pub stopped: usize,
}

/// Units by material, what became of them and its detail.
type ByBecame<'a> = BTreeMap<(String, Option<String>, String), Vec<&'a [f64]>>;

/// The case's units after a step, by material and what became of them.
fn flow_of(prepared: &Prepared, states: &[Kept], step: usize) -> FlowStep {
    let volume = prepared.fields.volume();
    let mut groups: ByBecame<'_> = BTreeMap::new();
    let mut stopped = 0;
    for kept in states {
        let Ok(state) = kept else {
            stopped += 1;
            continue;
        };
        for u in state.column.units() {
            let became = u.became.as_ref().map(|b| (format!("{:?}", b.kind).to_lowercase(), b.detail.clone()));
            let key = (u.in_situ.material.to_lowercase(), became.as_ref().map(|b| b.0.clone()), became.map(|b| b.1).unwrap_or_default());
            groups.entry(key).or_default().push(u.values());
        }
    }
    let groups = groups
        .into_iter()
        .map(|((material, became, detail), values)| {
            let tonnes = tonnes_by(prepared, None, &values).map(|d| values.iter().map(|u| u[volume] * u[d]).filter(|t| t.is_finite()).sum::<f64>() + 0.0);
            let v = values.iter().map(|u| u[volume]).filter(|v| v.is_finite()).sum::<f64>() + 0.0;
            FlowGroup { material, became, detail, units: values.len(), volume: v, tonnes }
        })
        .collect();
    let name = if step == 0 { "as input".into() } else { prepared.steps[step - 1].0.clone() };
    FlowStep { step, name, groups, stopped }
}

/// One step's time over the whole case, its columns across the cores.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepTime {
    /// Its number: 0 for the blocks read as input, 1 for the first step.
    pub step: usize,
    pub name: String,
    pub ms: f64,
}

/// A group of units, or a folder of what's written, added up: how many, and each field by its own rule (volumes and
/// tonnes summed, qualities weighted by tonnes, densities by volume). None where nothing weighs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Totals {
    /// `coal`, `waste`, `coal, loss (roof)`; or a folder, `Insitu\Coal`, `Loss\Roof`, `ROM\Coal`.
    pub group: String,
    pub count: usize,
    pub values: BTreeMap<String, Option<f64>>,
    /// Its tonnes as the table works them: a folder's leaf by leaf, by the ROM density under ROM and Scheduled, else by
    /// the in situ density where there is one, else by the dataflow's density; units by the in situ density. None where
    /// there's no density.
    pub tonnes: Option<f64>,
}

/// The density a folder's tonnes are worked by, as the reserves table works them: ROM and Scheduled by the ROM density,
/// the rest by the in situ density where any piece has one, else the dataflow's first density. Units are taken as in
/// situ (no folder).
fn tonnes_by(prepared: &Prepared, folder: Option<&str>, values: &[&[f64]]) -> Option<usize> {
    let fields = prepared.fields.list();
    let setting = |key: &str| prepared.steps.iter().find(|(n, _, _)| n == "in_situ").map_or(key.to_string(), |(_, _, s)| s.setting(key).to_string());
    let field = |key: &str| fields.iter().position(|f| f.name == setting(key));
    let held = |d: usize| values.iter().any(|v| v[d].is_finite());
    let rom_basis = folder.is_some_and(|f| ["ROM", "Scheduled"].iter().any(|top| f.starts_with(&format!("{top}\\"))));
    if let (true, Some(_), Some(rd_rom)) = (rom_basis, field("rd"), field("rd_rom"))
        && held(rd_rom)
    {
        return Some(rd_rom);
    }
    if let (Some(_), Some(rd_is)) = (field("rd"), field("rd_is"))
        && held(rd_is)
    {
        return Some(rd_is);
    }
    fields.iter().position(|f| f.kind == zd_reserves::FieldKind::Density)
}

/// What the step said over the case: how many units and their volume (`roof loss`, 24 units, 249,253 m³); or a check and
/// how many blocks it names, with no volume.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SaidTotal {
    pub what: String,
    pub units: usize,
    pub volume: Option<f64>,
    /// The blocks a check names, the first few.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
}

/// No ground made or lost at the step (decision 8): the case's volume in from its blocks, the volume its columns hold
/// now, what came in from outside the model (the underburden), and the columns off by more than rounding.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Balance {
    pub volume_in: f64,
    pub volume_now: f64,
    pub added: f64,
    /// The worst column's difference, relative to its volume in.
    pub worst: f64,
    /// Every column kept its volume to `reserves.unbalanced`'s tolerance.
    pub balanced: bool,
    /// The columns that didn't, each with its difference in m³, the first few and how many more.
    pub unbalanced: Vec<String>,
}

/// The whole case at a step (decisions 8 and 9).
#[derive(Debug, Clone, Serialize)]
pub struct StepTotals {
    pub table: ObjectId,
    /// The step's number: 0 for the blocks as read, 1 for the first step.
    pub step: usize,
    /// Its name: `as input` for none.
    pub name: String,
    /// Every step's name in run order, for picking another.
    pub steps: Vec<String>,
    /// Columns run to the step, and those that stopped before it, each with why.
    pub columns: usize,
    pub stopped: Vec<String>,
    pub fields: Vec<ColumnField>,
    /// The units as they stand, by material and what became of them.
    pub units: Vec<Totals>,
    /// What the steps so far have written to the table, by folder, as the table adds it up.
    pub written: Vec<Totals>,
    /// What this step said over the case.
    pub said: Vec<SaidTotal>,
    pub balance: Balance,
    /// The step the last change ran from, by number; none where nothing it changed is read by this step or before it.
    pub ran_from: Option<usize>,
    /// The steps the last change ran, each with its time over the whole case.
    pub ran: Vec<StepTime>,
    /// Roughly the memory the kept columns hold, in MB: the columns at the step and before it.
    pub kept_mb: f64,
}

/// What a step reads of the settings: some parameter columns by name (lower case), or every one.
#[derive(Debug, Clone, PartialEq)]
enum Reads {
    Some(BTreeSet<String>),
    All,
}

impl Reads {
    fn touches(&self, changed: &Changed) -> bool {
        match (self, changed) {
            (_, Changed::Nothing) => false,
            (Reads::All, _) | (_, Changed::All) => true,
            (Reads::Some(names), Changed::Columns(columns)) => names.iter().any(|n| columns.contains(n)),
        }
    }
}

/// Which settings an edit changed: the columns whose values moved (lower case), or every one (a row's filters, the
/// rows' order, a table added or taken away).
#[derive(Debug, Clone, PartialEq)]
enum Changed {
    Nothing,
    Columns(BTreeSet<String>),
    All,
}

impl Changed {
    fn add(&mut self, other: Changed) {
        *self = match (std::mem::replace(self, Changed::Nothing), other) {
            (Changed::All, _) | (_, Changed::All) => Changed::All,
            (Changed::Nothing, o) | (o, Changed::Nothing) => o,
            (Changed::Columns(mut a), Changed::Columns(b)) => {
                a.extend(b);
                Changed::Columns(a)
            }
        };
    }
}

/// What `dir(p)` reads: which settings a unit has, not their values. A cell filled or cleared changes it.
const NAMES: &str = "@names";

/// The columns of a parameter table an edit changed, read as its lookup reads them: a value moved in a row changes its
/// column, and a cell filled or cleared which names a unit has too; a filter, a row's filters or the rows themselves
/// changed, every column of it.
fn table_changed(was: &ParameterTable, now: &ParameterTable) -> Changed {
    if was == now {
        return Changed::Nothing;
    }
    let lower = |s: &str| s.to_lowercase();
    let same_shape = was.filters.iter().map(|f| lower(f)).eq(now.filters.iter().map(|f| lower(f)))
        && was.rows.len() == now.rows.len()
        && was.rows.iter().zip(&now.rows).all(|(a, b)| a.when == b.when);
    if !same_shape {
        return Changed::All;
    }
    let mut columns = BTreeSet::new();
    for (a, b) in was.rows.iter().zip(&now.rows) {
        let a_values: BTreeMap<String, &zd_core::ParamValue> = a.values.iter().map(|(k, v)| (lower(k), v)).collect();
        let b_values: BTreeMap<String, &zd_core::ParamValue> = b.values.iter().map(|(k, v)| (lower(k), v)).collect();
        for name in a_values.keys().chain(b_values.keys()) {
            let (a, b) = (a_values.get(name), b_values.get(name));
            if a != b {
                columns.insert(name.clone());
                if a.is_none() || b.is_none() {
                    columns.insert(NAMES.to_string());
                }
            }
        }
    }
    if columns.is_empty() { Changed::Nothing } else { Changed::Columns(columns) }
}

/// The settings a formula or allocation script reads: the columns `p.name` and `getattr(p, "name")` name; a name built
/// as it runs (`getattr(p, field + "_min")`) taken as a field's own or its `_default`, `_min` or `_max`, as Read by
/// takes it; `dir(p)`, which names a unit has; `p` handed on whole (`helper(p)`), every setting.
fn script_reads(source: &str, dataflow: &Dataflow) -> Reads {
    let mut names: BTreeSet<String> = zd_core::parameters_read_in(source).into_iter().map(|n| n.to_lowercase()).collect();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("");
        if line.trim_start().starts_with("def ") {
            continue;
        }
        let bytes = line.as_bytes();
        for (at, _) in line.match_indices('p') {
            let before = at.checked_sub(1).map(|b| bytes[b]);
            if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'"' || b == b'\'') {
                continue;
            }
            let after = line[at + 1..].trim_start();
            if after.chars().next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '"' || c == '\'') {
                continue;
            }
            // `p` as a name on its own: `dir(p)`, getattr's or hasattr's first argument, else handed on whole.
            let head = line[..at].trim_end();
            if head.ends_with("dir(") {
                names.insert(NAMES.to_string());
                continue;
            }
            if head.ends_with("getattr(") || head.ends_with("hasattr(") {
                let name = after.trim_start_matches(',').trim_start();
                if !name.starts_with('"') && !name.starts_with('\'') {
                    for f in dataflow.fields.iter().map(|f| f.field.name.as_str()).chain(dataflow.words.iter().map(|w| w.name.as_str())) {
                        names.insert(f.to_lowercase());
                        for end in ["default", "min", "max"] {
                            names.insert(format!("{f}_{end}").to_lowercase());
                        }
                    }
                }
                continue;
            }
            return Reads::All;
        }
    }
    Reads::Some(names)
}

/// The settings the blocks are read with, before any step: the fields and words taken from a parameter column, and those
/// from a block attribute, which fall back to the column of their name.
fn input_reads(dataflow: &Dataflow) -> Reads {
    let mut names = BTreeSet::new();
    for (name, source) in dataflow.fields.iter().map(|f| (&f.field.name, &f.source)).chain(dataflow.words.iter().map(|w| (&w.name, &w.source))) {
        match source {
            FieldSource::Parameter { column } => {
                names.insert(column.to_lowercase());
            }
            FieldSource::Attribute { .. } => {
                names.insert(name.to_lowercase());
            }
            _ => {}
        }
    }
    Reads::Some(names)
}

/// The blocks read the same way: the same columns of the same blocks at the same revisions with the same attributes
/// (the Objects table's calculated columns among them), in the same stack, by the same levels.
fn same_blocks(was: &Prepared, now: &Prepared) -> bool {
    was.horizons == now.horizons
        && was.upper == now.upper
        && was.lower == now.lower
        && was.imported_from == now.imported_from
        && was.columns.len() == now.columns.len()
        && was.columns.iter().zip(&now.columns).all(|((ka, a), (kb, b))| {
            ka == kb && a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id && x.revision == y.revision && x.attributes == y.attributes)
        })
}

/// The first step that reads something changed between two preparings of the case, by number: 0 where the blocks are
/// read differently, `i + 1` for step `i`; none where nothing any step reads has changed. `asked` is what each built-in
/// step looked up when the case was last run to it; a step not run yet is taken to read everything.
fn first_changed(was: &Prepared, now: &Prepared, asked: &[BTreeSet<String>]) -> Option<usize> {
    let without_steps = |d: &Dataflow| Dataflow { steps: Vec::new(), ..d.clone() };
    if !same_blocks(was, now) || without_steps(&was.dataflow) != without_steps(&now.dataflow) || was.tables.len() != now.tables.len() {
        return Some(0);
    }
    let mut changed = Changed::Nothing;
    for (a, b) in was.tables.iter().zip(&now.tables) {
        if !Arc::ptr_eq(a, b) {
            changed.add(table_changed(a, b));
        }
    }
    // The tables its scripts look up by name: a change to one is read by every script step that names it.
    let lookups_changed: Vec<&str> = was
        .declared
        .lookups
        .named
        .iter()
        .zip(&now.declared.lookups.named)
        .filter(|((na, a), (nb, b))| na != nb || (!Arc::ptr_eq(a, b) && a != b))
        .map(|((name, _), _)| name.as_str())
        .collect();
    if was.declared.lookups.named.len() != now.declared.lookups.named.len() {
        return Some(0);
    }
    if input_reads(&now.dataflow).touches(&changed) {
        return Some(0);
    }
    for i in 0..was.steps.len().max(now.steps.len()) {
        let (Some((na, _, sa)), Some((nb, _, sb))) = (was.steps.get(i), now.steps.get(i)) else { return Some(i + 1) };
        if na != nb || sa != sb {
            return Some(i + 1);
        }
        let script = match &now.steps[i].1 {
            StepRun::BuiltIn(_) => None,
            StepRun::Formula(_) | StepRun::Allocation(_) => sb.script(),
        };
        let reads = match (script, asked.get(i)) {
            (Some(source), asked) => {
                if lookups_changed.iter().any(|name| source.contains(name)) {
                    return Some(i + 1);
                }
                match (script_reads(source, &now.dataflow), asked) {
                    (Reads::Some(mut names), Some(asked)) => {
                        names.extend(asked.iter().map(|n| n.to_lowercase()));
                        Reads::Some(names)
                    }
                    (reads, _) => reads,
                }
            }
            (None, Some(asked)) => Reads::Some(asked.iter().map(|n| n.to_lowercase()).collect()),
            (None, None) => Reads::All,
        };
        if reads.touches(&changed) {
            return Some(i + 1);
        }
    }
    None
}

/// The parameters a case is run with: those of the run that made the table, as Run again reads them, into no table.
fn case_params(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId) -> Result<RunReservesParams, CommandError> {
    let mut p = super::rerun_params(project, log, table, &[])?;
    p.table = None;
    p.previous = None;
    Ok(p)
}

fn prepare_case(project: &zd_core::Project, p: &RunReservesParams, unsaved: &Unsaved) -> Result<Prepared, CommandError> {
    let read = unsaved.reader(project)?;
    let get = crate::objects_own::with_calculated(project, &p.blocks, &read);
    match &unsaved.dataflow {
        Some(edited) => super::prepare_flow(&get(p.dataflow)?.name, edited, &get, p, true),
        None => super::prepare(&get, p),
    }
}

/// A reserves table's case run to a step and kept (decisions 8 and 9): every block column of its case at the step, and
/// as it stood before it. Opened on what's saved, or on edits not yet saved; nothing it does is saved.
pub struct CaseAtStep {
    table: ObjectId,
    case: String,
    params: RunReservesParams,
    prepared: Prepared,
    keys: Vec<ColumnKey>,
    /// How many steps have run: 0 for the blocks as read.
    step: usize,
    /// Each column after `step - 1` steps, to run the step open again from; none at step 0.
    before: Vec<Kept>,
    /// Each column after `step` steps.
    at: Vec<Kept>,
    /// The parameter columns each step looked up, over the case, as it last ran.
    asked: Vec<BTreeSet<String>>,
    ran_from: Option<usize>,
    ran: Vec<StepTime>,
    /// The flow up to the step, a step at a time as each ran.
    flow: Vec<FlowStep>,
    /// The flow after the step, run on from it when asked; none once anything runs again.
    flow_after: Option<Vec<FlowStep>>,
}

impl CaseAtStep {
    /// Opens a reserves table's case at step `step` (0 for its blocks as read, 1 after the first step): every column of
    /// its case run from its blocks, with the settings of the run that made the table, on what's saved or `unsaved` as
    /// edited. A table no reserves run in the log made is refused.
    pub fn open(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId, step: usize, unsaved: &Unsaved) -> Result<Self, CommandError> {
        let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
        if !matches!(object.body, Body::Table(_)) {
            return Err(CommandError::WrongKind { id: table, expected: ObjectKind::Table, found: object.kind() });
        }
        let params = case_params(project, log, table)?;
        let prepared = prepare_case(project, &params, unsaved)?;
        let case = crate::reserves_view::case_of(&object.attributes);
        let mut me = CaseAtStep {
            table,
            case,
            params,
            prepared,
            keys: Vec::new(),
            step: 0,
            before: Vec::new(),
            at: Vec::new(),
            asked: Vec::new(),
            ran_from: None,
            ran: Vec::new(),
            flow: Vec::new(),
            flow_after: None,
        };
        me.check_step(step)?;
        me.run_to(step);
        Ok(me)
    }

    /// The step's number by its name; `as input` is 0.
    pub fn step_number(&self, name: &str) -> Result<usize, CommandError> {
        if name == "as input" {
            return Ok(0);
        }
        self.prepared.steps.iter().position(|(n, _, _)| n == name).map(|i| i + 1).ok_or_else(|| {
            let names: Vec<&str> = self.prepared.steps.iter().map(|(n, _, _)| n.as_str()).collect();
            CommandError::Invalid(format!("the dataflow has no step {name}; it runs {}", names.join(", ")))
        })
    }

    pub fn step(&self) -> usize {
        self.step
    }

    fn check_step(&self, step: usize) -> Result<(), CommandError> {
        if step > self.prepared.steps.len() {
            return Err(CommandError::Invalid(format!("the dataflow runs {} steps, not {step}", self.prepared.steps.len())));
        }
        Ok(())
    }

    /// The case from its blocks to `step`, keeping each column before the last step.
    fn run_to(&mut self, step: usize) {
        let started = Instant::now();
        self.keys = self.prepared.columns.keys().filter(|k| k.case == self.case).cloned().collect();
        let prepared = &self.prepared;
        let mut states: Vec<Kept> = crate::builtin::in_parallel(&self.keys, |k| begin(prepared, k));
        self.ran = vec![StepTime { step: 0, name: "as input".into(), ms: ms(started) }];
        self.ran_from = Some(0);
        self.asked = Vec::new();
        self.before = Vec::new();
        self.flow = vec![flow_of(&self.prepared, &states, 0)];
        self.flow_after = None;
        for i in 0..step {
            states = self.run_step(i, states, i + 1 == step);
        }
        self.at = states;
        self.step = step;
    }

    /// Step `i` on every column, timed, what it looked up noted; with `keep`, the columns before it kept.
    fn run_step(&mut self, i: usize, states: Vec<Kept>, keep: bool) -> Vec<Kept> {
        let started = Instant::now();
        let prepared = &self.prepared;
        let done: Vec<(Option<Kept>, Kept, BTreeSet<String>)> = each(states, |kept| {
            let before = keep.then(|| kept.clone());
            let (after, asked) = step_column(prepared, kept, i);
            (before, after, asked)
        });
        let mut asked = BTreeSet::new();
        let mut after = Vec::with_capacity(done.len());
        let mut before = Vec::with_capacity(if keep { done.len() } else { 0 });
        for (b, a, names) in done {
            asked.extend(names);
            after.push(a);
            if let Some(b) = b {
                before.push(b);
            }
        }
        if self.asked.len() <= i {
            self.asked.resize(i + 1, BTreeSet::new());
        }
        self.asked[i] = asked;
        if keep {
            self.before = before;
        }
        self.ran.push(StepTime { step: i + 1, name: self.prepared.steps[i].0.clone(), ms: ms(started) });
        self.flow.truncate(i + 1);
        self.flow.push(flow_of(&self.prepared, &after, i + 1));
        self.flow_after = None;
        after
    }

    /// Moves the case to step `step`: on from where it is when later, else again from its blocks. What ran is added to
    /// what the last change ran.
    pub fn go_to(&mut self, step: usize) -> Result<(), CommandError> {
        self.check_step(step)?;
        if step == self.step {
            return Ok(());
        }
        if step < self.step {
            let ran = std::mem::take(&mut self.ran);
            self.run_to(step);
            self.ran.splice(0..0, ran);
            return Ok(());
        }
        let mut states = std::mem::take(&mut self.at);
        for i in self.step..step {
            states = self.run_step(i, states, i + 1 == step);
        }
        self.at = states;
        self.step = step;
        Ok(())
    }

    /// Reruns the case with what's saved now and `unsaved` as edited (decision 9): from the first step that reads a
    /// change, the steps before it reused. A change the open step reads first runs that step again from the columns kept
    /// before it; one read earlier, or a change to the blocks, the stack or the dataflow's fields, runs the case again
    /// from its blocks; one read by no step up to here runs nothing.
    pub fn rerun(&mut self, project: &zd_core::Project, log: &[crate::LogEntry], unsaved: &Unsaved) -> Result<(), CommandError> {
        let (params, prepared, first) = self.compared(project, log, unsaved)?;
        self.params = params;
        self.prepared = prepared;
        self.run_again(first);
        Ok(())
    }

    /// The case as edited, from this one (decision 9): a copy of what's kept here run again with `unsaved` from the first
    /// step that reads a change, so the first edit to a case runs only what it reaches too. This one is left as it is.
    pub fn edited(&self, project: &zd_core::Project, log: &[crate::LogEntry], unsaved: &Unsaved) -> Result<CaseAtStep, CommandError> {
        let (params, prepared, first) = self.compared(project, log, unsaved)?;
        let mut copy = CaseAtStep {
            table: self.table,
            case: self.case.clone(),
            params,
            prepared,
            keys: self.keys.clone(),
            step: self.step,
            before: self.before.clone(),
            at: self.at.clone(),
            asked: self.asked.clone(),
            ran_from: None,
            ran: Vec::new(),
            flow: self.flow.clone(),
            flow_after: self.flow_after.clone(),
        };
        copy.run_again(first);
        Ok(copy)
    }

    /// The case's parameters and dataflow prepared again on what's saved now and `unsaved`, and the first step that reads
    /// a change from what this one ran on (0 for the blocks, none for no step).
    fn compared(&self, project: &zd_core::Project, log: &[crate::LogEntry], unsaved: &Unsaved) -> Result<(RunReservesParams, Prepared, Option<usize>), CommandError> {
        let params = case_params(project, log, self.table)?;
        let prepared = prepare_case(project, &params, unsaved)?;
        let same_params = serde_json::to_value(&params).ok() == serde_json::to_value(&self.params).ok();
        let first = if same_params { first_changed(&self.prepared, &prepared, &self.asked) } else { Some(0) };
        Ok((params, prepared, first))
    }

    /// Runs again from step `first` (0 the blocks) on what's prepared now: the step open from the columns kept before
    /// it, an earlier step from the blocks, a later one not at all.
    fn run_again(&mut self, first: Option<usize>) {
        self.ran = Vec::new();
        self.ran_from = None;
        let step = self.step.min(self.prepared.steps.len());
        if first.is_some() {
            // A change any step reads: the flow after the step runs on again when next asked.
            self.flow_after = None;
        }
        match first {
            Some(k) if k <= step && k == self.step && k > 0 => {
                // The step open is the first to read it: again from the columns kept before it.
                let i = k - 1;
                let started = Instant::now();
                let prepared = &self.prepared;
                let done: Vec<(Kept, BTreeSet<String>)> = crate::builtin::in_parallel(&self.before, |kept| step_column(prepared, kept.clone(), i));
                let mut asked = BTreeSet::new();
                self.at = done.into_iter().map(|(a, names)| {
                    asked.extend(names);
                    a
                }).collect();
                self.asked[i] = asked;
                self.ran = vec![StepTime { step: k, name: self.prepared.steps[i].0.clone(), ms: ms(started) }];
                self.ran_from = Some(k);
                self.flow.truncate(k);
                self.flow.push(flow_of(&self.prepared, &self.at, k));
            }
            Some(k) if k <= step => self.run_to(step),
            _ => {}
        }
    }

    /// The whole case at the step: its units by material and what became of them, what the steps so far have written by
    /// folder, what the step said, and the balance (decision 8).
    pub fn totals(&self) -> StepTotals {
        let fields = self.prepared.fields.list();
        let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
        let values_of = |added: Vec<f64>| -> BTreeMap<String, Option<f64>> { field_names.iter().cloned().zip(added.into_iter().map(|v| v.is_finite().then_some(v))).collect() };
        let volume = self.prepared.fields.volume();

        // The units as they stand, by material and what became of them.
        let mut groups: BTreeMap<String, Vec<&[f64]>> = BTreeMap::new();
        // What's written, by folder in the order first written, and by leaf (its column and levels below) as the table
        // adds it up.
        type Leaves<'a> = BTreeMap<(&'a [String], &'a [String]), Vec<&'a [f64]>>;
        let mut folders: Vec<(String, Leaves<'_>)> = Vec::new();
        let mut stopped = Vec::new();
        let (mut volume_in, mut volume_now, mut added, mut worst) = (0.0, 0.0, 0.0, 0.0_f64);
        let mut unbalanced: Vec<String> = Vec::new();
        for kept in &self.at {
            let state = match kept {
                Ok(s) => s,
                Err(why) => {
                    stopped.push(why.clone());
                    continue;
                }
            };
            for u in state.column.units() {
                let mut group = u.in_situ.material.to_lowercase();
                if let Some(b) = &u.became {
                    let kind = format!("{:?}", b.kind).to_lowercase();
                    group = if b.detail.is_empty() { format!("{group}, {kind}") } else { format!("{group}, {kind} ({})", b.detail) };
                }
                groups.entry(group).or_default().push(u.values());
            }
            for piece in &state.rows {
                let at = match folders.iter().position(|(f, _)| *f == piece.folder) {
                    Some(at) => at,
                    None => {
                        folders.push((piece.folder.clone(), BTreeMap::new()));
                        folders.len() - 1
                    }
                };
                folders[at].1.entry((state.column.key.path.as_slice(), piece.leaf.as_slice())).or_default().push(&piece.values);
            }
            let now = state.column.balance()[volume];
            let off = state.input_volume - (now - state.added_volume);
            volume_in += state.input_volume;
            volume_now += now;
            added += state.added_volume;
            let relative = (off / state.input_volume.max(1.0)).abs();
            worst = worst.max(relative);
            if relative > BALANCE_TOLERANCE {
                unbalanced.push(format!("{} ({:+.3} m3)", state.column.key.words(), off));
            }
        }
        let unbalanced_count = unbalanced.len();
        if unbalanced_count > NAMED {
            unbalanced.truncate(NAMED);
            unbalanced.push(format!("and {} more", unbalanced_count - NAMED));
        }
        let prepared = &self.prepared;
        let units = groups
            .into_iter()
            .map(|(group, values)| {
                let tonnes = tonnes_by(prepared, None, &values).map(|d| values.iter().map(|u| u[volume] * u[d]).filter(|t| t.is_finite()).sum::<f64>() + 0.0);
                Totals { group, count: values.len(), values: values_of(add_up_values(&prepared.fields, &values)), tonnes }
            })
            .collect();
        let written = folders
            .into_iter()
            .map(|(group, leaves)| {
                let all: Vec<&[f64]> = leaves.values().flatten().copied().collect();
                // Leaf by leaf, as the table: each leaf's volume by its density, added up by their rules.
                let tonnes = tonnes_by(prepared, Some(&group), &all).map(|d| {
                    leaves.values().map(|pieces| add_up_values(&prepared.fields, pieces)).map(|a| a[volume] * a[d]).filter(|t| t.is_finite()).sum::<f64>() + 0.0
                });
                Totals { group, count: all.len(), values: values_of(add_up_values(&prepared.fields, &all)), tonnes }
            })
            .collect();

        StepTotals {
            table: self.table,
            step: self.step,
            name: if self.step == 0 { "as input".into() } else { self.prepared.steps[self.step - 1].0.clone() },
            steps: self.prepared.steps.iter().map(|(n, _, _)| n.clone()).collect(),
            columns: self.at.len() - stopped.len(),
            stopped,
            fields: fields.iter().map(|f| ColumnField { name: f.name.clone(), unit: super::unit_of_kind(f.kind), kind: f.kind, add_up: f.add_up.clone() }).collect(),
            units,
            written,
            said: self.said(),
            balance: Balance { volume_in: volume_in + 0.0, volume_now: volume_now + 0.0, added: added + 0.0, worst, balanced: unbalanced_count == 0, unbalanced },
            ran_from: self.ran_from,
            ran: self.ran.clone(),
            kept_mb: self.before.iter().chain(&self.at).filter_map(|k| k.as_ref().ok()).map(ColumnState::bytes).sum::<usize>() as f64 / 1e6,
        }
    }

    /// What the step open said over the case: each count's units and volume, each check's blocks.
    fn said(&self) -> Vec<SaidTotal> {
        let empty = Said::default();
        let mut counts: BTreeMap<String, (usize, f64)> = BTreeMap::new();
        let mut named: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, kept) in self.at.iter().enumerate() {
            let Ok(now) = kept else { continue };
            let was = match self.before.get(k) {
                Some(Ok(b)) if self.step > 0 => &b.said,
                _ => &empty,
            };
            for (what, (n, v)) in &now.said.counts {
                let (n0, v0) = was.counts.get(what).copied().unwrap_or_default();
                if *n > n0 {
                    let c = counts.entry(what.clone()).or_default();
                    c.0 += n - n0;
                    c.1 += v - v0;
                }
            }
            for (check, blocks) in &now.said.named {
                let had = was.named.get(check).map_or(0, Vec::len);
                named.entry(check.clone()).or_default().extend(blocks[had.min(blocks.len())..].iter().cloned());
            }
        }
        let mut said: Vec<SaidTotal> = counts.into_iter().map(|(what, (units, v))| SaidTotal { what, units, volume: Some(v), blocks: Vec::new() }).collect();
        said.extend(named.into_iter().filter(|(_, b)| !b.is_empty()).map(|(what, blocks)| SaidTotal { what, units: blocks.len(), volume: None, blocks: blocks.into_iter().take(NAMED).collect() }));
        said
    }

    /// What the step did to each block column, for the block map (decision 8): the volume it said it moved, the volume
    /// it wrote, and the column's coal before and after it; or why the column stopped.
    pub fn columns(&self) -> Vec<ColumnDid> {
        let volume = self.prepared.fields.volume();
        let coal = |s: &ColumnState| s.column.units().iter().filter(|u| super::is_coal(u) && u.became.is_none()).map(|u| u.value(volume)).sum::<f64>() + 0.0;
        self.keys
            .iter()
            .zip(&self.at)
            .enumerate()
            .map(|(k, (key, kept))| {
                let path = key.path.clone();
                let now = match kept {
                    Ok(now) => now,
                    Err(why) => return ColumnDid { path, stopped: Some(why.clone()), moved: 0.0, written: 0.0, coal_before: 0.0, coal_after: 0.0 },
                };
                let was = match self.before.get(k) {
                    Some(Ok(b)) if self.step > 0 => Some(b),
                    _ => None,
                };
                let moved = now.said.counts.iter().map(|(what, (_, v))| v - was.and_then(|w| w.said.counts.get(what)).map_or(0.0, |c| c.1)).sum::<f64>() + 0.0;
                let from = was.map_or(0, |w| w.rows.len()).min(now.rows.len());
                let written = now.rows[from..].iter().map(|p| p.values[volume]).filter(|v| v.is_finite()).sum::<f64>() + 0.0;
                ColumnDid { path, stopped: None, moved, written, coal_before: coal(was.unwrap_or(now)), coal_after: coal(now) }
            })
            .collect()
    }

    /// The case through every step of its dataflow, its units by material and what became of them after each (decision
    /// 10): up to the step as it ran, after it run on from the step's columns and not kept, again only once something
    /// runs again. Steps after one that stops a column leave it out, counted.
    pub fn flow(&mut self) -> Vec<FlowStep> {
        if self.flow_after.is_none() {
            let prepared = &self.prepared;
            let mut states: Vec<Kept> = self.at.clone();
            let mut after = Vec::with_capacity(prepared.steps.len().saturating_sub(self.step));
            for i in self.step..prepared.steps.len() {
                states = each(states, |kept| step_column(prepared, kept, i).0);
                after.push(flow_of(prepared, &states, i + 1));
            }
            self.flow_after = Some(after);
        }
        self.flow.iter().chain(self.flow_after.iter().flatten()).cloned().collect()
    }

    /// The step's name by its number: `as input` for 0.
    pub fn step_name(&self) -> String {
        if self.step == 0 { "as input".into() } else { self.prepared.steps[self.step - 1].0.clone() }
    }

    /// The units at the step as a table, a row a unit (Tables and scripts plan, decision 10), from what's kept.
    pub fn units(&self) -> StepUnits {
        let ran: Vec<Result<Vec<UnitView>, String>> = self
            .at
            .iter()
            .map(|kept| match kept {
                Ok(state) => Ok(super::view(&state.clone().resume(&self.prepared), "", Vec::new()).units),
                Err(why) => Err(why.clone()),
            })
            .collect();
        let keys: Vec<&ColumnKey> = self.keys.iter().collect();
        super::step_units(&self.prepared, self.step, &keys, ran)
    }

    /// The table the case would make from here: its columns run on from the step to the dataflow's last, then written as
    /// a reserves run writes a new table, table scripts and all. Nothing is kept or saved.
    pub fn table(&self, project: &zd_core::Project) -> Result<Table, CommandError> {
        let started = Instant::now();
        let prepared = &self.prepared;
        let mut states: Vec<Kept> = self.at.clone();
        for i in self.step..prepared.steps.len() {
            states = each(states, |kept| step_column(prepared, kept, i).0);
        }
        let (mut ran, mut stopped) = (Vec::new(), Vec::new());
        for (key, kept) in self.keys.iter().zip(states) {
            match kept {
                Ok(s) => {
                    let imbalance = s.input_volume - (s.column.balance()[prepared.fields.volume()] - s.added_volume);
                    let placed = if s.finalised { s.placed } else { Vec::new() };
                    ran.push(Ran { moves: s.moves, finalised: s.finalised, placed, key: key.clone(), rows: s.rows, said: s.said, imbalance, input_volume: s.input_volume });
                }
                Err(why) => stopped.push(Stopped { why }),
            }
        }
        let p = &self.params;
        let objects: Vec<&Arc<zd_core::Object>> = project.objects().collect();
        let setup = zd_core::TableSetup {
            dataflow: p.dataflow,
            add_ons: p.add_ons.clone(),
            tables: p.tables.clone(),
            blocks: zd_core::BlockPick::of_blocks(objects.iter().copied(), &p.blocks),
            model: p.model,
            stack: p.stack.clone(),
            parameters: p.parameters.clone(),
            sources: p.sources.clone(),
            input: p.input.clone(),
            row_dataflows: Vec::new(),
        };
        let mut out = super::outcome(prepared, p, ran, stopped, started, "at a step", &setup);
        let mut by_name = super::super::table_script::tables_by_name(project.objects());
        if prepared.dataflow.steps.iter().filter(|s| s.table.is_some()).filter_map(|s| s.script()).any(crate::objects_own::reads_objects) {
            by_name.push((crate::objects_own::OBJECTS_OWN.to_string(), Arc::new(crate::objects_own::objects_rows(project))));
        }
        super::super::table_script::run_table_steps(&mut out, &prepared.table_steps, &prepared.tables, &by_name, &prepared.declared.lookups);
        out.changes
            .into_iter()
            .find_map(|c| match c {
                zd_core::Change::Create { body: Body::Table(t), .. } => Some((*t).clone()),
                _ => None,
            })
            .ok_or_else(|| CommandError::Invalid(format!("no blocks of case {} ran to the end", self.case)))
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}
