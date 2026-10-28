//! Regular grids from XYZ / CSV text: one `x y z` point per line, separated by commas, spaces,
//! tabs or semicolons. The lattice (origin, cell size, rows, cols) is inferred from the points;
//! missing nodes become NaN nulls. Rotated grids are rejected, not guessed at.

use std::fmt::Write as _;
use std::path::Path;

use zd_core::Grid;

use crate::IoError;

#[derive(Debug, Clone, Default)]
pub struct XyzOptions {
    /// A z value to read as null (e.g. -999). Non-finite z and |z| >= 1e20 are always null.
    pub null_value: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GridImport {
    pub grid: Grid,
    pub log: XyzLog,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XyzLog {
    pub points_read: usize,
    /// Non-numeric lines before the first point (column headings).
    pub header_lines: usize,
    pub null_points: usize,
    /// Points that landed on an already-filled node; the first value is kept.
    pub duplicate_points: usize,
}

/// Largest lattice the reader will allocate (nodes), as a guard against a mis-inferred cell size.
const MAX_NODES: usize = 500_000_000;

pub fn read_xyz_grid(path: &Path, options: &XyzOptions) -> Result<GridImport, IoError> {
    let bytes = std::fs::read(path)?;
    parse_xyz_grid(&bytes, options)
}

pub fn parse_xyz_grid(bytes: &[u8], options: &XyzOptions) -> Result<GridImport, IoError> {
    let mut log = XyzLog::default();
    let mut points: Vec<[f64; 3]> = Vec::new();

    for (index, line) in bytes.split(|&b| b == b'\n').enumerate() {
        let line = line.trim_ascii();
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }
        match parse_point(line) {
            Some([x, y, z]) => {
                let null = !z.is_finite() || z.abs() >= 1e20 || options.null_value == Some(z);
                if null {
                    log.null_points += 1;
                }
                points.push([x, y, if null { f64::NAN } else { z }]);
            }
            None if points.is_empty() => log.header_lines += 1,
            None => {
                return Err(IoError::Format {
                    line: index + 1,
                    message: format!("expected 'x y z', found '{}'", String::from_utf8_lossy(line)),
                });
            }
        }
    }
    log.points_read = points.len();

    let (x0, dx, cols) = infer_axis(points.iter().map(|p| p[0]), "x")?;
    let (y0, dy, rows) = infer_axis(points.iter().map(|p| p[1]), "y")?;
    if cols.saturating_mul(rows) > MAX_NODES {
        return Err(IoError::Format {
            line: 0,
            message: format!("inferred lattice of {cols} x {rows} nodes is too large; check the cell size"),
        });
    }

    let mut z = vec![f64::NAN; cols * rows];
    let mut filled = vec![false; cols * rows];
    for p in &points {
        let col = ((p[0] - x0) / dx).round() as usize;
        let row = ((p[1] - y0) / dy).round() as usize;
        let i = row * cols + col;
        if filled[i] {
            log.duplicate_points += 1;
            continue;
        }
        filled[i] = true;
        z[i] = p[2];
    }

    Ok(GridImport { grid: Grid::new([x0, y0], [dx, dy], cols, rows, z), log })
}

/// Writes every node as `x,y,z`; null nodes are skipped.
pub fn write_xyz_grid(path: &Path, grid: &Grid) -> Result<(), IoError> {
    let mut out = String::with_capacity(grid.z.len() * 40);
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let z = grid.z_at(col, row);
            if z.is_nan() {
                continue;
            }
            let [x, y] = grid.node_xy(col, row);
            let _ = writeln!(out, "{x},{y},{z}");
        }
    }
    std::fs::write(path, out)?;
    Ok(())
}

