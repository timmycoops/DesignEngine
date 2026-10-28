use zd_core::{Grid, TriMesh};

/// Triangulates a grid: two triangles per cell with four live corners, one with three.
/// Triangles are wound counter-clockwise seen from above (upward normals). Null nodes get no
/// vertex.
pub fn triangulate_grid(grid: &Grid) -> TriMesh {
    let mut index = vec![u32::MAX; grid.z.len()];
    let mut vertices = Vec::with_capacity(grid.z.len() - grid.null_count());
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            let z = grid.z_at(col, row);
            if !z.is_nan() {
                let [x, y] = grid.node_xy(col, row);
                index[grid.index(col, row)] = vertices.len() as u32;
                vertices.push([x, y, z]);
            }
        }
    }

    let mut triangles = Vec::with_capacity(2 * grid.cols.saturating_sub(1) * grid.rows.saturating_sub(1));
    for row in 0..grid.rows.saturating_sub(1) {
        for col in 0..grid.cols.saturating_sub(1) {
            // Corners in counter-clockwise order: SW, SE, NE, NW.
            let ring = [
                index[grid.index(col, row)],
                index[grid.index(col + 1, row)],
                index[grid.index(col + 1, row + 1)],
                index[grid.index(col, row + 1)],
            ];
            let live = ring.iter().filter(|&&i| i != u32::MAX).count();
            match live {
                4 => {
                    triangles.push([ring[0], ring[1], ring[2]]);
                    triangles.push([ring[0], ring[2], ring[3]]);
                }
                3 => {
                    let mut tri = ring.iter().copied().filter(|&i| i != u32::MAX);
                    triangles.push([tri.next().unwrap(), tri.next().unwrap(), tri.next().unwrap()]);
                }
                _ => {}
            }
        }
    }
    TriMesh::new(vertices, triangles)
}

#[derive(Debug, Clone)]
pub struct SampledGrid {
    pub grid: Grid,
    /// Nodes under more than one triangle at different elevations (overhangs or folds). The
    /// highest elevation is kept.
    pub multi_valued_nodes: usize,
}

/// Samples a triangulated surface at every node of `lattice` by linear interpolation inside the
/// triangle above or below the node. Nodes outside the surface's footprint are null.
pub fn sample_surface_on_grid(surface: &TriMesh, lattice: &Grid) -> SampledGrid {
    let mut grid = lattice.null_like();
    let mut multi_valued_nodes = 0;
    let [x0, y0] = lattice.origin;
    let [dx, dy] = lattice.cell_size;

    for &[ia, ib, ic] in &surface.triangles {
        let a = surface.vertices[ia as usize];
        let b = surface.vertices[ib as usize];
        let c = surface.vertices[ic as usize];
        // Work relative to corner a so mine-grid coordinates keep their precision.
        let (bx, by) = (b[0] - a[0], b[1] - a[1]);
        let (cx, cy) = (c[0] - a[0], c[1] - a[1]);
        let det = bx * cy - by * cx;
        if det == 0.0 {
            continue; // vertical in plan: covers no nodes
        }
        let col_range = node_range(a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]), x0, dx, lattice.cols);
        let row_range = node_range(a[1].min(b[1]).min(c[1]), a[1].max(b[1]).max(c[1]), y0, dy, lattice.rows);
        let tolerance = 1e-9;
        for row in row_range {
            let py = y0 + row as f64 * dy - a[1];
            for col in col_range.clone() {
                let px = x0 + col as f64 * dx - a[0];
                let wb = (px * cy - py * cx) / det;
                let wc = (bx * py - by * px) / det;
                let wa = 1.0 - wb - wc;
                if wa < -tolerance || wb < -tolerance || wc < -tolerance {
                    continue;
                }
                let z = wa * a[2] + wb * b[2] + wc * c[2];
                let i = grid.index(col, row);
                let existing = grid.z[i];
                if existing.is_nan() {
                    grid.z[i] = z;
                } else {
                    if (existing - z).abs() > 1e-6 {
                        multi_valued_nodes += 1;
                    }
                    grid.z[i] = existing.max(z);
                }
            }
        }
    }
    SampledGrid { grid, multi_valued_nodes }
}

/// Lattice node indices whose coordinate lies within `[lo, hi]`, clamped to the lattice.
fn node_range(lo: f64, hi: f64, origin: f64, spacing: f64, count: usize) -> std::ops::Range<usize> {
    let first = ((lo - origin) / spacing - 1e-9).ceil().max(0.0);
    let last = ((hi - origin) / spacing + 1e-9).floor();
    if last < 0.0 || first >= count as f64 || first > last {
        return 0..0;
    }
    first as usize..(last as usize + 1).min(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_grid_gives_two_upward_triangles_per_cell() {
        let g = Grid::from_fn([650_000.0, 7_450_000.0], [10.0, 5.0], 3, 3, |_, _| 100.0);
        let m = triangulate_grid(&g);
        assert_eq!(m.vertices.len(), 9);
        assert_eq!(m.triangles.len(), 8);
        let plan: f64 = m.triangles.iter().map(|&t| 0.5 * m.triangle_cross(t)[2]).sum();
        assert!((plan - 200.0).abs() < 1e-9, "plan area 20 m x 10 m, all normals up");
        assert!(m.validity().consistent_winding);
    }

    #[test]
    fn null_corner_leaves_one_triangle_in_its_cell() {
        let mut g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 3, 3, |_, _| 0.0);
        g.z[0] = f64::NAN;
        let m = triangulate_grid(&g);
        assert_eq!(m.vertices.len(), 8);
        assert_eq!(m.triangles.len(), 7);
    }

    #[test]
    fn sampling_a_plane_reproduces_it_exactly() {
        let plane = |x: f64, y: f64| 120.0 + 0.02 * (x - 650_000.0) - 0.01 * (y - 7_450_000.0);
        let coarse = Grid::from_fn([650_000.0, 7_450_000.0], [25.0, 25.0], 9, 9, plane);
        let surface = triangulate_grid(&coarse);
        // Finer lattice, offset by half a cell and overhanging the surface on two sides.
        let lattice = Grid::from_fn([650_005.0, 7_450_005.0], [10.0, 10.0], 25, 25, |_, _| 0.0);
        let sampled = sample_surface_on_grid(&surface, &lattice);
        assert_eq!(sampled.multi_valued_nodes, 0);
        let mut live = 0;
        for row in 0..lattice.rows {
            for col in 0..lattice.cols {
                let [x, y] = lattice.node_xy(col, row);
                let z = sampled.grid.z_at(col, row);
                let inside = x <= 650_200.0 && y <= 7_450_200.0;
                assert_eq!(!z.is_nan(), inside, "node ({col}, {row})");
                if inside {
                    live += 1;
                    assert!((z - plane(x, y)).abs() < 1e-9);
                }
            }
        }
        assert_eq!(live, 20 * 20);
    }
}
