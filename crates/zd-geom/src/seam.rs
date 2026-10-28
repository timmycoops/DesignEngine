use zd_core::{Grid, TriMesh};

use crate::GeomError;

/// A closed seam solid between roof and floor grids, with its 2.5D column volume for
/// reconciliation.
#[derive(Debug, Clone)]
pub struct SeamSolid {
    pub mesh: TriMesh,
    /// Sum over cell triangles of plan area times mean thickness. Roof and floor share the same
    /// triangulation, so this equals the solid's enclosed volume up to rounding.
    pub column_volume: f64,
    pub log: SeamLog,
}

/// What `build_seam_solid` excluded or changed, so nothing is cleaned silently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeamLog {
    pub active_cells: usize,
    /// Cells with a null corner or a corner thinner than the minimum thickness.
    pub thin_or_null_cells: usize,
    /// Cells dropped where two active cells met only at a corner (that would pinch the solid
    /// to a non-manifold edge).
    pub pinch_cells_removed: usize,
    /// Nodes where the floor lay above the roof; thickness clamped to zero.
    pub floor_nodes_clamped: usize,
}

/// Builds a watertight solid between `roof` and `floor` on the same lattice.
///
/// Thickness is clamped at zero so the horizons never cross. A cell is part of the solid only if
/// all four corners have both surfaces and at least `min_thickness`; the solid's sides are
/// vertical walls along the footprint boundary.
pub fn build_seam_solid(roof: &Grid, floor: &Grid, min_thickness: f64) -> Result<SeamSolid, GeomError> {
    if !roof.same_lattice(floor) {
        return Err(GeomError::InvalidInput("roof and floor grids must share a lattice".into()));
    }
    if min_thickness <= 0.0 || min_thickness.is_nan() {
        return Err(GeomError::InvalidInput("minimum seam thickness must be positive".into()));
    }
    let mut log = SeamLog::default();

    let (floor_z, thickness) = clamp_floor_to_roof(roof, floor, &mut log);
    let (cols, rows) = (roof.cols - 1, roof.rows - 1);
    let mut active = vec![false; cols * rows];
    for row in 0..rows {
        for col in 0..cols {
            let corners = cell_corners(roof, col, row);
            let thick = corners.iter().all(|&i| thickness[i] >= min_thickness);
            active[row * cols + col] = thick;
            if !thick {
                log.thin_or_null_cells += 1;
            }
        }
    }
    log.pinch_cells_removed = remove_corner_pinches(&mut active, cols, rows);
    log.active_cells = active.iter().filter(|&&a| a).count();
    if log.active_cells == 0 {
        return Err(GeomError::InvalidInput("no cells meet the minimum seam thickness".into()));
    }

    let mesh = seam_mesh(roof, &floor_z, &active);
    let half_cell = 0.5 * roof.cell_size[0] * roof.cell_size[1];
    let mut column_volume = 0.0;
    for row in 0..rows {
        for col in 0..cols {
            if active[row * cols + col] {
                let [sw, se, ne, nw] = cell_corners(roof, col, row).map(|i| thickness[i]);
                column_volume += half_cell * ((sw + se + ne) + (sw + ne + nw)) / 3.0;
            }
        }
    }
    Ok(SeamSolid { mesh, column_volume, log })
}

/// Floor elevations with the floor never above the roof, and node thickness (NaN where either
/// surface is null).
fn clamp_floor_to_roof(roof: &Grid, floor: &Grid, log: &mut SeamLog) -> (Vec<f64>, Vec<f64>) {
    let mut floor_z = floor.z.clone();
    let mut thickness = vec![f64::NAN; roof.z.len()];
    for i in 0..roof.z.len() {
        let (r, f) = (roof.z[i], floor.z[i]);
        if r.is_nan() || f.is_nan() {
            continue;
        }
        if f > r {
            floor_z[i] = r;
            log.floor_nodes_clamped += 1;
        }
        thickness[i] = r - floor_z[i];
    }
    (floor_z, thickness)
}

