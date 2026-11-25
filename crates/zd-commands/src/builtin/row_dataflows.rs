//! A case's row dataflows (the Build Plan's "Reserves case plan", decisions 2 to 5, `docs/decisions/0105-reserves-cases.md`):
//! the dataflows a reserves table's setup lists after its reserves dataflow, each run on the table's rows. A formula step
//! is `def formula(row, p):` on each row in the dataflow's range, reading the row's words and numbers (the columns of the
//! reserves run and of the dataflows above it) and its settings, the first parameter row that fits the row's words; a table
//! script step is `def run(t, p):` on the whole table. A dataflow writes only its own columns, `Haulage\distance`, and a row
//! outside its range has them empty, never 0. `table.run_dataflow` runs one; running one never runs another (rule 4).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, ColumnKind, ColumnValues, Dataflow, Object, ObjectId, ObjectKind, ParamValue, ParameterTable, Table, TableColumn, TableSetup};
use zd_script::{ColumnInput, Declared, FieldDecl, Formula, ScriptValue, UnitInput};

use crate::{Code, Command, CommandError, CommandSpec, Field, Inputs, Note, Outcome};

/// One of a case's row dataflows, read: its object's name, the dataflow, the parameter tables it reads in order, its range
/// and the columns it writes, each with the field it's written from.
pub struct RowFlow {
    pub id: ObjectId,
    pub name: String,
    pub flow: Arc<Dataflow>,
    pub table_ids: Vec<ObjectId>,
    pub tables: Vec<Arc<ParameterTable>>,
    pub range: BTreeMap<String, String>,
    /// (field, column) for each field then each word field.
    pub columns: Vec<(String, String)>,
}

/// Reads an object, for the run's record or not.
pub type Get<'a> = &'a dyn Fn(ObjectId) -> Result<Arc<Object>, CommandError>;

/// One row dataflow of a setup read: a dataflow that is a row dataflow and holds together, and its parameter tables.
pub fn row_flow(get: Get<'_>, listed: &zd_core::RowDataflow) -> Result<RowFlow, CommandError> {
    let object = get(listed.dataflow)?;
    let Body::Dataflow(flow) = &object.body else {
        return Err(CommandError::WrongKind { id: listed.dataflow, expected: ObjectKind::Dataflow, found: object.kind() });
    };
    if !flow.rows {
        return Err(CommandError::Invalid(format!(
            "{} isn't a row dataflow: a case's dataflows after its reserves dataflow work on the table's rows (`rows`), reading the columns above them",
            object.name
        )));
    }
    flow.check().map_err(|e| CommandError::Invalid(format!("{}: {e}", object.name)))?;
    let table_ids = listed.tables.clone().unwrap_or_else(|| flow.parameters.clone());
    let mut tables = Vec::with_capacity(table_ids.len());
    for &id in &table_ids {
        let t = get(id)?;
        let Body::ParameterTable(t) = &t.body else {
            return Err(CommandError::Invalid(format!("{} reads {}, which isn't a parameter table", object.name, t.name)));
        };
        tables.push(t.clone());
    }
    Ok(RowFlow { id: object.id, name: object.name.clone(), flow: flow.clone(), table_ids, tables, range: listed.range.clone(), columns: flow.row_columns(&object.name) })
}

/// Every row dataflow of a setup read, in order: each listed once, no two writing one column (decision 5).
pub fn row_flows(get: Get<'_>, setup: &TableSetup) -> Result<Vec<RowFlow>, CommandError> {
    let mut out: Vec<RowFlow> = Vec::with_capacity(setup.row_dataflows.len());
    for listed in &setup.row_dataflows {
        if listed.dataflow == setup.dataflow {
            return Err(CommandError::Invalid("the case's reserves dataflow is listed again as a row dataflow: it runs first, once".into()));
        }
        let flow = row_flow(get, listed)?;
        if out.iter().any(|f| f.id == flow.id) {
            return Err(CommandError::Invalid(format!("{} is listed twice in the case: a dataflow runs once a case", flow.name)));
        }
        for (_, column) in &flow.columns {
            if let Some(other) = out.iter().find(|f| f.columns.iter().any(|(_, c)| c == column)) {
                return Err(CommandError::Invalid(format!("{} and {} both write {column}: a column has one writer; give one field another name", other.name, flow.name)));
            }
        }
        out.push(flow);
    }
    Ok(out)
}

/// The columns every row dataflow of a setup writes, read without recording: what a reserves Run again keeps by each
/// leaf's key until those dataflows run again. A dataflow gone or not a row dataflow writes none here.
pub fn row_columns_of(project: &zd_core::Project, setup: &TableSetup) -> Vec<String> {
    let mut out = Vec::new();
    for listed in &setup.row_dataflows {
        if let Some(o) = project.object(listed.dataflow)
            && let Body::Dataflow(d) = &o.body
            && d.rows
        {
            out.extend(d.row_columns(&o.name).into_iter().map(|(_, c)| c));
        }
    }
    out
}

