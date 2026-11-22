//! Table scripts (the Build Plan's "Tables and scripts plan", decisions 7 and 11, `docs/decisions/0099-tables-and-scripts.md`):
//! Starlark on a whole table, `def run(t, p):`, the grouping, joins and adding up done in Rust (`zd_script::TableWork`,
//! the grid's own add-up and order). One script runs three ways without being rewritten: as a task, `table.script`, making
//! a new table or putting columns on the one it ran on; as a dataflow step after the final writes, its columns the run's
//! own; and as a workflow step, as any command is.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ColumnKind, ColumnValues, Object, ObjectId, ObjectKind, ParamValue, ParameterTable, Table};
use zd_script::{Libraries, ScriptValue, TableInput, TableRan, TableScript, Values};

use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

/// Where a table script's table goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptInto {
    /// A new table of its own (a summary by strip, a reconciliation, a schedule input).
    #[default]
    New,
    /// Its new columns put on the table it ran on, as entered columns kept by each row's key: it must give back the
    /// same rows.
    Columns,
}

/// `table.script`: a table script run as a task (decision 11).
pub struct RunTableScript;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableScriptParams {
    /// The table it runs on, `t`.
    pub table: ObjectId,
    /// A script of the project, by id; or `source`, the script written in.
    #[serde(default)]
    pub script: Option<ObjectId>,
    #[serde(default)]
    pub source: Option<String>,
    /// The parameter tables `p` reads, first first.
    #[serde(default)]
    pub parameters: Vec<ObjectId>,
    #[serde(default)]
    pub into: ScriptInto,
    /// A new table's name: the table's and the script's by default.
    #[serde(default)]
    pub name: String,
    /// A new table's layer.
    #[serde(default = "tables_layer")]
    pub layer: String,
}

/// The layer a table script's new tables go on.
pub const TABLES_LAYER: &str = "tables";

fn tables_layer() -> String {
    TABLES_LAYER.into()
}

/// The engine's work for a table script: the grid's add-up by each column's rule, and its order.
pub struct GridWork;

impl zd_script::TableWork for GridWork {
    fn add_up(&self, table: &Table, column: &str, rows: &[usize]) -> Option<f64> {
        crate::reserves_view::add_up_column(table, column, rows)
    }

    fn groups(&self, table: &Table, by: &[String], rows: &[usize]) -> Vec<(Vec<String>, Vec<usize>)> {
        let words: Vec<Vec<String>> = by
            .iter()
            .map(|b| match table.column(b).map(|c| &c.values) {
                Some(ColumnValues::Text(v)) => v.iter().map(|w| w.clone().unwrap_or_default()).collect(),
                _ => vec![String::new(); table.rows()],
            })
            .collect();
        let mut out = Vec::new();
        groups_under(by, &words, Vec::new(), rows, &mut out);
        out
    }

    fn order(&self, a: &str, b: &str) -> std::cmp::Ordering {
        crate::reserves_view::natural(a, b)
    }
}

/// The groups under `path`, in the grid's order level by level (a seam in the stack's order, a bench top down).
fn groups_under(by: &[String], words: &[Vec<String>], path: Vec<String>, rows: &[usize], out: &mut Vec<(Vec<String>, Vec<usize>)>) {
    let depth = path.len();
    if depth == by.len() {
        out.push((path, rows.to_vec()));
        return;
    }
    for (word, under) in crate::reserves_view::children_of(&by[depth], &words[depth], rows) {
        let mut p = path.clone();
        p.push(word);
        groups_under(by, words, p, &under, out);
    }
}

/// The parameters a whole table reads: each column of the tables from the first row that fits no words in particular.
pub(crate) fn table_parameters(tables: &[Arc<ParameterTable>]) -> Values {
    let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
    let none = Attributes::new();
    let mut out: Vec<(&str, ScriptValue)> = Vec::new();
    for t in tables {
        for c in &t.columns {
            if out.iter().any(|(n, _)| *n == c.name) {
                continue;
            }
            let value = match ParameterTable::lookup_in(&refs, &none, &c.name).map(|f| f.value) {
                Some(ParamValue::Number(x)) => ScriptValue::Number(*x),
                Some(ParamValue::Text(w)) => ScriptValue::Text(w.clone()),
                None => continue,
            };
            out.push((c.name.as_str(), value));
        }
    }
    zd_script::values(out)
}

