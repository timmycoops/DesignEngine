//! The brief an AI is handed to write a script right (the AI script brief, decision 0104): one shape for every kind of
//! script, built from the project. How scripts are written (the reference), what this kind is and may set, a dictionary
//! of every name it can read (words or a number, its unit, what it means where the engine knows, how many rows have it),
//! the parameter tables and the project's scripts it can load, the script as it stands, and the request. Names only:
//! the words a column holds (seam and horizon names) go in only when asked, as on a client's project they're the
//! client's; a number never does.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Body, ColumnValues, Dataflow, FieldSource, ObjectId, Project, Table};

use crate::CommandError;

/// The kinds of script a brief is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptKind {
    /// A dataflow's formula step, `def formula(unit, p):` on each unit.
    Formula,
    /// A dataflow's allocation step, `def allocate(column, p):` on each block column.
    Allocation,
    /// A calculated column, `def formula(row, p):` on each row of a table or of the Objects table.
    Calculated,
    /// An object script, `def formula(row, p):` setting each block's attributes.
    Object,
    /// A table script, `def run(t, p):` on a whole table.
    Table,
    /// A script of functions others load.
    Library,
}

/// What it's asked to do with the script.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BriefAsk {
    #[default]
    Write,
    Explain,
    /// Fix the problems the check found, each with its line.
    Fix,
}

/// What a brief is for: the kind, what it runs on (a dataflow, a table, or the Objects table), the script as it stands
/// and its name, whether the words columns hold go in, and the problems to fix.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefFor {
    pub kind: Option<ScriptKind>,
    #[serde(default)]
    pub dataflow: Option<ObjectId>,
    /// The dataflow as edited, not yet saved: its names are read from it in place of the saved one's.
    #[serde(default)]
    pub content: Option<Dataflow>,
    #[serde(default)]
    pub table: Option<ObjectId>,
    /// On the Objects table: a calculated column or table script on it; an object script always is.
    #[serde(default)]
    pub objects: bool,
    /// The layers whose solids an object script runs on, where it's checked and tried; none, every solid.
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub name: String,
    /// Put in the words each word column holds (seam, horizon, material names): off unless asked.
    #[serde(default)]
    pub words: bool,
    #[serde(default)]
    pub ask: BriefAsk,
    /// The problems to fix, each its line (from 1) where it has one.
    #[serde(default)]
    pub problems: Vec<(Option<usize>, String)>,
}

/// How many of a column's words the brief lists, when words are asked for.
const WORDS_SHOWN: usize = 40;

/// What a name the engine writes means, where it knows: the measures, the cuts' attributes, the run's own fields.
fn meaning(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "name" => "the object's name",
        "layer" => "the layer it's on",
        "kind" => "solid, surface, string or grid",
        "volume" => "the solid's volume (m³), the volume of record",
        "tonnes" => "volume × the density column, rd",
        "plan_area" => "its footprint in plan (m²)",
        "thickness" => "vertical thickness, volume ÷ footprint (m)",
        "true_thickness" => "true thickness, vertical × cos dip (m): what seam rules and losses use",
        "length" => "length along its strip, the long side of its footprint's rectangle (m)",
        "width" => "width across its strip (m)",
        "dip" => "its seam's dip (degrees), measured from the floor and roof",
        "centroid_x" => "easting of its centroid (m)",
        "centroid_y" => "northing of its centroid (m)",
        "centroid_z" => "RL of its centroid (m)",
        "strip" => "its strip, from the strip cut",
        "block" => "its block, from the block polygons",
        "pit" => "its pit",
        "horizon" => "the seam-model horizon it was cut from (a seam, or the waste above it)",
        "seam" => "the seam it belongs to: waste takes the seam below it",
        "material" => "coal or waste",
        "bench" => "its bench, by the bench rules",
        "bench_floor" => "its bench's floor RL (m)",
        "bench_top" => "its bench's top RL (m)",
        "benched_from" => "the block it was benched from",
        "case" => "the workflow case that made it",
        "rd" => "relative density, air dried (t/m³)",
        "rd_is" => "relative density in situ (t/m³), Preston-Sanders from rd",
        "rd_rom" => "relative density at ROM moisture (t/m³)",
        "mad" => "moisture, air dried (%)",
        "mis" => "moisture in situ (%)",
        "mrom" => "moisture at ROM (%)",
        "ash" => "ash, air dried (%)",
        "interrogated" => "the seam model its grids were read from",
        "column_volume" => "the 2.5D column check's volume (m³)",
        "column_delta" => "solid volume less the column check's (m³)",
        "process" | "method" | "zone" => "a word the waste allocation sets",
        _ => "",
    }
}

