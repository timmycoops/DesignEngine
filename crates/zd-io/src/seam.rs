//! Seam stack definitions as CSV, and grids read in place with their fingerprints (decision 0007).
//!
//! A definition is one row per horizon, top of the stack first, so it can be written by
//! `seam.suggest_definition` and edited in Excel:
//!
//! ```text
//! # comment lines start with #
//! folder,D:\models\pit\grids
//! horizon,parent,material,working,roof,floor,as,rd,ts
//! upper,,coal,yes,pitupper.srg,pitupper.sfg,pitupper.asg,pitupper.rdg,pitupper.tsg
//! ```
//!
//! `horizon`, `roof` and `floor` are required. `parent` names the horizon a split comes from;
//! `material` defaults to coal; `working` says whether the horizon is stacked for solids and reserves
//! (blank: yes for a horizon with no daughters, no for a parent). Every other column is a grid
//! variable, and each cell names that horizon's file, or is blank. Files are relative to `folder`;
//! a relative folder, or none, is relative to the definition's own folder.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use zd_core::{Fingerprint, Grid};

use crate::{IoError, XyzOptions, parse_vulcan_grid, parse_xyz_grid, vulcan_grid_name};

/// Columns with a fixed meaning; any other column is a grid variable.
const FIXED: [&str; 6] = ["horizon", "parent", "material", "working", "roof", "floor"];

#[derive(Debug, Clone, PartialEq)]
pub struct SeamDefinition {
    /// The folder grid files are relative to.
    pub folder: PathBuf,
    /// Top of the stack first.
    pub horizons: Vec<DefinedHorizon>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefinedHorizon {
    pub code: String,
    pub parent: Option<String>,
    pub material: String,
    pub working: bool,
    pub roof: String,
    pub floor: String,
    /// Other grid files by variable code.
    pub grids: BTreeMap<String, String>,
}

impl SeamDefinition {
    /// Where a file the definition names is.
    pub fn path(&self, file: &str) -> PathBuf {
        let file = Path::new(file);
        if file.is_absolute() { file.to_path_buf() } else { self.folder.join(file) }
    }
}

pub fn read_seam_definition(path: &Path) -> Result<SeamDefinition, IoError> {
    let bytes = std::fs::read(path)?;
    let base = path.parent().unwrap_or(Path::new(""));
    parse_seam_definition(&decode_text(&bytes), base)
}

/// Excel saves CSV as UTF-8 (with a byte-order mark) or in the Windows code page; the latter is
/// read as Latin-1, which agrees with it for every letter in file names.
fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Reads a definition, listing every problem found rather than stopping at the first.
pub fn parse_seam_definition(text: &str, base: &Path) -> Result<SeamDefinition, IoError> {
    let mut problems = Vec::new();
    let mut folder: Option<String> = None;
    let mut header: Option<Vec<String>> = None;
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let mut fields = match csv_fields(raw) {
            Ok(fields) => fields,
            Err(e) => {
                problems.push(format!("line {line}: {e}"));
                continue;
            }
        };
        while fields.last().is_some_and(String::is_empty) {
            fields.pop();
        }
        if fields.is_empty() || fields[0].starts_with('#') {
            continue;
        }
        if header.is_some() {
            rows.push((line, fields));
        } else if fields[0].eq_ignore_ascii_case("folder") {
            folder = fields.get(1).cloned();
        } else {
            header = Some(fields.iter().map(|f| f.to_ascii_lowercase()).collect());
        }
    }
    let Some(header) = header else {
        return Err(IoError::Definition(vec!["no header row (horizon, roof, floor, ...)".into()]));
    };
    let mut column: HashMap<&str, usize> = HashMap::new();
    for (i, name) in header.iter().enumerate() {
        if name.is_empty() {
            problems.push(format!("column {} has no name", i + 1));
        } else if column.insert(name.as_str(), i).is_some() {
            problems.push(format!("column '{name}' appears twice"));
        }
    }
    for needed in ["horizon", "roof", "floor"] {
        if !column.contains_key(needed) {
            problems.push(format!("no '{needed}' column"));
        }
    }
    if !problems.is_empty() {
        return Err(IoError::Definition(problems));
    }

