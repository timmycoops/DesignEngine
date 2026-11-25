//! `reserves.run`: a reserves dataflow run over blocks (decisions 0070 and 0072, workstream 2b). A column a case, strip
//! and block, each block's horizon (and bench) a unit with its fields' values, run through the dataflow's steps in order
//! across the cores; what the steps write is a results table a case. A block that can't run (a horizon not in the stack,
//! a setting no parameter row gives) is left out with why; the rest go on.
//!
//! The steps are named and registered here, so a new one slots in by name (decision 0072, point 9), in Spry's order:
//! `small_volumes` (units under a minimum volume set aside and written, decision 0073), `initialise` (the stack order,
//! benches top down; thickness and area checked; underburden below the last coal; quality limits and defaults),
//! `in_situ` (each horizon's benches written as they are, before anything is joined) and `join_benches` (each horizon's
//! benches joined with their in situ shares, as working sections start in Spry). Each step reads its parameters for each
//! unit from the parameter tables by the attributes of the block it came from, and what it changed is counted and said. The engineering steps (working sections to clean coal) come in 0070's workstreams
//! 3 and 4.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{
    AttrValue, Attributes, Body, Change, Dataflow, DataflowStep, FieldSource, Object, ObjectId, ObjectKind, ParamValue, ParameterTable, Table, TableColumn, attribute_ci,
    attribute_word,
};
use zd_reserves::{AddUp, Amount, Became, BecameKind, Bench, Column, ColumnKey, End, FieldKind, Fields, Labels, Origin, Unit, UnitId};

use super::blocks::horizon_order;
use super::formulas;
use zd_script::Problem;
use super::measure::plan_area;
use super::in_parallel_watched;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Severity};

/// The case a block belongs to when it carries none: the workflow's own steps (decision 0066).
const BASE: &str = "Base";

/// How far a column's volume out may be from its volume in, relative, before the run says so: rounding, no more.
const BALANCE_TOLERANCE: f64 = 1e-9;

/// Blocks named in a note before the rest are counted.
const NAMED: usize = 5;

/// What a step does to a column, given its settings. Registered by name; a dataflow lists names.
type StepFn = fn(&mut ColumnRun<'_>, &DataflowStep) -> Result<(), String>;

/// A step as a run makes it: a built-in step, or a dataflow's own formula compiled once (decision 0077).
pub(crate) enum StepRun {
    BuiltIn(StepFn),
    Formula(Arc<zd_script::Formula>),
    /// An allocation step's script, compiled once (Waste allocation plan, decision 4).
    Allocation(Arc<zd_script::Allocation>),
}

/// The origin an added unit carries in place of a block: ground outside the model, the underburden (decision 0073).
pub const OUT_OF_MODEL: &str = "out of model";

/// The underburden's thickness where no parameter row gives one (m), as Spry's (decision 0073, point 4).
pub const UNDERBURDEN_THICKNESS: f64 = 10.0;

/// The steps a dataflow can name, in Spry's order, with what each does.
pub const STEPS: [(&str, &str); 10] = [
    ("small_volumes", "sets aside and writes each unit under its min_volume, never dropping it"),
    (
        "initialise",
        "puts the column in the seam model's stack order, a horizon's benches top down; checks thickness and area against max_thickness and max_area; adds underburden below the last coal where there's no waste; gives qualities their limits and defaults",
    ),
    (
        "in_situ",
        "works each unit's in situ density by Preston-Sanders from its RD, air-dried and in situ moisture, and its ROM density, then writes each horizon's benches as they are, before anything is joined",
    ),
    ("join_benches", "joins each horizon's benches into one unit with their in situ shares, so later writes put pieces back by share"),
    (
        "working_sections",
        "sets aside coal its discarded mark says (the starter's discard formula), forms working sections top down by min_parting, ws_strip_ratio and min_coal_thickness, splits them by ws_split, labels partings and thin coal, and writes each section",
    ),
    (
        "rom_loss",
        "takes each working section's roof loss to the waste above and floor loss to the waste below (roof_loss, floor_loss m), edge loss (edge_loss m x the block's length x the ply's thickness) and other loss (other_loss %) to the waste above; a section short of its loss is thin loss",
    ),
    (
        "rom_dilution",
        "cuts each waste interval into floor dilution for the coal above, roof dilution for the coal below (floor_dilution, roof_dilution m), and edge and other dilution (edge_dilution m, other_dilution %) for the coal below; a waste too thin for them all is shared out and its losses go back, said",
    ),
    ("rom_ash_cutoff", "tests each ROM coal section's diluted air-dried ash once against rom_ash_cutoff; a section over it becomes high ROM ash loss, its dilution goes back to waste and its losses come back"),
    ("rom", "nothing built in: the stage the ROM formulas follow (the starter's rom_qualities: moistures, Mad alignment, negative qualities, ROM densities)"),
    (
        "final",
        "writes ROM coal and waste on the leaves they end on, benches back by their in situ shares, and Scheduled beside them; clean coal, at its section's average where clean_coal_average is yes; the underburden taken as out-of-model volume, less model waste below the last coal, which isn't mined; each section's quality on its coal where aggregate_sections is yes",
    ),
];

fn step_fn(name: &str) -> Option<StepFn> {
    match name {
        "small_volumes" => Some(small_volumes),
        "initialise" => Some(initialise),
        "in_situ" => Some(in_situ),
        "join_benches" => Some(join_benches),
        "working_sections" => Some(working_sections),
        "rom_loss" => Some(rom_loss),
        "rom_dilution" => Some(rom_dilution),
        "rom_ash_cutoff" => Some(rom_ash_cutoff),
        "rom" => Some(rom),
        "final" => Some(final_step),
        _ => None,
    }
}

pub struct RunReserves;

fn default_name() -> String {
    "{dataflow} {case}".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunReservesParams {
    pub dataflow: ObjectId,
    /// The block solids: each carries `strip`, `block`, `horizon`, `material` and `seam`, and `bench` with `bench_floor`
    /// where it was benched; `case` where a workflow case made it. None with `input`.
    #[serde(default)]
    pub blocks: Vec<ObjectId>,
    /// The seam model whose stack orders each column; or give `stack`.
    #[serde(default)]
    pub model: Option<ObjectId>,
    /// The horizons top down, as the seam cut names them, where no seam model is given.
    #[serde(default)]
    pub stack: Vec<String>,
    /// Layer for the results tables.
    pub layer: String,
    /// The step's own parameter rows, read before the dataflow's tables: a case changes a setting here.
    #[serde(default)]
    pub parameters: Option<ParameterTable>,
    /// Each table's name: `{dataflow}` and `{case}` filled in.
    #[serde(default = "default_name")]
    pub name: String,
    /// Run again into this reserves table, revised in place (decision 0085): the blocks of its case under `only` run, their
    /// rows replaced, every other row kept. None makes a table a case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<ObjectId>,
    /// With `table`: the start of the column paths run again, the dataflow's levels above the seam: none for every block,
    /// `[pit, strip]` a strip, `[pit, strip, block]` a block.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only: Vec<String>,
    /// With `table`: the run that wrote the table before, so a block not run again is read against its own run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<zd_core::RunId>,
    /// The add-on dataflows to run with it, in order (Table setup plan, decision 2); none given, the dataflow's own list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_ons: Option<Vec<ObjectId>>,
    /// Where a field comes from for this run, by name, in place of the dataflow's (Table setup plan, decision 3).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, FieldSource>,
    /// The parameter tables it reads, in order, in place of the dataflow's (Table setup plan, decision 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tables: Option<Vec<ObjectId>>,
    /// A table read in place of the blocks, a row a piece, its columns named by the mapping (Tables and scripts plan,
    /// decision 12): its volumes are imported, not of record, and the run says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<zd_core::TableInput>,
}

/// What a step writes (decision 0074): a piece of a unit on a leaf of the reserves table, under a folder. The leaf is
/// the values of the dataflow's levels below the column (seam and bench by default), from the block the piece came from;
/// the folder a path such as `Insitu\Coal`, its last part the material from the seam stack. Texts are words for columns
/// of their own, such as the working section's name.
#[derive(Debug, Clone)]
struct Piece {
    folder: String,
    /// The unit it was written from, or onto where a step shares material out (ROM's dilution on its plies).
    unit: UnitId,
    leaf: Vec<String>,
    values: Vec<f64>,
    texts: Vec<(String, String)>,
}

/// A column in work: the column, its horizons' names by stack place, the attributes of each input unit's block (what
/// parameter rows are matched by), the parameter tables, what its steps have written, and what they've said.
pub struct ColumnRun<'a> {
    pub column: Column,
    horizons: Vec<String>,
    attributes: Vec<Attributes>,
    tables: &'a [Arc<ParameterTable>],
    dataflow: &'a Dataflow,
    rows: Vec<Piece>,
    /// The attributes of the levels below the column, top down: what a piece's leaf is read from.
    lower: &'a [String],
    /// The attributes of the levels above the seam, top down, which make the column: what its own parameters are read by.
    upper: &'a [String],
    said: Said,
    /// Volume added from outside the model (the underburden), kept out of the column's balance to its blocks.
    added_volume: f64,
    /// The largest plan area of the column's blocks.
    block_area: f64,
    /// The parameters read, unit by unit, while a column is opened as a notebook (decision 0081); none in a run.
    reads: Option<std::cell::RefCell<Vec<ParamRead>>>,
    /// A built-in step's arithmetic, unit by unit, while a column is opened as a notebook (decision 0082); none in a run.
    workings: Option<std::cell::RefCell<Vec<Working>>>,
    /// What moved between coal and waste, for the table's balance (decision 0086).
    moves: Vec<Piece>,
    finalised: bool,
    /// What formulas placed onto other leaves of the column (Waste allocation plan, decision 5), written by `final` where
    /// the leaf has something scheduled on it.
    placed: Vec<Placement>,
}

/// A schedule field's value placed onto a leaf of the column (`unit.place_onto`, Waste allocation plan, decision 5): the
/// unit that placed it, the field, the leaf (the levels below the column, as a piece's) and the value, a joined unit's
/// shared by its benches' in situ shares.
#[derive(Debug, Clone)]
struct Placement {
    unit: UnitId,
    field: usize,
    leaf: Vec<String>,
    value: f64,
}

/// A line of a built-in step's arithmetic for a unit, as the notebook shows it (decision 0082): what was worked out or
/// tested, the numbers that went in with their names and units, what it came to, and what followed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Working {
    /// The unit it's for, by its id as the step found it.
    pub unit: UnitId,
    /// What was worked out or tested: `roof dilution`, `parting`, `ROM ash`.
    pub what: String,
    /// The numbers that went in: `roof_dilution 0.05 m × area 9,570.5 m²`, or a test, `thickness 0.4 m ≤ min_parting 1 m`.
    pub sum: String,
    /// What it came to, in `measure`; none for a test.
    pub value: Option<f64>,
    pub measure: &'static str,
    /// What followed: `a break`, `joins the section`, `sent into section 1 waste A`.
    pub then: Option<String>,
}

/// A working line before it's put on its units: what, the numbers, the value and its measure, and what followed.
struct Line {
    what: String,
    sum: String,
    value: Option<f64>,
    measure: &'static str,
    then: Option<String>,
}

impl Line {
    /// A sum that came to a value in `measure`.
    fn sum(what: &str, sum: String, value: f64, measure: &'static str) -> Self {
        Line { what: what.into(), sum, value: Some(value), measure, then: None }
    }

    /// A test, and what followed from it.
    fn test(what: &str, sum: String, then: impl Into<String>) -> Self {
        Line { what: what.into(), sum, value: None, measure: "", then: Some(then.into()) }
    }

    /// What a step did, said with its numbers, no more.
    fn note(what: &str, sum: String) -> Self {
        Line { what: what.into(), sum, value: None, measure: "", then: None }
    }

    fn then(mut self, then: impl Into<String>) -> Self {
        self.then = Some(then.into());
        self
    }
}

/// A number as a working shows it: thousands marked, to `dp` places at most, no trailing zeros (`9,570.5`, `0.05`).
fn figure(x: f64, dp: usize) -> String {
    if !x.is_finite() {
        return "none".into();
    }
    let text = format!("{:.*}", dp, x.abs());
    let (whole, part) = text.split_once('.').unwrap_or((&text, ""));
    let part = part.trim_end_matches('0');
    let mut marked = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            marked.push(',');
        }
        marked.push(c);
    }
    let sign = if x < 0.0 && (marked != "0" || !part.is_empty()) { "-" } else { "" };
    if part.is_empty() { format!("{sign}{marked}") } else { format!("{sign}{marked}.{part}") }
}

/// A volume or area to a tenth; a setting or thickness to what it holds, up to four places.
fn vol(x: f64) -> String {
    figure(x, 1)
}

fn set(x: f64) -> String {
    figure(x, 4)
}

/// What a column's steps changed, counted by what and how much, and the blocks out of range by check.
#[derive(Debug, Default, Clone)]
struct Said {
    counts: BTreeMap<String, (usize, f64)>,
    named: BTreeMap<String, Vec<String>>,
}

impl Said {
    fn count(&mut self, what: String, volume: f64) {
        let c = self.counts.entry(what).or_default();
        c.0 += 1;
        c.1 += volume;
    }

    fn name(&mut self, check: &str, block: &str) {
        let n = self.named.entry(check.to_string()).or_default();
        if !n.iter().any(|b| b == block) {
            n.push(block.to_string());
        }
    }

    /// What was said after `before`, in the run's own words: a count with its volume, or a check with the blocks it names.
    fn since(&self, before: &Said) -> Vec<String> {
        let mut lines = Vec::new();
        for (what, (n, v)) in &self.counts {
            let (n0, v0) = before.counts.get(what).copied().unwrap_or_default();
            if *n > n0 {
                let n = n - n0;
                lines.push(format!("{n} unit{} {what}, {:.1} m3", plural(n), v - v0));
            }
        }
        for (check, blocks) in &self.named {
            let had = before.named.get(check).map_or(0, Vec::len);
            if blocks.len() > had {
                lines.push(format!("{check}: {}", blocks[had..].join(", ")));
            }
        }
        lines
    }
}

/// A parameter a step read for a unit, as the notebook shows it (decision 0081): the column, what it found, and the table
/// (its place in the list read) and row that gave it; none of those where no row gives it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamRead {
    pub unit: UnitId,
    pub column: String,
    pub value: Option<f64>,
    pub text: Option<String>,
    pub table: Option<usize>,
    pub row: Option<usize>,
}

impl ColumnRun<'_> {
    /// A parameter for the unit at `at`, matched by the attributes of the block it came from (its top bench's, once
    /// joined, as Spry's).
    fn parameter(&self, at: usize, column: &str) -> Option<f64> {
        self.lookup(at, column).and_then(|v| match v {
            ParamValue::Number(n) => Some(n),
            ParamValue::Text(t) => t.trim().parse().ok(),
        })
    }

    /// A parameter as words, for a setting such as `ws_split` or a yes or no.
    fn parameter_word(&self, at: usize, column: &str) -> Option<String> {
        self.lookup(at, column).map(|v| match v {
            ParamValue::Number(n) => n.to_string(),
            ParamValue::Text(t) => t.trim().to_string(),
        })
    }

    /// Every parameter a step reads comes through here, so the notebook can say where each came from.
    fn lookup(&self, at: usize, column: &str) -> Option<ParamValue> {
        let refs: Vec<&ParameterTable> = self.tables.iter().map(|t| t.as_ref()).collect();
        let found = ParameterTable::lookup_in(&refs, &self.attributes[self.column.units()[at].origin as usize], column);
        self.note_read(self.column.units()[at].id, column, found.as_ref());
        found.map(|f| f.value.clone())
    }

    /// A parameter an allocation step read by the column's own attributes (its levels above the seam), traced in the
    /// notebook as a unit's are, on the unit given (the column's top one).
    fn lookup_for_column(&self, unit: Option<UnitId>, attributes: &Attributes, column: &str) {
        let Some(unit) = unit else { return };
        let refs: Vec<&ParameterTable> = self.tables.iter().map(|t| t.as_ref()).collect();
        let found = ParameterTable::lookup_in(&refs, attributes, column);
        self.note_read(unit, column, found.as_ref());
    }

    /// Notes a parameter read, when the column is opened as a notebook: what it found, and the table and row.
    fn note_read(&self, unit: UnitId, column: &str, found: Option<&zd_core::Found<'_>>) {
        if let Some(reads) = &self.reads {
            let (value, text) = match found.as_ref().map(|f| f.value) {
                Some(ParamValue::Number(n)) => (Some(*n), None),
                Some(ParamValue::Text(t)) => (t.trim().parse().ok(), Some(t.trim().to_string())),
                None => (None, None),
            };
            let read = ParamRead { unit, column: column.to_string(), value, text, table: found.map(|f| f.table), row: found.map(|f| f.row) };
            let mut reads = reads.borrow_mut();
            if !reads.contains(&read) {
                reads.push(read);
            }
        }
    }

    /// Writes a working line on each of these units, when the column is opened as a notebook; in a run nothing is made.
    fn work(&self, units: &[UnitId], line: impl FnOnce() -> Line) {
        let Some(workings) = &self.workings else { return };
        let line = line();
        let mut workings = workings.borrow_mut();
        for &unit in units {
            workings.push(Working { unit, what: line.what.clone(), sum: line.sum.clone(), value: line.value, measure: line.measure, then: line.then.clone() });
        }
    }

    /// The ids of units by their places now.
    fn ids(&self, at: &[usize]) -> Vec<UnitId> {
        at.iter().map(|&i| self.column.units()[i].id).collect()
    }

    /// A unit's words in a working: its horizon and material, and bench (`A coal`, `OB waste, bench 2`).
    fn words_of(&self, at: usize) -> String {
        let u = &self.column.units()[at];
        format!("{} {}{}", self.horizons[u.stack as usize], u.in_situ.material, u.bench.as_ref().map_or(String::new(), |b| format!(", bench {}", b.label)))
    }

    fn name_of(&self, at: usize) -> String {
        self.column.origins()[self.column.units()[at].origin as usize].name.clone()
    }

    fn is_out_of_model(&self, u: &Unit) -> bool {
        self.column.origins()[u.origin as usize].object == OUT_OF_MODEL
    }
}

fn is_coal(u: &Unit) -> bool {
    u.in_situ.material.eq_ignore_ascii_case("coal")
}

/// A block left out, and why.
struct Stopped {
    why: String,
}

struct Ran {
    /// What moved between coal and waste, and in from or out of the model, a piece a unit on its own leaves (decision
    /// 0086): `to_waste`, `to_coal`, `from_outside`, `not_mined` as the folder. Made by `final`.
    moves: Vec<Piece>,
    /// Whether `final` ran, so ROM and Scheduled were written and the balance can be read.
    finalised: bool,
    /// Schedule fields placed onto other leaves (Waste allocation plan, decision 5), each where `final` found its leaf.
    placed: Vec<Placement>,
    key: ColumnKey,
    rows: Vec<Piece>,
    said: Said,
    /// Inputs' volume less the column's balance at the end: what no step should change.
    imbalance: f64,
    input_volume: f64,
}

/// What a dataflow run reads, settled once before the columns run.
pub(crate) struct Prepared {
    name: String,
    /// The worked examples of a dataflow being edited that don't hold, said: it runs anyway (decision 0081).
    failing: Vec<String>,
    dataflow: Arc<Dataflow>,
    fields: Arc<Fields>,
    tables: Vec<Arc<ParameterTable>>,
    horizons: Vec<String>,
    steps: Vec<(String, StepRun, DataflowStep)>,
    /// Its table script steps, run on each table the run makes once its columns are done (decision 11 of 0099).
    table_steps: Vec<(String, Arc<zd_script::TableScript>)>,
    /// What the dataflow's formulas may use.
    declared: zd_script::Declared,
    /// The attributes of the levels above the seam, which make a column, and of those below it, which make a leaf.
    upper: Vec<String>,
    lower: Vec<String>,
    /// Blocks by case and their values of the upper levels.
    columns: BTreeMap<ColumnKey, Vec<Arc<Object>>>,
    /// Blocks missing a level that has no default, by name, with the level.
    unplaced: Vec<(String, String)>,
    /// What the run read, for the table's audit (decision 0086): the dataflow and its parameter tables at their revisions.
    audit_dataflow: String,
    /// The add-ons run with the dataflow, each `name rN`, for the audit (Table setup plan, decision 2).
    audit_add_ons: Vec<String>,
    /// Which add-on each of the run's steps came from, by step name; a step of the main dataflow has none.
    step_from: BTreeMap<String, ObjectId>,
    audit_tables: String,
    /// The pieces read from a table (Tables and scripts plan, decision 12), by their ids: the volume from its column, and
    /// the thickness or plan area where mapped. Empty where the run reads block solids.
    imported: HashMap<ObjectId, ImportedPiece>,
    /// The table read and its revision, for the run's words and the audit: none for block solids.
    imported_from: Option<String>,
}

/// A row of a table read as a block piece: its volume, and its thickness or plan area where the mapping gives them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImportedPiece {
    volume: f64,
    thickness: Option<f64>,
    area: Option<f64>,
}

/// A table's rows as pieces, each piece's volume and thickness or area by its id, and the table's name and revision.
pub(crate) type TablePieces = (Vec<Arc<Object>>, HashMap<ObjectId, ImportedPiece>, String);

/// A table's rows as block pieces (decision 12 of 0099): each row's columns its attributes under their own names, the
/// mapped ones under the name the run reads; the volume from its column, a row without one refused, as is a mapping
/// naming a column the table hasn't. Each piece's id is the table's with its row, so it's the same every run.
pub(crate) fn pieces_of_table(get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, input: &zd_core::TableInput) -> Result<TablePieces, CommandError> {
    let object = get(input.table)?;
    let Body::Table(t) = &object.body else { return Err(CommandError::WrongKind { id: input.table, expected: ObjectKind::Table, found: object.kind() }) };
    let map = &input.map;
    let numbers = |what: &str, column: &str| -> Result<&Vec<Option<f64>>, CommandError> {
        match t.column(column).map(|c| &c.values) {
            Some(zd_core::ColumnValues::Number(v)) => Ok(v),
            Some(_) => Err(CommandError::Invalid(format!("{}'s {column}, mapped as the {what}, holds words, not numbers", object.name))),
            None => Err(CommandError::Invalid(format!("the mapping reads the {what} from {column}, a column {} hasn't", object.name))),
        }
    };
    if map.volume.trim().is_empty() {
        return Err(CommandError::Invalid(format!("the mapping names no volume column: say which of {}'s columns is the volume", object.name)));
    }
    let volume = numbers("volume", &map.volume)?;
    let thickness = map.thickness.as_deref().map(|c| numbers("thickness", c)).transpose()?;
    let area = map.area.as_deref().map(|c| numbers("plan area", c)).transpose()?;
    for (attribute, column) in &map.columns {
        if t.column(column).is_none() {
            return Err(CommandError::Invalid(format!("the mapping reads {attribute} from {column}, a column {} hasn't", object.name)));
        }
    }
    let value = |column: &str, row: usize| -> Option<AttrValue> {
        match t.column(column).map(|c| &c.values) {
            Some(zd_core::ColumnValues::Number(v)) => v[row].map(AttrValue::Number),
            Some(zd_core::ColumnValues::Text(v)) => v[row].clone().filter(|w| !w.trim().is_empty()).map(|w| AttrValue::Text(w.trim().to_string())),
            None => None,
        }
    };
    let mut pieces = Vec::with_capacity(t.rows());
    let mut imported = HashMap::with_capacity(t.rows());
    for row in 0..t.rows() {
        let mut attributes = Attributes::new();
        for c in &t.columns {
            if let Some(v) = value(&c.name, row) {
                attributes.insert(c.name.clone(), v);
            }
        }
        for (attribute, column) in &map.columns {
            match value(column, row) {
                Some(v) => attributes.insert(attribute.clone(), v),
                None => attributes.remove(attribute),
            };
        }
        let name = match attributes.get("name") {
            Some(AttrValue::Text(n)) => n.clone(),
            _ => format!("{} row {}", object.name, row + 1),
        };
        let v = volume[row].filter(|v| v.is_finite()).ok_or_else(|| CommandError::Invalid(format!("{name} has no volume in {}: every row of a table read as blocks gives one", map.volume)))?;
        let id = ObjectId::from_u128(input.table.as_u128().wrapping_add(((row as u128) + 1) << 64));
        imported.insert(id, ImportedPiece { volume: v, thickness: thickness.and_then(|t| t[row]), area: area.and_then(|a| a[row]) });
        pieces.push(Arc::new(Object { id, revision: object.revision, name, layer: object.layer, provenance: object.provenance, attributes, body: Body::Table(Arc::new(Table::default())) }));
    }
    Ok((pieces, imported, format!("{} r{}", object.name, object.revision)))
}

/// The horizons top down by a table's order column (decision 12): each horizon at its smallest order, smallest first.
fn horizons_by_order(pieces: &[Arc<Object>], input: &zd_core::TableInput) -> Option<Vec<String>> {
    let column = input.map.order.as_deref()?;
    let mut order: Vec<(String, f64)> = Vec::new();
    for o in pieces {
        let (Some(h), Some(AttrValue::Number(k))) = (attribute_ci(&o.attributes, "horizon").map(attribute_word), o.attributes.get(column)) else { continue };
        match order.iter_mut().find(|(n, _)| *n == h) {
            Some((_, was)) => *was = was.min(*k),
            None => order.push((h, *k)),
        }
    }
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    Some(order.into_iter().map(|(h, _)| h).collect())
}

