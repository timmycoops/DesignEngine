//! One face a strip, and strip solids (decision 0049): known answers on a box at mine-grid coordinates.

use std::time::Instant;

use zd_core::TriMesh;
use zd_geom::{FaceShort, Shortfall, SliceRefusal, faces_through_solid, join_faces, shortfall_words, slice_solid_by_faces};

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
    }
    assert!(faces_through_solid(&pit(), &refs).unwrap().is_empty(), "every face passes right through the pit");
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

/// The shortfalls a slice refuses, which [`faces_through_solid`] finds the same without cutting.
fn refused(faces: &[&TriMesh]) -> Vec<FaceShort> {
    let found = faces_through_solid(&pit(), faces).unwrap();
    match slice_solid_by_faces(&pit(), faces) {
        Err(SliceRefusal::Short(shorts)) => {
            assert_eq!(shorts, found, "the check finds what the slice refuses");
            shorts
        }
        other => panic!("refused as short, not {other:?}"),
    }
}

#[test]
fn a_face_short_of_the_floor_or_the_top_is_refused_with_how_far() {
    // A face from its toe at RL 20 only, 300 m east: it stands 20 m above the pit's floor.
    let toe_up = face(&[[300.0, 20.0], [430.0, 150.0]], -100.0, 600.0, 2);
    let shorts = refused(&[&batter_45(100.0), &toe_up]);
    assert_eq!(shorts.len(), 1, "{shorts:?}");
    assert_eq!(shorts[0].face, 1);
    let Shortfall::Toe { by, at } = shorts[0].shortfall else { panic!("its toe: {shorts:?}") };
    assert!((by - 20.0).abs() < 1e-9 && (at[0] - (E + 300.0)).abs() < 1e-9, "{by} m at {at:?}");
    assert!(shortfall_words(&shorts[0].shortfall).contains("toe stands up to 20.00 m above the solid's floor"));

    // A face up to RL 60 only: its top is 40 m below the pit's.
    let low = face(&[[250.0, -50.0], [300.0, 0.0], [360.0, 60.0]], -100.0, 600.0, 2);
    let shorts = refused(&[&low]);
    let [FaceShort { face: 0, shortfall: Shortfall::Top { by, .. } }] = shorts.as_slice() else { panic!("its top: {shorts:?}") };
    assert!((by - 40.0).abs() < 1e-9, "{by} m");
}

#[test]
fn a_face_that_stops_on_the_solids_top_and_floor_is_refused_too() {
    // From its toe on the pit's floor at 300 m east up at 45° to the pit's top, as a face built to the surfaces the pit
    // was made between is: it meets the pit's top and floor, and goes no further.
    let flush = face(&[[300.0, 0.0], [400.0, 100.0]], -100.0, 600.0, 2);
    let shorts = refused(&[&flush]);
    assert_eq!(shorts.len(), 2, "its top and its toe: {shorts:?}");
    let words: Vec<String> = shorts.iter().map(|s| shortfall_words(&s.shortfall)).collect();
    assert!(words[0].contains("its top stops on the solid's top") && words[0].contains("carry the face on above the solid"), "{words:?}");
    assert!(words[1].contains("its toe stops on the solid's floor") && words[1].contains("carry the face on below the solid"), "{words:?}");
    // Carried on 1 m past each, it slices: 500 x the integral of (300 + z) over RL 0 to 100.
    let through = face(&[[299.0, -1.0], [401.0, 101.0]], -100.0, 600.0, 2);
    let slice = slice_solid_by_faces(&pit(), &[&through]).unwrap();
    assert_volume(slice.strips[0].volume, 500.0 * (300.0 * 100.0 + 5000.0), "in front of it");
}

#[test]
fn a_face_short_of_the_pit_at_an_end_is_refused_with_the_run_on_it_needs() {
    // Starts 100 m north of the pit's south side.
    let short = face(&[[250.0, -50.0], [300.0, 0.0], [450.0, 150.0]], 100.0, 600.0, 4);
    let shorts = refused(&[&batter_45(100.0), &short]);
    let [FaceShort { face: 1, shortfall: Shortfall::End(end) }] = shorts.as_slice() else { panic!("its end: {shorts:?}") };
    let more = end.run_on.expect("its end is found");
    // 100 m back to the pit's side, and the 3 m margin past it.
    assert!((100.0..=104.0).contains(&more), "{more} m");
    assert!((end.at[1] - (N + 100.0)).abs() < 1e-6, "at its south end: {:?}", end.at);
}