/// The project's tables a script may read by name, with their ids, the first of a name kept.
fn project_tables<'a>(objects: impl Iterator<Item = &'a Arc<Object>>) -> Vec<(String, ObjectId, Arc<Table>)> {
    let mut out: Vec<(String, ObjectId, Arc<Table>)> = Vec::new();
    for o in objects {
        if let Body::Table(t) = &o.body
            && !out.iter().any(|(n, _, _)| *n == o.name)
        {
            out.push((o.name.clone(), o.id, t.clone()));
        }
    }
    out
}

/// A table script compiled, every problem with its line.
pub fn compile_table_script(source: &str, libraries: Libraries) -> Result<TableScript, String> {
    TableScript::compile(source, libraries).map_err(|problems| problems.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("; "))
}

/// Whether `given` keeps `base`'s rows: as many, and the same words in every one of `base`'s word columns it has.
fn keeps_rows(base: &Table, given: &Table) -> Result<(), String> {
    if given.rows() != base.rows() {
        return Err(format!("it gave back {} rows for the table's {}: columns go on a table only with its own rows (make a new table instead)", given.rows(), base.rows()));
    }
    for c in &base.columns {
        let (ColumnValues::Text(a), Some(ColumnValues::Text(b))) = (&c.values, given.column(&c.name).map(|g| &g.values)) else { continue };
        if let Some(r) = (0..a.len()).find(|&r| a[r] != b[r]) {
            return Err(format!("row {} isn't the table's row {0} any more ({} {:?}, was {:?}): it sorted, filtered or grouped the rows; columns go on a table only with its own rows", r + 1, c.name, b[r], a[r]));
        }
    }
    Ok(())
}

/// A column of `given` that `base` has, changed: what a script mustn't do to a run's numbers (decision 3).
fn changed(base: &zd_core::TableColumn, given: &zd_core::TableColumn) -> bool {
    base.values != given.values
}

