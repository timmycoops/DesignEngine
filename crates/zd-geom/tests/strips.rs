//! One face a strip, and strip solids (decision 0049): known answers on a box at mine-grid coordinates.

use std::time::Instant;

use zd_core::TriMesh;
use zd_geom::{SliceRefusal, join_faces, slice_solid_by_faces};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// The pit solid: 1000 m east, 500 m north, RL 0 to 100, at mine-grid coordinates. 50,000,000 m³.
fn pit() -> TriMesh {
    let ([x0, y0, z0], [x1, y1, z1]) = ([E, N, 0.0], [E + 1000.0, N + 500.0, 100.0]);
    let vertices = vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
    let triangles = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    TriMesh::new(vertices, triangles)
}

/// A face across the pit from `y0` to `y1` m north, through the section points `(east, RL)` from the pit side out,
/// in `rows` rows along it. It rises away from the pit, to the east.
fn face(section: &[[f64; 2]], y0: f64, y1: f64, rows: usize) -> TriMesh {
    let mut vertices = Vec::new();
    for r in 0..=rows {
        let y = y0 + (y1 - y0) * r as f64 / rows as f64;
        vertices.extend(section.iter().map(|p| [E + p[0], N + y, p[1]]));
    }
    let n = section.len() as u32;
    let mut triangles = Vec::new();
    for r in 0..rows as u32 {
        for i in 0..n - 1 {
            let (a, b, c, d) = (r * n + i, r * n + i + 1, (r + 1) * n + i + 1, (r + 1) * n + i);
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    TriMesh::new(vertices, triangles)
}

/// A 45° face from its toe at RL 0, `toe` m east, down to RL -50 and up to RL 150, run on 100 m past the pit's ends.
fn batter_45(toe: f64) -> TriMesh {
    face(&[[toe - 50.0, -50.0], [toe, 0.0], [toe + 150.0, 150.0]], -100.0, 600.0, 4)
}

/// Volume tolerance: 1 m³ in the 50,000,000 m³ pit (1 part in 50 million).
fn assert_volume(actual: f64, expected: f64, what: &str) {
    assert!((actual - expected).abs() < 1.0, "{what}: {actual:.3} m³ against {expected:.3} m³");
}

#[test]
fn fifteen_45_degree_faces_slice_the_pit_into_sixteen_strips() {
    // Toes 50 m apart from 100 m east, given out of order: the slice orders them by what's in front of each.
    let toes: Vec<f64> = (0..15).map(|k| 100.0 + 50.0 * k as f64).collect();
    let mut given: Vec<usize> = (0..15).collect();
    given.reverse();
    given.swap(3, 9);
    let faces: Vec<TriMesh> = given.iter().map(|&k| batter_45(toes[k])).collect();
    let refs: Vec<&TriMesh> = faces.iter().collect();
    let start = Instant::now();
    let slice = slice_solid_by_faces(&pit(), &refs).unwrap();
    let ms = start.elapsed().as_secs_f64() * 1e3;
    let triangles_out: usize = slice.strips.iter().map(|s| s.mesh.triangle_count()).sum();
    println!(
        "12 + {} triangles in, {triangles_out} out, {ms:.0} ms (close {:.0}, strips {:.0}, from Manifold {:.0})",
        faces.iter().map(TriMesh::triangle_count).sum::<usize>(),
        slice.timings.close_ms,
        slice.timings.strips_ms,
        slice.timings.from_manifold_ms
    );

    let order: Vec<usize> = slice.order.iter().map(|&i| given[i]).collect();
    assert_eq!(order, (0..15).collect::<Vec<_>>(), "faces from the pit out");
    assert_eq!(slice.strips.len(), 16);
    // In front of the first toe at 100 m: 500 x the integral of (100 + z) over RL 0 to 100.
    assert_volume(slice.strips[0].volume, 7_500_000.0, "strip 1");
    for k in 1..15 {
        assert_volume(slice.strips[k].volume, 50.0 * 500.0 * 100.0, &format!("strip {}", k + 1));
    }
    assert_volume(slice.strips[15].volume, 7_500_000.0, "behind the last face");
    let total: f64 = slice.strips.iter().map(|s| s.volume).sum();
    assert_volume(total, 50_000_000.0, "the strips together");
    assert_volume(slice.input_volume, 50_000_000.0, "the pit");
    for (k, strip) in slice.strips.iter().enumerate() {
        assert!(strip.validity.is_watertight(), "strip {} isn't watertight: {:?}", k + 1, strip.validity);
        assert_eq!(strip.parts, 1, "strip {}", k + 1);
    }
    assert!(slice.crossings.is_empty());
    for (i, cut) in slice.faces.iter().enumerate() {
        let toe = toes[given[i]];
        assert_volume(cut.front, 500.0 * (100.0 * toe + 5000.0), &format!("in front of the face at {toe}"));
        assert!(cut.below_lowest.is_none() && cut.top_below.is_none());
    }
}

#[test]
fn a_face_with_a_berm_has_its_known_volume_in_front() {
    // Toe at 200 m east: a 45° wall to RL 40, a 20 m berm, a 45° wall on to RL 150; and down at 45° to RL -50.
    let section = [[150.0, -50.0], [200.0, 0.0], [240.0, 40.0], [260.0, 40.0], [370.0, 150.0]];
    let berm = face(&section, -100.0, 600.0, 3);
    let slice = slice_solid_by_faces(&pit(), &[&berm]).unwrap();
    // 500 x (integral of 200 + z over 0 to 40, plus 220 + z over 40 to 100).
    let front = 500.0 * (8_000.0 + 800.0 + 13_200.0 + 4_200.0);
    assert_volume(slice.strips[0].volume, front, "in front of the berm face");
    assert_volume(slice.faces[0].front, front, "the face on its own");
    assert_volume(slice.strips[1].volume, 50_000_000.0 - front, "behind it");
}

#[test]
fn below_the_lowest_line_the_cut_goes_straight_down_and_says_how_much() {
    // A face from its toe at RL 20 only: below RL 20 the cut is straight down under the toe at 300 m east.
    let short = face(&[[300.0, 20.0], [430.0, 150.0]], -100.0, 600.0, 2);
    let slice = slice_solid_by_faces(&pit(), &[&short]).unwrap();
    // 500 x (300 x 20 below the toe, and the integral of 300 + (z - 20) over 20 to 100).
    let front = 500.0 * (300.0 * 20.0 + 300.0 * 80.0 + 3_200.0);
    assert_volume(slice.strips[0].volume, front, "in front");
    let (rl, below) = slice.faces[0].below_lowest.expect("the pit runs below the face");
    assert_eq!(rl, 20.0);
    assert_volume(below, 1000.0 * 500.0 * 20.0, "the pit below RL 20");
}

#[test]
fn a_face_short_of_the_pit_at_an_end_is_refused_with_the_run_on_it_needs() {
    // Starts 100 m north of the pit's south side.
    let short = face(&[[250.0, -50.0], [300.0, 0.0], [450.0, 150.0]], 100.0, 600.0, 4);
    match slice_solid_by_faces(&pit(), &[&batter_45(100.0), &short]) {
        Err(SliceRefusal::Short { face, end }) => {
            assert_eq!(face, 1);
            let more = end.run_on.expect("its end is found");
            // 100 m back to the pit's side, and the 3 m margin past it.
            assert!((100.0..=104.0).contains(&more), "{more} m");
            assert!((end.at[1] - (N + 100.0)).abs() < 1e-6, "at its south end: {:?}", end.at);
        }
        other => panic!("refused as short, not {other:?}"),
    }
}

#[test]
fn faces_crossing_inside_the_pit_are_sliced_and_reported() {
    // A: 45° from its toe at 300 m. B: 80° from its toe at 330 m. They cross at RL 30 / (1 - cot 80°).
    let a = batter_45(300.0);
    let cot = 1.0 / 80f64.to_radians().tan();
    let b = face(&[[330.0 - 50.0 * cot, -50.0], [330.0, 0.0], [330.0 + 150.0 * cot, 150.0]], -100.0, 600.0, 4);
    let slice = slice_solid_by_faces(&pit(), &[&a, &b]).unwrap();
    assert_eq!(slice.order, vec![1, 0], "B has less in front of it");
    let rl = 30.0 / (1.0 - cot);
    // In front of B and behind A: 500 x the integral of (30 - (1 - cot) z) up to where they cross.
    let crossed = 500.0 * (30.0 * rl - (1.0 - cot) * rl * rl / 2.0);
    assert_eq!(slice.crossings.len(), 1);
    assert_eq!(slice.crossings[0].face, 0);
    assert_volume(slice.crossings[0].volume, crossed, "in front of B, behind A");
    assert!(slice.crossings[0].at[2] < rl, "below where they cross: {:?}", slice.crossings[0].at);
    let total: f64 = slice.strips.iter().map(|s| s.volume).sum();
    assert_volume(total, 50_000_000.0, "the strips still add up");
}

#[test]
fn an_up_and_a_down_face_join_into_one() {
    // Down from the toe at 200 m to RL -50 in 3 rows, up to RL 150 in 5 rows: the toe has different vertices each side.
    let down = face(&[[150.0, -50.0], [200.0, 0.0]], -100.0, 600.0, 3);
    let up = face(&[[200.0, 0.0], [350.0, 150.0]], -100.0, 600.0, 5);
    let (joined, log) = join_faces(&[&down, &up], 0.001).unwrap();
    assert_eq!(log.faces, 2);
    // Each side's inner toe corners lie on the other's toe edges: 2 one way, 4 the other.
    assert_eq!(log.stitched, 6);
    assert!(log.filled.is_empty());
    assert_eq!(zd_geom::boundary_loops(&joined).unwrap().len(), 1);
    let area = |m: &TriMesh| m.area();
    assert!((area(&joined) - area(&down) - area(&up)).abs() < 1e-4, "area kept to 1 cm²");
    let v = joined.validity();
    assert!(v.consistent_winding && v.non_manifold_edges == 0 && v.inconsistent_edges == 0, "{v:?}");
    // And it slices as the 45° face does.
    let slice = slice_solid_by_faces(&pit(), &[&joined]).unwrap();
    assert_volume(slice.strips[0].volume, 500.0 * (100.0 * 200.0 + 5000.0), "in front of the joined face");
}

#[test]
fn a_gap_the_faces_close_all_round_is_filled() {
    // Up and down faces leaving a 10 m hole in the toe between 200 m and 210 m north: two pieces of up face either side.
    let down = face(&[[150.0, -50.0], [200.0, 0.0]], -100.0, 600.0, 70);
    let mut ups = Vec::new();
    for (y0, y1, rows) in [(-100.0, 200.0, 30), (210.0, 600.0, 39)] {
        ups.push(face(&[[200.0, 0.0], [350.0, 150.0]], y0, y1, rows));
    }
    let bridge = face(&[[210.0, 10.0], [350.0, 150.0]], 200.0, 210.0, 1);
    let (joined, log) = join_faces(&[&down, &ups[0], &ups[1], &bridge], 0.001).unwrap();
    assert_eq!(log.filled.len(), 1, "{log:?}");
    assert!((log.filled[0].area - 100.0).abs() < 1e-6, "10 m by 10 m in plan: {}", log.filled[0].area);
    assert_eq!(zd_geom::boundary_loops(&joined).unwrap().len(), 1);
}

#[test]
fn faces_that_dont_meet_are_refused_with_how_far_apart() {
    let down = face(&[[150.0, -50.0], [200.0, 0.0]], -100.0, 600.0, 2);
    let up = face(&[[202.0, 0.0], [352.0, 150.0]], -100.0, 600.0, 2);
    let error = join_faces(&[&down, &up], 0.001).unwrap_err().to_string();
    assert!(error.contains("2 pieces") && error.contains("2.00 m apart"), "{error}");
}

/// A surface over the square from `(x0, y0)` to `(x1, y1)` m off the mine-grid origin, `rows` by `rows`, at `z(x, y)`.
fn surface(x0: f64, y0: f64, x1: f64, y1: f64, rows: usize, z: impl Fn(f64, f64) -> f64) -> TriMesh {
    let mut vertices = Vec::new();
    for j in 0..=rows {
        for i in 0..=rows {
            let (x, y) = (x0 + (x1 - x0) * i as f64 / rows as f64, y0 + (y1 - y0) * j as f64 / rows as f64);
            vertices.push([E + x, N + y, z(x, y)]);
        }
    }
    let n = rows as u32 + 1;
    let mut triangles = Vec::new();
    for j in 0..rows as u32 {
        for i in 0..rows as u32 {
            let (a, b, c, d) = (j * n + i, j * n + i + 1, (j + 1) * n + i + 1, (j + 1) * n + i);
            triangles.extend([[a, b, c], [a, c, d]]);
        }
    }
    TriMesh::new(vertices, triangles)
}

#[test]
fn a_solid_between_a_flat_top_and_a_dipping_floor_inside_a_string() {
    // Top RL 100; floor dipping east at 1 in 10 from RL 0; the string a 100 m square from 0, 0. Volume: 100 x 100 x 100
    // less 100 x 0.1 x 100² / 2.
    let top = surface(-20.0, -20.0, 120.0, 120.0, 7, |_, _| 100.0);
    let floor = surface(-20.0, -20.0, 120.0, 120.0, 14, |x, _| 0.1 * x);
    let square = [[E, N], [E + 100.0, N], [E + 100.0, N + 100.0], [E, N + 100.0]];
    let between = zd_geom::solid_between(&top, &floor, &square).unwrap();
    println!("{} + {} triangles in, {} out, {:.1} ms", top.triangle_count(), floor.triangle_count(), between.mesh.triangle_count(), between.ms);
    assert!((between.volume - 950_000.0).abs() < 1e-3, "{} m³", between.volume);
    assert!(between.validity.is_watertight());
    assert_eq!(between.parts, 1);
    assert!((between.area - 10_000.0).abs() < 1e-6);
    assert!(between.top_uncovered < 1e-6 && between.bottom_uncovered < 1e-6);
    // Clockwise, it's the same.
    let clockwise: Vec<[f64; 2]> = square.iter().rev().copied().collect();
    assert!((zd_geom::solid_between(&top, &floor, &clockwise).unwrap().volume - 950_000.0).abs() < 1e-3);
}

#[test]
fn where_the_top_doesnt_cover_the_string_there_is_no_solid_and_its_area_is_said() {
    // The top covers only the west half of the square.
    let top = surface(-20.0, -20.0, 50.0, 120.0, 7, |_, _| 100.0);
    let floor = surface(-20.0, -20.0, 120.0, 120.0, 7, |_, _| 0.0);
    let square = [[E, N], [E + 100.0, N], [E + 100.0, N + 100.0], [E, N + 100.0]];
    let between = zd_geom::solid_between(&top, &floor, &square).unwrap();
    assert!((between.volume - 500_000.0).abs() < 1e-3, "{} m³", between.volume);
    assert!((between.top_uncovered - 5_000.0).abs() < 1e-3, "{} m²", between.top_uncovered);
    assert!(between.bottom_uncovered < 1e-6);
}
