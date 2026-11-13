//! Solids cut by a seam stack's layers (decision 0062, workstream 3), with volumes worked by hand at mine grid
//! coordinates. Volumes to 0.001 m³ unless said.

use zd_core::{Lattice, TriMesh};
use zd_geom::{LayerCut, cut_solid_by_layers, grid_hole_patches, layer_stack, solid_off_the_grids};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// 10 m cells from 50 m west and south of (E, N) to 550 m east and 150 m north.
const LATTICE: Lattice = Lattice { origin: [E - 50.0, N - 50.0], cell_size: [10.0, 10.0], cols: 61, rows: 21 };

/// A grid on the lattice from local x (east of E).
fn grid(f: impl Fn(f64) -> f64) -> Vec<f64> {
    (0..LATTICE.rows).flat_map(|_| (0..LATTICE.cols).map(|c| -50.0 + 10.0 * c as f64)).map(&f).collect()
}

fn flat(z: f64) -> Vec<f64> {
    grid(|_| z)
}

/// A solid running east from `x0` to `x1` with the same convex section all along: `section` in (north, RL) about
/// `(N, 0)`. Wound outward.
fn along_strike(x0: f64, x1: f64, section: &[(f64, f64)]) -> TriMesh {
    let n = section.len();
    let mut vertices = Vec::new();
    for x in [x0, x1] {
        vertices.extend(section.iter().map(|&(y, z)| [E + x, N + y, z]));
    }
    let mut triangles = Vec::new();
    for k in 1..n - 1 {
        triangles.push([n as u32, (n + k) as u32, (n + k + 1) as u32]);
        triangles.push([0, (k + 1) as u32, k as u32]);
    }
    for k in 0..n {
        let (a, b) = (k as u32, ((k + 1) % n) as u32);
        triangles.push([a, b, b + n as u32]);
        triangles.push([a, b + n as u32, a + n as u32]);
    }
    let mesh = TriMesh::new(vertices, triangles);
    if mesh.signed_volume() < 0.0 { TriMesh::new(mesh.vertices, mesh.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect()) } else { mesh }
}

/// A strip 60 m wide (north) with vertical sides from RL `z0` to `z1`.
fn strip(x0: f64, x1: f64, z0: f64, z1: f64) -> TriMesh {
    along_strike(x0, x1, &[(0.0, z0), (60.0, z0), (60.0, z1), (0.0, z1)])
}

/// Three flat seams: U 25 to 23, M 18 to 15, L 8 to 5.
fn three_seams() -> Vec<Vec<f64>> {
    [25.0, 23.0, 18.0, 15.0, 8.0, 5.0].map(flat).to_vec()
}

fn cut(solid: &TriMesh, surfaces: &[Vec<f64>]) -> LayerCut {
    let horizons: Vec<(&[f64], &[f64])> = surfaces.chunks(2).map(|p| (p[0].as_slice(), p[1].as_slice())).collect();
    let stack = layer_stack(LATTICE, &horizons).unwrap();
    cut_solid_by_layers(solid, &stack, 0.01).unwrap()
}

