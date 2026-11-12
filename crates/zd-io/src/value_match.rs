//! The match file for setting blocks beside another package's numbers (decision 0061, workstream 4): which field of
//! theirs is which value of ours, and how near they must be. A CSV the user edits, kept on their machine:
//!
//! ```text
//! source,ours,tolerance
//! Volume,volume,0.1%
//! RD,rd,0.005
//! Ash,as,
//! ```
//!
//! `source` is the attribute the other package's value came in as (Spry's or Deswik's field); `ours` is the attribute the
//! interrogation wrote, or `volume` for the block's own volume. A tolerance ending in `%` is a share of their value; a plain
//! number is in the value's own unit; blank leaves it to the run.

use std::path::Path;

use crate::IoError;
use crate::text::csv_fields;

/// How near two values must be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tolerance {
    /// A share of their value: 0.001 is 0.1 %.
    Share(f64),
    /// In the value's own unit.
    Within(f64),
}

/// One row of a match file.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueMatch {
    pub source: String,
    pub ours: String,
    pub tolerance: Option<Tolerance>,
}

/// Reads a match file.
pub fn read_value_match(path: &Path) -> Result<Vec<ValueMatch>, IoError> {
    let text = std::fs::read_to_string(path)?;
    parse_value_match(&text).map_err(|problems| IoError::Definition(problems.into_iter().map(|p| format!("{}: {p}", path.display())).collect()))
}

/// A match file's rows, every problem listed by line.
pub fn parse_value_match(text: &str) -> Result<Vec<ValueMatch>, Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (mut rows, mut problems) = (Vec::new(), Vec::new());
    let mut columns: Option<[usize; 3]> = None;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let fields = match csv_fields(raw) {
            Ok(f) => f,
            Err(e) => {
                problems.push(format!("line {line}: {e}"));
                continue;
            }
        };
        if fields.iter().all(|f| f.trim().is_empty()) || fields[0].trim_start().starts_with('#') {
            continue;
        }
        let Some([s, o, t]) = columns else {
            let at = |name: &str| fields.iter().position(|f| f.trim().eq_ignore_ascii_case(name));
            match (at("source"), at("ours")) {
                (Some(s), Some(o)) => columns = Some([s, o, at("tolerance").unwrap_or(usize::MAX)]),
                _ => return Err(vec![format!("line {line}: the first row names the columns, and needs 'source' and 'ours'")]),
            }
            continue;
        };
        let cell = |k: usize| fields.get(k).map_or("", |f| f.trim());
        let (source, ours) = (cell(s), cell(o));
        if source.is_empty() || ours.is_empty() {
            problems.push(format!("line {line}: both 'source' and 'ours' are needed"));
            continue;
        }
        let tolerance = match cell(t) {
            "" => None,
            given => match read_tolerance(given) {
                Some(tolerance) => Some(tolerance),
                None => {
                    problems.push(format!("line {line}: '{given}' isn't a tolerance: a number, or a share ending in %"));
                    continue;
                }
            },
        };
        rows.push(ValueMatch { source: source.to_string(), ours: ours.to_string(), tolerance });
    }
    if columns.is_none() {
        return Err(vec!["no rows: the first row names the columns, 'source', 'ours' and 'tolerance'".into()]);
    }
    if problems.is_empty() { Ok(rows) } else { Err(problems) }
}

fn read_tolerance(text: &str) -> Option<Tolerance> {
    let number = |t: &str| t.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0);
    match text.strip_suffix('%') {
        Some(share) => number(share).map(|v| Tolerance::Share(v / 100.0)),
        None => number(text).map(Tolerance::Within),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_file_reads_its_rows_and_tolerances() {
        let text = "\u{feff}# which is which\nsource,ours,tolerance\nVolume,volume,0.1%\n\"Raw Ash, %\",as,0.05\nRD,rd,\n\n";
        assert_eq!(
            parse_value_match(text).unwrap(),
            vec![
                ValueMatch { source: "Volume".into(), ours: "volume".into(), tolerance: Some(Tolerance::Share(0.001)) },
                ValueMatch { source: "Raw Ash, %".into(), ours: "as".into(), tolerance: Some(Tolerance::Within(0.05)) },
                ValueMatch { source: "RD".into(), ours: "rd".into(), tolerance: None },
            ]
        );
        // Columns in any order, the tolerance column left out.
        assert_eq!(parse_value_match("ours,source\nas,Ash\n").unwrap()[0].source, "Ash");
    }

    #[test]
    fn what_a_match_file_gets_wrong_is_said_by_line() {
        assert!(parse_value_match("field,code\nAsh,as\n").unwrap_err()[0].contains("needs 'source' and 'ours'"));
        let e = parse_value_match("source,ours,tolerance\nAsh,,\nRD,rd,close\n").unwrap_err();
        assert!(e[0].starts_with("line 2: both") && e[1].starts_with("line 3: 'close' isn't a tolerance"), "{e:?}");
        assert!(parse_value_match("").is_err());
    }
}