impl Command for RunReserves {
    type Params = RunReservesParams;
    const NAME: &'static str = "reserves.run";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run a reserves dataflow over blocks: a column a pit, strip and block (the dataflow's levels above the seam), its steps in order, a reserves table a case",
            inputs: vec![
                Field::object("dataflow", ObjectKind::Dataflow, "The dataflow"),
                Field::new("blocks", FieldType::Objects, "The block solids"),
                Field::object("model", ObjectKind::SeamModel, "The seam model whose stack orders each column"),
            ],
            params: vec![
                Field::new("stack", FieldType::Texts, "The horizons top down, where no seam model is given"),
                Field::new("layer", FieldType::Text, "Layer for the results tables"),
                Field::new("parameters", FieldType::Text, "The step's own parameter rows, read first"),
                Field::new("name", FieldType::Text, "Each table's name; {dataflow} {case} by default"),
                Field::object("table", ObjectKind::Table, "Run again into this reserves table, revised in place"),
                Field::new("only", FieldType::Texts, "With table: the column paths' start run again (a block, a strip); none for all"),
            ],
            outputs: vec![Field::object("tables", ObjectKind::Table, "A results table a case")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let started = Instant::now();
        // The Objects table's calculated columns (Table setup plan, decision 9) taken as attributes the blocks haven't.
        let calculated = crate::objects_own::blocks_with_calculated(inputs.project(), &p.blocks);
        let own = if calculated.is_empty() { None } else { crate::objects_own::objects_own(inputs.project()).map(|o| inputs.object(o.id)).transpose()? };
        let mut prepared = prepare(&|id| match calculated.get(&id) {
            Some(o) => {
                inputs.object(id)?;
                Ok(o.clone())
            }
            None => inputs.object(id).cloned(),
        }, p)?;
        if let Some(o) = own {
            let was = std::mem::take(&mut prepared.audit_tables);
            prepared.audit_tables = [was, format!("{} r{} (calculated columns)", o.name, o.revision)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("; ");
        }
        // Run again into a table (decision 0085): only its case's blocks under `only`.
        let into = match p.table {
            Some(id) => {
                let object = inputs.object(id)?.clone();
                let Body::Table(_) = &object.body else { return Err(CommandError::WrongKind { id, expected: ObjectKind::Table, found: object.kind() }) };
                let case = crate::reserves_view::case_of(&object.attributes);
                prepared.columns.retain(|k, _| k.case == case && k.path.starts_with(&p.only));
                if prepared.columns.is_empty() {
                    let under = if p.only.is_empty() { String::new() } else { format!(" under {}", p.only.join(" › ")) };
                    return Err(CommandError::Invalid(format!("no blocks of case {case}{under} to run again into {}", object.name)));
                }
                Some(object)
            }
            None => None,
        };
        let keys: Vec<(&ColumnKey, &Vec<Arc<Object>>)> = prepared.columns.iter().collect();
        let done = in_parallel_watched(inputs, "blocks", &keys, |(key, blocks)| run_column(&prepared, key, blocks, None))?;
        let (mut ran, mut stopped) = (Vec::new(), Vec::new());
        for d in done {
            match d {
                Ok(r) => ran.push(r),
                Err(s) => stopped.push(s),
            }
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let when = if p.table.is_some() { format!("{}, run again", utc_words(now)) } else { utc_words(now) };
        // What fills each table it makes (Table setup plan, decision 1): kept with the table, so Run again runs it.
        let objects: Vec<&Arc<Object>> = inputs.objects().collect();
        let setup = zd_core::TableSetup { dataflow: p.dataflow, add_ons: p.add_ons.clone(), tables: p.tables.clone(), blocks: zd_core::BlockPick::of_blocks(objects.iter().copied(), &p.blocks), model: p.model, stack: p.stack.clone(), parameters: p.parameters.clone(), sources: p.sources.clone(), input: p.input.clone(), row_dataflows: Vec::new() };
        let mut out = outcome(&prepared, p, ran, stopped, started, &when, &setup);
        // Read from a table (decision 12 of 0099): its volumes are imported, not of record, and the run says so.
        if let Some(from) = &prepared.imported_from {
            out.notes.push(crate::Note::warning(crate::Code::ImportedVolumes, format!("volumes imported from {from}, not measured from solids: they're not the volumes of record")));
        }
        if let Some(object) = into {
            into_table(&mut out, inputs.project(), &object, &prepared, p)?;
        }
        // The table script steps, on the whole table each time: one run again is worked on every row (decision 11).
        // `table("Objects")` in a table step reads the Objects table, worked out only where a step names it (0103, 10).
        let mut by_name = super::table_script::tables_by_name(inputs.objects());
        if prepared.dataflow.steps.iter().filter(|s| s.table.is_some()).filter_map(|s| s.script()).any(crate::objects_own::reads_objects) {
            by_name.push((crate::objects_own::OBJECTS_OWN.to_string(), Arc::new(crate::objects_own::objects_rows(inputs.project()))));
        }
        super::table_script::run_table_steps(&mut out, &prepared.table_steps, &prepared.tables, &by_name, &prepared.declared.lookups);
        Ok(out)
    }
}

/// Reads the dataflow, its tables and the stack, checks its steps, and groups the blocks into columns.
pub(crate) fn prepare(get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, p: &RunReservesParams) -> Result<Prepared, CommandError> {
    let flow = get(p.dataflow)?;
    let Body::Dataflow(dataflow) = &flow.body else {
        return Err(CommandError::WrongKind { id: p.dataflow, expected: ObjectKind::Dataflow, found: flow.kind() });
    };
    let mut prepared = prepare_flow(&flow.name, dataflow, get, p, false)?;
    prepared.audit_dataflow = std::iter::once(format!("{} r{}", flow.name, flow.revision)).chain(prepared.audit_add_ons.iter().cloned()).collect::<Vec<_>>().join(" + ");
    Ok(prepared)
}

/// As `prepare`, for a dataflow as given: one being edited, not yet saved (decision 0077, 7d).
///
/// With `flagging`, a dataflow being edited: a formula's worked example that fails is kept in `failing`, not refused.
pub(crate) fn prepare_flow(name: &str, dataflow: &Arc<Dataflow>, get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, p: &RunReservesParams, flagging: bool) -> Result<Prepared, CommandError> {
    if dataflow.add_on {
        return Err(CommandError::Invalid(format!("{name} is an add-on: it runs with a main dataflow that lists it, or a table whose setup does")));
    }
    if dataflow.rows {
        return Err(CommandError::Invalid(format!("{name} is a row dataflow: it runs on a case's table rows after its reserves dataflow (table.run_dataflow), not on blocks")));
    }
    // Its add-ons put in at their stage (Table setup plan, decision 2): the run's own list, else the dataflow's.
    let add_on_ids = p.add_ons.clone().unwrap_or_else(|| dataflow.add_ons.clone());
    // A table's own parameter tables (Table setup plan, decision 4), in place of the dataflow's; its add-ons' after.
    let dataflow: Arc<Dataflow> = match &p.tables {
        Some(tables) => Arc::new(Dataflow { parameters: tables.clone(), ..(**dataflow).clone() }),
        None => dataflow.clone(),
    };
    let dataflow = &dataflow;
    let mut audit_add_ons = Vec::new();
    let mut step_from = BTreeMap::new();
    let dataflow: Arc<Dataflow> = if add_on_ids.is_empty() {
        dataflow.clone()
    } else {
        let mut add_ons: Vec<(String, Arc<Dataflow>)> = Vec::new();
        for &id in &add_on_ids {
            let object = get(id)?;
            let Body::Dataflow(a) = &object.body else {
                return Err(CommandError::WrongKind { id, expected: ObjectKind::Dataflow, found: object.kind() });
            };
            audit_add_ons.push(format!("{} r{}", object.name, object.revision));
            for s in &a.steps {
                step_from.insert(s.step.clone(), id);
            }
            add_ons.push((object.name.clone(), a.clone()));
        }
        let pairs: Vec<(&str, &Dataflow)> = add_ons.iter().map(|(n, d)| (n.as_str(), d.as_ref())).collect();
        Arc::new(dataflow.with_add_ons(&pairs).map_err(CommandError::Invalid)?)
    };
    // A table's own sources (Table setup plan, decision 3): where a field comes from, in place of the dataflow's.
    let dataflow: Arc<Dataflow> = if p.sources.is_empty() {
        dataflow
    } else {
        let mut d = (*dataflow).clone();
        for (field, source) in &p.sources {
            if let Some(f) = d.fields.iter_mut().find(|f| f.field.name == *field) {
                f.source = source.clone();
            } else if let Some(w) = d.words.iter_mut().find(|w| w.name == *field) {
                w.source = source.clone();
            } else {
                return Err(CommandError::Invalid(format!("the setup says where {field} comes from, and {name} has no field {field}")));
            }
        }
        Arc::new(d)
    };
    let dataflow = &dataflow;
    dataflow.check().map_err(CommandError::Invalid)?;
    let fields = Arc::new(dataflow.engine_fields().map_err(CommandError::Invalid)?);
    let mut tables = Vec::new();
    let mut audit_tables: Vec<String> = Vec::new();
    if let Some(own) = &p.parameters {
        own.check().map_err(|e| CommandError::Invalid(format!("the step's parameter rows: {e}")))?;
        tables.push(Arc::new(own.clone()));
        audit_tables.push("the run's own rows".into());
    }
    for &id in &dataflow.parameters {
        let object = get(id)?;
        audit_tables.push(format!("{} r{}", object.name, object.revision));
        let Body::ParameterTable(table) = &object.body else {
            return Err(CommandError::WrongKind { id, expected: ObjectKind::ParameterTable, found: object.kind() });
        };
        tables.push(table.clone());
    }
    // A table read in place of the blocks (decision 12 of 0099): its rows as pieces.
    let from_table = match &p.input {
        Some(input) => Some(pieces_of_table(get, input)?),
        None => None,
    };
    let horizons: Vec<String> = match p.model {
        None if p.stack.iter().all(|h| h.trim().is_empty()) && from_table.is_some() => {
            let input = p.input.as_ref().expect("with from_table");
            horizons_by_order(&from_table.as_ref().expect("given").0, input).unwrap_or_default()
        }
        Some(id) => {
            let object = get(id)?;
            let Body::SeamModel(model) = &object.body else {
                return Err(CommandError::WrongKind { id, expected: ObjectKind::SeamModel, found: object.kind() });
            };
            horizon_order(model).into_iter().map(|(h, _)| h).collect()
        }
        None => p.stack.iter().map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect(),
    };
    if horizons.is_empty() {
        return Err(CommandError::Invalid("no stack order: give the seam model, or the horizons top down".into()));
    }
    let built_in: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
    let table_refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
    let mut declared = formulas::declared(&fields, &table_refs);
    formulas::declare_words(&mut declared, &dataflow.words);
    declared.libraries = super::libraries_of(&|id| get(id).ok(), &dataflow.scripts).map_err(CommandError::Invalid)?;
    // The tables its scripts look up by name (Schedule quantities plan, decision 1): read by id, so the run records them.
    let named = super::lookups_by_id(&|id| get(id).ok(), &dataflow.lookups, &dataflow.lookup_names).map_err(CommandError::Invalid)?;
    for &id in &dataflow.lookups {
        let object = get(id)?;
        audit_tables.push(format!("{} r{} (looked up)", object.name, object.revision));
    }
    declared.lookups = Arc::new(zd_script::Lookups { own: tables.clone(), named });
    let mut failing: Vec<String> = Vec::new();
    let mut compiled: BTreeMap<usize, Arc<zd_script::Formula>> =
        formulas::check_formula_steps_flagging(dataflow, &built_in, &declared, flagging.then_some(&mut failing)).map_err(CommandError::Invalid)?.into_iter().map(|(i, f)| (i, Arc::new(f))).collect();
    let mut allocations: BTreeMap<usize, Arc<zd_script::Allocation>> =
        formulas::check_allocation_steps(dataflow, &built_in, &declared).map_err(CommandError::Invalid)?.into_iter().map(|(i, a)| (i, Arc::new(a))).collect();
    let mut steps = Vec::new();
    let table_steps = super::check_table_steps(dataflow, &built_in, &declared.libraries).map_err(CommandError::Invalid)?;
    for (i, s) in dataflow.steps.iter().enumerate().filter(|(_, s)| s.on && s.table.is_none()) {
        if let Some(f) = compiled.remove(&i) {
            steps.push((s.step.clone(), StepRun::Formula(f), s.clone()));
            continue;
        }
        if let Some(a) = allocations.remove(&i) {
            steps.push((s.step.clone(), StepRun::Allocation(a), s.clone()));
            continue;
        }
        let Some(f) = step_fn(&s.step) else {
            return Err(CommandError::Invalid(format!("the dataflow names a step {}, which this build doesn't have; it has {}", s.step, built_in.join(", "))));
        };
        steps.push((s.step.clone(), StepRun::BuiltIn(f), s.clone()));
    }
    let split = dataflow.column_levels();
    let upper: Vec<String> = dataflow.levels[..split].iter().map(|l| l.attribute.clone()).collect();
    let lower: Vec<String> = dataflow.levels[split..].iter().map(|l| l.attribute.clone()).collect();
    let mut columns: BTreeMap<ColumnKey, Vec<Arc<Object>>> = BTreeMap::new();
    let mut unplaced = Vec::new();
    let (blocks, imported, imported_from): (Vec<Arc<Object>>, HashMap<ObjectId, ImportedPiece>, Option<String>) = match from_table {
        Some((pieces, imported, from)) => {
            // Refused before it runs (decision 12): a piece with no material, or a horizon the stack hasn't.
            let mut missing: Vec<String> = Vec::new();
            for o in &pieces {
                let h = attribute_ci(&o.attributes, "horizon").map(attribute_word).unwrap_or_default();
                if !horizons.iter().any(|k| k.eq_ignore_ascii_case(&h)) && !missing.contains(&h) {
                    missing.push(h);
                }
                if attribute_ci(&o.attributes, "material").map(attribute_word).unwrap_or_default().is_empty() {
                    return Err(CommandError::Invalid(format!("{} has no material: map the column that says coal or waste to material", o.name)));
                }
            }
            if !missing.is_empty() {
                let named: Vec<String> = missing.iter().map(|h| if h.is_empty() { "(none)".to_string() } else { h.clone() }).collect();
                return Err(CommandError::Invalid(format!("{from}'s horizons {} aren't in the stack ({}): map the horizon column, or give the stack", named.join(", "), horizons.join(", "))));
            }
            (pieces, imported, Some(from))
        }
        None => {
            let mut blocks = Vec::with_capacity(p.blocks.len());
            for &id in &p.blocks {
                let object = get(id)?;
                if !matches!(object.body, Body::Solid(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::Solid, found: object.kind() });
                }
                blocks.push(object);
            }
            (blocks, HashMap::new(), None)
        }
    };
    'blocks: for object in blocks {
        let word = |key: &str| attribute_ci(&object.attributes, key).map(attribute_word).filter(|w| !w.is_empty());
        let mut path = Vec::with_capacity(split);
        for level in &dataflow.levels[..split] {
            match word(&level.attribute).or_else(|| level.default.clone()) {
                Some(v) => path.push(v),
                None => {
                    unplaced.push((object.name.clone(), level.attribute.clone()));
                    continue 'blocks;
                }
            }
        }
        let case = word("case").unwrap_or_else(|| BASE.to_string());
        columns.entry(ColumnKey { case, path }).or_default().push(object);
    }
    let audit_tables = audit_tables.join("; ");
    let audit_dataflow = format!("{name} as edited");
    Ok(Prepared { name: name.to_string(), failing, dataflow: dataflow.clone(), fields, tables, horizons, steps, table_steps, declared, upper, lower, columns, unplaced, audit_dataflow, audit_tables, audit_add_ons, step_from, imported, imported_from })
}

/// A value from the parameter tables, read in order.
fn parameter(tables: &[Arc<ParameterTable>], attributes: &Attributes, column: &str) -> Option<f64> {
    let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
    ParameterTable::lookup_in(&refs, attributes, column).and_then(|f| match f.value {
        ParamValue::Number(n) => Some(*n),
        ParamValue::Text(t) => t.trim().parse().ok(),
    })
}

/// A unit's words as the table's words columns under `folder` (`ROM\Waste\process`), a word field each it has a word
/// for (Waste allocation plan, decision 2).
/// A word field named with a folder is a schedule field (decision 5), written as `Scheduled\<its name>` beside the unit's
/// Scheduled pieces and nowhere else.
fn word_texts(u: &Unit, folder: &str) -> Vec<(String, String)> {
    u.words
        .iter()
        .filter(|(_, w)| !w.is_empty())
        .filter_map(|(field, word)| match zd_core::is_schedule_field(field) {
            false => Some((format!("{folder}\\{field}"), word.clone())),
            true if folder.starts_with("Scheduled\\") => Some((format!("Scheduled\\{field}"), word.clone())),
            true => None,
        })
        .collect()
}

/// A block's words as it becomes a unit (Waste allocation plan, decision 2): from its attribute (else the parameter
/// tables' column of the field's name), from a parameter column, or none for one a step works out.
fn block_words(prepared: &Prepared, object: &Object) -> BTreeMap<String, String> {
    let refs: Vec<&ParameterTable> = prepared.tables.iter().map(|t| t.as_ref()).collect();
    let param = |column: &str| {
        ParameterTable::lookup_in(&refs, &object.attributes, column).map(|f| match f.value {
            ParamValue::Number(n) => format!("{n}"),
            ParamValue::Text(t) => t.trim().to_string(),
        })
    };
    prepared
        .dataflow
        .words
        .iter()
        .filter_map(|w| {
            let word = match &w.source {
                FieldSource::Attribute { attribute } => attribute_ci(&object.attributes, attribute).map(attribute_word).filter(|a| !a.is_empty()).or_else(|| param(&w.name)),
                FieldSource::Parameter { column } => param(column),
                _ => None,
            }?;
            (!word.is_empty()).then(|| (w.name.clone(), word))
        })
        .collect()
}

/// The bench a block is, from what the bench cut wrote (decision 0071): its label and floor, the ground below the levels
/// at minus infinity.
fn bench_of(object: &Object) -> Result<Option<Bench>, String> {
    let Some(value) = attribute_ci(&object.attributes, "bench") else { return Ok(None) };
    let label = attribute_word(value);
    let floor = match attribute_ci(&object.attributes, "bench_floor") {
        Some(AttrValue::Number(f)) => *f,
        _ if label.starts_with("below") => f64::NEG_INFINITY,
        _ => return Err(format!("{} has a bench, {label}, but no bench_floor to put it in order", object.name)),
    };
    Ok(Some(Bench::new(&label, floor)))
}

/// A block as a unit: its stack place, labels, bench and a value a field.
fn unit_of(prepared: &Prepared, object: &Object) -> Result<(u32, Labels, Option<Bench>, Vec<f64>), String> {
    // Its volume and plan area: the solid's own, or a table row's columns (decision 12 of 0099).
    let (volume, area, thickness) = match (&object.body, prepared.imported.get(&object.id)) {
        (Body::Solid(solid), _) => (solid.volume(), plan_area(solid.mesh()), None),
        (_, Some(piece)) => {
            let area = piece.area.or_else(|| piece.thickness.filter(|t| *t > 0.0).map(|t| piece.volume / t)).unwrap_or(f64::NAN);
            (piece.volume, area, piece.thickness)
        }
        _ => unreachable!("blocks are solids or a table's rows"),
    };
    let word = |key: &str| attribute_ci(&object.attributes, key).map(attribute_word).unwrap_or_default();
    let horizon = word("horizon");
    let stack = prepared
        .horizons
        .iter()
        .position(|h| h.eq_ignore_ascii_case(&horizon))
        .ok_or_else(|| format!("{}: its horizon '{horizon}' isn't in the stack ({})", object.name, prepared.horizons.join(", ")))?;
    let material = word("material");
    if material.is_empty() {
        return Err(format!("{} has no material", object.name));
    }
    let mut values = Vec::with_capacity(prepared.dataflow.fields.len());
    for f in &prepared.dataflow.fields {
        let summed = f.field.add_up == AddUp::Sum;
        let value = match &f.source {
            FieldSource::Volume => volume,
            FieldSource::PlanArea => area,
            FieldSource::Thickness => match thickness {
                Some(t) => t,
                None if area > 0.0 => volume / area,
                None => 0.0,
            },
            FieldSource::Attribute { attribute } => match attribute_ci(&object.attributes, attribute) {
                Some(AttrValue::Number(n)) => *n,
                _ => parameter(&prepared.tables, &object.attributes, &f.field.name).unwrap_or(f64::NAN),
            },
            // A setting no row gives is missing here; a step that needs it stops the block (0072, decision 3).
            FieldSource::Parameter { column } => parameter(&prepared.tables, &object.attributes, column).unwrap_or(f64::NAN),
            FieldSource::Worked => {
                if summed {
                    0.0
                } else {
                    f64::NAN
                }
            }
        };
        values.push(value);
    }
    // The volume of record, as the blocks carry it: none stops the block.
    if let Some(v) = prepared.dataflow.fields.iter().position(|f| f.field.kind == FieldKind::Volume)
        && !values[v].is_finite()
    {
        return Err(format!("{} has no {}: measure the blocks (solid.measure) or read the volume from the solid", object.name, prepared.dataflow.fields[v].field.name));
    }
    Ok((stack as u32, Labels::new(&material, &word("seam")), bench_of(object)?, values))
}

/// A column's units, after each step when asked: the preview, and the notebook's cells (decision 0081).
#[derive(Debug, Clone, Serialize)]
pub struct StepView {
    pub step: String,
    /// The add-on dataflow it came from (Table setup plan, decision 2), where it isn't the main dataflow's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataflow: Option<ObjectId>,
    pub units: Vec<UnitView>,
    /// What the step said of this column, in the run's words (none for the column as input).
    pub said: Vec<String>,
    pub kind: StepKind,
    /// What a built-in step does, in words; empty for a formula and the column as input.
    pub rule: String,
    /// A formula step's filter, in the bench rules' words (`material`: `coal`); none, every unit.
    pub when: BTreeMap<String, String>,
    /// A formula step's script.
    pub source: Option<String>,
    /// The units a formula step's filter took, by id, as the column stood before it; every unit for a built-in step.
    pub fits: Vec<UnitId>,
    /// The parameters a built-in step read, unit by unit, in the order read (a formula's are in its trace).
    pub reads: Vec<ParamRead>,
    /// A built-in step's arithmetic, unit by unit, in the order worked (decision 0082); none for a formula, whose lines
    /// are in its trace, or a step that writes none yet.
    pub workings: Vec<Working>,
    /// What the step wrote to the reserves table for this column, a folder each, in the order written (decision 0084);
    /// the column as input writes Imported.
    pub writes: Vec<StepWrite>,
    /// The fields of the units the step reads (decision 0084), for the Dataflow view to light in Before and After: a
    /// built-in step's as declared beside it, a formula's read off its source. None for the column as input.
    pub fields_read: Vec<String>,
}

/// The fields of the units a built-in step reads (decision 0084), declared beside the steps for the Dataflow view to
/// light; the fields a step changes are seen on the units. `join_benches` adds every field up, its shares by volume and
/// thickness; `rom_ash_cutoff` adds up the ash it tests by its own rule. A field the dataflow lacks is left out.
fn builtin_fields_read(step: &DataflowStep, fields: &Fields) -> Vec<String> {
    let list = fields.list();
    let (volume, thickness) = (list[fields.volume()].name.clone(), list[fields.thickness()].name.clone());
    let of_kind = |kind: FieldKind| list.iter().filter(move |f| f.kind == kind).map(|f| f.name.clone());
    let named = |setting: &str| fields.at(step.setting(setting)).ok().map(|i| list[i].name.clone());
    match step.step.as_str() {
        "small_volumes" => vec![volume],
        "initialise" => std::iter::once(thickness).chain(of_kind(FieldKind::Area)).collect(),
        "in_situ" => named("rd_is").into_iter().collect(),
        "join_benches" => vec![volume, thickness],
        "working_sections" => named("discarded").into_iter().chain([thickness, volume]).collect(),
        "rom_loss" | "rom_dilution" => [thickness, volume].into_iter().chain(of_kind(FieldKind::Area)).chain(named("length")).collect(),
        "rom_ash_cutoff" => named("ash").into_iter().chain([volume]).collect(),
        "final" => std::iter::once(volume).chain(of_kind(FieldKind::Quality)).collect(),
        _ => Vec::new(),
    }
}

/// What a step wrote to one folder of the reserves table for a column (decision 0084): the units its pieces were written
/// from (or onto, where a step shares material out over plies), and the volume written, its pieces added up.
#[derive(Debug, Clone, Serialize)]
pub struct StepWrite {
    /// `Insitu\Coal`, `Loss\Roof`: the folder the pieces went under, without the field.
    pub folder: String,
    pub units: Vec<UnitId>,
    pub volume: f64,
}

