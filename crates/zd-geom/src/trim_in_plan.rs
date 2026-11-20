//! Trim in plan (Surface tools plan, workstream 3; decision 0095): anything picked cut by a closed string in plan,
//! keeping the inside or the outside, each kind by its own rule. A string is clipped as Clip does it, at its own heights
//! (`clip_string`); a surface is cut exactly along the string at its own heights; a solid is cut vertically along the
//! string and stays closed, its volume exact (the cut `cut_solid_by_polygons` makes, with one polygon); a grid has its
//! nodes on the side dropped set to no value, and nothing filled.
//!
//! Each comes back as it was ([`InPlan::Untouched`]) where the string leaves it wholly on the side kept, or as nothing
//! ([`InPlan::Dropped`]) where it lies wholly on the side dropped: what a run does with either is the command's to say.

use std::time::Instant;

use manifold_csg::{CrossSection, Manifold};
use zd_core::{Aabb, Grid, Polyline, TriMesh};

use crate::cut::{design_near, from_manifold, to_manifold};
use crate::merge_surfaces::upper_skin;
use crate::string_cut::{Trimmed, WHOLLY_INSIDE, WHOLLY_OUTSIDE};
use crate::{GeomError, Side, clip_string, close_surface_to_halfspace, cut_solid_by_polygons};

/// A grid node this close to the string in plan lies on it, and is kept whichever side is (m).
pub const ON_THE_TRIM: f64 = 1e-6;

/// What a trim leaves of a thing.
#[derive(Debug, Clone, PartialEq)]
pub enum InPlan<T> {
    /// The string doesn't take anything from it: it lies wholly on the side kept.
    Untouched,
    /// Nothing of it is on the side kept.
    Dropped,
    /// What's left of it.
    Trimmed(T),
}

impl<T> InPlan<T> {
    pub fn trimmed(&self) -> Option<&T> {
        match self {
            InPlan::Trimmed(t) => Some(t),
            _ => None,
        }
    }
}

/// A surface trimmed: what's left, its area in plan before and after (m²), and the time it took.
#[derive(Debug, Clone)]
pub struct SurfaceTrim {
    pub kept: InPlan<TriMesh>,
    pub area_in: f64,
    pub area_kept: f64,
    pub ms: f64,
}

/// A solid trimmed: what's left, its volume before and after (m³), the separate parts left, and the time it took.
#[derive(Debug, Clone)]
pub struct SolidTrim {
    pub kept: InPlan<TriMesh>,
    pub volume_in: f64,
    pub volume_kept: f64,
    pub parts: usize,
    pub ms: f64,
}

/// A grid trimmed: what's left, cropped to the nodes with a height on the same lattice; the nodes with a height before
/// and after; and the time it took.
#[derive(Debug, Clone)]
pub struct GridTrim {
    pub kept: InPlan<Grid>,
    pub nodes_in: usize,
    pub nodes_kept: usize,
    pub ms: f64,
}

/// The closed string's ring in plan, the closing vertex not repeated, checked to close round an area.
pub fn trim_ring(polygon: &Polyline) -> Result<Vec<[f64; 2]>, GeomError> {
    if !polygon.closed || polygon.vertices.len() < 3 {
        return Err(GeomError::InvalidInput("the string to trim with isn't a closed string of three vertices or more".into()));
    }
    let mut ring: Vec<[f64; 2]> = polygon.vertices.iter().map(|v| [v[0], v[1]]).collect();
    if ring.len() > 3 && ring.first() == ring.last() {
        ring.pop();
    }
    if signed_area(&ring).abs() < 1e-9 {
        return Err(GeomError::InvalidInput("the string to trim with closes round no area in plan".into()));
    }
    Ok(ring)
}

/// A string trimmed: Clip's cut, its parts kept in the order it runs.
pub fn trim_string_in_plan(line: &Polyline, polygon: &Polyline, inside: bool) -> Result<InPlan<Trimmed>, GeomError> {
    match clip_string(line, polygon, inside) {
        Ok(t) if t.cuts == 0 => Ok(InPlan::Untouched),
        Ok(t) => Ok(InPlan::Trimmed(t)),
        Err(GeomError::InvalidInput(words)) if words == WHOLLY_OUTSIDE || words == WHOLLY_INSIDE => Ok(InPlan::Dropped),
        Err(e) => Err(e),
    }
}

