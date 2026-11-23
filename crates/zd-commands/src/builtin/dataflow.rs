//! Parameter tables and dataflows (decision 0072): saved new or as the next revision of one, so each edit is logged and
//! undone like any run. A parameter table is the assumptions any tool reads (Tim: "lots of mine planning assumptions will
//! generally be parameter table type data"); a dataflow names its fields, its steps and the tables it reads.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, Dataflow, FieldSource, ObjectId, ObjectKind, ParameterTable};

use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

/// The layers parameter tables and dataflows sit on.
pub const PARAMETERS_LAYER: &str = "parameters";
pub const DATAFLOW_LAYER: &str = "dataflows";

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// `parameters.save`: a parameter table saved, new or as the next revision of one, from its rows or from CSV.
pub struct SaveParameters;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveParametersParams {
    /// The table to revise; a new one if left out.
    #[serde(default)]
    pub table: Option<ObjectId>,
    pub name: String,
    /// The table as rows; or give `csv`.
    #[serde(default)]
    pub content: Option<ParameterTable>,
    /// The table as CSV text: `when <attribute>` columns filter, the rest are values, `(unit)` after a name.
    #[serde(default)]
    pub csv: Option<String>,
    /// A CSV file to read the table from, instead of `content` or `csv`.
    #[serde(default)]
    pub path: Option<String>,
}

impl Command for SaveParameters {
    type Params = SaveParametersParams;
    const NAME: &'static str = "parameters.save";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save a parameter table: assumptions read top down, the first row that fits and holds a value giving it",
            inputs: vec![],
            params: vec![
                Field::new("table", FieldType::Text, "The table to revise; a new one if left out"),
                Field::new("name", FieldType::Text, "Name of the table"),
                Field::new("content", FieldType::Text, "The table as rows"),
                Field::new("csv", FieldType::Text, "Or the table as CSV text"),
                Field::new("path", FieldType::Text, "Or a CSV file to read it from"),
            ],
            outputs: vec![Field::object("table", ObjectKind::ParameterTable, "The saved table")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let table = match (&p.content, &p.csv, &p.path) {
            (Some(content), None, None) => {
                content.check().map_err(CommandError::Invalid)?;
                content.clone()
            }
            (None, Some(csv), None) => ParameterTable::from_csv(csv).map_err(CommandError::Invalid)?,
            (None, None, Some(path)) => {
                let text = std::fs::read_to_string(path).map_err(|e| CommandError::Io(format!("{path}: {e}")))?;
                ParameterTable::from_csv(&text).map_err(|e| CommandError::Invalid(format!("{path}: {e}")))?
            }
            _ => return Err(CommandError::Invalid("give the table one way: its rows, CSV text or a CSV file".into())),
        };
        // A table read as a curve somewhere holds no two rows with the same filters at one x (Schedule quantities plan,
        // decision 2): said with what reads it.
        for (reader, x) in super::curve_readers(inputs.objects(), &p.name) {
            table.check_curve(&x).map_err(|why| CommandError::Invalid(format!("{reader} reads {} as a curve on {x}: {why}", p.name)))?;
        }
        // A parameter table is found by its name (Table setup plan, decision 4): one name, one table.
        if let Some(first) = inputs.objects().find(|o| o.name == p.name && matches!(o.body, Body::ParameterTable(_)) && Some(o.id) != p.table) {
            return Err(CommandError::Invalid(format!("a parameter table named {} is there already ({}): revise it, or give this one another name", p.name, first.id)));
        }
        let body = Body::ParameterTable(Arc::new(table.clone()));
        let change = match p.table {
            Some(id) => {
                let object = inputs.object(id)?;
                if !matches!(object.body, Body::ParameterTable(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::ParameterTable, found: object.kind() });
                }
                Change::Revise { id, base: object.revision, name: Some(p.name.clone()), layer: None, attributes: None, body: Some(body) }
            }
            None => Change::Create { name: p.name.clone(), layer: PARAMETERS_LAYER.into(), attributes: Attributes::new(), body },
        };
        let (rows, columns) = (table.rows.len(), table.columns.len());
        let filters = if table.filters.is_empty() { "no filters".to_string() } else { format!("filters on {}", table.filters.join(", ")) };
        Ok(Outcome::new(vec![change]).message(format!("saved {}: {rows} row{}, {columns} value{}, {filters}", p.name, plural(rows), plural(columns))))
    }
}

