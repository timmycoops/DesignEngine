//! Update records from a file (the Build Plan's "Tables and scripts plan", decision 5,
//! `docs/decisions/0099-tables-and-scripts.md`): a CSV matched on key columns to objects (their attributes) or to a
//! table's rows (its entered columns), with a report of what matches and what would change before anything is written.
//! The same matching gives the report and the run, so what's shown is what's written.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, Change, ColumnKind, ColumnValues, ObjectId, Project, Table, TableColumn, attribute_ci, attribute_word, csv_cells};

use super::attributes::NOT_TYPED;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

/// What the file updates: objects' attributes, or a table's entered columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "of")]
pub enum RecordsTarget {
    Objects { objects: Vec<ObjectId> },
    Table { table: ObjectId },
}

/// Update: the file's values where its key matches, blank cells leaving a value as it is. Replace: the file's columns
/// replaced, so what the file leaves blank, or a record it doesn't have, is cleared.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordsMode {
    #[default]
    Update,
    Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRecordsParams {
    pub target: RecordsTarget,
    /// The CSV's text: a header row, `name (unit)` where a column has a unit, then a record a row.
    pub csv: String,
    /// The file's columns its records are matched on (block, or strip, block and seam): an object's name or attribute,
    /// a table's column of the same name.
    pub key: Vec<String>,
    #[serde(default)]
    pub mode: RecordsMode,
}

/// One value the file would change: the record it's on, the column, what it is and what it would be.
#[derive(Debug, Clone, Serialize)]
pub struct RecordChange {
    pub record: String,
    pub column: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// What the file matches and would change (decision 5), before anything is written.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecordsReport {
    /// The file's columns that are its values, not its key.
    pub columns: Vec<String>,
    /// The file's records, and how many matched something.
    pub rows: usize,
    pub matched: usize,
    /// Objects or table rows a record matched: a key of block alone matches each seam of the block.
    pub targets: usize,
    /// Keys in the file only, as words, the first [`SHOWN`].
    pub file_only: Vec<String>,
    pub file_only_count: usize,
    /// Objects or table rows no record matched.
    pub target_only: usize,
    /// The values that would change, the first [`SHOWN`], and how many in all.
    pub changes: Vec<RecordChange>,
    pub changed: usize,
    /// Why the file can't be taken as it is: nothing is written while there's any.
    pub refused: Vec<String>,
}

/// How many keys in the file only, and changes, the report lists; the rest are counted.
pub const SHOWN: usize = 200;

/// The file read: its header (name and unit a column) and its rows of cells.
pub(crate) struct File {
    pub(crate) names: Vec<String>,
    pub(crate) units: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
}

pub(crate) fn read_file(csv: &str) -> Result<File, String> {
    let text = csv.strip_prefix('\u{feff}').unwrap_or(csv);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = csv_cells(lines.next().ok_or("the file is empty")?);
    let (mut names, mut units) = (Vec::new(), Vec::new());
    for h in &header {
        let h = h.trim();
        let (name, unit) = match (h.rfind('('), h.ends_with(')')) {
            (Some(open), true) if open > 0 => (h[..open].trim(), h[open + 1..h.len() - 1].trim()),
            _ => (h, ""),
        };
        if name.is_empty() {
            return Err("a column of the file has no name".into());
        }
        if names.iter().any(|n: &String| n.eq_ignore_ascii_case(name)) {
            return Err(format!("the file names {name} twice"));
        }
        names.push(name.to_string());
        units.push(unit.to_string());
    }
    let mut rows = Vec::new();
    for (i, line) in lines.enumerate() {
        let mut cells = csv_cells(line);
        if cells.len() > names.len() {
            return Err(format!("row {} has {} cells for {} columns", i + 1, cells.len(), names.len()));
        }
        cells.resize(names.len(), String::new());
        rows.push(cells.into_iter().map(|c| c.trim().to_string()).collect());
    }
    Ok(File { names, units, rows })
}

/// A key's word: a number as one (`3`, `3.0` and `03` the same), words as they are.
fn key_word(cell: &str) -> String {
    match cell.trim().parse::<f64>() {
        Ok(x) if x.is_finite() => attribute_word(&AttrValue::Number(x)),
        _ => cell.trim().to_string(),
    }
}