/// A surface cut exactly along the ring at its own heights: closed down to a floor below it as the solid under it, that
/// solid cut by the ring's prism, and the upper skin of what's left the surface. Every vertex where the ring crosses a
/// triangle lies on that triangle. Keeping the inside, only the triangles near the ring go through the cut.
pub fn trim_surface_in_plan(surface: &TriMesh, ring: &[[f64; 2]], inside: bool) -> Result<SurfaceTrim, GeomError> {
    let started = Instant::now();
    if surface.triangles.is_empty() {
        return Err(GeomError::InvalidInput("the surface has no triangles".into()));
    }
    let area_in = mesh_plan_area(surface);
    let (lo, hi) = ring_bounds(ring);
    let working = if inside {
        let plan = Aabb { min: [lo[0], lo[1], 0.0], max: [hi[0], hi[1], 0.0] };
        design_near(surface, &plan, 1.0 + 0.01 * (hi[0] - lo[0]).hypot(hi[1] - lo[1]))
    } else {
        surface.clone()
    };
    let done = |kept: InPlan<TriMesh>, area_kept: f64| SurfaceTrim { kept, area_in, area_kept, ms: started.elapsed().as_secs_f64() * 1e3 };
    let Some(bounds) = working.bounds() else {
        return Ok(done(InPlan::Dropped, 0.0));
    };
    let origin = [((bounds.min[0] + bounds.max[0]) / 2.0).round(), ((bounds.min[1] + bounds.max[1]) / 2.0).round(), 0.0];
    let floor = bounds.min[2] - 10.0 - 0.01 * (bounds.max[2] - bounds.min[2]);
    let top = bounds.max[2] + 10.0;
    let closed = close_surface_to_halfspace(&working, Side::Below, floor)?;
    let below = to_manifold(&closed.mesh, [-origin[0], -origin[1], 0.0], "the surface closed down")?;
    let prism = prism(ring, [origin[0], origin[1]], floor - 5.0, top);
    let kept = if inside { below.intersection(&prism) } else { below.difference(&prism) };
    kept.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    let skin = upper_skin(&from_manifold(&kept, origin), floor);
    let area_kept = mesh_plan_area(&skin);
    let tolerance = 1e-9 * area_in + 1e-6;
    Ok(if area_kept <= tolerance {
        done(InPlan::Dropped, 0.0)
    } else if (area_in - area_kept).abs() <= tolerance {
        done(InPlan::Untouched, area_in)
    } else {
        done(InPlan::Trimmed(skin), area_kept)
    })
}

/// A solid cut vertically along the ring, its piece inside or outside kept: closed, the pieces adding up to the solid.
pub fn trim_solid_in_plan(solid: &TriMesh, ring: &[[f64; 2]], inside: bool) -> Result<SolidTrim, GeomError> {
    let started = Instant::now();
    let cut = cut_solid_by_polygons(solid, &[ring.to_vec()])?;
    let (kept, dropped) = if inside { (cut.pieces.into_iter().next(), cut.remainder) } else { (cut.remainder, cut.pieces.into_iter().next()) };
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let volume_in = cut.input_volume;
    Ok(match (kept, dropped) {
        (_, None) => SolidTrim { kept: InPlan::Untouched, volume_in, volume_kept: volume_in, parts: 1, ms },
        (None, Some(_)) => SolidTrim { kept: InPlan::Dropped, volume_in, volume_kept: 0.0, parts: 0, ms },
        (Some(piece), Some(_)) => SolidTrim { volume_kept: piece.volume, parts: piece.parts, kept: InPlan::Trimmed(piece.mesh), volume_in, ms },
    })
}

