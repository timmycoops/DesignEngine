//! Strings from surfaces (String tools plan, workstream 5; decision 0083): where two surfaces meet, contours, outlines
//! and what a section cuts, through the commands as the app and Python run them. Every answer is known; mine grid
//! coordinates throughout.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId, Polyline};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

/// A square surface `size` m a side from (E, N) in `cells` × `cells`, each corner at the RL `rl` gives it.
fn surface(s: &mut Session, name: &str, size: f64, cells: usize, rl: impl Fn(f64, f64) -> f64) -> ObjectId {
    let mut vertices = Vec::new();
    for j in 0..=cells {
        for i in 0..=cells {
            let (x, y) = (i as f64 * size / cells as f64, j as f64 * size / cells as f64);
            vertices.push([E + x, N + y, rl(x, y)]);
        }
    }
    let at = |i: usize, j: usize| j * (cells + 1) + i;
    let mut triangles = Vec::new();
    for j in 0..cells {
        for i in 0..cells {
            triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    created(&s.run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": vertices, "triangles": triangles })).unwrap())[0]
}

fn cube(s: &mut Session, name: &str, x: f64, side: f64, z: f64) -> ObjectId {
    let v: Vec<[f64; 3]> = (0..8).map(|i| [E + x + (i & 1) as f64 * side, N + (i >> 1 & 1) as f64 * side, z + (i >> 2) as f64 * side]).collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "solids", "vertices": v, "triangles": triangles })).unwrap())[0]
}