fn kind_words(kind: ScriptKind) -> (&'static str, &'static str) {
    match kind {
        ScriptKind::Formula => (
            "A formula step of a reserves dataflow: `def formula(unit, p):`, run on each unit its filter fits. It reads the unit's fields and labels and `p`, the parameters for the unit's words, and sets the unit's fields.",
            "It sets only the dataflow's fields; never a unit's volume when it's the volume of record, and never a label. It never moves material between units.",
        ),
        ScriptKind::Allocation => (
            "An allocation step of a reserves dataflow: `def allocate(column, p):`, run once on each block column after `rom`. It works on the column's waste interval with `column.intervals()`, `column.take_from_top/bottom`, `u.split_thickness`, `u.split_shares` and `u.place_onto`, and fills each process's schedule fields.",
            "Every number comes from a parameter row through `p` or `lookup`, never written in (only 0 and 1). Splits are the engine's, so pieces always add back.",
        ),
        ScriptKind::Calculated => (
            "A calculated column: `def formula(row, p):`, setting the column on each row, worked out when the table is shown and never stored.",
            "It sets its own column only, a number. A row it stops on (a division by an empty value) has no value; the rest go on.",
        ),
        ScriptKind::Object => (
            "An object script: `def formula(row, p):` on each block's row of the Objects table. What it sets becomes the block's attribute: words where the line gives words (a quoted value, `str(...)`, another word column), else a number.",
            "It never sets name, layer, kind, volume or tonnes, and never touches geometry. A block it stops on stops the run, and nothing is written.",
        ),
        ScriptKind::Table => (
            "A table script: `def run(t, p):` on a whole table, giving back a table. Filter, group, join and add up with the table's calls (`t.where`, `t.group`, `t.join`, `t.total`, `t.with_column`); `table(\"name\")` reads another table, `table(\"Objects\")` the Objects table.",
            "It never writes over a column a run wrote or a person entered. Grouping adds each number up by its own rule; a number with no rule is empty in a group.",
        ),
        ScriptKind::Library => (
            "A script of functions, loaded by others with `load(\"name\", \"function\")`. It has no unit, row or table of its own.",
            "Only functions: no files, network, clock or imports.",
        ),
    }
}

/// A column's line in the dictionary: name, words or a number, unit, meaning, how many rows have it, and its words.
fn column_lines(table: &Table, words: bool, out: &mut Vec<String>) {
    let rows = table.rows();
    out.push("| Name | Holds | Unit | Means | Rows with one |".into());
    out.push("|---|---|---|---|---|".into());
    for c in &table.columns {
        let (holds, filled) = match &c.values {
            ColumnValues::Number(v) => ("number", v.iter().filter(|x| x.is_some()).count()),
            ColumnValues::Text(v) => ("words", v.iter().filter(|x| x.as_deref().is_some_and(|w| !w.is_empty())).count()),
        };
        out.push(format!("| `{}` | {holds} | {} | {} | {filled} of {rows} |", c.name, c.unit, meaning(c.name.rsplit('\\').next().unwrap_or(&c.name))));
    }
    if words {
        let lists: Vec<String> = table
            .columns
            .iter()
            .filter_map(|c| match &c.values {
                ColumnValues::Text(v) => {
                    let set: BTreeSet<&str> = v.iter().flatten().map(|w| w.as_str()).filter(|w| !w.is_empty()).collect();
                    if set.is_empty() || c.name == "name" {
                        return None;
                    }
                    let shown: Vec<&str> = set.iter().copied().take(WORDS_SHOWN).collect();
                    let more = if set.len() > WORDS_SHOWN { format!(" and {} more", set.len() - WORDS_SHOWN) } else { String::new() };
                    Some(format!("- `{}`: {}{more}", c.name, shown.join(", ")))
                }
                _ => None,
            })
            .collect();
        if !lists.is_empty() {
            out.push(String::new());
            out.push("The words each word column holds:".into());
            out.extend(lists);
        }
    }
}