/// A grid's nodes on the side dropped set to no value; nodes on the string itself are kept either way. What's left is
/// cropped to the nodes with a height, on the same lattice. Nothing is filled.
pub fn trim_grid_in_plan(grid: &Grid, ring: &[[f64; 2]], inside: bool) -> GridTrim {
    let started = Instant::now();
    let (cols, rows) = (grid.cols, grid.rows);
    let mut within = vec![false; cols * rows];
    let n = ring.len();
    let mut xs = Vec::new();
    for row in 0..rows {
        let y = grid.node_xy(0, row)[1];
        xs.clear();
        for k in 0..n {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            if (a[1] <= y) != (b[1] <= y) {
                xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            let first = ((pair[0] - grid.origin[0]) / grid.cell_size[0]).ceil().max(0.0) as usize;
            let last = ((pair[1] - grid.origin[0]) / grid.cell_size[0]).floor();
            if last < 0.0 {
                continue;
            }
            for col in first..=(last as usize).min(cols.saturating_sub(1)) {
                within[grid.index(col, row)] = true;
            }
        }
    }
    let on_the_line = nodes_on_the_ring(grid, ring);
    let mut z = grid.z.clone();
    let (mut nodes_in, mut nodes_kept) = (0, 0);
    for (k, height) in z.iter_mut().enumerate() {
        if height.is_nan() {
            continue;
        }
        nodes_in += 1;
        if within[k] == inside || on_the_line[k] {
            nodes_kept += 1;
        } else {
            *height = f64::NAN;
        }
    }
    let ms = |s: Instant| s.elapsed().as_secs_f64() * 1e3;
    if nodes_kept == nodes_in {
        return GridTrim { kept: InPlan::Untouched, nodes_in, nodes_kept, ms: ms(started) };
    }
    if nodes_kept == 0 {
        return GridTrim { kept: InPlan::Dropped, nodes_in, nodes_kept, ms: ms(started) };
    }
    GridTrim { kept: InPlan::Trimmed(cropped(grid, z)), nodes_in, nodes_kept, ms: ms(started) }
}

/// The nodes within [`ON_THE_TRIM`] of the ring's edges, each edge looking only at the nodes in its own box.
fn nodes_on_the_ring(grid: &Grid, ring: &[[f64; 2]]) -> Vec<bool> {
    let mut on = vec![false; grid.cols * grid.rows];
    let index = |v: f64, origin: f64, cell: f64, most: usize| ((v - origin) / cell).clamp(0.0, most as f64) as usize;
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let (x0, x1) = (a[0].min(b[0]) - ON_THE_TRIM, a[0].max(b[0]) + ON_THE_TRIM);
        let (y0, y1) = (a[1].min(b[1]) - ON_THE_TRIM, a[1].max(b[1]) + ON_THE_TRIM);
        if x1 < grid.origin[0] || y1 < grid.origin[1] {
            continue;
        }
        let (c0, c1) = (index(x0, grid.origin[0], grid.cell_size[0], grid.cols - 1), index(x1, grid.origin[0], grid.cell_size[0], grid.cols - 1));
        let (r0, r1) = (index(y0, grid.origin[1], grid.cell_size[1], grid.rows - 1), index(y1, grid.origin[1], grid.cell_size[1], grid.rows - 1));
        for row in r0..=r1 {
            for col in c0..=(c1 + 1).min(grid.cols - 1) {
                if distance_to_segment(grid.node_xy(col, row), a, b) <= ON_THE_TRIM {
                    on[grid.index(col, row)] = true;
                }
            }
        }
    }
    on
}

/// The grid cropped to the rows and columns that still have a height, on the same lattice.
fn cropped(grid: &Grid, z: Vec<f64>) -> Grid {
    let (mut c0, mut c1, mut r0, mut r1) = (usize::MAX, 0, usize::MAX, 0);
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            if !z[grid.index(col, row)].is_nan() {
                (c0, c1, r0, r1) = (c0.min(col), c1.max(col), r0.min(row), r1.max(row));
            }
        }
    }
    let (cols, rows) = (c1 - c0 + 1, r1 - r0 + 1);
    let mut kept = Vec::with_capacity(cols * rows);
    for row in r0..=r1 {
        for col in c0..=c1 {
            kept.push(z[grid.index(col, row)]);
        }
    }
    Grid::new(grid.node_xy(c0, r0), grid.cell_size, cols, rows, kept)
}

fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// The ring's prism from `bottom` to `top`, about a whole-metre origin.
fn prism(ring: &[[f64; 2]], origin: [f64; 2], bottom: f64, top: f64) -> Manifold {
    let mut local: Vec<[f64; 2]> = ring.iter().map(|p| [p[0] - origin[0], p[1] - origin[1]]).collect();
    if signed_area(&local) < 0.0 {
        local.reverse();
    }
    Manifold::extrude(&CrossSection::from_polygons(std::slice::from_ref(&local)), top - bottom).translate(0.0, 0.0, bottom)
}

fn ring_bounds(ring: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    ring.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]))
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let o = ring[0];
    let mut twice = 0.0;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        twice += (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1]);
    }
    0.5 * twice
}

/// A mesh's area in plan: each triangle's, however it faces (m²).
pub fn mesh_plan_area(mesh: &TriMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare_surfaces::tests::{E, N, level, pit};
    use crate::{compare_surfaces, sample_surface_on_grid};

    /// A square ring `side` m across with its south-west corner at (x, y) off (E, N).
    fn square(x: f64, y: f64, side: f64) -> Vec<[f64; 2]> {
        vec![[E + x, N + y], [E + x + side, N + y], [E + x + side, N + y + side], [E + x, N + y + side]]
    }

    fn ring_string(ring: &[[f64; 2]], rl: f64) -> Polyline {
        Polyline { vertices: ring.iter().map(|p| [p[0], p[1], rl]).collect(), closed: true }
    }

    /// A plane rising 1 in 10 to the east, `size` m square from (E - 100, N - 100), cut in `n` by `n` squares.
    fn slope(size: f64, n: usize) -> TriMesh {
        let mut mesh = level(size, n, 100.0);
        for v in &mut mesh.vertices {
            v[2] = 100.0 + 0.1 * (v[0] - E);
        }
        mesh
    }

    #[test]
    fn a_plane_trimmed_to_a_square_inside_and_outside() {
        // A plane rising 1 in 10, 400 m square, trimmed to a 100 m square set off its own triangles' corners: inside,
        // 10,000 m² in plan, every vertex on the plane; outside, the rest, 150,000 m².
        let plane = slope(400.0, 8);
        let ring = square(12.5, 37.5, 100.0);
        let t = trim_surface_in_plan(&plane, &ring, true).unwrap();
        let InPlan::Trimmed(mesh) = &t.kept else { panic!("trimmed: {:?}", t.kept) };
        assert!((t.area_kept - 10_000.0).abs() < 1e-6, "{}", t.area_kept);
        assert!((mesh_plan_area(mesh) - 10_000.0).abs() < 1e-6);
        for v in &mesh.vertices {
            assert!((v[2] - (100.0 + 0.1 * (v[0] - E))).abs() < 1e-9, "on the plane: {v:?}");
            assert!(v[0] >= E + 12.5 - 1e-9 && v[0] <= E + 112.5 + 1e-9 && v[1] >= N + 37.5 - 1e-9 && v[1] <= N + 137.5 + 1e-9, "inside the square: {v:?}");
        }
        let t = trim_surface_in_plan(&plane, &ring, false).unwrap();
        assert!(matches!(t.kept, InPlan::Trimmed(_)));
        assert!((t.area_kept - 150_000.0).abs() < 1e-6, "{}", t.area_kept);
        assert!((t.area_in - 160_000.0).abs() < 1e-6);
    }

    #[test]
    fn the_pit_trimmed_keeps_its_volume_inside_and_the_rest_outside() {
        // The pit design trimmed to its own crest square (120 m, RL 100) and compared against the level topography: the
        // frustum, 121,333.33 m³, inside; nothing outside, where the design is level with the topography.
        let topo = level(400.0, 8, 100.0);
        let ring = square(40.0, 40.0, 120.0);
        let inside = trim_surface_in_plan(&pit(), &ring, true).unwrap();
        let InPlan::Trimmed(mesh) = &inside.kept else { panic!("trimmed") };
        let c = compare_surfaces(&topo, mesh, None, 1.0, false).unwrap();
        let frustum = 10.0 / 3.0 * (10_000.0 + 14_400.0 + 12_000.0);
        assert!((c.cut - frustum).abs() / frustum < 1e-6 && c.fill.abs() < 1e-6, "{} {}", c.cut, c.fill);
        assert!((c.area - 14_400.0).abs() < 1e-6, "{}", c.area);
        let outside = trim_surface_in_plan(&pit(), &ring, false).unwrap();
        let InPlan::Trimmed(mesh) = &outside.kept else { panic!("trimmed") };
        let c = compare_surfaces(&topo, mesh, None, 1.0, false).unwrap();
        assert!(c.cut.abs() < 1e-6 && c.fill.abs() < 1e-6 && (c.area - 25_600.0).abs() < 1e-6, "{} {} {}", c.cut, c.fill, c.area);
    }

    #[test]
    fn a_surface_wholly_on_one_side_is_untouched_or_dropped() {
        let plane = slope(400.0, 8);
        let around = square(-200.0, -200.0, 800.0);
        assert!(matches!(trim_surface_in_plan(&plane, &around, true).unwrap().kept, InPlan::Untouched));
        assert!(matches!(trim_surface_in_plan(&plane, &around, false).unwrap().kept, InPlan::Dropped));
        let away = square(1_000.0, 1_000.0, 50.0);
        assert!(matches!(trim_surface_in_plan(&plane, &away, true).unwrap().kept, InPlan::Dropped));
        assert!(matches!(trim_surface_in_plan(&plane, &away, false).unwrap().kept, InPlan::Untouched));
    }

    /// A box solid `side` m square at (x, y) off (E, N), from RL `z0` to `z1`, facing out.
    fn cube(x: f64, y: f64, side: f64, z0: f64, z1: f64) -> TriMesh {
        let v: Vec<[f64; 3]> = (0..8).map(|i| [E + x + (i & 1) as f64 * side, N + y + (i >> 1 & 1) as f64 * side, if i >> 2 == 0 { z0 } else { z1 }]).collect();
        let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh { vertices: v, triangles }
    }

    #[test]
    fn a_solid_cut_vertically_closed_and_exact() {
        // A 100 m cube from RL 50 to 150, trimmed by a 50 m square over its corner: 250,000 m³ inside, 750,000 outside,
        // each closed.
        let solid = cube(0.0, 0.0, 100.0, 50.0, 150.0);
        let ring = square(50.0, 50.0, 100.0);
        let inside = trim_solid_in_plan(&solid, &ring, true).unwrap();
        let InPlan::Trimmed(mesh) = &inside.kept else { panic!("trimmed") };
        assert!(mesh.validity().is_watertight());
        assert!((inside.volume_kept - 250_000.0).abs() < 1e-6 && (mesh.signed_volume() - 250_000.0).abs() < 1e-6, "{}", inside.volume_kept);
        let outside = trim_solid_in_plan(&solid, &ring, false).unwrap();
        assert!((outside.volume_kept - 750_000.0).abs() < 1e-6, "{}", outside.volume_kept);
        assert!((inside.volume_kept + outside.volume_kept - inside.volume_in).abs() < 1e-6);
        assert!(matches!(trim_solid_in_plan(&solid, &square(-10.0, -10.0, 200.0), true).unwrap().kept, InPlan::Untouched));
        assert!(matches!(trim_solid_in_plan(&solid, &square(500.0, 500.0, 10.0), true).unwrap().kept, InPlan::Dropped));
    }

    #[test]
    fn a_grid_loses_its_nodes_on_the_side_dropped_and_keeps_those_on_the_line() {
        // A 1 m grid 101 by 101 nodes, trimmed by a square on nodes from 20 to 60: inside, 41 by 41 nodes, cropped to
        // them, every height as it was; outside, all but the 39 by 39 strictly inside, the lattice as it was.
        let plane = slope(400.0, 8);
        let lattice = Grid::new([E, N], [1.0, 1.0], 101, 101, vec![0.0; 101 * 101]);
        let grid = sample_surface_on_grid(&plane, &lattice).grid;
        let ring = square(20.0, 20.0, 40.0);
        let inside = trim_grid_in_plan(&grid, &ring, true);
        let InPlan::Trimmed(g) = &inside.kept else { panic!("trimmed") };
        assert_eq!((g.cols, g.rows, g.origin, inside.nodes_kept, inside.nodes_in), (41, 41, [E + 20.0, N + 20.0], 1_681, 10_201));
        assert!(g.z.iter().all(|z| !z.is_nan()));
        assert!((g.z[g.index(5, 7)] - (100.0 + 0.1 * 25.0)).abs() < 1e-9);
        let outside = trim_grid_in_plan(&grid, &ring, false);
        let InPlan::Trimmed(g) = &outside.kept else { panic!("trimmed") };
        assert_eq!((g.cols, g.rows, g.origin, outside.nodes_kept), (101, 101, [E, N], 10_201 - 39 * 39));
        assert!(g.z[g.index(40, 40)].is_nan() && !g.z[g.index(20, 40)].is_nan());
        assert!(matches!(trim_grid_in_plan(&grid, &square(-5.0, -5.0, 200.0), true).kept, InPlan::Untouched));
        assert!(matches!(trim_grid_in_plan(&grid, &square(-5.0, -5.0, 200.0), false).kept, InPlan::Dropped));
    }

    #[test]
    fn a_string_as_clip_cuts_it() {
        let line = Polyline { vertices: vec![[E, N + 50.0, 100.0], [E + 200.0, N + 50.0, 120.0]], closed: false };
        let ring = ring_string(&square(50.0, 0.0, 100.0), 0.0);
        let InPlan::Trimmed(t) = trim_string_in_plan(&line, &ring, true).unwrap() else { panic!("trimmed") };
        assert_eq!(t.pieces.len(), 1);
        let expected = [[E + 50.0, N + 50.0, 105.0], [E + 150.0, N + 50.0, 115.0]];
        assert_eq!(t.pieces[0].vertices.len(), 2);
        for (v, w) in t.pieces[0].vertices.iter().zip(expected) {
            assert!((0..3).all(|k| (v[k] - w[k]).abs() < 1e-9), "{v:?} against {w:?}");
        }
        let away = ring_string(&square(500.0, 0.0, 10.0), 0.0);
        assert_eq!(trim_string_in_plan(&line, &away, true).unwrap(), InPlan::Dropped);
        assert_eq!(trim_string_in_plan(&line, &away, false).unwrap(), InPlan::Untouched);
    }

    #[test]
    fn a_ring_with_no_area_is_refused() {
        let flat = Polyline { vertices: vec![[E, N, 0.0], [E + 10.0, N, 0.0], [E + 20.0, N, 0.0]], closed: true };
        assert!(trim_ring(&flat).is_err());
        let open = Polyline { vertices: vec![[E, N, 0.0], [E + 10.0, N, 0.0], [E, N + 10.0, 0.0]], closed: false };
        assert!(trim_ring(&open).is_err());
    }

    /// Timing, release, made-up data: a 1 M-triangle topography 4 km square trimmed to a 1 km square inside and outside,
    /// and a 2 m grid of it the same.
    #[test]
    #[ignore]
    fn timing_trim() {
        let topo = slope(4_000.0, 707);
        let ring = square(1_000.0, 1_000.0, 1_000.0);
        for inside in [true, false] {
            let t = trim_surface_in_plan(&topo, &ring, inside).unwrap();
            let out = t.kept.trimmed().map_or(0, |m| m.triangles.len());
            println!("surface {} triangles, {}: {:.0} ms, {} out", topo.triangles.len(), if inside { "inside" } else { "outside" }, t.ms, out);
        }
        let lattice = Grid::new([E - 100.0, N - 100.0], [2.0, 2.0], 2_001, 2_001, vec![0.0; 2_001 * 2_001]);
        let grid = sample_surface_on_grid(&topo, &lattice).grid;
        for inside in [true, false] {
            let t = trim_grid_in_plan(&grid, &ring, inside);
            println!("grid 2,001 by 2,001, {}: {:.0} ms, {} nodes kept", if inside { "inside" } else { "outside" }, t.ms, t.nodes_kept);
        }
    }
}
