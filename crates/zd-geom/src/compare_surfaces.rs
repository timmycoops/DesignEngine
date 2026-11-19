//! Compare two surfaces (Surface tools plan, workstream 1; decision 0093): the cut and fill between them inside a closed
//! string, or wherever both are. Cut is where the second lies below the first (a design under the topography), fill where
//! it lies above, each in BCM. Both are exact, as the solids between the surfaces each way (`solid_between`), the volume
//! of record. A 2.5D column volume at a cell size runs alongside as a check, as Vulcan and Minex would work it, and any
//! difference beyond 0.1 % is said. Asked for, the thickness at each node, the second less the first, comes back as a
//! grid: an isopach.

use std::time::Instant;

use manifold_csg::{CrossSection, FillRule};
use zd_core::{Grid, TriMesh};

use crate::{GeomError, sample_surface_on_grid, solid_between, surface_outline};

/// The column check's difference from the exact volumes past which it's said (a fraction).
pub const COLUMN_CHECK: f64 = 0.001;

/// The most column nodes a compare samples: a finer cell over a larger area is refused, with the cell that would do.
pub const MOST_COLUMN_NODES: usize = 50_000_000;

/// Two surfaces compared: cut and fill, exact, in m³; the plan area both cover inside the string; the same by columns
/// at `cell` m; the thickness grid if asked for; and the time each took.
#[derive(Debug, Clone)]
pub struct SurfaceCompare {
    pub cut: f64,
    pub fill: f64,
    /// The plan area inside the string that both surfaces cover (m²).
    pub area: f64,
    pub cell: f64,
    pub column_cut: f64,
    pub column_fill: f64,
    /// Columns sampled: nodes inside the string where both surfaces are.
    pub columns: usize,
    /// The second less the first at each node, no value where either surface isn't or outside the string.
    pub thickness: Option<Grid>,
    pub exact_ms: f64,
    pub column_ms: f64,
}

impl SurfaceCompare {
    /// How far the column check is from the exact volumes, the larger of cut's and fill's, as a fraction of the
    /// larger exact volume; none where both are nothing.
    pub fn column_difference(&self) -> f64 {
        let scale = self.cut.max(self.fill);
        if scale <= 0.0 {
            return 0.0;
        }
        (self.cut - self.column_cut).abs().max((self.fill - self.column_fill).abs()) / scale
    }
}