    let cell = |fields: &[String], name: &str| column.get(name).and_then(|&i| fields.get(i)).cloned().unwrap_or_default();
    let variables: Vec<(&str, usize)> = header
        .iter()
        .enumerate()
        .filter(|(_, name)| !FIXED.contains(&name.as_str()))
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let mut horizons = Vec::new();
    let mut working: Vec<Option<bool>> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (line, fields) in &rows {
        if fields.len() > header.len() {
            problems.push(format!("line {line}: {} cells but only {} columns", fields.len(), header.len()));
        }
        let code = cell(fields, "horizon");
        if code.is_empty() {
            problems.push(format!("line {line}: no horizon code"));
            continue;
        }
        if let Some(first) = seen.insert(code.to_ascii_lowercase(), *line) {
            problems.push(format!("line {line}: horizon '{code}' is already on line {first}"));
        }
        let (roof, floor) = (cell(fields, "roof"), cell(fields, "floor"));
        for (what, file) in [("roof", &roof), ("floor", &floor)] {
            if file.is_empty() {
                problems.push(format!("line {line}: horizon '{code}' has no {what} file"));
            }
        }
        working.push(match cell(fields, "working").to_ascii_lowercase().as_str() {
            "" => None,
            "yes" | "y" | "true" | "1" => Some(true),
            "no" | "n" | "false" | "0" => Some(false),
            other => {
                problems.push(format!("line {line}: working must be yes or no, not '{other}'"));
                None
            }
        });
        let parent = Some(cell(fields, "parent")).filter(|p| !p.is_empty());
        let material = Some(cell(fields, "material")).filter(|m| !m.is_empty()).unwrap_or_else(|| "coal".into());
        let grids = variables
            .iter()
            .filter_map(|&(name, i)| fields.get(i).filter(|f| !f.is_empty()).map(|f| (name.to_string(), f.clone())))
            .collect();
        horizons.push(DefinedHorizon { code, parent, material, working: false, roof, floor, grids });
    }
    if horizons.is_empty() && problems.is_empty() {
        problems.push("no horizons listed".into());
    }

    // Parents must be horizons of the definition, and following them must never loop.
    let index: HashMap<String, usize> = horizons.iter().enumerate().map(|(i, h)| (h.code.to_ascii_lowercase(), i)).collect();
    let parent_of = |h: &DefinedHorizon| h.parent.as_ref().and_then(|p| index.get(&p.to_ascii_lowercase()).copied());
    for h in &horizons {
        if let Some(p) = &h.parent
            && parent_of(h).is_none()
        {
            problems.push(format!("horizon '{}' names parent '{p}', which is not in the definition", h.code));
        }
    }
    for (start, h) in horizons.iter().enumerate() {
        let mut at = parent_of(h);
        for _ in 0..horizons.len() {
            match at {
                Some(p) if p == start => {
                    problems.push(format!("the parents of horizon '{}' loop back to it", h.code));
                    break;
                }
                Some(p) => at = parent_of(&horizons[p]),
                None => break,
            }
        }
    }
    if !problems.is_empty() {
        return Err(IoError::Definition(problems));
    }

    let parents: BTreeSet<usize> = horizons.iter().filter_map(parent_of).collect();
    for (i, h) in horizons.iter_mut().enumerate() {
        h.working = working[i].unwrap_or(!parents.contains(&i));
    }
    let folder = match folder.filter(|f| !f.is_empty()) {
        Some(f) if Path::new(&f).is_absolute() => PathBuf::from(f),
        Some(f) => base.join(f),
        None => base.to_path_buf(),
    };
    Ok(SeamDefinition { folder, horizons })
}

/// Splits one CSV line: commas separate fields, a field may be wrapped in double quotes, and `""`
/// inside quotes is one quote. Unquoted fields are trimmed.
fn csv_fields(line: &str) -> Result<Vec<String>, &'static str> {
    let mut fields = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
        let mut field = String::new();
        if chars.next_if_eq(&'"').is_some() {
            loop {
                match chars.next() {
                    None => return Err("a quoted cell is not closed"),
                    Some('"') if chars.next_if_eq(&'"').is_some() => field.push('"'),
                    Some('"') => break,
                    Some(c) => field.push(c),
                }
            }
            while let Some(c) = chars.next_if(|c| *c != ',') {
                if !c.is_whitespace() {
                    return Err("text after the closing quote of a cell");
                }
            }
        } else {
            while let Some(c) = chars.next_if(|c| *c != ',') {
                field.push(c);
            }
            field.truncate(field.trim_end().len());
        }
        fields.push(field);
        if chars.next().is_none() {
            return Ok(fields);
        }
    }
}