/// Each layer's piece volume, 0 where none.
fn by_layer(cut: &LayerCut, layers: usize) -> Vec<f64> {
    let mut v = vec![0.0; layers];
    for p in &cut.pieces {
        assert!(p.mesh.validity().is_watertight(), "layer {} {:?}", p.layer, p.mesh.validity());
        v[p.layer] += p.volume;
    }
    v
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn flat_seams_cut_a_strip_into_its_seven_layers_exactly() {
    // 500 x 60 m, RL 0 to 30: overburden 5 m, U 2, U-M 5, M 3, M-L 7, L 3 and 5 m below, each times 30,000 m².
    let cut = cut(&strip(0.0, 500.0, 0.0, 30.0), &three_seams());
    let want = [5.0, 2.0, 5.0, 3.0, 7.0, 3.0, 5.0].map(|t| t * 30_000.0);
    let got = by_layer(&cut, 7);
    for (layer, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(near(*g, w, 0.001), "layer {layer}: {g} against {w}");
    }
    assert!(cut.outside.is_none());
    assert!(cut.thin.abs() < 0.001, "{}", cut.thin);
    assert!(cut.difference().abs() < 0.001, "{}", cut.difference());
    assert!(cut.pieces.iter().all(|p| p.parts == 1));
}

#[test]
fn a_battered_strip_keeps_its_batter_in_every_layer() {
    // The north side at 65° from (60, 0) up to (60 + 30 cot 65°, 30): the section at RL z is 60 + z cot 65° wide, so a
    // layer from za to zb holds 500 * (60 (zb - za) + cot 65° (zb² - za²) / 2).
    let c = 65f64.to_radians().tan().recip();
    let solid = along_strike(0.0, 500.0, &[(0.0, 0.0), (60.0, 0.0), (60.0 + 30.0 * c, 30.0), (0.0, 30.0)]);
    let got = by_layer(&cut(&solid, &three_seams()), 7);
    let rls = [30.0, 25.0, 23.0, 18.0, 15.0, 8.0, 5.0, 0.0];
    for layer in 0..7 {
        let (zb, za) = (rls[layer], rls[layer + 1]);
        let want = 500.0 * (60.0 * (zb - za) + c * (zb * zb - za * za) / 2.0);
        assert!(near(got[layer], want, 0.001), "layer {layer}: {} against {want}", got[layer]);
    }
}

#[test]
fn a_seam_lying_on_its_floor_over_half_the_strip_leaves_coal_only_where_it_is() {
    // U is 2 m thick to x = 240 and thins straight to nothing at x = 250, its roof on its floor (23) beyond. It's
    // thicker than 0.01 m to x = 249.95: 60 (2 * 240 + 2 * 9.95 - 0.1 * 9.95²) = 29,399.985 m³ of coal. The wedge from
    // 0.01 m to nothing over the last 0.05 m is left out: 0.5 * 0.01 * 0.05 * 60 = 0.015 m³. The overburden reaches
    // down to the roof: 60 (5 * 240 + (70 - 10) + 7 * 250) = 180,600 m³.
    let thickness = |x: f64| if x <= 240.0 { 2.0 } else if x >= 250.0 { 0.0 } else { 2.0 - 0.2 * (x - 240.0) };
    let mut surfaces = three_seams();
    surfaces[0] = grid(|x| 23.0 + thickness(x));
    let cut = cut(&strip(0.0, 500.0, 0.0, 30.0), &surfaces);
    let got = by_layer(&cut, 7);
    assert!(near(got[1], 29_399.985, 0.001), "U {}", got[1]);
    assert!(near(got[0], 180_600.0, 0.001), "overburden {}", got[0]);
    assert!(near(cut.thin, 0.015, 0.001), "thin {}", cut.thin);
    assert!(cut.thin < 0.1);
    assert_eq!(cut.pieces.iter().filter(|p| p.layer == 1).map(|p| p.parts).sum::<usize>(), 1);
    assert!(cut.difference().abs() < 0.001);
}

#[test]
fn an_absent_seam_leaves_its_waste_with_the_seam_below() {
    // M is null from x = 300 east: laid on U's floor there, so U-M and M have nothing and M-L runs from 23 to 8. Across
    // the cell from 290 to 300 M-L thickens from 7 to 15: M-L = 60 (7 * 290 + 110 + 15 * 200) = 308,400 m³.
    let mut surfaces = three_seams();
    for s in [2, 3] {
        surfaces[s] = grid(|x| if x >= 300.0 { f64::NAN } else { [18.0, 15.0][s - 2] });
    }
    let horizons: Vec<(&[f64], &[f64])> = surfaces.chunks(2).map(|p| (p[0].as_slice(), p[1].as_slice())).collect();
    let stack = layer_stack(LATTICE, &horizons).unwrap();
    assert_eq!(stack.absent, 26 * 21, "x = 300 to 550 on every row");
    assert_eq!(stack.lowered, 0);
    let cut = cut_solid_by_layers(&strip(0.0, 500.0, 0.0, 30.0), &stack, 0.01).unwrap();
    let got = by_layer(&cut, 7);
    assert!(near(got[4], 308_400.0, 0.001), "M-L {}", got[4]);
    assert!(got[2] < 60.0 * (5.0 * 290.0 + 25.0) + 0.001 && got[3] < 60.0 * (3.0 * 290.0 + 15.0) + 0.001);
    assert!(near(got[0] + got[1], 7.0 * 30_000.0, 0.001), "U and above unchanged");
    // Left out where U-M and M thin past 0.01 m across that cell: wedges 0.01 m high and 10 * 0.01 / 5 and 10 * 0.01 / 3
    // m wide, 60 m long: 0.006 + 0.01 m³.
    assert!(near(cut.thin, 0.016, 1e-6), "{}", cut.thin);
    assert!(cut.difference().abs() < 0.001);
}

#[test]
fn a_top_seam_absent_lets_the_overburden_reach_the_next() {
    // U is null everywhere: laid on M's roof, so the overburden runs from 30 to 18.
    let mut surfaces = three_seams();
    surfaces[0] = flat(f64::NAN);
    surfaces[1] = flat(f64::NAN);
    let got = by_layer(&cut(&strip(0.0, 500.0, 0.0, 30.0), &surfaces), 7);
    assert!(near(got[0], 12.0 * 30_000.0, 0.001), "{}", got[0]);
    assert_eq!((got[1], got[2]), (0.0, 0.0));
}

#[test]
fn what_lies_off_the_grids_is_kept_whole() {
    // The strip starts 100 m west of E; the grids 50 m west: 50 x 60 x 30 = 90,000 m³ off them.
    let cut = cut(&strip(-100.0, 500.0, 0.0, 30.0), &three_seams());
    let outside = cut.outside.as_ref().expect("a piece off the grids");
    assert!(near(outside.volume, 90_000.0, 0.001), "{}", outside.volume);
    assert!(near(cut.pieces.iter().map(|p| p.volume).sum::<f64>(), 550.0 * 60.0 * 30.0, 0.001));
    assert!(cut.difference().abs() < 0.001);
}

#[test]
fn a_solid_standing_on_the_last_floor_has_nothing_below_it() {
    // The strip's bottom is L's floor, RL 5: as a pit solid made down to the lowest floor is.
    let cut = cut(&strip(0.0, 500.0, 5.0, 30.0), &three_seams());
    let got = by_layer(&cut, 7);
    assert!(near(got[5], 90_000.0, 0.001), "L {}", got[5]);
    assert_eq!(got[6], 0.0);
    assert!(cut.thin.abs() < 0.001 && cut.difference().abs() < 0.001);
}

#[test]
fn surfaces_crossing_within_the_tolerance_are_lowered_and_counted() {
    // M's roof 1 mm above U's floor on the east half.
    let mut surfaces = three_seams();
    surfaces[2] = grid(|x| if x >= 250.0 { 23.001 } else { 18.0 });
    let horizons: Vec<(&[f64], &[f64])> = surfaces.chunks(2).map(|p| (p[0].as_slice(), p[1].as_slice())).collect();
    let stack = layer_stack(LATTICE, &horizons).unwrap();
    assert_eq!(stack.lowered, 31 * 21);
    assert!(near(stack.most_lowered, 0.001, 1e-9));
}

/// Holes at these nodes (col, row) of the lattice.
fn holes_at(nodes: &[(usize, usize)]) -> Vec<bool> {
    let mut holes = vec![false; LATTICE.cols * LATTICE.rows];
    for &(c, r) in nodes {
        holes[r * LATTICE.cols + c] = true;
    }
    holes
}

#[test]
fn grids_over_the_whole_solid_with_no_hole_under_it_cover_it() {
    let started = std::time::Instant::now();
    let cover = solid_off_the_grids(&strip(0.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[])).unwrap();
    assert!(cover.covered(), "{cover:?}");
    println!("checked in {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
    // A hole well away from the solid, east of it and north, doesn't count.
    assert!(solid_off_the_grids(&strip(0.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[(57, 15)])).unwrap().covered());
}

#[test]
fn a_solid_past_the_grids_edge_is_found_with_the_area_past_it() {
    // The strip starts 100 m west of E, the grids 50 m west: 50 x 60 m past their edge, about (E - 75, N + 30).
    let cover = solid_off_the_grids(&strip(-100.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[])).unwrap();
    assert_eq!(cover.past_edge.len(), 1, "{cover:?}");
    assert!(near(cover.past_edge[0].area, 3_000.0, 1e-6), "{:?}", cover.past_edge);
    assert!(near(cover.past_edge[0].at[0], E - 75.0, 1e-6) && near(cover.past_edge[0].at[1], N + 30.0, 1e-6), "{:?}", cover.past_edge);
    assert!(cover.over_holes.is_empty());
}

#[test]
fn a_hole_under_the_solid_takes_the_four_cells_round_it_and_one_on_its_edge_takes_none() {
    // A hole at node (E + 50, N + 30), inside the strip: the four 10 m cells round it, 400 m² about the node.
    let cover = solid_off_the_grids(&strip(0.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[(10, 8)])).unwrap();
    assert!(cover.past_edge.is_empty());
    assert_eq!(cover.over_holes.len(), 1, "{cover:?}");
    assert!(near(cover.over_holes[0].area, 400.0, 1e-6), "{:?}", cover.over_holes);
    assert!(near(cover.over_holes[0].at[0], E + 50.0, 1e-6) && near(cover.over_holes[0].at[1], N + 30.0, 1e-6), "{:?}", cover.over_holes);
    // At (E + 50, N): half its cells are under the strip, 200 m².
    let cover = solid_off_the_grids(&strip(0.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[(10, 5)])).unwrap();
    assert!(near(cover.over_holes.iter().map(|u| u.area).sum::<f64>(), 200.0, 1e-6), "{:?}", cover.over_holes);
    // At (E + 50, N + 70), 10 m north of the strip: its cells only touch it.
    assert!(solid_off_the_grids(&strip(0.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[(10, 12)])).unwrap().covered());
    // Two holes side by side make one part: six cells, 600 m².
    let cover = solid_off_the_grids(&strip(0.0, 500.0, 0.0, 30.0), LATTICE, &holes_at(&[(10, 8), (11, 8)])).unwrap();
    assert_eq!(cover.over_holes.len(), 1, "{cover:?}");
    assert!(near(cover.over_holes[0].area, 600.0, 1e-6), "{:?}", cover.over_holes);
}

#[test]
fn holes_in_a_grid_are_told_apart_largest_first() {
    // Three nodes joined corner to corner about (E + 50, N + 30), and one alone at (E + 300, N + 100).
    let holes = holes_at(&[(9, 7), (10, 8), (11, 9), (35, 15)]);
    let found = grid_hole_patches(LATTICE, &holes);
    assert_eq!(found.iter().map(|h| h.nodes).collect::<Vec<_>>(), [3, 1]);
    assert_eq!(found[0].at, [E + 50.0, N + 30.0]);
    assert_eq!(found[1].at, [E + 300.0, N + 100.0]);
    assert!(grid_hole_patches(LATTICE, &holes_at(&[])).is_empty());
}