fn value_of(cell: &str) -> AttrValue {
    match cell.parse::<f64>() {
        Ok(x) if x.is_finite() => AttrValue::Number(x),
        _ => AttrValue::Text(cell.to_string()),
    }
}

/// The file matched to its target: the report, and the changes that write it.
pub fn match_records(project: &Project, p: &UpdateRecordsParams) -> Result<(RecordsReport, Vec<Change>), CommandError> {
    let file = read_file(&p.csv).map_err(CommandError::Invalid)?;
    let mut report = RecordsReport { rows: file.rows.len(), ..Default::default() };
    if p.key.is_empty() {
        report.refused.push("pick the file's columns to match its records on".into());
    }
    let mut key_at = Vec::new();
    for k in &p.key {
        match file.names.iter().position(|n| n.eq_ignore_ascii_case(k)) {
            Some(at) => key_at.push(at),
            None => report.refused.push(format!("the file has no column {k} to match on")),
        }
    }
    let values: Vec<usize> = (0..file.names.len()).filter(|i| !key_at.contains(i)).collect();
    report.columns = values.iter().map(|&i| file.names[i].clone()).collect();
    if values.is_empty() && !p.key.is_empty() {
        report.refused.push("the file has no columns besides its key: nothing to update".into());
    }
    // A record a key; two records with one key is a file that can't say which to take.
    let mut records: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    let mut doubled = BTreeSet::new();
    for (r, row) in file.rows.iter().enumerate() {
        let key: Vec<String> = key_at.iter().map(|&c| key_word(&row[c])).collect();
        if records.insert(key.clone(), r).is_some() {
            doubled.insert(key.join(" "));
        }
    }
    if !doubled.is_empty() {
        report.refused.push(format!("{} key{} in the file more than once: {}", doubled.len(), if doubled.len() == 1 { " is" } else { "s are" }, doubled.into_iter().take(10).collect::<Vec<_>>().join(", ")));
    }
    let keys: Vec<String> = key_at.iter().map(|&c| file.names[c].clone()).collect();
    let changes = match &p.target {
        RecordsTarget::Objects { objects } => objects_changes(project, objects, &file, &keys, &values, &records, p.mode, &mut report)?,
        RecordsTarget::Table { table } => table_changes(project, *table, &file, &keys, &values, &records, p.mode, &mut report)?,
    };
    let changes = if report.refused.is_empty() { changes } else { Vec::new() };
    Ok((report, changes))
}

/// Notes a record matched or a file-only key; a value that changes, listed up to [`SHOWN`].
fn note_change(report: &mut RecordsReport, record: &str, column: &str, old: Option<String>, new: Option<String>) {
    if old == new {
        return;
    }
    report.changed += 1;
    if report.changes.len() < SHOWN {
        report.changes.push(RecordChange { record: record.to_string(), column: column.to_string(), old, new });
    }
}

