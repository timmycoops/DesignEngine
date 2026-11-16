//! `string.ramp_polygon` (decision 0079) through the registry, as the app calls it: a ramp placed from a start, along
//! a line or a direction, to an RL, a surface or a grid, and moved by placing it again.

use serde_json::{Value, json};
use zd_commands::{Code, RunRecord, Session, Severity};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId, Polyline};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn changed(record: &RunRecord, how: ChangeKind) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == how).map(|o| o.id).collect()
}

fn line_of(s: &Session, id: ObjectId) -> Polyline {
    match &s.project().object(id).expect("live").body {
        Body::String(line) => line.clone(),
        other => panic!("a {:?}", other.kind()),
    }
}

/// A string's vertices about the origin, to the micrometre.
fn local(s: &Session, id: ObjectId) -> Vec<[f64; 3]> {
    let r = |v: f64| (v * 1e6).round() / 1e6 + 0.0;
    line_of(s, id).vertices.iter().map(|p| [r(p[0] - E), r(p[1] - N), r(p[2])]).collect()
}

fn at(x: f64, y: f64, z: f64) -> Value {
    json!([E + x, N + y, z])
}

fn number(s: &Session, id: ObjectId, key: &str) -> f64 {
    match s.project().object(id).unwrap().attributes.get(key) {
        Some(AttrValue::Number(n)) => *n,
        other => panic!("{key} is {other:?}"),
    }
}

/// A toe 150 m east then north, on strip 4.
fn toe(s: &mut Session) -> ObjectId {
    let points = [[E, N, 100.0], [E + 150.0, N, 100.0], [E + 150.0, N + 300.0, 100.0]];
    let id = changed(&s.run("string.create", json!({ "name": "Toe", "layer": "toes", "points": points })).unwrap(), ChangeKind::Created)[0];
    s.run("string.number_across", json!({ "strings": [id], "from": [E + 50.0, N - 10.0, 0.0], "to": [E + 50.0, N + 10.0, 0.0], "start": 4.0, "name": "Toe" })).unwrap();
    id
}

/// A ramp placed along a toe, and moved. From 50 m along it at RL 100, 30 m wide on the left at 10 % to RL 120: 100 m
/// to the corner and 100 m on, the corner at RL 110 from edge to edge, to 1 µm. It's a closed string on the toe's
/// layer, on its strip, carrying its width, grade, length and RLs. Placed again 50 m further along with `replaces`,
/// the same polygon takes its next revision there, and one undo puts it back.
#[test]
fn a_ramp_along_a_toe_placed_and_moved() {
    let mut s = Session::with_builtins();
    let toe = toe(&mut s);
    let place = |start: Value| json!({ "start": start, "towards": at(150.0, 200.0, 0.0), "line": toe, "width": 30.0, "grade": 10.0, "side": "left", "to": { "kind": "rl", "rl": 120.0 } });
    let record = s.run("string.ramp_polygon", place(at(50.0, 0.0, 100.0))).unwrap();
    let ramp = changed(&record, ChangeKind::Created)[0];
    assert_eq!(local(&s, ramp), [[50.0, 0.0, 100.0], [150.0, 0.0, 110.0], [150.0, 100.0, 120.0], [120.0, 100.0, 120.0], [120.0, 30.0, 110.0], [50.0, 30.0, 100.0]]);
    assert!(line_of(&s, ramp).closed);
    assert_eq!(record.messages[0], "a ramp 30 m wide at 10 % up along 'Toe 4', from RL 100 to RL 120: 200 m long, 6 vertices");
    let object = s.project().object(ramp).unwrap().clone();
    assert_eq!(object.name, "Ramp RL 100 to 120");
    assert_eq!(s.project().layer(object.layer).unwrap().name, "toes");
    assert_eq!(object.attributes.get("strip"), Some(&AttrValue::Number(4.0)));
    assert_eq!([number(&s, ramp, "ramp_width"), number(&s, ramp, "ramp_grade"), number(&s, ramp, "ramp_from_rl")], [30.0, 10.0, 100.0]);
    assert!((number(&s, ramp, "ramp_length") - 200.0).abs() < 1e-9 && (number(&s, ramp, "ramp_to_rl") - 120.0).abs() < 1e-9);

    // Moved: the start 50 m further along, so the corner is 50 m in and the top 150 m up the north leg.
    let mut moved = place(at(100.0, 0.0, 100.0));
    moved["replaces"] = json!(ramp);
    let record = s.run("string.ramp_polygon", moved).unwrap();
    assert_eq!(changed(&record, ChangeKind::Revised), [ramp], "the same polygon, its next revision");
    assert!(changed(&record, ChangeKind::Created).is_empty());
    assert!(record.messages[0].ends_with("; moved from where it was"), "{:?}", record.messages);
    assert_eq!(local(&s, ramp), [[100.0, 0.0, 100.0], [150.0, 0.0, 105.0], [150.0, 150.0, 120.0], [120.0, 150.0, 120.0], [120.0, 30.0, 105.0], [100.0, 30.0, 100.0]]);
    assert_eq!(s.project().object(ramp).unwrap().name, "Ramp RL 100 to 120", "it keeps its name");
    s.undo().unwrap();
    assert_eq!(local(&s, ramp)[0], [50.0, 0.0, 100.0], "one undo puts it back where it was");

    // A ramp can only take the place of a ramp.
    let mut onto_the_toe = place(at(50.0, 0.0, 100.0));
    onto_the_toe["replaces"] = json!(toe);
    assert!(s.run("string.ramp_polygon", onto_the_toe).unwrap_err().to_string().contains("isn't a ramp's polygon"));
}