/// The definition as CSV text, ready for Excel: a byte-order mark so Excel reads it as UTF-8, a
/// comment saying how to edit it, the folder, the header and one row per horizon.
pub fn format_seam_definition(definition: &SeamDefinition) -> String {
    let variables: BTreeSet<&str> = definition.horizons.iter().flat_map(|h| h.grids.keys().map(String::as_str)).collect();
    let mut out = String::from("\u{feff}");
    for note in [
        "# Seam stack definition: one row per horizon and top of the stack first.",
        "# parent = the horizon a split comes from. working = yes stacks the horizon for solids and reserves.",
        "# Columns after floor are grid variables: each cell names that horizon's file in the folder.",
    ] {
        out.push_str(note);
        out.push_str("\r\n");
    }
    let row = |cells: Vec<&str>| cells.into_iter().map(csv_cell).collect::<Vec<_>>().join(",") + "\r\n";
    out.push_str(&row(vec!["folder", &definition.folder.to_string_lossy()]));
    out.push_str(&row(FIXED.into_iter().chain(variables.iter().copied()).collect()));
    for h in &definition.horizons {
        let mut cells = vec![h.code.as_str(), h.parent.as_deref().unwrap_or(""), &h.material, if h.working { "yes" } else { "no" }, &h.roof, &h.floor];
        cells.extend(variables.iter().map(|v| h.grids.get(*v).map_or("", String::as_str)));
        out.push_str(&row(cells));
    }
    out
}

pub fn write_seam_definition(path: &Path, definition: &SeamDefinition) -> Result<(), IoError> {
    Ok(std::fs::write(path, format_seam_definition(definition))?)
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) || text.trim() != text {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// A grid read in place, with the fingerprint of the file it came from.
#[derive(Debug, Clone)]
pub struct LinkedGrid {
    pub grid: Grid,
    pub fingerprint: Fingerprint,
}

/// Reads a grid file without copying it anywhere: Vulcan grids (`.<variable>g`, plain or
/// compressed) and XYZ/CSV text grids (`.xyz`, `.csv`, `.txt`, `.asc`).
pub fn read_linked_grid(path: &Path) -> Result<LinkedGrid, IoError> {
    let bytes = std::fs::read(path)?;
    let fingerprint = Fingerprint { bytes: bytes.len() as u64, modified_unix_ms: modified_ms(path)?, checksum: fnv1a(&bytes) };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let grid = match ext.as_str() {
        "xyz" | "csv" | "txt" | "asc" => parse_xyz_grid(&bytes, &XyzOptions::default())?.grid,
        _ if vulcan_grid_name(&name).is_some() => parse_vulcan_grid(&bytes)?.grid,
        _ => return Err(IoError::Unsupported(format!("no grid reader for '{name}'"))),
    };
    Ok(LinkedGrid { grid, fingerprint })
}

/// The fingerprint of a file as it is now, to compare with the one a model recorded.
pub fn fingerprint_file(path: &Path) -> Result<Fingerprint, IoError> {
    let bytes = std::fs::read(path)?;
    Ok(Fingerprint { bytes: bytes.len() as u64, modified_unix_ms: modified_ms(path)?, checksum: fnv1a(&bytes) })
}

fn modified_ms(path: &Path) -> Result<u64, IoError> {
    let modified = std::fs::metadata(path)?.modified()?;
    Ok(modified.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64))
}

