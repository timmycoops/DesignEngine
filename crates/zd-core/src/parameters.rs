//! Parameter tables (decision 0072): the assumptions a tool reads, one table any tool can use. The reserves dataflow reads
//! its settings from them first; dragline, haulage and equipment parameters later. Tim: "lots of mine planning
//! assumptions will generally be parameter table type data".
//!
//! A table has filter columns (strip, block, seam, horizon, material, bench or any attribute, each cell a comma list of
//! words, numbers or ranges such as `1-5`, written as the bench rules are) and value columns, each with its unit. It's read
//! top down: a thing takes each value from the first row that fits it and holds that value, so a row can change one value
//! for one seam and leave the rest to the rows below; the last row, its filters blank, is the default.
//!
//! As CSV a filter column's header is `when <attribute>`, a value column's `<name>` or `<name> (<unit>)`:
//!
//! ```text
//! when seam,when strip,ash_cutoff (%),waste_rd (t/m3)
//! U,1-5,30,
//! ,,35,2.3
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Attributes;
use crate::filters::filter_fits;

/// One value in a table: a number or a word.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Number(f64),
    Text(String),
}

impl ParamValue {
    pub fn number(&self) -> Option<f64> {
        match self {
            ParamValue::Number(n) => Some(*n),
            ParamValue::Text(_) => None,
        }
    }

    fn cell(&self) -> String {
        match self {
            ParamValue::Number(n) => n.to_string(),
            ParamValue::Text(t) => t.clone(),
        }
    }
}

/// A value column: its name, its unit, whether it holds words, and a note on what it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueColumn {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Holds words rather than numbers.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub text: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl ValueColumn {
    pub fn number(name: &str, unit: &str) -> Self {
        ValueColumn { name: name.to_string(), unit: unit.to_string(), text: false, note: String::new() }
    }

    pub fn text(name: &str) -> Self {
        ValueColumn { name: name.to_string(), unit: String::new(), text: true, note: String::new() }
    }
}

/// One row: what a thing must carry to take it, and the values it gives. A value left out passes to the rows below.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterRow {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
    #[serde(default)]
    pub values: BTreeMap<String, ParamValue>,
}

/// Where a value was found: the table (its place in the list read), the row and the value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Found<'a> {
    pub table: usize,
    pub row: usize,
    pub value: &'a ParamValue,
}

/// A parameter table: filter columns, value columns and rows read top down.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterTable {
    /// The attributes rows filter on, in the order shown.
    #[serde(default)]
    pub filters: Vec<String>,
    pub columns: Vec<ValueColumn>,
    #[serde(default)]
    pub rows: Vec<ParameterRow>,
}