/// Pieces as the folders they went to, in the order first written: their units, once each, and their volume added up.
fn writes_of(pieces: &[Piece], volume: usize) -> Vec<StepWrite> {
    let mut writes: Vec<StepWrite> = Vec::new();
    for piece in pieces {
        let at = match writes.iter().position(|w| w.folder == piece.folder) {
            Some(at) => at,
            None => {
                writes.push(StepWrite { folder: piece.folder.clone(), units: Vec::new(), volume: 0.0 });
                writes.len() - 1
            }
        };
        let w = &mut writes[at];
        if !w.units.contains(&piece.unit) {
            w.units.push(piece.unit);
        }
        w.volume += piece.values[volume];
    }
    writes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Input,
    BuiltIn,
    Formula,
    /// An allocation step (Waste allocation plan, decision 4): its script on the whole column.
    Allocation,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitView {
    /// Its id in the column, kept through every step: a unit a step splits off or joins is a new id, and one it joins or
    /// takes away is gone after it. Before and after a step are matched by it.
    pub id: UnitId,
    pub horizon: String,
    pub material: String,
    pub seam: String,
    pub bench: Option<String>,
    /// For a joined unit, its benches and their shares.
    pub joined: Vec<(String, f64)>,
    pub became: Option<String>,
    /// Its working section, once formed: its number, coal or waste, and its name.
    pub section: Option<String>,
    /// Where it goes in ROM, once a step has sent it somewhere: the section's number, coal or waste, and its name.
    pub rom: Option<String>,
    /// Each field's value by name.
    pub values: BTreeMap<String, f64>,
    /// Its words by word field (Waste allocation plan, decision 2).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub words: BTreeMap<String, String>,
}

fn view(run: &ColumnRun<'_>, step: &str, said: Vec<String>) -> StepView {
    let became = |b: &Became| {
        let kind = format!("{:?}", b.kind).to_lowercase();
        if b.detail.is_empty() { kind } else { format!("{kind} ({})", b.detail) }
    };
    let fields = run.column.fields();
    let units = run
        .column
        .units()
        .iter()
        .map(|u: &Unit| UnitView {
            id: u.id,
            horizon: run.horizons[u.stack as usize].clone(),
            material: u.in_situ.material.clone(),
            seam: u.in_situ.seam.clone(),
            bench: u.bench.as_ref().map(|b| b.label.clone()),
            joined: u.joined.map(|j| run.column.joins()[j as usize].benches.iter().map(|b| (b.bench.label.clone(), b.share)).collect()).unwrap_or_default(),
            became: u.became.as_ref().map(became),
            section: u.section.as_ref().map(|s| format!("{} {} {}", s.id.map_or(String::new(), |i| i.to_string()), s.material, s.seam).trim().to_string()),
            rom: u.rom.as_ref().map(|s| format!("{} {} {}", s.id.map_or(String::new(), |i| i.to_string()), s.material, s.seam).trim().to_string()),
            values: fields.list().iter().zip(u.values()).map(|(f, v)| (f.name.clone(), *v)).collect(),
            words: u.words.clone(),
        })
        .collect();
    StepView { step: step.to_string(), dataflow: None, units, said, kind: StepKind::Input, rule: String::new(), when: BTreeMap::new(), source: None, fits: Vec::new(), reads: Vec::new(), workings: Vec::new(), writes: Vec::new(), fields_read: Vec::new() }
}

/// One column: its blocks made units, then each step in turn. With `preview`, the column after each step, with what
/// each step is, the units it took and the parameters it read: the notebook's cells (decision 0081).
fn run_column(prepared: &Prepared, key: &ColumnKey, blocks: &[Arc<Object>], mut preview: Option<&mut Vec<StepView>>) -> Result<Ran, Stopped> {
    let stop = |why: String| Stopped { why };
    let (mut run, input_volume) = begin_column(prepared, key, blocks)?;
    let volume = prepared.fields.volume();
    if let Some(p) = preview.as_deref_mut() {
        // What reading the blocks said (a volume measured before a cut) goes with the column as input, as does Imported.
        let mut input = view(&run, "as input", run.said.since(&Said::default()));
        input.writes = writes_of(&run.rows, volume);
        p.push(input);
        run.reads = Some(std::cell::RefCell::new(Vec::new()));
        run.workings = Some(std::cell::RefCell::new(Vec::new()));
    }
    for (name, step, settings) in &prepared.steps {
        let before = preview.is_some().then(|| run.said.clone());
        let written = run.rows.len();
        let fits: Vec<UnitId> = match (preview.is_some(), step) {
            (true, StepRun::Formula(_)) => formula_fits(&run, settings).into_iter().map(|at| run.column.units()[at].id).collect(),
            (true, StepRun::BuiltIn(_) | StepRun::Allocation(_)) => run.column.units().iter().map(|u| u.id).collect(),
            _ => Vec::new(),
        };
        let done = run_step(&mut run, step, settings, &prepared.declared);
        done.map_err(|why| stop(format!("{} at {name}: {why}", key.words())))?;
        if let (Some(p), Some(before)) = (preview.as_deref_mut(), before) {
            let mut cell = view(&run, name, run.said.since(&before));
            cell.dataflow = prepared.step_from.get(name).copied();
            cell.fits = fits;
            cell.reads = run.reads.as_ref().map(|r| std::mem::take(&mut *r.borrow_mut())).unwrap_or_default();
            cell.workings = run.workings.as_ref().map(|w| std::mem::take(&mut *w.borrow_mut())).unwrap_or_default();
            cell.writes = writes_of(&run.rows[written..], volume);
            match step {
                StepRun::Formula(formula) => {
                    let f = settings.formula.as_ref();
                    cell.kind = StepKind::Formula;
                    cell.fields_read = formula.fields_read();
                    cell.when = f.map(|f| f.when.clone()).unwrap_or_default();
                    cell.source = f.map(|f| f.script.clone());
                    // A formula's own reads are in its trace; what its fields' parameters gave isn't the step's.
                    cell.reads.clear();
                }
                StepRun::Allocation(_) => {
                    cell.kind = StepKind::Allocation;
                    cell.source = settings.allocation.as_ref().map(|a| a.script.clone());
                }
                StepRun::BuiltIn(_) => {
                    cell.kind = StepKind::BuiltIn;
                    cell.fields_read = builtin_fields_read(settings, &prepared.fields);
                    cell.rule =STEPS.iter().find(|(n, _)| *n == settings.step).map_or(String::new(), |(_, r)| r.to_string());
                }
            }
            p.push(cell);
        }
    }
    let imbalance = input_volume - (run.column.balance()[volume] - run.added_volume);
    let placed = if run.finalised { run.placed } else { Vec::new() };
    Ok(Ran { moves: run.moves, finalised: run.finalised, placed, key: key.clone(), rows: run.rows, said: run.said, imbalance, input_volume })
}

/// A column's blocks made units, its imported rows written: ready for its steps. Gives back its input volume too.
fn begin_column<'a>(prepared: &'a Prepared, key: &ColumnKey, blocks: &[Arc<Object>]) -> Result<(ColumnRun<'a>, f64), Stopped> {
    let stop = |why: String| Stopped { why };
    let mut column = Column::new(key.clone(), prepared.fields.clone());
    let mut attributes = Vec::with_capacity(blocks.len());
    let mut mismatched: Vec<String> = Vec::new();
    let mut block_area: f64 = 0.0;
    for object in blocks {
        let (stack, labels, bench, values) = unit_of(prepared, object).map_err(stop)?;
        // The volume the block carries against its solid's: a block measured before it was last cut is said.
        if let (Body::Solid(solid), Some(v)) = (&object.body, prepared.dataflow.fields.iter().position(|f| f.field.kind == FieldKind::Volume)) {
            let (carried, own) = (values[v], solid.volume());
            if (carried - own).abs() > 1e-6 * own.abs().max(1.0) {
                mismatched.push(object.name.clone());
            }
        }
        let origin = Origin { object: object.id.to_string(), name: object.name.clone() };
        column.push_bench_input(origin, stack, labels, bench, values).map_err(|e| stop(e.to_string()))?;
        let last = column.len() - 1;
        column.unit_mut(last).words = block_words(prepared, object);
        attributes.push(object.attributes.clone());
        if let Body::Solid(solid) = &object.body {
            block_area = block_area.max(plan_area(solid.mesh()));
        } else if let Some(piece) = prepared.imported.get(&object.id) {
            let area = piece.area.or_else(|| piece.thickness.filter(|t| *t > 0.0).map(|t| piece.volume / t)).unwrap_or(0.0);
            block_area = block_area.max(area);
        }
    }
    let volume = prepared.fields.volume();
    let input_volume = column.balance()[volume];
    let mut run = ColumnRun {
        column,
        horizons: prepared.horizons.clone(),
        attributes,
        tables: &prepared.tables,
        dataflow: &prepared.dataflow,
        rows: Vec::new(),
        lower: &prepared.lower,
        upper: &prepared.upper,
        said: Said::default(),
        added_volume: 0.0,
        block_area,
        reads: None,
        workings: None,
        moves: Vec::new(),
        finalised: false,
        placed: Vec::new(),
    };
    for name in &mismatched {
        run.said.name("whose volume isn't its solid's (measured before it was cut again?)", name);
    }
    // Imported: each block as read, before any step.
    write_pieces(&mut run, |_| Some("Imported".into()), |_| Vec::new());
    Ok((run, input_volume))
}

