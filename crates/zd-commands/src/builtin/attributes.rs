//! Values a person puts in a table (the Build Plan's "Tables and scripts plan", decisions 2 to 4,
//! `docs/decisions/0099-tables-and-scripts.md`): objects' attributes typed or pasted in the Tables tab's grid, and the
//! entered columns of a reserves table. Each is a command run, one undo; a number a run wrote is never typed over.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, Change, ColumnKind, ColumnValues, ObjectId, ObjectKind, Table, TableColumn};

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

/// A value typed in: a number, words, yes or no, or nothing (the value taken away).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Entry {
    Number(f64),
    Bool(bool),
    Text(String),
}

/// `object.set_attributes`: attributes set on objects, a cell at a time as the grid gives them, every object revised once
/// in one run. Its geometry, name and layer stay as they were; so does a solid's volume, the volume of record. A value of
/// nothing takes the attribute away.
pub struct SetAttributes;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetAttributesParams {
    pub cells: Vec<AttributeCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeCell {
    pub object: ObjectId,
    pub attribute: String,
    #[serde(default)]
    pub value: Option<Entry>,
}

/// Attributes an object's own measures or the cut own: the grid shows them, but they're not typed over.
pub(crate) const NOT_TYPED: [&str; 5] = ["name", "layer", "kind", "volume", "tonnes"];

impl Command for SetAttributes {
    type Params = SetAttributesParams;
    const NAME: &'static str = "object.set_attributes";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Set attributes on objects, as typed or pasted in a table",
            inputs: vec![Field::new("cells", FieldType::Text, "Each object, attribute and value; no value takes the attribute away")],
            params: vec![],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.cells.is_empty() {
            return Err(CommandError::Invalid("no attributes to set".into()));
        }
        // Every object once, its cells in the order given: the last value for an attribute wins.
        let mut by_object: BTreeMap<ObjectId, Vec<&AttributeCell>> = BTreeMap::new();
        for cell in &p.cells {
            let name = cell.attribute.trim();
            if name.is_empty() {
                return Err(CommandError::Invalid("an attribute needs a name".into()));
            }
            if NOT_TYPED.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                return Err(CommandError::Invalid(format!("{name} is the object's own, not an attribute to type: rename or measure it instead")));
            }
            by_object.entry(cell.object).or_default().push(cell);
        }
        let mut changes = Vec::new();
        let (mut set, mut taken) = (0usize, 0usize);
        for (id, cells) in &by_object {
            let object = inputs.object(*id)?;
            let mut attributes = object.attributes.clone();
            for cell in cells {
                let name = cell.attribute.trim().to_string();
                match &cell.value {
                    Some(Entry::Number(x)) if !x.is_finite() => return Err(CommandError::Invalid(format!("{}: {name} must be a number that's there", object.name))),
                    Some(Entry::Number(x)) => {
                        attributes.insert(name, AttrValue::Number(*x));
                        set += 1;
                    }
                    Some(Entry::Bool(b)) => {
                        attributes.insert(name, AttrValue::Bool(*b));
                        set += 1;
                    }
                    Some(Entry::Text(w)) if w.trim().is_empty() => {
                        taken += usize::from(attributes.remove(&name).is_some());
                    }
                    Some(Entry::Text(w)) => {
                        attributes.insert(name, AttrValue::Text(w.trim().to_string()));
                        set += 1;
                    }
                    None => taken += usize::from(attributes.remove(&name).is_some()),
                }
            }
            if attributes != object.attributes {
                changes.push(Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: Some(attributes), body: None });
            }
        }
        let objects = changes.len();
        let mut out = Outcome::new(changes);
        let mut said = format!("{set} value{} set", plural(set));
        if taken > 0 {
            said.push_str(&format!(", {taken} taken away"));
        }
        out.notes.push(Note::info(format!("{said} on {objects} object{}", plural(objects))));
        Ok(out)
    }
}