/// An add-on saved (Table setup plan, decision 2): its parameter tables there, its scripts' loads and lookups bound by
/// name, its allocation and table script steps compiled and its formulas parsed. A formula is checked against a main
/// dataflow's names when one that lists the add-on is saved or run, where its fields are known.
fn save_add_on(inputs: &Inputs<'_>, p: &SaveDataflowParams) -> Result<Outcome, CommandError> {
    let d = &p.content;
    for &id in &d.parameters {
        let object = inputs.object(id)?;
        if !matches!(object.body, Body::ParameterTable(_)) {
            return Err(CommandError::WrongKind { id, expected: ObjectKind::ParameterTable, found: object.kind() });
        }
    }
    let sources: Vec<&str> = d.steps.iter().filter_map(|s| s.script()).collect();
    let (scripts, libraries) = super::bind_loads(inputs.objects(), &sources).map_err(CommandError::Invalid)?;
    let (lookups, named) = super::bind_lookups(inputs.objects(), &sources).map_err(CommandError::Invalid)?;
    super::check_curves(inputs.objects(), &sources).map_err(CommandError::Invalid)?;
    let said = |step: &str, problems: Vec<zd_script::Problem>| CommandError::Invalid(format!("the step {step}: {}", problems.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("; ")));
    for s in &d.steps {
        if let Some(a) = &s.allocation {
            let declared = zd_script::Declared { libraries: libraries.clone(), lookups: Arc::new(zd_script::Lookups { own: Vec::new(), named: named.clone() }), ..Default::default() };
            zd_script::Allocation::compile(&a.script, declared).map_err(|e| said(&s.step, e))?;
        } else if let Some(t) = &s.table {
            zd_script::TableScript::compile(&t.script, libraries.clone()).map_err(|e| said(&s.step, e))?;
        } else if let Some(f) = &s.formula {
            zd_script::check_library(&s.step, &f.script, &libraries).map_err(|e| said(&s.step, e))?;
        }
    }
    let mut bound = d.clone();
    bound.scripts = scripts;
    bound.lookups = lookups;
    bound.lookup_names = named.into_iter().map(|(n, _)| n).collect();
    let body = Body::Dataflow(Arc::new(bound));
    let change = match p.dataflow {
        Some(id) => {
            let object = inputs.object(id)?;
            if !matches!(object.body, Body::Dataflow(_)) {
                return Err(CommandError::WrongKind { id, expected: ObjectKind::Dataflow, found: object.kind() });
            }
            Change::Revise { id, base: object.revision, name: Some(p.name.clone()), layer: None, attributes: None, body: Some(body) }
        }
        None => Change::Create { name: p.name.clone(), layer: DATAFLOW_LAYER.into(), attributes: Attributes::new(), body },
    };
    Ok(Outcome::new(vec![change]).message(format!(
        "saved {}, an add-on: {} field{}, {} word field{}, {} step{}, {} parameter table{}",
        p.name,
        d.fields.len(),
        plural(d.fields.len()),
        d.words.len(),
        plural(d.words.len()),
        d.steps.len(),
        plural(d.steps.len()),
        d.parameters.len(),
        plural(d.parameters.len())
    )))
}

/// `parameters.export_csv`: a parameter table written as CSV, to edit and save back.
pub struct ExportParameters;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportParametersParams {
    pub table: ObjectId,
    pub path: String,
}

impl Command for ExportParameters {
    type Params = ExportParametersParams;
    const NAME: &'static str = "parameters.export_csv";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Write a parameter table as CSV",
            inputs: vec![Field::object("table", ObjectKind::ParameterTable, "The table")],
            params: vec![Field::new("path", FieldType::Text, "The CSV file to write")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.table)?;
        let Body::ParameterTable(table) = &object.body else {
            return Err(CommandError::WrongKind { id: p.table, expected: ObjectKind::ParameterTable, found: object.kind() });
        };
        std::fs::write(&p.path, table.to_csv()).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))?;
        Ok(Outcome::new(vec![]).message(format!("wrote {} ({} rows) to {}", object.name, table.rows.len(), p.path)))
    }
}

/// `dataflow.save`: a dataflow saved, new or as the next revision of one.
pub struct SaveDataflow;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveDataflowParams {
    /// The dataflow to revise; a new one if left out.
    #[serde(default)]
    pub dataflow: Option<ObjectId>,
    pub name: String,
    pub content: Dataflow,
}

