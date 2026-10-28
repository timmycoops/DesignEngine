//! Vulcan grid files, `<proj><seam>.<variable>g` (e.g. `pitupper.srg` roof, `.sfg` floor, `.rag` raw
//! ash): the plain binary layout.
//!
//! There is no public specification. The layout below was worked out from sample files and checked
//! (decision 0006); it stays marked unverified until a grid matches an independent export node for
//! node. Big-endian throughout:
//!
//! | Bytes   | Content                                                             |
//! |---------|---------------------------------------------------------------------|
//! | 0..32   | free text: name and timestamp, or how a derived grid was made       |
//! | 32..64  | f64 x0, x1, y0, y1: extents of the node lattice                     |
//! | 64..68  | i32 node count (cols × rows)                                        |
//! | 68..72  | i32, zero in every sample                                           |
//! | 72..80  | f32 dx, dy                                                          |
//! | 80..88  | i32 cols, rows                                                      |
//! | 88..    | per node: f32 value, i32 flag (1 = live; anything else is null)     |
//!
//! Nodes run row by row, x fastest, from the south-west corner. Files may end in zero padding.
//! Values are single precision and widen to f64 exactly. The compressed layout (`vulZ`) is not read
//! yet; export those grids to XYZ or ESRI ASCII instead.

use std::collections::BTreeMap;
use std::path::Path;

use zd_core::Grid;

use crate::IoError;

const HEADER: usize = 88;
const VULZ: &[u8; 4] = b"vulZ";

#[derive(Debug, Clone)]
pub struct VulcanGrid {
    /// Null nodes are NaN.
    pub grid: Grid,
    /// The free-text header (bytes 0..32), trimmed.
    pub description: String,
    pub log: VulcanLog,
}

/// What the reader found and set aside, so nothing is dropped silently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VulcanLog {
    pub nodes: usize,
    pub live: usize,
    pub nulls: usize,
    /// Null nodes that still hold a non-zero value, which was ignored.
    pub stale_nulls: usize,
    /// Flags other than 0 and 1, read as null.
    pub odd_flags: usize,
    /// Live nodes holding NaN or infinity, read as null.
    pub non_finite: usize,
    /// Zero bytes after the last node.
    pub padding_bytes: usize,
}

pub fn read_vulcan_grid(path: &Path) -> Result<VulcanGrid, IoError> {
    let bytes = std::fs::read(path)?;
    parse_vulcan_grid(&bytes).map_err(|e| match e {
        IoError::Binary(message) => IoError::Binary(format!("{}: {message}", path.display())),
        IoError::Unsupported(message) => IoError::Unsupported(format!("{}: {message}", path.display())),
        other => other,
    })
}

pub fn parse_vulcan_grid(bytes: &[u8]) -> Result<VulcanGrid, IoError> {
    let bad = |message: String| IoError::Binary(message);
    if bytes.len() >= 8 && &bytes[4..8] == VULZ {
        return Err(IoError::Unsupported(
            "compressed Vulcan grid (vulZ) is not read yet; export it as XYZ or ESRI ASCII".into(),
        ));
    }
    if bytes.len() < HEADER {
        return Err(bad(format!("{} bytes is too short for a Vulcan grid header", bytes.len())));
    }
    let f64_at = |o: usize| f64::from_be_bytes(bytes[o..o + 8].try_into().unwrap());
    let f32_at = |o: usize| f32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
    let i32_at = |o: usize| i32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());

    let description = bytes[..32].iter().map(|&b| b as char).collect::<String>().trim_matches(['\0', ' ']).to_string();
    let [x0, x1, y0, y1] = [32, 40, 48, 56].map(f64_at);
    let (count, dx, dy, cols, rows) = (i32_at(64), f32_at(72), f32_at(76), i32_at(80), i32_at(84));
    if cols < 2 || rows < 2 || i64::from(count) != i64::from(cols) * i64::from(rows) {
        return Err(bad(format!("header gives {count} nodes but a {cols} x {rows} lattice")));
    }
    if !(dx.is_finite() && dy.is_finite() && dx > 0.0 && dy > 0.0) || ![x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
        return Err(bad(format!("header cell size {dx} x {dy} or extents are not usable")));
    }
    let (dx, dy) = (f64::from(dx), f64::from(dy));
    let spans = [((x1 - x0) / dx + 1.0, cols), ((y1 - y0) / dy + 1.0, rows)];
    if spans.iter().any(|&(span, n)| (span - f64::from(n)).abs() > 1e-3) {
        return Err(bad(format!(
            "extents {x0}..{x1}, {y0}..{y1} at {dx} x {dy} m do not give {cols} x {rows} nodes"
        )));
    }
    let (cols, rows) = (cols as usize, rows as usize);
    let nodes = cols * rows;
    let end = HEADER + 8 * nodes;
    if bytes.len() < end {
        return Err(bad(format!("truncated: {nodes} nodes need {end} bytes, the file has {}", bytes.len())));
    }
    if bytes[end..].iter().any(|&b| b != 0) {
        return Err(bad(format!("{} bytes after the last node are not zero padding", bytes.len() - end)));
    }

    let mut log = VulcanLog { nodes, padding_bytes: bytes.len() - end, ..VulcanLog::default() };
    let mut z = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let value = f32_at(HEADER + 8 * i);
        let flag = i32_at(HEADER + 8 * i + 4);
        if flag == 1 && value.is_finite() {
            log.live += 1;
            z.push(f64::from(value));
            continue;
        }
        log.nulls += 1;
        match flag {
            1 => log.non_finite += 1,
            0 => {}
            _ => log.odd_flags += 1,
        }
        if flag != 1 && value != 0.0 {
            log.stale_nulls += 1;
        }
        z.push(f64::NAN);
    }
    Ok(VulcanGrid { grid: Grid::new([x0, y0], [dx, dy], cols, rows, z), description, log })
}