impl Command for RunTableScript {
    type Params = TableScriptParams;
    const NAME: &'static str = "table.script";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run a table script on a table: Starlark `def run(t, p):` grouping, joining and adding up in Rust, making a new table or putting columns on this one",
            inputs: vec![
                Field::object("table", ObjectKind::Table, "The table it runs on"),
                Field::object("script", ObjectKind::Script, "A script of the project; or the script written in"),
                Field::new("parameters", FieldType::Objects, "The parameter tables p reads"),
            ],
            params: vec![
                Field::new("source", FieldType::Text, "The script written in, where no script of the project is named"),
                Field::new("into", FieldType::Text, "new: a table of its own; columns: its new columns on this table"),
                Field::new("name", FieldType::Text, "A new table's name"),
                Field::new("layer", FieldType::Text, "A new table's layer"),
            ],
            outputs: vec![Field::object("table", ObjectKind::Table, "The table made, or the one it put columns on")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.table)?;
        let Body::Table(table) = &object.body else {
            return Err(CommandError::WrongKind { id: p.table, expected: ObjectKind::Table, found: object.kind() });
        };
        let (source, script_name) = match (&p.script, &p.source) {
            (Some(id), None) => {
                let s = inputs.object(*id)?;
                let Body::Script(script) = &s.body else {
                    return Err(CommandError::WrongKind { id: *id, expected: ObjectKind::Script, found: s.kind() });
                };
                (script.source.clone(), s.name.clone())
            }
            (None, Some(source)) => (source.clone(), "script".to_string()),
            _ => return Err(CommandError::Invalid("name a script of the project, or write one in: one or the other".into())),
        };
        let (loaded, libraries) = super::bind_loads(inputs.objects(), &[source.as_str()]).map_err(CommandError::Invalid)?;
        let (looked_up, named) = super::bind_lookups(inputs.objects(), &[source.as_str()]).map_err(CommandError::Invalid)?;
        for id in loaded.into_iter().chain(looked_up) {
            inputs.object(id)?;
        }
        let script = compile_table_script(&source, libraries).map_err(|e| CommandError::Invalid(format!("{script_name}: {e}")))?;
        let mut parameter_tables = Vec::new();
        for &id in &p.parameters {
            let t = inputs.object(id)?;
            let Body::ParameterTable(t) = &t.body else {
                return Err(CommandError::WrongKind { id, expected: ObjectKind::ParameterTable, found: t.kind() });
            };
            parameter_tables.push(t.clone());
        }
        let tables = project_tables(inputs.objects());
        let ran = script
            .run(TableInput {
                name: object.name.clone(),
                table: table.clone(),
                tables: tables.iter().map(|(n, _, t)| (n.clone(), t.clone())).collect(),
                parameters: table_parameters(&parameter_tables),
                work: Arc::new(GridWork),
                lookups: Arc::new(zd_script::Lookups { own: parameter_tables.clone(), named }),
            })
            .map_err(|problem| CommandError::Invalid(format!("{script_name} stopped: {problem}")))?;
        // The tables it read by name are its inputs too.
        for name in &ran.read {
            if let Some((_, id, _)) = tables.iter().find(|(n, _, _)| n == name) {
                inputs.object(*id)?;
            }
        }
        ran.table.check().map_err(|e| CommandError::Invalid(format!("{script_name} gave back a table that doesn't hold together: {e}")))?;
        let mut notes: Vec<Note> = ran.said.iter().map(|s| Note::info(format!("{script_name} said: {s}"))).collect();
        notes.extend(ran.notes.iter().map(|n| Note::info(n.clone())));
        let (change, message) = match p.into {
            ScriptInto::New => {
                let name = if p.name.trim().is_empty() { format!("{} {script_name}", object.name) } else { p.name.trim().to_string() };
                let message = format!("{name}: {} rows, {} columns, from {} by {script_name}", ran.table.rows(), ran.table.columns.len(), object.name);
                (Change::Create { name, layer: p.layer.clone(), attributes: Default::default(), body: Body::Table(Arc::new(ran.table)) }, message)
            }
            ScriptInto::Columns => {
                let (merged, added) = columns_onto(table, &ran.table, ColumnKind::Entered).map_err(|e| CommandError::Invalid(format!("{script_name}: {e}")))?;
                let message = format!("{}: {} column{} from {script_name}{}", object.name, added.len(), if added.len() == 1 { "" } else { "s" }, if added.is_empty() { String::new() } else { format!(", {}", added.join(", ")) });
                (Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(merged))) }, message)
            }
        };
        let mut out = Outcome::new(vec![change]).message(message);
        out.notes.extend(notes);
        Ok(out)
    }
}

/// `given`'s columns put on `base`, which it must keep the rows of: a column `base` hasn't is added as `kind`, one
/// already there of that kind replaced; one written by the run is left as it is, and changing it is refused (decision 3:
/// a number a run wrote is never written over). Gives back the table and the columns added or replaced.
pub(crate) fn columns_onto(base: &Table, given: &Table, kind: ColumnKind) -> Result<(Table, Vec<String>), String> {
    keeps_rows(base, given)?;
    let mut merged = base.clone();
    let mut added = Vec::new();
    for c in &given.columns {
        match merged.columns.iter_mut().find(|m| m.name == c.name) {
            Some(m) if m.kind == kind => {
                if changed(m, c) {
                    *m = zd_core::TableColumn { kind, ..c.clone() };
                    added.push(c.name.clone());
                }
            }
            Some(m) => {
                if changed(m, c) {
                    return Err(format!("it changed {}, which {}: a script adds columns, never writes over these", c.name, if m.kind.is_written() { "the run wrote" } else { "a person entered" }));
                }
            }
            None => {
                if merged.calculated.iter().any(|k| k.name == c.name) {
                    return Err(format!("{} is a calculated column of the table: give the script's another name", c.name));
                }
                merged.columns.push(zd_core::TableColumn { kind, ..c.clone() });
                added.push(c.name.clone());
            }
        }
    }
    Ok((merged, added))
}

