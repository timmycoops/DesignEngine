//! Solids cut by block polygons (decision 0062, workstream 2), with volumes worked by hand at mine grid coordinates.
//! Volumes to 1 part in a million of the solid unless said.

use zd_core::TriMesh;
use zd_geom::{COVER_SLIVER, cut_solid_by_polygons, polygons_overlapping, polygons_uncovered};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// A solid running east from `x0` to `x1` with the same convex section all along: `section` in (north, RL) about
/// `(N, 0)`, anticlockwise looking west. Wound outward.
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

/// A strip 60 m wide (north) and 30 m high with vertical sides.
fn square_strip(x0: f64, x1: f64) -> TriMesh {
    along_strike(x0, x1, &[(0.0, 0.0), (60.0, 0.0), (60.0, 30.0), (0.0, 30.0)])
}

/// A plan rectangle at mine coordinates.
fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
    vec![[E + x0, N + y0], [E + x1, N + y0], [E + x1, N + y1], [E + x0, N + y1]]
}

/// Polygons across the strip every 100 m from `x0`, running 50 m past its sides.
fn across(x0: f64, count: usize) -> Vec<Vec<[f64; 2]>> {
    (0..count).map(|k| rectangle(x0 + 100.0 * k as f64, -50.0, x0 + 100.0 * (k + 1) as f64, 110.0)).collect()
}

#[test]
fn a_strip_cut_by_fifteen_polygons_across_it_adds_back_exactly() {
    let strip = square_strip(0.0, 1500.0);
    let cut = cut_solid_by_polygons(&strip, &across(0.0, 15)).unwrap();
    println!("{} triangles in, {} pieces, {:.0} ms", strip.triangle_count(), cut.pieces.len(), cut.ms);
    assert!((cut.input_volume - 2_700_000.0).abs() < 1e-3);
    assert_eq!(cut.pieces.len(), 15);
    for (k, piece) in cut.pieces.iter().enumerate() {
        assert_eq!(piece.polygon, k);
        assert!((piece.volume - 180_000.0).abs() < 0.18, "{}", piece.volume);
        assert_eq!(piece.parts, 1);
        assert!(piece.mesh.validity().is_watertight());
    }
    assert!(cut.remainder.is_none(), "the polygons cover the strip");
    assert!(cut.difference().abs() < 2.7, "{}", cut.difference());
}

/// The highwall and lowwall at 65°: polygons wider than the strip cut only its ends, so each piece keeps the batter.
#[test]
fn polygons_wider_than_a_battered_strip_keep_its_sides() {
    let lean = 30.0 / 65f64.to_radians().tan();
    let strip = along_strike(0.0, 300.0, &[(0.0, 0.0), (60.0, 0.0), (60.0 + lean, 30.0), (-lean, 30.0)]);
    let section = (60.0 + 60.0 + 2.0 * lean) / 2.0 * 30.0;
    let cut = cut_solid_by_polygons(&strip, &across(0.0, 3)).unwrap();
    assert_eq!(cut.pieces.len(), 3);
    for piece in &cut.pieces {
        assert!((piece.volume - 100.0 * section).abs() < 1e-6 * 100.0 * section, "{} against {}", piece.volume, 100.0 * section);
    }
    assert!(cut.remainder.is_none());
}

#[test]
fn what_lies_outside_every_polygon_is_the_remainder() {
    let strip = square_strip(0.0, 1500.0);
    // Blocks over the west 1,000 m only.
    let cut = cut_solid_by_polygons(&strip, &across(0.0, 10)).unwrap();
    assert_eq!(cut.pieces.len(), 10);
    let rest = cut.remainder.as_ref().expect("the east 500 m");
    assert!((rest.volume - 900_000.0).abs() < 0.9, "{}", rest.volume);
    assert_eq!(rest.parts, 1);
    assert!(cut.difference().abs() < 2.7);
}

#[test]
fn a_polygon_turned_30_degrees_takes_its_own_area() {
    let strip = along_strike(0.0, 600.0, &[(-300.0, 0.0), (300.0, 0.0), (300.0, 20.0), (-300.0, 20.0)]);
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let square: Vec<[f64; 2]> = [(-50.0, -50.0), (50.0, -50.0), (50.0, 50.0), (-50.0, 50.0)]
        .iter()
        .map(|&(x, y)| [E + 300.0 + c * x - s * y, N + s * x + c * y])
        .collect();
    let cut = cut_solid_by_polygons(&strip, &[square]).unwrap();
    assert_eq!(cut.pieces.len(), 1);
    assert!((cut.pieces[0].volume - 200_000.0).abs() < 0.2, "{}", cut.pieces[0].volume);
    let rest = cut.remainder.unwrap();
    assert!((rest.volume - (7_200_000.0 - 200_000.0)).abs() < 7.2);
}

/// A polygon clockwise is taken the same as anticlockwise; one reaching into the solid in two places makes one piece
/// of two parts; one away from the solid isn't cut with.
#[test]
fn polygon_winding_two_parts_and_far_polygons() {
    let strip = square_strip(0.0, 300.0);
    let mut clockwise = rectangle(0.0, -50.0, 100.0, 110.0);
    clockwise.reverse();
    // A U in plan: two arms across the strip at x 0-50 and 250-300, joined north of it.
    let u = vec![[E, N - 50.0], [E + 50.0, N - 50.0], [E + 50.0, N + 80.0], [E + 250.0, N + 80.0], [E + 250.0, N - 50.0], [E + 300.0, N - 50.0], [E + 300.0, N + 100.0], [E, N + 100.0]];
    let far = rectangle(5_000.0, 0.0, 5_100.0, 60.0);
    let cut = cut_solid_by_polygons(&strip, &[clockwise, far]).unwrap();
    assert_eq!(cut.polygons_near, 1);
    assert_eq!(cut.pieces.len(), 1);
    assert!((cut.pieces[0].volume - 180_000.0).abs() < 0.18);

    let cut = cut_solid_by_polygons(&strip, &[u]).unwrap();
    assert_eq!(cut.pieces.len(), 1);
    assert_eq!(cut.pieces[0].parts, 2);
    assert!((cut.pieces[0].volume - 2.0 * 50.0 * 60.0 * 30.0).abs() < 0.18);
}