/// A unit's labels as a formula reads them and a formula step's filter matches them.
fn unit_labels(u: &Unit) -> [(&'static str, Option<String>); 4] {
    let became = u.became.as_ref().map(|b| serde_json::to_value(b.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default());
    [("seam", Some(u.in_situ.seam.clone())), ("material", Some(u.in_situ.material.clone())), ("bench", u.bench.as_ref().map(|b| b.label.clone())), ("became", became)]
}

/// A formula step on a column (decision 0077): the units its filter fits, top down, each with its fields, labels, block,
/// parameters and working section added up; what it sets written back. It may not set the volume, the volume of record:
/// moving material is a built-in step's work. What it prints is said with the step's name.
fn run_formula(run: &mut ColumnRun<'_>, formula: &zd_script::Formula, step: &DataflowStep, declared: &zd_script::Declared) -> Result<(), String> {
    use zd_script::ScriptValue;
    let (input, which) = formula_input(run, step, declared);
    let fields = run.column.fields().clone();
    let out = formula.run(&input, &which).map_err(|p| {
        let unit = p.unit.map(|u| format!(" on {}", run.name_of(u))).unwrap_or_default();
        format!("{}{unit}", Problem { unit: None, ..p })
    })?;
    let volume = fields.volume();
    for done in out {
        for (field, value) in done.sets {
            // A word field (Waste allocation plan, decision 2) takes words, or None to clear it.
            if run.dataflow.words.iter().any(|w| w.name == field) {
                let word = match value {
                    ScriptValue::Text(t) => Some(t),
                    ScriptValue::None => None,
                    ScriptValue::Number(x) => return Err(format!("it sets the word field {field} to a number ({x}) on {}: give it words", run.name_of(done.unit))),
                };
                run.column.unit_mut(done.unit).set_word(&field, word);
                continue;
            }
            let i = fields.at(&field).map_err(|e| e.to_string())?;
            if i == volume {
                return Err(format!("it sets {field} on {}: the volume is of record, and moving material is a built-in step's work", run.name_of(done.unit)));
            }
            let x = match value {
                ScriptValue::Number(x) => x,
                ScriptValue::None => f64::NAN,
                ScriptValue::Text(t) => return Err(format!("it sets {field} to words ({t:?}) on {}", run.name_of(done.unit))),
            };
            run.column.unit_mut(done.unit).set(i, x);
        }
        for placed in &done.placed {
            place_onto(run, done.unit, placed)?;
        }
        let v = run.column.units()[done.unit].value(volume);
        for text in done.said {
            run.said.count(format!("printed by {}: {text}", step.step), v);
        }
    }
    Ok(())
}

/// A step of the column, whatever kind: a built-in step, a formula on its units, or an allocation script on the column.
fn run_step(run: &mut ColumnRun<'_>, step: &StepRun, settings: &DataflowStep, declared: &zd_script::Declared) -> Result<(), String> {
    match step {
        StepRun::BuiltIn(f) => f(run, settings),
        StepRun::Formula(f) => run_formula(run, f, settings, declared),
        StepRun::Allocation(a) => run_allocation(run, a, settings, declared, false).map(|_| ()),
    }
}

/// Where each unit ends once `final` writes it (Waste allocation plan, decision 4), as `final` decides: a member of a ROM
/// coal section ends in ROM coal; the rest ends in ROM waste, but for small volumes, the underburden left outside the
/// model and model waste below the last coal, which aren't mined.
fn ends_of(run: &ColumnRun<'_>) -> HashMap<UnitId, zd_script::Ends> {
    let mut ends = HashMap::new();
    for s in sections_of(run).iter().filter(|s| s.coal) {
        for at in rom_members(run, s.id) {
            ends.insert(run.column.units()[at].id, zd_script::Ends::Coal);
        }
    }
    let last_coal = (0..run.column.len()).rev().find(|&i| {
        let u = &run.column.units()[i];
        is_coal(u) && !run.is_out_of_model(u)
    });
    for (i, u) in run.column.units().iter().enumerate() {
        if ends.contains_key(&u.id) || run.is_out_of_model(u) || u.became.as_ref().is_some_and(|b| b.kind == BecameKind::SmallVolume) {
            continue;
        }
        if last_coal.is_some_and(|c| i > c) && !is_coal(u) {
            continue;
        }
        ends.insert(u.id, zd_script::Ends::Waste);
    }
    ends
}

/// The column's own attributes: its levels above the seam (pit, strip, block), what its parameters `p` are read by.
fn column_attributes(run: &ColumnRun<'_>) -> Attributes {
    let mut attributes = Attributes::new();
    for (level, word) in run.upper.iter().zip(&run.column.key.path) {
        attributes.insert(level.clone(), AttrValue::Text(word.clone()));
    }
    attributes
}

/// An allocation step on a column (Waste allocation plan, decisions 1, 3 and 4): the script given the column's units,
/// where each ends, each block's attributes and parameters and the column's own; the column it leaves taken as the run's.
/// Each split is said and, in the notebook, worked on its pieces; each value placed goes to `final`; each parameter it
/// read is traced to its table and row. With `trace`, the run traced line by line, given back.
fn run_allocation(run: &mut ColumnRun<'_>, allocation: &zd_script::Allocation, step: &DataflowStep, declared: &zd_script::Declared, trace: bool) -> Result<Option<zd_script::Trace>, String> {
    let tables: Vec<&ParameterTable> = run.tables.iter().map(|t| t.as_ref()).collect();
    let column_attributes = column_attributes(run);
    let input = zd_script::AllocationInput {
        column: run.column.clone(),
        name: run.column.key.words(),
        blocks: run.attributes.iter().map(formulas::block_values).collect(),
        parameters: run.attributes.iter().map(|a| formulas::parameter_values(&tables, declared, a)).collect(),
        column_block: formulas::block_values(&column_attributes),
        column_parameters: formulas::parameter_values(&tables, declared, &column_attributes),
        ends: ends_of(run),
    };
    let (done, traced) = if trace {
        let (t, done) = allocation.trace(input);
        match done {
            Some(done) => (done, Some(t)),
            None => return Ok(Some(t)),
        }
    } else {
        (allocation.run(input).map_err(|p| p.to_string())?, None)
    };
    let v = run.column.fields().volume();
    run.column = done.column;
    for split in &done.splits {
        let made: Vec<UnitId> = split.pieces.iter().flatten().copied().collect();
        let Some(at) = made.iter().find_map(|&id| run.column.position(id)) else { continue };
        let volume: f64 = made.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
        run.said.count(format!("split by {} into {} pieces", step.step, made.len()), volume);
        run.work(&made, || {
            let pieces: Vec<String> = made.iter().filter_map(|&id| run.column.position(id)).map(|at| format!("{} m³", vol(run.column.units()[at].value(v)))).collect();
            Line::sum("split", format!("{} by {}: {}", run.words_of(at), split.how, pieces.join(" + ")), volume, "m³").then(format!("{} pieces, adding back to the unit", made.len()))
        });
    }
    for (unit, placed) in &done.placed {
        let at = run.column.position(*unit).ok_or("a unit went missing")?;
        place_onto(run, at, placed)?;
    }
    for text in &done.said {
        let column = run.column.key.words();
        run.said.name(&format!("printed by {}: {text}", step.step), &column);
    }
    if run.reads.is_some() {
        let top = run.column.units().first().map(|u| u.id);
        for (whose, name) in &done.read {
            match whose.and_then(|id| run.column.position(id)) {
                Some(at) => {
                    run.lookup(at, name);
                }
                None => run.lookup_for_column(top, &column_attributes, name),
            }
        }
    }
    Ok(traced)
}
/// A schedule field a formula placed onto another leaf of the column (Waste allocation plan, decision 5): the leaf is the
/// unit's own with the levels it named put in, each named level one below the column; a joined unit's value is shared
/// over its benches by their in situ shares, as its own pieces are. Only a field that adds up by sum moves to another leaf.
fn place_onto(run: &mut ColumnRun<'_>, at: usize, placed: &zd_script::Placed) -> Result<(), String> {
    let fields = run.column.fields();
    let field = fields.at(&placed.field).map_err(|e| e.to_string())?;
    if !fields.is_summed(field) {
        return Err(format!("it places {} onto another leaf from {}, but it doesn't add up by sum: only a sum moves to another leaf", placed.field, run.name_of(at)));
    }
    // Onto the leaves another unit of the column is written on (an allocation's `unit=`), by its benches' shares.
    if let Some(id) = placed.unit {
        let unit = run.column.units()[at].id;
        let onto = run.column.position(id).ok_or_else(|| format!("it places {} onto a unit that's gone from the column", placed.field))?;
        let u = &run.column.units()[onto];
        let shares: Vec<(u32, f64)> = match u.joined {
            Some(j) => run.column.joins()[j as usize].benches.iter().map(|b| (b.origin, b.share)).collect(),
            None => vec![(u.origin, 1.0)],
        };
        for (origin, share) in shares {
            let attributes = &run.attributes[origin as usize];
            let leaf: Vec<String> = run.lower.iter().map(|l| attribute_ci(attributes, l).map(attribute_word).unwrap_or_default()).collect();
            run.placed.push(Placement { unit, field, leaf, value: placed.value * share });
        }
        return Ok(());
    }
    let mut named: Vec<(usize, String)> = Vec::new();
    for (level, word) in &placed.onto {
        let Some(l) = run.lower.iter().position(|m| m.eq_ignore_ascii_case(level)) else {
            return Err(format!("it places {} onto {level}={word}, but {level} isn't a level below the column ({})", placed.field, run.lower.join(", ")));
        };
        named.push((l, word.clone()));
    }
    let u = &run.column.units()[at];
    let shares: Vec<(u32, f64)> = match u.joined {
        Some(j) => run.column.joins()[j as usize].benches.iter().map(|b| (b.origin, b.share)).collect(),
        None => vec![(u.origin, 1.0)],
    };
    let unit = u.id;
    for (origin, share) in shares {
        let attributes = &run.attributes[origin as usize];
        let mut leaf: Vec<String> = run.lower.iter().map(|l| attribute_ci(attributes, l).map(attribute_word).unwrap_or_default()).collect();
        for (l, word) in &named {
            leaf[*l] = word.clone();
        }
        run.placed.push(Placement { unit, field, leaf, value: placed.value * share });
    }
    Ok(())
}

/// Where `final` writes what formulas placed (Waste allocation plan, decision 5): each onto its leaf where the column has
/// something scheduled there, that leaf's own spelling kept; one onto a leaf with nothing scheduled lands nowhere and is
/// said with its block, never written.
fn land_placements(run: &mut ColumnRun<'_>, pieces: &[Piece]) {
    let scheduled: Vec<&Vec<String>> = pieces.iter().filter(|p| p.folder.starts_with("Scheduled\\")).map(|p| &p.leaf).collect();
    let same = |a: &[String], b: &[String]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.trim().eq_ignore_ascii_case(y.trim()));
    let placed = std::mem::take(&mut run.placed);
    for mut p in placed {
        if let Some(leaf) = scheduled.iter().find(|l| same(l, &p.leaf)) {
            p.leaf = (*leaf).clone();
            if let Some(at) = run.column.position(p.unit) {
                let name = run.column.fields().list()[p.field].name.clone();
                let words: Vec<String> = run.lower.iter().zip(&p.leaf).filter(|(_, w)| !w.is_empty()).map(|(l, w)| format!("{l} {w}")).collect();
                let value = p.value;
                run.work(&[p.unit], || Line::sum("placed", format!("{} {name}", run.words_of(at)), value, "").then(format!("Scheduled\\{name}, on the leaf {}", words.join(", "))));
            }
            run.placed.push(p);
        } else {
            let name = run.column.fields().list()[p.field].name.clone();
            let words: Vec<String> = run.lower.iter().zip(&p.leaf).filter(|(_, w)| !w.is_empty()).map(|(l, w)| format!("{l} {w}")).collect();
            let block = run.column.position(p.unit).map(|at| run.name_of(at)).unwrap_or_default();
            run.said.name(&format!("{name} placed onto {}, a leaf with nothing scheduled on it: not written", words.join(", ")), &block);
        }
    }
}

/// What a formula step is handed for a column: every unit top down, with its fields, labels, block, parameters, working
/// section and ROM section's coal added up, and the column's; and the units its filter fits.
fn formula_input(run: &ColumnRun<'_>, step: &DataflowStep, declared: &zd_script::Declared) -> (zd_script::ColumnInput, Vec<usize>) {
    use zd_script::{ColumnInput, ScriptValue, UnitInput, Values};
    let fields = run.column.fields().clone();
    let names: Vec<&str> = fields.list().iter().map(|f| f.name.as_str()).collect();
    let numbers = |values: &[f64]| -> Vec<(String, ScriptValue)> {
        names.iter().zip(values).map(|(n, v)| (n.to_string(), if v.is_nan() { ScriptValue::None } else { ScriptValue::Number(*v) })).collect()
    };
    let tables: Vec<&ParameterTable> = run.tables.iter().map(|t| t.as_ref()).collect();
    let blocks: Vec<Values> = run.attributes.iter().map(formulas::block_values).collect();
    let params: Vec<Values> = run.attributes.iter().map(|a| formulas::parameter_values(&tables, declared, a)).collect();
    // Each working section's units, added up by their rules.
    let mut ids: Vec<u32> = Vec::new();
    let section_of: Vec<Option<usize>> = run
        .column
        .units()
        .iter()
        .map(|u| {
            let id = u.section.as_ref()?.id?;
            Some(ids.iter().position(|&i| i == id).unwrap_or_else(|| {
                ids.push(id);
                ids.len() - 1
            }))
        })
        .collect();
    let sections: Vec<Values> = (0..ids.len())
        .map(|s| Arc::new(numbers(&run.column.add_up((0..run.column.len()).filter(|&at| section_of[at] == Some(s))))))
        .collect();
    // Each ROM coal section's coal (its plies, nothing done to them) added up, and the units in each: its coal, partings,
    // dilution and coal lost into it from a neighbour.
    let mut rom_of: Vec<Option<usize>> = vec![None; run.column.len()];
    let rom_coals: Vec<Values> = sections_of(run)
        .into_iter()
        .filter(|s| s.coal)
        .enumerate()
        .map(|(k, s)| {
            for i in rom_members(run, s.id) {
                rom_of[i] = Some(k);
            }
            Arc::new(numbers(&run.column.add_up(plies_of(run, &s))))
        })
        .collect();
    let units: Vec<UnitInput> = run
        .column
        .units()
        .iter()
        .enumerate()
        .map(|(at, u)| {
            let mut values = numbers(u.values());
            values.extend(unit_labels(u).into_iter().map(|(k, v)| (k.to_string(), v.map_or(ScriptValue::None, ScriptValue::Text))));
            values.extend(run.dataflow.words.iter().map(|w| (w.name.clone(), u.word(&w.name).map_or(ScriptValue::None, |x| ScriptValue::Text(x.to_string())))));
            UnitInput { fields: Arc::new(values), block: blocks[u.origin as usize].clone(), parameters: params[u.origin as usize].clone(), section: section_of[at], rom_coal: rom_of[at] }
        })
        .collect();
    let which = formula_fits(run, step);
    (ColumnInput { units, sections, rom_coals, column: Arc::new(numbers(&run.column.balance())) }, which)
}

/// The units a formula step's filter takes, top down: matched on each unit's labels and its block's attributes.
fn formula_fits(run: &ColumnRun<'_>, step: &DataflowStep) -> Vec<usize> {
    let when = step.formula.as_ref().map(|f| f.when.clone()).unwrap_or_default();
    (0..run.column.len())
        .filter(|&at| {
            let u = &run.column.units()[at];
            let mut attributes = run.attributes[u.origin as usize].clone();
            for (k, v) in unit_labels(u) {
                if let Some(v) = v {
                    attributes.insert(k.to_string(), AttrValue::Text(v));
                }
            }
            zd_core::filter_fits(&when, &attributes)
        })
        .collect()
}

/// A line of a traced formula: its number, its text and the values it gave its names.
#[derive(Debug, Clone, Serialize)]
pub struct TraceLineView {
    pub line: usize,
    pub text: String,
    pub values: Vec<(String, String)>,
}

/// A formula step traced on one solid's unit (decision 0077, 7d): the unit as the formula found it, whether its filter
/// takes it, the lines that ran with their values, what it read and set, what it printed, and where it stopped.
#[derive(Debug, Clone, Serialize)]
pub struct FormulaTraceView {
    pub block: String,
    pub unit: String,
    pub fits: bool,
    pub before: Vec<(String, Option<f64>)>,
    pub lines: Vec<TraceLineView>,
    pub reads: Vec<(String, String)>,
    pub sets: Vec<(String, String)>,
    pub said: Vec<String>,
    pub stopped: Option<super::formulas::FormulaProblem>,
}

/// Runs a solid's block column through a dataflow (as given, saved or not) up to its formula step `step`, then traces the
/// formula on the unit the solid is part of. Nothing is kept.
pub fn formula_trace(get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, dataflow: &Arc<Dataflow>, p: &RunReservesParams, step: &str, solid: ObjectId) -> Result<FormulaTraceView, CommandError> {
    let prepared = prepare_flow("dataflow", dataflow, get, p, true)?;
    let (key, blocks) = prepared.columns.iter().find(|(_, blocks)| blocks.iter().any(|b| b.id == solid)).ok_or_else(|| CommandError::Invalid("the picked solid isn't one of the blocks run".into()))?;
    let (mut run, _) = begin_column(&prepared, key, blocks).map_err(|s| CommandError::Invalid(s.why))?;
    let mut traced = None;
    for (name, s, settings) in &prepared.steps {
        if name == step {
            match s {
                StepRun::Formula(f) => traced = Some((f.clone(), settings.clone())),
                StepRun::Allocation(a) => return allocation_trace(&mut run, &prepared, a, settings),
                StepRun::BuiltIn(_) => return Err(CommandError::Invalid(format!("{step} isn't a formula step"))),
            }
            break;
        }
        let done = run_step(&mut run, s, settings, &prepared.declared);
        done.map_err(|why| CommandError::Invalid(format!("{} at {name}: {why}", key.words())))?;
    }
    let (formula, settings) = traced.ok_or_else(|| CommandError::Invalid(format!("{step} isn't a step the dataflow runs (is it off?)")))?;
    let id = solid.to_string();
    let origins = run.column.origins();
    let of_solid = |u: &Unit| {
        origins[u.origin as usize].object == id || u.joined.is_some_and(|j| run.column.joins()[j as usize].benches.iter().any(|b| origins[b.origin as usize].object == id))
    };
    let at = (0..run.column.len()).find(|&i| of_solid(&run.column.units()[i]) && run.column.units()[i].became.is_none()).or_else(|| (0..run.column.len()).find(|&i| of_solid(&run.column.units()[i])));
    let at = at.ok_or_else(|| CommandError::Invalid("the picked solid has no unit at this step".into()))?;
    Ok(trace_at(&run, &prepared, &formula, &settings, at))
}

/// A formula traced on the unit at `at` of a column brought up to its step: the unit as the formula finds it, whether
/// its filter takes it, the lines that ran with their values, what it read and set, what it printed, where it stopped.
fn trace_at(run: &ColumnRun<'_>, prepared: &Prepared, formula: &zd_script::Formula, settings: &DataflowStep, at: usize) -> FormulaTraceView {
    let (input, which) = formula_input(run, settings, &prepared.declared);
    let u = &run.column.units()[at];
    let unit = format!("{} {}{}", u.in_situ.seam, u.in_situ.material, u.bench.as_ref().map_or(String::new(), |b| format!(", bench {}", b.label)));
    let before = run.column.fields().list().iter().zip(u.values()).map(|(f, v)| (f.name.clone(), v.is_finite().then_some(*v))).collect();
    let trace = formula.trace(&input, at);
    let show = |v: &zd_script::ScriptValue| v.to_string();
    FormulaTraceView {
        block: run.name_of(at),
        unit,
        fits: which.contains(&at),
        before,
        lines: trace.lines.into_iter().map(|l| TraceLineView { line: l.line, text: l.text, values: l.values }).collect(),
        reads: trace.events.iter().filter_map(|e| match e { zd_script::Event::Read { what, value } => Some((what.clone(), show(value))), _ => None }).collect(),
        sets: trace.sets.iter().map(|(k, v)| (k.clone(), show(v))).collect(),
        said: trace.said,
        stopped: trace.stopped.map(|p| super::formulas::FormulaProblem { message: p.message, line: p.line }),
    }
}

/// An allocation step traced on a column brought up to it (Waste allocation plan, decision 4): the whole column's run, line
/// by line, as the notebook shows a formula's on a unit; what it read, set and printed, and where it stopped.
fn allocation_trace(run: &mut ColumnRun<'_>, prepared: &Prepared, allocation: &zd_script::Allocation, settings: &DataflowStep) -> Result<FormulaTraceView, CommandError> {
    let block = run.column.key.words();
    let units = run.column.len();
    let trace = run_allocation(run, allocation, settings, &prepared.declared, true).map_err(CommandError::Invalid)?.expect("traced");
    let show = |v: &zd_script::ScriptValue| v.to_string();
    Ok(FormulaTraceView {
        block,
        unit: format!("the column, {units} units"),
        fits: true,
        before: Vec::new(),
        lines: trace.lines.into_iter().map(|l| TraceLineView { line: l.line, text: l.text, values: l.values }).collect(),
        reads: trace.events.iter().filter_map(|e| match e { zd_script::Event::Read { what, value } => Some((what.clone(), show(value))), _ => None }).collect(),
        sets: trace.sets.iter().map(|(k, v)| (k.clone(), show(v))).collect(),
        said: trace.said,
        stopped: trace.stopped.map(|p| super::formulas::FormulaProblem { message: p.message, line: p.line }),
    })
}

/// The column after each of the dataflow's steps, for one block: the canvas's before and after (decision 0072, point 7).
pub fn reserves_preview(get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, p: &RunReservesParams, case: &str, path: &[String]) -> Result<Vec<StepView>, CommandError> {
    let prepared = prepare(get, p)?;
    let key = ColumnKey { case: case.to_string(), path: path.to_vec() };
    let blocks = prepared.columns.get(&key).ok_or_else(|| CommandError::Invalid(format!("no blocks of case {case} at {}", key.words())))?;
    let mut views = Vec::new();
    run_column(&prepared, &key, blocks, Some(&mut views)).map_err(|s| CommandError::Invalid(s.why))?;
    Ok(views)
}

/// A reserves table's block column, step by step (decision 0075, 5b, Open its column): the column the node sits in, run
/// through its dataflow again with the settings of the run that made the table, every unit after each step.
#[derive(Debug, Clone, Serialize)]
pub struct ColumnSteps {
    /// The column's words, its levels above the seam (`Pit`, `3`, `B1`).
    pub column: Vec<String>,
    pub case: String,
    pub dataflow: String,
    /// The saved dataflow's id: where its formulas are edited.
    pub dataflow_id: ObjectId,
    /// The dataflow's fields in its order, each with its unit and kind.
    pub fields: Vec<ColumnField>,
    /// The parameter tables read, in order, by name: what a read's `table` counts in.
    pub tables: Vec<String>,
    /// Each of those tables' ids, where it's an object of the project: where a setting read is edited (decision 0082,
    /// point 7). None for the run's own rows and a table since deleted.
    pub table_ids: Vec<Option<ObjectId>>,
    /// The column's block solids by name, as the run read them: what the column as input is made of (decision 0084).
    pub blocks: Vec<String>,
    /// The seam model whose stack orders the column, by name; none where the run gave its own stack of horizons.
    pub model: Option<String>,
    /// How many horizons the stack holds, the model's or the run's own.
    pub horizons: usize,
    /// Whether the column was run on anything given as edited, not as saved (decisions 0081, point 7, and 0082).
    pub edited: bool,
    /// What it was run on as edited, by name: the dataflow, and the parameter tables it reads (decision 0082, point 5).
    pub unsaved: Vec<String>,
    /// The edited dataflow's worked examples that don't hold, each with its step and test: run anyway, said here.
    pub failing: Vec<String>,
    pub steps: Vec<StepView>,
    /// What the table was made from that has changed since, by name: the column is run on them as they are now, so it
    /// may not be the table's.
    pub changed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnField {
    pub name: String,
    pub unit: &'static str,
    pub kind: FieldKind,
}

/// What a column is run on that isn't saved (decision 0082, points 4 and 5): the dataflow as edited, and parameter tables
/// as edited, each standing in for the saved one with its id. Nothing is kept: the run reads them, the project doesn't.
#[derive(Debug, Clone, Default)]
pub struct Unsaved {
    pub dataflow: Option<Arc<Dataflow>>,
    pub tables: Vec<(ObjectId, Arc<ParameterTable>)>,
}

impl Unsaved {
    /// Only the dataflow as edited.
    pub fn flow(dataflow: Arc<Dataflow>) -> Self {
        Self { dataflow: Some(dataflow), tables: Vec::new() }
    }

    /// The project's objects, an edited parameter table in place of its saved one; each edited table checked first.
    fn reader<'a>(&'a self, project: &'a zd_core::Project) -> Result<impl Fn(ObjectId) -> Result<Arc<Object>, CommandError> + 'a, CommandError> {
        for (id, table) in &self.tables {
            let name = project.object(*id).map_or_else(|| "a parameter table".to_string(), |o| o.name.clone());
            table.check().map_err(|e| CommandError::Invalid(format!("{name} as edited: {e}")))?;
        }
        Ok(move |id: ObjectId| {
            let object = project.object(id).cloned().ok_or(CommandError::MissingObject(id))?;
            Ok(match self.tables.iter().find(|(t, _)| *t == id) {
                Some((_, table)) if matches!(object.body, Body::ParameterTable(_)) => {
                    let mut edited = (*object).clone();
                    edited.body = Body::ParameterTable(table.clone());
                    Arc::new(edited)
                }
                _ => object,
            })
        })
    }
}

/// Runs the column a node of a reserves table sits in (a block, or a seam or bench of it) through the dataflow again,
/// with the parameters the table's own run was given, read from the run log; or through `dataflow` as given, one being
/// edited (decision 0081). Nothing is kept. A node above a column (a pit, a strip) is refused, as is a table no reserves
/// run in the log made.
pub fn table_column(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId, node: &[String], unsaved: &Unsaved) -> Result<ColumnSteps, CommandError> {
    let get = unsaved.reader(project)?;
    let opened = open_column(project, log, &get, table, node, unsaved.dataflow.as_ref())?;
    let blocks = &opened.prepared.columns[&opened.key];
    let mut steps = Vec::new();
    run_column(&opened.prepared, &opened.key, blocks, Some(&mut steps)).map_err(|s| CommandError::Invalid(s.why))?;
    let fields = opened.prepared.fields.list().iter().map(|f| ColumnField { name: f.name.clone(), unit: unit_of_kind(f.kind), kind: f.kind }).collect();
    Ok(ColumnSteps {
        column: opened.key.path.clone(),
        case: opened.key.case.clone(),
        dataflow: opened.prepared.name.clone(),
        dataflow_id: opened.dataflow_id,
        fields,
        tables: opened.tables,
        table_ids: opened.table_ids,
        blocks: blocks.iter().map(|b| b.name.clone()).collect(),
        model: opened.model,
        horizons: opened.prepared.horizons.len(),
        edited: !opened.unsaved.is_empty(),
        unsaved: opened.unsaved,
        failing: opened.prepared.failing.clone(),
        steps,
        changed: opened.changed,
    })
}

/// A formula step of a table's column traced on one unit (decision 0081): the column run up to `step` (its place in
/// the notebook, 1 being the first step after the column as input), then the formula traced on the unit with id `unit`
/// as it stands there. On the saved dataflow, or `dataflow` as edited. Nothing is kept.
pub fn column_trace(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId, node: &[String], step: usize, unit: UnitId, unsaved: &Unsaved) -> Result<FormulaTraceView, CommandError> {
    let get = unsaved.reader(project)?;
    let opened = open_column(project, log, &get, table, node, unsaved.dataflow.as_ref())?;
    let prepared = &opened.prepared;
    let (name, s, settings) = step.checked_sub(1).and_then(|i| prepared.steps.get(i)).ok_or_else(|| CommandError::Invalid(format!("the dataflow runs {} steps, not {step}", prepared.steps.len())))?;
    if matches!(s, StepRun::BuiltIn(_)) {
        return Err(CommandError::Invalid(format!("{name} is built in, not a formula: it has no lines to trace")));
    }
    let (mut run, _) = begin_column(prepared, &opened.key, &prepared.columns[&opened.key]).map_err(|s| CommandError::Invalid(s.why))?;
    for (name, s, settings) in &prepared.steps[..step - 1] {
        let done = run_step(&mut run, s, settings, &prepared.declared);
        done.map_err(|why| CommandError::Invalid(format!("{} at {name}: {why}", opened.key.words())))?;
    }
    let formula = match s {
        StepRun::Formula(f) => f,
        StepRun::Allocation(a) => return allocation_trace(&mut run, prepared, a, settings),
        StepRun::BuiltIn(_) => unreachable!("refused above"),
    };
    let at = run.column.position(unit).ok_or_else(|| CommandError::Invalid(format!("the column has no unit {unit} at {name}")))?;
    let mut view = trace_at(&run, prepared, formula, settings, at);
    // A unit the step's filter leaves out isn't run on, so no line ran for it: the cell says so, and shows none.
    if !view.fits {
        view.lines.clear();
        view.reads.clear();
        view.sets.clear();
        view.said.clear();
        view.stopped = None;
    }
    Ok(view)
}

/// Every block of a reserves table's case at one step (Tables and scripts plan, decision 10): what the Dataflow tab shows
/// for one block, for all of them at once, as a table to group and add up.
#[derive(Debug, Clone, Serialize)]
pub struct StepUnits {
    /// The step the units are after: `as input` for none.
    pub step: String,
    /// The levels above the seam, which make a column: the table's first columns.
    pub upper: Vec<String>,
    /// Block columns run to the step.
    pub columns: usize,
    /// Columns that stopped before the step, each with why: their units aren't in the table.
    pub stopped: Vec<String>,
    /// A row a unit: its column's words, its name, its labels, then each field with its unit and how it adds up.
    #[serde(skip)]
    pub table: Table,
}

/// Every column of a reserves table's case run through its dataflow to `step` (how many steps, 0 for the columns as
/// input), with the settings of the run that made the table, read from the log as Open its column reads them; the units
/// after it as a table. Run on demand and never kept: on what's saved, or `unsaved` as edited. A column that stops on
/// the way is said, and the rest go on.
pub fn units_at_step(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId, step: usize, unsaved: &Unsaved) -> Result<StepUnits, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    if !matches!(object.body, Body::Table(_)) {
        return Err(CommandError::WrongKind { id: table, expected: ObjectKind::Table, found: object.kind() });
    }
    let mut p = rerun_params(project, log, table, &[])?;
    p.table = None;
    p.previous = None;
    let read = unsaved.reader(project)?;
    let get = crate::objects_own::with_calculated(project, &p.blocks, &read);
    let prepared = match &unsaved.dataflow {
        Some(edited) => prepare_flow(&get(p.dataflow)?.name, edited, &get, &p, true)?,
        None => prepare(&get, &p)?,
    };
    if step > prepared.steps.len() {
        return Err(CommandError::Invalid(format!("the dataflow runs {} steps, not {step}", prepared.steps.len())));
    }
    let case = crate::reserves_view::case_of(&object.attributes);
    let columns: Vec<(&ColumnKey, &Vec<Arc<Object>>)> = prepared.columns.iter().filter(|(k, _)| k.case == case).collect();
    let ran = super::in_parallel(&columns, |(key, blocks)| -> Result<Vec<UnitView>, String> {
        let (mut run, _) = begin_column(&prepared, key, blocks).map_err(|s| s.why)?;
        for (name, s, settings) in &prepared.steps[..step] {
            let done = run_step(&mut run, s, settings, &prepared.declared);
            done.map_err(|why| format!("{} at {name}: {why}", key.words()))?;
        }
        Ok(view(&run, "", Vec::new()).units)
    });
    let fields = prepared.fields.list();
    let upper = prepared.upper.clone();
    let words = ["seam", "horizon", "material", "bench", "became", "section", "rom"];
    let mut text: Vec<Vec<Option<String>>> = vec![Vec::new(); upper.len() + 1 + words.len()];
    let mut numbers: Vec<Vec<Option<f64>>> = vec![Vec::new(); fields.len()];
    let word_fields: Vec<String> = prepared.dataflow.words.iter().map(|w| w.name.clone()).collect();
    let mut word_values: Vec<Vec<Option<String>>> = vec![Vec::new(); word_fields.len()];
    let mut stopped = Vec::new();
    for ((key, _), units) in columns.iter().zip(ran) {
        let units = match units {
            Ok(units) => units,
            Err(why) => {
                stopped.push(why);
                continue;
            }
        };
        for u in units {
            let bench = u.bench.clone().unwrap_or_default();
            let name = [key.path.join(" "), u.horizon.clone(), bench.clone(), u.became.clone().unwrap_or_default()].into_iter().filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
            let labels = [Some(u.seam.clone()), Some(u.horizon.clone()), Some(u.material.clone()), u.bench.clone(), u.became.clone(), u.section.clone(), u.rom.clone()];
            for (i, w) in key.path.iter().enumerate() {
                text[i].push(Some(w.clone()));
            }
            text[upper.len()].push(Some(name));
            for (k, w) in labels.into_iter().enumerate() {
                text[upper.len() + 1 + k].push(w.filter(|w| !w.is_empty()));
            }
            for (f, column) in fields.iter().zip(numbers.iter_mut()) {
                column.push(u.values.get(&f.name).copied().filter(|v| v.is_finite()));
            }
            for (w, column) in word_fields.iter().zip(word_values.iter_mut()) {
                column.push(u.words.get(w).cloned().filter(|w| !w.is_empty()));
            }
        }
    }
    let names: Vec<&str> = upper.iter().map(String::as_str).chain(std::iter::once("name")).chain(words).collect();
    let mut table_columns: Vec<TableColumn> = names.iter().zip(text).map(|(n, v)| TableColumn::text(n, v)).collect();
    // Its word fields after its labels (Waste allocation plan, decision 2).
    table_columns.extend(word_fields.iter().zip(word_values).map(|(n, v)| TableColumn::text(n, v)));
    for (f, v) in fields.iter().zip(numbers) {
        // A field named as a label (a dataflow with a `seam` field) keeps the label's column; its numbers are left out.
        if names.contains(&f.name.as_str()) {
            continue;
        }
        table_columns.push(TableColumn { name: f.name.clone(), unit: unit_of_kind(f.kind).to_string(), add_up: Some(f.add_up.clone()), kind: zd_core::ColumnKind::Written, values: zd_core::ColumnValues::Number(v) });
    }
    let step_name = if step == 0 { "as input".to_string() } else { prepared.steps[step - 1].0.clone() };
    Ok(StepUnits { step: step_name, upper, columns: columns.len(), stopped, table: Table { columns: table_columns, calculated: Vec::new(), setup: None, objects: false } })
}

/// A table's column made ready to run: the dataflow prepared with the table's own run's parameters, the column's key,
/// the tables read by name, and what has changed since the table was made.
struct OpenedColumn {
    prepared: Prepared,
    dataflow_id: ObjectId,
    key: ColumnKey,
    tables: Vec<String>,
    table_ids: Vec<Option<ObjectId>>,
    /// The seam model the run named, by name.
    model: Option<String>,
    changed: Vec<String>,
    /// What it's run on as edited, by name: the dataflow, and the edited tables among those it reads.
    unsaved: Vec<String>,
}

fn open_column(project: &zd_core::Project, log: &[crate::LogEntry], get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, table: ObjectId, node: &[String], dataflow: Option<&Arc<Dataflow>>) -> Result<OpenedColumn, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::Table(t) = &object.body else {
        return Err(CommandError::WrongKind { id: table, expected: ObjectKind::Table, found: object.kind() });
    };
    let record = table_run(log, object).ok_or_else(|| CommandError::Invalid(format!("{} wasn't made by a reserves run in this project's log, so its columns can't be run again", object.name)))?;
    let p: RunReservesParams = serde_json::from_value(record.params.clone()).map_err(|e| CommandError::Invalid(format!("the run that made {} can't be read again: {e}", object.name)))?;
    // The run that last wrote this node's column (decision 0085): what it's read against for what has changed since. A
    // node's words start with its column's, so they're checked against what each run again took.
    let wrote = run_for_column(log, record.run, node).unwrap_or(record);
    let mut p: RunReservesParams = if std::ptr::eq(wrote, record) { p } else { serde_json::from_value(wrote.params.clone()).map_err(|e| CommandError::Invalid(format!("the run that made {} can't be read again: {e}", object.name)))? };
    let record = wrote;
    let mut changed: Vec<String> = Vec::new();
    let mut gone = 0;
    for input in &record.inputs {
        // The table itself, revised by a run again into it, isn't a change to what it was made from.
        if input.id == table {
            continue;
        }
        match project.object(input.id) {
            Some(o) if o.revision != input.revision => changed.push(o.name.clone()),
            None => gone += 1,
            _ => {}
        }
    }
    if gone > 0 {
        changed.push(format!("{gone} object{} deleted since", plural(gone)));
    }
    p.blocks.retain(|id| project.object(*id).is_some());
    let saved = get(p.dataflow)?;
    let calculated = crate::objects_own::with_calculated(project, &p.blocks, get);
    let get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError> = &calculated;
    let prepared = match dataflow {
        Some(edited) => prepare_flow(&saved.name, edited, get, &p, true)?,
        None => prepare(get, &p)?,
    };
    let mut tables: Vec<String> = Vec::new();
    let mut table_ids: Vec<Option<ObjectId>> = Vec::new();
    if p.parameters.is_some() {
        tables.push("the run's own rows".into());
        table_ids.push(None);
    }
    let mut unsaved: Vec<String> = dataflow.map(|_| saved.name.clone()).into_iter().collect();
    for &id in &prepared.dataflow.parameters {
        let name = project.object(id).map_or_else(|| "a table since deleted".into(), |o| o.name.clone());
        // A table read as edited is said by name: the lookup gave it in place of the saved one.
        if get(id).ok().zip(project.object(id)).is_some_and(|(read, stored)| !Arc::ptr_eq(&read, stored)) {
            unsaved.push(name.clone());
        }
        tables.push(name);
        table_ids.push(project.object(id).map(|_| id));
    }
    // The node's words by the table's levels; the column is the words of the dataflow's levels above the seam.
    let levels = crate::reserves_view::levels(t);
    let mut column = Vec::with_capacity(prepared.upper.len());
    for attribute in &prepared.upper {
        let at = levels.iter().position(|l| l.eq_ignore_ascii_case(attribute)).ok_or_else(|| CommandError::Invalid(format!("{} has no {attribute} level", object.name)))?;
        let word = node.get(at).ok_or_else(|| CommandError::Invalid(format!("pick a {} or below it: a column is one {}", prepared.upper.join(", "), prepared.upper.last().map_or("block", String::as_str))))?;
        column.push(word.clone());
    }
    let case = crate::reserves_view::case_of(&object.attributes);
    let key = ColumnKey { case: case.clone(), path: column };
    if !prepared.columns.contains_key(&key) {
        return Err(CommandError::Invalid(format!("no blocks of case {case} at {} now", key.words())));
    }
    let model = p.model.map(|id| project.object(id).map_or_else(|| "a seam model since deleted".into(), |o| o.name.clone()));
    Ok(OpenedColumn { prepared, dataflow_id: p.dataflow, key, tables, table_ids, model, changed, unsaved })
}

/// `small_volumes`: each unit under the `min_volume` its parameter rows give set aside and written (decision 0073,
/// point 1). Nothing is set aside where no row gives a minimum.
fn small_volumes(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let column = step.setting("min_volume");
    let volume = run.column.fields().volume();
    let small: Vec<usize> = (0..run.column.len())
        .filter(|&at| {
            let u = &run.column.units()[at];
            !u.is_set_aside() && run.parameter(at, column).is_some_and(|min| u.value(volume) < min)
        })
        .collect();
    for &at in &small {
        let v = run.column.units()[at].value(volume);
        run.column.label_became(at, Became { kind: BecameKind::SmallVolume, detail: String::new() });
        run.said.count("set aside as small volumes".into(), v);
    }
    write_pieces(run, |u| u.became.as_ref().is_some_and(|b| b.kind == BecameKind::SmallVolume).then(|| "Small volume".into()), |_| Vec::new());
    Ok(())
}

/// `initialise` (decision 0073, points 2 to 4): the stack order, benches top down; each unit's thickness and area
/// checked against `max_thickness` and `max_area` (said, never changed: they're the solid's); underburden below the last
/// coal where there's no waste there. Limits and defaults are the starter's `limits` formula now (decision 0077).
fn initialise(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    run.column.sort();
    check_size(run, step);
    add_underburden(run, step);
    Ok(())
}

/// Thickness and area against their maximums, the blocks beyond named by check.
fn check_size(run: &mut ColumnRun<'_>, step: &DataflowStep) {
    let fields = run.column.fields();
    let thickness = fields.thickness();
    let area = fields.list().iter().position(|f| f.kind == FieldKind::Area);
    for at in 0..run.column.len() {
        let u = &run.column.units()[at];
        if u.is_set_aside() {
            continue;
        }
        let over = |value: f64, column: &str| run.parameter(at, column).is_some_and(|max| value > max);
        let thick = over(u.value(thickness), step.setting("max_thickness"));
        let wide = area.is_some_and(|a| over(u.value(a), step.setting("max_area")));
        let name = run.name_of(at);
        if thick {
            run.said.name("thicker than max_thickness", &name);
        }
        if wide {
            run.said.name("wider than max_area", &name);
        }
    }
}

/// Where the column's last unit is coal, underburden below it (decision 0073, point 4): waste `underburden_thickness`
/// thick (10 m where no row gives one; 0 for none) over the column's largest block area, outside the model, its volume
/// kept out of the column's balance to its blocks.
fn add_underburden(run: &mut ColumnRun<'_>, step: &DataflowStep) {
    let Some(last) = (0..run.column.len()).rev().find(|&at| !run.column.units()[at].is_set_aside()) else { return };
    if !is_coal(&run.column.units()[last]) {
        return;
    }
    let thickness = run.parameter(last, step.setting("underburden_thickness")).unwrap_or(UNDERBURDEN_THICKNESS);
    if thickness <= 0.0 {
        return;
    }
    let fields = run.column.fields();
    let area = fields.list().iter().position(|f| f.kind == FieldKind::Area).map(|a| run.column.units().iter().map(|u| u.value(a)).fold(0.0, f64::max)).filter(|a| *a > 0.0).unwrap_or(run.block_area);
    let mut attributes = run.attributes[run.column.units()[last].origin as usize].clone();
    attributes.insert("material".into(), AttrValue::Text("waste".into()));
    attributes.insert("horizon".into(), AttrValue::Text("underburden".into()));
    attributes.remove("seam");
    attributes.remove("bench");
    // Its values as a waste block's would be: its size, and the rest from its parameter rows.
    let values: Vec<f64> = run
        .dataflow
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| match f.field.kind {
            FieldKind::Volume => thickness * area,
            FieldKind::Thickness => thickness,
            FieldKind::Area => area,
            _ => match &f.source {
                FieldSource::Attribute { .. } => parameter(run.tables, &attributes, &f.field.name).unwrap_or(f64::NAN),
                FieldSource::Parameter { column } => parameter(run.tables, &attributes, column).unwrap_or(f64::NAN),
                _ if fields.is_summed(i) => 0.0,
                _ => f64::NAN,
            },
        })
        .collect();
    run.attributes.push(attributes);
    run.horizons.push("underburden".into());
    let stack = (run.horizons.len() - 1) as u32;
    let origin = Origin { object: OUT_OF_MODEL.into(), name: "underburden".into() };
    let volume = thickness * area;
    run.column.push_input(origin, stack, Labels::new("waste", ""), values).expect("the underburden's values are one a field");
    run.added_volume += volume;
    run.said.count("underburden added below the last coal, out of model".into(), volume);
}