/// Whether a case's row dataflows fit its table (decision 5): none writes a column the reserves run wrote, a person
/// entered or the table works out, and none reads a column only a dataflow below it writes. `written` are the columns
/// the row dataflows of the case as saved write, which the table may hold from their runs; any other column of the
/// table is the reserves run's or a person's. Reads are found in each script's source, by the column's name as written
/// (`row["Costs\\cost"]`).
pub fn check_case(table: &Table, flows: &[RowFlow], written: &[String]) -> Result<(), String> {
    for f in flows {
        for (_, column) in &f.columns {
            if let Some(c) = table.column(column).filter(|c| !(c.kind.is_written() && written.contains(&c.name))) {
                let who = if c.kind.is_written() { "the reserves run writes" } else { "a person entered" };
                return Err(format!("{} writes {column}, which {who}: a column has one writer; give the field another name", f.name));
            }
            if table.calculated.iter().any(|c| c.name == *column) {
                return Err(format!("{} writes {column}, a calculated column of the table: give the field another name", f.name));
            }
        }
    }
    for (i, f) in flows.iter().enumerate() {
        let scripts: Vec<&str> = f.flow.steps.iter().filter_map(|s| s.script()).collect();
        for later in &flows[i + 1..] {
            for (_, column) in &later.columns {
                let escaped = column.replace('\\', "\\\\");
                let quoted = |c: &str| scripts.iter().any(|s| s.contains(&format!("\"{c}\"")) || s.contains(&format!("'{c}'")));
                if quoted(column) || quoted(&escaped) {
                    return Err(format!("{} reads {column}, which {} writes after it: a dataflow reads only the columns of those above it; move it below {}, or read another column", f.name, later.name, later.name));
                }
            }
        }
    }
    Ok(())
}

/// The columns a row dataflow's filters can name, case aside: its range's, each formula step's and every row's of its
/// settings tables. A row's attributes need hold only these.
fn filter_keys(f: &RowFlow) -> std::collections::HashSet<String> {
    let mut keys: std::collections::HashSet<String> = f.range.keys().map(|k| k.to_lowercase()).collect();
    for s in &f.flow.steps {
        if let Some(formula) = &s.formula {
            keys.extend(formula.when.keys().map(|k| k.to_lowercase()));
        }
    }
    for t in &f.tables {
        keys.extend(t.filters.iter().map(|k| k.to_lowercase()));
        for r in &t.rows {
            keys.extend(r.when.keys().map(|k| k.to_lowercase()));
        }
    }
    keys
}

/// A row's words and numbers as attributes: what its range and a step's filter fit, and its parameter rows are matched on;
/// only the columns `keys` names (lower case), where given.
fn row_attributes_of(table: &Table, row: usize, keys: Option<&std::collections::HashSet<String>>) -> Attributes {
    let mut a = Attributes::new();
    for c in table.columns.iter().filter(|c| keys.is_none_or(|k| k.contains(&c.name.to_lowercase()))) {
        match &c.values {
            ColumnValues::Text(v) => {
                if let Some(w) = v[row].as_deref().filter(|w| !w.is_empty()) {
                    a.insert(c.name.clone(), AttrValue::Text(w.to_string()));
                }
            }
            ColumnValues::Number(v) => {
                if let Some(x) = v[row] {
                    a.insert(c.name.clone(), AttrValue::Number(x));
                }
            }
        }
    }
    a
}

/// A row's parameters: each column of the tables, from the first row that fits the row's words.
fn row_parameters(tables: &[Arc<ParameterTable>], words: &Attributes) -> zd_script::Values {
    let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
    let mut out: Vec<(&str, ScriptValue)> = Vec::new();
    for t in tables {
        for c in &t.columns {
            if out.iter().any(|(n, _)| *n == c.name) {
                continue;
            }
            let value = match ParameterTable::lookup_in(&refs, words, &c.name).map(|f| f.value) {
                Some(ParamValue::Number(x)) => ScriptValue::Number(*x),
                Some(ParamValue::Text(w)) => ScriptValue::Text(w.clone()),
                None => ScriptValue::None,
            };
            out.push((c.name.as_str(), value));
        }
    }
    zd_script::values(out)
}

