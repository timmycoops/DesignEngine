//! A table of results (decision 0072): what a dataflow run makes, a table a case, kept in the project as Parquet. Each
//! column is numbers or words with its unit; an empty cell is a value that isn't there, never a zero. The workflow's HTML
//! report reads these, so a metric a step adds reaches the report as a new column.

use serde::{Deserialize, Serialize};
use zd_reserves::AddUp;

/// A column's values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "values", rename_all = "snake_case")]
pub enum ColumnValues {
    Number(Vec<Option<f64>>),
    Text(Vec<Option<String>>),
}

impl ColumnValues {
    pub fn len(&self) -> usize {
        match self {
            ColumnValues::Number(v) => v.len(),
            ColumnValues::Text(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What made a column (Tables and scripts plan, decision 2): a run, which replaces it when it runs again; or a person,
/// typed or imported, kept by each row's key when the run runs again (`docs/decisions/0099-tables-and-scripts.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnKind {
    #[default]
    Written,
    Entered,
}

impl ColumnKind {
    pub fn is_written(&self) -> bool {
        *self == ColumnKind::Written
    }
}

/// One column: its name, its unit, how it adds up over rows, what made it, and its values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// How the column adds up over rows (a dataflow field's rule), so a report totals a new metric right without
    /// knowing it; none for a label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_up: Option<AddUp>,
    /// Written by a run, or entered by a person. Named `made` in JSON: `kind` is the values' own tag (numbers or words).
    #[serde(rename = "made", default, skip_serializing_if = "ColumnKind::is_written")]
    pub kind: ColumnKind,
    #[serde(flatten)]
    pub values: ColumnValues,
}

impl TableColumn {
    pub fn number(name: &str, unit: &str, values: Vec<Option<f64>>) -> Self {
        TableColumn { name: name.to_string(), unit: unit.to_string(), add_up: None, kind: ColumnKind::Written, values: ColumnValues::Number(values) }
    }

    /// A number column that adds up by a rule.
    pub fn adding(name: &str, unit: &str, add_up: AddUp, values: Vec<Option<f64>>) -> Self {
        TableColumn { name: name.to_string(), unit: unit.to_string(), add_up: Some(add_up), kind: ColumnKind::Written, values: ColumnValues::Number(values) }
    }

    pub fn text(name: &str, values: Vec<Option<String>>) -> Self {
        TableColumn { name: name.to_string(), unit: String::new(), add_up: None, kind: ColumnKind::Written, values: ColumnValues::Text(values) }
    }

    /// The column marked as entered by a person (decision 2 of 0099).
    pub fn entered(mut self) -> Self {
        self.kind = ColumnKind::Entered;
        self
    }
}

/// A column's name, unit and kind without its values: what the project file's objects table holds, the values going in
/// a Parquet entry of their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnHeader {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Words rather than numbers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub text: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_up: Option<AddUp>,
    #[serde(default, skip_serializing_if = "ColumnKind::is_written")]
    pub kind: ColumnKind,
}

/// A table: columns of one length, names unique.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<TableColumn>,
}

impl Table {
    /// Whether the table holds together: names given and unique, every column as long as the first.
    pub fn check(&self) -> Result<(), String> {
        let rows = self.rows();
        for (i, c) in self.columns.iter().enumerate() {
            if c.name.trim().is_empty() {
                return Err(format!("column {} has no name", i + 1));
            }
            if self.columns[..i].iter().any(|d| d.name == c.name) {
                return Err(format!("the column {} is named twice", c.name));
            }
            if c.values.len() != rows {
                return Err(format!("the column {} has {} values for {rows} rows", c.name, c.values.len()));
            }
        }
        Ok(())
    }

    pub fn rows(&self) -> usize {
        self.columns.first().map_or(0, |c| c.values.len())
    }

    pub fn column(&self, name: &str) -> Option<&TableColumn> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// The columns' names, units and kinds.
    pub fn headers(&self) -> Vec<ColumnHeader> {
        self.columns.iter().map(|c| ColumnHeader { name: c.name.clone(), unit: c.unit.clone(), text: matches!(c.values, ColumnValues::Text(_)), add_up: c.add_up.clone(), kind: c.kind }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_holds_together_or_is_refused() {
        let t = Table {
            columns: vec![
                TableColumn::text("seam", vec![Some("U".into()), Some("L".into())]),
                TableColumn::number("volume", "m3", vec![Some(100.0), None]),
            ],
        };
        t.check().unwrap();
        assert_eq!(t.rows(), 2);
        assert_eq!(t.headers()[1], ColumnHeader { name: "volume".into(), unit: "m3".into(), text: false, add_up: None, kind: ColumnKind::Written });
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Table>(&json).unwrap(), t);
        // An entered column says so beside its values' own kind, and comes back entered.
        let mut entered = t.clone();
        entered.columns.push(TableColumn::text("comment", vec![Some("first".into()), None]).entered());
        let json = serde_json::to_string(&entered).unwrap();
        assert!(json.contains(r#""made":"entered""#), "{json}");
        assert_eq!(serde_json::from_str::<Table>(&json).unwrap(), entered);
        let mut short = t.clone();
        short.columns.push(TableColumn::number("rd", "t/m3", vec![Some(1.5)]));
        assert!(short.check().unwrap_err().contains("1 values for 2 rows"));
    }
}
