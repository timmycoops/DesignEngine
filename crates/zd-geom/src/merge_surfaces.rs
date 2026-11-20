//! Merge surfaces, grids and solids (Surface tools plan, workstream 2; decision 0094): their bottom, the lowest of them at
//! each place (a pit shell or strip solids dug out of the topography), or their top, the highest (a dump laid on it).
//! Where only one covers a place, it stands as it is. A solid counts by its floor for the bottom and by its top for the
//! top. It comes out as a grid at a cell size, fast at any size, or as an exact surface, every crest, toe and berm where
//! the inputs put them and joined exactly where they meet (Tim, 20 November: "the ability to make a grid custom grid
//! size ... then have the option of creating a surface perfectly also").

use std::time::Instant;

use manifold_csg::{CrossSection, Manifold};
use zd_core::{Grid, TriMesh};

use crate::cut::{from_manifold, to_manifold};
use crate::{GeomError, Side, close_surface_to_halfspace, sample_surface_on_grid, triangulate_grid};

/// Which of them is kept at each place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envelope {
    /// The lowest: what's dug out.
    Bottom,
    /// The highest: what's laid on.
    Top,
}

/// One thing merged.
#[derive(Debug, Clone, Copy)]
pub enum MergeInput<'a> {
    Surface(&'a TriMesh),
    Grid(&'a Grid),
    /// A closed solid: its floor counts for the bottom, its top for the top.
    Solid(&'a TriMesh),
}

/// The most nodes a merged grid may have: a finer cell over a larger area is refused, with the cell that would do.
pub const MOST_MERGE_NODES: usize = 100_000_000;

/// A merged grid, and whether it was the inputs' own lattice taken node by node, and the time it took.
#[derive(Debug, Clone)]
pub struct MergedGrid {
    pub grid: Grid,
    pub nodes_with_height: usize,
    pub same_lattice: bool,
    pub ms: f64,
}

/// A merged exact surface: its mesh, the triangles that went in, and the time it took.
#[derive(Debug, Clone)]
pub struct MergedSurface {
    pub mesh: TriMesh,
    pub triangles_in: usize,
    pub ms: f64,
}

/// A solid's skin facing up (its top) or down (its floor), as a surface: the triangles whose plan normal points that way.
pub fn solid_skin(solid: &TriMesh, top: bool) -> TriMesh {
    let triangles = solid
        .triangles
        .iter()
        .copied()
        .filter(|t| {
            let [a, b, c] = t.map(|i| solid.vertices[i as usize]);
            let up = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if top { up > 0.0 } else { up < 0.0 }
        })
        .collect();
    compact(&solid.vertices, triangles)
}

/// The vertices a set of triangles uses, renumbered.
fn compact(vertices: &[[f64; 3]], triangles: Vec<[u32; 3]>) -> TriMesh {
    let mut index = vec![u32::MAX; vertices.len()];
    let mut kept = Vec::new();
    let triangles = triangles
        .into_iter()
        .map(|t| {
            t.map(|i| {
                if index[i as usize] == u32::MAX {
                    index[i as usize] = kept.len() as u32;
                    kept.push(vertices[i as usize]);
                }
                index[i as usize]
            })
        })
        .collect();
    TriMesh { vertices: kept, triangles }
}

/// Each input as the surface it counts by.
fn surfaces_of(inputs: &[MergeInput], keep: Envelope) -> Vec<TriMesh> {
    inputs
        .iter()
        .map(|input| match input {
            MergeInput::Surface(mesh) => (*mesh).clone(),
            MergeInput::Grid(grid) => triangulate_grid(grid),
            MergeInput::Solid(solid) => solid_skin(solid, keep == Envelope::Top),
        })
        .collect()
}

fn check_count(inputs: &[MergeInput]) -> Result<(), GeomError> {
    if inputs.len() < 2 {
        return Err(GeomError::InvalidInput(format!("a merge takes two things or more; {} was given", inputs.len())));
    }
    Ok(())
}

/// The bottom or top as a grid. Grids all on one lattice, with no cell asked for, are taken node by node; anything else
/// is sampled at every node of a lattice `cell` m square over all of them, each node the lowest or highest height there,
/// none where nothing is.
pub fn merge_to_grid(inputs: &[MergeInput], keep: Envelope, cell: Option<f64>) -> Result<MergedGrid, GeomError> {
    check_count(inputs)?;
    let started = Instant::now();
    let pick = |a: f64, b: f64| match (a.is_finite(), b.is_finite()) {
        (true, true) => {
            if keep == Envelope::Bottom {
                a.min(b)
            } else {
                a.max(b)
            }
        }
        (true, false) => a,
        (false, true) => b,
        (false, false) => f64::NAN,
    };
    let grids: Vec<&Grid> = inputs.iter().filter_map(|i| if let MergeInput::Grid(g) = i { Some(*g) } else { None }).collect();
    let same = grids.len() == inputs.len() && grids.windows(2).all(|w| w[0].origin == w[1].origin && w[0].cell_size == w[1].cell_size && w[0].cols == w[1].cols && w[0].rows == w[1].rows);
    if same && cell.is_none_or(|c| grids[0].cell_size == [c, c]) {
        let mut z = grids[0].z.clone();
        for g in &grids[1..] {
            for (a, &b) in z.iter_mut().zip(&g.z) {
                *a = pick(*a, b);
            }
        }
        let nodes_with_height = z.iter().filter(|v| v.is_finite()).count();
        let g = grids[0];
        return Ok(MergedGrid { grid: Grid::new(g.origin, g.cell_size, g.cols, g.rows, z), nodes_with_height, same_lattice: true, ms: started.elapsed().as_secs_f64() * 1e3 });
    }
    let cell = cell.unwrap_or(5.0);
    if !(cell.is_finite() && cell > 0.0) {
        return Err(GeomError::InvalidInput(format!("the grid's cell must be more than nothing; it was {cell} m")));
    }
    let surfaces = surfaces_of(inputs, keep);
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for s in &surfaces {
        if let Some(b) = s.bounds() {
            lo = [lo[0].min(b.min[0]), lo[1].min(b.min[1])];
            hi = [hi[0].max(b.max[0]), hi[1].max(b.max[1])];
        }
    }
    if !(lo[0] <= hi[0] && lo[1] <= hi[1]) {
        return Err(GeomError::InvalidInput("nothing to merge: every input is empty".into()));
    }
    let x0 = (lo[0] / cell).floor() * cell;
    let y0 = (lo[1] / cell).floor() * cell;
    let cols = ((hi[0] - x0) / cell).ceil() as usize + 1;
    let rows = ((hi[1] - y0) / cell).ceil() as usize + 1;
    if cols.saturating_mul(rows) > MOST_MERGE_NODES {
        let fits = ((hi[0] - lo[0]) * (hi[1] - lo[1]) / MOST_MERGE_NODES as f64).sqrt().ceil();
        return Err(GeomError::Refused(format!("a {cell} m cell over these is {} nodes, more than {MOST_MERGE_NODES}; {fits} m or more would do", cols * rows)));
    }
    let lattice = Grid::new([x0, y0], [cell, cell], cols, rows, vec![f64::NAN; cols * rows]);
    let mut z = vec![f64::NAN; cols * rows];
    for s in &surfaces {
        let sampled = sample_surface_on_grid(s, &lattice).grid;
        for (a, &b) in z.iter_mut().zip(&sampled.z) {
            *a = pick(*a, b);
        }
    }
    let nodes_with_height = z.iter().filter(|v| v.is_finite()).count();
    Ok(MergedGrid { grid: Grid::new([x0, y0], [cell, cell], cols, rows, z), nodes_with_height, same_lattice: false, ms: started.elapsed().as_secs_f64() * 1e3 })
}

/// The bottom or top as an exact surface. Each input closed down to a floor below them all as the solid under it; the
/// bottom keeps what's under every one of them where it is (and under the rest outside it), the top what's under any;
/// the new surface is the upper skin of what's kept. Every vertex where the inputs meet is where they meet.
pub fn merge_to_surface(inputs: &[MergeInput], keep: Envelope) -> Result<MergedSurface, GeomError> {
    check_count(inputs)?;
    let started = Instant::now();
    let surfaces = surfaces_of(inputs, keep);
    let triangles_in = surfaces.iter().map(|s| s.triangles.len()).sum();
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for s in &surfaces {
        if let Some(b) = s.bounds() {
            lo = [lo[0].min(b.min[0]), lo[1].min(b.min[1]), lo[2].min(b.min[2])];
            hi = [hi[0].max(b.max[0]), hi[1].max(b.max[1]), hi[2].max(b.max[2])];
        }
    }
    if !(lo[0] <= hi[0] && lo[1] <= hi[1]) {
        return Err(GeomError::InvalidInput("nothing to merge: every input is empty".into()));
    }
    let origin = [((lo[0] + hi[0]) / 2.0).round(), ((lo[1] + hi[1]) / 2.0).round(), 0.0];
    let to_local = [-origin[0], -origin[1], 0.0];
    let floor = lo[2] - 10.0 - 0.01 * (hi[2] - lo[2]);
    let height = hi[2] - floor + 20.0;
    let mut below = Vec::with_capacity(surfaces.len());
    let mut footprints = Vec::with_capacity(surfaces.len());
    for (k, s) in surfaces.iter().enumerate() {
        if s.triangles.is_empty() {
            return Err(GeomError::InvalidInput(format!("input {} has nothing to count by: a solid with no {} skin, or an empty surface", k + 1, if keep == Envelope::Top { "top" } else { "floor" })));
        }
        let closed = close_surface_to_halfspace(s, Side::Below, floor)?;
        let m = to_manifold(&closed.mesh, to_local, "an input closed down")?;
        footprints.push(CrossSection::from_polygons(&m.project()));
        below.push(m);
    }
    let kept = match keep {
        Envelope::Top => Manifold::batch_union(&below),
        Envelope::Bottom => {
            let region = CrossSection::batch_union(&footprints);
            let column = |area: &CrossSection| Manifold::extrude(area, height).translate(0.0, 0.0, floor - 5.0);
            let mut kept = column(&region);
            for (b, f) in below.iter().zip(&footprints) {
                // Where it isn't, the others stand: under it, or anywhere in the column outside it. Covering all of
                // them, there's no outside, and an empty area can't be stood up as a column.
                let outside = region.difference(f);
                kept = if outside.area() > 1e-9 { kept.intersection(&b.union(&column(&outside))) } else { kept.intersection(b) };
            }
            kept
        }
    };
    kept.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    let solid = from_manifold(&kept, origin);
    let skin: Vec<[u32; 3]> = solid
        .triangles
        .iter()
        .copied()
        .filter(|t| {
            let [a, b, c] = t.map(|i| solid.vertices[i as usize]);
            let up = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            up > 1e-9 && (a[2] + b[2] + c[2]) / 3.0 > floor + 1e-6
        })
        .collect();
    Ok(MergedSurface { mesh: compact(&solid.vertices, skin), triangles_in, ms: started.elapsed().as_secs_f64() * 1e3 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare_surfaces::tests::{E, N, level, pit};
    use crate::compare_surfaces;

    fn frustum() -> f64 {
        10.0 / 3.0 * (10_000.0 + 14_400.0 + 12_000.0)
    }

    /// A box solid `side` m square at (x, y) off (E, N), from RL `z0` to `z1`, facing out.
    fn cube(x: f64, y: f64, side: f64, z0: f64, z1: f64) -> TriMesh {
        let v: Vec<[f64; 3]> = (0..8).map(|i| [E + x + (i & 1) as f64 * side, N + y + (i >> 1 & 1) as f64 * side, if i >> 2 == 0 { z0 } else { z1 }]).collect();
        let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh { vertices: v, triangles }
    }

    fn grid_at(g: &Grid, x: f64, y: f64) -> f64 {
        g.z[g.index(((E + x - g.origin[0]) / g.cell_size[0]).round() as usize, ((N + y - g.origin[1]) / g.cell_size[1]).round() as usize)]
    }

    #[test]
    fn a_pit_dug_out_of_the_topography_exactly() {
        // The bottom of a level topography at RL 100 and the pit design: one surface, the pit's crest at RL 100, and the
        // merged surface less the topography the pit, 121,333.33 m³ exact.
        let topo = level(400.0, 8, 100.0);
        let merged = merge_to_surface(&[MergeInput::Surface(&topo), MergeInput::Surface(&pit())], Envelope::Bottom).unwrap();
        let c = compare_surfaces(&topo, &merged.mesh, None, 1.0, false).unwrap();
        assert!((c.cut - frustum()).abs() / frustum() < 1e-6 && c.fill.abs() < 1e-6, "cut {} fill {}", c.cut, c.fill);
        assert!((c.area - 160_000.0).abs() < 1e-3, "it covers the topography, beyond the design: {}", c.area);
        let low = merged.mesh.vertices.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
        assert!((low - 90.0).abs() < 1e-9, "its floor at RL 90: {low}");
    }

    #[test]
    fn a_higher_topography_cuts_deeper_and_a_lower_one_keeps_the_design_above_it_out() {
        // At RL 102 the design's level ground at RL 100 is below it: the bottom keeps it, 2 m under the topography over
        // the design's 200 m square, and the pit below that.
        let topo = level(400.0, 8, 102.0);
        let merged = merge_to_surface(&[MergeInput::Surface(&topo), MergeInput::Surface(&pit())], Envelope::Bottom).unwrap();
        let c = compare_surfaces(&topo, &merged.mesh, None, 1.0, false).unwrap();
        let expected = frustum() + 2.0 * 40_000.0;
        assert!((c.cut - expected).abs() / expected < 1e-6, "{} against {expected}", c.cut);
        // Its top at RL 95 is the topography's wherever the design's above it.
        let topo = level(400.0, 8, 95.0);
        let top = merge_to_surface(&[MergeInput::Surface(&topo), MergeInput::Surface(&pit())], Envelope::Top).unwrap();
        let c = compare_surfaces(&topo, &top.mesh, None, 1.0, false).unwrap();
        let fill = 40_000.0 * 5.0 - 5.0 / 3.0 * (12_100.0 + 14_400.0 + 13_200.0);
        assert!(c.cut.abs() < 1e-6 && (c.fill - fill).abs() / fill < 1e-6, "cut {} fill {} against {fill}", c.cut, c.fill);
    }

    #[test]
    fn solids_count_by_their_floor_and_their_top() {
        let topo = level(400.0, 8, 100.0);
        // A block from RL 90 to 110 dug out: its floor, 20 m square 10 m down, 4,000 m³.
        let block = cube(100.0, 100.0, 20.0, 90.0, 110.0);
        let dug = merge_to_surface(&[MergeInput::Surface(&topo), MergeInput::Solid(&block)], Envelope::Bottom).unwrap();
        let c = compare_surfaces(&topo, &dug.mesh, None, 1.0, false).unwrap();
        assert!((c.cut - 4_000.0).abs() < 1e-6 && c.fill.abs() < 1e-6, "{} {}", c.cut, c.fill);
        // A dump from RL 100 to 112 laid on: its top, 12 m up, 4,800 m³.
        let dump = cube(200.0, 200.0, 20.0, 100.0, 112.0);
        let laid = merge_to_surface(&[MergeInput::Surface(&topo), MergeInput::Solid(&dump)], Envelope::Top).unwrap();
        let c = compare_surfaces(&topo, &laid.mesh, None, 1.0, false).unwrap();
        assert!((c.fill - 4_800.0).abs() < 1e-6 && c.cut.abs() < 1e-6, "{} {}", c.cut, c.fill);
        // As a 1 m grid: the block's floor at its middle, the topography outside.
        let g = merge_to_grid(&[MergeInput::Surface(&topo), MergeInput::Solid(&block)], Envelope::Bottom, Some(1.0)).unwrap();
        assert!(!g.same_lattice);
        assert!((grid_at(&g.grid, 110.0, 110.0) - 90.0).abs() < 1e-9 && (grid_at(&g.grid, 50.0, 50.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn as_a_grid_the_lowest_at_each_node_and_grids_alike_node_by_node() {
        let topo = level(400.0, 8, 100.0);
        let g = merge_to_grid(&[MergeInput::Surface(&topo), MergeInput::Surface(&pit())], Envelope::Bottom, Some(5.0)).unwrap();
        assert_eq!((g.grid.cols, g.grid.rows), (81, 81));
        assert!((grid_at(&g.grid, 100.0, 100.0) - 90.0).abs() < 1e-9, "the floor");
        assert!((grid_at(&g.grid, 300.0, 300.0) - 100.0).abs() < 1e-9, "the topography, past the design");
        assert!((grid_at(&g.grid, 45.0, 100.0) - 95.0).abs() < 1e-9, "the wall, half way up");
        // Two grids on one lattice, no cell asked for: node by node.
        let a = Grid::new([E, N], [5.0, 5.0], 3, 2, vec![100.0, 100.0, f64::NAN, 100.0, 100.0, 100.0]);
        let b = Grid::new([E, N], [5.0, 5.0], 3, 2, vec![90.0, 110.0, 95.0, f64::NAN, 100.0, 99.0]);
        let low = merge_to_grid(&[MergeInput::Grid(&a), MergeInput::Grid(&b)], Envelope::Bottom, None).unwrap();
        assert!(low.same_lattice);
        assert_eq!(low.grid.z, vec![90.0, 100.0, 95.0, 100.0, 100.0, 99.0]);
        let high = merge_to_grid(&[MergeInput::Grid(&a), MergeInput::Grid(&b)], Envelope::Top, None).unwrap();
        assert_eq!(high.grid.z, vec![100.0, 110.0, 95.0, 100.0, 100.0, 100.0]);
    }

    #[test]
    fn what_cant_be_merged_is_refused() {
        let topo = level(100.0, 2, 100.0);
        assert!(merge_to_surface(&[MergeInput::Surface(&topo)], Envelope::Bottom).is_err());
        assert!(merge_to_grid(&[MergeInput::Surface(&topo), MergeInput::Surface(&topo)], Envelope::Bottom, Some(0.0)).is_err());
        let fine = merge_to_grid(&[MergeInput::Surface(&topo), MergeInput::Surface(&topo)], Envelope::Bottom, Some(0.001)).unwrap_err().to_string();
        assert!(fine.contains("would do"), "{fine}");
    }

    /// Timings on made-up data, as the merge spike was: a 1.0 M-triangle topography 4 km square and a 100,000-triangle
    /// pit shell, merged as an exact surface and as 2, 5 and 10 m grids.
    /// `cargo test -p zd-geom --release --lib timing_merge -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_merge() {
        let mut topo = level(4_000.0, 707, 0.0);
        for v in &mut topo.vertices {
            let (x, y) = (v[0] - E, v[1] - N);
            v[2] = 100.0 + 6.0 * (x / 300.0).sin() * (y / 250.0).cos() + 0.004 * x;
        }
        let mut shell = level(1_200.0, 224, 0.0);
        for v in &mut shell.vertices {
            v[0] += 1_400.0;
            v[1] += 1_400.0;
            let (x, y) = (v[0] - E - 2_000.0, v[1] - N - 2_000.0);
            v[2] = 40.0 + (x.abs().max(y.abs()) - 300.0).max(0.0).min(150.0);
        }
        let inputs = [MergeInput::Surface(&topo), MergeInput::Surface(&shell)];
        let exact = merge_to_surface(&inputs, Envelope::Bottom).unwrap();
        println!("exact surface: {} triangles in, {:.0} ms, {} triangles out", exact.triangles_in, exact.ms, exact.mesh.triangles.len());
        for cell in [2.0, 5.0, 10.0] {
            let g = merge_to_grid(&inputs, Envelope::Bottom, Some(cell)).unwrap();
            println!("{cell} m grid: {:.0} ms, {} by {} nodes", g.ms, g.grid.cols, g.grid.rows);
        }
    }
}