/// What a row dataflow's run did: the rows it ran on and left out, each step's stops, and what its scripts said.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RowsRan {
    pub rows: usize,
    pub outside: usize,
    /// (step, rows it stopped on, the first named with its line).
    pub stopped: Vec<(String, usize, String)>,
    pub said: Vec<String>,
    pub checks: Vec<String>,
    /// Each formula step's reads off its source (the table's columns and the dataflow's own fields), by step.
    #[serde(skip)]
    pub reads: BTreeMap<String, Vec<String>>,
    /// The fields each step set on a row, by step.
    #[serde(skip)]
    pub sets: BTreeMap<String, Vec<String>>,
}

/// How many rows a formula runs on at once; a batch it stops in is run again a row at a time.
const BATCH: usize = 256;

/// A row's label for a note: its case and its levels' words.
fn label(table: &Table, row: usize) -> String {
    let words: Vec<String> = std::iter::once("case".to_string())
        .chain(zd_core::leaves::levels(table))
        .filter_map(|name| match table.column(&name).map(|c| &c.values) {
            Some(ColumnValues::Text(v)) => v[row].clone().filter(|w| !w.is_empty()),
            _ => None,
        })
        .collect();
    if words.is_empty() { format!("row {}", row + 1) } else { words.join(" ") }
}

/// A row dataflow run on a table (decisions 2, 4 and 5): its columns made again empty, its steps run in order on the rows
/// in its range, a formula on those its filter fits, a table script on the table. A formula setting a column not its own,
/// or a table script changing one or giving back one it doesn't list, is refused. A row a formula stops on keeps its
/// columns empty, said with the first; the rest go on.
pub fn run_row_flow(table: &Table, f: &RowFlow, libraries: zd_script::Libraries, lookups: Arc<zd_script::Lookups>, project_tables: &[(String, Arc<Table>)]) -> Result<(Table, RowsRan), String> {
    run_row_flow_to(table, f, usize::MAX, libraries, lookups, project_tables)
}

/// A row dataflow run as `run_row_flow` runs it, its first `steps` steps that are on only (none: its columns empty).
fn run_row_flow_to(table: &Table, f: &RowFlow, steps: usize, libraries: zd_script::Libraries, lookups: Arc<zd_script::Lookups>, project_tables: &[(String, Arc<Table>)]) -> Result<(Table, RowsRan), String> {
    run_row_flow_keeping(table, f, steps, &[], libraries, lookups, project_tables).map(|(t, ran, _)| (t, ran))
}

/// Its own columns as they stood after so many steps.
type Kept = BTreeMap<usize, Vec<TableColumn>>;

