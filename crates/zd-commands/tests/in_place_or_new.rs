//! In place or as new on a layer (decision 0091): any command that changes an object, run with `result` as new, makes
//! each changed object anew, named for what was done, on the layer given or its original's, and leaves the originals as
//! they were, deleting nothing. Every answer is known; mine grid coordinates throughout.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId, Polyline};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn changed(record: &RunRecord, kind: ChangeKind) -> usize {
    record.outputs.iter().filter(|o| o.change == kind).count()
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

fn layer(s: &Session, id: ObjectId) -> String {
    let object = s.project().object(id).unwrap();
    s.project().layer(object.layer).unwrap().name.clone()
}

fn revision(s: &Session, id: ObjectId) -> u32 {
    s.project().object(id).unwrap().revision
}

fn string(s: &mut Session, name: &str, points: &[[f64; 3]]) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points, "closed": false })).unwrap())[0]
}

/// Two toes, flat at RL 100, graded at 10 % as new on 'graded': two new strings "Toe 1 graded" and "Toe 2 graded" there,
/// each rising 10 m over its 100 m; the toes at their revisions and heights; the run's record keeps `result`; one undo
/// takes the copies off. In place, the same run revises the toes.
#[test]
fn graded_as_new_leaves_the_toes_and_makes_graded_copies_on_a_layer() {
    let mut s = Session::with_builtins();
    let toes = [0.0, 20.0].map(|y| string(&mut s, if y == 0.0 { "Toe 1" } else { "Toe 2" }, &[[E, N + y, 100.0], [E + 50.0, N + y, 100.0], [E + 100.0, N + y, 100.0]]));
    let before = toes.map(|id| revision(&s, id));
    let record = s.run("string.set_grade", json!({ "strings": toes, "grade": 10.0, "result": { "as_new": true, "layer": "graded" } })).unwrap();
    let made = created(&record);
    assert_eq!((made.len(), changed(&record, ChangeKind::Revised), changed(&record, ChangeKind::Deleted)), (2, 0, 0));
    assert_eq!(made.iter().map(|&id| name(&s, id)).collect::<Vec<_>>(), ["Toe 1 graded", "Toe 2 graded"]);
    for (&copy, &toe) in made.iter().zip(&toes) {
        assert_eq!(layer(&s, copy), "graded");
        let (graded, original) = (line(&s, copy), line(&s, toe));
        assert!((graded.vertices[2][2] - graded.vertices[0][2] - 10.0).abs() < 1e-9, "{:?}", graded.vertices);
        assert!(original.vertices.iter().all(|p| p[2] == 100.0), "the toe as it was");
    }
    assert_eq!(toes.map(|id| revision(&s, id)), before, "the toes at their revisions");
    assert_eq!(record.params["result"], json!({ "as_new": true, "layer": "graded" }), "the record keeps how it was made");
    assert!(record.messages.last().unwrap().contains("made as new, on 'graded': 2 copies"), "{:?}", record.messages);
    s.undo().unwrap();
    assert!(made.iter().all(|&id| s.project().object(id).is_none()), "one undo takes the copies off");

    let record = s.run("string.set_grade", json!({ "strings": toes, "grade": 10.0 })).unwrap();
    assert_eq!((created(&record).len(), changed(&record, ChangeKind::Revised)), (0, 2), "in place, the default, revises them");
}

/// A toe split where a line crosses it, as new with no layer: the first part "Toe 1 trimmed" and the second
/// "Toe 1 part 2", both on the toe's own layer; the toe whole and as it was.
#[test]
fn a_trim_as_new_with_no_layer_keeps_each_on_its_originals_layer() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe 1", &[[E, N, 100.0], [E + 100.0, N, 110.0]]);
    let across = string(&mut s, "Across", &[[E + 60.0, N - 50.0, 0.0], [E + 60.0, N + 50.0, 0.0]]);
    let record = s.run("string.trim", json!({ "strings": [toe], "by": across, "keep": "both", "result": { "as_new": true } })).unwrap();
    let made = created(&record);
    assert_eq!(made.iter().map(|&id| name(&s, id)).collect::<Vec<_>>(), ["Toe 1 trimmed", "Toe 1 part 2"]);
    assert!(made.iter().all(|&id| layer(&s, id) == "toes"));
    assert_eq!(line(&s, toe).vertices.len(), 2, "the toe whole");
    assert!((line(&s, made[0]).vertices[1][2] - 106.0).abs() < 1e-9, "the cut at the toe's own height");
}

/// Three pieces joined as new: one new string, and all three pieces still there, where in place two would be deleted.
#[test]
fn a_join_as_new_deletes_nothing() {
    let mut s = Session::with_builtins();
    let pieces: Vec<ObjectId> = (0..3).map(|k| string(&mut s, &format!("Piece {}", k + 1), &[[E + 10.0 * k as f64, N, 50.0], [E + 10.0 * (k + 1) as f64, N, 50.0]])).collect();
    let record = s.run("string.join", json!({ "strings": pieces, "result": { "as_new": true, "layer": "joined" } })).unwrap();
    let made = created(&record);
    assert_eq!((made.len(), changed(&record, ChangeKind::Deleted)), (1, 0));
    assert_eq!(line(&s, made[0]).vertices.len(), 4);
    assert!(pieces.iter().all(|&id| s.project().object(id).is_some()), "every piece still there");
    assert!(record.messages.last().unwrap().contains("2 kept that a change in place would have deleted"), "{:?}", record.messages);
}

/// Move as new is what Copy was: "Toe 1 copy" on the toe's layer, moved 100 m east, the toe where it was.
#[test]
fn a_move_as_new_is_a_copy() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe 1", &[[E, N, 100.0], [E + 50.0, N, 105.0]]);
    let record = s.run("object.transform", json!({ "objects": [toe], "transform": { "kind": "move", "by": [100.0, 0.0, 0.0] }, "result": { "as_new": true } })).unwrap();
    let copy = created(&record)[0];
    assert_eq!((name(&s, copy), layer(&s, copy)), ("Toe 1 copy".to_string(), "toes".to_string()));
    assert_eq!(line(&s, copy).vertices[0], [E + 100.0, N, 100.0]);
    assert_eq!(line(&s, toe).vertices[0], [E, N, 100.0]);
}

/// What `result` can't be: an unknown setting, or a blank layer, is refused and nothing changes. A tool that makes
/// something new anyway, run as new, says it made no difference.
#[test]
fn what_result_cant_be() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe 1", &[[E, N, 100.0], [E + 50.0, N, 100.0]]);
    let refused = |s: &mut Session, result: serde_json::Value| s.run("string.set_grade", json!({ "strings": [toe], "grade": 5.0, "result": result })).unwrap_err().to_string();
    assert!(refused(&mut s, json!({ "as_new": true, "colour": "red" })).contains("`result`"));
    assert!(refused(&mut s, json!({ "as_new": true, "layer": "  " })).contains("blank"));
    assert_eq!(revision(&s, toe), 1, "nothing changed");
    let record = s.run("string.offset", json!({ "strings": [toe], "distance": 10.0, "side": "left", "result": { "as_new": true } })).unwrap();
    assert!(record.messages.last().unwrap().contains("made no difference"), "{:?}", record.messages);
}