/// What the walk sees: a coal ply, or a run of waste (discarded coal with it) between plies.
#[derive(Debug, Clone)]
enum Item {
    Ply(usize),
    Waste(Vec<usize>),
}

/// `working_sections` (decision 0073, points 6 and 7), after the benches are joined, in Spry's order:
///
/// 1. coal set aside by hand (`discard` yes) or by its in situ ash over `insitu_ash_cutoff` is discarded: carried as
///    waste, and always a break;
/// 2. a run of waste between plies thicker than `min_parting` breaks the column into groups (the run's own setting,
///    else the larger of the plies' either side);
/// 3. within a group, top down, a section starts at its first ply; the next parting and the ply below it join it if that
///    ply's coal volume × `ws_strip_ratio` (the largest of the section's plies' and its own) covers the parting's volume,
///    bcm for bcm; otherwise the section closes and the next starts at that ply;
/// 4. a section counts if its thickness, partings in, exceeds `min_coal_thickness` (the largest of its plies'); the coal
///    of one that doesn't is thin loss, set aside, and its partings stay waste;
/// 5. a section is split where its plies' `ws_split` names change, the parting going with the ply above.
///
/// Sections are numbered top down with the waste between them, each coal section named by its roof and floor seams; a
/// parting in a section is parting dilution. Each section, thin ply and discarded ply is written at `sections`.
fn working_sections(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let fields = run.column.fields();
    let (thickness, volume) = (fields.thickness(), fields.volume());
    let active: Vec<usize> = (0..run.column.len()).filter(|&i| !run.column.units()[i].is_set_aside()).collect();

    // 1. Discarded coal: what the starter's `discard` formula marked (decision 0077, 7c), 1 by hand, 2 by its in situ
    // ash, any other number by a site's own formula. A dataflow with no `discarded` field discards nothing.
    if let Ok(discarded) = fields.at(step.setting("discarded")) {
        for &i in &active {
            let u = &run.column.units()[i];
            let mark = u.value(discarded);
            if !is_coal(u) || u.became.is_some() || mark.is_nan() || mark <= 0.0 {
                continue;
            }
            let detail = match mark {
                1.0 => "user specified",
                2.0 => "high in situ ash",
                _ => "by formula",
            };
            let (v, id) = (u.value(volume), u.id);
            run.column.label_became(i, Became { kind: BecameKind::Discarded, detail: detail.into() });
            run.said.count(format!("coal discarded, {detail}"), v);
            run.work(&[id], || Line::test("discarded", format!("{} {}", step.setting("discarded"), set(mark)), format!("{detail}: carried as waste, and a break")));
        }
    }

    // The plies and the waste between them, top down.
    let is_ply = |run: &ColumnRun<'_>, i: usize| {
        let u = &run.column.units()[i];
        is_coal(u) && u.became.is_none()
    };
    let mut items: Vec<Item> = Vec::new();
    for &i in &active {
        if is_ply(run, i) {
            items.push(Item::Ply(i));
        } else if let Some(Item::Waste(w)) = items.last_mut() {
            w.push(i);
        } else {
            items.push(Item::Waste(vec![i]));
        }
    }
    let need = |run: &ColumnRun<'_>, i: usize, key: &str| -> Result<f64, String> {
        run.parameter(i, step.setting(key)).ok_or_else(|| format!("{} has no {} (no parameter row gives it)", run.name_of(i), step.setting(key)))
    };
    let sum = |run: &ColumnRun<'_>, units: &[usize], f: usize| units.iter().map(|&i| run.column.units()[i].value(f)).sum::<f64>();

    // 2. Groups of plies, split at thick partings and discarded coal.
    let mut groups: Vec<Vec<Item>> = Vec::new();
    let mut current: Vec<Item> = Vec::new();
    for (k, item) in items.iter().enumerate() {
        match item {
            Item::Ply(_) => current.push(item.clone()),
            Item::Waste(w) => {
                let between = matches!(items.get(k.wrapping_sub(1)), Some(Item::Ply(_))) && matches!(items.get(k + 1), Some(Item::Ply(_)));
                let discarded = w.iter().any(|&i| run.column.units()[i].became.as_ref().is_some_and(|b| b.kind == BecameKind::Discarded));
                // The parting's thickness against its limit, and whether the limit was its own row's.
                let mut tested: Option<(f64, f64, bool)> = None;
                let breaks = !between || discarded || {
                    let own = w.iter().filter_map(|&i| run.parameter(i, step.setting("min_parting"))).fold(f64::NAN, f64::max);
                    let limit = if own.is_nan() {
                        let (Item::Ply(above), Item::Ply(below)) = (&items[k - 1], &items[k + 1]) else { unreachable!("between plies") };
                        need(run, *above, "min_parting")?.max(need(run, *below, "min_parting")?)
                    } else {
                        own
                    };
                    let t = sum(run, w, thickness);
                    tested = Some((t, limit, !own.is_nan()));
                    t > limit
                };
                if between {
                    let then = if breaks { "a break: the plies either side start new groups" } else { "may join the plies either side, by the strip ratio" };
                    run.work(&run.ids(w), || match tested {
                        None => Line::test("parting", "holds discarded coal".into(), then),
                        Some((t, limit, own)) => {
                            let whose = if own { "its own row's" } else { "the larger of the plies' either side" };
                            Line::test("parting", format!("thickness {} m {} {} {} m ({whose})", set(t), if breaks { ">" } else { "≤" }, step.setting("min_parting"), set(limit)), then)
                        }
                    });
                }
                if breaks {
                    if !current.is_empty() {
                        groups.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(item.clone());
                }
            }
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }

    // 3. Within each group, sections top down by the strip ratio; 4. kept if thick enough; 5. split by name.
    let mut sections: Vec<Vec<usize>> = Vec::new();
    let mut thin: Vec<usize> = Vec::new();
    for group in groups {
        let mut formed: Vec<(Vec<usize>, Vec<usize>)> = Vec::new(); // (units, plies)
        let mut k = 0;
        while k < group.len() {
            let Item::Ply(first) = group[k] else { unreachable!("a group starts and ends with a ply") };
            let (mut units, mut plies) = (vec![first], vec![first]);
            let mut ratio = need(run, first, "ws_strip_ratio")?;
            k += 1;
            while k + 1 < group.len() {
                let (Item::Waste(parting), Item::Ply(ply)) = (&group[k], &group[k + 1]) else { unreachable!("plies and partings alternate") };
                let threshold = ratio.max(need(run, *ply, "ws_strip_ratio")?);
                let (ply_volume, parting_volume) = (run.column.units()[*ply].value(volume), sum(run, parting, volume));
                let joins = ply_volume * threshold >= parting_volume;
                let mut on = run.ids(parting);
                on.push(run.column.units()[*ply].id);
                run.work(&on, || {
                    let ply_words = run.words_of(*ply);
                    let sum = format!(
                        "{ply_words} {} m³ × {} {} = {} m³ {} parting {} m³",
                        vol(ply_volume),
                        step.setting("ws_strip_ratio"),
                        set(threshold),
                        vol(ply_volume * threshold),
                        if joins { "≥" } else { "<" },
                        vol(parting_volume)
                    );
                    let then = if joins { format!("{ply_words} joins the section, with the parting") } else { format!("the section closes and the next starts at {ply_words}; the parting stays waste") };
                    Line::sum("strip ratio", sum, ply_volume * threshold, "m³").then(then)
                });
                if joins {
                    units.extend(parting);
                    units.push(*ply);
                    plies.push(*ply);
                    ratio = threshold;
                    k += 2;
                } else {
                    k += 1;
                    break;
                }
            }
            formed.push((units, plies));
        }
        for (units, plies) in formed {
            let mut minimum: f64 = 0.0;
            for &p in &plies {
                minimum = minimum.max(need(run, p, "min_coal_thickness")?);
            }
            let t = sum(run, &units, thickness);
            let kept = t > minimum;
            run.work(&run.ids(&units), || {
                let sum = format!("{} m, partings in, {} {} {} m", set(t), if kept { ">" } else { "≤" }, step.setting("min_coal_thickness"), set(minimum));
                Line::test("section thickness", sum, if kept { "a working section" } else { "thin loss: its coal set aside, its partings waste" })
            });
            if !kept {
                thin.extend(plies);
                continue;
            }
            // Forced splits: a new part where the plies' names change, each parting with the ply above.
            let name = |run: &ColumnRun<'_>, i: usize| run.parameter_word(i, step.setting("ws_split")).unwrap_or_default();
            let mut part: Vec<usize> = Vec::new();
            let mut last: Option<String> = None;
            for &i in &units {
                if is_ply(run, i) {
                    let n = name(run, i);
                    if let Some(l) = last.as_ref().filter(|l| **l != n) {
                        run.work(&run.ids(&[i]), || Line::test("forced split", format!("{} {l} → {n}", step.setting("ws_split")), "a new section starts at this ply; the partings above it go with the ply above"));
                        // The partings since the last ply go with the ply above.
                        sections.push(std::mem::take(&mut part));
                    }
                    last = Some(n);
                }
                part.push(i);
            }
            sections.push(part);
        }
    }

    // Labels: thin coal set aside, partings in sections are dilution, numbers top down.
    for &i in &thin {
        let v = run.column.units()[i].value(volume);
        run.column.label_became(i, Became { kind: BecameKind::ThinLoss, detail: "thin".into() });
        run.said.count("coal in no working section, thin loss".into(), v);
    }
    let section_of: BTreeMap<usize, usize> = sections.iter().enumerate().flat_map(|(s, units)| units.iter().map(move |&i| (i, s))).collect();
    let mut id = 0u32;
    let mut previous: Option<Option<usize>> = None;
    for &i in &active {
        if run.column.units()[i].is_set_aside() {
            continue;
        }
        let s = section_of.get(&i).copied();
        if previous != Some(s) {
            id += 1;
            previous = Some(s);
        }
        let name = match s {
            Some(s) => {
                let plies: Vec<usize> = sections[s].iter().copied().filter(|&p| is_ply(run, p)).collect();
                let seam = |p: usize| run.column.units()[p].in_situ.seam.clone();
                let (roof, floor) = (seam(plies[0]), seam(*plies.last().expect("a section has a ply")));
                if roof == floor { roof } else { format!("{roof}-{floor}") }
            }
            None => run.column.units()[i].in_situ.seam.clone(),
        };
        let coal = s.is_some();
        let parting = coal && !is_ply(run, i);
        run.work(&run.ids(&[i]), || {
            let line = Line::note("working section", format!("number {id}, {}, {name}", if coal { "coal" } else { "waste" }));
            if parting { line.then("a parting in a coal section: parting dilution") } else { line }
        });
        if parting {
            run.column.label_became(i, Became { kind: BecameKind::Dilution, detail: "parting".into() });
        }
        run.column.unit_mut(i).section = Some(Labels { material: if coal { "coal".into() } else { "waste".into() }, seam: name, id: Some(id) });
    }

    // The writes: each section's units under Working section, with its number and name; thin and discarded coal under
    // Loss, as Spry's Loss\Thin, Loss\Discarded and Loss\High Insitu Ash.
    for units in &sections {
        let v = sum(run, units, volume);
        run.said.count("coal formed into working sections".into(), v);
    }
    write_pieces(
        run,
        |u| match &u.became {
            Some(b) if b.kind == BecameKind::ThinLoss => Some("Loss\\Thin".into()),
            Some(b) if b.kind == BecameKind::Discarded && b.detail == "high in situ ash" => Some("Loss\\High insitu ash".into()),
            Some(b) if b.kind == BecameKind::Discarded => Some("Loss\\Discarded".into()),
            _ if u.section.as_ref().is_some_and(|s| s.material == "coal") => Some("Working section".into()),
            _ => None,
        },
        |u| match &u.section {
            Some(s) if s.material == "coal" => vec![("Working section\\id".into(), s.id.map_or(String::new(), |i| i.to_string())), ("Working section\\name".into(), s.seam.clone())],
            _ => Vec::new(),
        },
    );
    Ok(())
}

/// One working section or run of waste between them, top down: its number, whether it's coal, its name and its units.
#[derive(Debug, Clone)]
struct Section {
    id: u32,
    coal: bool,
    name: String,
    units: Vec<UnitId>,
}

/// The column's working sections and the waste between them, top down, from the labels `working_sections` gave.
fn sections_of(run: &ColumnRun<'_>) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for u in run.column.units() {
        if u.is_set_aside() {
            continue;
        }
        let Some(s) = &u.section else { continue };
        let id = s.id.unwrap_or(0);
        match out.last_mut() {
            Some(last) if last.id == id => last.units.push(u.id),
            _ => out.push(Section { id, coal: s.material == "coal", name: s.seam.clone(), units: vec![u.id] }),
        }
    }
    out
}

/// Takes `volume` off the top or bottom of these units, splitting the last one it reaches: the pieces taken, top down.
fn take_volume(run: &mut ColumnRun<'_>, units: &[UnitId], volume: f64, end: End) -> Result<Vec<UnitId>, String> {
    let v = run.column.fields().volume();
    let mut left = volume;
    let mut taken = Vec::new();
    let order: Vec<UnitId> = if end == End::Top { units.to_vec() } else { units.iter().rev().copied().collect() };
    for id in order {
        if left <= 0.0 {
            break;
        }
        let at = run.column.position(id).ok_or("a unit went missing")?;
        let has = run.column.units()[at].value(v);
        if has <= left * (1.0 + 1e-12) {
            taken.push(id);
            left -= has;
        } else {
            let split = run.column.split(at, Amount::Volume(left), end).map_err(|e| e.to_string())?;
            taken.push(run.column.units()[split.piece].id);
            left = 0.0;
        }
    }
    if end == End::Bottom {
        taken.reverse();
    }
    Ok(taken)
}

/// Labels pieces as loss of a kind, sent to a section in ROM.
fn send_loss(run: &mut ColumnRun<'_>, pieces: &[UnitId], kind: &str, to: &Section) {
    for &id in pieces {
        let Some(at) = run.column.position(id) else { continue };
        run.column.label_became(at, Became { kind: BecameKind::Loss, detail: kind.into() });
        run.column.unit_mut(at).rom = Some(Labels { material: if to.coal { "coal".into() } else { "waste".into() }, seam: to.name.clone(), id: Some(to.id) });
    }
}

/// `rom_loss` (decision 0074, point 4), Spry's way. For each working section, top down:
///
/// - roof loss, `roof_loss` m (the largest of its plies') over its area (the largest of its units'), off its top into
///   the section above; floor loss, `floor_loss` m, off its bottom into the section below (the underburden where it's the
///   last coal); across a boundary with another coal section only where `loss_between_coal` is yes;
/// - a section that doesn't hold its roof and floor loss is thin loss, all of it;
/// - then, for each of its coal plies, edge loss (`edge_loss` m × the block's length × the ply's thickness) and other
///   loss (`other_loss` % of the ply's volume) off its top into the nearest waste above; a section that doesn't hold
///   them is thin loss.
///
/// The block's length is its `length` attribute (the step's setting names it), else the longer side of the smallest
/// rectangle round it in plan. Each loss is written on the leaf it came from, under `Loss\Roof`, `Loss\Floor`,
/// `Loss\Edge` and `Loss\Other`, coal alone as Spry writes it.
fn rom_loss(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let fields = run.column.fields();
    let (v, thickness) = (fields.volume(), fields.thickness());
    let area = fields.list().iter().position(|f| f.kind == FieldKind::Area);
    let sections = sections_of(run);
    let across = |run: &ColumnRun<'_>, at: usize| run.parameter_word(at, step.setting("loss_between_coal")).is_some_and(|t| matches!(t.to_lowercase().as_str(), "yes" | "y" | "true" | "1"));
    for (k, s) in sections.iter().enumerate() {
        if !s.coal {
            continue;
        }
        let plies: Vec<usize> = s.units.iter().filter_map(|&id| run.column.position(id)).filter(|&at| is_coal(&run.column.units()[at])).collect();
        let largest = |run: &ColumnRun<'_>, key: &str| -> Result<f64, String> {
            let mut most: f64 = 0.0;
            for &at in &plies {
                let value = run.parameter(at, step.setting(key)).ok_or_else(|| format!("{} has no {} (no parameter row gives it)", run.name_of(at), step.setting(key)))?;
                most = most.max(value);
            }
            Ok(most)
        };
        let (roof, floor, edge, other) = (largest(run, "roof_loss")?, largest(run, "floor_loss")?, largest(run, "edge_loss")?, largest(run, "other_loss")?);
        let section_area = match area {
            Some(a) => s.units.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(a)).fold(0.0, f64::max),
            None => run.block_area,
        };
        let between = plies.first().is_some_and(|&at| across(run, at));
        let above = k.checked_sub(1).map(|j| sections[j].clone()).filter(|a| !a.coal || between);
        let below = sections.get(k + 1).cloned().filter(|b| !b.coal || between);
        let (roof_v, floor_v) = (if above.is_some() { roof * section_area } else { 0.0 }, floor * section_area);
        let held: f64 = s.units.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
        if floor_v > 0.0 && below.is_none() && sections.get(k + 1).is_none() {
            return Err(format!("working section {} ({}) has no waste below it for its floor loss: the column ends in coal with no underburden", s.id, s.name));
        }
        let this = section_words(s);
        run.work(&s.units, || match (&above, k) {
            (Some(a), _) => Line::sum("roof loss", format!("{} {} m × area {} m²", step.setting("roof_loss"), set(roof), vol(section_area)), roof_v, "m³").then(format!("off the top of {this}, into {}", section_words(a))),
            (None, 0) => Line::test("roof loss", format!("{} {} m", step.setting("roof_loss"), set(roof)), "none: no section above"),
            (None, _) => Line::test("roof loss", format!("{} {} m", step.setting("roof_loss"), set(roof)), format!("none: the section above is coal, and {} isn't yes", step.setting("loss_between_coal"))),
        });
        run.work(&s.units, || {
            let line = Line::sum("floor loss", format!("{} {} m × area {} m²", step.setting("floor_loss"), set(floor), vol(section_area)), floor_v, "m³");
            match &below {
                Some(b) => line.then(format!("off the bottom of {this}, into {}", section_words(b))),
                None => line.then(format!("not taken: the section below is coal, and {} isn't yes", step.setting("loss_between_coal"))),
            }
        });
        // Too thin for its roof and floor loss: thin loss, all of it.
        let short = held <= roof_v + floor_v;
        run.work(&s.units, || {
            let sum = format!("{} m³ {} roof and floor loss {} m³", vol(held), if short { "≤" } else { ">" }, vol(roof_v + floor_v));
            Line::test("section holds", sum, if short { "thin loss, all of it" } else { "it holds them" })
        });
        if short {
            thin_section(run, s, &sections, k, "short of its roof and floor loss");
            continue;
        }
        // Edge and other loss come from each ply as it stood before roof and floor loss, as Spry's do.
        let mut asked = Vec::new();
        let mut asked_lines: Vec<(UnitId, Line, Line)> = Vec::new();
        for &at in &plies {
            let length = if edge > 0.0 { length_of(run, at, step)? } else { 0.0 };
            let u = &run.column.units()[at];
            let (e, o) = (edge * length * u.value(thickness), other / 100.0 * u.value(v));
            asked.push((u.id, e, o));
            if run.workings.is_some() {
                let edge_line = Line::sum("edge loss", format!("{} {} m × length {} m × thickness {} m", step.setting("edge_loss"), set(edge), vol(length), set(u.value(thickness))), e, "m³");
                let other_line = Line::sum("other loss", format!("{} {} % × volume {} m³", step.setting("other_loss"), set(other), vol(u.value(v))), o, "m³");
                asked_lines.push((u.id, edge_line, other_line));
            }
        }
        let mut rest = s.units.clone();
        if let Some(above) = &above
            && roof_v > 0.0
        {
            let pieces = take_volume(run, &rest, roof_v, End::Top)?;
            rest = units_left(run, &pieces, &rest);
            send_loss(run, &pieces, "roof", above);
            say_loss(run, &pieces, "roof");
        }
        if let Some(below) = &below
            && floor_v > 0.0
        {
            let pieces = take_volume(run, &rest, floor_v, End::Bottom)?;
            rest = units_left(run, &pieces, &rest);
            send_loss(run, &pieces, "floor", below);
            say_loss(run, &pieces, "floor");
        }
        // Edge and other loss, from each coal ply's top, into the nearest waste above.
        let Some(waste_above) = sections[..k].iter().rev().find(|w| !w.coal).cloned() else {
            for (id, e, o) in asked_lines {
                run.work(&[id], || e.then("not taken: no waste above"));
                run.work(&[id], || o.then("not taken: no waste above"));
            }
            continue;
        };
        let remaining: f64 = rest.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
        let wanted: f64 = asked.iter().map(|(_, e, o)| e + o).sum();
        let into = format!("off its top, into {}", section_words(&waste_above));
        for (id, e, o) in asked_lines {
            run.work(&[id], || e.then(into.clone()));
            run.work(&[id], || o.then(into.clone()));
        }
        if remaining <= wanted {
            run.work(&s.units, || Line::test("section holds", format!("{} m³ ≤ edge and other loss {} m³", vol(remaining), vol(wanted)), "thin loss, all of it"));
            thin_section(run, s, &sections, k, "short of its edge and other loss");
            continue;
        }
        for (ply, e, o) in asked {
            // What's left of the ply: the piece that kept its id, unless all of it went as roof or floor loss.
            let in_rest: Vec<UnitId> = rest.iter().copied().filter(|&id| id == ply).collect();
            if in_rest.is_empty() {
                continue;
            }
            for (amount, kind) in [(e, "edge"), (o, "other")] {
                if amount <= 0.0 {
                    continue;
                }
                let pieces = take_volume(run, &in_rest, amount, End::Top)?;
                send_loss(run, &pieces, kind, &waste_above);
                say_loss(run, &pieces, kind);
            }
        }
    }
    // The writes: each loss on the leaf it came from, coal alone.
    write_pieces(
        run,
        |u| match &u.became {
            Some(b) if b.kind == BecameKind::Loss && is_coal(u) => Some(format!("Loss\\{}", title(&b.detail))),
            Some(b) if b.kind == BecameKind::ThinLoss && b.detail.starts_with("short") && is_coal(u) => Some("Loss\\Thin".into()),
            _ => None,
        },
        |_| Vec::new(),
    );
    Ok(())
}

/// A section in a working's words: `section 2 coal A-B`.
fn section_words(s: &Section) -> String {
    format!("section {} {} {}", s.id, if s.coal { "coal" } else { "waste" }, s.name).trim_end().to_string()
}

/// The units of a section still in it after pieces were taken: what was `rest` less the pieces taken (a unit split keeps
/// its id for what's left of it).
fn units_left(run: &ColumnRun<'_>, pieces: &[UnitId], rest: &[UnitId]) -> Vec<UnitId> {
    rest.iter().copied().filter(|id| !pieces.contains(id) && run.column.position(*id).is_some()).collect()
}

fn say_loss(run: &mut ColumnRun<'_>, pieces: &[UnitId], kind: &str) {
    let v = run.column.fields().volume();
    let volume: f64 = pieces.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
    run.said.count(format!("{kind} loss"), volume);
}

/// A section too thin for its loss: its coal thin loss and its partings waste again, all of it in the nearest waste (above,
/// else below), as Spry.
fn thin_section(run: &mut ColumnRun<'_>, s: &Section, sections: &[Section], k: usize, why: &str) {
    let to = sections[..k].iter().rev().find(|w| !w.coal).or_else(|| sections[k + 1..].iter().find(|w| !w.coal)).cloned();
    let v = run.column.fields().volume();
    let mut volume = 0.0;
    for &id in &s.units {
        let Some(at) = run.column.position(id) else { continue };
        let coal = is_coal(&run.column.units()[at]);
        if coal {
            volume += run.column.units()[at].value(v);
            run.column.label_became(at, Became { kind: BecameKind::ThinLoss, detail: why.into() });
        } else {
            run.column.unit_mut(at).became = None;
        }
        if let Some(to) = &to {
            run.column.unit_mut(at).section = Some(Labels { material: "waste".into(), seam: to.name.clone(), id: Some(to.id) });
        }
    }
    run.said.count(format!("coal in a working section {why}, thin loss"), volume);
}

/// The largest of a coal section's plies' values of a setting; a ply no row gives it stops the block.
fn largest_of(run: &ColumnRun<'_>, s: &Section, step: &DataflowStep, key: &str) -> Result<f64, String> {
    let mut most: f64 = 0.0;
    for &id in &s.units {
        let Some(at) = run.column.position(id) else { continue };
        if !is_coal(&run.column.units()[at]) || run.column.units()[at].became.is_some() {
            continue;
        }
        let value = run.parameter(at, step.setting(key)).ok_or_else(|| format!("{} has no {} (no parameter row gives it)", run.name_of(at), step.setting(key)))?;
        most = most.max(value);
    }
    Ok(most)
}

/// A section's area: the largest of its units' plan areas, else the column's largest block.
fn section_area(run: &ColumnRun<'_>, s: &Section) -> f64 {
    match run.column.fields().list().iter().position(|f| f.kind == FieldKind::Area) {
        Some(a) => s.units.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(a)).fold(0.0, f64::max),
        None => run.block_area,
    }
}

/// A coal unit's length in plan, the dataflow's `length` field (the step's setting names it): what the interrogation
/// measured on its block (decision 0074), or what the parameter rows correct it to. Asked for edge loss or dilution
/// where there's none, the block stops.
fn length_of(run: &ColumnRun<'_>, at: usize, step: &DataflowStep) -> Result<f64, String> {
    let field = run.column.fields().at(step.setting("length")).map_err(|_| format!("the dataflow has no {} field for edge loss and dilution: add one, read from the blocks' measured length", step.setting("length")))?;
    let length = run.column.units()[at].value(field);
    if length.is_finite() { Ok(length) } else { Err(format!("{} has no {}: measure the blocks (solid.measure) or give one in a parameter row", run.name_of(at), step.setting("length"))) }
}