/// Splits a Vulcan grid file name into its stem (project prefix plus seam) and variable, lower case:
/// `pitupper.rag` gives `("pitupper", "ra")`. `None` unless the extension is letters or digits ending
/// in `g`.
pub fn vulcan_grid_name(file_name: &str) -> Option<(String, String)> {
    let (stem, ext) = file_name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    let variable = ext.strip_suffix('g')?;
    if stem.is_empty() || variable.is_empty() || variable.len() > 3 || !variable.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((stem.to_ascii_lowercase(), variable.to_string()))
}

/// Every grid of one seam: `<prefix><seam>.<variable>g`, matched case-insensitively on the whole
/// stem (so seam `upper` does not pick up `upperp15`).
#[derive(Debug, Clone)]
pub struct VulcanSeam {
    pub seam: String,
    /// By variable code (`sr`, `sf`, `tk`, `ra`, ...).
    pub grids: BTreeMap<String, VulcanGrid>,
    /// Variables left out, with the reason (e.g. a compressed file).
    pub skipped: Vec<(String, String)>,
}

impl VulcanSeam {
    pub fn roof(&self) -> &VulcanGrid {
        &self.grids["sr"]
    }

    pub fn floor(&self) -> &VulcanGrid {
        &self.grids["sf"]
    }
}