/// A row dataflow run to `steps`, keeping its own columns as they stood once each of `keep` steps had run (0: empty).
fn run_row_flow_keeping(
    table: &Table,
    f: &RowFlow,
    steps: usize,
    keep: &[usize],
    libraries: zd_script::Libraries,
    lookups: Arc<zd_script::Lookups>,
    project_tables: &[(String, Arc<Table>)],
) -> Result<(Table, RowsRan, Kept), String> {
    let mut kept = Kept::new();
    let mut t = table.clone();
    let own: Vec<&str> = f.columns.iter().map(|(_, c)| c.as_str()).collect();
    t.columns.retain(|c| !own.contains(&c.name.as_str()));
    let n = t.rows();
    let keys = filter_keys(f);
    let in_range: Vec<usize> = (0..n).filter(|&r| zd_core::filter_fits(&f.range, &row_attributes_of(&t, r, Some(&keys)))).collect();
    let mut ran = RowsRan { rows: in_range.len(), outside: n - in_range.len(), ..Default::default() };
    // Its own columns, empty, numbers with their unit and rule, words after.
    let mut values: BTreeMap<String, Vec<Option<f64>>> = BTreeMap::new();
    let mut words: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    for d in &f.flow.fields {
        values.insert(d.field.name.clone(), vec![None; n]);
    }
    for w in &f.flow.words {
        words.insert(w.name.clone(), vec![None; n]);
    }
    let column_of = |field: &str| f.columns.iter().find(|(name, _)| name == field).map(|(_, c)| c.clone()).expect("a field of its own");
    let unit_of = |d: &zd_core::DataflowField| f.flow.units.get(&d.field.name).cloned().unwrap_or_else(|| super::reserves_run::unit_of_kind(d.field.kind).to_string());
    let put = |t: &mut Table, values: &BTreeMap<String, Vec<Option<f64>>>, words: &BTreeMap<String, Vec<Option<String>>>| {
        t.columns.retain(|c| !own.contains(&c.name.as_str()));
        for d in &f.flow.fields {
            t.columns.push(TableColumn { name: column_of(&d.field.name), unit: unit_of(d), add_up: Some(d.field.add_up.clone()), kind: ColumnKind::Written, values: ColumnValues::Number(values[&d.field.name].clone()) });
        }
        for w in &f.flow.words {
            t.columns.push(TableColumn { name: column_of(&w.name), unit: String::new(), add_up: None, kind: ColumnKind::Written, values: ColumnValues::Text(words[&w.name].clone()) });
        }
    };
    let snapshot = |t: &Table| t.columns.iter().filter(|c| own.contains(&c.name.as_str())).cloned().collect::<Vec<_>>();
    let mut done = 0;
    for s in f.flow.steps.iter().filter(|s| s.on).take(steps) {
        put(&mut t, &values, &words);
        if keep.contains(&done) {
            kept.insert(done, snapshot(&t));
        }
        done += 1;
        if let Some(formula) = &s.formula {
            let rows: Vec<usize> = in_range.iter().copied().filter(|&r| zd_core::filter_fits(&formula.when, &row_attributes_of(&t, r, Some(&keys)))).collect();
            run_formula_step(&t, f, &s.step, &formula.script, &rows, &keys, &libraries, &lookups, &mut values, &mut words, &mut ran)?;
        } else if let Some(table_step) = &s.table {
            let script = super::table_script::compile_table_script(&table_step.script, libraries.clone()).map_err(|e| format!("the table script step {}: {e}", s.step))?;
            let r = script
                .run(zd_script::TableInput {
                    name: f.name.clone(),
                    table: Arc::new(t.clone()),
                    tables: project_tables.to_vec(),
                    parameters: super::table_script::table_parameters(&f.tables),
                    work: Arc::new(super::GridWork),
                    lookups: lookups.clone(),
                })
                .map_err(|p| format!("the table script step {} stopped: {p}", s.step))?;
            super::table_script::keeps_rows(&t, &r.table).map_err(|e| format!("the table script step {}: {e}", s.step))?;
            for c in &r.table.columns {
                match f.columns.iter().find(|(_, col)| *col == c.name) {
                    Some((field, _)) => match (&c.values, values.get_mut(field), words.get_mut(field)) {
                        (ColumnValues::Number(v), Some(own), _) => {
                            for &row in &in_range {
                                own[row] = v[row];
                            }
                        }
                        (ColumnValues::Text(v), _, Some(own)) => {
                            for &row in &in_range {
                                own[row] = v[row].clone();
                            }
                        }
                        _ => return Err(format!("the table script step {} gave back {} as {}, and {} has it as {}", s.step, c.name, if matches!(c.values, ColumnValues::Text(_)) { "words" } else { "numbers" }, f.name, if words.contains_key(field) { "words" } else { "numbers" })),
                    },
                    None => match t.column(&c.name) {
                        Some(was) if was.values == c.values => {}
                        Some(_) => return Err(format!("the table script step {} changed {}, which {} doesn't write: a dataflow writes only its own columns", s.step, c.name, f.name)),
                        None => return Err(format!("the table script step {} gave back {}, which {} doesn't list as a field: add the field, or leave the column out", s.step, c.name, f.name)),
                    },
                }
            }
            ran.sets.insert(s.step.clone(), f.columns.iter().filter(|(_, col)| r.table.column(col).map(|c| &c.values) != t.column(col).map(|c| &c.values)).map(|(field, _)| field.clone()).collect());
            ran.said.extend(r.said.iter().map(|w| format!("{} said: {w}", s.step)));
            ran.said.extend(r.notes.iter().map(|w| format!("{}: {w}", s.step)));
            ran.checks.extend(r.checks.iter().map(|w| format!("{}: {w}", s.step)));
        }
    }
    put(&mut t, &values, &words);
    if keep.contains(&done) {
        kept.insert(done, snapshot(&t));
    }
    Ok((t, ran, kept))
}

/// `base` with these columns in place of its own of the same names.
fn with_columns(base: &Table, columns: &[TableColumn]) -> Table {
    let mut t = base.clone();
    t.columns.retain(|c| !columns.iter().any(|x| x.name == c.name));
    t.columns.extend(columns.iter().cloned());
    t
}