/// 64-bit FNV-1a: enough to tell whether a file has changed, and no new dependency.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<SeamDefinition, IoError> {
        parse_seam_definition(text, Path::new("defs"))
    }

    fn problems(text: &str) -> Vec<String> {
        match parse(text) {
            Err(IoError::Definition(p)) => p,
            other => panic!("expected definition problems, got {other:?}"),
        }
    }

    const SPLIT: &str = "# a seam split in two\n\
        folder,grids\n\
        Horizon,Parent,Material,Working,Roof,Floor,RD,ts\n\
        a,,coal,,pita.srg,pita.sfg,pita.rdg,\n\
        au,a,coal,,pitau.srg,pitau.sfg,pitau.rdg,pitau.tsg\n\
        al,a,,no,pital.srg,pital.sfg,,\n";

    #[test]
    fn a_definition_reads_with_defaults() {
        let d = parse(SPLIT).unwrap();
        assert_eq!(d.folder, Path::new("defs").join("grids"));
        let codes: Vec<&str> = d.horizons.iter().map(|h| h.code.as_str()).collect();
        assert_eq!(codes, ["a", "au", "al"]);
        // Blank working: no for the parent, yes for a daughter; al says no.
        assert_eq!(d.horizons.iter().map(|h| h.working).collect::<Vec<_>>(), [false, true, false]);
        assert_eq!(d.horizons[2].material, "coal");
        assert_eq!(d.horizons[1].parent.as_deref(), Some("a"));
        assert_eq!(d.horizons[1].grids.keys().collect::<Vec<_>>(), ["rd", "ts"], "variable names lower-cased");
        assert!(d.horizons[2].grids.is_empty());
        assert_eq!(d.path("pita.srg"), Path::new("defs").join("grids").join("pita.srg"));
    }

    #[test]
    fn every_problem_is_listed_at_once() {
        let text = "horizon,parent,working,roof,floor\n\
            a,b,maybe,a.srg,\n\
            A,,yes,a2.srg,a2.sfg\n\
            ,,,x.srg,x.sfg\n";
        let p = problems(text);
        assert_eq!(
            p,
            [
                "line 2: horizon 'a' has no floor file",
                "line 2: working must be yes or no, not 'maybe'",
                "line 3: horizon 'A' is already on line 2",
                "line 4: no horizon code",
                "horizon 'a' names parent 'b', which is not in the definition",
            ]
        );
    }

    #[test]
    fn parents_that_loop_are_refused() {
        let text = "horizon,parent,roof,floor\nx,y,x.srg,x.sfg\ny,x,y.srg,y.sfg\nz,z,z.srg,z.sfg\n";
        let p = problems(text);
        assert_eq!(p.len(), 3, "{p:?}");
        assert!(p.iter().all(|m| m.contains("loop back")));
    }

    #[test]
    fn missing_columns_are_named() {
        assert_eq!(problems("horizon,roof\na,a.srg\n"), ["no 'floor' column"]);
        assert_eq!(problems("# nothing but a comment\n"), ["no header row (horizon, roof, floor, ...)"]);
    }

    #[test]
    fn excel_quoting_and_trailing_cells_read() {
        // Excel pads short rows with commas and quotes cells holding commas.
        let text = "folder,\"old, grids\",,,\r\nhorizon,roof,floor,,\r\n\"x \"\"main\"\"\",\"x,1.srg\", x.sfg ,,\r\n,,,,\r\n";
        let d = parse(text).unwrap();
        assert_eq!(d.folder, Path::new("defs").join("old, grids"));
        assert_eq!(d.horizons.len(), 1, "the row of empty cells is skipped");
        assert_eq!((d.horizons[0].code.as_str(), d.horizons[0].roof.as_str(), d.horizons[0].floor.as_str()), ("x \"main\"", "x,1.srg", "x.sfg"));
        assert!(matches!(parse("horizon,roof,floor\n\"a\"b,x,y\n"), Err(IoError::Definition(_))));
        assert!(matches!(parse("horizon,roof,floor\n\"a,x,y\n"), Err(IoError::Definition(_))));
    }

    #[test]
    fn written_definitions_read_back_the_same() {
        let mut d = parse(SPLIT).unwrap();
        d.folder = PathBuf::from("grids, older");
        d.horizons[2].material = "coal, \"dull\"".into();
        let back = parse(&format_seam_definition(&d)).unwrap();
        assert_eq!(back.horizons, d.horizons);
        assert_eq!(back.folder, Path::new("defs").join("grids, older"));
    }

    #[test]
    fn a_latin1_definition_reads() {
        let mut bytes = b"horizon,roof,floor\nn\xe9,a.srg,a.sfg\n".to_vec();
        let text = decode_text(&bytes);
        assert_eq!(parse(&text).unwrap().horizons[0].code, "n\u{e9}");
        bytes.splice(0..0, *b"\xef\xbb\xbf");
        assert!(decode_text(&bytes).starts_with("horizon"));
    }

    #[test]
    fn linked_grids_read_in_place_with_a_fingerprint() {
        let dir = std::env::temp_dir().join(format!("zd-linked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let values: Vec<Option<f32>> = (0..12).map(|i| if i == 5 { None } else { Some(100.0 + i as f32) }).collect();
        let bytes = crate::vulcan::tests::plain_grid("PITA.SRG", [500_000.0, 7_000_000.0], 10.0, 4, 3, &values, 0);
        let path = dir.join("pita.srg");
        std::fs::write(&path, &bytes).unwrap();
        let linked = read_linked_grid(&path).unwrap();
        assert_eq!((linked.grid.cols, linked.grid.rows, linked.grid.z[1]), (4, 3, 101.0));
        assert!(linked.grid.z[5].is_nan());
        assert_eq!(linked.fingerprint.bytes, bytes.len() as u64);
        assert_eq!(fingerprint_file(&path).unwrap(), linked.fingerprint);

        // Changing one byte of a node value changes the checksum.
        let mut changed = bytes.clone();
        changed[88 + 3] ^= 1;
        std::fs::write(&path, &changed).unwrap();
        assert_ne!(fingerprint_file(&path).unwrap().checksum, linked.fingerprint.checksum);

        let other = dir.join("notes.doc");
        std::fs::write(&other, b"x").unwrap();
        assert!(matches!(read_linked_grid(&other), Err(IoError::Unsupported(_))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fnv1a_matches_the_published_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }
}