/// Compares `second` against `first` inside `outline` (a closed ring in plan, either way round), or, with none, wherever
/// both are. Cut: the second below the first; fill: above. Refused if they don't overlap in plan, or the column check
/// would sample more than [`MOST_COLUMN_NODES`].
pub fn compare_surfaces(first: &TriMesh, second: &TriMesh, outline: Option<&[[f64; 2]]>, cell: f64, thickness: bool) -> Result<SurfaceCompare, GeomError> {
    if !(cell.is_finite() && cell > 0.0) {
        return Err(GeomError::InvalidInput(format!("the column check's cell must be more than nothing; it was {cell} m")));
    }
    let (a, b) = (first.bounds().ok_or_else(|| GeomError::InvalidInput("the first surface is empty".into()))?, second.bounds().ok_or_else(|| GeomError::InvalidInput("the second surface is empty".into()))?);
    let both_lo = [a.min[0].max(b.min[0]), a.min[1].max(b.min[1])];
    let both_hi = [a.max[0].min(b.max[0]), a.max[1].min(b.max[1])];
    if both_lo[0] >= both_hi[0] || both_lo[1] >= both_hi[1] {
        return Err(GeomError::Refused("the two surfaces don't overlap in plan".into()));
    }
    // With no string, a rectangle round where both could be: what either doesn't cover adds nothing.
    let ring: Vec<[f64; 2]> = match outline {
        Some(ring) if ring.len() >= 3 => ring.to_vec(),
        Some(_) => return Err(GeomError::InvalidInput("the string needs three points or more to close round an area".into())),
        None => vec![[both_lo[0] - 1.0, both_lo[1] - 1.0], [both_hi[0] + 1.0, both_lo[1] - 1.0], [both_hi[0] + 1.0, both_hi[1] + 1.0], [both_lo[0] - 1.0, both_hi[1] + 1.0]],
    };

    let started = Instant::now();
    let cut = solid_between(first, second, &ring)?.volume;
    let fill = solid_between(second, first, &ring)?.volume;
    let area = covered_area(first, second, &ring);
    let exact_ms = started.elapsed().as_secs_f64() * 1e3;

    // Columns: a node in the middle of each cell, inside the string, where both surfaces are.
    let started = Instant::now();
    let (lo, hi) = ring.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
    let (lo, hi) = ([lo[0].max(both_lo[0]), lo[1].max(both_lo[1])], [hi[0].min(both_hi[0]), hi[1].min(both_hi[1])]);
    let x0 = (lo[0] / cell).floor() * cell;
    let y0 = (lo[1] / cell).floor() * cell;
    let cols = (((hi[0] - x0) / cell).ceil() as usize).max(1);
    let rows = (((hi[1] - y0) / cell).ceil() as usize).max(1);
    if cols.saturating_mul(rows) > MOST_COLUMN_NODES {
        let fits = ((hi[0] - lo[0]) * (hi[1] - lo[1]) / MOST_COLUMN_NODES as f64).sqrt().ceil();
        return Err(GeomError::Refused(format!("a {cell} m cell over this area is {} columns, more than {MOST_COLUMN_NODES}; {fits} m or more would do", cols * rows)));
    }
    let lattice = Grid::new([x0 + cell / 2.0, y0 + cell / 2.0], [cell, cell], cols, rows, vec![f64::NAN; cols * rows]);
    let (top, bottom) = (sample_surface_on_grid(first, &lattice).grid, sample_surface_on_grid(second, &lattice).grid);
    let mut grid = thickness.then(|| lattice.null_like());
    let (mut column_cut, mut column_fill, mut columns) = (0.0, 0.0, 0usize);
    let cell_area = cell * cell;
    for row in 0..rows {
        for col in 0..cols {
            let k = lattice.index(col, row);
            let (t, s) = (top.z[k], bottom.z[k]);
            if !(t.is_finite() && s.is_finite()) {
                continue;
            }
            let p = [lattice.origin[0] + col as f64 * cell, lattice.origin[1] + row as f64 * cell];
            if !inside(&ring, p) {
                continue;
            }
            columns += 1;
            let d = t - s;
            if d > 0.0 {
                column_cut += d * cell_area;
            } else {
                column_fill -= d * cell_area;
            }
            if let Some(g) = grid.as_mut() {
                g.z[k] = s - t;
            }
        }
    }
    Ok(SurfaceCompare { cut, fill, area, cell, column_cut, column_fill, columns, thickness: grid, exact_ms, column_ms: started.elapsed().as_secs_f64() * 1e3 })
}

/// The plan area inside the ring that both surfaces cover: each surface's edge, its holes left out, overlapped.
fn covered_area(first: &TriMesh, second: &TriMesh, ring: &[[f64; 2]]) -> f64 {
    let origin = ring[0];
    let local = |p: [f64; 2]| [p[0] - origin[0], p[1] - origin[1]];
    let footprint = |mesh: &TriMesh| {
        let rings: Vec<Vec<[f64; 2]>> = surface_outline(mesh).into_iter().filter(|l| l.closed && l.points.len() >= 3).map(|l| l.points.iter().map(|p| local([p[0], p[1]])).collect()).collect();
        CrossSection::from_polygons_with_fill_rule(&rings, FillRule::EvenOdd)
    };
    let mut around: Vec<[f64; 2]> = ring.iter().map(|&p| local(p)).collect();
    let n = around.len();
    let twice: f64 = (0..n).map(|k| around[k][0] * around[(k + 1) % n][1] - around[(k + 1) % n][0] * around[k][1]).sum();
    if twice < 0.0 {
        around.reverse();
    }
    footprint(first).intersection(&footprint(second)).intersection(&CrossSection::from_polygons(&[around])).area()
}

