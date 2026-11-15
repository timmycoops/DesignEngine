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
    AttrValue, Attributes, Body, Change, Dataflow, DataflowStep, FieldSource, Object, ObjectId, ObjectKind, ParamValue, ParameterTable, Table, TableColumn, TriMesh, attribute_ci,
    attribute_word,
};
use zd_reserves::{AddUp, Became, BecameKind, Bench, Column, ColumnKey, FieldKind, Fields, Labels, Origin, Unit};

use super::blocks::horizon_order;
use super::in_parallel_watched;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Severity};

/// The case a block belongs to when it carries none: the workflow's own steps (decision 0066).
const BASE: &str = "Base";

/// Blocks named in a note before the rest are counted.
const NAMED: usize = 5;

/// What a step does to a column, given its settings. Registered by name; a dataflow lists names.
type StepFn = fn(&mut ColumnRun<'_>, &DataflowStep) -> Result<(), String>;

/// The origin an added unit carries in place of a block: ground outside the model, the underburden (decision 0073).
pub const OUT_OF_MODEL: &str = "out of model";

/// The underburden's thickness where no parameter row gives one (m), as Spry's (decision 0073, point 4).
pub const UNDERBURDEN_THICKNESS: f64 = 10.0;

/// The steps a dataflow can name, in Spry's order, with what each does.
pub const STEPS: [(&str, &str); 4] = [
    ("small_volumes", "sets aside and writes each unit under its min_volume, never dropping it"),
    (
        "initialise",
        "puts the column in the seam model's stack order, a horizon's benches top down; checks thickness and area against max_thickness and max_area; adds underburden below the last coal where there's no waste; gives qualities their limits and defaults",
    ),
    ("in_situ", "writes each horizon's benches as they are, before anything is joined"),
    ("join_benches", "joins each horizon's benches into one unit with their in situ shares, so later writes put pieces back by share"),
];

fn step_fn(name: &str) -> Option<StepFn> {
    match name {
        "small_volumes" => Some(small_volumes),
        "initialise" => Some(initialise),
        "in_situ" => Some(in_situ),
        "join_benches" => Some(join_benches),
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

/// One row a step writes.
#[derive(Debug, Clone)]
struct Row {
    write: &'static str,
    horizon: String,
    seam: String,
    material: String,
    bench: Option<String>,
    became: Option<String>,
    units: usize,
    sources: Vec<String>,
    values: Vec<f64>,
}

/// A column in work: the column, its horizons' names by stack place, the attributes of each input unit's block (what
/// parameter rows are matched by), the parameter tables, what its steps have written, and what they've said.
pub struct ColumnRun<'a> {
    pub column: Column,
    horizons: Vec<String>,
    attributes: Vec<Attributes>,
    tables: &'a [Arc<ParameterTable>],
    dataflow: &'a Dataflow,
    rows: Vec<Row>,
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
    rows: Vec<Row>,
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
    steps: Vec<(&'static str, StepFn, DataflowStep)>,
    /// Blocks by case, strip and block.
    columns: BTreeMap<ColumnKey, Vec<Arc<Object>>>,
    /// Blocks with no strip or block, by name.
    unplaced: Vec<String>,
}

impl Command for RunReserves {
    type Params = RunReservesParams;
    const NAME: &'static str = "reserves.run";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run a reserves dataflow over blocks: a column a strip and block, its steps in order, a results table a case",
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
    let mut steps = Vec::new();
    for s in dataflow.steps.iter().filter(|s| s.on) {
        let Some(f) = step_fn(&s.step) else {
            let known: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
            return Err(CommandError::Invalid(format!("the dataflow names a step {}, which this build doesn't have; it has {}", s.step, known.join(", "))));
        };
        let name = STEPS.iter().find(|(n, _)| *n == s.step).map(|(n, _)| *n).expect("a registered step");
        steps.push((name, f, s.clone()));
    }
    let mut columns: BTreeMap<ColumnKey, Vec<Arc<Object>>> = BTreeMap::new();
    let mut unplaced = Vec::new();
    for &id in &p.blocks {
        let object = get(id)?;
        if !matches!(object.body, Body::Solid(_)) {
            return Err(CommandError::WrongKind { id, expected: ObjectKind::Solid, found: object.kind() });
        }
        let word = |key: &str| attribute_ci(&object.attributes, key).map(attribute_word).filter(|w| !w.is_empty());
        let (Some(strip), Some(block)) = (word("strip"), word("block")) else {
            unplaced.push(object.name.clone());
            continue;
        };
        let case = word("case").unwrap_or_else(|| BASE.to_string());
        columns.entry(ColumnKey { case, strip, block }).or_default().push(object);
    }
    Ok(Prepared { name: flow.name.clone(), dataflow: dataflow.clone(), fields, tables, horizons, steps, columns, unplaced })
}

/// The plan area of a solid: its upward-facing triangles in plan, about its first vertex so mine coordinates keep their
/// precision. For a block, whose walls are vertical, its footprint.
fn plan_area(mesh: &TriMesh) -> f64 {
    let Some(&o) = mesh.vertices.first() else { return 0.0 };
    let twice: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| {
                let v = mesh.vertices[i as usize];
                [v[0] - o[0], v[1] - o[1]]
            });
            ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).max(0.0)
        })
        .sum();
    twice / 2.0
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
    let mut block_area: f64 = 0.0;
    for object in blocks {
        let (stack, labels, bench, values) = unit_of(prepared, object).map_err(stop)?;
        let origin = Origin { object: object.id.to_string(), name: object.name.clone() };
        column.push_bench_input(origin, stack, labels, bench, values).map_err(|e| stop(e.to_string()))?;
        attributes.push(object.attributes.clone());
        if let Body::Solid(solid) = &object.body {
            block_area = block_area.max(plan_area(solid.mesh()));
        }
    }
    let volume = prepared.fields.volume();
    let input_volume = column.balance()[volume];
    let mut run = ColumnRun { column, horizons: prepared.horizons.clone(), attributes, tables: &prepared.tables, dataflow: &prepared.dataflow, rows: Vec::new(), said: Said::default(), added_volume: 0.0, block_area };
    if let Some(p) = preview.as_deref_mut() {
        p.push(view(&run, "as input"));
    }
    for (name, step, settings) in &prepared.steps {
        step(&mut run, settings).map_err(|why| stop(format!("{}/{} at {name}: {why}", key.strip, key.block)))?;
        if let Some(p) = preview.as_deref_mut() {
            p.push(view(&run, name));
        }
    }
    let imbalance = input_volume - (run.column.balance()[volume] - run.added_volume);
    Ok(Ran { key: key.clone(), rows: run.rows, said: run.said, imbalance, input_volume })
}

