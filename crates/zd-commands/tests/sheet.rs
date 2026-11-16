//! `surface.between_strings` (decision 0079) through the registry, as the app calls it, and a ramp made by hand from
//! polylines alone with the string tools: Set grade, Offset, Project and a surface between the strings.

use std::sync::Arc;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, Object, ObjectId, Polyline, TriMesh};
use zd_geom::boundary_loops;

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn named(s: &Session, name: &str) -> Arc<Object> {
    s.project().objects().find(|o| o.name == name).unwrap_or_else(|| panic!("{name} wasn't made")).clone()
}

fn line_of(s: &Session, id: ObjectId) -> Polyline {
    match &s.project().object(id).expect("live").body {
        Body::String(line) => line.clone(),
        other => panic!("a {:?}", other.kind()),
    }
}

fn sheet_of(s: &Session, id: ObjectId) -> TriMesh {
    match &s.project().object(id).expect("live").body {
        Body::Surface(sheet) => sheet.mesh().clone(),
        other => panic!("a {:?}", other.kind()),
    }
}

fn string(s: &mut Session, name: &str, points: &[[f64; 3]]) -> ObjectId {
    let points: Vec<[f64; 3]> = points.iter().map(|p| [E + p[0], N + p[1], p[2]]).collect();
    created(&s.run("string.create", json!({ "name": name, "layer": "ramp", "points": points })).unwrap())[0]
}

/// The area of a parallelogram on two sides (m²).
fn parallelogram(u: [f64; 3], v: [f64; 3]) -> f64 {
    let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

/// A ramp made by hand from polylines alone, as it's made in Deswik. The road's inside edge is drawn 200 m long and
/// graded at 10 % from RL 100. Offset 30 m to the right is its outside edge, at the same heights. The cut's crest is the
/// inside edge projected 15 m up at 45° on the left, and the fill's toe the outside edge projected 10 m down at 45° on
/// the right. One surface between the four, picked in any order, is the ramp: the cut batter, the road and the fill
/// batter, three parallelograms whose areas are known, to 1e-6 m², in one sheet with one boundary.
#[test]
fn a_ramp_by_hand_from_polylines() {
    let mut s = Session::with_builtins();
    let inside = string(&mut s, "Inside edge", &[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0]]);
    s.run("string.set_grade", json!({ "string": inside, "from": 0, "grade": 10.0 })).unwrap();
    assert_eq!(line_of(&s, inside).vertices[1][2], 120.0, "200 m at 10 %");
    let outside = created(&s.run("string.offset", json!({ "string": inside, "distance": 30.0, "side": "right", "name": "Outside edge" })).unwrap())[0];
    assert!(line_of(&s, outside).vertices.iter().zip([100.0, 120.0]).all(|(p, z)| (p[1] - N + 30.0).abs() < 1e-9 && p[2] == z), "the offset keeps the edge's heights");
    let crest = created(&s.run("string.project", json!({ "strings": [inside], "to": { "kind": "height", "height": 15.0 }, "batter": 45.0, "side": "left", "name": "Cut crest" })).unwrap())[0];
    let toe = created(&s.run("string.project", json!({ "strings": [outside], "to": { "kind": "height", "height": 10.0 }, "batter": 45.0, "side": "right", "direction": "down", "name": "Fill toe" })).unwrap())[0];

    let record = s.run("surface.between_strings", json!({ "strings": [outside, crest, toe, inside], "name": "Ramp 1", "layer": "ramps" })).unwrap();
    let ramp = sheet_of(&s, created(&record)[0]);
    let along = [200.0, 0.0, 20.0];
    let want = parallelogram(along, [0.0, 15.0, 15.0]) + parallelogram(along, [0.0, 30.0, 0.0]) + parallelogram(along, [0.0, 10.0, 10.0]);
    assert!((ramp.area() - want).abs() < 1e-6, "{} against {want}", ramp.area());
    assert_eq!(ramp.vertices.len(), 8, "every corner is a string's vertex, shared where two bands meet");
    assert_eq!(boundary_loops(&ramp).unwrap().len(), 1, "one sheet");
    assert!(ramp.triangles.iter().all(|t| {
        let [a, b, c] = t.map(|i| ramp.vertices[i as usize]);
        (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]) > 0.0
    }), "wound upward");
    assert!(
        record.messages[0].starts_with("a surface between 4 strings, in order across 'Cut crest', 'Inside edge', 'Outside edge', 'Fill toe': 13118.6 m2, 11000.0 m2 in plan, 6 triangles; "),
        "{:?}",
        record.messages
    );
    assert_eq!(named(&s, "Ramp 1").name, "Ramp 1");
    assert!(s.project().layers().iter().any(|l| l.name == "ramps"));
    assert_eq!(line_of(&s, inside).vertices.len(), 2, "the strings are left as they are");
    s.undo().unwrap();
    assert!(s.project().objects().all(|o| o.name != "Ramp 1"), "one undo takes the surface back");
}

/// Two strings make the sheet between them, named for them, keeping the strip they share; given running opposite
/// ways, one is taken the other way along and the run says so. What can't be a surface is refused naming the strings.
#[test]
fn two_strings_and_what_is_refused() {
    let mut s = Session::with_builtins();
    let a = string(&mut s, "A", &[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0]]);
    let b = string(&mut s, "B", &[[200.0, 30.0, 100.0], [0.0, 30.0, 100.0]]);
    s.run("string.number_across", json!({ "strings": [a], "from": [E + 50.0, N - 10.0, 0.0], "to": [E + 50.0, N + 10.0, 0.0], "start": 7.0, "name": "A" })).unwrap();
    s.run("string.number_across", json!({ "strings": [b], "from": [E + 50.0, N + 20.0, 0.0], "to": [E + 50.0, N + 40.0, 0.0], "start": 7.0, "name": "B" })).unwrap();
    let (a_name, b_name) = (s.project().object(a).unwrap().name.clone(), s.project().object(b).unwrap().name.clone());
    let record = s.run("surface.between_strings", json!({ "strings": [a, b] })).unwrap();
    let made = s.project().object(created(&record)[0]).unwrap().clone();
    assert_eq!(made.name, format!("{a_name} to {b_name}"));
    assert_eq!(made.attributes.get("strip"), Some(&AttrValue::Number(7.0)));
    assert!((sheet_of(&s, made.id).area() - 6000.0).abs() < 1e-6);
    assert!(record.messages[0].contains(&format!("'{b_name}' taken the other way along, to run with '{a_name}'")), "{:?}", record.messages);

    let crossing = string(&mut s, "Crossing", &[[0.0, -50.0, 100.0], [200.0, 50.0, 100.0]]);
    let why = s.run("surface.between_strings", json!({ "strings": [a, crossing] })).unwrap_err().to_string();
    assert!(why.contains(&format!("no surface was made. between '{a_name}' and 'Crossing': they cross in plan")), "{why}");
    assert!(s.run("surface.between_strings", json!({ "strings": [a] })).unwrap_err().to_string().contains("two strings or more"));
    assert!(s.run("surface.between_strings", json!({ "strings": [a, a] })).is_err());
    assert!(s.run("surface.between_strings", json!({ "strings": [a, made.id] })).is_err(), "a surface is not a string");
}
