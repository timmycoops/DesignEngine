//! Scripts of the project (the Build Plan's "Tables and scripts plan", decision 9, `docs/decisions/0099-tables-and-scripts.md`):
//! Starlark a dataflow's formula, a calculated column or another script loads by its name. What loads one keeps its id
//! (a dataflow's `scripts`, a calculated column's), bound when it's saved from the names its `load` lines ask for, so a
//! run reads the script through its id and names the revision it read.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Body, Change, Object, ObjectId, ObjectKind, Script, loads_in};
use zd_script::Libraries;

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

/// The layer new scripts go on.
pub const SCRIPTS_LAYER: &str = "scripts";

/// The project's scripts that `sources` load by name, and those load in turn: their ids, in the order first asked for,
/// and their names and Starlark for the loader. A name no script of the project has is refused.
pub fn bind_loads<'a>(objects: impl IntoIterator<Item = &'a Arc<Object>>, sources: &[&str]) -> Result<(Vec<ObjectId>, Libraries), String> {
    let scripts: Vec<(&str, ObjectId, &str)> = objects
        .into_iter()
        .filter_map(|o| match &o.body {
            Body::Script(s) => Some((o.name.as_str(), o.id, s.source.as_str())),
            _ => None,
        })
        .collect();
    let mut wanted: Vec<String> = sources.iter().flat_map(|s| loads_in(s)).collect();
    let mut ids: Vec<ObjectId> = Vec::new();
    let mut libraries: Vec<(String, String)> = Vec::new();
    let mut at = 0;
    while at < wanted.len() {
        let name = wanted[at].clone();
        at += 1;
        if libraries.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let Some(&(_, id, source)) = scripts.iter().find(|(n, _, _)| *n == name) else {
            return Err(format!("can't load {name}: the project has no script of that name"));
        };
        ids.push(id);
        libraries.push((name, source.to_string()));
        wanted.extend(loads_in(source));
    }
    Ok((ids, Arc::new(libraries)))
}

/// The scripts a dataflow or a calculated column keeps, read by id: their names and Starlark for the loader. One since
/// deleted is said.
pub fn libraries_of(get: &dyn Fn(ObjectId) -> Option<Arc<Object>>, ids: &[ObjectId]) -> Result<Libraries, String> {
    let mut out = Vec::with_capacity(ids.len());
    for &id in ids {
        let object = get(id).ok_or("it loads a script since deleted")?;
        let Body::Script(s) = &object.body else { return Err(format!("{} isn't a script", object.name)) };
        out.push((object.name.clone(), s.source.clone()));
    }
    Ok(Arc::new(out))
}

/// `script.save`: a script of the project made, or its next revision (decision 9). Checked first as a library: it
/// parses, and what it loads is in the project, loads cleanly and doesn't load it back; every problem said with its
/// line. Two scripts of one name are refused, as a `load` couldn't tell them apart.
pub struct SaveScript;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveScriptParams {
    /// The script revised; none makes one.
    #[serde(default)]
    pub script: Option<ObjectId>,
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "scripts_layer")]
    pub layer: String,
}

fn scripts_layer() -> String {
    SCRIPTS_LAYER.into()
}

impl Command for SaveScript {
    type Params = SaveScriptParams;
    const NAME: &'static str = "script.save";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save a script of the project: Starlark that formulas and calculated columns load by its name",
            inputs: vec![Field::new("script", FieldType::Text, "The script revised; none makes one")],
            params: vec![
                Field::new("name", FieldType::Text, "Its name, what a load asks for"),
                Field::new("source", FieldType::Text, "Its Starlark"),
                Field::new("note", FieldType::Text, "What it's for"),
                Field::new("layer", FieldType::Text, "Layer for a new script"),
            ],
            outputs: vec![Field::object("script", ObjectKind::Script, "The saved script")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let name = p.name.trim();
        if name.is_empty() || name.contains(['"', '\'', '\n']) {
            return Err(CommandError::Invalid("a script needs a name a load can ask for: no quotes".into()));
        }
        if let Some(other) = inputs.objects().find(|o| matches!(o.body, Body::Script(_)) && o.name == name && Some(o.id) != p.script) {
            return Err(CommandError::Invalid(format!("the project has a script named {name} already: a load couldn't tell them apart", name = other.name)));
        }
        // The other scripts, by name, as the loader finds them.
        let others: Vec<(String, String)> = inputs
            .objects()
            .filter(|o| Some(o.id) != p.script)
            .filter_map(|o| match &o.body {
                Body::Script(s) => Some((o.name.clone(), s.source.clone())),
                _ => None,
            })
            .collect();
        zd_script::check_library(name, &p.source, &Arc::new(others)).map_err(|problems| CommandError::Invalid(problems.iter().map(|p| p.message.clone()).collect::<Vec<_>>().join("; ")))?;
        let body = Body::Script(Arc::new(Script { source: p.source.replace("\r\n", "\n"), note: p.note.trim().to_string() }));
        let change = match p.script {
            Some(id) => {
                let object = inputs.object(id)?;
                if !matches!(object.body, Body::Script(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::Script, found: object.kind() });
                }
                Change::Revise { id, base: object.revision, name: (object.name != name).then(|| name.to_string()), layer: None, attributes: None, body: Some(body) }
            }
            None => Change::Create { name: name.to_string(), layer: p.layer.clone(), attributes: Default::default(), body },
        };
        let mut out = Outcome::new(vec![change]);
        out.notes.push(Note::info(format!("the script {name} saved")));
        Ok(out)
    }
}