/// Reads a seam's grids from `dir`. The roof (`sr`) and floor (`sf`) must read; another variable that
/// cannot be read is skipped and listed. Every grid must share the roof's lattice.
pub fn read_vulcan_seam(dir: &Path, prefix: &str, seam: &str) -> Result<VulcanSeam, IoError> {
    let stem = format!("{prefix}{seam}").to_ascii_lowercase();
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some((s, variable)) = vulcan_grid_name(&name)
            && s == stem
            && entry.file_type()?.is_file()
        {
            files.push((variable, entry.path()));
        }
    }
    files.sort();

    let mut out = VulcanSeam { seam: seam.to_string(), grids: BTreeMap::new(), skipped: Vec::new() };
    for (variable, path) in files {
        match read_vulcan_grid(&path) {
            Ok(grid) => {
                out.grids.insert(variable, grid);
            }
            Err(e) if variable != "sr" && variable != "sf" => out.skipped.push((variable, e.to_string())),
            Err(e) => return Err(e),
        }
    }
    for needed in ["sr", "sf"] {
        if !out.grids.contains_key(needed) {
            return Err(IoError::Unsupported(format!("seam '{seam}' has no {needed} grid ({stem}.{needed}g) in {}", dir.display())));
        }
    }
    let lattice = out.roof().grid.clone();
    if let Some((variable, _)) = out.grids.iter().find(|(_, g)| !g.grid.same_lattice(&lattice)) {
        return Err(IoError::Binary(format!("{stem}.{variable}g is on a different lattice from {stem}.srg")));
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes the plain layout, for tests: `values` row by row from the south-west, `None` = null.
    pub fn plain_grid(description: &str, origin: [f64; 2], cell: f32, cols: usize, rows: usize, values: &[Option<f32>], padding: usize) -> Vec<u8> {
        let mut out = vec![b' '; 32];
        out[..description.len()].copy_from_slice(description.as_bytes());
        let x1 = origin[0] + (cols - 1) as f64 * f64::from(cell);
        let y1 = origin[1] + (rows - 1) as f64 * f64::from(cell);
        for v in [origin[0], x1, origin[1], y1] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out.extend_from_slice(&((cols * rows) as i32).to_be_bytes());
        out.extend_from_slice(&0i32.to_be_bytes());
        out.extend_from_slice(&cell.to_be_bytes());
        out.extend_from_slice(&cell.to_be_bytes());
        out.extend_from_slice(&(cols as i32).to_be_bytes());
        out.extend_from_slice(&(rows as i32).to_be_bytes());
        for v in values {
            out.extend_from_slice(&v.unwrap_or(0.36).to_be_bytes()); // nulls keep a stale value
            out.extend_from_slice(&i32::from(v.is_some()).to_be_bytes());
        }
        out.extend(std::iter::repeat_n(0u8, padding));
        out
    }

    const E: f64 = 500_025.0;
    const N: f64 = 7_000_625.0;

    #[test]
    fn plain_layout_reads_values_nulls_and_lattice() {
        // 4 x 3 nodes, row 0 the southern edge; one null.
        let values: Vec<Option<f32>> = (0..12).map(|i| if i == 5 { None } else { Some(-133.9 + i as f32 * 0.25) }).collect();
        let bytes = plain_grid("PITUPPER.SRG    01Jan20200000", [E, N], 25.0, 4, 3, &values, 640);
        let read = parse_vulcan_grid(&bytes).unwrap();
        let g = &read.grid;
        assert_eq!((g.cols, g.rows, g.origin, g.cell_size), (4, 3, [E, N], [25.0, 25.0]));
        assert_eq!(read.description, "PITUPPER.SRG    01Jan20200000");
        assert_eq!(g.z_at(0, 0), f64::from(-133.9f32), "first node is the south-west corner, widened exactly");
        assert_eq!(g.z_at(3, 2), f64::from(-133.9f32 + 11.0 * 0.25));
        assert!(g.z_at(1, 1).is_nan());
        assert_eq!(read.log, VulcanLog { nodes: 12, live: 11, nulls: 1, stale_nulls: 1, odd_flags: 0, non_finite: 0, padding_bytes: 640 });
    }

    #[test]
    fn inconsistent_or_unknown_files_are_refused() {
        let values = vec![Some(1.0f32); 6];
        let good = plain_grid("X", [E, N], 25.0, 3, 2, &values, 0);
        let mut wrong_count = good.clone();
        wrong_count[64..68].copy_from_slice(&7i32.to_be_bytes());
        assert!(matches!(parse_vulcan_grid(&wrong_count), Err(IoError::Binary(m)) if m.contains("7 nodes")));
        let mut wrong_extent = good.clone();
        wrong_extent[40..48].copy_from_slice(&(E + 75.0).to_be_bytes());
        assert!(matches!(parse_vulcan_grid(&wrong_extent), Err(IoError::Binary(m)) if m.contains("do not give")));
        assert!(matches!(parse_vulcan_grid(&good[..good.len() - 1]), Err(IoError::Binary(m)) if m.contains("truncated")));
        let mut trailing = good.clone();
        trailing.extend_from_slice(&[0, 0, 7]);
        assert!(matches!(parse_vulcan_grid(&trailing), Err(IoError::Binary(m)) if m.contains("not zero padding")));
        let mut vulz = good;
        vulz[4..8].copy_from_slice(b"vulZ");
        assert!(matches!(parse_vulcan_grid(&vulz), Err(IoError::Unsupported(m)) if m.contains("vulZ")));
    }

    #[test]
    fn grid_names_split_into_stem_and_variable() {
        assert_eq!(vulcan_grid_name("pitupper.rag"), Some(("pitupper".into(), "ra".into())));
        assert_eq!(vulcan_grid_name("PITRL400.SFG"), Some(("pitrl400".into(), "sf".into())));
        assert_eq!(vulcan_grid_name("pitlower.s1g"), Some(("pitlower".into(), "s1".into())));
        assert_eq!(vulcan_grid_name("topo.dxf"), None);
        assert_eq!(vulcan_grid_name("notes.g"), None);
    }

    #[test]
    fn a_seam_set_reads_by_name_and_shares_one_lattice() {
        let dir = std::env::temp_dir().join(format!("zd-vulcan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let grid = |v: f32| plain_grid("t", [E, N], 25.0, 3, 2, &[Some(v); 6], 0);
        std::fs::write(dir.join("pitupper.srg"), grid(120.0)).unwrap();
        std::fs::write(dir.join("PITUPPER.SFG"), grid(118.5)).unwrap();
        std::fs::write(dir.join("pitupper.rag"), grid(30.5)).unwrap();
        std::fs::write(dir.join("pitupperp15.sfg"), grid(135.0)).unwrap(); // another stem: ignored
        let mut compressed = grid(1.5);
        compressed[4..8].copy_from_slice(b"vulZ");
        std::fs::write(dir.join("pitupper.rdg"), compressed).unwrap();

        let seam = read_vulcan_seam(&dir, "pit", "UPPER").unwrap();
        assert_eq!(seam.grids.keys().collect::<Vec<_>>(), ["ra", "sf", "sr"]);
        assert_eq!(seam.roof().grid.z_at(2, 1) - seam.floor().grid.z_at(2, 1), 1.5);
        assert_eq!(seam.skipped.len(), 1, "compressed rd skipped and listed");
        assert_eq!(seam.skipped[0].0, "rd");

        std::fs::write(dir.join("pitupper.tkg"), plain_grid("t", [E, N], 50.0, 3, 2, &[Some(1.5); 6], 0)).unwrap();
        assert!(matches!(read_vulcan_seam(&dir, "pit", "upper"), Err(IoError::Binary(m)) if m.contains("different lattice")));
        assert!(read_vulcan_seam(&dir, "pit", "lower").is_err(), "no roof or floor");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