/// `table.enter`: values a person enters in a table's column (decision 2 of 0099), a column of their own, kept by each row's
/// key when its run runs again. A column that isn't there yet is made: numbers where every value given is one, else
/// words. A column a run wrote is refused: a reserve is changed through its inputs, never typed over (decision 3).
pub struct EnterInTable;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnterParams {
    pub table: ObjectId,
    pub column: String,
    /// A new column's unit (m3, t, %); a column that's there keeps its own.
    #[serde(default)]
    pub unit: String,
    /// A new column of words, whatever the values given; else numbers where every value given is one.
    #[serde(default)]
    pub words: bool,
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableCell {
    pub row: usize,
    #[serde(default)]
    pub value: Option<Entry>,
}

impl Command for EnterInTable {
    type Params = EnterParams;
    const NAME: &'static str = "table.enter";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Enter values in a table's own column, kept when its run runs again",
            inputs: vec![Field::new("table", FieldType::Text, "Id of a results table")],
            params: vec![
                Field::new("column", FieldType::Text, "The entered column, made if it isn't there"),
                Field::new("unit", FieldType::Text, "A new column's unit"),
                Field::new("cells", FieldType::Text, "Each row and its value; no value clears the cell"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.table)?;
        let Body::Table(table) = &object.body else {
            return Err(CommandError::WrongKind { id: p.table, expected: ObjectKind::Table, found: object.kind() });
        };
        let name = p.column.trim();
        if name.is_empty() {
            return Err(CommandError::Invalid("the column needs a name".into()));
        }
        let rows = table.rows();
        if let Some(c) = p.cells.iter().find(|c| c.row >= rows) {
            return Err(CommandError::Invalid(format!("{} has {rows} rows, not a row {}", object.name, c.row + 1)));
        }
        let mut t: Table = (**table).clone();
        let at = match t.columns.iter().position(|c| c.name == name) {
            Some(at) if t.columns[at].kind == ColumnKind::Written => {
                return Err(CommandError::Invalid(format!("{name} was written by the run: a reserve is changed through its blocks, its parameter rows or a formula, never typed over")));
            }
            Some(at) => at,
            None => {
                let numbers = !p.words && p.cells.iter().all(|c| !matches!(c.value, Some(Entry::Text(_)) | Some(Entry::Bool(_))));
                let column = if numbers { TableColumn::number(name, p.unit.trim(), vec![None; rows]) } else { TableColumn::text(name, vec![None; rows]) };
                t.columns.push(column.entered());
                t.columns.len() - 1
            }
        };
        let column = &mut t.columns[at];
        let mut entered = 0;
        for cell in &p.cells {
            match &mut column.values {
                ColumnValues::Number(v) => {
                    v[cell.row] = match &cell.value {
                        None => None,
                        Some(Entry::Number(x)) if x.is_finite() => Some(*x),
                        Some(Entry::Text(w)) if w.trim().is_empty() => None,
                        Some(other) => return Err(CommandError::Invalid(format!("{name} holds numbers: row {} can't take {}", cell.row + 1, words(other)))),
                    }
                }
                ColumnValues::Text(v) => {
                    v[cell.row] = match &cell.value {
                        None => None,
                        Some(e) => Some(words(e)).filter(|w| !w.is_empty()),
                    }
                }
            }
            entered += 1;
        }
        let mut out = Outcome::new(vec![Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(t))) }]);
        out.notes.push(Note::info(format!("{}: {entered} value{} entered in {name}", object.name, plural(entered))));
        Ok(out)
    }
}

/// `table.remove_column`: a column a person entered taken out of a table; one the run wrote is refused.
pub struct RemoveColumn;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveColumnParams {
    pub table: ObjectId,
    pub column: String,
}

impl Command for RemoveColumn {
    type Params = RemoveColumnParams;
    const NAME: &'static str = "table.remove_column";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Take an entered column out of a table",
            inputs: vec![Field::new("table", FieldType::Text, "Id of a results table")],
            params: vec![Field::new("column", FieldType::Text, "The entered column")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.table)?;
        let Body::Table(table) = &object.body else {
            return Err(CommandError::WrongKind { id: p.table, expected: ObjectKind::Table, found: object.kind() });
        };
        let at = table.columns.iter().position(|c| c.name == p.column).ok_or_else(|| CommandError::Invalid(format!("{} has no column {}", object.name, p.column)))?;
        if table.columns[at].kind == ColumnKind::Written {
            return Err(CommandError::Invalid(format!("{} was written by the run: only a column entered by hand comes out", p.column)));
        }
        let mut t: Table = (**table).clone();
        t.columns.remove(at);
        let mut out = Outcome::new(vec![Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: None, body: Some(Body::Table(Arc::new(t))) }]);
        out.notes.push(Note::info(format!("{}: {} taken out", object.name, p.column)));
        Ok(out)
    }
}

fn words(e: &Entry) -> String {
    match e {
        Entry::Number(x) => x.to_string(),
        Entry::Bool(b) => (if *b { "yes" } else { "no" }).to_string(),
        Entry::Text(w) => w.trim().to_string(),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