fn file_only(report: &mut RecordsReport, records: &BTreeMap<Vec<String>, usize>, used: &BTreeSet<Vec<String>>) {
    report.matched = used.len();
    for key in records.keys().filter(|k| !used.contains(*k)) {
        report.file_only_count += 1;
        if report.file_only.len() < SHOWN {
            report.file_only.push(key.join(" "));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn objects_changes(
    project: &Project,
    objects: &[ObjectId],
    file: &File,
    keys: &[String],
    values: &[usize],
    records: &BTreeMap<Vec<String>, usize>,
    mode: RecordsMode,
    report: &mut RecordsReport,
) -> Result<Vec<Change>, CommandError> {
    for &c in values {
        if NOT_TYPED.iter().any(|n| n.eq_ignore_ascii_case(&file.names[c])) {
            report.refused.push(format!("{} is the object's own, not an attribute the file can set", file.names[c]));
        }
    }
    let layer = |o: &zd_core::Object| project.layer(o.layer).map(|l| l.name.clone()).unwrap_or_default();
    let mut used = BTreeSet::new();
    let mut changes = Vec::new();
    for &id in objects {
        let object = project.object(id).ok_or(CommandError::MissingObject(id))?;
        let key: Vec<String> = keys
            .iter()
            .map(|k| match k.to_ascii_lowercase().as_str() {
                "name" => key_word(&object.name),
                "layer" => key_word(&layer(object)),
                _ => attribute_ci(&object.attributes, k).map(attribute_word).unwrap_or_default(),
            })
            .collect();
        let row = records.get(&key).map(|&r| &file.rows[r]);
        if row.is_some() {
            used.insert(key.clone());
            report.targets += 1;
        } else {
            report.target_only += 1;
            if mode == RecordsMode::Update {
                continue;
            }
        }
        let mut attributes = object.attributes.clone();
        for &c in values {
            let name = &file.names[c];
            let cell = row.map_or("", |r| r[c].as_str());
            let old = attributes.get(name).map(attribute_word);
            if cell.is_empty() {
                if mode == RecordsMode::Replace {
                    attributes.remove(name);
                }
            } else {
                attributes.insert(name.clone(), value_of(cell));
            }
            let new = attributes.get(name).map(attribute_word);
            note_change(report, &object.name, name, old, new);
        }
        if attributes != object.attributes {
            changes.push(Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: Some(attributes), body: None });
        }
    }
    file_only(report, records, &used);
    Ok(changes)
}

fn word_at(t: &Table, column: &str, row: usize) -> String {
    match t.column(column).map(|c| &c.values) {
        Some(ColumnValues::Text(v)) => v[row].as_deref().map(key_word).unwrap_or_default(),
        Some(ColumnValues::Number(v)) => v[row].map(|x| attribute_word(&AttrValue::Number(x))).unwrap_or_default(),
        None => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn table_changes(
    project: &Project,
    table: ObjectId,
    file: &File,
    keys: &[String],
    values: &[usize],
    records: &BTreeMap<Vec<String>, usize>,
    mode: RecordsMode,
    report: &mut RecordsReport,
) -> Result<Vec<Change>, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::Table(t) = &object.body else {
        return Err(CommandError::WrongKind { id: table, expected: zd_core::ObjectKind::Table, found: object.kind() });
    };
    // The table's own column for each key, by name, its case aside.
    let mut key_columns = Vec::new();
    for k in keys {
        match t.columns.iter().find(|c| c.name.eq_ignore_ascii_case(k)) {
            Some(c) => key_columns.push(c.name.clone()),
            None => report.refused.push(format!("{} has no column {k} to match on", object.name)),
        }
    }
    // Each value column: the table's entered one of that name, or a new one; a written one is never typed over.
    let mut columns: Vec<TableColumn> = Vec::new();
    for &c in values {
        let (name, unit) = (&file.names[c], &file.units[c]);
        let numbers = file.rows.iter().all(|r| r[c].is_empty() || r[c].parse::<f64>().is_ok());
        match t.columns.iter().find(|tc| tc.name.eq_ignore_ascii_case(name)) {
            Some(tc) if tc.kind == ColumnKind::Written => report.refused.push(format!("{} was written by the run: a reserve is changed through its blocks, its parameter rows or a formula, never by a file", tc.name)),
            Some(tc) => {
                if !unit.is_empty() && !tc.unit.is_empty() && !unit.eq_ignore_ascii_case(&tc.unit) {
                    report.refused.push(format!("the file's {name} is in {unit}, the table's in {}", tc.unit));
                }
                if matches!(tc.values, ColumnValues::Number(_)) && !numbers {
                    report.refused.push(format!("{} holds numbers: the file has words in it", tc.name));
                }
                columns.push(tc.clone());
            }
            None => {
                let rows = t.rows();
                let column = if numbers { TableColumn::number(name, unit, vec![None; rows]) } else { TableColumn::text(name, vec![None; rows]) };
                columns.push(column.entered());
            }
        }
    }
    if !report.refused.is_empty() {
        return Ok(Vec::new());
    }
    let label = |r: usize| {
        let words: Vec<String> = zd_core::leaves::levels(t).iter().map(|l| word_at(t, l, r)).filter(|w| !w.is_empty()).collect();
        if words.is_empty() { format!("row {}", r + 1) } else { words.join(" ") }
    };
    let mut used = BTreeSet::new();
    for r in 0..t.rows() {
        let key: Vec<String> = key_columns.iter().map(|k| word_at(t, k, r)).collect();
        let row = records.get(&key).map(|&i| &file.rows[i]);
        if row.is_some() {
            used.insert(key);
            report.targets += 1;
        } else {
            report.target_only += 1;
            if mode == RecordsMode::Update {
                continue;
            }
        }
        for (column, &c) in columns.iter_mut().zip(values) {
            let cell = row.map_or("", |row| row[c].as_str());
            if cell.is_empty() && mode == RecordsMode::Update {
                continue;
            }
            let name = column.name.clone();
            match &mut column.values {
                ColumnValues::Number(v) => {
                    let old = v[r].map(|x| attribute_word(&AttrValue::Number(x)));
                    v[r] = cell.parse().ok();
                    let new = v[r].map(|x| attribute_word(&AttrValue::Number(x)));
                    note_change(report, &label(r), &name, old, new);
                }
                ColumnValues::Text(v) => {
                    let old = v[r].clone();
                    v[r] = Some(cell.to_string()).filter(|w| !w.is_empty());
                    note_change(report, &label(r), &name, old, v[r].clone());
                }
            }
        }
    }
    file_only(report, records, &used);
    if report.changed == 0 {
        return Ok(Vec::new());
    }
    let mut updated: Table = (**t).clone();
    for column in columns {
        match updated.columns.iter_mut().find(|c| c.name == column.name) {
            Some(at) => *at = column,
            None => updated.columns.push(column),
        }
    }
    Ok(vec![Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(updated))) }])
}

