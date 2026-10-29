//! Strings as CSV (decision 0009): one row per vertex, so they can be edited in Excel and passed
//! to packages that read point lists.
//!
//! ```text
//! string,name,layer,closed,x,y,z
//! 1,toe,design,no,650000.5,7450000.25,80
//! 1,toe,design,no,650100.5,7450000.25,81
//! ```
//!
//! The header names the columns, in any order and any case. `x` (or `easting`, `east`), `y`
//! (`northing`, `north`) and `z` (`rl`, `elevation`) are required. A new string starts where the
//! `string` value changes; without a `string` column, a blank row ends a string. `name`, `layer`
//! and `closed` are read from a string's first row. A string whose last row repeats its first is
//! read as closed and the repeat dropped, unless `closed` says no. Other columns are listed in the
//! log and not read.

use std::collections::HashMap;
use std::path::Path;

use zd_core::Polyline;

use crate::IoError;
use crate::text::{csv_cell, csv_fields, decode_text};

#[derive(Debug, Clone, PartialEq)]
pub struct CsvString {
    pub name: Option<String>,
    pub layer: Option<String>,
    pub line: Polyline,
}

#[derive(Debug, Clone)]
pub struct CsvStrings {
    pub strings: Vec<CsvString>,
    pub log: CsvLog,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CsvLog {
    pub rows_read: usize,
    pub strings_read: usize,
    /// Strings whose last row repeated the first: read as closed, the repeat dropped.
    pub closing_repeats_dropped: usize,
    /// Strings with fewer than two vertices, passed over.
    pub short_strings: usize,
    /// Closed strings with only two vertices, read as open.
    pub closed_too_short: usize,
    /// Header columns the reader does not use.
    pub ignored_columns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Column {
    X,
    Y,
    Z,
    String,
    Name,
    Layer,
    Closed,
}

fn column(heading: &str) -> Option<Column> {
    Some(match heading.trim().to_ascii_lowercase().as_str() {
        "x" | "easting" | "east" => Column::X,
        "y" | "northing" | "north" => Column::Y,
        "z" | "rl" | "elevation" => Column::Z,
        "string" => Column::String,
        "name" => Column::Name,
        "layer" => Column::Layer,
        "closed" => Column::Closed,
        _ => return None,
    })
}

pub fn read_csv_strings(path: &Path) -> Result<CsvStrings, IoError> {
    parse_csv_strings(&decode_text(&std::fs::read(path)?))
}

pub fn parse_csv_strings(text: &str) -> Result<CsvStrings, IoError> {
    let mut log = CsvLog::default();
    let mut header: Option<HashMap<Column, usize>> = None;
    let mut strings = Vec::new();
    let mut current: Option<Building> = None;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.trim_start().starts_with('#') {
            continue;
        }
        let fields = csv_fields(raw).map_err(|e| IoError::Format { line, message: e.into() })?;
        let blank = fields.iter().all(|f| f.is_empty());
        let Some(columns) = &header else {
            if !blank {
                header = Some(read_header(&fields, line, &mut log)?);
            }
            continue;
        };
        if blank {
            if !columns.contains_key(&Column::String) {
                finish(current.take(), &mut strings, &mut log);
            }
            continue;
        }

        let cell = |c: Column| columns.get(&c).map(|&i| fields.get(i).map_or("", String::as_str));
        let coordinate = |c: Column, heading: &str| -> Result<f64, IoError> {
            let text = cell(c).unwrap_or("");
            text.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| IoError::Format {
                line,
                message: format!("{heading} '{text}' is not a number"),
            })
        };
        let point = [coordinate(Column::X, "x")?, coordinate(Column::Y, "y")?, coordinate(Column::Z, "z")?];
        log.rows_read += 1;

        let key = cell(Column::String).map(str::to_string);
        if current.as_ref().is_some_and(|b| b.key != key) {
            finish(current.take(), &mut strings, &mut log);
        }
        let building = match &mut current {
            Some(building) => building,
            None => current.insert(Building {
                key,
                name: cell(Column::Name).filter(|s| !s.is_empty()).map(str::to_string),
                layer: cell(Column::Layer).filter(|s| !s.is_empty()).map(str::to_string),
                closed: closed_cell(cell(Column::Closed).unwrap_or(""), line)?,
                vertices: Vec::new(),
            }),
        };
        building.vertices.push(point);
    }
    if header.is_none() {
        return Err(IoError::Format { line: 1, message: "no header row (x, y and z columns)".into() });
    }
    finish(current, &mut strings, &mut log);
    Ok(CsvStrings { strings, log })
}

