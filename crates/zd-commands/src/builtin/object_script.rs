//! `object.script`: an object script (the Build Plan's "Table setup plan", decision 10; 0099 decision 8). Written as a
//! calculated column is, `def formula(row, p):`, the row a block's row of the Objects table: what it sets becomes the
//! block's attributes (`row.case = "50m"`, material from the horizon, a discard mark from a rule). It sets attributes only:
//! never a block's name, layer, kind, volume or tonnes, and never its geometry. Each block it changes is revised once,
//! all in one undo; a reserves table made before then says its blocks have changed since.
//!
//! A new attribute is words where the line that sets it gives words (a quoted value, `str(…)`, or another word column),
//! else a number. A block the script stops on stops the run, said with its line: nothing is written.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, Change, ColumnValues, ObjectId, ParameterTable, Table};
use zd_script::{ColumnInput, Declared, FieldDecl, Formula, ScriptValue, UnitInput};

use super::attributes::NOT_TYPED;
use super::formulas::param_value;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

pub struct ObjectScript;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectScriptParams {
    /// The blocks it runs on, exactly these; or every solid on `layers` that fits `filter`.
    #[serde(default)]
    pub objects: Vec<ObjectId>,
    #[serde(default)]
    pub layers: Vec<String>,
    /// In the bench rules' words, by attribute (`seam`: `U, M`).
    #[serde(default)]
    pub filter: BTreeMap<String, String>,
    /// A script of the project, by id; or `source`, the script written in.
    #[serde(default)]
    pub script: Option<ObjectId>,
    #[serde(default)]
    pub source: Option<String>,
    /// The parameter tables `p` reads, first first, each row matched on the block's attributes.
    #[serde(default)]
    pub parameters: Vec<ObjectId>,
}

/// The names a script sets on its row, `row.name = …` or `row["name"] = …`, each with whether the line gives words.
fn names_set(source: &str, words: &[String]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some(at) = line.find("row") else { continue };
        let rest = &line[at + 3..];
        let (name, after) = if let Some(r) = rest.strip_prefix('.') {
            let end = r.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(r.len());
            (r[..end].to_string(), &r[end..])
        } else if let Some(r) = rest.strip_prefix("[\"").or_else(|| rest.strip_prefix("['")) {
            let Some(end) = r.find(['"', '\'']) else { continue };
            (r[..end].to_string(), r[end + 1..].trim_start().strip_prefix(']').unwrap_or(""))
        } else {
            continue;
        };
        let after = after.trim_start();
        let Some(value) = after.strip_prefix('=').filter(|v| !v.starts_with('=')) else { continue };
        if name.is_empty() {
            continue;
        }
        let value = value.trim_start();
        let gives_words = value.starts_with(['"', '\'']) || value.starts_with("str(") || words.iter().any(|w| value == format!("row.{w}") || value == format!("row[\"{w}\"]"));
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some((_, w)) => *w |= gives_words,
            None => out.push((name, gives_words)),
        }
    }
    out
}

