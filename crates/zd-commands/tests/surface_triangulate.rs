//! Triangulate (Surface tools plan, workstream 4; decision 0096): `surface.triangulate` through the session. Every answer
//! is known; mine grid coordinates throughout.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn string(s: &mut Session, name: &str, points: Vec<[f64; 3]>, closed: bool) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": "lines", "points": points, "closed": closed })).unwrap())[0]
}

/// A closed square `half` m from (E + 100, N + 100) at `rl`.
fn square(s: &mut Session, name: &str, half: f64, rl: f64) -> ObjectId {
    string(s, name, [[-half, -half], [half, -half], [half, half], [-half, half]].map(|[x, y]| [E + 100.0 + x, N + 100.0 + y, rl]).to_vec(), true)
}

/// The toe and crest of the compare tests' pit: compared against a level topography at RL 100, the frustum, 121,333.3
/// m³, over the crest's 14,400 m². The strings are as they were.
#[test]
fn a_toe_and_crest_make_the_pit() {
    let mut s = Session::with_builtins();
    let toe = square(&mut s, "Toe", 50.0, 90.0);
    let crest = square(&mut s, "Crest", 60.0, 100.0);
    let topo = created(&s.run("surface.from_mesh", json!({ "name": "Topography", "layer": "surfaces", "vertices": [[E - 100.0, N - 100.0, 100.0], [E + 300.0, N - 100.0, 100.0], [E + 300.0, N + 300.0, 100.0], [E - 100.0, N + 300.0, 100.0]], "triangles": [[0, 1, 2], [0, 2, 3]] })).unwrap())[0];
    let revisions = [toe, crest].map(|id| s.project().object(id).unwrap().revision);
    let record = s.run("surface.triangulate", json!({ "strings": [toe, crest] })).unwrap();
    let made = created(&record);
    let object = s.project().object(made[0]).unwrap();
    assert_eq!(object.name, "Triangulated");
    assert_eq!(s.project().layer(object.layer).unwrap().name, "triangulated");
    assert!(matches!(object.body, Body::Surface(_)));
    assert!(record.messages[0].starts_with("triangulated 2 strings (8 vertices): 10 triangles, 14,400.0 m² in plan, bounded by 'Crest'"), "{:?}", record.messages);
    assert_eq!([toe, crest].map(|id| s.project().object(id).unwrap().revision), revisions);
    let compared = s.run("surface.compare", json!({ "surface": topo, "with": made[0], "cell": 1.0 })).unwrap();
    assert!(compared.messages[0].contains("cut 121,333.3 m³, fill 0.0 m³, net cut 121,333.3 m³, over 14,400.0 m² both cover"), "{:?}", compared.messages);
}

/// A closed string drawn at RL 100 alone: a pad of its own area, named for it, on the layer asked for.
#[test]
fn a_polygon_alone_is_a_pad() {
    let mut s = Session::with_builtins();
    let pad = string(&mut s, "Pad", vec![[E, N, 100.0], [E + 80.0, N, 100.0], [E + 80.0, N + 30.0, 100.0], [E, N + 30.0, 100.0]], true);
    let record = s.run("surface.triangulate", json!({ "strings": [pad], "layer": "pads" })).unwrap();
    let object = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(object.name, "Pad surface");
    let Body::Surface(surface) = &object.body else { panic!("a surface") };
    assert!(surface.mesh().vertices.iter().all(|v| v[2] == 100.0));
    assert!(record.messages[0].contains("2,400.0 m² in plan, bounded by 'Pad'"), "{:?}", record.messages);
}

/// Strings crossing at different heights are refused, each place named with both heights; one partly outside the
/// closed string bounding it is refused, named; and only strings are taken.
#[test]
fn what_cant_be_triangulated_is_refused() {
    let mut s = Session::with_builtins();
    let a = string(&mut s, "Crest A", vec![[E, N, 100.0], [E + 100.0, N + 100.0, 100.0]], false);
    let b = string(&mut s, "Toe B", vec![[E, N + 100.0, 80.0], [E + 100.0, N, 100.0]], false);
    let refused = |s: &mut Session, params: serde_json::Value| s.run("surface.triangulate", params).unwrap_err().to_string();
    let words = refused(&mut s, json!({ "strings": [a, b] }));
    assert!(words.contains("cross or touch in plan at different heights in 1 place") && words.contains("'Crest A' and 'Toe B' at 650,050.0 E 7,450,050.0 N, RL 100.000 and 90.000"), "{words}");
    let bound = square(&mut s, "Limit", 20.0, 100.0);
    let words = refused(&mut s, json!({ "strings": [bound, a] }));
    assert!(words.contains("'Crest A' lies partly outside every closed string picked"), "{words}");
    let topo = created(&s.run("surface.from_mesh", json!({ "name": "Topography", "layer": "surfaces", "vertices": [[E, N, 100.0], [E + 10.0, N, 100.0], [E, N + 10.0, 100.0]], "triangles": [[0, 1, 2]] })).unwrap())[0];
    assert!(refused(&mut s, json!({ "strings": [topo] })).to_lowercase().contains("string"));
    assert!(refused(&mut s, json!({ "strings": [] })).contains("nothing to triangulate"));
}