/// One formula step on its rows: compiled against the table's columns and the dataflow's own fields, each row handed
/// over with the numbers it reads, its words and its parameters, its sets kept on its own fields.
#[allow(clippy::too_many_arguments)]
fn run_formula_step(
    t: &Table,
    f: &RowFlow,
    step: &str,
    script: &str,
    rows: &[usize],
    keys: &std::collections::HashSet<String>,
    libraries: &zd_script::Libraries,
    lookups: &Arc<zd_script::Lookups>,
    values: &mut BTreeMap<String, Vec<Option<f64>>>,
    words: &mut BTreeMap<String, Vec<Option<String>>>,
    ran: &mut RowsRan,
) -> Result<(), String> {
    let own_columns: Vec<&str> = f.columns.iter().map(|(_, c)| c.as_str()).collect();
    let numbers: Vec<&TableColumn> = t.columns.iter().filter(|c| matches!(c.values, ColumnValues::Number(_)) && !own_columns.contains(&c.name.as_str())).collect();
    let texts: Vec<&TableColumn> = t.columns.iter().filter(|c| matches!(c.values, ColumnValues::Text(_)) && !own_columns.contains(&c.name.as_str())).collect();
    let mut fields: Vec<FieldDecl> = numbers.iter().map(|c| FieldDecl::number(&c.name, &c.unit)).collect();
    for d in &f.flow.fields {
        if fields.iter().any(|x| x.name == d.field.name) || texts.iter().any(|c| c.name == d.field.name) {
            return Err(format!("{}'s field {} has the name of a column of the table: give it another", f.name, d.field.name));
        }
        fields.push(FieldDecl::number(&d.field.name, f.flow.units.get(&d.field.name).map_or("", String::as_str)));
    }
    for w in &f.flow.words {
        if fields.iter().any(|x| x.name == w.name) || texts.iter().any(|c| c.name == w.name) {
            return Err(format!("{}'s field {} has the name of a column of the table: give it another", f.name, w.name));
        }
        fields.push(FieldDecl::text(&w.name));
    }
    let mut params: Vec<FieldDecl> = Vec::new();
    for table in &f.tables {
        for c in &table.columns {
            if !params.iter().any(|p| p.name == c.name) {
                params.push(if c.text { FieldDecl::text(&c.name) } else { FieldDecl::number(&c.name, &c.unit) });
            }
        }
    }
    let declared = Declared { fields, labels: texts.iter().map(|c| c.name.clone()).collect(), parameters: params, libraries: libraries.clone(), lookups: lookups.clone() };
    let formula = Formula::compile(script, declared).map_err(|problems| {
        let said: Vec<String> = problems.iter().map(|p| p.to_string()).collect();
        format!("the formula step {step}: {}", said.join("; "))
    })?;
    // What it reads: the names read by dot off its source, and the columns named in brackets (`row["Loss\\Roof"]`), as
    // written or with their backslashes doubled; none found, every column.
    let mut read: Vec<String> = formula.fields_read();
    for c in &numbers {
        let doubled = c.name.replace('\\', "\\\\");
        let quoted = [c.name.as_str(), doubled.as_str()].iter().any(|n| script.contains(&format!("\"{n}\"")) || script.contains(&format!("'{n}'")));
        if quoted && !read.contains(&c.name) {
            read.push(c.name.clone());
        }
    }
    ran.reads.insert(step.to_string(), read.clone());
    let unit = |row: usize| -> UnitInput {
        let mut v: Vec<(&str, ScriptValue)> = Vec::new();
        for c in &numbers {
            if read.is_empty() || read.contains(&c.name) {
                let ColumnValues::Number(x) = &c.values else { unreachable!() };
                v.push((c.name.as_str(), x[row].map_or(ScriptValue::None, ScriptValue::Number)));
            }
        }
        for c in &texts {
            let ColumnValues::Text(x) = &c.values else { unreachable!() };
            v.push((c.name.as_str(), x[row].clone().filter(|w| !w.is_empty()).map_or(ScriptValue::None, ScriptValue::Text)));
        }
        for (name, x) in values.iter() {
            v.push((name.as_str(), x[row].map_or(ScriptValue::None, ScriptValue::Number)));
        }
        for (name, x) in words.iter() {
            v.push((name.as_str(), x[row].clone().map_or(ScriptValue::None, ScriptValue::Text)));
        }
        UnitInput { fields: zd_script::values(v), block: zd_script::values([]), parameters: row_parameters(&f.tables, &row_attributes_of(t, row, Some(keys))), section: None, rom_coal: None }
    };
    let input = ColumnInput { units: rows.iter().map(|&r| unit(r)).collect(), ..Default::default() };
    let mut stops = 0usize;
    let mut first = String::new();
    let mut sets_all: Vec<zd_script::UnitSets> = Vec::new();
    let all: Vec<usize> = (0..rows.len()).collect();
    for batch in all.chunks(BATCH) {
        match formula.run(&input, batch) {
            Ok(sets) => sets_all.extend(sets),
            Err(_) => {
                for &u in batch {
                    match formula.run(&input, &[u]) {
                        Ok(sets) => sets_all.extend(sets),
                        Err(p) => {
                            stops += 1;
                            if first.is_empty() {
                                let at = p.line.map_or(String::new(), |l| format!("line {l}: "));
                                first = format!("{} ({at}{})", label(t, rows[u]), p.message);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut set: Vec<String> = Vec::new();
    for s in sets_all {
        let row = rows[s.unit];
        for (name, value) in &s.sets {
            if !set.contains(name) {
                set.push(name.clone());
            }
            if let Some(column) = values.get_mut(name) {
                column[row] = match value {
                    ScriptValue::Number(x) if x.is_finite() => Some(*x),
                    ScriptValue::None => None,
                    other => return Err(format!("the formula step {step} set {name} to {other} on {}: {name} is a number", label(t, row))),
                };
            } else if let Some(column) = words.get_mut(name) {
                column[row] = match value {
                    ScriptValue::Text(w) => Some(w.clone()).filter(|w| !w.is_empty()),
                    ScriptValue::None => None,
                    other => Some(other.to_string()),
                };
            } else {
                return Err(format!("the formula step {step} sets {name}, which {} doesn't write: a dataflow writes only its own fields' columns", f.name));
            }
        }
        ran.said.extend(s.said.iter().map(|w| format!("{step} said: {w}")));
    }
    ran.sets.insert(step.to_string(), set);
    if stops > 0 {
        ran.stopped.push((step.to_string(), stops, first));
    }
    Ok(())
}

/// `table.run_dataflow`: one of a case's row dataflows run on its table's rows, in place (decisions 2, 6 and 14).
pub struct RunRowDataflow;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRowDataflowParams {
    /// The reserves table, the case.
    pub table: ObjectId,
    /// One of the row dataflows its setup lists.
    pub dataflow: ObjectId,
}

impl Command for RunRowDataflow {
    type Params = RunRowDataflowParams;
    const NAME: &'static str = "table.run_dataflow";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run one of a case's row dataflows on its reserves table's rows in its range, writing its own columns; the dataflows below it aren't run",
            inputs: vec![Field::object("table", ObjectKind::Table, "The reserves table, the case"), Field::object("dataflow", ObjectKind::Dataflow, "A row dataflow its setup lists")],
            params: Vec::new(),
            outputs: vec![Field::object("table", ObjectKind::Table, "The table, its columns from the dataflow written")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let started = Instant::now();
        let object = inputs.object(p.table)?;
        let Body::Table(table) = &object.body else {
            return Err(CommandError::WrongKind { id: p.table, expected: ObjectKind::Table, found: object.kind() });
        };
        let Some(setup) = &table.setup else {
            return Err(CommandError::Invalid(format!("{} has no setup listing its dataflows: run it again once, or save its setup", object.name)));
        };
        if p.dataflow == setup.dataflow {
            return Err(CommandError::Invalid(format!("that's {}'s reserves dataflow, which makes its rows: run it again (reserves.run with the table)", object.name)));
        }
        let Some(at) = setup.row_dataflows.iter().position(|r| r.dataflow == p.dataflow) else {
            let name = inputs.project().object(p.dataflow).map_or_else(|| p.dataflow.to_string(), |o| o.name.clone());
            return Err(CommandError::Invalid(format!("{name} isn't one of {}'s dataflows: add it to the case's list first", object.name)));
        };
        // The others read only to check the case, never recorded: a change to one below isn't this run's input.
        let project = inputs.project();
        let quiet = |id: ObjectId| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
        let flows = row_flows(&quiet, setup)?;
        check_case(table, &flows, &row_columns_of(project, setup)).map_err(CommandError::Invalid)?;
        // This one read for the record: the dataflow, its tables, the scripts it loads and the tables it looks up.
        let recorded = |id: ObjectId| inputs.object(id).cloned();
        let f = row_flow(&recorded, &setup.row_dataflows[at])?;
        let libraries = super::libraries_of(&|id| inputs.object(id).ok().cloned(), &f.flow.scripts).map_err(|e| CommandError::Invalid(format!("{}: {e}", f.name)))?;
        let named = super::lookups_by_id(&|id| inputs.object(id).ok().cloned(), &f.flow.lookups, &f.flow.lookup_names).map_err(|e| CommandError::Invalid(format!("{}: {e}", f.name)))?;
        let lookups = Arc::new(zd_script::Lookups { own: f.tables.clone(), named });
        let by_name = super::table_script::tables_by_name(inputs.objects());
        let (next, ran) = run_row_flow(table, &f, libraries, lookups, &by_name).map_err(|e| CommandError::Invalid(format!("{}: {e}", f.name)))?;
        let wrote: Vec<&str> = f.columns.iter().map(|(_, c)| c.as_str()).collect();
        let range = if f.range.is_empty() { String::new() } else { format!(" ({})", f.range.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ")) };
        let mut message = format!("{}: {} ran on {} of {} rows{range}, wrote {}", object.name, f.name, ran.rows, ran.rows + ran.outside, wrote.join(", "));
        if ran.outside > 0 {
            message.push_str(&format!("; {} rows outside its range left empty", ran.outside));
        }
        message.push_str(&format!("; {:.0} ms", started.elapsed().as_secs_f64() * 1000.0));
        let change = Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(next))) };
        let mut out = Outcome::new(vec![change]).message(message);
        for (step, n, first) in &ran.stopped {
            out.notes.push(Note::warning(Code::RowsStopped, format!("{}: {step} stopped on {n} row{}, its columns left empty there: {first}", f.name, if *n == 1 { "" } else { "s" })));
        }
        out.notes.extend(ran.said.iter().map(|s| Note::info(format!("{}: {s}", f.name))));
        out.notes.extend(ran.checks.iter().map(|c| Note::warning(Code::TableCheckOut, format!("{}: {c}", f.name))));
        Ok(out)
    }
}

/// One step of a row dataflow, for the Reserves case tab: its name, a formula on each row or a table script on the table,
/// its script, the rows its filter fits, what it reads off its source (the table's columns, the dataflow's own fields)
/// and the fields it set on a row when last run here.
#[derive(Debug, Clone, Serialize)]
pub struct RowStepInfo {
    pub name: String,
    pub kind: &'static str,
    pub script: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
    pub reads: Vec<String>,
    pub sets: Vec<String>,
}

/// One of a row dataflow's columns at a step: its field and column, unit and rule, whether it holds words, and added up
/// over the rows in its range by its rule before the step, after it, and as the table holds it from the last run.
#[derive(Debug, Clone, Serialize)]
pub struct RowColumnAt {
    pub field: String,
    pub column: String,
    pub unit: String,
    pub add_up: Option<zd_reserves::AddUp>,
    pub words: bool,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub last_run: Option<f64>,
}

/// A row in a row dataflow's range: its number in the table, its words, and its columns before and after the step, in
/// the order of `columns` (a number, words, or null for empty).
#[derive(Debug, Clone, Serialize)]
pub struct RowAt {
    pub row: usize,
    pub label: String,
    pub before: Vec<serde_json::Value>,
    pub after: Vec<serde_json::Value>,
}

/// A case's row dataflow at a step (Reserves case plan, decisions 4 and 8, workstream 6): its steps, its range and the
/// rows in it, each of its columns before and after the step and as the last run left it, the settings tables it reads,
/// and what the step stopped on or said. Run on what's saved, or `unsaved` as edited (its own dataflow, its tables).
#[derive(Debug, Clone, Serialize)]
pub struct RowDataflowAtStep {
    pub table: ObjectId,
    pub dataflow: ObjectId,
    pub name: String,
    /// 0 for its columns empty, before its first step; 1 after its first step.
    pub step: usize,
    pub steps: Vec<RowStepInfo>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub range: BTreeMap<String, String>,
    pub rows_in_range: usize,
    pub outside: usize,
    pub columns: Vec<RowColumnAt>,
    pub rows: Vec<RowAt>,
    /// Its settings tables, in the order read: id and name.
    pub tables: Vec<(ObjectId, String)>,
    pub stopped: Vec<String>,
    pub said: Vec<String>,
    pub ms: f64,
}

fn cell_of(t: &Table, column: &str, row: usize) -> serde_json::Value {
    match t.column(column).map(|c| &c.values) {
        Some(ColumnValues::Number(v)) => v[row].and_then(serde_json::Number::from_f64).map_or(serde_json::Value::Null, serde_json::Value::Number),
        Some(ColumnValues::Text(v)) => v[row].clone().map_or(serde_json::Value::Null, serde_json::Value::String),
        None => serde_json::Value::Null,
    }
}

/// One of a case's row dataflows run on its table to step `step` (0, none) and the step before it, on what's saved or
/// `unsaved` as edited: `unsaved.dataflow` stands for the row dataflow itself. Nothing is kept or saved.
pub fn row_dataflow_at_step(project: &zd_core::Project, table: ObjectId, dataflow: ObjectId, step: usize, unsaved: &super::Unsaved) -> Result<RowDataflowAtStep, CommandError> {
    let started = Instant::now();
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::Table(t) = &object.body else {
        return Err(CommandError::WrongKind { id: table, expected: ObjectKind::Table, found: object.kind() });
    };
    let Some(setup) = &t.setup else {
        return Err(CommandError::Invalid(format!("{} has no setup listing its dataflows", object.name)));
    };
    let Some(listed) = setup.row_dataflows.iter().find(|r| r.dataflow == dataflow) else {
        let name = project.object(dataflow).map_or_else(|| dataflow.to_string(), |o| o.name.clone());
        return Err(CommandError::Invalid(format!("{name} isn't one of {}'s row dataflows", object.name)));
    };
    let read = unsaved.reader(project)?;
    let get = |id: ObjectId| -> Result<Arc<Object>, CommandError> {
        let o = read(id)?;
        Ok(match (&unsaved.dataflow, id == dataflow) {
            (Some(edited), true) => {
                let mut o = (*o).clone();
                o.body = Body::Dataflow(edited.clone());
                Arc::new(o)
            }
            _ => o,
        })
    };
    let f = row_flow(&get, listed)?;
    let libraries = super::libraries_of(&|id| get(id).ok(), &f.flow.scripts).map_err(|e| CommandError::Invalid(format!("{}: {e}", f.name)))?;
    let named = super::lookups_by_id(&|id| get(id).ok(), &f.flow.lookups, &f.flow.lookup_names).map_err(|e| CommandError::Invalid(format!("{}: {e}", f.name)))?;
    let lookups = Arc::new(zd_script::Lookups { own: f.tables.clone(), named });
    let by_name = super::table_script::tables_by_name(project.objects());
    let on: Vec<&zd_core::DataflowStep> = f.flow.steps.iter().filter(|s| s.on).collect();
    if step > on.len() {
        return Err(CommandError::Invalid(format!("{} runs {} steps, not {step}", f.name, on.len())));
    }
    // One run to its end, its columns kept as they stood before the step and after it.
    let first = step.saturating_sub(1);
    let (all, ran_all, kept) = run_row_flow_keeping(t, &f, usize::MAX, &[first, step], libraries, lookups, &by_name).map_err(|e| CommandError::Invalid(format!("{}: {e}", f.name)))?;
    let before = with_columns(&all, &kept[&first]);
    let after = with_columns(&all, &kept[&step]);
    // What the steps up to this one stopped on and said.
    let upto: Vec<&str> = on[..step].iter().map(|s| s.step.as_str()).collect();
    let mine = |line: &str| upto.iter().any(|s| line.starts_with(&format!("{s} ")) || line.starts_with(&format!("{s}:")));
    let ran = RowsRan {
        stopped: ran_all.stopped.iter().filter(|(s, _, _)| upto.contains(&s.as_str())).cloned().collect(),
        said: ran_all.said.iter().filter(|l| mine(l)).cloned().collect(),
        ..RowsRan::default()
    };
    let keys = filter_keys(&f);
    let in_range: Vec<usize> = (0..t.rows()).filter(|&r| zd_core::filter_fits(&f.range, &row_attributes_of(t, r, Some(&keys)))).collect();
    let steps = on
        .iter()
        .map(|s| RowStepInfo {
            name: s.step.clone(),
            kind: if s.formula.is_some() { "formula" } else { "table" },
            script: s.script().unwrap_or_default().to_string(),
            when: s.formula.as_ref().map(|x| x.when.clone()).unwrap_or_default(),
            reads: ran_all.reads.get(&s.step).cloned().unwrap_or_default(),
            sets: ran_all.sets.get(&s.step).cloned().unwrap_or_default(),
        })
        .collect();
    let columns: Vec<RowColumnAt> = f
        .columns
        .iter()
        .map(|(field, column)| {
            let c = all.column(column);
            let words = matches!(c.map(|c| &c.values), Some(ColumnValues::Text(_)));
            let total = |x: &Table| if words { None } else { crate::reserves_view::add_up_column(x, column, &in_range) };
            RowColumnAt {
                field: field.clone(),
                column: column.clone(),
                unit: c.map(|c| c.unit.clone()).unwrap_or_default(),
                add_up: c.and_then(|c| c.add_up.clone()),
                words,
                before: total(&before),
                after: total(&after),
                last_run: if t.column(column).is_some() { total(t) } else { None },
            }
        })
        .collect();
    let rows = in_range
        .iter()
        .map(|&r| RowAt {
            row: r,
            label: label(t, r),
            before: columns.iter().map(|c| cell_of(&before, &c.column, r)).collect(),
            after: columns.iter().map(|c| cell_of(&after, &c.column, r)).collect(),
        })
        .collect();
    Ok(RowDataflowAtStep {
        table,
        dataflow,
        name: f.name.clone(),
        step,
        steps,
        range: f.range.clone(),
        rows_in_range: in_range.len(),
        outside: t.rows() - in_range.len(),
        columns,
        rows,
        tables: f.table_ids.iter().map(|&id| (id, project.object(id).map_or_else(|| id.to_string(), |o| o.name.clone()))).collect(),
        stopped: ran.stopped.iter().map(|(s, n, first)| format!("{s} stopped on {n} row{}: {first}", if *n == 1 { "" } else { "s" })).collect(),
        said: ran.said.clone(),
        ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}