impl Command for ObjectScript {
    type Params = ObjectScriptParams;
    const NAME: &'static str = "object.script";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run an object script on blocks: `def formula(row, p):` on each block's row of the Objects table, what it sets becoming the block's attributes",
            inputs: vec![Field::new("objects", FieldType::Texts, "The blocks, by id; or every solid on layers that fits a filter")],
            params: vec![
                Field::new("layers", FieldType::Texts, "Layers whose solids it runs on, by name"),
                Field::new("filter", FieldType::Text, "In the bench rules' words, by attribute"),
                Field::new("script", FieldType::Text, "A script of the project, by id; or source, written in"),
                Field::new("parameters", FieldType::Texts, "The parameter tables p reads"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let project = inputs.project();
        // The blocks: exactly those named, or every solid on the layers that fits the filter.
        let ids: Vec<ObjectId> = if !p.objects.is_empty() {
            p.objects.clone()
        } else if !p.layers.is_empty() {
            let layers = p.layers.iter().map(|n| inputs.layer(n).map(|l| l.id)).collect::<Result<Vec<_>, _>>()?;
            zd_core::BlockPick { layers, filter: p.filter.clone(), ids: Vec::new() }.resolve(project.objects())
        } else {
            return Err(CommandError::Invalid("name the blocks, or the layers (and a filter) whose solids it runs on".into()));
        };
        if ids.is_empty() {
            return Err(CommandError::Invalid("no blocks to run on: none on those layers fit the filter".into()));
        }
        for &id in &ids {
            inputs.object(id)?;
        }
        let (source, script_name) = match (&p.script, &p.source) {
            (Some(id), None) => {
                let s = inputs.object(*id)?;
                let Body::Script(script) = &s.body else {
                    return Err(CommandError::WrongKind { id: *id, expected: zd_core::ObjectKind::Script, found: s.kind() });
                };
                (script.source.clone(), s.name.clone())
            }
            (None, Some(source)) => (source.clone(), "the object script".to_string()),
            _ => return Err(CommandError::Invalid("name a script of the project, or write one in: one or the other".into())),
        };
        let mut tables: Vec<Arc<ParameterTable>> = Vec::new();
        for &id in &p.parameters {
            let t = inputs.object(id)?;
            let Body::ParameterTable(t) = &t.body else {
                return Err(CommandError::WrongKind { id, expected: zd_core::ObjectKind::ParameterTable, found: t.kind() });
            };
            tables.push(t.clone());
        }

        // The blocks' rows of the Objects table, and the names the script may read and set.
        let table: Table = crate::table_grid::objects_table(project, &ids, Some(crate::objects_own::OBJECTS_DENSITY));
        let words: Vec<String> = table.columns.iter().filter(|c| matches!(c.values, ColumnValues::Text(_))).map(|c| c.name.clone()).collect();
        let set = names_set(&source, &words);
        if let Some((name, _)) = set.iter().find(|(n, _)| NOT_TYPED.iter().any(|t| t.eq_ignore_ascii_case(n))) {
            return Err(CommandError::Invalid(format!("{script_name} sets {name}, which is the block's own: an object script sets attributes only")));
        }
        let mut fields: Vec<FieldDecl> = Vec::new();
        for c in &table.columns {
            fields.push(match c.values {
                ColumnValues::Number(_) => FieldDecl::number(&c.name, &c.unit),
                ColumnValues::Text(_) => FieldDecl::text(&c.name),
            });
        }
        for (name, gives_words) in &set {
            if !fields.iter().any(|f| f.name == *name) {
                fields.push(if *gives_words { FieldDecl::text(name) } else { FieldDecl::number(name, "") });
            }
        }
        let mut params: Vec<FieldDecl> = Vec::new();
        for t in &tables {
            for c in &t.columns {
                if !params.iter().any(|q| q.name == c.name) {
                    params.push(if c.text { FieldDecl::text(&c.name) } else { FieldDecl::number(&c.name, &c.unit) });
                }
            }
        }
        let (loaded, libraries) = super::bind_loads(inputs.objects(), &[source.as_str()]).map_err(CommandError::Invalid)?;
        let (looked_up, named) = super::bind_lookups(inputs.objects(), &[source.as_str()]).map_err(CommandError::Invalid)?;
        for id in loaded.into_iter().chain(looked_up) {
            inputs.object(id)?;
        }
        let declared = Declared { fields, labels: Vec::new(), parameters: params, libraries, lookups: Arc::new(zd_script::Lookups { own: tables.clone(), named }) };
        let formula = Formula::compile(&source, declared).map_err(|problems| {
            let said: Vec<String> = problems.iter().map(|q| match q.line {
                Some(l) => format!("line {l}: {}", q.message),
                None => q.message.clone(),
            }).collect();
            CommandError::Invalid(format!("{script_name}: {}", said.join("; ")))
        })?;

        // Each block as a unit: its row's values, and its parameters by its attributes.
        let refs: Vec<&ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
        let objects: Vec<&Arc<zd_core::Object>> = ids.iter().map(|&id| project.object(id).expect("read above")).collect();
        let units: Vec<UnitInput> = (0..table.rows())
            .map(|r| {
                let mut values: Vec<(&str, ScriptValue)> = Vec::new();
                for c in &table.columns {
                    values.push((
                        c.name.as_str(),
                        match &c.values {
                            ColumnValues::Number(v) => v[r].map_or(ScriptValue::None, ScriptValue::Number),
                            ColumnValues::Text(v) => v[r].clone().filter(|w| !w.is_empty()).map_or(ScriptValue::None, ScriptValue::Text),
                        },
                    ));
                }
                let mut parameters: Vec<(&str, ScriptValue)> = Vec::new();
                for t in &tables {
                    for c in &t.columns {
                        if parameters.iter().any(|(n, _)| *n == c.name) {
                            continue;
                        }
                        let v = ParameterTable::lookup_in(&refs, &objects[r].attributes, &c.name).map_or(ScriptValue::None, |f| param_value(f.value));
                        parameters.push((c.name.as_str(), v));
                    }
                }
                UnitInput { fields: zd_script::values(values), block: zd_script::values([]), parameters: zd_script::values(parameters), section: None, rom_coal: None }
            })
            .collect();
        let input = ColumnInput { units, ..Default::default() };
        let all: Vec<usize> = (0..objects.len()).collect();
        let mut changes = Vec::new();
        let mut set_count: BTreeMap<String, usize> = BTreeMap::new();
        let mut said = Vec::new();
        for batch in all.chunks(256) {
            let ran = formula.run(&input, batch).or_else(|_| {
                // Run again a block at a time, to say which it stopped on.
                let mut each = Vec::new();
                for &u in batch {
                    match formula.run(&input, &[u]) {
                        Ok(s) => each.extend(s),
                        Err(q) => {
                            let at = q.line.map_or(String::new(), |l| format!(" at line {l}"));
                            return Err(CommandError::Invalid(format!("{script_name} stopped on {}{at}: {}; nothing was written", objects[u].name, q.message)));
                        }
                    }
                }
                Ok(each)
            })?;
            for sets in ran {
                let object = objects[sets.unit];
                said.extend(sets.said.iter().map(|s| format!("{}: {s}", object.name)));
                let mut attributes = object.attributes.clone();
                for (name, _) in &set {
                    let Some(value) = sets.get(name) else { continue };
                    let key = attributes.keys().find(|k| k.eq_ignore_ascii_case(name)).map(str::to_string).unwrap_or_else(|| name.clone());
                    let next = match value {
                        ScriptValue::Number(x) if x.is_finite() => Some(AttrValue::Number(*x)),
                        ScriptValue::Text(w) if !w.trim().is_empty() => Some(AttrValue::Text(w.trim().to_string())),
                        _ => None,
                    };
                    if attributes.get(&key) == next.as_ref() {
                        continue;
                    }
                    match next {
                        Some(v) => attributes.insert(key, v),
                        None => attributes.remove(&key),
                    };
                    *set_count.entry(name.clone()).or_default() += 1;
                }
                if attributes != object.attributes {
                    changes.push(Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: Some(attributes), body: None });
                }
            }
        }
        let changed = changes.len();
        let what = if set_count.is_empty() { "nothing set".to_string() } else { set_count.iter().map(|(n, k)| format!("{n} on {k}")).collect::<Vec<_>>().join(", ") };
        let mut out = Outcome::new(changes).message(format!("{script_name} on {} block{}: {changed} changed ({what})", objects.len(), if objects.len() == 1 { "" } else { "s" }));
        out.notes.extend(said.into_iter().take(20).map(Note::info));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::names_set;

    #[test]
    fn the_names_a_script_sets_are_read_off_it_words_where_its_line_gives_words() {
        let source = "def formula(row, p):\n    row.case = \"50m\"\n    row[\"discard\"] = 1 if row.ash > 40 else 0\n    if row.horizon == \"OB\":  # not a set\n        row.material = row.horizon\n    row.tag = str(row.strip)\n";
        let words = vec!["horizon".to_string(), "strip".to_string()];
        assert_eq!(names_set(source, &words), [("case".to_string(), true), ("discard".to_string(), false), ("material".to_string(), true), ("tag".to_string(), true)]);
    }
}