/// A dataflow's table script steps, compiled in order (decision 11): each after the final writes, never with a built-in
/// step's name. Every problem is said with its step and line.
pub fn check_table_steps(dataflow: &zd_core::Dataflow, built_in: &[&str], libraries: &Libraries) -> Result<Vec<(String, Arc<TableScript>)>, String> {
    let mut out = Vec::new();
    let last_built_in = dataflow.steps.iter().rposition(|s| s.formula.is_none() && s.table.is_none());
    for (i, s) in dataflow.steps.iter().enumerate() {
        let Some(t) = &s.table else { continue };
        if built_in.contains(&s.step.as_str()) {
            return Err(format!("the table script step {} has a built-in step's name: give it its own", s.step));
        }
        if last_built_in.is_none_or(|b| dataflow.steps[b].step != "final" || b > i) {
            return Err(format!("the table script step {} runs on the case's table, so it goes after final (the final writes)", s.step));
        }
        let script = compile_table_script(&t.script, libraries.clone()).map_err(|e| format!("the table script step {}: {e}", s.step))?;
        if s.on {
            out.push((s.step.clone(), Arc::new(script)));
        }
    }
    Ok(out)
}

/// A dataflow's table script steps run on each table a reserves run made or ran again into, in order: each one's new
/// columns put on the table as the run's own (written, so the next run writes them again). A step that stops, or gives
/// back a table that doesn't keep the rows, is said and its columns left off; the table still stands.
pub(crate) fn run_table_steps(out: &mut Outcome, steps: &[(String, Arc<TableScript>)], parameters: &[Arc<ParameterTable>], project: &[(String, Arc<Table>)], lookups: &Arc<zd_script::Lookups>) {
    if steps.is_empty() {
        return;
    }
    let p = table_parameters(parameters);
    let mut notes = Vec::new();
    for change in &mut out.changes {
        let (name, body) = match change {
            Change::Create { name, body, .. } => (name.clone(), body),
            Change::Revise { body: Some(body), .. } => (String::new(), body),
            _ => continue,
        };
        let Body::Table(table) = body else { continue };
        let label = if name.is_empty() { "the table".to_string() } else { name.clone() };
        let mut current: Table = (**table).clone();
        for (step, script) in steps {
            let ran: Result<TableRan, String> = script
                .run(TableInput { name: name.clone(), table: Arc::new(current.clone()), tables: project.to_vec(), parameters: p.clone(), work: Arc::new(GridWork), lookups: lookups.clone() })
                .map_err(|problem| problem.to_string());
            match ran.and_then(|r| columns_onto(&current, &r.table, ColumnKind::Written).map(|(t, added)| (t, added, r))) {
                Ok((t, added, r)) => {
                    current = t;
                    notes.extend(r.said.iter().map(|s| Note::info(format!("{step} said: {s}"))));
                    notes.extend(r.notes.iter().map(|n| Note::info(format!("{step}: {n}"))));
                    if !added.is_empty() {
                        notes.push(Note::info(format!("{label}: {step} wrote {}", added.join(", "))));
                    }
                }
                Err(why) => notes.push(Note::warning(Code::ReservesTableScriptStopped, format!("{label}: the table script step {step} stopped, its columns left off: {why}"))),
            }
        }
        *body = Body::Table(Arc::new(current));
    }
    out.notes.extend(notes);
}

/// The project's tables as a dataflow's table script reads them by name.
pub(crate) fn tables_by_name<'a>(objects: impl Iterator<Item = &'a Arc<Object>>) -> Vec<(String, Arc<Table>)> {
    project_tables(objects).into_iter().map(|(n, _, t)| (n, t)).collect()
}

/// A script as edited, checked as a table script where it defines `run` (the app's editors): its problems, each on its
/// line; none for a script of functions only, which `check_library` checks.
pub fn check_table_script(source: &str, libraries: Libraries) -> Vec<super::FormulaProblem> {
    if !zd_script::is_table_script(source) {
        return Vec::new();
    }
    match TableScript::compile(source, libraries) {
        Ok(_) => Vec::new(),
        Err(problems) => problems.into_iter().map(|p| super::FormulaProblem { message: p.message, line: p.line }).collect(),
    }
}
