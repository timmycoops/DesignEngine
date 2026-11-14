//! Solids cut at bench levels (decision 0071), with volumes worked by hand at mine grid coordinates. Volumes to 1 part
//! in a million unless said.

use zd_core::TriMesh;
use zd_geom::{BenchCut, bench_levels, cut_solid_by_levels};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

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
    outward(TriMesh::new(vertices, triangles))
}

fn outward(mesh: TriMesh) -> TriMesh {
    if mesh.signed_volume() < 0.0 { TriMesh::new(mesh.vertices, mesh.triangles.iter().map(|t| [t[0], t[2], t[1]]).collect()) } else { mesh }
}

/// A block 100 m east by 60 m north from RL `z0` to `z1`.
fn block(z0: f64, z1: f64) -> TriMesh {
    along_strike(0.0, 100.0, &[(0.0, z0), (60.0, z0), (60.0, z1), (0.0, z1)])
}

fn levels(from: f64, to: f64, every: f64) -> Vec<f64> {
    bench_levels(from, to, every).unwrap()
}

/// Each piece as (floor, top, volume), bottom up.
fn benches(cut: &BenchCut) -> Vec<(Option<f64>, Option<f64>, f64)> {
    cut.pieces.iter().map(|p| (p.floor, p.top, p.volume)).collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn assert_benches(cut: &BenchCut, expected: &[(Option<f64>, Option<f64>, f64)]) {
    let got = benches(cut);
    assert_eq!(got.len(), expected.len(), "{got:?}");
    for (g, e) in got.iter().zip(expected) {
        assert_eq!((g.0, g.1), (e.0, e.1), "{got:?}");
        assert!(near(g.2, e.2), "{} m³ against {} m³: {got:?}", g.2, e.2);
    }
    assert!(cut.difference().abs() <= 1e-6 * cut.input_volume, "adds up: {}", cut.difference());
    for p in &cut.pieces {
        assert!(p.mesh.validity().is_watertight(), "each piece watertight");
    }
}

#[test]
fn levels_run_from_the_from_rl_up_to_the_to_rl() {
    let all = levels(0.0, 500.0, 10.0);
    assert_eq!(all.len(), 51);
    assert_eq!((all[0], all[50]), (0.0, 500.0));
    assert_eq!(levels(0.0, 25.0, 10.0), vec![0.0, 10.0, 20.0], "to the last level under the to RL");
    assert_eq!(levels(-15.0, 15.0, 7.5), vec![-15.0, -7.5, 0.0, 7.5, 15.0]);
    assert_eq!(levels(0.3, 0.9, 0.1).len(), 7, "a to RL on the levels by its figures is one despite rounding");
    assert!(bench_levels(10.0, 0.0, 5.0).is_err());
    assert!(bench_levels(0.0, 10.0, 0.0).is_err());
    assert!(bench_levels(0.0, 1e9, 1e-3).is_err());
}

/// A block from RL 0 to 30 cut every 10 m: three benches of 60,000 m³, floors 0, 10 and 20. The levels at its floor and
/// top (0 and 30) don't cut it.
#[test]
fn a_block_cut_every_10_m_into_three_benches() {
    let cut = cut_solid_by_levels(&block(0.0, 30.0), &levels(0.0, 500.0, 10.0), 0.01).unwrap();
    assert_benches(&cut, &[(Some(0.0), Some(10.0), 60_000.0), (Some(10.0), Some(20.0), 60_000.0), (Some(20.0), Some(30.0), 60_000.0)]);
    assert_eq!(cut.levels_cut, 2);
    assert_eq!(cut.flapped, 0);
    println!("{} triangles in, {} out, {:.2} ms", block(0.0, 30.0).triangle_count(), cut.pieces.iter().map(|p| p.mesh.triangle_count()).sum::<usize>(), cut.ms);
}

/// A block from RL -5 to 37 against levels 0 to 30: 5 m below the from RL (no floor), three whole benches, and 7 m above
/// the highest level (no top).
#[test]
fn ground_below_the_from_rl_and_above_the_to_rl_is_a_piece_each() {
    let cut = cut_solid_by_levels(&block(-5.0, 37.0), &levels(0.0, 30.0, 10.0), 0.01).unwrap();
    assert_benches(
        &cut,
        &[
            (None, Some(0.0), 30_000.0),
            (Some(0.0), Some(10.0), 60_000.0),
            (Some(10.0), Some(20.0), 60_000.0),
            (Some(20.0), Some(30.0), 60_000.0),
            (Some(30.0), None, 42_000.0),
        ],
    );
    // Wholly within one bench: one piece, labelled.
    let within = cut_solid_by_levels(&block(12.0, 18.0), &levels(0.0, 30.0, 10.0), 0.01).unwrap();
    assert_benches(&within, &[(Some(10.0), Some(20.0), 36_000.0)]);
    assert_eq!(within.levels_cut, 0);
    // Wholly above every level.
    let above = cut_solid_by_levels(&block(40.0, 41.0), &levels(0.0, 30.0, 10.0), 0.01).unwrap();
    assert_benches(&above, &[(Some(30.0), None, 6_000.0)]);
}

/// A strip with 45° walls both sides, 60 m wide at RL 0 and 20 m at RL 20, 100 m long: its benches are trapezoids,
/// 50 m wide on average in the first (50,000 m³) and 30 m in the second (30,000 m³).
#[test]
fn a_battered_strip_keeps_its_batter_in_every_bench() {
    let strip = along_strike(0.0, 100.0, &[(0.0, 0.0), (60.0, 0.0), (40.0, 20.0), (20.0, 20.0)]);
    let cut = cut_solid_by_levels(&strip, &levels(0.0, 100.0, 10.0), 0.01).unwrap();
    assert_benches(&cut, &[(Some(0.0), Some(10.0), 50_000.0), (Some(10.0), Some(20.0), 30_000.0)]);
}

/// A seam floor dipping 1 in 10 east under a flat top at RL 30: from RL 0 at the west end to RL 10 at the east. The
/// bench from 0 to 10 is the wedge above the floor, (10 × 100 − 0.1 × 100² / 2) × 60 = 30,000 m³; the two above are
/// whole, 60,000 m³ each. Where the wedge thins under 1 cm at the east end, a knife edge, it's a flap (decision 0064):
/// cut out 5 mm further, from x = 99.895 m, 0.1 × 0.105² / 2 × 60 = 0.033075 m³, taken off and said.
#[test]
fn a_floor_dipping_through_a_level_leaves_a_wedge_and_its_knife_edge_is_a_flap() {
    let vertices = vec![
        [E, N, 0.0],
        [E + 100.0, N, 10.0],
        [E + 100.0, N + 60.0, 10.0],
        [E, N + 60.0, 0.0],
        [E, N, 30.0],
        [E + 100.0, N, 30.0],
        [E + 100.0, N + 60.0, 30.0],
        [E, N + 60.0, 30.0],
    ];
    let triangles = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    let wedge = outward(TriMesh::new(vertices, triangles));
    assert!(near(wedge.signed_volume(), 150_000.0));
    let knife = 0.1 * 0.105_f64.powi(2) / 2.0 * 60.0;
    let cut = cut_solid_by_levels(&wedge, &levels(0.0, 30.0, 10.0), 0.01).unwrap();
    assert_benches(&cut, &[(Some(0.0), Some(10.0), 30_000.0 - knife), (Some(10.0), Some(20.0), 60_000.0), (Some(20.0), Some(30.0), 60_000.0)]);
    assert_eq!(cut.flapped, 1);
    assert!((cut.flaps - knife).abs() < 1e-6, "{} against {knife}", cut.flaps);
    let whole = cut_solid_by_levels(&wedge, &levels(0.0, 30.0, 10.0), 0.0).unwrap();
    assert_benches(&whole, &[(Some(0.0), Some(10.0), 30_000.0), (Some(10.0), Some(20.0), 60_000.0), (Some(20.0), Some(30.0), 60_000.0)]);
}

/// A block whose top is 4 mm over a level at RL 30: the sheet above it, 4 mm × 100 × 60 = 24 m³, is a flap, taken off
/// and said; with no flap thickness it's a piece.
#[test]
fn a_top_a_few_millimetres_over_a_level_leaves_a_flap_taken_off() {
    let solid = block(0.0, 30.004);
    let cut = cut_solid_by_levels(&solid, &levels(0.0, 30.0, 10.0), 0.01).unwrap();
    assert_eq!(cut.pieces.len(), 3, "{:?}", benches(&cut));
    assert_eq!(cut.flapped, 1);
    assert!((cut.flaps - 24.0).abs() < 0.001, "{}", cut.flaps);
    assert!(cut.difference().abs() < 1e-6 * cut.input_volume);
    let kept = cut_solid_by_levels(&solid, &levels(0.0, 30.0, 10.0), 0.0).unwrap();
    assert_eq!(kept.pieces.len(), 4);
    assert!((kept.pieces[3].volume - 24.0).abs() < 0.001, "{}", kept.pieces[3].volume);
    assert_eq!((kept.pieces[3].floor, kept.pieces[3].top), (Some(30.0), None));
}

#[test]
fn levels_out_of_order_are_refused() {
    assert!(cut_solid_by_levels(&block(0.0, 30.0), &[10.0, 5.0], 0.01).is_err());
    assert!(cut_solid_by_levels(&block(0.0, 30.0), &[10.0, 10.0], 0.01).is_err());
    assert!(cut_solid_by_levels(&block(0.0, 30.0), &[f64::NAN], 0.01).is_err());
}

/// A strip 1,500 m long cut every 1 m from RL 0 to 30: thirty benches of 90,000 m³. Timed.
#[test]
fn a_long_strip_cut_every_metre() {
    let strip = along_strike(0.0, 1500.0, &[(0.0, 0.0), (60.0, 0.0), (60.0, 30.0), (0.0, 30.0)]);
    let cut = cut_solid_by_levels(&strip, &levels(0.0, 30.0, 1.0), 0.01).unwrap();
    assert_eq!(cut.pieces.len(), 30);
    for p in &cut.pieces {
        assert!(near(p.volume, 90_000.0), "{}", p.volume);
    }
    println!("{} triangles in, {} out, {:.1} ms", strip.triangle_count(), cut.pieces.iter().map(|p| p.mesh.triangle_count()).sum::<usize>(), cut.ms);
}