/// `rom_dilution` (decision 0074, point 6), Spry's way. Each waste interval, top down, gives dilution to the coal either
/// side of it, cut in Spry's order: floor dilution for the coal above off its top (`floor_dilution` m, the largest of
/// that section's plies', over its area); roof dilution for the coal below off its bottom (`roof_dilution` m); then
/// edge dilution (`edge_dilution` m × the block's length × the ply's thickness) and other dilution (`other_dilution` %
/// of the ply's volume) for every coal section below it down to the next waste. Where the waste can't give it all, each
/// is shared out in proportion to what it holds, and the losses that were sent into that waste go back to their coal, as
/// Spry's; ours says so, with the volumes. Each piece is labelled dilution of its kind, its ROM label the coal section it
/// goes to, and written under `Dilution\Roof`, `\Floor`, `\Edge` and `\Other` on the leaf it was taken from.
fn rom_dilution(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let v = run.column.fields().volume();
    let thickness = run.column.fields().thickness();
    let sections = sections_of(run);
    for (k, w) in sections.iter().enumerate() {
        if w.coal {
            continue;
        }
        let above = k.checked_sub(1).map(|j| sections[j].clone()).filter(|s| s.coal);
        // The coal sections below this waste, down to the next waste.
        let below: Vec<Section> = sections[k + 1..].iter().take_while(|s| s.coal).cloned().collect();
        // Each ask: its kind, its volume, the coal section it's for, and its sum in words for the notebook.
        let mut asks: Vec<(&'static str, f64, Section, String)> = Vec::new();
        if let Some(a) = &above {
            let (floor, area) = (largest_of(run, a, step, "floor_dilution")?, section_area(run, a));
            asks.push(("floor", floor * area, a.clone(), format!("{} {} m × area {} m²", step.setting("floor_dilution"), set(floor), vol(area))));
        }
        if let Some(b) = below.first() {
            let (roof, area) = (largest_of(run, b, step, "roof_dilution")?, section_area(run, b));
            asks.push(("roof", roof * area, b.clone(), format!("{} {} m × area {} m²", step.setting("roof_dilution"), set(roof), vol(area))));
        }
        for b in &below {
            let (edge, other) = (largest_of(run, b, step, "edge_dilution")?, largest_of(run, b, step, "other_dilution")?);
            let (mut e, mut o) = (0.0, 0.0);
            let (mut edge_terms, mut coal) = (Vec::new(), 0.0);
            for &id in &b.units {
                let Some(at) = run.column.position(id) else { continue };
                let u = &run.column.units()[at];
                if !is_coal(u) || u.became.is_some() {
                    continue;
                }
                if edge > 0.0 {
                    let length = length_of(run, at, step)?;
                    e += edge * length * u.value(thickness);
                    edge_terms.push(format!("length {} m × thickness {} m", vol(length), set(u.value(thickness))));
                }
                o += other / 100.0 * u.value(v);
                coal += u.value(v);
            }
            let edge_sum = match edge_terms.as_slice() {
                [one] => format!("{} {} m × {one}", step.setting("edge_dilution"), set(edge)),
                many => format!("{} {} m × ({})", step.setting("edge_dilution"), set(edge), many.join(" + ")),
            };
            asks.push(("edge", e, b.clone(), edge_sum));
            asks.push(("other", o, b.clone(), format!("{} {} % × coal {} m³", step.setting("other_dilution"), set(other), vol(coal))));
        }
        asks.retain(|(_, volume, _, _)| *volume > 0.0);
        let total: f64 = asks.iter().map(|(_, volume, _, _)| volume).sum();
        if total <= 0.0 {
            continue;
        }
        let held: f64 = w.units.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
        // Each ask on the waste's units and its coal section's, with where it's cut from: floor dilution off the top.
        let interval = section_words(w);
        let short = held < total * (1.0 - 1e-12);
        let scale = if short { held / total } else { 1.0 };
        for (kind, volume, to, sum) in &asks {
            let mut on = w.units.clone();
            on.extend(&to.units);
            let end = if *kind == "floor" { "top" } else { "bottom" };
            run.work(&on, || {
                let line = Line::sum(&format!("{kind} dilution"), sum.clone(), *volume, "m³");
                if short {
                    line.then(format!("{} m³ of it, shared out, off the {end} of {interval}, into {}", vol(volume * scale), section_words(to)))
                } else {
                    line.then(format!("off the {end} of {interval}, into {}", section_words(to)))
                }
            });
        }
        if short {
            run.work(&w.units, || Line::test("waste holds", format!("{} m³ < dilution asked {} m³", vol(held), vol(total)), "all of it given as dilution, shared out by what each asked; the losses sent into it go back to their coal"));
            // Too thin: shared out, and the losses sent into it go back to their coal.
            for ask in &mut asks {
                ask.1 *= scale;
            }
            let mut returned = 0.0;
            for i in 0..run.column.len() {
                let u = &run.column.units()[i];
                let sent_here = u.became.as_ref().is_some_and(|b| b.kind == BecameKind::Loss) && u.rom.as_ref().is_some_and(|r| r.id == Some(w.id));
                if sent_here {
                    returned += u.value(v);
                    let unit = run.column.unit_mut(i);
                    unit.became = None;
                    unit.rom = None;
                }
            }
            run.said.count("waste in intervals short of their dilution, all of it given as dilution shared out".into(), held);
            if returned > 0.0 {
                run.said.count("loss sent into waste short of its dilution, returned to its coal".into(), returned);
            }
            let first = w.units.iter().find_map(|&id| run.column.position(id)).map(|at| run.name_of(at)).unwrap_or_default();
            run.said.name("short of their dilution (dilution shared out, losses sent there returned)", &format!("{first} ({:.1} of {:.1} m3 asked)", held, total));
        }
        // Floor dilution off the top; the rest off the bottom, roof first.
        let mut rest = w.units.clone();
        for (kind, volume, to, _) in asks.iter().filter(|(kind, _, _, _)| *kind == "floor") {
            let pieces = take_volume(run, &rest, *volume, End::Top)?;
            rest = units_left(run, &pieces, &rest);
            send_dilution(run, &pieces, kind, to);
        }
        for (kind, volume, to, _) in asks.iter().filter(|(kind, _, _, _)| *kind != "floor") {
            let pieces = take_volume(run, &rest, *volume, End::Bottom)?;
            rest = units_left(run, &pieces, &rest);
            send_dilution(run, &pieces, kind, to);
        }
    }
    write_pieces(
        run,
        |u| match &u.became {
            Some(b) if b.kind == BecameKind::Dilution && b.detail != "parting" => Some(format!("Dilution\\{}", title(&b.detail))),
            _ => None,
        },
        |_| Vec::new(),
    );
    Ok(())
}

/// Labels pieces as dilution of a kind, sent to a coal section in ROM.
fn send_dilution(run: &mut ColumnRun<'_>, pieces: &[UnitId], kind: &str, to: &Section) {
    let v = run.column.fields().volume();
    let mut volume = 0.0;
    for &id in pieces {
        let Some(at) = run.column.position(id) else { continue };
        volume += run.column.units()[at].value(v);
        run.column.label_became(at, Became { kind: BecameKind::Dilution, detail: kind.into() });
        run.column.unit_mut(at).rom = Some(Labels { material: "coal".into(), seam: to.name.clone(), id: Some(to.id) });
    }
    run.said.count(format!("{kind} dilution"), volume);
}

/// What a ROM section holds: the units sent to it, and its own units not sent elsewhere or set aside.
fn rom_members(run: &ColumnRun<'_>, id: u32) -> Vec<usize> {
    (0..run.column.len())
        .filter(|&i| {
            let u = &run.column.units()[i];
            match &u.rom {
                Some(r) => r.id == Some(id),
                None => !u.is_set_aside() && u.section.as_ref().is_some_and(|s| s.id == Some(id) && s.material == "coal"),
            }
        })
        .collect()
}

/// `rom_ash_cutoff` (decision 0074, point 7), once, as Spry. Each ROM coal section's ash, its air-dried ash averaged
/// over its coal, partings and dilution by the field's rule (the step's `ash` setting names the field), against
/// `rom_ash_cutoff` (the smallest of its plies'; none, no test). Over it, the section is rejected: its coal becomes high
/// ROM ash loss, its dilution goes back to its waste, and the losses it sent to its neighbours come back as high ROM ash
/// loss too. Each rejection is said with the ash and the cut-off; the coal is written under `Loss\High ROM ash` on its
/// leaves.
fn rom_ash_cutoff(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let Ok(ash) = run.column.fields().at(step.setting("ash")) else { return Ok(()) };
    let v = run.column.fields().volume();
    for s in sections_of(run).into_iter().filter(|s| s.coal) {
        let mut cutoff = f64::INFINITY;
        for &id in &s.units {
            let Some(at) = run.column.position(id) else { continue };
            if is_coal(&run.column.units()[at])
                && let Some(c) = run.parameter(at, step.setting("rom_ash_cutoff"))
            {
                cutoff = cutoff.min(c);
            }
        }
        let members = rom_members(run, s.id);
        let on = run.ids(&members);
        if !cutoff.is_finite() {
            run.work(&on, || Line::test("ROM ash", format!("no {} for its coal", step.setting("rom_ash_cutoff")), "not tested"));
            continue;
        }
        let rom_ash = run.column.add_up(members.iter().copied())[ash];
        if !rom_ash.is_finite() {
            run.work(&on, || Line::test("ROM ash", format!("no {} in the section", step.setting("ash")), "not tested"));
            run.said.name("ROM section with no ash to test against its cut-off", &format!("{} section {}", run.name_of(members[0]), s.name));
            continue;
        }
        let over = rom_ash > cutoff;
        run.work(&on, || {
            let sum = format!(
                "{} {} %, over its coal, partings and dilution, {} {} {} %",
                step.setting("ash"),
                figure(rom_ash, 2),
                if over { ">" } else { "≤" },
                step.setting("rom_ash_cutoff"),
                set(cutoff)
            );
            let then = if over { format!("{} rejected: its coal high ROM ash loss, its dilution back to its waste", section_words(&s)) } else { "kept".into() };
            Line::test("ROM ash", sum, then)
        });
        if !over {
            continue;
        }
        let mut volume = 0.0;
        for i in 0..run.column.len() {
            let u = &run.column.units()[i];
            let from_here = u.section.as_ref().is_some_and(|x| x.id == Some(s.id));
            let to_here = u.rom.as_ref().is_some_and(|r| r.id == Some(s.id));
            let is_dilution = u.became.as_ref().is_some_and(|b| b.kind == BecameKind::Dilution && b.detail != "parting");
            if to_here && is_dilution {
                // Dilution back to its waste.
                let unit = run.column.unit_mut(i);
                unit.became = None;
                unit.rom = None;
            } else if from_here && is_coal(u) && (u.became.is_none() || u.became.as_ref().is_some_and(|b| b.kind == BecameKind::Loss)) {
                volume += u.value(v);
                let unit = run.column.unit_mut(i);
                unit.became = Some(Became { kind: BecameKind::HighAshLoss, detail: "rom ash".into() });
                unit.rom = None;
            } else if from_here && !is_coal(u) {
                let unit = run.column.unit_mut(i);
                if unit.became.as_ref().is_some_and(|b| b.detail == "parting") {
                    unit.became = None;
                }
                if let Some(section) = unit.section.as_mut() {
                    section.material = "waste".into();
                }
            }
        }
        run.said.count("coal in a ROM section over its ash cut-off, high ROM ash loss".into(), volume);
        let first = s.units.iter().find_map(|&id| run.column.position(id)).map(|at| run.name_of(at)).unwrap_or_default();
        run.said.name("ROM section over its ash cut-off, rejected", &format!("{first} section {} ({rom_ash:.2} over {cutoff:.2})", s.name));
    }
    write_pieces(
        run,
        |u| match &u.became {
            Some(b) if b.kind == BecameKind::HighAshLoss => Some("Loss\\High ROM ash".into()),
            _ => None,
        },
        |_| Vec::new(),
    );
    Ok(())
}

/// The coal plies of a ROM coal section, top down: its coal in situ, nothing done to it.
fn plies_of(run: &ColumnRun<'_>, s: &Section) -> Vec<usize> {
    s.units.iter().filter_map(|&id| run.column.position(id)).filter(|&at| {
        let u = &run.column.units()[at];
        is_coal(u) && u.became.is_none()
    }).collect()
}

fn says_yes(run: &ColumnRun<'_>, at: usize, column: &str) -> bool {
    run.parameter_word(at, column).is_some_and(|t| matches!(t.to_lowercase().as_str(), "yes" | "y" | "true" | "1"))
}

/// `rom` (decision 0074, point 8): nothing built in since decision 0077 (7c, part 2). The starter's `rom_qualities`
/// formula, after it, gives dilution and partings their coal's moistures, aligns their air-dried moisture with the coal's
/// (qualities, RD and air-dried tonnes reworked, Spry's way), gives a negative quality the coal's and works every unit's
/// ROM density. The step stays as the stage those formulas follow.
fn rom(_: &mut ColumnRun<'_>, _: &DataflowStep) -> Result<(), String> {
    Ok(())
}

/// Where a piece goes in the reserves table: onto the leaves of `onto`, a joined unit's benches by their in situ shares
/// (Spry's reinstated benches), its summed fields shared out, under `folder`, with the words of the unit `from` it is
/// (Waste allocation plan, decision 2) beside its numbers. A move between coal and waste (`to_coal`, a folder of no
/// parts) and a Summary folder take no words.
fn place(run: &ColumnRun<'_>, values: &[f64], from: usize, onto: usize, folder: &str, texts: &[(String, String)]) -> Vec<Piece> {
    let fields = run.column.fields();
    let mut texts = texts.to_vec();
    if folder.contains('\\') && !folder.starts_with("Summary") {
        texts.extend(word_texts(&run.column.units()[from], folder));
    }
    let u = &run.column.units()[onto];
    let shares: Vec<(u32, f64)> = match u.joined {
        Some(j) => run.column.joins()[j as usize].benches.iter().map(|b| (b.origin, b.share)).collect(),
        None => vec![(u.origin, 1.0)],
    };
    shares
        .into_iter()
        .map(|(origin, share)| {
            let attributes = &run.attributes[origin as usize];
            let leaf = run.lower.iter().map(|l| attribute_ci(attributes, l).map(attribute_word).unwrap_or_default()).collect();
            let values = values.iter().enumerate().map(|(f, x)| if fields.is_summed(f) { x * share } else { *x }).collect();
            Piece { folder: folder.to_string(), unit: u.id, leaf, values, texts: texts.clone() }
        })
        .collect()
}

/// `final` (decision 0074, point 9). Every unit written where it ends, Spry's way:
///
/// - **ROM coal**, each section's: its coal and partings on their own leaves; roof dilution on its first ply's leaves and
///   floor dilution on its last's; edge and other dilution over its plies by their volumes; coal lost into it from the
///   section above on its first ply's, from below on its last's. A joined ply's benches take their pieces by their in situ
///   shares. Where the first ply's row says `aggregate_sections` yes, each piece carries the section's qualities.
/// - **ROM waste**: waste, discarded, thin and high ROM ash coal on their own leaves; roof, edge and other loss on the last
///   unit of the waste it went into, floor loss on its first; a loss into the underburden, which has no leaf, on its own.
/// - **Scheduled**, beside them: what a schedule takes, a copy of ROM until factors come between them.
/// - **ROM clean coal** (`ROM\Clean coal`, decision 0086): what's coal in situ and in ROM, each section's coal alone, its qualities the section's coal's where
///   aggregated.
/// - **Out of model**: underburden taken into ROM coal as dilution, under `Summary\Out of model`, on the leaves it's
///   written on; the underburden left is never written. Small volumes are never in ROM.
fn final_step(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let v = run.column.fields().volume();
    let qualities: Vec<usize> = run.column.fields().list().iter().enumerate().filter(|(_, f)| f.kind == FieldKind::Quality).map(|(i, _)| i).collect();
    let sections = sections_of(run);
    let mut pieces: Vec<Piece> = Vec::new();
    let mut moves: Vec<Piece> = Vec::new();
    let mut in_rom_coal = vec![false; run.column.len()];
    for s in sections.iter().filter(|s| s.coal) {
        let plies = plies_of(run, s);
        if plies.is_empty() {
            continue;
        }
        let members = rom_members(run, s.id);
        let aggregate = says_yes(run, plies[0], step.setting("aggregate_sections"));
        // Clean coal at its section's coal average, by tonnes (Spry's Aggregate WS Clean Coal), where the first ply's row
        // says `clean_coal_average` yes, or where the section's qualities are aggregated anyway.
        let clean_average = aggregate || says_yes(run, plies[0], step.setting("clean_coal_average"));
        let section_values = run.column.add_up(members.iter().copied());
        let clean_values = run.column.add_up(plies.iter().copied());
        let texts = vec![("ROM\\section".to_string(), s.name.clone())];
        let ply_volume: f64 = plies.iter().map(|&p| run.column.units()[p].value(v)).sum();
        if aggregate && !qualities.is_empty() {
            run.work(&run.ids(&members), || {
                let q: Vec<String> = qualities.iter().map(|&q| format!("{} {}", run.column.fields().list()[q].name, figure(section_values[q], 2))).collect();
                Line::note("section qualities", format!("{} yes: every piece of {} at the section's {}", step.setting("aggregate_sections"), section_words(s), q.join(", ")))
            });
        }
        for &m in &members {
            in_rom_coal[m] = true;
            let u = &run.column.units()[m];
            let mut values = u.values().to_vec();
            if aggregate {
                for &q in &qualities {
                    values[q] = section_values[q];
                }
            }
            let kind = u.became.as_ref().map(|b| (b.kind, b.detail.clone()));
            // Which plies it goes on, and in what shares.
            let onto: Vec<(usize, f64)> = match &kind {
                None => vec![(m, 1.0)],
                Some((BecameKind::Dilution, d)) if d == "parting" => vec![(m, 1.0)],
                Some((BecameKind::Dilution, d)) if d == "roof" => vec![(plies[0], 1.0)],
                Some((BecameKind::Dilution, d)) if d == "floor" => vec![(*plies.last().expect("plies"), 1.0)],
                Some((BecameKind::Loss, d)) if d == "floor" => vec![(plies[0], 1.0)],
                Some((BecameKind::Loss, _)) => vec![(*plies.last().expect("plies"), 1.0)],
                _ => plies.iter().map(|&p| (p, if ply_volume > 0.0 { run.column.units()[p].value(v) / ply_volume } else { 1.0 / plies.len() as f64 })).collect(),
            };
            let out_of_model = run.is_out_of_model(u);
            run.work(&[u.id], || {
                let whose = if out_of_model { "underburden, outside the model, " } else { "" };
                match onto.as_slice() {
                    [(p, _)] if *p == m => Line::sum("ROM coal", format!("{whose}{}", run.words_of(m)), u.value(v), "m³").then(format!("ROM\\Coal and Scheduled\\Coal, {} on its own leaves", section_words(s))),
                    [(p, _)] => Line::sum("ROM coal", format!("{whose}{}", run.words_of(m)), u.value(v), "m³").then(format!("ROM\\Coal and Scheduled\\Coal, {}, on {}'s leaves", section_words(s), run.words_of(*p))),
                    many => {
                        let shares: Vec<String> = many.iter().map(|(p, share)| format!("{} {} m³", run.words_of(*p), vol(u.value(v) * share))).collect();
                        Line::sum("ROM coal", format!("{whose}{}, shared over the plies by their volumes", run.words_of(m)), u.value(v), "m³").then(format!("ROM\\Coal and Scheduled\\Coal, {}: {}", section_words(s), shares.join(", ")))
                    }
                }
            });
            for &(p, share) in &onto {
                let scaled: Vec<f64> = values.iter().enumerate().map(|(f, x)| if run.column.fields().is_summed(f) { x * share } else { *x }).collect();
                pieces.extend(place(run, &scaled, m, p, "ROM\\Coal", &texts));
                pieces.extend(place(run, &scaled, m, p, "Scheduled\\Coal", &texts));
                if out_of_model {
                    pieces.extend(place(run, &scaled, m, p, "Summary\\Out of model", &[]));
                }
            }
            if out_of_model {
                run.said.count("underburden taken into ROM coal, out of model".into(), u.value(v));
            }
            // Waste ending in ROM coal moved to coal; the underburden came from outside the model (decision 0086). The
            // underburden has no record of its own (no seam, never written): what came in from outside is said on the
            // records it went into as ROM coal's rock, the lowest coal for floor dilution, so each record balances.
            if out_of_model {
                for &(p, share) in &onto {
                    let scaled: Vec<f64> = u.values().iter().enumerate().map(|(f, x)| if run.column.fields().is_summed(f) { x * share } else { *x }).collect();
                    moves.extend(place(run, &scaled, m, p, "from_outside", &[]));
                }
            } else if !is_coal(u) {
                moves.extend(place(run, u.values(), m, m, "to_coal", &[]));
            }
            if plies.contains(&m) {
                let mut clean = values.clone();
                if clean_average {
                    for &q in &qualities {
                        clean[q] = clean_values[q];
                    }
                }
                run.work(&[run.column.units()[m].id], || {
                    let at = if clean_average { format!(", at the section's coal average ({} or {} yes)", step.setting("clean_coal_average"), step.setting("aggregate_sections")) } else { String::new() };
                    Line::sum("clean coal", format!("{}{at}", run.words_of(m)), clean[v], "m³").then("ROM\\Clean coal")
                });
                pieces.extend(place(run, &clean, m, m, "ROM\\Clean coal", &texts));
            }
        }
    }
    // Model waste below the last coal isn't mined: it's taken as underburden, as Spry's Final Calculations take it, and
    // netted off the out-of-model volume (decision 0077, point 7): `Summary\Out of model` is the underburden taken into
    // ROM coal less this.
    let last_coal = (0..run.column.len()).rev().find(|&i| {
        let u = &run.column.units()[i];
        is_coal(u) && !run.is_out_of_model(u)
    });
    // ROM waste: everything else in the model but small volumes.
    for (i, &taken) in in_rom_coal.iter().enumerate() {
        let u = &run.column.units()[i];
        if taken {
            continue;
        }
        if run.is_out_of_model(u) || u.became.as_ref().is_some_and(|b| b.kind == BecameKind::SmallVolume) {
            let why = if run.is_out_of_model(u) { "the underburden left, outside the model" } else { "a small volume" };
            run.work(&[u.id], || Line::test("ROM", format!("{}: {why}", run.words_of(i)), "never written in ROM"));
            continue;
        }
        if last_coal.is_some_and(|c| i > c) && !is_coal(u) {
            let summed: Vec<f64> = u.values().iter().enumerate().map(|(f, x)| if run.column.fields().is_summed(f) { -x } else { *x }).collect();
            run.work(&[u.id], || Line::sum("out of model", format!("{}, model waste below the last coal", run.words_of(i)), -u.value(v), "m³").then("not mined: taken as underburden, netted off Summary\\Out of model"));
            moves.extend(place(run, u.values(), i, i, "not_mined", &[]));
            pieces.extend(place(run, &summed, i, i, "Summary\\Out of model", &[]));
            run.said.count("model waste below the last coal taken as underburden, out of model".into(), u.value(v));
            continue;
        }
        let onto = match (&u.became, &u.rom) {
            (Some(b), Some(r)) if b.kind == BecameKind::Loss => {
                let target = sections.iter().find(|s| Some(s.id) == r.id && !s.coal);
                let at = target.and_then(|t| if b.detail == "floor" { t.units.first() } else { t.units.last() }).and_then(|&id| run.column.position(id));
                // A loss into the underburden, outside the model, has no leaf there: it's written on its own.
                at.filter(|&at| !run.is_out_of_model(&run.column.units()[at])).unwrap_or(i)
            }
            _ => i,
        };
        run.work(&[u.id], || {
            let then = if onto == i { "ROM\\Waste and Scheduled\\Waste, on its own leaves".to_string() } else { format!("ROM\\Waste and Scheduled\\Waste, on {}'s leaves, the waste it was lost into", run.words_of(onto)) };
            Line::sum("ROM waste", run.words_of(i), u.value(v), "m³").then(then)
        });
        // Coal ending in ROM waste moved to waste: losses, thin and high ROM ash coal, discarded coal (decision 0086).
        if is_coal(u) {
            moves.extend(place(run, u.values(), i, i, "to_waste", &[]));
        }
        let values = u.values().to_vec();
        pieces.extend(place(run, &values, i, onto, "ROM\\Waste", &[]));
        pieces.extend(place(run, &values, i, onto, "Scheduled\\Waste", &[]));
    }
    land_placements(run, &pieces);
    run.rows.extend(pieces);
    run.moves.extend(moves);
    run.finalised = true;
    Ok(())
}

/// `join_benches`: each horizon's benches joined into one unit, each bench's in situ share of its volume kept.
fn join_benches(run: &mut ColumnRun<'_>, _: &DataflowStep) -> Result<(), String> {
    let before: Vec<UnitId> = run.column.units().iter().map(|u| u.id).collect();
    // Each bench as it comes in, by the block it came from: whose lines a join's working goes on.
    let mut benches: BTreeMap<u32, UnitId> = BTreeMap::new();
    for u in run.column.units().iter().filter(|u| u.bench.is_some() && u.became.is_none() && u.joined.is_none()) {
        benches.entry(u.origin).or_insert(u.id);
    }
    run.column.join_benches();
    work_joins(run, &before, &benches);
    Ok(())
}

/// The joins' workings (decision 0082, workstream 4): each bench's volume and the joined unit's, with each bench's in situ
/// share, on the benches and the unit they made.
fn work_joins(run: &ColumnRun<'_>, before: &[UnitId], benches: &BTreeMap<u32, UnitId>) {
    if run.workings.is_none() {
        return;
    }
    let v = run.column.fields().volume();
    for u in run.column.units() {
        let Some(j) = u.joined else { continue };
        if before.contains(&u.id) {
            continue;
        }
        let join = &run.column.joins()[j as usize];
        let horizon = &run.horizons[u.stack as usize];
        let ids: Vec<UnitId> = join.benches.iter().filter_map(|b| benches.get(&b.origin).copied()).chain([u.id]).collect();
        run.work(&ids, || {
            let parts: Vec<String> = join.benches.iter().map(|b| format!("bench {} {} m³", b.bench.label, vol(b.values[v]))).collect();
            let shares: Vec<String> = join.benches.iter().map(|b| format!("{} {}", b.bench.label, figure(b.share, 4))).collect();
            Line::sum("join", format!("{horizon} {}", parts.join(" + ")), u.value(v), "m³").then(format!("one unit, {horizon} ({} benches), each bench's in situ share kept to write it back: {}", join.benches.len(), shares.join(", ")))
        });
    }
}

/// The in situ density from an air-dried RD and the air-dried and in situ moistures (fractions), by Preston and
/// Sanders: RD × (1 − Mad) ÷ (1 + RD × (Mis − Mad) − Mis).
pub fn preston_sanders(rd: f64, mad: f64, mis: f64) -> f64 {
    rd * (1.0 - mad) / (1.0 + rd * (mis - mad) - mis)
}

/// `in_situ` (decision 0073, point 5): each horizon's units put back on their benches and written, a row a horizon and
/// bench; units set aside and the underburden, outside the model, left out. The densities it writes with are the
/// starter's `densities` formula's now (decision 0077): a coal unit that reaches it with none stops its block, said. A
/// dataflow with no in situ density is written on its RD's own basis, as before.
fn in_situ(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    if let Ok(rd_is) = run.column.fields().at(step.setting("rd_is")) {
        for i in 0..run.column.len() {
            let u = &run.column.units()[i];
            if !u.is_set_aside() && is_coal(u) && !u.value(rd_is).is_finite() {
                return Err(format!("{} is coal with no {}: no step worked it (the starter's densities formula does)", run.name_of(i), step.setting("rd_is")));
            }
        }
    }
    work_in_situ(run, step);
    write_pieces(run, |u| (!u.is_set_aside()).then(|| "Insitu".into()), |_| Vec::new());
    Ok(())
}

/// The in situ step's workings (decision 0082, workstream 4): each unit's volume and, where the dataflow has an in situ
/// density, its in situ tonnes, and where it's written; a unit set aside or outside the model isn't written, said.
fn work_in_situ(run: &ColumnRun<'_>, step: &DataflowStep) {
    if run.workings.is_none() {
        return;
    }
    let fields = run.column.fields();
    let v = fields.volume();
    let rd_is = fields.at(step.setting("rd_is")).ok();
    for (at, u) in run.column.units().iter().enumerate() {
        run.work(&[u.id], || {
            if run.is_out_of_model(u) {
                return Line::test("in situ", format!("{}: the underburden, outside the model", run.words_of(at)), "not written");
            }
            if let Some(b) = u.became.as_ref().filter(|_| u.is_set_aside()) {
                return Line::test("in situ", format!("{}: set aside, {}", run.words_of(at), format!("{:?}", b.kind).to_lowercase()), "not written in situ");
            }
            let folder = format!("Insitu\\{}", title(&u.in_situ.material));
            let then = match u.joined {
                Some(j) => format!("{folder}, on its {} benches by their in situ shares", run.column.joins()[j as usize].benches.len()),
                None => folder,
            };
            match rd_is.map(|d| u.value(d)).filter(|d| d.is_finite()) {
                Some(d) => Line::sum("in situ tonnes", format!("{} {} m³ × {} {} t/m³", run.words_of(at), vol(u.value(v)), step.setting("rd_is"), set(d)), u.value(v) * d, "t").then(then),
                None => Line::sum("in situ", run.words_of(at), u.value(v), "m³").then(then),
            }
        });
    }
}

/// Writes each unit `folder` names a folder for (decision 0074): a piece a bench it holds, a joined unit's summed fields
/// shared out by its benches' in situ shares, each on the leaf of the block it came from, under `folder\Material` (the
/// unit's material from the seam stack), or the folder as named where it already ends in a material or kind. The
/// underburden, outside the model, is never written.
fn write_pieces(run: &mut ColumnRun<'_>, folder: impl Fn(&Unit) -> Option<String>, texts: impl Fn(&Unit) -> Vec<(String, String)>) {
    let fields = run.column.fields();
    let summed: Vec<bool> = (0..fields.len()).map(|f| fields.is_summed(f)).collect();
    let mut pieces = Vec::new();
    for u in run.column.units() {
        if run.is_out_of_model(u) {
            continue;
        }
        let Some(top) = folder(u) else { continue };
        // A folder of its own (Loss\Thin) as named; a stage gets the material below it.
        let folder = if top.contains('\\') { top } else { format!("{top}\\{}", title(&u.in_situ.material)) };
        let mut texts = texts(u);
        texts.extend(word_texts(u, &folder));
        let shares: Vec<(u32, f64)> = match u.joined {
            Some(j) => run.column.joins()[j as usize].benches.iter().map(|b| (b.origin, b.share)).collect(),
            None => vec![(u.origin, 1.0)],
        };
        for (origin, share) in shares {
            let values = u.values().iter().zip(&summed).map(|(v, s)| if *s { v * share } else { *v }).collect();
            let attributes = &run.attributes[origin as usize];
            let leaf = run.lower.iter().map(|l| attribute_ci(attributes, l).map(attribute_word).unwrap_or_default()).collect();
            pieces.push(Piece { folder: folder.clone(), unit: u.id, leaf, values, texts: texts.clone() });
        }
    }
    run.rows.extend(pieces);
}

/// A material as a folder: its first letter capital, as Spry's Coal and Waste.
fn title(material: &str) -> String {
    let mut c = material.trim().chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => "Unknown".into(),
    }
}