#[test]
fn overlapping_polygons_are_found_and_neighbours_are_not() {
    let mut polygons = across(0.0, 3);
    assert!(polygons_overlapping(&polygons, 0.01).is_empty(), "sharing an edge isn't an overlap");
    polygons.push(rectangle(90.0, -50.0, 190.0, 110.0));
    let found = polygons_overlapping(&polygons, 0.01);
    assert_eq!(found.len(), 2, "{found:?}");
    let (i, j, area, at) = found[0];
    assert_eq!((i, j), (0, 3));
    assert!((area - 10.0 * 160.0).abs() < 1e-6);
    assert!((at[0] - E - 95.0).abs() < 1e-6 && (at[1] - N - 30.0).abs() < 1e-6, "{at:?}");
    assert_eq!((found[1].0, found[1].1), (1, 3));
    assert!((found[1].2 - 90.0 * 160.0).abs() < 1e-6);
}

/// Polygons over the whole strip cover it, battered sides and all; over the west 1,000 m they leave the east 500 m, 30,000 m²
/// in plan, about its middle.
#[test]
fn polygons_over_the_whole_solid_cover_it_and_a_short_set_leaves_the_rest() {
    let strip = square_strip(0.0, 1500.0);
    let started = std::time::Instant::now();
    assert!(polygons_uncovered(&strip, &across(0.0, 15)).unwrap().is_empty());
    println!("checked {} triangles against 15 polygons in {:.1} ms", strip.triangle_count(), started.elapsed().as_secs_f64() * 1000.0);
    let lean = 30.0 / 65f64.to_radians().tan();
    let battered = along_strike(0.0, 300.0, &[(0.0, 0.0), (60.0, 0.0), (60.0 + lean, 30.0), (-lean, 30.0)]);
    assert!(polygons_uncovered(&battered, &across(0.0, 3)).unwrap().is_empty(), "the crest's 14 m past the toe is inside the polygons");

    let gaps = polygons_uncovered(&strip, &across(0.0, 10)).unwrap();
    assert_eq!(gaps.len(), 1);
    assert!((gaps[0].area - 30_000.0).abs() < 1e-3, "{}", gaps[0].area);
    assert!((gaps[0].at[0] - (E + 1250.0)).abs() < 1e-3 && (gaps[0].at[1] - (N + 30.0)).abs() < 1e-3, "{:?}", gaps[0].at);
}

/// A crest's lean past polygons drawn to the toe is uncovered: 14 m by 300 m each side for a 30 m wall at 65°.
#[test]
fn polygons_drawn_to_the_toe_leave_the_battered_walls_uncovered() {
    let lean = 30.0 / 65f64.to_radians().tan();
    let battered = along_strike(0.0, 300.0, &[(0.0, 0.0), (60.0, 0.0), (60.0 + lean, 30.0), (-lean, 30.0)]);
    let to_the_toe: Vec<Vec<[f64; 2]>> = (0..3).map(|k| rectangle(100.0 * k as f64, 0.0, 100.0 * (k + 1) as f64, 60.0)).collect();
    let gaps = polygons_uncovered(&battered, &to_the_toe).unwrap();
    assert_eq!(gaps.len(), 2, "{gaps:?}");
    for gap in &gaps {
        assert!((gap.area - 300.0 * lean).abs() < 1e-3 * 300.0 * lean, "{} against {}", gap.area, 300.0 * lean);
    }
}

/// Polygons a millimetre apart, or with an edge on the solid's own edge, cover it: slivers under COVER_SLIVER are rounding.
/// A metre apart, the gap is 60 m² between them. A solid away from every polygon is uncovered whole.
#[test]
fn slivers_are_rounding_and_a_metre_gap_or_a_solid_away_is_found() {
    let strip = square_strip(0.0, 300.0);
    let apart = |gap: f64| vec![rectangle(0.0, -50.0, 150.0, 110.0), rectangle(150.0 + gap, -50.0, 300.0, 110.0)];
    assert!(polygons_uncovered(&strip, &apart(0.001)).unwrap().is_empty());
    assert!(COVER_SLIVER > 0.001);
    let on_its_edges = vec![rectangle(0.0, 0.0, 300.0, 60.0)];
    assert!(polygons_uncovered(&strip, &on_its_edges).unwrap().is_empty());

    let gaps = polygons_uncovered(&strip, &apart(1.0)).unwrap();
    assert_eq!(gaps.len(), 1);
    assert!((gaps[0].area - 60.0).abs() < 1e-3, "{}", gaps[0].area);
    assert!((gaps[0].at[0] - (E + 150.5)).abs() < 1e-3, "{:?}", gaps[0].at);

    let away = polygons_uncovered(&square_strip(5_000.0, 5_100.0), &across(0.0, 3)).unwrap();
    assert_eq!(away.len(), 1);
    assert!((away[0].area - 6_000.0).abs() < 1e-3);
}
