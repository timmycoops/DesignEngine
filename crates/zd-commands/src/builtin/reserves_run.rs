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

use std::collections::BTreeMap;
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
        "sets aside coal discarded by hand or by its in situ ash, forms working sections top down by min_parting, ws_strip_ratio and min_coal_thickness, splits them by ws_split, labels partings and thin coal, and writes each section",
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
    (
        "rom",
        "gives dilution its coal's moisture where it has none, aligns its air-dried moisture with the coal's where align_dilution_mad is yes, gives a negative dilution quality the coal's, and works every unit's ROM density",
    ),
    (
        "final",
        "writes ROM coal and waste on the leaves they end on, benches back by their in situ shares, and Scheduled beside them; clean coal; the underburden taken as out-of-model volume; each section's quality on its coal where aggregate_sections is yes",
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
    /// where it was benched; `case` where a workflow case made it.
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
}

/// What a step writes (decision 0074): a piece of a unit on a leaf of the reserves table, under a folder. The leaf is
/// the values of the dataflow's levels below the column (seam and bench by default), from the block the piece came from;
/// the folder a path such as `Insitu\Coal`, its last part the material from the seam stack. Texts are words for columns
/// of their own, such as the working section's name.
#[derive(Debug, Clone)]
struct Piece {
    folder: String,
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
    said: Said,
    /// Volume added from outside the model (the underburden), kept out of the column's balance to its blocks.
    added_volume: f64,
    /// The largest plan area of the column's blocks.
    block_area: f64,
}

/// What a column's steps changed, counted by what and how much, and the blocks out of range by check.
#[derive(Debug, Default)]
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
}

impl ColumnRun<'_> {
    /// A parameter for the unit at `at`, matched by the attributes of the block it came from (its top bench's, once
    /// joined, as Spry's).
    fn parameter(&self, at: usize, column: &str) -> Option<f64> {
        parameter(self.tables, &self.attributes[self.column.units()[at].origin as usize], column)
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
    dataflow: Arc<Dataflow>,
    fields: Arc<Fields>,
    tables: Vec<Arc<ParameterTable>>,
    horizons: Vec<String>,
    steps: Vec<(String, StepRun, DataflowStep)>,
    /// What the dataflow's formulas may use.
    declared: zd_script::Declared,
    /// The attributes of the levels above the seam, which make a column, and of those below it, which make a leaf.
    upper: Vec<String>,
    lower: Vec<String>,
    /// Blocks by case and their values of the upper levels.
    columns: BTreeMap<ColumnKey, Vec<Arc<Object>>>,
    /// Blocks missing a level that has no default, by name, with the level.
    unplaced: Vec<(String, String)>,
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
            ],
            outputs: vec![Field::object("tables", ObjectKind::Table, "A results table a case")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let started = Instant::now();
        let prepared = prepare(&|id| inputs.object(id).cloned(), p)?;
        let keys: Vec<(&ColumnKey, &Vec<Arc<Object>>)> = prepared.columns.iter().collect();
        let done = in_parallel_watched(inputs, "blocks", &keys, |(key, blocks)| run_column(&prepared, key, blocks, None))?;
        let (mut ran, mut stopped) = (Vec::new(), Vec::new());
        for d in done {
            match d {
                Ok(r) => ran.push(r),
                Err(s) => stopped.push(s),
            }
        }
        Ok(outcome(&prepared, p, ran, stopped, started))
    }
}