fn dataflow_lines(project: &Project, d: &Dataflow, out: &mut Vec<String>) {
    let add_ons: Vec<(String, Arc<Dataflow>)> = d
        .add_ons
        .iter()
        .filter_map(|&id| {
            let o = project.object(id)?;
            match &o.body {
                Body::Dataflow(a) => Some((o.name.clone(), a.clone())),
                _ => None,
            }
        })
        .collect();
    let pairs: Vec<(&str, &Dataflow)> = add_ons.iter().map(|(n, a)| (n.as_str(), a.as_ref())).collect();
    let merged = d.with_add_ons(&pairs).unwrap_or_else(|_| d.clone());
    out.push("Fields, numbers on each unit (`unit.name`), each with where it starts and how it adds up:".into());
    out.push("| Field | Kind | Starts from | Adds up | Means |".into());
    out.push("|---|---|---|---|---|".into());
    for f in &merged.fields {
        let from = match &f.source {
            FieldSource::Volume => "the solid's volume".to_string(),
            FieldSource::PlanArea => "the solid's plan area".to_string(),
            FieldSource::Thickness => "volume ÷ plan area".to_string(),
            FieldSource::Attribute { attribute } => format!("block attribute `{attribute}`"),
            FieldSource::Parameter { column } => format!("parameter column `{column}`"),
            FieldSource::Worked => "worked by a step".to_string(),
        };
        out.push(format!("| `{}` | {:?} | {from} | {:?} | {} |", f.field.name, f.field.kind, f.field.add_up, meaning(&f.field.name)).replace("AddUp", ""));
    }
    if !merged.words.is_empty() {
        out.push(String::new());
        out.push(format!("Word fields, words on each unit a step may set: {}.", merged.words.iter().map(|w| format!("`{}`", w.name)).collect::<Vec<_>>().join(", ")));
    }
    out.push(String::new());
    out.push(format!(
        "Labels, read only: `seam`, `material`, `bench`, `became`, `horizon`; the levels {}.",
        merged.levels.iter().map(|l| format!("`{}`", l.attribute)).collect::<Vec<_>>().join(", ")
    ));
}

