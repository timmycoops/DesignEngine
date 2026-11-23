//! `table.import`: a CSV read into a table of the project (the Build Plan's "Tables and scripts plan", decision 12), so a
//! Deswik or Vulcan reserves export, or blocks with a site's own attribute names, can be run through a dataflow by a
//! mapping. Its first line names the columns (a unit in brackets, `Vol (m3)`); a column whose every filled cell is a
//! number is numbers, the rest words. The columns are entered, as a person brings them in: no run wrote them. A column
//! in cubic metres or tonnes adds up by summing; the others have no rule until a script gives one.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Body, Change, Table, TableColumn};
use zd_reserves::AddUp;

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct ImportTable;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportTableParams {
    /// Its name: the file's own, without `.csv`, where none is given.
    #[serde(default)]
    pub name: String,
    /// The file's text; or `path`, the file read.
    #[serde(default)]
    pub csv: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default = "tables_layer")]
    pub layer: String,
}

fn tables_layer() -> String {
    super::TABLES_LAYER.into()
}

/// The file as a table: a column a header, numbers where every filled cell is one.
pub fn table_of_csv(csv: &str) -> Result<Table, String> {
    let file = super::update_records::read_file(csv)?;
    if file.rows.is_empty() {
        return Err("the file has a header and no rows".into());
    }
    let mut columns = Vec::with_capacity(file.names.len());
    for (k, name) in file.names.iter().enumerate() {
        let cells: Vec<&str> = file.rows.iter().map(|r| r[k].as_str()).collect();
        let numbers: Option<Vec<Option<f64>>> = cells
            .iter()
            .map(|c| match c.trim() {
                "" => Some(None),
                t => t.parse::<f64>().ok().filter(|x| x.is_finite()).map(Some),
            })
            .collect();
        let unit = file.units[k].clone();
        let column = match numbers {
            Some(values) if values.iter().any(Option::is_some) => {
                let sums = matches!(unit.to_ascii_lowercase().as_str(), "m3" | "m³" | "bcm" | "t" | "tonnes" | "lcm");
                if sums { TableColumn::adding(name, &unit, AddUp::Sum, values) } else { TableColumn::number(name, &unit, values) }
            }
            _ => TableColumn::text(name, cells.iter().map(|c| Some(c.trim().to_string()).filter(|w| !w.is_empty())).collect()),
        };
        columns.push(column.entered());
    }
    let table = Table { columns, ..Table::default() };
    table.check()?;
    Ok(table)
}

impl Command for ImportTable {
    type Params = ImportTableParams;
    const NAME: &'static str = "table.import";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Read a CSV into a table of the project: a reserves export, or blocks with a site's own names, to run a dataflow on by a mapping",
            inputs: vec![],
            params: vec![
                Field::new("name", FieldType::Text, "The table's name"),
                Field::new("csv", FieldType::Text, "The file's text: a header line naming the columns, then a row a line"),
                Field::new("path", FieldType::Text, "Or the CSV file to read"),
                Field::new("layer", FieldType::Text, "Its layer"),
            ],
            outputs: vec![Field::object("table", zd_core::ObjectKind::Table, "The table")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let read;
        let (csv, stem) = match &p.path {
            Some(path) => {
                read = std::fs::read_to_string(path).map_err(|e| CommandError::Invalid(format!("{path} can't be read: {e}")))?;
                (read.as_str(), std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())
            }
            None => (p.csv.as_str(), String::new()),
        };
        let name = if p.name.trim().is_empty() { stem.trim() } else { p.name.trim() };
        if name.is_empty() {
            return Err(CommandError::Invalid("the table needs a name".into()));
        }
        let table = table_of_csv(csv).map_err(CommandError::Invalid)?;
        let (rows, columns) = (table.rows(), table.columns.len());
        let numbers = table.columns.iter().filter(|c| matches!(c.values, zd_core::ColumnValues::Number(_))).count();
        let change = Change::Create { name: name.to_string(), layer: p.layer.clone(), attributes: Default::default(), body: Body::Table(Arc::new(table)) };
        Ok(Outcome::new(vec![change]).message(format!("{name}: {rows} rows, {columns} columns ({numbers} of numbers, {} of words)", columns - numbers)))
    }
}

#[cfg(test)]
mod tests {
    use super::table_of_csv;
    use zd_core::ColumnValues;

    #[test]
    fn a_csv_s_columns_are_numbers_where_every_filled_cell_is_one() {
        let t = table_of_csv("Strip,Blk,Seam_Name,Vol (m3),RD_ad\n3,B5,L,10402,1.43\n3,B5,M-L,51246,\n").unwrap();
        assert_eq!(t.rows(), 2);
        assert!(matches!(t.column("Strip").unwrap().values, ColumnValues::Number(_)));
        assert!(matches!(t.column("Blk").unwrap().values, ColumnValues::Text(_)));
        let vol = t.column("Vol").unwrap();
        assert_eq!((vol.unit.as_str(), vol.add_up.is_some()), ("m3", true), "cubic metres sum");
        assert!(t.column("RD_ad").unwrap().add_up.is_none(), "no rule until a script gives one");
        assert!(table_of_csv("a,b\n").is_err(), "a header and no rows");
    }
}