impl Command for SaveDataflow {
    type Params = SaveDataflowParams;
    const NAME: &'static str = "dataflow.save";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save a reserves dataflow: its fields, its steps and the parameter tables it reads",
            inputs: vec![],
            params: vec![
                Field::new("dataflow", FieldType::Text, "The dataflow to revise; a new one if left out"),
                Field::new("name", FieldType::Text, "Name of the dataflow"),
                Field::new("content", FieldType::Text, "Its fields, steps and parameter tables"),
            ],
            outputs: vec![Field::object("dataflow", ObjectKind::Dataflow, "The saved dataflow")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let d = &p.content;
        d.check().map_err(CommandError::Invalid)?;
        if d.add_on {
            return save_add_on(inputs, p);
        }
        // A main dataflow's add-ons (Table setup plan, decision 2): each an add-on, and the checks below run on the
        // dataflow with them put in at their stage, as a run puts them.
        let mut with_add_ons: Option<zd_core::Dataflow> = None;
        if !d.add_ons.is_empty() {
            let mut add_ons: Vec<(String, Arc<zd_core::Dataflow>)> = Vec::new();
            for &id in &d.add_ons {
                let object = inputs.object(id)?;
                let Body::Dataflow(a) = &object.body else {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::Dataflow, found: object.kind() });
                };
                if Some(id) == p.dataflow {
                    return Err(CommandError::Invalid("a dataflow can't run itself as an add-on".into()));
                }
                add_ons.push((object.name.clone(), a.clone()));
            }
            let pairs: Vec<(&str, &zd_core::Dataflow)> = add_ons.iter().map(|(n, a)| (n.as_str(), a.as_ref())).collect();
            with_add_ons = Some(d.with_add_ons(&pairs).map_err(CommandError::Invalid)?);
        }
        let own = d;
        let d = with_add_ons.as_ref().unwrap_or(own);
        let mut tables = Vec::new();
        for &id in &d.parameters {
            let object = inputs.object(id)?;
            let Body::ParameterTable(table) = &object.body else {
                return Err(CommandError::WrongKind { id, expected: ObjectKind::ParameterTable, found: object.kind() });
            };
            tables.push(table.clone());
        }
        // Formula steps compile against the dataflow's names and give their known answers, or it isn't saved (0077).
        let engine_fields = d.engine_fields().map_err(CommandError::Invalid)?;
        let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
        let built_in: Vec<&str> = super::STEPS.iter().map(|(n, _)| *n).collect();
        // The scripts its formulas load, bound by name to the project's (decision 0099): kept by id, read by the run. Its
        // own are kept; an add-on keeps its own, and they're bound here only to check the dataflow with them.
        let sources: Vec<&str> = d.steps.iter().filter_map(|s| s.script()).collect();
        let own_sources: Vec<&str> = own.steps.iter().filter_map(|s| s.script()).collect();
        let (_, libraries) = super::bind_loads(inputs.objects(), &sources).map_err(CommandError::Invalid)?;
        let (scripts, _) = super::bind_loads(inputs.objects(), &own_sources).map_err(CommandError::Invalid)?;
        // The tables its scripts look up by name (Schedule quantities plan, decision 1), kept by id like its scripts.
        let (_, named) = super::bind_lookups(inputs.objects(), &sources).map_err(CommandError::Invalid)?;
        let (lookups, own_named) = super::bind_lookups(inputs.objects(), &own_sources).map_err(CommandError::Invalid)?;
        super::check_curves(inputs.objects(), &sources).map_err(CommandError::Invalid)?;
        for &id in scripts.iter().chain(&lookups) {
            inputs.object(id)?;
        }
        let mut declared = super::formula_declared(&engine_fields, &refs);
        super::declare_words(&mut declared, &d.words);
        declared.libraries = libraries;
        declared.lookups = std::sync::Arc::new(zd_script::Lookups { own: tables.clone(), named });
        let formulas = super::check_formula_steps(d, &built_in, &declared).map_err(CommandError::Invalid)?.len();
        // Allocation steps compile too, and sit between rom and final (Waste allocation plan, decision 1).
        super::check_allocation_steps(d, &built_in, &declared).map_err(CommandError::Invalid)?;
        // Table script steps (decision 11 of 0099) compile too, and sit after the final writes.
        super::check_table_steps(d, &built_in, &declared.libraries).map_err(CommandError::Invalid)?;
        let mut bound = own.clone();
        bound.scripts = scripts;
        bound.lookups = lookups;
        bound.lookup_names = own_named.into_iter().map(|(n, _)| n).collect();
        let d = &bound;
        // A setting no table has a column for can never be given: said now, not block by block at the run.
        let mut notes = Vec::new();
        for f in &d.fields {
            let column = match &f.source {
                FieldSource::Parameter { column } => column.as_str(),
                _ => continue,
            };
            if !tables.iter().any(|t| t.column(column).is_some()) {
                notes.push(Note::warning(Code::DataflowNoColumn, format!("{} comes from the parameter column {column}, which none of its {} table{} has", f.field.name, tables.len(), plural(tables.len()))));
            }
        }
        let body = Body::Dataflow(Arc::new(d.clone()));
        let change = match p.dataflow {
            Some(id) => {
                let object = inputs.object(id)?;
                if !matches!(object.body, Body::Dataflow(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::Dataflow, found: object.kind() });
                }
                Change::Revise { id, base: object.revision, name: Some(p.name.clone()), layer: None, attributes: None, body: Some(body) }
            }
            None => Change::Create { name: p.name.clone(), layer: DATAFLOW_LAYER.into(), attributes: Attributes::new(), body },
        };
        let on = d.steps.iter().filter(|s| s.on).count();
        let mut outcome = Outcome::new(vec![change]).message(format!(
            "saved {}: {} field{}, {on} of {} step{} on{}, {} parameter table{}",
            p.name,
            d.fields.len(),
            plural(d.fields.len()),
            d.steps.len(),
            plural(d.steps.len()),
            if formulas == 0 { String::new() } else { format!(" ({formulas} formula{} checked)", plural(formulas)) },
            tables.len(),
            plural(tables.len())
        ));
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}