impl ParameterTable {
    /// Whether the table holds together: names given and unique (filters and values apart, case aside), every row's
    /// filters among the table's, every value in a column of its kind, numbers finite.
    pub fn check(&self) -> Result<(), String> {
        let mut seen: Vec<String> = Vec::new();
        for name in self.filters.iter().map(|f| ("filter", f)).chain(self.columns.iter().map(|c| ("value column", &c.name))) {
            let (what, name) = name;
            let key = name.trim().to_lowercase();
            if key.is_empty() {
                return Err(format!("a {what} has no name"));
            }
            if seen.contains(&key) {
                return Err(format!("{name} is named twice"));
            }
            seen.push(key);
        }
        for (i, row) in self.rows.iter().enumerate() {
            let at = i + 1;
            for key in row.when.keys() {
                if !self.filters.iter().any(|f| f.eq_ignore_ascii_case(key)) {
                    return Err(format!("row {at} filters on {key}, which isn't one of the table's filters"));
                }
            }
            for (name, value) in &row.values {
                let Some(column) = self.column(name) else {
                    return Err(format!("row {at} gives {name}, which isn't one of the table's columns"));
                };
                match value {
                    ParamValue::Number(n) if !n.is_finite() => return Err(format!("row {at}: {name} is {n}")),
                    ParamValue::Text(t) if !column.text => return Err(format!("row {at}: {name} holds numbers, not '{t}'")),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// A value column by name, case aside.
    pub fn column(&self, name: &str) -> Option<&ValueColumn> {
        self.columns.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The first row a thing carrying these attributes fits that holds this value.
    pub fn lookup(&self, attributes: &Attributes, column: &str) -> Option<(usize, &ParamValue)> {
        self.rows.iter().enumerate().find_map(|(i, row)| {
            let value = row.values.get(column).or_else(|| row.values.iter().find(|(k, _)| k.eq_ignore_ascii_case(column)).map(|(_, v)| v))?;
            filter_fits(&row.when, attributes).then_some((i, value))
        })
    }

    /// The rows of each table in turn, the first table's first: a case's or a step's own rows read before the rest.
    pub fn lookup_in<'a>(tables: &[&'a ParameterTable], attributes: &Attributes, column: &str) -> Option<Found<'a>> {
        tables.iter().enumerate().find_map(|(t, table)| table.lookup(attributes, column).map(|(row, value)| Found { table: t, row, value }))
    }

    /// The table as CSV, its filters first.
    pub fn to_csv(&self) -> String {
        let mut header: Vec<String> = self.filters.iter().map(|f| format!("when {f}")).collect();
        header.extend(self.columns.iter().map(|c| if c.unit.is_empty() { c.name.clone() } else { format!("{} ({})", c.name, c.unit) }));
        let mut out = csv_line(&header);
        for row in &self.rows {
            let mut cells: Vec<String> = self
                .filters
                .iter()
                .map(|f| row.when.iter().find(|(k, _)| k.eq_ignore_ascii_case(f)).map(|(_, v)| v.clone()).unwrap_or_default())
                .collect();
            cells.extend(self.columns.iter().map(|c| row.values.get(&c.name).map(ParamValue::cell).unwrap_or_default()));
            out.push_str(&csv_line(&cells));
        }
        out
    }

    /// A table from CSV: `when <attribute>` columns filter, the rest are values, `(unit)` after a name its unit. A column
    /// any cell of which isn't a number holds words. Blank cells give nothing.
    pub fn from_csv(text: &str) -> Result<Self, String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header = csv_cells(lines.next().ok_or("the CSV is empty")?);
        enum Col {
            Filter(String),
            Value(usize),
        }
        let mut table = ParameterTable::default();
        let mut cols = Vec::new();
        for h in &header {
            let h = h.trim();
            if let Some(f) = h.strip_prefix("when ").or_else(|| h.strip_prefix("When ")).or_else(|| h.strip_prefix("WHEN ")) {
                table.filters.push(f.trim().to_string());
                cols.push(Col::Filter(f.trim().to_string()));
            } else {
                let (name, unit) = match (h.rfind('('), h.ends_with(')')) {
                    (Some(open), true) => (h[..open].trim(), h[open + 1..h.len() - 1].trim()),
                    _ => (h, ""),
                };
                table.columns.push(ValueColumn::number(name, unit));
                cols.push(Col::Value(table.columns.len() - 1));
            }
        }
        let rows: Vec<Vec<String>> = lines.map(csv_cells).collect();
        for (c, col) in cols.iter().enumerate() {
            if let Col::Value(v) = col {
                let words = rows.iter().filter_map(|r| r.get(c)).map(|s| s.trim()).any(|s| !s.is_empty() && s.parse::<f64>().is_err());
                table.columns[*v].text = words;
            }
        }
        for (i, cells) in rows.iter().enumerate() {
            if cells.len() > cols.len() {
                return Err(format!("row {} has {} cells for {} columns", i + 1, cells.len(), cols.len()));
            }
            let mut row = ParameterRow::default();
            for (cell, col) in cells.iter().zip(&cols) {
                let cell = cell.trim();
                if cell.is_empty() {
                    continue;
                }
                match col {
                    Col::Filter(f) => {
                        row.when.insert(f.clone(), cell.to_string());
                    }
                    Col::Value(v) => {
                        let column = &table.columns[*v];
                        let value = if column.text { ParamValue::Text(cell.to_string()) } else { ParamValue::Number(cell.parse().expect("checked a number")) };
                        row.values.insert(column.name.clone(), value);
                    }
                }
            }
            table.rows.push(row);
        }
        table.check()?;
        Ok(table)
    }
}

/// One CSV line's cells: commas split them, double quotes hold commas, `""` in quotes is a quote.
fn csv_cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                cells.last_mut().expect("a cell").push('"');
            }
            ('"', _) => quoted = !quoted,
            (',', false) => cells.push(String::new()),
            _ => cells.last_mut().expect("a cell").push(c),
        }
    }
    cells
}

