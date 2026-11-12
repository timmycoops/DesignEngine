//! Solids cut by block polygons (decision 0062, workstream 2), with volumes worked by hand at mine grid coordinates.
//! Volumes to 1 part in a million of the solid unless said.

use zd_core::TriMesh;
use zd_geom::{cut_solid_by_polygons, polygons_overlapping};

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