/// Straight from a start, to an RL, a surface and a grid. From RL 100 going east at 10 %, 30 m wide on the right: up to
/// RL 120 it's 200 m long; down to RL 90, 100 m. Under a surface falling 1 in 20 east from RL 130 it meets it 200 m out
/// at RL 120, to 1 µm, and under a level Vulcan grid at RL 115, 150 m out. It goes on the `ramps` layer.
#[test]
fn a_ramp_straight_from_a_start() {
    let mut s = Session::with_builtins();
    let straight = |to: Value| json!({ "start": at(0.0, 0.0, 100.0), "towards": at(25.0, 0.0, 0.0), "width": 30.0, "grade": 10.0, "side": "right", "to": to });
    let up = changed(&s.run("string.ramp_polygon", straight(json!({ "kind": "rl", "rl": 120.0 }))).unwrap(), ChangeKind::Created)[0];
    assert_eq!(local(&s, up), [[0.0, 0.0, 100.0], [200.0, 0.0, 120.0], [200.0, -30.0, 120.0], [0.0, -30.0, 100.0]]);
    assert_eq!(s.project().layer(s.project().object(up).unwrap().layer).unwrap().name, "ramps");
    let record = s.run("string.ramp_polygon", straight(json!({ "kind": "rl", "rl": 90.0 }))).unwrap();
    let down = changed(&record, ChangeKind::Created)[0];
    assert_eq!(local(&s, down), [[0.0, 0.0, 100.0], [100.0, 0.0, 90.0], [100.0, -30.0, 90.0], [0.0, -30.0, 100.0]]);
    assert_eq!(record.messages[0], "a ramp 30 m wide at 10 % down straight from its start, from RL 100 to RL 90: 100 m long, 4 vertices");
    assert_eq!(number(&s, down, "ramp_grade"), -10.0);

    let z = |x: f64| 130.0 - 0.05 * x;
    let corners = [[E - 500.0, N - 500.0, z(-500.0)], [E + 900.0, N - 500.0, z(900.0)], [E + 900.0, N + 500.0, z(900.0)], [E - 500.0, N + 500.0, z(-500.0)]];
    let topo = changed(&s.run("surface.from_mesh", json!({ "name": "Topography", "layer": "surfaces", "vertices": corners, "triangles": [[0, 1, 2], [0, 2, 3]] })).unwrap(), ChangeKind::Created)[0];
    let record = s.run("string.ramp_polygon", straight(json!({ "kind": "surface", "id": topo }))).unwrap();
    assert_eq!(local(&s, changed(&record, ChangeKind::Created)[0]), [[0.0, 0.0, 100.0], [200.0, 0.0, 120.0], [200.0, -30.0, 120.0], [0.0, -30.0, 100.0]]);

    // A level grid at RL 115, in Vulcan's plain layout over the same ground.
    let dir = std::env::temp_dir().join(format!("zd-ramp-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pitlower.srg"), level_vulcan_grid(115.0, 41, 21)).unwrap();
    std::fs::write(dir.join("pitlower.sfg"), level_vulcan_grid(60.0, 41, 21)).unwrap();
    s.run("grid.import_vulcan_seam", json!({ "dir": dir.to_string_lossy(), "prefix": "pit", "seam": "lower", "layer": "geology" })).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let roof = s.project().objects().find(|o| o.name == "LOWER sr").unwrap().id;
    let record = s.run("string.ramp_polygon", straight(json!({ "kind": "surface", "id": roof }))).unwrap();
    assert_eq!(local(&s, changed(&record, ChangeKind::Created)[0]), [[0.0, 0.0, 100.0], [150.0, 0.0, 115.0], [150.0, -30.0, 115.0], [0.0, -30.0, 100.0]]);
}

