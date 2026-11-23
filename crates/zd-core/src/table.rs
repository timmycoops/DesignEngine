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
    /// Worked out from the row when shown (decision 6): a table's grid says a calculated column is one. A column of
    /// values is never calculated; its script is a [`CalculatedColumn`].
    Calculated,
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

/// How a calculated column adds up over a group of rows (Tables and scripts plan, decision 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum CalcAddUp {
    /// Worked again on the group's own inputs, each added up by its rule: a ratio's way, so a strip's strip ratio is its
    /// waste over its coal, never an average of its blocks'.
    FromInputs,
    /// Worked on each row, then summed: a test on each leaf (tonnes where ash is under 12).
    Sum,
    /// Worked on each row, then the largest.
    Max,
    /// Worked on each row, then the smallest.
    Min,
    /// Worked on each row, then averaged weighted by a column of the table (a quality by its tonnes).
    WeightedBy { column: String },
}

/// A column worked out from each row by a script, when it's shown, never kept as numbers (decision 6): a formula that
/// sets the column on the row, `def formula(row, p): row.strip_ratio = …`, reading the row's columns, its words, and the
/// parameter tables it names, by the row's words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalculatedColumn {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    pub script: String,
    pub adds: CalcAddUp,
    /// The parameter tables `p` reads, in order: a row's value is the first row of them that fits its words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<crate::ObjectId>,
    /// The project's scripts its formula loads, bound by name when it's put on the table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<crate::ObjectId>,
    /// The parameter tables its formula looks up by name (Schedule quantities plan, decision 1), bound when it's put on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookups: Vec<crate::ObjectId>,
    /// The name each of `lookups` is looked up by (Table setup plan, decision 4), so a renamed table is still read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookup_names: Vec<String>,
}

/// Which blocks a reserves table is run on (the Build Plan's "Table setup plan", decision 1): every solid on these layers
/// that fits the filter, in the bench rules' words, so a block added to a layer is taken up; or, where a run took only
/// some of a layer's solids, exactly these.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPick {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<crate::LayerId>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub filter: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<crate::ObjectId>,
}

impl BlockPick {
    /// The project's solids it picks, in the project's order: exactly its ids that are still there, or every solid on its
    /// layers that fits its filter.
    pub fn resolve<'a>(&self, objects: impl IntoIterator<Item = &'a std::sync::Arc<crate::Object>>) -> Vec<crate::ObjectId> {
        let solids = objects.into_iter().filter(|o| matches!(o.body, crate::Body::Solid(_)));
        if !self.ids.is_empty() {
            let there: Vec<crate::ObjectId> = solids.map(|o| o.id).collect();
            return self.ids.iter().copied().filter(|id| there.contains(id)).collect();
        }
        solids.filter(|o| self.layers.contains(&o.layer) && crate::filter_fits(&self.filter, &o.attributes)).map(|o| o.id).collect()
    }

    /// The pick a run's blocks make: their layers where they're every solid on them, else exactly these.
    pub fn of_blocks<'a>(objects: impl IntoIterator<Item = &'a std::sync::Arc<crate::Object>> + Clone, blocks: &[crate::ObjectId]) -> BlockPick {
        let mut layers: Vec<crate::LayerId> = Vec::new();
        for o in objects.clone().into_iter().filter(|o| blocks.contains(&o.id)) {
            if !layers.contains(&o.layer) {
                layers.push(o.layer);
            }
        }
        let by_layers = BlockPick { layers, ..BlockPick::default() };
        let mut all = by_layers.resolve(objects);
        let mut given = blocks.to_vec();
        all.sort_unstable();
        given.sort_unstable();
        given.dedup();
        if all == given { by_layers } else { BlockPick { ids: blocks.to_vec(), ..BlockPick::default() } }
    }
}

/// What fills a reserves table (the Build Plan's "Table setup plan", decision 1): the dataflow, the blocks, the seam model
/// that orders each column or the stack where there's none, and the table's own parameter rows, read first. Its case is
/// the table's own. Run again runs it; every run is still logged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSetup {
    pub dataflow: crate::ObjectId,
    /// The add-on dataflows run with it, in order (decision 2); none, the dataflow's own list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_ons: Option<Vec<crate::ObjectId>>,
    /// The parameter tables it reads, in order, the case's first (decision 4); none, the dataflow's own list. Its
    /// add-ons' tables are read after them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tables: Option<Vec<crate::ObjectId>>,
    pub blocks: BlockPick,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::ObjectId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<crate::ParameterTable>,
    /// Where a field comes from for this table, by the field's name, in place of the dataflow's (decision 3): a design
    /// dip from a parameter column where the dataflow reads the block's measured one.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub sources: std::collections::BTreeMap<String, crate::FieldSource>,
}

/// A table: columns of one length, names unique, and the columns worked out from them; a reserves table also what fills
/// it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<TableColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calculated: Vec<CalculatedColumn>,
    /// What fills it, for a reserves table (Table setup plan, decision 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<TableSetup>,
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
        for (i, c) in self.calculated.iter().enumerate() {
            if c.name.trim().is_empty() {
                return Err("a calculated column has no name".into());
            }
            if self.column(&c.name).is_some() || self.calculated[..i].iter().any(|d| d.name == c.name) {
                return Err(format!("the column {} is named twice", c.name));
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
            calculated: Vec::new(),
            setup: None,
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