pub(crate) fn unit_of_kind(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Volume => "m3",
        FieldKind::Thickness => "m",
        FieldKind::Area => "m2",
        FieldKind::Length => "m",
        FieldKind::Density => "t/m3",
        FieldKind::Moisture => "%",
        _ => "",
    }
}

/// A field's unit in the table: its kind's, and a block's dip in degrees and its centroid in metres (decision 0086).
fn unit_of_field(f: &zd_reserves::Field) -> &'static str {
    match f.name.as_str() {
        "dip" => "deg",
        "centroid_x" | "centroid_y" | "centroid_z" => "m",
        _ => unit_of_kind(f.kind),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The reserves table of one case's columns (decision 0074): a row a leaf (the case, then a value a level), a column a
/// field under each folder its pieces were written to, `Insitu\Coal\volume` say, each a leaf's pieces added up by the
/// field's rule, with the field's unit. Where a folder holds an in situ density, its tonnes on the in situ basis, its
/// air-dried tonnes and each quality on the in situ basis beside them; where only an RD, its tonnes on the RD's basis.
/// A column with no value at all is left out.
fn table_of(prepared: &Prepared, ran: &[&Ran], when: &str) -> Table {
    let fields = prepared.fields.list();
    // The leaves in the order they were first written, and what each holds.
    // A leaf: its key (case and level values), each folder's pieces' values, and its words by column.
    // And what formulas placed on it by schedule field (Waste allocation plan, decision 5).
    type Leaf = (Vec<String>, BTreeMap<String, Vec<Vec<f64>>>, BTreeMap<String, String>, BTreeMap<String, f64>, BTreeMap<usize, f64>);
    let mut leaves: Vec<Leaf> = Vec::new();
    let mut at: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    let mut folders: Vec<String> = Vec::new();
    let mut text_columns: Vec<String> = Vec::new();
    for r in ran {
        for piece in &r.rows {
            let mut key = vec![r.key.case.clone()];
            key.extend(r.key.path.iter().cloned());
            key.extend(piece.leaf.iter().cloned());
            let i = *at.entry(key.clone()).or_insert_with(|| {
                leaves.push((key, BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new()));
                leaves.len() - 1
            });
            if !folders.contains(&piece.folder) {
                folders.push(piece.folder.clone());
            }
            leaves[i].1.entry(piece.folder.clone()).or_default().push(piece.values.clone());
            for (column, word) in &piece.texts {
                if !text_columns.contains(column) {
                    text_columns.push(column.clone());
                }
                let held = leaves[i].2.entry(column.clone()).or_default();
                if held.is_empty() {
                    *held = word.clone();
                } else if !held.split("; ").any(|w| w == word) {
                    held.push_str("; ");
                    held.push_str(word);
                }
            }
        }
        // What moved between coal and waste, on the leaves of the units it moved (decision 0086).
        let v = prepared.fields.volume();
        for piece in &r.moves {
            let mut key = vec![r.key.case.clone()];
            key.extend(r.key.path.iter().cloned());
            key.extend(piece.leaf.iter().cloned());
            let i = *at.entry(key.clone()).or_insert_with(|| {
                leaves.push((key, BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new()));
                leaves.len() - 1
            });
            *leaves[i].3.entry(piece.folder.clone()).or_default() += piece.values[v];
        }
        for p in &r.placed {
            let mut key = vec![r.key.case.clone()];
            key.extend(r.key.path.iter().cloned());
            key.extend(p.leaf.iter().cloned());
            let i = *at.entry(key.clone()).or_insert_with(|| {
                leaves.push((key, BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new()));
                leaves.len() - 1
            });
            *leaves[i].4.entry(p.field).or_default() += p.value;
        }
    }
    let mut columns = vec![TableColumn::text("case", leaves.iter().map(|l| Some(l.0[0].clone())).collect())];
    for (n, level) in prepared.upper.iter().chain(&prepared.lower).enumerate() {
        columns.push(TableColumn::text(level, leaves.iter().map(|l| Some(l.0[n + 1].clone()).filter(|v| !v.is_empty())).collect()));
    }
    let setting = |key: &str| prepared.steps.iter().find(|(n, _, _)| n == "in_situ").map_or(key.to_string(), |(_, _, s)| s.setting(key).to_string());
    let field = |key: &str| fields.iter().position(|f| f.name == setting(key));
    let v = prepared.fields.volume();
    for folder in &folders {
        // Each leaf's pieces in this folder, added up by the fields' rules.
        let added: Vec<Option<Vec<f64>>> = leaves
            .iter()
            .map(|l| {
                l.1.get(folder).map(|pieces| {
                    let refs: Vec<&[f64]> = pieces.iter().map(Vec::as_slice).collect();
                    zd_reserves::add_up_values(&prepared.fields, &refs)
                })
            })
            .collect();
        let value = |f: usize| added.iter().map(|a| a.as_ref().map(|a| a[f]).filter(|x| x.is_finite())).collect::<Vec<_>>();
        let mut push = |column: TableColumn| {
            if let zd_core::ColumnValues::Number(values) = &column.values
                && values.iter().any(Option::is_some)
            {
                columns.push(column);
            }
        };
        // ROM and Scheduled waste keep what waste has (decision 0086): no qualities or moistures, which are coal's.
        let waste_out = folder == "ROM\\Waste" || folder == "Scheduled\\Waste";
        for (i, f) in fields.iter().enumerate() {
            if waste_out && matches!(f.kind, FieldKind::Quality | FieldKind::Moisture | FieldKind::MoistureFactor) {
                continue;
            }
            // A schedule field is written under Scheduled only, by its own name (Waste allocation plan, decision 5).
            if zd_core::is_schedule_field(&f.name) {
                continue;
            }
            // A thickness adds down a column and an area is a column's largest; across leaves neither adds up, so a
            // summary shows neither.
            // So is a block's dip (decision 0086): the Reserves table tab adds these up by block.
            let across = if matches!(f.kind, FieldKind::Thickness | FieldKind::Area | FieldKind::Length) || f.name == "dip" { None } else { Some(f.add_up.clone()) };
            push(TableColumn { name: format!("{folder}\\{}", f.name), unit: unit_of_field(f).to_string(), add_up: across, kind: zd_core::ColumnKind::Written, values: zd_core::ColumnValues::Number(value(i)) });
        }
        let tonnes = |d: usize| added.iter().map(|a| a.as_ref().map(|a| a[v] * a[d]).filter(|x| x.is_finite())).collect::<Vec<_>>();
        let named = |n: &str| format!("{folder}\\{n}");
        // ROM, Scheduled and clean coal on the ROM basis: tonnes by the ROM density, qualities × (1 − Mrom) ÷ (1 − Mad).
        let rom_basis = ["ROM", "Scheduled"].iter().any(|top| folder.starts_with(&format!("{top}\\")));
        if let (true, Some(rd), Some(rd_rom)) = (rom_basis, field("rd"), field("rd_rom"))
            && added.iter().flatten().any(|a| a[rd_rom].is_finite())
        {
            push(TableColumn::adding(&named("tonnes"), "t", AddUp::Sum, tonnes(rd_rom)));
            push(TableColumn::adding(&named("tonnes_ad"), "t", AddUp::Sum, tonnes(field(super::TONNES_AD_PER_M3).unwrap_or(rd))));
            if let (false, Some(mad), Some(mrom)) = (waste_out, field("mad"), field("mrom")) {
                for (q, f) in fields.iter().enumerate().filter(|(_, f)| f.kind == FieldKind::Quality) {
                    let values = added.iter().map(|a| a.as_ref().map(|a| a[q] * (1.0 - a[mrom] / 100.0) / (1.0 - a[mad] / 100.0)).filter(|x| x.is_finite())).collect();
                    push(TableColumn::adding(&named(&format!("{}_rom", f.name)), "", AddUp::Mass { density: named(&setting("rd_rom")) }, values));
                }
            }
            continue;
        }
        match (field("rd"), field("rd_is")) {
            (Some(rd), Some(rd_is)) if added.iter().flatten().any(|a| a[rd_is].is_finite()) => {
                push(TableColumn::adding(&named("tonnes"), "t", AddUp::Sum, tonnes(rd_is)));
                push(TableColumn::adding(&named("tonnes_ad"), "t", AddUp::Sum, tonnes(field(super::TONNES_AD_PER_M3).unwrap_or(rd))));
                if let (Some(mad), Some(mis)) = (field("mad"), field("mis")) {
                    for (q, f) in fields.iter().enumerate().filter(|(_, f)| f.kind == FieldKind::Quality) {
                        let values = added.iter().map(|a| a.as_ref().map(|a| a[q] * (1.0 - a[mis] / 100.0) / (1.0 - a[mad] / 100.0)).filter(|x| x.is_finite())).collect();
                        push(TableColumn::adding(&named(&format!("{}_is", f.name)), "", AddUp::Mass { density: named(&setting("rd_is")) }, values));
                    }
                }
            }
            _ => {
                if let Some(d) = fields.iter().position(|f| f.kind == FieldKind::Density) {
                    push(TableColumn::adding(&named("tonnes"), "t", AddUp::Sum, tonnes(d)));
                }
            }
        }
    }
    columns.extend(schedule_columns(prepared, &leaves.iter().map(|l| (&l.1, &l.4)).collect::<Vec<_>>()));
    for column in &text_columns {
        columns.push(TableColumn::text(column, leaves.iter().map(|l| l.2.get(column).cloned().filter(|w| !w.is_empty())).collect()));
    }
    if ran.iter().any(|r| r.finalised) {
        let moved: Vec<&BTreeMap<String, f64>> = leaves.iter().map(|l| &l.3).collect();
        let written: Vec<&BTreeMap<String, Vec<Vec<f64>>>> = leaves.iter().map(|l| &l.1).collect();
        columns.extend(balance_columns(&written, &moved, v));
    }
    columns.extend(ratio_columns(&columns));
    let mut audit = vec![("run", when.to_string()), ("dataflow", prepared.audit_dataflow.clone()), ("parameters", prepared.audit_tables.clone())];
    // Read from a table, not solids (decision 12 of 0099): its volumes are imported, not of record.
    if let Some(from) = &prepared.imported_from {
        audit.push(("input", format!("imported from {from}: volumes not of record")));
    }
    for (name, words) in audit {
        columns.push(TableColumn::text(&format!("Summary\\Audit\\{name}"), leaves.iter().map(|_| Some(words.clone()).filter(|w| !w.is_empty())).collect()));
    }
    Table { columns, calculated: Vec::new(), setup: None, objects: false }
}

/// A leaf's pieces by the folder they were written to.
type Folders = BTreeMap<String, Vec<Vec<f64>>>;

/// The schedule fields' columns (Waste allocation plan, decision 5): each field named with a folder written once, as
/// `Scheduled\<its name>` (`Scheduled\Waste\Dragline\Prime Volume`), a leaf's Scheduled pieces, coal and waste, added up
/// by the field's rule, with what formulas placed onto the leaf added in. A leaf with nothing scheduled and nothing placed
/// has none; a field no leaf has a value for has no column.
fn schedule_columns(prepared: &Prepared, leaves: &[(&Folders, &BTreeMap<usize, f64>)]) -> Vec<TableColumn> {
    let fields = prepared.fields.list();
    let mut columns = Vec::new();
    for (i, f) in fields.iter().enumerate().filter(|(_, f)| zd_core::is_schedule_field(&f.name)) {
        let values: Vec<Option<f64>> = leaves
            .iter()
            .map(|(folders, placed)| {
                let pieces: Vec<&[f64]> = folders.iter().filter(|(folder, _)| folder.starts_with("Scheduled\\")).flat_map(|(_, p)| p.iter().map(Vec::as_slice)).collect();
                let own = (!pieces.is_empty()).then(|| zd_reserves::add_up_values(&prepared.fields, &pieces)[i]).filter(|x| x.is_finite());
                match (own, placed.get(&i)) {
                    (Some(x), Some(p)) => Some(x + p),
                    (None, Some(p)) => Some(*p),
                    (own, None) => own,
                }
            })
            .collect();
        if values.iter().any(Option::is_some) {
            columns.push(TableColumn { name: format!("Scheduled\\{}", f.name), unit: unit_of_schedule_field(f).to_string(), add_up: Some(f.add_up.clone()), kind: zd_core::ColumnKind::Written, values: zd_core::ColumnValues::Number(values) });
        }
    }
    columns
}

/// A schedule field's unit in the table: its kind's, or for a field of no kind of its own (`other`), what its last name
/// says, as Spry's sections name them: a volume in m³, a thickness in m, tonnes in t.
fn unit_of_schedule_field(f: &zd_reserves::Field) -> &'static str {
    let last = f.name.rsplit('\\').next().unwrap_or_default().to_ascii_lowercase();
    match unit_of_field(f) {
        "" if last.contains("volume") => "m3",
        "" if last.contains("thickness") => "m",
        "" if last.contains("tonnes") => "t",
        unit => unit,
    }
}

/// The table's balance (decision 0086): coal, waste and the two together, each with its volume at every stage, what
/// moved between them and in from or out of the model, and each stage's check against the one before, which is zero for
/// every block. A leaf can be off where material moves between seams; read at the block and above.
fn balance_columns(written: &[&BTreeMap<String, Vec<Vec<f64>>>], moved: &[&BTreeMap<String, f64>], v: usize) -> Vec<TableColumn> {
    let n = written.len();
    // A stage's volume on each leaf: its coal folder, or every other material's (waste, partings) together.
    let stage = |top: &str, coal: bool| -> Vec<f64> {
        written
            .iter()
            .map(|folders| {
                folders
                    .iter()
                    .filter(|(f, _)| f.split_once('\\').is_some_and(|(t, m)| t == top && !m.contains('\\') && (m == "Coal") == coal && m != "Clean coal"))
                    .flat_map(|(_, pieces)| pieces.iter().map(|p| p[v]))
                    .sum()
            })
            .collect()
    };
    let moves = |what: &str| -> Vec<f64> { moved.iter().map(|m| m.get(what).copied().unwrap_or(0.0)).collect() };
    // A dataflow with no in situ step writes no Insitu: ROM is checked against what came in less small volumes.
    let has_insitu = written.iter().any(|folders| folders.keys().any(|f| f.starts_with("Insitu\\")));
    let (to_waste, to_coal, from_outside, not_mined) = (moves("to_waste"), moves("to_coal"), moves("from_outside"), moves("not_mined"));
    let zero = vec![0.0; n];
    let mut columns = Vec::new();
    for part in ["Coal", "Waste", "Total"] {
        let of = |top: &str| -> Vec<f64> {
            match part {
                "Coal" => stage(top, true),
                "Waste" => stage(top, false),
                _ => stage(top, true).iter().zip(stage(top, false)).map(|(a, b)| a + b).collect(),
            }
        };
        let (imported, small, rom, scheduled) = (of("Imported"), of("Small volume"), of("ROM"), of("Scheduled"));
        let insitu: Vec<f64> = if has_insitu { of("Insitu") } else { (0..n).map(|i| imported[i] - small[i]).collect() };
        // Coal sends to waste and takes to coal; waste the other way; the total only gains from outside and loses what
        // isn't mined.
        let (sign_waste, sign_coal, outside, unmined) = match part {
            "Coal" => (-1.0, 1.0, &from_outside, &zero),
            "Waste" => (1.0, -1.0, &zero, &not_mined),
            _ => (0.0, 0.0, &from_outside, &not_mined),
        };
        let unaccounted_insitu: Vec<f64> = (0..n).map(|i| imported[i] - small[i] - insitu[i]).collect();
        let unaccounted_rom: Vec<f64> = (0..n).map(|i| insitu[i] + sign_waste * to_waste[i] + sign_coal * to_coal[i] + outside[i] - unmined[i] - rom[i]).collect();
        let unaccounted_scheduled: Vec<f64> = (0..n).map(|i| rom[i] - scheduled[i]).collect();
        let shown = |x: &[f64]| x.iter().map(|&x| (x != 0.0).then_some(x)).collect::<Vec<_>>();
        let always = |x: &[f64]| x.iter().map(|&x| Some(x)).collect::<Vec<_>>();
        let mut add = |name: &str, values: Vec<Option<f64>>| {
            if values.iter().any(Option::is_some) {
                columns.push(TableColumn::adding(&format!("Summary\\Balance\\{part}\\{name}"), "m3", AddUp::Sum, values));
            }
        };
        add("imported", shown(&imported));
        add("small_volume", shown(&small));
        if has_insitu {
            add("insitu", shown(&insitu));
        }
        if part != "Total" {
            add("to_waste", shown(&to_waste));
            add("to_coal", shown(&to_coal));
        }
        add("from_outside", shown(outside));
        add("not_mined", shown(unmined));
        add("rom", shown(&rom));
        add("scheduled", shown(&scheduled));
        if has_insitu {
            add("unaccounted_insitu", always(&unaccounted_insitu));
        }
        add("unaccounted_rom", always(&unaccounted_rom));
        add("unaccounted_scheduled", always(&unaccounted_scheduled));
    }
    columns
}

/// The table's ratios (decision 0086), each a ratio of sums so it adds up right: the strip ratio in ROM (ROM waste bcm
/// per ROM coal tonne) and in situ, and the clean coal yield (ROM clean coal tonnes per ROM coal tonne), with the sums
/// they're made of beside them.
fn ratio_columns(columns: &[TableColumn]) -> Vec<TableColumn> {
    let numbers = |name: &str| columns.iter().find(|c| c.name == name).and_then(|c| match &c.values {
        zd_core::ColumnValues::Number(v) => Some(v.clone()),
        _ => None,
    });
    let mut out = Vec::new();
    let ratio = |out: &mut Vec<TableColumn>, name: &str, unit: &str, (of, of_name, of_unit): (&str, &str, &str), (per, per_name, per_unit): (&str, &str, &str)| {
        let (Some(top), Some(bottom)) = (numbers(of), numbers(per)) else { return };
        let at = |n: &str| format!("Summary\\Ratios\\{n}");
        for (n, u, values) in [(of_name, of_unit, &top), (per_name, per_unit, &bottom)] {
            if !out.iter().any(|c: &TableColumn| c.name == at(n)) {
                out.push(TableColumn::adding(&at(n), u, AddUp::Sum, values.clone()));
            }
        }
        let values = top.iter().zip(&bottom).map(|(a, b)| match (a, b) {
            (Some(a), Some(b)) if *b != 0.0 => Some(a / b),
            _ => None,
        });
        out.push(TableColumn::adding(&at(name), unit, AddUp::Ratio { of: of_name.into(), per: per_name.into() }, values.collect()));
    };
    ratio(&mut out, "strip_ratio", "bcm/t", ("ROM\\Waste\\volume", "rom_waste", "m3"), ("ROM\\Coal\\tonnes", "rom_coal", "t"));
    ratio(&mut out, "insitu_strip_ratio", "bcm/t", ("Insitu\\Waste\\volume", "insitu_waste", "m3"), ("Insitu\\Coal\\tonnes", "insitu_coal", "t"));
    ratio(&mut out, "clean_coal_yield", "t/t", ("ROM\\Clean coal\\tonnes", "clean_coal", "t"), ("ROM\\Coal\\tonnes", "rom_coal", "t"));
    out
}