fn read_header(fields: &[String], line: usize, log: &mut CsvLog) -> Result<HashMap<Column, usize>, IoError> {
    let mut columns = HashMap::new();
    for (i, heading) in fields.iter().enumerate() {
        match column(heading) {
            Some(c) if columns.insert(c, i).is_some() => {
                return Err(IoError::Format { line, message: format!("column '{heading}' appears twice") });
            }
            Some(_) => {}
            None if heading.is_empty() => {}
            None => log.ignored_columns.push(heading.clone()),
        }
    }
    let missing: Vec<&str> = [(Column::X, "x"), (Column::Y, "y"), (Column::Z, "z")]
        .into_iter()
        .filter(|(c, _)| !columns.contains_key(c))
        .map(|(_, name)| name)
        .collect();
    if !missing.is_empty() {
        return Err(IoError::Format {
            line,
            message: format!("the header has no {} column; found {}", missing.join(", "), fields.join(", ")),
        });
    }
    Ok(columns)
}

/// `Some(true)` or `Some(false)` when the cell says, `None` when blank.
fn closed_cell(text: &str, line: usize) -> Result<Option<bool>, IoError> {
    match text.trim().to_ascii_lowercase().as_str() {
        "" => Ok(None),
        "yes" | "y" | "true" | "1" => Ok(Some(true)),
        "no" | "n" | "false" | "0" => Ok(Some(false)),
        other => Err(IoError::Format { line, message: format!("closed '{other}' is not yes or no") }),
    }
}

struct Building {
    key: Option<String>,
    name: Option<String>,
    layer: Option<String>,
    closed: Option<bool>,
    vertices: Vec<[f64; 3]>,
}

fn finish(building: Option<Building>, strings: &mut Vec<CsvString>, log: &mut CsvLog) {
    let Some(Building { name, layer, closed, mut vertices, .. }) = building else { return };
    let repeats = vertices.len() >= 4 && vertices.first() == vertices.last();
    let closed = match closed {
        Some(false) => false,
        _ if repeats => {
            vertices.pop();
            log.closing_repeats_dropped += 1;
            true
        }
        said => said.unwrap_or(false),
    };
    if vertices.len() < 2 {
        log.short_strings += 1;
        return;
    }
    let closed = if closed && vertices.len() < 3 {
        log.closed_too_short += 1;
        false
    } else {
        closed
    };
    log.strings_read += 1;
    strings.push(CsvString { name, layer, line: Polyline { vertices, closed } });
}

/// Strings as CSV text: a byte-order mark so Excel reads it as UTF-8, the header
/// `string,name,layer,closed,x,y,z`, then one row per vertex, numbered by string. A closed string
/// does not repeat its first vertex. Coordinates use Rust's shortest round-trip formatting.
pub fn format_csv_strings(strings: &[(&str, &str, &Polyline)]) -> String {
    let mut out = String::from("\u{feff}string,name,layer,closed,x,y,z\r\n");
    for (number, (name, layer, line)) in strings.iter().enumerate() {
        let prefix = format!("{},{},{},{}", number + 1, csv_cell(name), csv_cell(layer), if line.closed { "yes" } else { "no" });
        for p in &line.vertices {
            out.push_str(&format!("{prefix},{},{},{}\r\n", p[0], p[1], p[2]));
        }
    }
    out
}