#[test]
fn every_shortfall_of_every_face_is_found_at_once() {
    // Short at both ends (100 m in from each side), and its toe 10 m above the floor; and a second face whose top is
    // 30 m below the pit's.
    let ends = face(&[[300.0, 10.0], [450.0, 160.0]], 100.0, 400.0, 3);
    let low = face(&[[550.0, -50.0], [600.0, 0.0], [670.0, 70.0]], -100.0, 600.0, 2);
    let start = Instant::now();
    let shorts = refused(&[&batter_45(100.0), &ends, &low]);
    println!("checked in {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    let kinds: Vec<(usize, &str)> = shorts
        .iter()
        .map(|s| {
            let kind = match s.shortfall {
                Shortfall::End(_) => "end",
                Shortfall::Top { .. } => "top",
                Shortfall::Toe { .. } => "toe",
            };
            (s.face, kind)
        })
        .collect();
    assert_eq!(kinds, vec![(1, "end"), (1, "end"), (1, "toe"), (2, "top")]);
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

#[test]
fn a_slice_says_how_far_it_has_got_and_stops_when_told() {
    let faces: Vec<TriMesh> = (0..4).map(|k| batter_45(200.0 + 150.0 * k as f64)).collect();
    let refs: Vec<&TriMesh> = faces.iter().collect();
    let mut said = Vec::new();
    let slice = zd_geom::slice_solid_by_faces_watched(&pit(), &refs, |done, of| {
        said.push((done, of));
        true
    })
    .unwrap();
    assert_eq!(said, vec![(0, 4), (0, 4), (1, 4), (2, 4), (3, 4)], "once before closing the faces, then before each strip");
    assert_eq!(slice.strips.len(), 5);
    // Told to stop before the third strip: nothing comes back but the refusal.
    let stopped = zd_geom::slice_solid_by_faces_watched(&pit(), &refs, |done, _| done < 2);
    assert!(matches!(stopped, Err(SliceRefusal::Stopped)), "{stopped:?}");
}

/// A face's own detail finer than the join's tolerance stays as its face made it: the weld is for where faces meet. An up
/// face holding two corners 0.4 mm apart (as a berm laid on its grid does where a grid line passes that close to its
/// corner, 0064) joins a down face at a 1 mm tolerance with both kept; only the two toe corners the faces share weld.
#[test]
fn a_faces_own_detail_finer_than_the_tolerance_is_not_welded() {
    let at = |x: f64, z: f64| [E + x, N, z];
    let down = TriMesh::new(vec![at(0.0, 0.0), at(10.0, 0.0), at(10.0, -10.0), at(0.0, -10.0)], vec![[0, 2, 1], [0, 3, 2]]);
    // The up face: corners c0 to c3, and p and q 0.4 mm apart in the middle, with the two slivers between them.
    let (p, q) = (at(5.0, 5.0), at(5.0004, 5.0));
    let up = TriMesh::new(
        vec![at(0.0, 0.0), at(10.0, 0.0), at(10.0, 10.0), at(0.0, 10.0), p, q],
        vec![[0, 1, 5], [0, 5, 4], [1, 2, 5], [2, 3, 4], [2, 4, 5], [3, 0, 4]],
    );
    let (joined, log) = join_faces(&[&down, &up], 0.001).unwrap();
    assert_eq!(log.weld.vertices_welded, 2, "the two toe corners, nothing inside a face: {log:?}");
    assert!(joined.vertices.contains(&p) && joined.vertices.contains(&q), "both kept where the face put them");
    assert_eq!(joined.triangles.len(), 8, "the slivers too");
    // Area tolerance 1e-9 m²: 100 m² down and 100 m² up, one sheet.
    let area: f64 = joined.triangles.iter().map(|t| {
        let [a, b, c] = t.map(|i| joined.vertices[i as usize]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        0.5 * ((u[1] * v[2] - u[2] * v[1]).powi(2) + (u[2] * v[0] - u[0] * v[2]).powi(2) + (u[0] * v[1] - u[1] * v[0]).powi(2)).sqrt()
    }).sum();
    assert!((area - 200.0).abs() < 1e-9, "{area}");
    assert_eq!(zd_geom::boundary_loops(&joined).unwrap().len(), 1);
}
