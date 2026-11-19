//! Vertices a few at a time (String tools plan, workstream 4): the vertices picked moved and deleted together, set to an
//! RL, a grade or a surface on their own; a vertex wherever strings cross; and a vertex every so many metres. Every answer
//! is known; mine grid coordinates throughout.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId, Polyline};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn at(x: f64, y: f64, z: f64) -> [f64; 3] {
    [E + x, N + y, z]
}

fn string(s: &mut Session, name: &str, points: &[[f64; 3]]) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points })).unwrap())[0]
}

fn line(s: &Session, id: ObjectId) -> Polyline {
    match &s.project().object(id).expect("live string").body {
        Body::String(line) => line.clone(),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

/// Ten vertices 10 m apart running east, climbing a metre each.
fn ten(s: &mut Session, name: &str, y: f64) -> ObjectId {
    string(s, name, &(0..10).map(|i| at(10.0 * i as f64, y, 100.0 + i as f64)).collect::<Vec<_>>())
}

/// Three of ten vertices picked and moved 5 m north in one run, with two of another string's: those moved, the rest
/// exactly where they were, one undo for both strings.
#[test]
fn three_of_ten_moved_five_metres_north_in_one_run_and_one_undo() {
    let mut s = Session::with_builtins();
    let (a, b) = (ten(&mut s, "Toe 1", 0.0), ten(&mut s, "Toe 2", 30.0));
    let (was_a, was_b) = (line(&s, a), line(&s, b));
    let record = s.run("string.move_vertices", json!({ "strings": [a, b], "vertices": [[2, 4, 6], [0, 9]], "by": [0.0, 5.0, 0.0] })).unwrap();
    assert_eq!(record.messages, ["moved 5 vertices 5 m north: 2 of 2 strings", "'Toe 1': vertices 3, 5 and 7 moved", "'Toe 2': vertices 1 and 10 moved"]);
    let (now_a, now_b) = (line(&s, a), line(&s, b));
    for i in 0..10 {
        let moves_a = [2, 4, 6].contains(&i);
        let moves_b = [0, 9].contains(&i);
        let shift = |v: [f64; 3], moves: bool| if moves { [v[0], v[1] + 5.0, v[2]] } else { v };
        assert_eq!(now_a.vertices[i], shift(was_a.vertices[i], moves_a), "Toe 1 vertex {i}");
        assert_eq!(now_b.vertices[i], shift(was_b.vertices[i], moves_b), "Toe 2 vertex {i}");
    }
    s.undo();
    assert_eq!((line(&s, a), line(&s, b)), (was_a, was_b), "one undo puts both back");
}

/// Deleted together; a string that would be left too short refuses the run, named, and nothing changes.
#[test]
fn vertices_deleted_together_and_never_too_few() {
    let mut s = Session::with_builtins();
    let (a, b) = (ten(&mut s, "Toe 1", 0.0), string(&mut s, "Short", &[at(0.0, 50.0, 100.0), at(10.0, 50.0, 100.0), at(20.0, 50.0, 100.0)]));
    s.run("string.delete_vertices", json!({ "strings": [a], "vertices": [[1, 2, 8]] })).unwrap();
    let kept = line(&s, a);
    assert_eq!(kept.vertices.len(), 7);
    assert_eq!(kept.vertices[1], at(30.0, 0.0, 103.0));
    let error = s.run("string.delete_vertices", json!({ "strings": [a, b], "vertices": [[0], [0, 1]] })).unwrap_err();
    assert_eq!(
        error.to_string(),
        "1 of 2 strings can't be done, so nothing was changed. 'Short': deleting 2 of its 3 vertices would leave 1, and an open string needs 2"
    );
    assert_eq!(line(&s, a).vertices.len(), 7, "nothing changed");
}

/// Set elevation, Set grade and Drape on the vertices picked alone; the rest keep their RLs.
#[test]
fn set_elevation_grade_and_drape_on_the_vertices_picked_alone() {
    let mut s = Session::with_builtins();
    let a = ten(&mut s, "Toe 1", 0.0);
    let was = line(&s, a);
    s.run("string.set_elevation", json!({ "strings": [a], "vertices": [[1, 8]], "rl": 90.0 })).unwrap();
    let level = line(&s, a);
    assert_eq!((level.vertices[1][2], level.vertices[8][2]), (90.0, 90.0));
    assert!((0..10).filter(|i| ![1, 8].contains(i)).all(|i| level.vertices[i] == was.vertices[i]));

    // Vertices 3, 4 and 8 at 10 % from RL 50: 0, 10 and 50 m along from vertex 3.
    let record = s.run("string.set_grade", json!({ "strings": [a], "vertices": [[7, 2, 3]], "grade": 10.0, "rl": 50.0 })).unwrap();
    assert!(record.notes.iter().any(|n| n.text == "'Toe 1': vertices 3, 4 and 8 at 10 % from RL 50.000"), "{:?}", record.notes);
    let graded = line(&s, a);
    assert_eq!(graded.vertices[2][2], 50.0);
    assert!((graded.vertices[3][2] - 51.0).abs() < 1e-9 && (graded.vertices[7][2] - 55.0).abs() < 1e-9);
    assert_eq!(graded.vertices[8][2], 90.0, "not picked, so as it was");

    // A plane rising 10 % north at RL 120 on the toe's line: vertices 1 and 10 take 120, nothing added.
    let mesh = json!({ "name": "Topography", "layer": "surfaces",
        "vertices": [at(-50.0, -50.0, 115.0), at(150.0, -50.0, 115.0), at(150.0, 50.0, 125.0), at(-50.0, 50.0, 125.0)],
        "triangles": [[0, 1, 2], [0, 2, 3]] });
    let surface = created(&s.run("surface.from_mesh", mesh).unwrap())[0];
    s.run("string.drape", json!({ "strings": [a], "vertices": [[0, 9]], "surface": surface })).unwrap();
    let draped = line(&s, a);
    assert_eq!(draped.vertices.len(), 10, "no crossings added with vertices picked");
    assert!((draped.vertices[0][2] - 120.0).abs() < 1e-9 && (draped.vertices[9][2] - 120.0).abs() < 1e-9);
    assert_eq!(draped.vertices[1..9], graded.vertices[1..9]);

    let error = s.run("string.set_elevation", json!({ "strings": [a], "vertices": [[1]], "from": 0, "rl": 1.0 })).unwrap_err();
    assert!(error.to_string().contains("not both"), "{error}");
}

/// Two lines crossing, both picked: each gains a vertex at the crossing, at its own RL there. Again, nothing more.
#[test]
fn two_lines_crossing_each_gain_a_vertex_at_the_crossing() {
    let mut s = Session::with_builtins();
    let east = string(&mut s, "East", &[at(0.0, 0.0, 100.0), at(100.0, 0.0, 110.0)]);
    let north = string(&mut s, "North", &[at(30.0, -20.0, 50.0), at(30.0, 20.0, 50.0)]);
    s.run("string.vertices_at_crossings", json!({ "strings": [east, north] })).unwrap();
    let (a, b) = (line(&s, east), line(&s, north));
    assert_eq!((a.vertices.len(), b.vertices.len()), (3, 3));
    let (v, w) = (a.vertices[1], b.vertices[1]);
    assert!((v[0] - (E + 30.0)).abs() < 1e-9 && (v[1] - N).abs() < 1e-9 && (v[2] - 103.0).abs() < 1e-9, "{v:?}");
    assert!((w[0] - (E + 30.0)).abs() < 1e-9 && (w[1] - N).abs() < 1e-9 && w[2] == 50.0, "{w:?}");
    let record = s.run("string.vertices_at_crossings", json!({ "strings": [east, north] })).unwrap();
    assert!(record.messages[0].starts_with("vertices added where strings cross: 0 of 2 strings"), "{:?}", record.messages);
    assert!(s.run("string.vertices_at_crossings", json!({ "strings": [east] })).is_err(), "one string crosses nothing");
}

/// A 100 m straight densified every 2 m: 51 vertices, all on the line, its ends exactly.
#[test]
fn a_100_m_straight_densified_every_2_m_is_51_vertices_on_the_line() {
    let mut s = Session::with_builtins();
    let id = string(&mut s, "Ramp line", &[at(0.0, 0.0, 100.0), at(60.0, 80.0, 110.0)]);
    let record = s.run("string.densify", json!({ "strings": [id], "spacing": 2.0 })).unwrap();
    assert!(record.notes.iter().any(|n| n.text == "'Ramp line': 49 vertices added, 51 in all"), "{:?}", record.notes);
    let dense = line(&s, id);
    assert_eq!(dense.vertices.len(), 51);
    assert_eq!((dense.vertices[0], dense.vertices[50]), (at(0.0, 0.0, 100.0), at(60.0, 80.0, 110.0)));
    for (k, v) in dense.vertices.iter().enumerate() {
        let t = k as f64 / 50.0;
        assert!((v[0] - (E + 60.0 * t)).abs() < 1e-9 && (v[1] - (N + 80.0 * t)).abs() < 1e-9 && (v[2] - (100.0 + 10.0 * t)).abs() < 1e-9, "vertex {k}");
    }
}