/// Parameter tables by the names scripts look them up by.
pub type NamedTables = Vec<(String, Arc<zd_core::ParameterTable>)>;

/// The parameter tables `sources` look up by name (the Build Plan's "Schedule quantities plan", decision 1): their ids,
/// in the order first named, and the tables by name for the script. A name no parameter table of the project has is
/// refused.
pub fn bind_lookups<'a>(objects: impl IntoIterator<Item = &'a Arc<Object>>, sources: &[&str]) -> Result<(Vec<ObjectId>, NamedTables), String> {
    let tables: Vec<(&str, ObjectId, &Arc<zd_core::ParameterTable>)> = objects
        .into_iter()
        .filter_map(|o| match &o.body {
            Body::ParameterTable(t) => Some((o.name.as_str(), o.id, t)),
            _ => None,
        })
        .collect();
    let mut ids = Vec::new();
    let mut named = Vec::new();
    for name in sources.iter().flat_map(|s| zd_core::lookups_in(s)) {
        if named.iter().any(|(n, _): &(String, _)| *n == name) {
            continue;
        }
        let Some(&(_, id, t)) = tables.iter().find(|(n, _, _)| *n == name) else {
            return Err(format!("can't look up {name}: the project has no parameter table of that name"));
        };
        ids.push(id);
        named.push((name, t.clone()));
    }
    Ok((ids, named))
}

/// The parameter tables a dataflow or calculated column looks up, read by id: each by its name, for the script. One since
/// deleted is said.
pub fn lookups_by_id(get: &dyn Fn(ObjectId) -> Option<Arc<Object>>, ids: &[ObjectId]) -> Result<NamedTables, String> {
    let mut out = Vec::with_capacity(ids.len());
    for &id in ids {
        let object = get(id).ok_or("it looks up a parameter table since deleted")?;
        let Body::ParameterTable(t) = &object.body else { return Err(format!("{} isn't a parameter table", object.name)) };
        out.push((object.name.clone(), t.clone()));
    }
    Ok(out)
}

/// The curves `sources` read checked against the project's parameter tables (Schedule quantities plan, decision 2): each
/// named table holds no two rows with the same filters at one `x`. Said with the table and its rows.
pub fn check_curves<'a>(objects: impl IntoIterator<Item = &'a Arc<Object>>, sources: &[&str]) -> Result<(), String> {
    let tables: Vec<(&str, &Arc<zd_core::ParameterTable>)> = objects
        .into_iter()
        .filter_map(|o| match &o.body {
            Body::ParameterTable(t) => Some((o.name.as_str(), t)),
            _ => None,
        })
        .collect();
    for (name, x) in sources.iter().flat_map(|s| zd_core::curves_in(s)) {
        if let Some((_, t)) = tables.iter().find(|(n, _)| *n == name) {
            t.check_curve(&x).map_err(|why| format!("{name} as a curve on {x}: {why}"))?;
        }
    }
    Ok(())
}

/// Every script of the project that reads `table` as a curve, with the `x` it reads it on and what holds it: a
/// dataflow's steps, a table's calculated columns, a script of the project. A parameter table being saved is checked
/// against each (decision 2 of the Schedule quantities plan).
pub fn curve_readers<'a>(objects: impl IntoIterator<Item = &'a Arc<Object>>, table: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for o in objects {
        let sources: Vec<&str> = match &o.body {
            Body::Dataflow(d) => d.steps.iter().filter_map(|s| s.script()).collect(),
            Body::Table(t) => t.calculated.iter().map(|c| c.script.as_str()).collect(),
            Body::Script(s) => vec![s.source.as_str()],
            _ => continue,
        };
        for (name, x) in sources.iter().flat_map(|s| zd_core::curves_in(s)) {
            if name == table && !out.iter().any(|(_, seen): &(String, String)| *seen == x) {
                out.push((o.name.clone(), x));
            }
        }
    }
    out
}