/// Inside a ring in plan, by the crossings of a line due east.
fn inside(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = ring.len();
    let mut within = false;
    for k in 0..n {
        let (a, b) = (ring[k], ring[(k + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
            within = !within;
        }
    }
    within
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const E: f64 = 650_000.0;
    pub(crate) const N: f64 = 7_450_000.0;

    /// A level surface `size` m square from (E, N) at `rl`, in `cells` × `cells`.
    pub(crate) fn level(size: f64, cells: usize, rl: f64) -> TriMesh {
        let mut vertices = Vec::new();
        for j in 0..=cells {
            for i in 0..=cells {
                vertices.push([E + i as f64 * size / cells as f64, N + j as f64 * size / cells as f64, rl]);
            }
        }
        let at = |i: usize, j: usize| (j * (cells + 1) + i) as u32;
        let mut triangles = Vec::new();
        for j in 0..cells {
            for i in 0..cells {
                triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        TriMesh { vertices, triangles }
    }

    /// A pit in a 200 m square design at RL 100: its floor 100 m square at RL 90, centred, walls at 45° to a crest 120 m
    /// square at RL 100. Facing up.
    pub(crate) fn pit() -> TriMesh {
        let square = |half: f64, rl: f64| [[-half, -half], [half, -half], [half, half], [-half, half]].map(|[x, y]| [E + 100.0 + x, N + 100.0 + y, rl]);
        let mut vertices = Vec::new();
        vertices.extend(square(50.0, 90.0)); // 0-3 floor
        vertices.extend(square(60.0, 100.0)); // 4-7 crest
        vertices.extend(square(100.0, 100.0)); // 8-11 outside
        let mut triangles = vec![[0, 1, 2], [0, 2, 3]];
        for (inner, outer) in [(0u32, 4u32), (4, 8)] {
            for k in 0..4u32 {
                let (a, b) = (inner + k, inner + (k + 1) % 4);
                let (c, d) = (outer + (k + 1) % 4, outer + k);
                triangles.push([a, d, c]);
                triangles.push([a, c, b]);
            }
        }
        TriMesh { vertices, triangles }
    }

    fn crest() -> Vec<[f64; 2]> {
        [[-60.0, -60.0], [60.0, -60.0], [60.0, 60.0], [-60.0, 60.0]].map(|[x, y]| [E + 100.0 + x, N + 100.0 + y]).to_vec()
    }

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs().max(1e-12)
    }

    #[test]
    fn a_pit_under_a_level_topography_is_a_frustum_of_cut() {
        // 10/3 × (100² + 120² + 100 × 120) = 121,333.33 m³ of cut, none of fill; the columns within 0.1 % at 1 m.
        let frustum = 10.0 / 3.0 * (10_000.0 + 14_400.0 + 12_000.0);
        let c = compare_surfaces(&level(200.0, 4, 100.0), &pit(), None, 1.0, false).unwrap();
        assert!(rel(c.cut, frustum) < 1e-6, "{} against {frustum}", c.cut);
        assert!(c.fill.abs() < 1e-6, "{}", c.fill);
        assert!((c.area - 40_000.0).abs() < 1e-6, "{}", c.area);
        assert!(c.column_difference() < COLUMN_CHECK, "columns {} against {}", c.column_cut, c.cut);
        assert!(c.thickness.is_none());
        // Inside the crest, the same.
        let c = compare_surfaces(&level(200.0, 4, 100.0), &pit(), Some(&crest()), 1.0, false).unwrap();
        assert!(rel(c.cut, frustum) < 1e-6 && (c.area - 14_400.0).abs() < 1e-6, "{} {}", c.cut, c.area);
    }

    #[test]
    fn a_topography_2_m_higher_adds_the_slab_and_one_at_rl_95_cuts_and_fills() {
        let frustum = 10.0 / 3.0 * (10_000.0 + 14_400.0 + 12_000.0);
        // RL 102 over the crest: the frustum and a 2 m slab 120 m square.
        let c = compare_surfaces(&level(200.0, 4, 102.0), &pit(), Some(&crest()), 1.0, false).unwrap();
        assert!(rel(c.cut, frustum + 2.0 * 14_400.0) < 1e-6 && c.fill.abs() < 1e-6, "{} {}", c.cut, c.fill);
        // RL 95: cut the pit below 95 (100 m to 110 m square over 5 m); fill what's above 95 inside the crest, 120² × 5
        // less the pit between 95 and 100 (110 m to 120 m square).
        let cut = 5.0 / 3.0 * (10_000.0 + 12_100.0 + 11_000.0);
        let fill = 14_400.0 * 5.0 - 5.0 / 3.0 * (12_100.0 + 14_400.0 + 13_200.0);
        let c = compare_surfaces(&level(200.0, 4, 95.0), &pit(), Some(&crest()), 1.0, false).unwrap();
        assert!(rel(c.cut, cut) < 1e-6 && rel(c.fill, fill) < 1e-6, "{} against {cut}; {} against {fill}", c.cut, c.fill);
        assert!(c.column_difference() < COLUMN_CHECK, "{} {} against {} {}", c.column_cut, c.column_fill, c.cut, c.fill);
    }

    #[test]
    fn its_thickness_grid_is_the_second_less_the_first() {
        let c = compare_surfaces(&level(200.0, 4, 100.0), &pit(), None, 1.0, true).unwrap();
        let g = c.thickness.unwrap();
        let at = |x: f64, y: f64| {
            let col = ((E + x - g.origin[0]) / g.cell_size[0]).round() as usize;
            let row = ((N + y - g.origin[1]) / g.cell_size[1]).round() as usize;
            g.z[g.index(col, row)]
        };
        assert!((at(100.5, 100.5) + 10.0).abs() < 1e-9, "the floor, 10 m down");
        assert!(at(10.5, 10.5).abs() < 1e-9, "outside the crest, nothing");
        // On the west wall, 4.5 m in from the floor's edge at 45°: RL 94.5, 5.5 m down.
        assert!((at(45.5, 100.5) + 5.5).abs() < 1e-9, "on the wall: {}", at(45.5, 100.5));
    }

    #[test]
    fn what_cant_be_compared_is_refused() {
        let mut apart = level(100.0, 2, 100.0);
        for v in &mut apart.vertices {
            v[0] += 1_000.0;
        }
        assert!(compare_surfaces(&level(100.0, 2, 100.0), &apart, None, 1.0, false).unwrap_err().to_string().contains("don't overlap"));
        assert!(compare_surfaces(&level(100.0, 2, 100.0), &pit(), None, 0.0, false).is_err());
        let too_fine = compare_surfaces(&level(200.0, 2, 100.0), &pit(), None, 0.01, false).unwrap_err().to_string();
        assert!(too_fine.contains("columns, more than"), "{too_fine}");
    }

    /// Timings on made-up data: a 1.0 M-triangle rolling topography 4 km square against a 100,000-triangle pit
    /// design in its middle, compared wherever both are, columns at 5 m.
    /// `cargo test -p zd-geom --release --lib timing_compare -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_compare() {
        let cells = 707;
        let mut topo = level(4_000.0, cells, 0.0);
        for v in &mut topo.vertices {
            let (x, y) = (v[0] - E, v[1] - N);
            v[2] = 100.0 + 6.0 * (x / 300.0).sin() * (y / 250.0).cos() + 0.004 * x;
        }
        let mut design = level(1_200.0, 224, 0.0);
        for v in &mut design.vertices {
            v[0] += 1_400.0;
            v[1] += 1_400.0;
            let (x, y) = (v[0] - E - 2_000.0, v[1] - N - 2_000.0);
            let out = (x.abs().max(y.abs()) - 300.0).max(0.0);
            v[2] = 40.0 + out.min(150.0);
        }
        let c = compare_surfaces(&topo, &design, None, 5.0, true).unwrap();
        println!(
            "compare: {} + {} triangles in; exact {:.0} ms, columns at 5 m {:.0} ms ({} columns); cut {:.0} m3, fill {:.0} m3, area {:.0} m2; columns differ by {:.4} %",
            topo.triangles.len(),
            design.triangles.len(),
            c.exact_ms,
            c.column_ms,
            c.columns,
            c.cut,
            c.fill,
            c.area,
            100.0 * c.column_difference()
        );
    }
}
