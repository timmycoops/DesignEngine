//! Block polygons through the registry, as the app calls them (decision 0062): lines across the strips every 100 m and
//! two lines closing them make a polygon of each area, named west to east; ends that stop short are joined or listed.

use serde_json::json;
use zd_commands::{Code, RunRecord, Session, Severity};
use zd_core::{Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn line(s: &mut Session, name: &str, points: &[(f64, f64)]) -> ObjectId {
    let points: Vec<[f64; 3]> = points.iter().map(|&(x, y)| [E + x, N + y, 0.0]).collect();
    created(&s.run("string.create", json!({ "name": name, "layer": "block lines", "points": points })).unwrap())[0]
}

/// Plan area of a closed string, by the shoelace.
fn area(s: &Session, id: ObjectId) -> f64 {
    let Body::String(polyline) = &s.project().object(id).unwrap().body else { panic!("a string") };
    assert!(polyline.closed);
    let v = &polyline.vertices;
    let n = v.len();
    (0..n).map(|k| (v[k][0] - E) * (v[(k + 1) % n][1] - N) - (v[(k + 1) % n][0] - E) * (v[k][1] - N)).sum::<f64>() / 2.0
}

/// A south and a north line 360 m apart, and five lines across them 100 m apart running 50 m past: four blocks of
/// 100 × 360 m, named west to east, in one undo.
#[test]
fn lines_across_and_two_closing_lines_make_named_blocks() {
    let mut s = Session::with_builtins();
    let mut lines = vec![line(&mut s, "south", &[(-20.0, 0.0), (420.0, 0.0)]), line(&mut s, "north", &[(-20.0, 360.0), (420.0, 360.0)])];
    for k in 0..5 {
        let x = 100.0 * k as f64;
        lines.push(line(&mut s, &format!("across {k}"), &[(x, -50.0), (x, 410.0)]));
    }
    let before = s.project().objects().count();
    let record = s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "blocks", "name": "Block #" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 4);
    for (k, &id) in made.iter().enumerate() {
        let object = s.project().object(id).unwrap();
        assert_eq!(object.name, format!("Block {}", k + 1));
        assert!((area(&s, id) - 36_000.0).abs() < 0.001, "{}", area(&s, id));
    }
    assert!(record.notes.iter().all(|n| n.severity != Severity::Warning), "{:?}", record.notes);
    assert!(record.messages.iter().any(|m| m.contains("14 ends run on past")), "{:?}", record.messages);
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), before);
}

/// The middle line across stops 5 mm short of the north line: joined at 1 cm and said; at the 1 mm default, listed
/// with its gap and place, and the two blocks either side of it are one.
#[test]
fn an_end_stopping_short_is_joined_within_the_tolerance_or_listed() {
    let mut s = Session::with_builtins();
    let mut lines = vec![line(&mut s, "south", &[(0.0, 0.0), (200.0, 0.0)]), line(&mut s, "north", &[(0.0, 360.0), (200.0, 360.0)])];
    lines.push(line(&mut s, "west", &[(0.0, 0.0), (0.0, 360.0)]));
    lines.push(line(&mut s, "middle", &[(100.0, 0.0), (100.0, 359.995)]));
    lines.push(line(&mut s, "east", &[(200.0, 0.0), (200.0, 360.0)]));

    let joined = s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "blocks", "tolerance": 0.01 })).unwrap();
    assert_eq!(created(&joined).len(), 2);
    let note = joined.notes.iter().find(|n| n.code == Some(Code::StringEndJoined)).expect("the join is said");
    assert!(note.text.contains("'middle'") && note.text.contains("5.0 mm"), "{}", note.text);

    let open = s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "blocks" })).unwrap();
    let made = created(&open);
    assert_eq!(made.len(), 1);
    assert!((area(&s, made[0]) - 72_000.0).abs() < 0.001);
    let short = open.notes.iter().find(|n| n.code == Some(Code::StringEndShort)).expect("the short end is listed");
    assert_eq!(short.severity, Severity::Warning);
    assert!(short.text.contains("'middle' ends 0.005 m short"), "{}", short.text);
    assert!((short.value.as_ref().unwrap().value - 0.005).abs() < 1e-6);
}

/// Lines that close nothing are refused, saying why; a line closing nothing beside ones that do is named.
#[test]
fn lines_closing_nothing_are_refused_or_named() {
    let mut s = Session::with_builtins();
    let a = line(&mut s, "a", &[(0.0, 0.0), (100.0, 0.0)]);
    let b = line(&mut s, "b", &[(0.0, 50.0), (100.0, 50.0)]);
    let refused = s.run("string.polygons_from_lines", json!({ "strings": [a, b], "layer": "blocks" })).unwrap_err();
    assert!(refused.to_string().contains("close no area"), "{refused}");

    let square = created(&s.run("string.create", json!({ "name": "square", "layer": "l", "closed": true,
        "points": [[E, N + 100.0, 0.0], [E + 50.0, N + 100.0, 0.0], [E + 50.0, N + 150.0, 0.0], [E, N + 150.0, 0.0]] })).unwrap())[0];
    let record = s.run("string.polygons_from_lines", json!({ "strings": [a, square], "layer": "blocks" })).unwrap();
    assert_eq!(created(&record).len(), 1);
    let unused = record.notes.iter().find(|n| n.code == Some(Code::StringClosesNothing)).unwrap();
    assert!(unused.text.contains("'a' closes no area"));
}