/// `table.update_records`: a CSV's records matched on key columns to objects or a table's rows, and its values written
/// (decision 5): objects' attributes, or a table's entered columns. Refused, with every reason, where the file can't be
/// taken as it is; nothing written then. What it matched and changed is said.
pub struct UpdateRecords;

impl Command for UpdateRecords {
    type Params = UpdateRecordsParams;
    const NAME: &'static str = "table.update_records";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Update records from a CSV, matched on key columns: objects' attributes or a table's entered columns",
            inputs: vec![Field::new("target", FieldType::Text, "The objects, or a table")],
            params: vec![
                Field::new("csv", FieldType::Text, "The file's text, a header row then a record a row"),
                Field::new("key", FieldType::Text, "The file's columns to match on"),
                Field::new("mode", FieldType::Text, "update (blank cells leave a value) or replace (the file's columns replaced)"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        // Every object the file may touch is read through the inputs, so the run records them.
        match &p.target {
            RecordsTarget::Objects { objects } => {
                for &id in objects {
                    inputs.object(id)?;
                }
            }
            RecordsTarget::Table { table } => {
                inputs.object(*table)?;
            }
        }
        let (report, changes) = match_records(inputs.project(), p)?;
        if !report.refused.is_empty() {
            return Err(CommandError::Invalid(report.refused.join("; ")));
        }
        if changes.is_empty() {
            return Err(CommandError::Invalid(format!("the file's {} record{} change nothing", report.rows, if report.rows == 1 { "" } else { "s" })));
        }
        let mut out = Outcome::new(changes);
        out.notes.push(Note::info(format!(
            "{} of {} record{} matched {} {}; {} value{} changed",
            report.matched,
            report.rows,
            if report.rows == 1 { "" } else { "s" },
            report.targets,
            match p.target {
                RecordsTarget::Objects { .. } => "objects",
                RecordsTarget::Table { .. } => "rows",
            },
            report.changed,
            if report.changed == 1 { "" } else { "s" }
        )));
        if report.file_only_count > 0 {
            out.notes.push(Note::info(format!("{} record{} in the file only: {}", report.file_only_count, if report.file_only_count == 1 { "" } else { "s" }, report.file_only.iter().take(10).cloned().collect::<Vec<_>>().join(", "))));
        }
        Ok(out)
    }
}