/// The column after each of the dataflow's steps, for one block: the canvas's before and after (decision 0072, point 7).
pub fn reserves_preview(
    get: &dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>,
    p: &RunReservesParams,
    case: &str,
    strip: &str,
    block: &str,
) -> Result<Vec<StepView>, CommandError> {
    let prepared = prepare(get, p)?;
    let key = ColumnKey { case: case.to_string(), strip: strip.to_string(), block: block.to_string() };
    let blocks = prepared.columns.get(&key).ok_or_else(|| CommandError::Invalid(format!("no blocks of case {case}, strip {strip}, block {block}")))?;
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
    write_groups(run, "small_volume", |u| u.became.as_ref().is_some_and(|b| b.kind == BecameKind::SmallVolume), Some("small volume"));
    Ok(())
}

/// `initialise` (decision 0073, points 2 to 4): the stack order, benches top down; each unit's thickness and area
/// checked against `max_thickness` and `max_area` (said, never changed: they're the solid's); underburden below the last
/// coal where there's no waste there; then each quality and density field given its limits and defaults.
fn initialise(run: &mut ColumnRun<'_>, step: &DataflowStep) -> Result<(), String> {
    run.column.sort();
    check_size(run, step);
    add_underburden(run, step);
    quality_defaults(run);
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

/// Each quality and density field given its limits and defaults (decision 0073, point 3): `<field>_min`, `<field>_max`
/// and `<field>_default`. Coal outside its limits, inclusive, or with no value, takes the default; waste takes its
/// default outright, as it has no grids. Each replacement counted by field and material.
fn quality_defaults(run: &mut ColumnRun<'_>) {
    let fields: Vec<(usize, String)> =
        run.column.fields().list().iter().enumerate().filter(|(_, f)| matches!(f.kind, FieldKind::Quality | FieldKind::Density)).map(|(i, f)| (i, f.name.clone())).collect();
    let volume = run.column.fields().volume();
    for at in 0..run.column.len() {
        if run.column.units()[at].is_set_aside() {
            continue;
        }
        let coal = is_coal(&run.column.units()[at]);
        for (i, name) in &fields {
            let Some(default) = run.parameter(at, &format!("{name}_default")) else { continue };
            let value = run.column.units()[at].value(*i);
            let replace = if coal {
                let (min, max) = (run.parameter(at, &format!("{name}_min")), run.parameter(at, &format!("{name}_max")));
                value.is_nan() || min.is_some_and(|m| value < m) || max.is_some_and(|m| value > m)
            } else {
                true
            };
            if replace {
                let v = run.column.units()[at].value(volume);
                run.column.unit_mut(at).set(*i, default);
                let why = if !coal { "waste given its default" } else if value.is_nan() { "coal with none given its default" } else { "coal outside its limits given its default" };
                run.said.count(format!("{name}: {why}"), v);
            }
        }
    }
}

/// `join_benches`: each horizon's benches joined into one unit, each bench's in situ share of its volume kept.
fn join_benches(run: &mut ColumnRun<'_>, _: &DataflowStep) -> Result<(), String> {
    run.column.join_benches();
    Ok(())
}

/// `in_situ`: each horizon's units put back on their benches and written, a row a horizon and bench; units set aside
/// and the underburden, outside the model, left out.
fn in_situ(run: &mut ColumnRun<'_>, _: &DataflowStep) -> Result<(), String> {
    write_groups(run, "in_situ", |u| !u.is_set_aside(), None);
    Ok(())
}

/// Writes the units `take` picks, a row a horizon and bench, at a write point; the underburden is never written.
fn write_groups(run: &mut ColumnRun<'_>, write: &'static str, take: impl Fn(&Unit) -> bool, became: Option<&str>) {
    let units = run.column.units();
    let kept: Vec<bool> = units.iter().map(|u| take(u) && !run.is_out_of_model(u)).collect();
    let groups = run.column.runs(|a, b| a.stack == b.stack && a.in_situ == b.in_situ);
    for g in groups {
        // Within a horizon, the units taken, as runs of their own.
        let mut at = g.start;
        while at < g.end {
            if !kept[at] {
                at += 1;
                continue;
            }
            let start = at;
            while at < g.end && kept[at] {
                at += 1;
            }
            let first = &run.column.units()[start];
            let (horizon, seam, material) = (run.horizons[first.stack as usize].clone(), first.in_situ.seam.clone(), first.in_situ.material.clone());
            for row in run.column.by_bench(start..at) {
                let sources = sources(&run.column, &row);
                run.rows.push(Row {
                    write,
                    horizon: horizon.clone(),
                    seam: seam.clone(),
                    material: material.clone(),
                    bench: row.bench.as_ref().map(|b| b.label.clone()),
                    became: became.map(str::to_string),
                    units: sources.len(),
                    sources,
                    values: row.values,
                });
            }
        }
    }
}

/// The blocks a written row came from: for a joined unit, the bench of its join the row is.
fn sources(column: &Column, row: &zd_reserves::BenchRow) -> Vec<String> {
    let mut names = Vec::new();
    for id in &row.units {
        let Some(at) = column.position(*id) else { continue };
        let u = &column.units()[at];
        let origin = match (u.joined, &row.bench) {
            (Some(j), Some(b)) => column.joins()[j as usize].benches.iter().find(|jb| jb.bench == *b).map_or(u.origin, |jb| jb.origin),
            _ => u.origin,
        };
        let name = column.origins()[origin as usize].name.clone();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn unit_of_kind(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Volume => "m3",
        FieldKind::Thickness => "m",
        FieldKind::Area => "m2",
        FieldKind::Density => "t/m3",
        _ => "",
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The results table of one case's columns: labels, then a column a field with its unit and rule, then tonnes where the
/// dataflow has a density (volume × density, on the density's own basis).
fn table_of(prepared: &Prepared, ran: &[&Ran]) -> Table {
    let rows: Vec<(&ColumnKey, &Row)> = ran.iter().flat_map(|r| r.rows.iter().map(move |row| (&r.key, row))).collect();
    let text = |f: &dyn Fn(&ColumnKey, &Row) -> Option<String>| rows.iter().map(|(k, r)| f(k, r)).collect::<Vec<_>>();
    let mut columns = vec![
        TableColumn::text("case", text(&|k, _| Some(k.case.clone()))),
        TableColumn::text("strip", text(&|k, _| Some(k.strip.clone()))),
        TableColumn::text("block", text(&|k, _| Some(k.block.clone()))),
        TableColumn::text("write", text(&|_, r| Some(r.write.to_string()))),
        TableColumn::text("horizon", text(&|_, r| Some(r.horizon.clone()))),
        TableColumn::text("seam", text(&|_, r| Some(r.seam.clone()).filter(|s| !s.is_empty()))),
        TableColumn::text("material", text(&|_, r| Some(r.material.clone()))),
        TableColumn::text("bench", text(&|_, r| r.bench.clone())),
        TableColumn::text("became", text(&|_, r| r.became.clone())),
        TableColumn::adding("units", "", AddUp::Sum, rows.iter().map(|(_, r)| Some(r.units as f64)).collect()),
        TableColumn::text("from", text(&|_, r| Some(r.sources.join("; ")))),
    ];
    let fields = prepared.fields.list();
    for (i, f) in fields.iter().enumerate() {
        let values = rows.iter().map(|(_, r)| Some(r.values[i]).filter(|v| !v.is_nan())).collect();
        // A thickness adds down a column and an area is a column's largest; across blocks neither adds up, so a report
        // shows them on each row and totals neither.
        let across = if matches!(f.kind, FieldKind::Thickness | FieldKind::Area) { None } else { Some(f.add_up.clone()) };
        columns.push(TableColumn { name: f.name.clone(), unit: unit_of_kind(f.kind).to_string(), add_up: across, values: zd_core::ColumnValues::Number(values) });
    }
    if let Some(d) = fields.iter().position(|f| f.kind == FieldKind::Density) {
        let v = prepared.fields.volume();
        let values = rows.iter().map(|(_, r)| Some(r.values[v] * r.values[d]).filter(|t| !t.is_nan())).collect();
        columns.push(TableColumn::adding("tonnes", "t", AddUp::Sum, values));
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
        changes.push(Change::Create { name: name.clone(), layer: p.layer.clone(), attributes: Attributes::new(), body: Body::Table(Arc::new(table)) });
        messages.push(format!("{name}: {} block{}, {rows} row{}", of.len(), plural(of.len()), plural(rows)));
    }
    let steps: Vec<&str> = prepared.steps.iter().map(|(n, _, _)| *n).collect();
    let mut head = format!(
        "{} of {} block{} ran through {}",
        ran.len(),
        ran.len() + stopped.len(),
        plural(ran.len() + stopped.len()),
        if steps.is_empty() { "no steps".to_string() } else { steps.join(", ") }
    );
    let volume: f64 = ran.iter().map(|r| r.input_volume).sum();
    let worst = ran.iter().map(|r| (r.imbalance / r.input_volume.max(1.0)).abs()).fold(0.0, f64::max);
    head.push_str(&format!("; {volume:.1} m3 in, every column balanced to {worst:.1e} of its volume; {:.0} ms", started.elapsed().as_secs_f64() * 1000.0));
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
    for (what, (n, v)) in counts {
        let code = if what.contains("small volume") {
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
        outcome.notes.push(Note::warning(Code::ReservesOutOfRange, format!("{} block{} {check}: {}", blocks.len(), plural(blocks.len()), words.join(", "))));
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
        let named: Vec<&str> = prepared.unplaced.iter().take(NAMED).map(String::as_str).collect();
        outcome.notes.push(Note::warning(Code::ReservesBlockStopped, format!("{n} solid{} with no strip or block left out: {}", plural(n), named.join(", "))));
    }
    // A field read from blocks that some have no value for, said once with how many.
    for (i, f) in prepared.dataflow.fields.iter().enumerate() {
        if !matches!(f.source, FieldSource::Attribute { .. } | FieldSource::Parameter { .. }) {
            continue;
        }
        let missing = ran.iter().flat_map(|r| r.rows.iter()).filter(|row| row.values[i].is_nan()).count();
        if missing > 0 {
            outcome.notes.push(Note::new(Severity::Info, Some(Code::ReservesValueMissing), format!("{missing} row{} written with no {}: no block attribute or parameter row gave it", plural(missing), f.field.name)));
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
    let mut fields = vec![
        field("volume", FieldKind::Volume, AddUp::Sum, FieldSource::Volume),
        field("thickness", FieldKind::Thickness, AddUp::Sum, FieldSource::Thickness),
        field("area", FieldKind::Area, AddUp::Max, FieldSource::PlanArea),
        field(&density, FieldKind::Density, AddUp::Volume, FieldSource::Attribute { attribute: density.clone() }),
    ];
    for q in &qualities {
        fields.push(field(q, FieldKind::Quality, AddUp::Mass { density: density.clone() }, FieldSource::Attribute { attribute: q.clone() }));
    }
    let steps = STEPS.iter().map(|(name, _)| DataflowStep::on(name)).collect();
    let mut columns = vec![
        ValueColumn::number("min_volume", "m3"),
        ValueColumn::number("max_thickness", "m"),
        ValueColumn::number("max_area", "m2"),
        ValueColumn::number("underburden_thickness", "m"),
    ];
    for q in std::iter::once(&density).chain(&qualities) {
        let unit = if *q == density { "t/m3" } else { "" };
        for end in ["min", "max", "default"] {
            columns.push(ValueColumn::number(&format!("{q}_{end}"), unit));
        }
    }
    let row = |material: &str| ParameterRow { when: [("material".to_string(), material.to_string())].into(), values: BTreeMap::new() };
    let default = ParameterRow { when: BTreeMap::new(), values: [("underburden_thickness".to_string(), ParamValue::Number(UNDERBURDEN_THICKNESS))].into() };
    let table = ParameterTable { filters: vec!["material".into(), "seam".into()], columns, rows: vec![row("waste"), row("coal"), default] };
    (table, Dataflow { fields, steps, parameters: Vec::new() })
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