/// A level grid in Vulcan's plain layout: big-endian, nodes from 250 m south-west of the origin at 25 m, x fastest.
fn level_vulcan_grid(z: f32, cols: usize, rows: usize) -> Vec<u8> {
    let mut out = b"SYNTHETIC TEST GRID             ".to_vec();
    let (x0, y0) = (E - 250.0, N - 250.0);
    for v in [x0, x0 + (cols - 1) as f64 * 25.0, y0, y0 + (rows - 1) as f64 * 25.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(cols * rows) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&25.0f32.to_be_bytes());
    out.extend_from_slice(&25.0f32.to_be_bytes());
    out.extend_from_slice(&(cols as i32).to_be_bytes());
    out.extend_from_slice(&(rows as i32).to_be_bytes());
    for _ in 0..cols * rows {
        out.extend_from_slice(&z.to_be_bytes());
        out.extend_from_slice(&1i32.to_be_bytes());
    }
    out
}

/// A ramp that runs out of line ends there at its grade and says how far short it is; one that can't be made is
/// refused with why, and nothing is made.
#[test]
fn a_ramp_short_of_its_target_says_so() {
    let mut s = Session::with_builtins();
    let toe = toe(&mut s);
    // 450 m of toe from its start; RL 150 at 10 % needs 500 m.
    let record = s.run("string.ramp_polygon", json!({ "start": at(0.0, 0.0, 100.0), "towards": at(100.0, 0.0, 0.0), "line": toe, "width": 30.0, "grade": 10.0, "side": "left", "to": { "kind": "rl", "rl": 150.0 } })).unwrap();
    let ramp = changed(&record, ChangeKind::Created)[0];
    assert!((number(&s, ramp, "ramp_to_rl") - 145.0).abs() < 1e-9 && (number(&s, ramp, "ramp_length") - 450.0).abs() < 1e-9);
    let note = record.notes.iter().find(|n| n.code == Some(Code::RampShortOfTarget)).expect("said");
    assert_eq!((note.severity, note.text.as_str()), (Severity::Warning, "the ramp ends at RL 145, short of RL 150: the line ends there, 50 m before it would get there at 10 %"));
    // A start 2 m off the line is put on it, and said.
    let record = s.run("string.ramp_polygon", json!({ "start": at(50.0, 2.0, 100.0), "towards": at(100.0, 0.0, 0.0), "line": toe, "width": 30.0, "grade": 10.0, "side": "right", "to": { "kind": "rl", "rl": 105.0 } })).unwrap();
    assert!(record.messages.iter().any(|m| m == "the start was 2 m off 'Toe 4': the ramp starts at the nearest place on it"), "{:?}", record.messages);

    let before = s.project().objects().count();
    let mut refused = |params: Value| s.run("string.ramp_polygon", params).unwrap_err().to_string();
    let base = |to: f64, grade: f64, width: f64| json!({ "start": at(0.0, 0.0, 100.0), "towards": at(100.0, 0.0, 0.0), "width": width, "grade": grade, "side": "left", "to": { "kind": "rl", "rl": to } });
    assert!(refused(base(100.0, 10.0, 30.0)).contains("the start is already at RL 100"));
    assert!(refused(base(120.0, 0.0, 30.0)).contains("grade must be more than zero"));
    assert!(refused(base(120.0, 10.0, 0.0)).contains("width must be more than zero"));
    assert_eq!(s.project().objects().count(), before, "a refusal makes nothing");
}