/// The brief, as Markdown.
pub fn brief(project: &Project, b: &BriefFor) -> Result<String, CommandError> {
    let kind = b.kind.ok_or_else(|| CommandError::Invalid("say which kind of script: formula, allocation, calculated, object, table or library".into()))?;
    let mut out: Vec<String> = vec![zd_script::REFERENCE.trim().to_string(), String::new(), "---".into(), String::new()];
    let (shape, limits) = kind_words(kind);
    out.push("## What to write".into());
    out.push(shape.into());
    out.push(limits.into());
    out.push(String::new());

    // What it can read.
    out.push("## What it can read".into());
    let objects = b.objects || kind == ScriptKind::Object;
    let mut dataflow_parameters: Vec<ObjectId> = Vec::new();
    match (kind, b.dataflow, b.table) {
        (ScriptKind::Formula | ScriptKind::Allocation, id, _) if id.is_some() || b.content.is_some() => {
            let saved = id.and_then(|id| project.object(id));
            let d: Dataflow = match (&b.content, saved.map(|o| &o.body)) {
                (Some(d), _) => d.clone(),
                (None, Some(Body::Dataflow(d))) => (**d).clone(),
                _ => return Err(CommandError::Invalid("a formula's brief reads its dataflow: name it".into())),
            };
            out.push(format!("The dataflow{}.", saved.map(|o| format!(" `{}`", o.name)).unwrap_or_default()));
            dataflow_lines(project, &d, &mut out);
            dataflow_parameters = d.parameters.clone();
        }
        _ if objects => {
            let ids = crate::objects_own::solids(project);
            let t = crate::objects_own::objects_rows(project);
            out.push(format!("The Objects table: a row a solid of the project, {} of them, its attributes and measures as columns{}.", ids.len(), if kind == ScriptKind::Object { "; a word column it sets is words, a number column a number" } else { "" }));
            column_lines(&t, b.words, &mut out);
        }
        (_, _, Some(id)) => {
            let o = project.object(id).ok_or(CommandError::MissingObject(id))?;
            let Body::Table(t) = &o.body else { return Err(CommandError::Invalid(format!("{} isn't a table", o.name))) };
            out.push(format!("The table `{}`: {} rows. A name with a folder is read in brackets, `row[\"Insitu\\\\Coal\\\\volume\"]`.", o.name, t.rows()));
            column_lines(t, b.words, &mut out);
            if let Some(setup) = &t.setup {
                dataflow_parameters = match &setup.tables {
                    Some(own) => own.clone(),
                    None => match project.object(setup.dataflow).map(|o| &o.body) {
                        Some(Body::Dataflow(d)) => d.parameters.clone(),
                        _ => Vec::new(),
                    },
                };
            }
        }
        _ => out.push("Nothing of the project's but its parameter tables and scripts, below.".into()),
    }
    if kind == ScriptKind::Table {
        let others: Vec<String> = project.objects().filter(|o| matches!(&o.body, Body::Table(t) if !t.objects)).map(|o| format!("`{}`", o.name)).collect();
        out.push(String::new());
        out.push(format!("Other tables `table(\"name\")` reads: `Objects`{}{}.", if others.is_empty() { "" } else { ", " }, others.join(", ")));
    }
    out.push(String::new());

    // Parameter tables: `p` reads those listed for it; `lookup("name", ...)` any.
    out.push("## Parameter tables".into());
    out.push("`p.column` reads the first row that fits the unit's or row's words, tables in the order listed; `lookup(\"table\", \"column\", key=...)` reads any table by name.".into());
    for o in project.objects() {
        let Body::ParameterTable(t) = &o.body else { continue };
        let read = if dataflow_parameters.contains(&o.id) { " (read through `p`)" } else { "" };
        let filters = if t.filters.is_empty() { String::new() } else { format!(" rows by {}", t.filters.join(", ")) };
        let columns: Vec<String> = t.columns.iter().map(|c| if c.text { format!("`{}` (words)", c.name) } else if c.unit.is_empty() { format!("`{}`", c.name) } else { format!("`{}` ({})", c.name, c.unit) }).collect();
        out.push(format!("- `{}`{read}{filters}: {}", o.name, columns.join(", ")));
    }
    out.push(String::new());

    // The project's scripts and their functions.
    let scripts: Vec<String> = project
        .objects()
        .filter_map(|o| match &o.body {
            Body::Script(s) => {
                let functions: Vec<String> = s.source.lines().filter_map(|l| l.trim_start().strip_prefix("def ")).filter_map(|l| l.split('(').next()).map(|f| format!("`{}`", f.trim())).collect();
                Some(format!("- `{}`: {}{}", o.name, if functions.is_empty() { "no functions".to_string() } else { functions.join(", ") }, if s.note.trim().is_empty() { String::new() } else { format!(". {}", s.note.trim()) }))
            }
            _ => None,
        })
        .collect();
    if !scripts.is_empty() {
        out.push("## The project's scripts, loaded by `load(\"script\", \"function\")`".into());
        out.extend(scripts);
        out.push(String::new());
    }

    // The script as it stands, and the ask.
    let name = if b.name.trim().is_empty() { String::new() } else { format!(" `{}`", b.name.trim()) };
    out.push(format!("## This script{name}"));
    out.push("```python".into());
    out.push(if b.source.trim().is_empty() { "# (empty: write it)".to_string() } else { b.source.trim_end().to_string() });
    out.push("```".into());
    out.push(String::new());
    match b.ask {
        BriefAsk::Write => {
            out.push("## What I want".into());
            out.push("<Say what it should do, in a planner's words, or paste a Spry snippet or an Excel formula here.>".into());
            out.push(String::new());
            out.push("Use only the names above. Answer with the whole script in one ```python block and nothing else, so it can be pasted back as it is. Use made-up numbers in any example, never a client's.".into());
        }
        BriefAsk::Explain => out.push("Explain in plain words what this script does, line by line, for a mine planner who doesn't program. Say what each name is and where its value comes from.".into()),
        BriefAsk::Fix => {
            out.push("## The check found".into());
            for (line, message) in &b.problems {
                out.push(match line {
                    Some(l) => format!("- line {l}: {message}"),
                    None => format!("- {message}"),
                });
            }
            out.push(String::new());
            out.push("Fix these, using only the names above. Answer with the whole script in one ```python block and nothing else.".into());
        }
    }
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::meaning;

    #[test]
    fn the_measures_say_what_they_are() {
        assert!(meaning("true_thickness").contains("cos dip"));
        assert!(meaning("dip").contains("degrees"));
        assert!(meaning("Seam").contains("waste takes the seam below"));
        assert_eq!(meaning("Vol_m3"), "", "a site's own name: the brief says nothing it doesn't know");
    }
}