fn parse_point(line: &[u8]) -> Option<[f64; 3]> {
    let text = std::str::from_utf8(line).ok()?;
    let mut fields = text
        .split(|c: char| c == ',' || c == ';' || c.is_ascii_whitespace())
        .filter(|f| !f.is_empty());
    let mut p = [0.0; 3];
    for v in &mut p {
        *v = fields.next()?.parse().ok()?;
    }
    Some(p)
}

/// Origin, spacing and node count along one axis. Every coordinate must sit on the lattice to
/// within a thousandth of a cell.
fn infer_axis(values: impl Iterator<Item = f64>, axis: &str) -> Result<(f64, f64, usize), IoError> {
    let mut unique: Vec<f64> = values.collect();
    unique.sort_unstable_by(f64::total_cmp);
    unique.dedup();
    if unique.len() < 2 {
        return Err(IoError::Format {
            line: 0,
            message: format!("need at least two distinct {axis} values to form a grid"),
        });
    }
    let origin = unique[0];
    let spacing = unique.windows(2).map(|w| w[1] - w[0]).fold(f64::INFINITY, f64::min);
    for &v in &unique {
        let steps = (v - origin) / spacing;
        if (steps - steps.round()).abs() > 1e-3 {
            return Err(IoError::Format {
                line: 0,
                message: format!(
                    "points are not on a regular grid: {axis} = {v} is off the {spacing} spacing from {origin}"
                ),
            });
        }
    }
    let count = ((unique[unique.len() - 1] - origin) / spacing).round() as usize + 1;
    Ok((origin, spacing, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_with_header_gap_and_null() {
        // 3 x 2 nodes at 10 m x 5 m cells; node (1, 1) missing, node (2, 0) null.
        let text = "X,Y,Z\n\
                    650000.0,7450000.0,100.5\n\
                    650010.0,7450000.0,101.0\n\
                    650020.0,7450000.0,-999\n\
                    650000.0,7450005.0,102.0\n\
                    650020.0,7450005.0,103.0\n";
        let options = XyzOptions { null_value: Some(-999.0) };
        let import = parse_xyz_grid(text.as_bytes(), &options).unwrap();
        let g = &import.grid;
        assert_eq!((g.cols, g.rows), (3, 2));
        assert_eq!(g.origin, [650_000.0, 7_450_000.0]);
        assert_eq!(g.cell_size, [10.0, 5.0]);
        assert_eq!(g.z_at(0, 0), 100.5);
        assert_eq!(g.z_at(2, 1), 103.0);
        assert!(g.z_at(1, 1).is_nan(), "missing node is null");
        assert!(g.z_at(2, 0).is_nan(), "null value is null");
        assert_eq!(import.log, XyzLog { points_read: 5, header_lines: 1, null_points: 1, duplicate_points: 0 });
    }

    #[test]
    fn whitespace_separated_and_duplicates_logged() {
        let text = "0 0 1\n1 0 2\n0 1 3\n1 1 4\n1 1 9\n";
        let import = parse_xyz_grid(text.as_bytes(), &XyzOptions::default()).unwrap();
        assert_eq!(import.grid.z_at(1, 1), 4.0, "first value kept");
        assert_eq!(import.log.duplicate_points, 1);
    }

    #[test]
    fn irregular_points_are_rejected() {
        let text = "0,0,1\n10,0,1\n13,0,1\n0,10,1\n";
        let err = parse_xyz_grid(text.as_bytes(), &XyzOptions::default()).unwrap_err();
        assert!(err.to_string().contains("not on a regular grid"), "{err}");
    }

    #[test]
    fn write_then_read_round_trips() {
        let g = Grid::from_fn([650_000.0, 7_450_000.0], [12.5, 12.5], 4, 3, |x, y| (x - 650_000.0) * 0.1 + (y - 7_450_000.0) * 0.01);
        let dir = std::env::temp_dir().join(format!("zd-io-xyz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("grid.xyz");
        write_xyz_grid(&path, &g).unwrap();
        let back = read_xyz_grid(&path, &XyzOptions::default()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(back.grid, g);
    }
}
