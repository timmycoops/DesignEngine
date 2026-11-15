//! `reserves.run`: a reserves dataflow run over blocks (decisions 0070 and 0072, workstream 2b). A column a case, strip
//! and block, each block's horizon (and bench) a unit with its fields' values, run through the dataflow's steps in order
//! across the cores; what the steps write is a results table a case. A block that can't run (a horizon not in the stack,
//! a setting no parameter row gives) is left out with why; the rest go on.
//!
//! The steps are named and registered here, so a new one slots in by name (decision 0072, point 9). This workstream ships
//! the structural ones, in Spry's order: `initialise` (the stack order, benches top down), `in_situ` (each horizon's
//! benches written as they are, before anything is joined) and `join_benches` (each horizon's benches joined into one
//! unit with their in situ shares, as working sections start in Spry, so later writes put pieces back by share). The engineering steps (working sections to clean coal) come in 0070's workstreams
//! 3 and 4.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{
    AttrValue, Attributes, Body, Change, Dataflow, FieldSource, Object, ObjectId, ObjectKind, ParamValue, ParameterTable, Table, TableColumn, TriMesh, attribute_ci, attribute_word,
};
use zd_reserves::{AddUp, Bench, Column, ColumnKey, FieldKind, Fields, Labels, Origin, Unit};

use super::blocks::horizon_order;
use super::in_parallel_watched;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Severity};

/// The case a block belongs to when it carries none: the workflow's own steps (decision 0066).
const BASE: &str = "Base";

/// Blocks named in a note before the rest are counted.
const NAMED: usize = 5;

/// What a step does to a column. Registered by name; a dataflow lists names.
type StepFn = fn(&mut ColumnRun<'_>) -> Result<(), String>;

/// The steps a dataflow can name, in Spry's order, with what each does.
pub const STEPS: [(&str, &str); 3] = [
    ("initialise", "puts the column in the seam model's stack order, a horizon's benches top down"),
    ("in_situ", "writes each horizon's benches as they are, before anything is joined"),
    ("join_benches", "joins each horizon's benches into one unit with their in situ shares, so later writes put pieces back by share"),
];

fn step_fn(name: &str) -> Option<StepFn> {
    match name {
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

/// A column in work: the column, its horizons' names by stack place, and what its steps have written.
pub struct ColumnRun<'a> {
    pub column: Column,
    horizons: &'a [String],
    rows: Vec<Row>,
}

/// A block left out, and why.
struct Stopped {
    why: String,
}

struct Ran {
    key: ColumnKey,
    rows: Vec<Row>,
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
    steps: Vec<(&'static str, StepFn)>,
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
        steps.push((name, f));
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
            became: u.became.as_ref().map(|b| format!("{:?}", b.kind).to_lowercase()),
            values: fields.list().iter().zip(u.values()).map(|(f, v)| (f.name.clone(), *v)).collect(),
        })
        .collect();
    StepView { step: step.to_string(), units }
}

/// One column: its blocks made units, then each step in turn. With `preview`, the column after each step.
fn run_column(prepared: &Prepared, key: &ColumnKey, blocks: &[Arc<Object>], mut preview: Option<&mut Vec<StepView>>) -> Result<Ran, Stopped> {
    let stop = |why: String| Stopped { why };
    let mut column = Column::new(key.clone(), prepared.fields.clone());
    for object in blocks {
        let (stack, labels, bench, values) = unit_of(prepared, object).map_err(stop)?;
        let origin = Origin { object: object.id.to_string(), name: object.name.clone() };
        column.push_bench_input(origin, stack, labels, bench, values).map_err(|e| stop(e.to_string()))?;
    }
    let volume = prepared.fields.volume();
    let input_volume = column.balance()[volume];
    let mut run = ColumnRun { column, horizons: &prepared.horizons, rows: Vec::new() };
    if let Some(p) = preview.as_deref_mut() {
        p.push(view(&run, "as input"));
    }
    for (name, step) in &prepared.steps {
        step(&mut run).map_err(|why| stop(format!("{}/{} at {name}: {why}", key.strip, key.block)))?;
        if let Some(p) = preview.as_deref_mut() {
            p.push(view(&run, name));
        }
    }
    let imbalance = input_volume - run.column.balance()[volume];
    Ok(Ran { key: key.clone(), rows: run.rows, imbalance, input_volume })
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

/// `initialise`: the seam model's stack order, a horizon's benches top down.
fn initialise(run: &mut ColumnRun<'_>) -> Result<(), String> {
    run.column.sort();
    Ok(())
}

/// `join_benches`: each horizon's benches joined into one unit, each bench's in situ share of its volume kept.
fn join_benches(run: &mut ColumnRun<'_>) -> Result<(), String> {
    run.column.join_benches();
    Ok(())
}

/// `in_situ`: each horizon's units put back on their benches and written, a row a horizon and bench.
fn in_situ(run: &mut ColumnRun<'_>) -> Result<(), String> {
    let groups = run.column.runs(|a, b| a.stack == b.stack && a.in_situ == b.in_situ);
    for g in groups {
        let first = &run.column.units()[g.start];
        let (horizon, seam, material) = (run.horizons[first.stack as usize].clone(), first.in_situ.seam.clone(), first.in_situ.material.clone());
        for row in run.column.by_bench(g.clone()) {
            let sources = sources(&run.column, &row);
            run.rows.push(Row {
                write: "in_situ",
                horizon: horizon.clone(),
                seam: seam.clone(),
                material: material.clone(),
                bench: row.bench.as_ref().map(|b| b.label.clone()),
                became: None,
                units: sources.len(),
                sources,
                values: row.values,
            });
        }
    }
    Ok(())
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
    let steps: Vec<&str> = prepared.steps.iter().map(|(n, _)| *n).collect();
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