/// Reads the dataflow, its tables and the stack, checks its steps, and groups the blocks into columns.
pub(crate) fn prepare(get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>, p: &RunReservesParams) -> Result<Prepared, CommandError> {
    let flow = get(p.dataflow)?;
    let Body::Dataflow(dataflow) = &flow.body else {
        return Err(CommandError::WrongKind { id: p.dataflow, expected: ObjectKind::Dataflow, found: flow.kind() });
    };
    dataflow.check().map_err(CommandError::Invalid)?;
    let fields = Arc::new(dataflow.engine_fields().map_err(CommandError::Invalid)?);
    let mut tables = Vec::new();
    if let Some(own) = &p.parameters {
        own.check().map_err(|e| CommandError::Invalid(format!("the step's parameter rows: {e}")))?;
        tables.push(Arc::new(own.clone()));
    }
    for &id in &dataflow.parameters {
        let object = get(id)?;
        let Body::ParameterTable(table) = &object.body else {
            return Err(CommandError::WrongKind { id, expected: ObjectKind::ParameterTable, found: object.kind() });
        };
        tables.push(table.clone());
    }
    let horizons: Vec<String> = match p.model {
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
    let declared = formulas::declared(&fields, &table_refs);
    let mut compiled: BTreeMap<usize, Arc<zd_script::Formula>> =
        formulas::check_formula_steps(dataflow, &built_in, &declared).map_err(CommandError::Invalid)?.into_iter().map(|(i, f)| (i, Arc::new(f))).collect();
    let mut steps = Vec::new();
    for (i, s) in dataflow.steps.iter().enumerate().filter(|(_, s)| s.on) {
        if let Some(f) = compiled.remove(&i) {
            steps.push((s.step.clone(), StepRun::Formula(f), s.clone()));
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
    'blocks: for &id in &p.blocks {
        let object = get(id)?;
        if !matches!(object.body, Body::Solid(_)) {
            return Err(CommandError::WrongKind { id, expected: ObjectKind::Solid, found: object.kind() });
        }
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
    Ok(Prepared { name: flow.name.clone(), dataflow: dataflow.clone(), fields, tables, horizons, steps, declared, upper, lower, columns, unplaced })
}

/// A word from the parameter tables, read in order.
fn parameter_text(tables: &[Arc<ParameterTable>], attributes: &Attributes, column: &str) -> Option<String> {
    let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
    ParameterTable::lookup_in(&refs, attributes, column).map(|f| match f.value {
        ParamValue::Number(n) => n.to_string(),
        ParamValue::Text(t) => t.trim().to_string(),
    })
}

/// A value from the parameter tables, read in order.
fn parameter(tables: &[Arc<ParameterTable>], attributes: &Attributes, column: &str) -> Option<f64> {
    let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
    ParameterTable::lookup_in(&refs, attributes, column).and_then(|f| match f.value {
        ParamValue::Number(n) => Some(*n),
        ParamValue::Text(t) => t.trim().parse().ok(),
    })
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
    let Body::Solid(solid) = &object.body else { unreachable!("blocks are checked to be solids") };
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
    let volume = solid.volume();
    let area = plan_area(solid.mesh());
    let mut values = Vec::with_capacity(prepared.dataflow.fields.len());
    for f in &prepared.dataflow.fields {
        let summed = f.field.add_up == AddUp::Sum;
        let value = match &f.source {
            FieldSource::Volume => volume,
            FieldSource::PlanArea => area,
            FieldSource::Thickness => {
                if area > 0.0 {
                    volume / area
                } else {
                    0.0
                }
            }
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

/// A column's units, after each step when asked: the preview.
#[derive(Debug, Clone, Serialize)]
pub struct StepView {
    pub step: String,
    pub units: Vec<UnitView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnitView {
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
}

fn view(run: &ColumnRun<'_>, step: &str) -> StepView {
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
            horizon: run.horizons[u.stack as usize].clone(),
            material: u.in_situ.material.clone(),
            seam: u.in_situ.seam.clone(),
            bench: u.bench.as_ref().map(|b| b.label.clone()),
            joined: u.joined.map(|j| run.column.joins()[j as usize].benches.iter().map(|b| (b.bench.label.clone(), b.share)).collect()).unwrap_or_default(),
            became: u.became.as_ref().map(became),
            section: u.section.as_ref().map(|s| format!("{} {} {}", s.id.map_or(String::new(), |i| i.to_string()), s.material, s.seam).trim().to_string()),
            rom: u.rom.as_ref().map(|s| format!("{} {} {}", s.id.map_or(String::new(), |i| i.to_string()), s.material, s.seam).trim().to_string()),
            values: fields.list().iter().zip(u.values()).map(|(f, v)| (f.name.clone(), *v)).collect(),
        })
        .collect();
    StepView { step: step.to_string(), units }
}

/// One column: its blocks made units, then each step in turn. With `preview`, the column after each step.
fn run_column(prepared: &Prepared, key: &ColumnKey, blocks: &[Arc<Object>], mut preview: Option<&mut Vec<StepView>>) -> Result<Ran, Stopped> {
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
        attributes.push(object.attributes.clone());
        if let Body::Solid(solid) = &object.body {
            block_area = block_area.max(plan_area(solid.mesh()));
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
        said: Said::default(),
        added_volume: 0.0,
        block_area,
    };
    for name in &mismatched {
        run.said.name("whose volume isn't its solid's (measured before it was cut again?)", name);
    }
    // Imported: each block as read, before any step.
    write_pieces(&mut run, |_| Some("Imported".into()), |_| Vec::new());
    if let Some(p) = preview.as_deref_mut() {
        p.push(view(&run, "as input"));
    }
    for (name, step, settings) in &prepared.steps {
        let done = match step {
            StepRun::BuiltIn(f) => f(&mut run, settings),
            StepRun::Formula(f) => run_formula(&mut run, f, settings, &prepared.declared),
        };
        done.map_err(|why| stop(format!("{} at {name}: {why}", key.words())))?;
        if let Some(p) = preview.as_deref_mut() {
            p.push(view(&run, name));
        }
    }
    let imbalance = input_volume - (run.column.balance()[volume] - run.added_volume);
    Ok(Ran { key: key.clone(), rows: run.rows, said: run.said, imbalance, input_volume })
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
    let units: Vec<UnitInput> = run
        .column
        .units()
        .iter()
        .enumerate()
        .map(|(at, u)| {
            let mut values = numbers(u.values());
            values.extend(unit_labels(u).into_iter().map(|(k, v)| (k.to_string(), v.map_or(ScriptValue::None, ScriptValue::Text))));
            UnitInput { fields: Arc::new(values), block: blocks[u.origin as usize].clone(), parameters: params[u.origin as usize].clone(), section: section_of[at] }
        })
        .collect();
    let when = step.formula.as_ref().map(|f| f.when.clone()).unwrap_or_default();
    let which: Vec<usize> = (0..run.column.len())
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
        .collect();
    let input = ColumnInput { units, sections, column: Arc::new(numbers(&run.column.balance())) };
    let out = formula.run(&input, &which).map_err(|p| {
        let unit = p.unit.map(|u| format!(" on {}", run.name_of(u))).unwrap_or_default();
        format!("{}{unit}", Problem { unit: None, ..p })
    })?;
    let volume = fields.volume();
    for done in out {
        for (field, value) in done.sets {
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
        let v = run.column.units()[done.unit].value(volume);
        for text in done.said {
            run.said.count(format!("printed by {}: {text}", step.step), v);
        }
    }
    Ok(())
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
    let ash = fields.at(step.setting("ash")).ok();
    let active: Vec<usize> = (0..run.column.len()).filter(|&i| !run.column.units()[i].is_set_aside()).collect();

    // 1. Discarded coal.
    for &i in &active {
        let u = &run.column.units()[i];
        if !is_coal(u) || u.became.is_some() {
            continue;
        }
        let attributes = &run.attributes[u.origin as usize];
        let by_hand = parameter_text(run.tables, attributes, step.setting("discard")).is_some_and(|t| matches!(t.to_lowercase().as_str(), "yes" | "y" | "true" | "1"));
        let high = ash.zip(run.parameter(i, step.setting("insitu_ash_cutoff"))).is_some_and(|(a, cut)| u.value(a) > cut);
        if by_hand || high {
            let detail = if by_hand { "user specified" } else { "high in situ ash" };
            let v = u.value(volume);
            run.column.label_became(i, Became { kind: BecameKind::Discarded, detail: detail.into() });
            run.said.count(format!("coal discarded, {detail}"), v);
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
                let breaks = !between || discarded || {
                    let own = w.iter().filter_map(|&i| run.parameter(i, step.setting("min_parting"))).fold(f64::NAN, f64::max);
                    let limit = if own.is_nan() {
                        let (Item::Ply(above), Item::Ply(below)) = (&items[k - 1], &items[k + 1]) else { unreachable!("between plies") };
                        need(run, *above, "min_parting")?.max(need(run, *below, "min_parting")?)
                    } else {
                        own
                    };
                    sum(run, w, thickness) > limit
                };
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
                if run.column.units()[*ply].value(volume) * threshold >= sum(run, parting, volume) {
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
            if sum(run, &units, thickness) <= minimum {
                thin.extend(plies);
                continue;
            }
            // Forced splits: a new part where the plies' names change, each parting with the ply above.
            let name = |run: &ColumnRun<'_>, i: usize| parameter_text(run.tables, &run.attributes[run.column.units()[i].origin as usize], step.setting("ws_split")).unwrap_or_default();
            let mut part: Vec<usize> = Vec::new();
            let mut last: Option<String> = None;
            for &i in &units {
                if is_ply(run, i) {
                    let n = name(run, i);
                    if last.as_ref().is_some_and(|l| *l != n) {
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
        if coal && !is_ply(run, i) {
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
    let across = |run: &ColumnRun<'_>, at: usize| parameter_text(run.tables, &run.attributes[run.column.units()[at].origin as usize], step.setting("loss_between_coal")).is_some_and(|t| matches!(t.to_lowercase().as_str(), "yes" | "y" | "true" | "1"));
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
        // Too thin for its roof and floor loss: thin loss, all of it.
        if held <= roof_v + floor_v {
            thin_section(run, s, &sections, k, "short of its roof and floor loss");
            continue;
        }
        // Edge and other loss come from each ply as it stood before roof and floor loss, as Spry's do.
        let mut asked = Vec::new();
        for &at in &plies {
            let length = if edge > 0.0 { length_of(run, at, step)? } else { 0.0 };
            let u = &run.column.units()[at];
            asked.push((u.id, edge * length * u.value(thickness), other / 100.0 * u.value(v)));
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
        let Some(waste_above) = sections[..k].iter().rev().find(|w| !w.coal).cloned() else { continue };
        let remaining: f64 = rest.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
        let wanted: f64 = asked.iter().map(|(_, e, o)| e + o).sum();
        if remaining <= wanted {
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
        let mut asks: Vec<(&'static str, f64, Section)> = Vec::new();
        if let Some(a) = &above {
            let floor = largest_of(run, a, step, "floor_dilution")?;
            asks.push(("floor", floor * section_area(run, a), a.clone()));
        }
        if let Some(b) = below.first() {
            let roof = largest_of(run, b, step, "roof_dilution")?;
            asks.push(("roof", roof * section_area(run, b), b.clone()));
        }
        for b in &below {
            let (edge, other) = (largest_of(run, b, step, "edge_dilution")?, largest_of(run, b, step, "other_dilution")?);
            let (mut e, mut o) = (0.0, 0.0);
            for &id in &b.units {
                let Some(at) = run.column.position(id) else { continue };
                let u = &run.column.units()[at];
                if !is_coal(u) || u.became.is_some() {
                    continue;
                }
                if edge > 0.0 {
                    e += edge * length_of(run, at, step)? * u.value(thickness);
                }
                o += other / 100.0 * u.value(v);
            }
            asks.push(("edge", e, b.clone()));
            asks.push(("other", o, b.clone()));
        }
        asks.retain(|(_, volume, _)| *volume > 0.0);
        let total: f64 = asks.iter().map(|(_, volume, _)| volume).sum();
        if total <= 0.0 {
            continue;
        }
        let held: f64 = w.units.iter().filter_map(|&id| run.column.position(id)).map(|at| run.column.units()[at].value(v)).sum();
        if held < total * (1.0 - 1e-12) {
            // Too thin: shared out, and the losses sent into it go back to their coal.
            let scale = held / total;
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
        for (kind, volume, to) in asks.iter().filter(|(kind, _, _)| *kind == "floor") {
            let pieces = take_volume(run, &rest, *volume, End::Top)?;
            rest = units_left(run, &pieces, &rest);
            send_dilution(run, &pieces, kind, to);
        }
        for (kind, volume, to) in asks.iter().filter(|(kind, _, _)| *kind != "floor") {
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
        if !cutoff.is_finite() {
            continue;
        }
        let members = rom_members(run, s.id);
        let rom_ash = run.column.add_up(members.iter().copied())[ash];
        if !rom_ash.is_finite() {
            run.said.name("ROM section with no ash to test against its cut-off", &format!("{} section {}", run.name_of(members[0]), s.name));
            continue;
        }
        if rom_ash <= cutoff {
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
    parameter_text(run.tables, &run.attributes[run.column.units()[at].origin as usize], column).is_some_and(|t| matches!(t.to_lowercase().as_str(), "yes" | "y" | "true" | "1"))
}

/// `rom` (decision 0074, point 8), Spry's ROM calculations. In each ROM coal section, what came in that isn't its coal
/// (dilution, partings, coal from a neighbour):
///
/// - takes the section's coal moisture (air dried, in situ, ROM, averaged by the fields' rules) where it has none, said;
/// - where the section's first ply's row says `align_dilution_mad` yes, takes the coal's air-dried moisture, each of its
///   qualities scaled by (1 − Mad coal) ÷ (1 − Mad own), as Spry's;
/// - a quality given as negative takes the section's coal quality (Spry's convention).
///
/// Then every unit's ROM density is its in situ density × (1 − Mis) ÷ (1 − Mrom), where it has both. The step's
/// settings name the fields, as `in_situ`'s.
fn rom(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let fields = run.column.fields();
    let at = |key: &str| fields.at(step.setting(key)).ok();
    let (mad, mis, mrom, rd_is, rd_rom) = (at("mad"), at("mis"), at("mrom"), at("rd_is"), at("rd_rom"));
    let qualities: Vec<usize> = fields.list().iter().enumerate().filter(|(_, f)| f.kind == FieldKind::Quality).map(|(i, _)| i).collect();
    let names: Vec<String> = fields.list().iter().map(|f| f.name.clone()).collect();
    let v = fields.volume();
    for s in sections_of(run).into_iter().filter(|s| s.coal) {
        let plies = plies_of(run, &s);
        if plies.is_empty() {
            continue;
        }
        let coal = run.column.add_up(plies.iter().copied());
        let align = says_yes(run, plies[0], step.setting("align_dilution_mad"));
        for i in rom_members(run, s.id) {
            if plies.contains(&i) {
                continue;
            }
            let volume = run.column.units()[i].value(v);
            for f in [mad, mis, mrom].into_iter().flatten() {
                if !run.column.units()[i].value(f).is_finite() && coal[f].is_finite() {
                    run.column.unit_mut(i).set(f, coal[f]);
                    run.said.count(format!("dilution and partings given their coal's {}", names[f]), volume);
                }
            }
            if let (true, Some(mad)) = (align, mad) {
                let own = run.column.units()[i].value(mad);
                if own.is_finite() && coal[mad].is_finite() && own != coal[mad] {
                    let factor = (1.0 - coal[mad] / 100.0) / (1.0 - own / 100.0);
                    for &q in &qualities {
                        let x = run.column.units()[i].value(q);
                        run.column.unit_mut(i).set(q, x * factor);
                    }
                    run.column.unit_mut(i).set(mad, coal[mad]);
                    run.said.count("dilution's air-dried moisture aligned with its coal's".into(), volume);
                }
            }
            for &q in &qualities {
                if run.column.units()[i].value(q) < 0.0 && coal[q].is_finite() {
                    run.column.unit_mut(i).set(q, coal[q]);
                    run.said.count(format!("dilution's negative {} given its coal's", names[q]), volume);
                }
            }
        }
    }
    if let (Some(mis), Some(mrom), Some(rd_is), Some(rd_rom)) = (mis, mrom, rd_is, rd_rom) {
        for i in 0..run.column.len() {
            let u = &run.column.units()[i];
            let (d, is, rom) = (u.value(rd_is), u.value(mis) / 100.0, u.value(mrom) / 100.0);
            if d.is_finite() && is.is_finite() && rom.is_finite() {
                run.column.unit_mut(i).set(rd_rom, d * (1.0 - is) / (1.0 - rom));
            }
        }
    }
    Ok(())
}

/// Where a piece goes in the reserves table: onto the leaves of `onto`, a joined unit's benches by their in situ shares
/// (Spry's reinstated benches), its summed fields shared out, under `folder`.
fn place(run: &ColumnRun<'_>, values: &[f64], onto: usize, folder: &str, texts: &[(String, String)]) -> Vec<Piece> {
    let fields = run.column.fields();
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
            Piece { folder: folder.to_string(), leaf, values, texts: texts.to_vec() }
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
/// - **Clean coal**: what's coal in situ and in ROM, each section's coal alone, its qualities the section's coal's where
///   aggregated.
/// - **Out of model**: underburden taken into ROM coal as dilution, under `Summary\Out of model`, on the leaves it's
///   written on; the underburden left is never written. Small volumes are never in ROM.
fn final_step(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    let v = run.column.fields().volume();
    let qualities: Vec<usize> = run.column.fields().list().iter().enumerate().filter(|(_, f)| f.kind == FieldKind::Quality).map(|(i, _)| i).collect();
    let sections = sections_of(run);
    let mut pieces: Vec<Piece> = Vec::new();
    let mut in_rom_coal = vec![false; run.column.len()];
    for s in sections.iter().filter(|s| s.coal) {
        let plies = plies_of(run, s);
        if plies.is_empty() {
            continue;
        }
        let members = rom_members(run, s.id);
        let aggregate = says_yes(run, plies[0], step.setting("aggregate_sections"));
        let section_values = run.column.add_up(members.iter().copied());
        let clean_values = run.column.add_up(plies.iter().copied());
        let texts = vec![("ROM\\section".to_string(), s.name.clone())];
        let ply_volume: f64 = plies.iter().map(|&p| run.column.units()[p].value(v)).sum();
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
            for (p, share) in onto {
                let scaled: Vec<f64> = values.iter().enumerate().map(|(f, x)| if run.column.fields().is_summed(f) { x * share } else { *x }).collect();
                pieces.extend(place(run, &scaled, p, "ROM\\Coal", &texts));
                pieces.extend(place(run, &scaled, p, "Scheduled\\Coal", &texts));
                if out_of_model {
                    pieces.extend(place(run, &scaled, p, "Summary\\Out of model", &[]));
                }
            }
            if out_of_model {
                run.said.count("underburden taken into ROM coal, out of model".into(), u.value(v));
            }
            if plies.contains(&m) {
                let mut clean = values.clone();
                if aggregate {
                    for &q in &qualities {
                        clean[q] = clean_values[q];
                    }
                }
                pieces.extend(place(run, &clean, m, "Clean coal\\Coal", &texts));
            }
        }
    }
    // ROM waste: everything else in the model but small volumes.
    for (i, &taken) in in_rom_coal.iter().enumerate() {
        let u = &run.column.units()[i];
        if taken || run.is_out_of_model(u) || u.became.as_ref().is_some_and(|b| b.kind == BecameKind::SmallVolume) {
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
        let values = u.values().to_vec();
        pieces.extend(place(run, &values, onto, "ROM\\Waste", &[]));
        pieces.extend(place(run, &values, onto, "Scheduled\\Waste", &[]));
    }
    run.rows.extend(pieces);
    Ok(())
}

/// `join_benches`: each horizon's benches joined into one unit, each bench's in situ share of its volume kept.
fn join_benches(run: &mut ColumnRun<'_>, _: &DataflowStep) -> Result<(), String> {
    run.column.join_benches();
    Ok(())
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
    write_pieces(run, |u| (!u.is_set_aside()).then(|| "Insitu".into()), |_| Vec::new());
    Ok(())
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
        let texts = texts(u);
        let shares: Vec<(u32, f64)> = match u.joined {
            Some(j) => run.column.joins()[j as usize].benches.iter().map(|b| (b.origin, b.share)).collect(),
            None => vec![(u.origin, 1.0)],
        };
        for (origin, share) in shares {
            let values = u.values().iter().zip(&summed).map(|(v, s)| if *s { v * share } else { *v }).collect();
            let attributes = &run.attributes[origin as usize];
            let leaf = run.lower.iter().map(|l| attribute_ci(attributes, l).map(attribute_word).unwrap_or_default()).collect();
            pieces.push(Piece { folder: folder.clone(), leaf, values, texts: texts.clone() });
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

fn unit_of_kind(kind: FieldKind) -> &'static str {
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

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The reserves table of one case's columns (decision 0074): a row a leaf (the case, then a value a level), a column a
/// field under each folder its pieces were written to, `Insitu\Coal\volume` say, each a leaf's pieces added up by the
/// field's rule, with the field's unit. Where a folder holds an in situ density, its tonnes on the in situ basis, its
/// air-dried tonnes and each quality on the in situ basis beside them; where only an RD, its tonnes on the RD's basis.
/// A column with no value at all is left out.
fn table_of(prepared: &Prepared, ran: &[&Ran]) -> Table {
    let fields = prepared.fields.list();
    // The leaves in the order they were first written, and what each holds.
    // A leaf: its key (case and level values), each folder's pieces' values, and its words by column.
    type Leaf = (Vec<String>, BTreeMap<String, Vec<Vec<f64>>>, BTreeMap<String, String>);
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
                leaves.push((key, BTreeMap::new(), BTreeMap::new()));
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
        for (i, f) in fields.iter().enumerate() {
            // A thickness adds down a column and an area is a column's largest; across leaves neither adds up, so a
            // summary shows neither.
            let across = if matches!(f.kind, FieldKind::Thickness | FieldKind::Area | FieldKind::Length) { None } else { Some(f.add_up.clone()) };
            push(TableColumn { name: format!("{folder}\\{}", f.name), unit: unit_of_kind(f.kind).to_string(), add_up: across, values: zd_core::ColumnValues::Number(value(i)) });
        }
        let tonnes = |d: usize| added.iter().map(|a| a.as_ref().map(|a| a[v] * a[d]).filter(|x| x.is_finite())).collect::<Vec<_>>();
        let named = |n: &str| format!("{folder}\\{n}");
        // ROM, Scheduled and clean coal on the ROM basis: tonnes by the ROM density, qualities × (1 − Mrom) ÷ (1 − Mad).
        let rom_basis = ["ROM", "Scheduled", "Clean coal"].iter().any(|top| folder.starts_with(&format!("{top}\\")));
        if let (true, Some(rd), Some(rd_rom)) = (rom_basis, field("rd"), field("rd_rom"))
            && added.iter().flatten().any(|a| a[rd_rom].is_finite())
        {
            push(TableColumn::adding(&named("tonnes"), "t", AddUp::Sum, tonnes(rd_rom)));
            push(TableColumn::adding(&named("tonnes_ad"), "t", AddUp::Sum, tonnes(field(super::RD_ADT).unwrap_or(rd))));
            if let (Some(mad), Some(mrom)) = (field("mad"), field("mrom")) {
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
                push(TableColumn::adding(&named("tonnes_ad"), "t", AddUp::Sum, tonnes(field(super::RD_ADT).unwrap_or(rd))));
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
    for column in &text_columns {
        columns.push(TableColumn::text(column, leaves.iter().map(|l| l.2.get(column).cloned().filter(|w| !w.is_empty())).collect()));
    }
    Table { columns }
}

fn outcome(prepared: &Prepared, p: &RunReservesParams, ran: Vec<Ran>, stopped: Vec<Stopped>, started: Instant) -> Outcome {
    let mut cases: BTreeMap<&str, Vec<&Ran>> = BTreeMap::new();
    for r in &ran {
        cases.entry(r.key.case.as_str()).or_default().push(r);
    }
    let mut changes = Vec::new();
    let mut messages = Vec::new();
    for (case, of) in &cases {
        let table = table_of(prepared, of);
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
    let mut dataflow = Dataflow { levels: zd_core::default_levels(), fields, steps, parameters: Vec::new() };
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