fn line(s: &Session, id: ObjectId) -> Polyline {
    match &s.project().object(id).expect("live string").body {
        Body::String(line) => line.clone(),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

fn name(s: &Session, id: ObjectId) -> String {
    s.project().object(id).unwrap().name.clone()
}

fn plan_area(l: &Polyline) -> f64 {
    let o = l.vertices[0];
    let n = l.vertices.len();
    (0..n).map(|k| {
        let (p, q) = (l.vertices[k], l.vertices[(k + 1) % n]);
        (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1])
    }).sum::<f64>().abs() / 2.0
}

/// A level design at RL 100 meeting a slope at 45° rising east from RL 70: one straight string 30 m east at RL 100,
/// the full 200 m north, on the layer asked for, named for both; the surfaces as they were.
#[test]
fn a_level_plane_meets_a_45_degree_plane_in_one_straight_string() {
    let mut s = Session::with_builtins();
    let level = surface(&mut s, "Design", 200.0, 7, |_, _| 100.0);
    let slope = surface(&mut s, "Topo", 200.0, 9, |x, _| 70.0 + x);
    let revisions = [level, slope].map(|id| s.project().object(id).unwrap().revision);
    let record = s.run("string.where_surfaces_meet", json!({ "surface": level, "with": slope, "layer": "crests" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 1);
    assert_eq!(name(&s, made[0]), "Design meets Topo");
    let crest = line(&s, made[0]);
    assert!(!crest.closed);
    for p in &crest.vertices {
        assert!((p[0] - (E + 30.0)).abs() < 1e-9 && (p[2] - 100.0).abs() < 1e-9);
    }
    let (ys, ye) = (crest.vertices[0][1] - N, crest.vertices[crest.vertices.len() - 1][1] - N);
    assert!((ys.min(ye)).abs() < 1e-9 && (ys.max(ye) - 200.0).abs() < 1e-9, "{ys} {ye}");
    assert_eq!([level, slope].map(|id| s.project().object(id).unwrap().revision), revisions, "the surfaces left as they were");
    assert!(s.run("string.where_surfaces_meet", json!({ "surface": level, "with": level, "layer": "crests" })).is_err(), "a surface with itself");
    let apart = surface(&mut s, "High", 200.0, 2, |_, _| 500.0);
    let refused = s.run("string.where_surfaces_meet", json!({ "surface": level, "with": apart, "layer": "crests" })).unwrap_err().to_string();
    assert!(refused.contains("don't meet anywhere"), "{refused}");
}

/// Contours every 5 m on a plane rising 10 % east, from RL 105 to 145: nine strings 50 m apart in plan, each named for
/// its RL and carrying it as `rl`. Left to the surface's own range, every 5 m from RL 105 to 150: ten.
#[test]
fn contours_every_5_m_carry_their_rl() {
    let mut s = Session::with_builtins();
    let plane = surface(&mut s, "Ramp", 500.0, 25, |x, _| 100.0 + 0.1 * x);
    let made = created(&s.run("string.contours", json!({ "surface": plane, "every": 5.0, "from": 105.0, "to": 145.0, "layer": "contours" })).unwrap());
    assert_eq!(made.len(), 9);
    for (k, &id) in made.iter().enumerate() {
        let rl = 105.0 + 5.0 * k as f64;
        assert_eq!(name(&s, id), format!("Contour {rl}"));
        assert_eq!(s.project().object(id).unwrap().attributes.get("rl"), Some(&AttrValue::Number(rl)));
        let c = line(&s, id);
        assert!(c.vertices.iter().all(|p| p[2] == rl && (p[0] - (E + 50.0 * (k + 1) as f64)).abs() < 1e-9));
    }
    let all = created(&s.run("string.contours", json!({ "surface": plane, "every": 5.0, "layer": "contours" })).unwrap());
    // RL 100 is the surface's lowest, its west edge: a corner at an RL counts as above it, as a corner on a section
    // plane does, so no line is drawn there. RL 105 to 150.
    assert_eq!(all.len(), 10, "RL 105 to 150, in the surface's own range");
    assert_eq!(name(&s, all[0]), "Contour 105");
    let refused = s.run("string.contours", json!({ "surface": plane, "every": 5.0, "from": 300.0, "to": 310.0, "layer": "contours" })).unwrap_err().to_string();
    assert!(refused.contains("doesn't reach"), "{refused}");
    assert!(s.run("string.contours", json!({ "surface": plane, "every": 0.0, "layer": "contours" })).is_err());
}

/// The outline of a 100 m square surface with a 20 m square hole: two closed strings, 10,000 m² and 400 m² in plan,
/// the outside first. A 10 m cube's: one closed square, 100 m², at its lowest RL.
#[test]
fn outlines_of_a_surface_with_a_hole_and_of_a_solid() {
    let mut s = Session::with_builtins();
    let mut vertices = Vec::new();
    for j in 0..=5 {
        for i in 0..=5 {
            vertices.push([E + 20.0 * i as f64, N + 20.0 * j as f64, 100.0]);
        }
    }
    let at = |i: usize, j: usize| j * 6 + i;
    let mut triangles = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            if (i, j) != (2, 2) {
                triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
    }
    let holed = created(&s.run("surface.from_mesh", json!({ "name": "Pad", "layer": "surfaces", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    let block = cube(&mut s, "Block", 300.0, 10.0, 40.0);
    let made = created(&s.run("string.outline", json!({ "objects": [holed, block], "layer": "outlines" })).unwrap());
    assert_eq!(made.iter().map(|&id| name(&s, id)).collect::<Vec<_>>(), ["Pad outline", "Pad hole 1", "Block outline"]);
    let lines: Vec<Polyline> = made.iter().map(|&id| line(&s, id)).collect();
    assert!(lines.iter().all(|l| l.closed));
    for (l, area) in lines.iter().zip([10_000.0, 400.0, 100.0]) {
        assert!((plan_area(l) - area).abs() < 1e-6, "{} against {area}", plan_area(l));
    }
    assert!(lines[2].vertices.iter().all(|p| p[2] == 40.0), "the solid's at its lowest RL");
}

/// A section east through two 10 m cubes side by side and a level surface: a closed 10 m × 10 m loop from each cube
/// and one 200 m line from the surface, each named for what it cut; a section missing everything is refused.
#[test]
fn a_section_kept_as_strings() {
    let mut s = Session::with_builtins();
    let a = cube(&mut s, "A", 20.0, 10.0, 90.0);
    let b = cube(&mut s, "B", 60.0, 10.0, 90.0);
    let floor = surface(&mut s, "Floor", 200.0, 4, |_, _| 85.0);
    let through = |y: f64| json!({ "from": [E - 10.0, N + y, 0.0], "to": [E + 300.0, N + y, 0.0], "objects": [a, b, floor], "layer": "section" });
    let made = created(&s.run("string.from_section", through(5.0)).unwrap());
    let names: Vec<String> = made.iter().map(|&id| name(&s, id)).collect();
    assert_eq!(names, ["A section", "B section", "Floor section"]);
    for &id in &made[..2] {
        let l = line(&s, id);
        assert!(l.closed && l.vertices.iter().all(|p| (p[1] - (N + 5.0)).abs() < 1e-9));
        let along: f64 = l.vertices.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max) - l.vertices.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        assert!((along - 10.0).abs() < 1e-9);
    }
    let f = line(&s, made[2]);
    assert!(!f.closed && f.vertices.iter().all(|p| p[2] == 85.0));
    let refused = s.run("string.from_section", through(-50.0)).unwrap_err().to_string();
    assert!(refused.contains("cuts none"), "{refused}");
}