/// A moment as the audit says it: the date and time, UTC (`2026-11-18 09:31 UTC`).
pub(crate) fn utc_words(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Days to a civil date (Howard Hinnant's days_from_civil, inverted).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02} UTC", rest / 3_600, rest % 3_600 / 60)
}

/// A run again into a table (decision 0085): the table the run made for the case taken into the one given, its rows for
/// the blocks run replaced and every other row kept, as that table's next revision.
fn into_table(out: &mut Outcome, project: &zd_core::Project, object: &Arc<Object>, prepared: &Prepared, p: &RunReservesParams) -> Result<(), CommandError> {
    let Body::Table(old) = &object.body else { unreachable!("checked as a table") };
    let made = std::mem::take(&mut out.changes);
    let Some(Change::Create { body: Body::Table(new), .. }) = made.into_iter().next() else {
        return Err(CommandError::Invalid(format!("no block of {} ran, so nothing was run again", object.name)));
    };
    let case = crate::reserves_view::case_of(&object.attributes);
    // The columns the case's row dataflows write (Reserves case plan, decision 6): kept by each leaf's key, as entered
    // values are, until those dataflows run again; never written over by this run, which doesn't make them.
    let by_key = old.setup.as_ref().map(|s| super::row_columns_of(project, s)).unwrap_or_default();
    let (mut merged, lost) = merge_rerun(old, &new, &prepared.upper, &case, &p.only, &by_key);
    // A table made before setups (Table setup plan, decision 1) takes the one its run was made from.
    if merged.setup.is_none() {
        merged.setup = new.setup.clone();
    }
    let kept = merged.rows() - new.rows();
    out.changes = vec![Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(merged))) }];
    let under = if p.only.is_empty() { "every block".to_string() } else { p.only.join(" › ") };
    out.notes.push(Note::info(format!("{}: {under} run again, {} lea{} written, {kept} kept as they were", object.name, new.rows(), if new.rows() == 1 { "f" } else { "ves" })));
    // Values a person entered on a leaf the run no longer makes: said with each value, never dropped silently (0099).
    if !lost.is_empty() {
        out.notes.push(Note::warning(Code::ReservesEnteredLost, format!("{} entered value{} on leaves this run no longer makes, not kept: {}", lost.len(), plural(lost.len()), lost.join("; "))));
    }
    Ok(())
}

/// `old` with its rows of `case` whose levels above the seam start with `only` taken out and `new`'s rows added after the
/// rest; columns by name, `old`'s first then any only `new` has, a value missing on either side left empty. A column a
/// person entered (decision 2 of 0099) keeps its values on the rows run again, matched by each row's key, its case and
/// its levels' words; a value whose leaf the run no longer makes is given back as `leaf: column = value`. The columns in
/// `by_key` (a case's row dataflows') are kept by each row's key the same way, a leaf gone taking its value with it.
pub(crate) fn merge_rerun(old: &Table, new: &Table, upper: &[String], case: &str, only: &[String], by_key: &[String]) -> (Table, Vec<String>) {
    let word = |t: &Table, name: &str, r: usize| match t.column(name).map(|c| &c.values) {
        Some(zd_core::ColumnValues::Text(v)) => v[r].clone().unwrap_or_default(),
        _ => String::new(),
    };
    let rerun = |r: usize| word(old, "case", r) == case && only.iter().zip(upper).all(|(w, level)| word(old, level, r) == *w);
    let keep: Vec<usize> = (0..old.rows()).filter(|&r| !rerun(r)).collect();
    // Each row's key: its case and its levels' words, the run's levels (an entered column is none of them).
    let levels = crate::reserves_view::levels(new);
    let key = |t: &Table, r: usize| std::iter::once(word(t, "case", r)).chain(levels.iter().map(|l| word(t, l, r))).collect::<Vec<_>>();
    let old_at: BTreeMap<Vec<String>, usize> = (0..old.rows()).filter(|&r| rerun(r)).map(|r| (key(old, r), r)).collect();
    let new_at: Vec<Option<usize>> = (0..new.rows()).map(|r| old_at.get(&key(new, r)).copied()).collect();
    let mut lost = Vec::new();
    let mut names: Vec<&TableColumn> = old.columns.iter().collect();
    names.extend(new.columns.iter().filter(|c| old.column(&c.name).is_none()));
    let columns = names
        .into_iter()
        .map(|like| {
            let from_old = old.column(&like.name);
            let from_new = new.column(&like.name);
            let entered = !like.kind.is_written();
            let keyed = entered || by_key.contains(&like.name);
            // A row run again: the run's value, or for an entered column the value its key had before.
            let source = |r: usize| if keyed { new_at[r].map(|o| (from_old, o)) } else { Some((from_new, r)) };
            if entered {
                let taken: std::collections::BTreeSet<usize> = new_at.iter().flatten().copied().collect();
                for (k, &r) in &old_at {
                    let value = match from_old.map(|c| &c.values) {
                        Some(zd_core::ColumnValues::Number(v)) => v[r].map(|x| x.to_string()),
                        Some(zd_core::ColumnValues::Text(v)) => v[r].clone(),
                        None => None,
                    };
                    if let Some(value) = value.filter(|_| !taken.contains(&r)) {
                        lost.push(format!("{}: {} = {value}", k.iter().filter(|w| !w.is_empty()).cloned().collect::<Vec<_>>().join(" "), like.name));
                    }
                }
            }
            let values = match &like.values {
                zd_core::ColumnValues::Number(_) => {
                    let get = |c: Option<&TableColumn>, r: usize| match c.map(|c| &c.values) {
                        Some(zd_core::ColumnValues::Number(v)) => v[r],
                        _ => None,
                    };
                    zd_core::ColumnValues::Number(keep.iter().map(|&r| get(from_old, r)).chain((0..new.rows()).map(|r| source(r).and_then(|(c, at)| get(c, at)))).collect())
                }
                zd_core::ColumnValues::Text(_) => {
                    let get = |c: Option<&TableColumn>, r: usize| match c.map(|c| &c.values) {
                        Some(zd_core::ColumnValues::Text(v)) => v[r].clone(),
                        _ => None,
                    };
                    zd_core::ColumnValues::Text(keep.iter().map(|&r| get(from_old, r)).chain((0..new.rows()).map(|r| source(r).and_then(|(c, at)| get(c, at)))).collect())
                }
            };
            let newest = from_new.unwrap_or(like);
            TableColumn { name: like.name.clone(), unit: newest.unit.clone(), add_up: newest.add_up.clone(), kind: like.kind, values }
        })
        .collect();
    (Table { columns, calculated: old.calculated.clone(), setup: old.setup.clone(), objects: old.objects }, lost)
}

/// What a reserves table was made from that has changed since (decision 4 of 0099): its blocks revised since the run
/// that last wrote the table read them (an attribute typed in the grid, a block cut again), anything else it read by
/// name (its dataflow, a parameter table, the seam model), and how many of its inputs are gone. Nothing runs again by
/// itself (rule 4): the Tables tab says so, with Run again (0085).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ChangedSince {
    pub blocks: usize,
    pub others: Vec<String>,
    pub gone: usize,
}

impl ChangedSince {
    pub fn is_empty(&self) -> bool {
        self.blocks == 0 && self.others.is_empty() && self.gone == 0
    }
}

pub fn changed_since(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId) -> Result<ChangedSince, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let record = table_run(log, object).ok_or_else(|| CommandError::Invalid(format!("{} wasn't made by a reserves run in this project's log", object.name)))?;
    let mut changed = ChangedSince::default();
    for input in &record.inputs {
        if input.id == table {
            continue;
        }
        match project.object(input.id) {
            Some(o) if o.revision != input.revision => {
                if matches!(o.body, Body::Solid(_)) {
                    changed.blocks += 1;
                } else {
                    changed.others.push(o.name.clone());
                }
            }
            None => changed.gone += 1,
            _ => {}
        }
    }
    Ok(changed)
}

/// The settings to run a reserves table's blocks under `only` again into it (decision 0085): its own run's, read from the
/// log as the column view reads them, the blocks still there, the table and the run that last wrote it named.
pub fn rerun_params(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId, only: &[String]) -> Result<RunReservesParams, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let record = table_run(log, object);
    let setup = setup_of(project, log, table)?;
    let blocks = if setup.input.is_some() { Vec::new() } else { setup.blocks.resolve(project.objects()) };
    if blocks.is_empty() && setup.input.is_none() {
        return Err(CommandError::Invalid(format!("{}'s setup picks no blocks: nothing to run again", object.name)));
    }
    // The run's own settings the setup doesn't hold (the layer and the name), from the table itself.
    let name = record.and_then(|r| serde_json::from_value::<RunReservesParams>(r.params.clone()).ok()).map_or_else(|| object.name.clone(), |p| p.name);
    Ok(RunReservesParams {
        dataflow: setup.dataflow,
        blocks,
        model: setup.model,
        stack: setup.stack,
        layer: project.layer(object.layer).map_or_else(|| object.layer.to_string(), |l| l.name.clone()),
        parameters: setup.parameters,
        name,
        table: Some(table),
        only: only.to_vec(),
        previous: record.map(|r| r.run),
        add_ons: setup.add_ons,
        sources: setup.sources,
        tables: setup.tables,
        input: setup.input,
    })
}

/// What fills a reserves table (Table setup plan, decision 1): the setup it holds, or, for a table made before setups,
/// the one the run that made it gives, its blocks picked by their layers where they were every solid on them.
pub fn setup_of(project: &zd_core::Project, log: &[crate::LogEntry], table: ObjectId) -> Result<zd_core::TableSetup, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::Table(t) = &object.body else { return Err(CommandError::WrongKind { id: table, expected: ObjectKind::Table, found: object.kind() }) };
    if let Some(setup) = &t.setup {
        return Ok(setup.clone());
    }
    let record = table_run(log, object).ok_or_else(|| CommandError::Invalid(format!("{} has no setup and wasn't made by a reserves run in this project's log, so it can't be run again", object.name)))?;
    let p: RunReservesParams = serde_json::from_value(record.params.clone()).map_err(|e| CommandError::Invalid(format!("the run that made {} can't be read again: {e}", object.name)))?;
    let blocks: Vec<ObjectId> = p.blocks.into_iter().filter(|id| project.object(*id).is_some()).collect();
    let all: Vec<&Arc<Object>> = project.objects().collect();
    Ok(zd_core::TableSetup { dataflow: p.dataflow, add_ons: p.add_ons, tables: p.tables, blocks: zd_core::BlockPick::of_blocks(all.iter().copied(), &blocks), model: p.model, stack: p.stack, parameters: p.parameters, sources: p.sources, input: p.input, row_dataflows: Vec::new() })
}

/// The reserves run that last wrote a table: the run its revision came from, or where a person has since entered values
/// in it (decision 2 of 0099), the last reserves run before that which made or revised it.
pub(crate) fn table_run<'a>(log: &'a [crate::LogEntry], object: &Object) -> Option<&'a crate::RunRecord> {
    if let Some(record) = run_of(log, object.provenance) {
        return Some(record);
    }
    let at = log.iter().position(|e| matches!(e, crate::LogEntry::Run(r) if r.run == object.provenance)).unwrap_or(log.len());
    log[..at].iter().rev().find_map(|e| match e {
        crate::LogEntry::Run(r) if r.command == RunReserves::NAME && r.outputs.iter().any(|o| o.id == object.id) => Some(r),
        _ => None,
    })
}

/// The reserves run with this id in the log.
fn run_of(log: &[crate::LogEntry], run: zd_core::RunId) -> Option<&crate::RunRecord> {
    log.iter().rev().find_map(|e| match e {
        crate::LogEntry::Run(r) if r.run == run && r.command == RunReserves::NAME => Some(r),
        _ => None,
    })
}

/// The run that last wrote a column of a table (decision 0085): the table's own run, else, where that ran a part of it
/// again that the column isn't in, the run before it, and so on back to one that wrote it.
fn run_for_column<'a>(log: &'a [crate::LogEntry], latest: zd_core::RunId, column: &[String]) -> Option<&'a crate::RunRecord> {
    let mut at = run_of(log, latest)?;
    for _ in 0..log.len() {
        let Ok(p) = serde_json::from_value::<RunReservesParams>(at.params.clone()) else { return Some(at) };
        match (p.table, p.previous) {
            (Some(_), Some(previous)) if !column.starts_with(&p.only) => match run_of(log, previous) {
                Some(before) => at = before,
                None => return Some(at),
            },
            _ => return Some(at),
        }
    }
    Some(at)
}

fn outcome(prepared: &Prepared, p: &RunReservesParams, ran: Vec<Ran>, stopped: Vec<Stopped>, started: Instant, when: &str, setup: &zd_core::TableSetup) -> Outcome {
    let mut cases: BTreeMap<&str, Vec<&Ran>> = BTreeMap::new();
    for r in &ran {
        cases.entry(r.key.case.as_str()).or_default().push(r);
    }
    let mut changes = Vec::new();
    let mut messages = Vec::new();
    for (case, of) in &cases {
        let mut table = table_of(prepared, of, when);
        table.setup = Some(setup.clone());
        let rows = table.rows();
        let name = p.name.replace("{dataflow}", &prepared.name).replace("{case}", case);
        // Its levels and their defaults, so a leaf can be matched to its solids as the run placed them (decision 0075).
        let levels = prepared.dataflow.levels.iter().map(|l| match &l.default {
            Some(d) => format!("{}={d}", l.attribute),
            None => l.attribute.clone(),
        });
        let attributes: Attributes = [("levels".to_string(), AttrValue::Text(levels.collect::<Vec<_>>().join(";")))].into_iter().collect();
        changes.push(Change::Create { name: name.clone(), layer: p.layer.clone(), attributes, body: Body::Table(Arc::new(table)) });
        messages.push(format!("{name}: {} block{}, {rows} lea{}", of.len(), plural(of.len()), if rows == 1 { "f" } else { "ves" }));
    }
    let steps: Vec<&str> = prepared.steps.iter().map(|(n, _, _)| n.as_str()).collect();
    let mut head = format!(
        "{} of {} block{} ran through {}",
        ran.len(),
        ran.len() + stopped.len(),
        plural(ran.len() + stopped.len()),
        if steps.is_empty() { "no steps".to_string() } else { steps.join(", ") }
    );
    // Plus 0.0 so no columns says 0.0, never -0.0.
    let volume: f64 = ran.iter().map(|r| r.input_volume).sum::<f64>() + 0.0;
    let worst = ran.iter().map(|r| (r.imbalance / r.input_volume.max(1.0)).abs()).fold(0.0, f64::max);
    head.push_str(&format!("; {volume:.1} m3 in, every column balanced to {worst:.1e} of its volume; {:.0} ms", started.elapsed().as_secs_f64() * 1000.0));
    // The balance after every run (decision 0077): what came out of each column is what went in, to rounding.
    let unbalanced: Vec<String> = ran
        .iter()
        .filter(|r| (r.imbalance / r.input_volume.max(1.0)).abs() > BALANCE_TOLERANCE)
        .map(|r| format!("{} ({:+.3} m3)", r.key.words(), r.imbalance))
        .collect();
    messages.insert(0, head);
    let mut outcome = Outcome::new(changes);
    for m in messages {
        outcome = outcome.message(m);
    }
    let mut counts: BTreeMap<&str, (usize, f64)> = BTreeMap::new();
    let mut named: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for r in &ran {
        for (what, (n, v)) in &r.said.counts {
            let c = counts.entry(what.as_str()).or_default();
            c.0 += n;
            c.1 += v;
        }
        for (check, blocks) in &r.said.named {
            named.entry(check.as_str()).or_default().extend(blocks.iter().map(String::as_str));
        }
    }
    if !unbalanced.is_empty() {
        let n = unbalanced.len();
        let mut words: Vec<String> = unbalanced.iter().take(NAMED).cloned().collect();
        if n > NAMED {
            words.push(format!("and {} more", n - NAMED));
        }
        outcome.notes.push(Note::warning(Code::ReservesUnbalanced, format!("{n} column{} didn't keep its volume: {}", plural(n), words.join(", "))));
    }
    for (what, (n, v)) in counts {
        let code = if what.starts_with("printed by ") {
            Code::ReservesFormulaSaid
        } else if what.contains("small volume") {
            Code::ReservesSmallVolume
        } else if what.contains("underburden") {
            Code::ReservesUnderburden
        } else {
            Code::ReservesDefaulted
        };
        outcome.notes.push(Note::new(Severity::Info, Some(code), format!("{n} unit{} {what}, {v:.1} m3", plural(n))));
    }
    for (check, blocks) in named {
        let mut words: Vec<String> = blocks.iter().take(NAMED).map(|b| b.to_string()).collect();
        if blocks.len() > NAMED {
            words.push(format!("and {} more", blocks.len() - NAMED));
        }
        let what = if check.starts_with("ROM section") || check.starts_with("short of") { "" } else { " block" };
        let n = blocks.len();
        let label = if what.is_empty() { format!("{n} {check}") } else { format!("{n}{what}{} {check}", plural(n)) };
        outcome.notes.push(Note::warning(Code::ReservesOutOfRange, format!("{label}: {}", words.join(", "))));
    }
    if !stopped.is_empty() {
        let mut words: Vec<String> = stopped.iter().take(NAMED).map(|s| s.why.clone()).collect();
        if stopped.len() > NAMED {
            words.push(format!("and {} more", stopped.len() - NAMED));
        }
        outcome.notes.push(Note::warning(Code::ReservesBlockStopped, format!("{} block{} left out: {}", stopped.len(), plural(stopped.len()), words.join("; "))));
    }
    if !prepared.unplaced.is_empty() {
        let n = prepared.unplaced.len();
        let mut levels: Vec<&str> = prepared.unplaced.iter().map(|(_, l)| l.as_str()).collect();
        levels.sort();
        levels.dedup();
        let named: Vec<&str> = prepared.unplaced.iter().take(NAMED).map(|(b, _)| b.as_str()).collect();
        outcome.notes.push(Note::warning(Code::ReservesBlockStopped, format!("{n} solid{} with no {} and no default for it left out: {}", plural(n), levels.join(" or "), named.join(", "))));
    }
    // A field read from blocks that some have no value for, said once with how many.
    for (i, f) in prepared.dataflow.fields.iter().enumerate() {
        if !matches!(f.source, FieldSource::Attribute { .. } | FieldSource::Parameter { .. }) {
            continue;
        }
        let missing = ran.iter().flat_map(|r| r.rows.iter()).filter(|piece| piece.folder.starts_with("Insitu\\") && piece.values[i].is_nan()).count();
        if missing > 0 {
            outcome.notes.push(Note::new(Severity::Info, Some(Code::ReservesValueMissing), format!("{missing} in situ piece{} with no {}: no block attribute or parameter row gave it", plural(missing), f.field.name)));
        }
    }
    outcome
}

/// A starter dataflow and parameter table from a seam model (decision 0073, point 9), so a dataflow is never built field
/// by field: volume, thickness and plan area from each block; the density and every other grid of the working section
/// from the interrogation, qualities by tonnes; every step built, in order; and a parameter table with every column
/// those steps read, a waste and a coal row to fill in, and a default row holding the underburden's 10 m.
pub fn dataflow_starter(model: &zd_core::SeamModel) -> (ParameterTable, Dataflow) {
    use zd_core::{DataflowField, ParameterRow, ValueColumn};
    use zd_reserves::Field as EngineField;
    let density = model.density.clone();
    let mut qualities: Vec<String> = Vec::new();
    for h in model.working_section() {
        for code in h.grids.keys() {
            if *code != density && !qualities.contains(code) {
                qualities.push(code.clone());
            }
        }
    }
    let field = |name: &str, kind, add_up, source| DataflowField { field: EngineField::new(name, kind, add_up), source };
    // Geometry as solid.measure wrote it onto the blocks (decision 0074): the dataflow reads the data table.
    let measured = |name: &str| FieldSource::Attribute { attribute: name.to_string() };
    let mut fields = vec![
        field("volume", FieldKind::Volume, AddUp::Sum, measured("volume")),
        // True thickness: what the seam rules and loss and dilution thicknesses are about (Tim, 15 November 2026).
        field("thickness", FieldKind::Thickness, AddUp::Sum, measured("true_thickness")),
        field("area", FieldKind::Area, AddUp::Max, measured("plan_area")),
        field("length", FieldKind::Length, AddUp::Max, measured("length")),
        // The rest of a block's geometry as solid.measure wrote it (decision 0086): its width and dip, a column's largest
        // (a seam's benches share one dip), and its centroid, by volume.
        field("width", FieldKind::Length, AddUp::Max, measured("width")),
        field("dip", FieldKind::Other, AddUp::Max, measured("dip")),
        field("centroid_x", FieldKind::Other, AddUp::Volume, measured("centroid_x")),
        field("centroid_y", FieldKind::Other, AddUp::Volume, measured("centroid_y")),
        field("centroid_z", FieldKind::Other, AddUp::Volume, measured("centroid_z")),
        field(&density, FieldKind::Density, AddUp::Volume, FieldSource::Attribute { attribute: density.clone() }),
    ];
    for q in &qualities {
        if ["mad", "mis", "mrom"].contains(&q.as_str()) {
            continue;
        }
        fields.push(field(q, FieldKind::Quality, AddUp::Mass { density: density.clone() }, FieldSource::Attribute { attribute: q.clone() }));
    }
    // Moistures (%): air dried and in situ from a grid where the model has one, else a parameter row; ROM from a row. The
    // densities on the in situ and ROM bases the in situ step works, each moisture averaged by the tonnes on its basis.
    fields.push(field("mad", FieldKind::Moisture, AddUp::Mass { density: density.clone() }, FieldSource::Attribute { attribute: "mad".into() }));
    fields.push(field("rd_is", FieldKind::Density, AddUp::Volume, FieldSource::Worked));
    fields.push(field("rd_rom", FieldKind::Density, AddUp::Volume, FieldSource::Worked));
    fields.push(field("mis", FieldKind::Moisture, AddUp::Mass { density: "rd_is".into() }, FieldSource::Attribute { attribute: "mis".into() }));
    fields.push(field("mrom", FieldKind::Moisture, AddUp::Mass { density: "rd_rom".into() }, FieldSource::Parameter { column: "mrom".into() }));
    // Working sections read the ash under the model's own code.
    let ash = ["ash", "as", "ash_ad", "asad"].into_iter().find(|a| qualities.iter().any(|q| q == a));
    let steps = STEPS
        .iter()
        .map(|(name, _)| {
            let mut step = DataflowStep::on(name);
            if let (true, Some(ash)) = (["working_sections", "rom_ash_cutoff"].contains(name), ash) {
                step.settings.insert("ash".into(), ash.into());
            }
            step
        })
        .collect();
    let mut columns = vec![
        ValueColumn::number("min_volume", "m3"),
        ValueColumn::number("max_thickness", "m"),
        ValueColumn::number("max_area", "m2"),
        ValueColumn::number("underburden_thickness", "m"),
        ValueColumn::number("mad", "%"),
        ValueColumn::number("mis", "%"),
        ValueColumn::number("mrom", "%"),
        ValueColumn::number("min_parting", "m"),
        ValueColumn::number("ws_strip_ratio", ""),
        ValueColumn::number("min_coal_thickness", "m"),
        ValueColumn::number("insitu_ash_cutoff", "%"),
        ValueColumn::text("discard"),
        ValueColumn::text("ws_split"),
        ValueColumn::number("roof_loss", "m"),
        ValueColumn::number("floor_loss", "m"),
        ValueColumn::number("edge_loss", "m"),
        ValueColumn::number("other_loss", "%"),
        ValueColumn::text("loss_between_coal"),
        ValueColumn::number("roof_dilution", "m"),
        ValueColumn::number("floor_dilution", "m"),
        ValueColumn::number("edge_dilution", "m"),
        ValueColumn::number("other_dilution", "%"),
        ValueColumn::number("rom_ash_cutoff", "%"),
        ValueColumn::text("align_dilution_mad"),
        ValueColumn::text("aggregate_sections"),
        ValueColumn::text("clean_coal_average"),
    ];
    let moistures = ["mad".to_string(), "mis".to_string(), "mrom".to_string()];
    let qualities: Vec<String> = qualities.into_iter().filter(|q| !moistures.contains(q)).collect();
    let geometry = ["thickness".to_string(), "area".to_string(), "length".to_string()];
    for q in geometry.iter().chain(std::iter::once(&density)).chain(&qualities).chain(&moistures) {
        let unit = match q.as_str() {
            "thickness" | "length" => "m",
            "area" => "m2",
            _ if *q == density => "t/m3",
            _ if moistures.contains(q) => "%",
            _ => "",
        };
        for end in ["min", "max", "default"] {
            columns.push(ValueColumn::number(&format!("{q}_{end}"), unit));
        }
    }
    let row = |material: &str| ParameterRow { when: [("material".to_string(), material.to_string())].into(), values: BTreeMap::new() };
    let default = ParameterRow { when: BTreeMap::new(), values: [("underburden_thickness".to_string(), ParamValue::Number(UNDERBURDEN_THICKNESS))].into() };
    let table = ParameterTable { filters: vec!["material".into(), "seam".into()], columns, rows: vec![row("waste"), row("coal"), default] };
    let mut dataflow = Dataflow { levels: zd_core::default_levels(), fields, steps, ..Dataflow::default() };
    super::with_starter_formulas(&mut dataflow);
    (table, dataflow)
}

impl crate::Session {
    /// Saves a starter parameter table and a dataflow reading it (decision 0073, point 9), from a seam model: two runs,
    /// `parameters.save` then `dataflow.save`, each logged and undone like any. Gives back the table's and the dataflow's
    /// ids.
    pub fn dataflow_starter(&mut self, model: ObjectId, name: &str) -> Result<(ObjectId, ObjectId), CommandError> {
        let object = self.project().object(model).cloned().ok_or(CommandError::MissingObject(model))?;
        let Body::SeamModel(m) = &object.body else {
            return Err(CommandError::WrongKind { id: model, expected: ObjectKind::SeamModel, found: object.kind() });
        };
        let (table, mut dataflow) = dataflow_starter(m);
        let made = |record: crate::RunRecord| record.outputs.iter().find(|o| o.change == zd_core::ChangeKind::Created).map(|o| o.id).ok_or_else(|| CommandError::Invalid("nothing was made".into()));
        let table_record = self.run("parameters.save", serde_json::json!({ "name": format!("{name} assumptions"), "content": table }))?;
        let table_id = made(table_record)?;
        dataflow.parameters = vec![table_id];
        let flow = made(self.run("dataflow.save", serde_json::json!({ "name": name, "content": dataflow }))?)?;
        Ok((table_id, flow))
    }
}