pub fn write_csv_strings(path: &Path, strings: &[(&str, &str, &Polyline)]) -> Result<(), IoError> {
    Ok(std::fs::write(path, format_csv_strings(strings))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips_bit_for_bit() {
        let toe = Polyline { vertices: vec![[650_000.1, 7_450_000.2, 100.3], [650_100.7, 7_450_000.9, 1.0 / 3.0]], closed: false };
        let pit = Polyline { vertices: vec![[1.0, 2.0, 3.0], [4.0, 2.0, 3.0], [4.0, 5.0, 3.0]], closed: true };
        let text = format_csv_strings(&[("toe, north", "design", &toe), ("pit \"A\"", "", &pit)]);
        assert!(text.starts_with("\u{feff}string,name,layer,closed,x,y,z\r\n1,\"toe, north\",design,no,650000.1,7450000.2,100.3\r\n"));
        let back = parse_csv_strings(&text).unwrap();
        assert_eq!(back.strings.len(), 2);
        assert_eq!(back.strings[0], CsvString { name: Some("toe, north".into()), layer: Some("design".into()), line: toe.clone() });
        assert_eq!(back.strings[1], CsvString { name: Some("pit \"A\"".into()), layer: None, line: pit });
        let bits = |l: &Polyline| l.vertices.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&back.strings[0].line), bits(&toe));
        assert_eq!(back.log, CsvLog { rows_read: 5, strings_read: 2, ..CsvLog::default() });
    }

    #[test]
    fn columns_by_name_in_any_order_and_blank_rows_between_strings() {
        let text = "# exported\nRL,Easting,Northing,Point\n10,1,2,a\n11,3,4,b\n\n12,5,6,c\n13,7,8,d\n12,5,6,e\n14,9,9,f\n12,5,6,g\n";
        let back = parse_csv_strings(text).unwrap();
        assert_eq!(back.log.ignored_columns, ["Point"]);
        assert_eq!(back.strings[0].line.vertices, [[1.0, 2.0, 10.0], [3.0, 4.0, 11.0]]);
        // The second string ends where it began: closed, and the repeat dropped.
        assert!(back.strings[1].line.closed);
        assert_eq!(back.strings[1].line.vertices.len(), 4);
        assert_eq!(back.log.closing_repeats_dropped, 1);
        assert_eq!(back.strings[1].name, None);
    }

    #[test]
    fn a_string_column_splits_strings_and_closed_no_keeps_a_repeat() {
        let text = "string,x,y,z,closed\n1,0,0,0,no\n1,1,0,0,\n1,1,1,0,\n1,0,0,0,\n2,5,5,5,\n\n2,6,6,6\n3,9,9,9\n";
        let back = parse_csv_strings(text).unwrap();
        assert_eq!(back.strings.len(), 2, "the one-vertex string is passed over");
        assert!(!back.strings[0].line.closed);
        assert_eq!(back.strings[0].line.vertices.len(), 4);
        assert_eq!(back.strings[1].line.vertices, [[5.0, 5.0, 5.0], [6.0, 6.0, 6.0]], "a blank row does not split a numbered string");
        assert_eq!(back.log.short_strings, 1);
    }

    #[test]
    fn problems_are_reported_with_their_line() {
        let missing = parse_csv_strings("easting,northing,elev\n1,2,3\n").unwrap_err();
        assert!(missing.to_string().contains("no z column"), "{missing}");
        let bad = parse_csv_strings("x,y,z\n1,2,3\n1,two,3\n").unwrap_err();
        assert!(matches!(bad, IoError::Format { line: 3, .. }), "{bad}");
        assert!(parse_csv_strings("x,y,z,closed\n1,2,3,maybe\n").is_err());
        assert!(parse_csv_strings("x,X,y,z\n").is_err(), "a column twice");
        assert!(parse_csv_strings("\n# nothing\n").is_err());
    }
}