/// Node indices of a cell's corners in counter-clockwise order: SW, SE, NE, NW.
fn cell_corners(grid: &Grid, col: usize, row: usize) -> [usize; 4] {
    [
        grid.index(col, row),
        grid.index(col + 1, row),
        grid.index(col + 1, row + 1),
        grid.index(col, row + 1),
    ]
}

/// Drops one cell of every diagonal pair that meets only at a corner, until none remain.
fn remove_corner_pinches(active: &mut [bool], cols: usize, rows: usize) -> usize {
    let mut removed = 0;
    loop {
        let before = removed;
        for row in 1..rows {
            for col in 1..cols {
                let sw = row * cols + col - cols - 1;
                let se = sw + 1;
                let nw = sw + cols;
                let ne = nw + 1;
                if active[sw] && active[ne] && !active[se] && !active[nw] {
                    active[ne] = false;
                    removed += 1;
                } else if active[se] && active[nw] && !active[sw] && !active[ne] {
                    active[nw] = false;
                    removed += 1;
                }
            }
        }
        if removed == before {
            return removed;
        }
    }
}

fn seam_mesh(roof: &Grid, floor_z: &[f64], active: &[bool]) -> TriMesh {
    let (cols, rows) = (roof.cols - 1, roof.rows - 1);
    let is_active = |col: isize, row: isize| {
        col >= 0 && row >= 0 && (col as usize) < cols && (row as usize) < rows && active[row as usize * cols + col as usize]
    };

    // One roof and one floor vertex per node used by an active cell.
    let mut used = vec![false; roof.z.len()];
    for row in 0..rows {
        for col in 0..cols {
            if active[row * cols + col] {
                for i in cell_corners(roof, col, row) {
                    used[i] = true;
                }
            }
        }
    }
    let mut roof_v = vec![u32::MAX; roof.z.len()];
    let mut floor_v = vec![u32::MAX; roof.z.len()];
    let mut vertices = Vec::new();
    for row in 0..roof.rows {
        for col in 0..roof.cols {
            let i = roof.index(col, row);
            if used[i] {
                let [x, y] = roof.node_xy(col, row);
                roof_v[i] = vertices.len() as u32;
                vertices.push([x, y, roof.z[i]]);
                floor_v[i] = vertices.len() as u32;
                vertices.push([x, y, floor_z[i]]);
            }
        }
    }

    let mut triangles = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            if !active[row * cols + col] {
                continue;
            }
            let [sw, se, ne, nw] = cell_corners(roof, col, row);
            triangles.push([roof_v[sw], roof_v[se], roof_v[ne]]);
            triangles.push([roof_v[sw], roof_v[ne], roof_v[nw]]);
            triangles.push([floor_v[sw], floor_v[ne], floor_v[se]]);
            triangles.push([floor_v[sw], floor_v[nw], floor_v[ne]]);

            // Counter-clockwise edges with the neighbouring cell across each one.
            let (c, r) = (col as isize, row as isize);
            for (a, b, neighbour) in [(sw, se, (c, r - 1)), (se, ne, (c + 1, r)), (ne, nw, (c, r + 1)), (nw, sw, (c - 1, r))] {
                if !is_active(neighbour.0, neighbour.1) {
                    // Outward wall: the cell lies to the left of a -> b seen from above.
                    triangles.push([floor_v[a], floor_v[b], roof_v[b]]);
                    triangles.push([floor_v[a], roof_v[b], roof_v[a]]);
                }
            }
        }
    }
    TriMesh::new(vertices, triangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use manifold_csg::Manifold;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn lattice(cols: usize, rows: usize, f: impl Fn(f64, f64) -> f64) -> Grid {
        Grid::from_fn([E, N], [1.0, 1.0], cols, rows, move |x, y| f(x - E, y - N))
    }

    fn manifold_volume(mesh: &TriMesh) -> f64 {
        let verts: Vec<f64> = mesh.vertices.iter().flatten().copied().collect();
        let tris: Vec<u64> = mesh.triangles.iter().flatten().map(|&i| u64::from(i)).collect();
        Manifold::from_mesh_f64(&verts, 3, &tris).expect("manifold accepts the seam solid").volume()
    }

    #[test]
    fn flat_seam_is_a_box() {
        // 10 m x 10 m footprint, roof 10 m above floor: 1000 m3.
        let roof = lattice(11, 11, |_, _| 110.0);
        let floor = lattice(11, 11, |_, _| 100.0);
        let seam = build_seam_solid(&roof, &floor, 0.01).unwrap();
        assert!(seam.mesh.validity().is_watertight());
        assert!(seam.mesh.validity().non_degenerate);
        assert!((seam.mesh.signed_volume() - 1000.0).abs() < 1e-6);
        assert!((seam.column_volume - 1000.0).abs() < 1e-9);
        assert!((manifold_volume(&seam.mesh) - 1000.0).abs() < 1e-6);
        assert_eq!(seam.log.active_cells, 100);
    }

    #[test]
    fn linear_thickness_matches_the_integral() {
        // Thickness 10 + 0.1x - 0.05y over [0,10]^2 integrates to 1025 m3; linear, so the
        // triangulated solid is exact.
        let roof = lattice(11, 11, |x, _| 110.0 + 0.1 * x);
        let floor = lattice(11, 11, |_, y| 100.0 + 0.05 * y);
        let seam = build_seam_solid(&roof, &floor, 0.01).unwrap();
        assert!(seam.mesh.validity().is_watertight());
        assert!((seam.mesh.signed_volume() - 1025.0).abs() < 1e-6);
        assert!((seam.column_volume - seam.mesh.signed_volume()).abs() < 1e-6);
        assert!((manifold_volume(&seam.mesh) - 1025.0).abs() < 1e-6);
    }

    #[test]
    fn crossing_floor_is_clamped_and_thin_cells_excluded() {
        // Floor crosses the roof at x = 5: west half has 0..5 m of coal, east half none.
        let roof = lattice(11, 3, |_, _| 100.0);
        let floor = lattice(11, 3, |x, _| 95.0 + x);
        let seam = build_seam_solid(&roof, &floor, 0.5).unwrap();
        assert_eq!(seam.log.floor_nodes_clamped, 5 * 3, "nodes at x = 6..10");
        // Cells with every corner at least 0.5 m thick: x in [0,4] (corner thickness 5..1).
        assert_eq!(seam.log.active_cells, 4 * 2);
        assert_eq!(seam.log.thin_or_null_cells, 6 * 2);
        assert!(seam.mesh.validity().is_watertight());
        // Thickness 5 - x over [0,4] x [0,2] = 2 * (20 - 8) = 24 m3.
        assert!((seam.mesh.signed_volume() - 24.0).abs() < 1e-9);
    }

    #[test]
    fn diagonal_cells_are_unpinched() {
        // 2 x 2 cells; thin nodes at the SE and NW corners leave only the SW and NE cells,
        // which meet at the centre node alone.
        let roof = lattice(3, 3, |_, _| 10.0);
        let floor = lattice(3, 3, |x, y| if (x, y) == (2.0, 0.0) || (x, y) == (0.0, 2.0) { 9.99 } else { 0.0 });
        let seam = build_seam_solid(&roof, &floor, 0.5).unwrap();
        assert_eq!(seam.log.pinch_cells_removed, 1);
        assert_eq!(seam.log.active_cells, 1);
        let v = seam.mesh.validity();
        assert!(v.is_watertight(), "{v:?}");
        assert!((seam.mesh.signed_volume() - 10.0).abs() < 1e-9);
        // Manifold sums its volume at raw mine-grid coordinates, so it only agrees to ~1e-9 m3.
        assert!((manifold_volume(&seam.mesh) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn mismatched_lattices_are_rejected() {
        let roof = lattice(4, 4, |_, _| 10.0);
        let floor = lattice(5, 4, |_, _| 0.0);
        assert!(build_seam_solid(&roof, &floor, 0.1).is_err());
    }
}