fn csv_line(cells: &[String]) -> String {
    let quoted: Vec<String> = cells.iter().map(|c| if c.contains([',', '"', '\n']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.clone() }).collect();
    format!("{}\n", quoted.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AttrValue;

    fn attrs(pairs: &[(&str, AttrValue)]) -> Attributes {
        let mut a = Attributes::new();
        for (k, v) in pairs {
            a.insert(k.to_string(), v.clone());
        }
        a
    }

    const CSV: &str = "when seam,when strip,ash_cutoff (%),waste_rd (t/m3),loader\nU,1-5,30,,\n\"U, M\",,,2.4,\n,,35,2.3,EX3600\n";

    #[test]
    fn a_value_comes_from_the_first_row_that_fits_and_holds_it() {
        let t = ParameterTable::from_csv(CSV).unwrap();
        assert_eq!(t.filters, ["seam", "strip"]);
        assert_eq!(t.columns.iter().map(|c| (c.name.as_str(), c.unit.as_str(), c.text)).collect::<Vec<_>>(), [("ash_cutoff", "%", false), ("waste_rd", "t/m3", false), ("loader", "", true)]);
        let u3 = attrs(&[("seam", AttrValue::Text("U".into())), ("strip", AttrValue::Number(3.0))]);
        let u7 = attrs(&[("seam", AttrValue::Text("u".into())), ("strip", AttrValue::Number(7.0))]);
        let l = attrs(&[("seam", AttrValue::Text("L".into())), ("strip", AttrValue::Number(3.0))]);
        assert_eq!(t.lookup(&u3, "ash_cutoff"), Some((0, &ParamValue::Number(30.0))));
        assert_eq!(t.lookup(&u3, "waste_rd"), Some((1, &ParamValue::Number(2.4))), "row 1 gives no waste RD, so row 2 does");
        assert_eq!(t.lookup(&u7, "ash_cutoff"), Some((2, &ParamValue::Number(35.0))));
        assert_eq!(t.lookup(&l, "WASTE_RD"), Some((2, &ParamValue::Number(2.3))));
        assert_eq!(t.lookup(&l, "loader"), Some((2, &ParamValue::Text("EX3600".into()))));
        assert_eq!(t.lookup(&l, "min_parting"), None);
    }

    #[test]
    fn a_case_s_own_rows_are_read_first() {
        let base = ParameterTable::from_csv(CSV).unwrap();
        let case = ParameterTable::from_csv("when seam,ash_cutoff (%)\nU,28\n").unwrap();
        let u3 = attrs(&[("seam", AttrValue::Text("U".into())), ("strip", AttrValue::Number(3.0))]);
        let found = ParameterTable::lookup_in(&[&case, &base], &u3, "ash_cutoff").unwrap();
        assert_eq!((found.table, found.row, found.value), (0, 0, &ParamValue::Number(28.0)));
        let found = ParameterTable::lookup_in(&[&case, &base], &u3, "waste_rd").unwrap();
        assert_eq!((found.table, found.row), (1, 1));
    }

    #[test]
    fn csv_out_reads_back_the_same() {
        let t = ParameterTable::from_csv(CSV).unwrap();
        assert_eq!(ParameterTable::from_csv(&t.to_csv()).unwrap(), t);
        assert!(t.to_csv().starts_with("when seam,when strip,ash_cutoff (%),waste_rd (t/m3),loader\n"));
        assert!(t.to_csv().contains("\"U, M\""));
    }

    #[test]
    fn a_table_that_doesn_t_hold_together_is_refused() {
        assert!(ParameterTable::from_csv("when seam,seam\nU,1\n").unwrap_err().contains("named twice"));
        let mut t = ParameterTable::from_csv(CSV).unwrap();
        t.rows[0].values.insert("ash_cutoff".into(), ParamValue::Text("high".into()));
        assert!(t.check().unwrap_err().contains("holds numbers"));
        let mut t = ParameterTable::from_csv(CSV).unwrap();
        t.rows[0].when.insert("pit".into(), "A".into());
        assert!(t.check().unwrap_err().contains("isn't one of the table's filters"));
        assert!(ParameterTable::from_csv("a,b\n1,2,3\n").unwrap_err().contains("3 cells"));
    }
}
