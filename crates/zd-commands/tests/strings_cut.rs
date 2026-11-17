//! Cutting strings by another (String tools plan, workstream 1): `string.trim` back to a cutting string keeping a side
//! or splitting at it, `string.extend` on to one at the string's own grade, `string.split` at a point clicked, and
//! `string.join` finished: ends within a tolerance as one vertex, a ring of pieces closed, the pieces' attributes kept
//! where they agree. Every answer is known; mine grid coordinates throughout.

use std::time::Instant;

use serde_json::json;
use zd_commands::{Code, RunRecord, Session, Severity};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId, Polyline};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn line(s: &Session, id: ObjectId) -> Polyline {
    match &s.project().object(id).expect("live string").body {
        Body::String(line) => line.clone(),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

fn string(s: &mut Session, name: &str, points: &[[f64; 3]], closed: bool) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points, "closed": closed })).unwrap())[0]
}

/// A point on the mine grid from local metres.
fn at(x: f64, y: f64, z: f64) -> [f64; 3] {
    [E + x, N + y, z]
}

/// A line across at `x` m east, running north, flat at RL 0: its heights must never reach a string it cuts.
fn across(s: &mut Session, x: f64) -> ObjectId {
    string(s, "Across", &[at(x, -50.0, 0.0), at(x, 250.0, 0.0)], false)
}

/// `count` toes 10 m apart running due east, 100 m long at 10 % from RL 100, a vertex every 50 m.
fn toes(s: &mut Session, count: usize) -> Vec<ObjectId> {
    (0..count).map(|k| string(s, &format!("Toe {}", k + 1), &[at(0.0, 10.0 * k as f64, 100.0), at(50.0, 10.0 * k as f64, 105.0), at(100.0, 10.0 * k as f64, 110.0)], false)).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Fifteen toes trimmed back to a line across 60 m along, the west kept: each ends there at RL 106, its own height,
/// in one run and one undo.
#[test]
fn fifteen_toes_trimmed_to_a_line_across_end_at_their_own_height() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 15);
    let limit = across(&mut s, 60.0);
    let record = s.run("string.trim", json!({ "strings": ids, "by": limit, "side": at(5.0, 5.0, 0.0) })).unwrap();
    assert_eq!(record.messages, ["trimmed 15 of 15 strings by 'Across': cut in 15 places, 15 parts dropped"]);
    for (k, &id) in ids.iter().enumerate() {
        let toe = line(&s, id);
        let end = *toe.vertices.last().unwrap();
        assert_eq!(toe.vertices.len(), 3);
        assert!(close(end[0], E + 60.0) && close(end[1], N + 10.0 * k as f64) && close(end[2], 106.0), "{end:?}");
        assert_eq!(toe.vertices[..2], [at(0.0, 10.0 * k as f64, 100.0), at(50.0, 10.0 * k as f64, 105.0)], "the vertices kept, exactly");
    }
    assert_eq!(line(&s, limit).vertices, [at(60.0, -50.0, 0.0), at(60.0, 250.0, 0.0)], "the cutting string as it was");
    s.undo();
    assert!(ids.iter().all(|&id| line(&s, id).vertices.len() == 3 && close(line(&s, id).vertices[2][0], E + 100.0)), "one undo puts all fifteen back");
}

/// Keep both splits each string where the line crosses it: the first part is the string's next revision and the
/// second a new string named for it, on its layer, carrying its attributes (a strip number here).
#[test]
fn keep_both_splits_each_string_and_the_new_parts_carry_its_attributes() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 3);
    s.run("string.number_across", json!({ "strings": ids, "from": at(20.0, -5.0, 0.0), "to": at(20.0, 30.0, 0.0), "start": 1.0 })).unwrap();
    let cut = across(&mut s, 60.0);
    let record = s.run("string.trim", json!({ "strings": ids, "by": cut, "keep": "both" })).unwrap();
    assert_eq!(record.messages, ["split 3 of 3 strings where 'Across' crosses them: cut in 3 places, 3 parts kept as new strings"]);
    let parts = created(&record);
    assert_eq!(parts.len(), 3);
    for (k, (&id, &part)) in ids.iter().zip(&parts).enumerate() {
        let (first, second) = (line(&s, id), line(&s, part));
        assert_eq!(first.vertices.last(), second.vertices.first(), "the two share the cut");
        assert!(close(second.vertices[0][2], 106.0) && second.vertices.last() == Some(&at(100.0, 10.0 * k as f64, 110.0)));
        let object = s.project().object(part).unwrap();
        assert_eq!(object.name, format!("Strip {} part 2", k + 1));
        assert_eq!(object.attributes.get("strip"), Some(&AttrValue::Number((k + 1) as f64)));
    }
}

/// A closed pit limit keeps what's inside it, or what's outside: a toe running out both sides is kept as two.
#[test]
fn a_closed_limit_keeps_inside_or_outside() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 1);
    let limit = string(&mut s, "Pit limit", &[at(20.0, -20.0, 0.0), at(80.0, -20.0, 0.0), at(80.0, 20.0, 0.0), at(20.0, 20.0, 0.0)], true);
    s.run("string.trim", json!({ "string": ids[0], "by": limit, "side": at(50.0, 5.0, 0.0) })).unwrap();
    let inside = line(&s, ids[0]);
    assert!(close(inside.vertices[0][0], E + 20.0) && close(inside.vertices[0][2], 102.0));
    assert!(close(inside.vertices[2][0], E + 80.0) && close(inside.vertices[2][2], 108.0));
    s.undo();
    let record = s.run("string.trim", json!({ "string": ids[0], "by": limit, "side": at(-10.0, 0.0, 0.0) })).unwrap();
    assert_eq!(record.messages, ["trimmed 1 of 1 strings by 'Pit limit': cut in 2 places, 1 part dropped, 1 part kept as new strings"]);
    assert!(close(line(&s, ids[0]).vertices[1][0], E + 20.0) && close(line(&s, created(&record)[0]).vertices[0][0], E + 80.0));
}

/// What can't be trimmed refuses the run and changes nothing; a string the line doesn't cross is left among several,
/// and refused on its own.
#[test]
fn what_cant_be_trimmed_is_refused_and_one_not_crossed_is_left() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 2);
    let far = string(&mut s, "Far toe", &[at(200.0, 0.0, 100.0), at(300.0, 0.0, 100.0)], false);
    let cut = across(&mut s, 60.0);
    let refused = |s: &mut Session, params: serde_json::Value| s.run("string.trim", params).unwrap_err().to_string();

    assert_eq!(
        refused(&mut s, json!({ "strings": [ids[0], far, ids[1]], "by": cut, "side": at(5.0, 5.0, 0.0) })),
        "1 of 3 strings can't be done, so nothing was changed. 'Far toe': it lies wholly on the side dropped"
    );
    assert_eq!(
        refused(&mut s, json!({ "strings": ids, "by": cut, "side": at(60.0, 5.0, 0.0) })),
        "2 of 2 strings can't be done, so nothing was changed. 'Toe 1': the point saying which side to keep is on the cutting string; pick one to the side of it; 'Toe 2': the point saying which side to keep is on the cutting string; pick one to the side of it"
    );
    assert_eq!(refused(&mut s, json!({ "strings": [ids[0], cut], "by": cut, "side": at(5.0, 5.0, 0.0) })), "'Across' is the cutting string, so it can't be trimmed by itself; leave it out of those picked");
    assert_eq!(refused(&mut s, json!({ "strings": ids, "by": cut })), "which side to keep isn't given: a point on it as `side`, or `keep` both to split");
    assert_eq!(refused(&mut s, json!({ "strings": ids, "by": cut, "keep": "both", "side": at(5.0, 5.0, 0.0) })), "`side` is for keeping one side; with `keep` both every part is kept");
    assert_eq!(refused(&mut s, json!({ "string": far, "by": cut, "side": at(200.0, 5.0, 0.0) })), "'Far toe' isn't crossed by 'Across', so there's nothing to trim");
    assert!([ids[0], ids[1], far].iter().all(|&id| s.project().object(id).unwrap().revision == 1), "every string as it was");

    let record = s.run("string.trim", json!({ "strings": [ids[0], far, ids[1]], "by": cut, "side": at(90.0, 5.0, 0.0) })).unwrap();
    assert_eq!(record.messages, ["trimmed 2 of 3 strings by 'Across': cut in 2 places, 2 parts dropped; 1 not crossed and left as it was: 'Far toe'"]);
    assert_eq!(s.project().object(far).unwrap().revision, 1);
}

/// Three toes 50 m long at 5 % from RL 100, run on to a line across 80 m along: each ends there at RL 104, its own
/// grade carried on, and says how far it ran. One already ending on the line is left and said.
#[test]
fn toes_run_on_to_a_line_at_their_own_grade() {
    let mut s = Session::with_builtins();
    let ids: Vec<ObjectId> = (0..3).map(|k| string(&mut s, &format!("Toe {}", k + 1), &[at(0.0, 10.0 * k as f64, 100.0), at(50.0, 10.0 * k as f64, 102.5)], false)).collect();
    let there = string(&mut s, "There", &[at(0.0, 40.0, 100.0), at(80.0, 40.0, 104.0)], false);
    let limit = across(&mut s, 80.0);
    let mut picked = ids.clone();
    picked.push(there);
    let record = s.run("string.extend", json!({ "strings": picked, "to": limit })).unwrap();
    assert_eq!(record.messages[0], "ran on to 'Across': 3 of 4 strings; 1 came out as it was and is left: 'There'");
    assert_eq!(record.messages[1], "'Toe 1': ran on 30.000 m at its end to 'Across'");
    assert_eq!(record.notes[1].value.as_ref().map(|v| v.value), Some(30.0));
    for (k, &id) in ids.iter().enumerate() {
        assert_eq!(line(&s, id).vertices, [at(0.0, 10.0 * k as f64, 100.0), at(80.0, 10.0 * k as f64, 104.0)]);
    }
    assert_eq!(s.project().object(there).unwrap().revision, 1);
    s.undo();

    let refused = s.run("string.extend", json!({ "strings": ids, "to": limit, "reach": 20.0 })).unwrap_err().to_string();
    assert!(refused.starts_with("3 of 3 strings can't be done, so nothing was changed. 'Toe 1': neither end meets it running on within 20 m"), "{refused}");
    let refused = s.run("string.extend", json!({ "string": there, "to": limit })).unwrap_err().to_string();
    assert_eq!(refused, "'There' already ends on 'Across'");
}

/// Split at a point clicked a hair off the string: a vertex goes in at the string's height there. Within a millimetre
/// of a vertex it splits at the vertex. `at` still splits at a vertex by number.
#[test]
fn a_split_at_a_point_puts_a_vertex_in_at_the_strings_height() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 1);
    let record = s.run("string.split", json!({ "string": ids[0], "point": at(30.0, 0.4, 0.0) })).unwrap();
    assert_eq!(record.messages, ["split at a new vertex: 2 and 3 vertices"]);
    let (first, second) = (line(&s, ids[0]), line(&s, created(&record)[0]));
    assert!(close(first.vertices[1][0], E + 30.0) && first.vertices[1][1] == N && close(first.vertices[1][2], 103.0));
    assert_eq!(first.vertices[1], second.vertices[0]);
    s.undo();
    let record = s.run("string.split", json!({ "string": ids[0], "point": at(50.0005, 0.0, 0.0) })).unwrap();
    assert_eq!(record.messages, ["split at vertex 2: 2 and 2 vertices"]);
    s.undo();
    let refused = |s: &mut Session, params: serde_json::Value| s.run("string.split", params).unwrap_err().to_string();
    assert_eq!(refused(&mut s, json!({ "string": ids[0], "at": 1, "point": at(30.0, 0.0, 0.0) })), "give `at` or `point`, not both");
    assert!(refused(&mut s, json!({ "string": ids[0], "point": at(30.0, 3.0, 0.0) })).starts_with("the point is 3.000 m from the string in plan"));
    assert_eq!(s.run("string.split", json!({ "string": ids[0], "at": 1 })).unwrap().messages, ["split at vertex 2: 2 and 2 vertices"]);
}

/// Ends 5 mm apart join as one vertex at the default centimetre, with nothing to warn of; 2 cm apart are bridged.
#[test]
fn ends_within_the_tolerance_join_as_one_vertex() {
    let mut s = Session::with_builtins();
    let a = string(&mut s, "a", &[at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0)], false);
    let b = string(&mut s, "b", &[at(10.005, 0.0, 50.0), at(20.0, 0.0, 50.0)], false);
    let record = s.run("string.join", json!({ "strings": [a, b] })).unwrap();
    assert_eq!(record.messages, ["joined 2 strings into 'a': 3 vertices; 0 joins met, 1 within 0.010 m made one vertex"]);
    assert_eq!(line(&s, a).vertices, [at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0), at(20.0, 0.0, 50.0)], "the first piece's end kept");
    let c = string(&mut s, "c", &[at(20.02, 0.0, 50.0), at(30.0, 0.0, 50.0)], false);
    let record = s.run("string.join", json!({ "strings": [a, c] })).unwrap();
    assert_eq!(record.notes[1].code, Some(Code::StringGapBridged));
    assert_eq!(line(&s, a).vertices.len(), 5);
}

/// The four sides of a pit limit drawn as pieces join into one closed string of four corners.
#[test]
fn a_ring_of_pieces_closes() {
    let mut s = Session::with_builtins();
    let corners = [at(0.0, 0.0, 80.0), at(100.0, 0.0, 80.0), at(100.0, 100.0, 80.0), at(0.0, 100.0, 80.0)];
    let sides: Vec<ObjectId> = (0..4).map(|k| string(&mut s, &format!("Side {}", k + 1), &[corners[k], corners[(k + 1) % 4]], false)).collect();
    let record = s.run("string.join", json!({ "strings": [sides[0], sides[2], sides[1], sides[3]] })).unwrap();
    assert_eq!(record.messages, ["joined 4 strings into 'Side 1': 4 vertices; 4 joins met; closed, as its two ends meet"]);
    assert_eq!(line(&s, sides[0]), Polyline { vertices: corners.to_vec(), closed: true });
}

/// The kept string takes a strip number the other pieces agree on; where they differ it's left off and said, and
/// where one differs from the kept string's own, the kept string's stands and that's said.
#[test]
fn the_pieces_attributes_are_kept_where_they_agree() {
    let mut s = Session::with_builtins();
    let number = |s: &mut Session, id: ObjectId, x: f64, strip: f64| {
        s.run("string.number_across", json!({ "strings": [id], "from": at(x, -5.0, 0.0), "to": at(x, 5.0, 0.0), "start": strip })).unwrap();
    };
    let pieces = |s: &mut Session| -> Vec<ObjectId> {
        (0..3).map(|k| string(s, &format!("Piece {}", k + 1), &[at(10.0 * k as f64, 0.0, 50.0), at(10.0 * (k + 1) as f64, 0.0, 50.0)], false)).collect()
    };
    let p = pieces(&mut s);
    number(&mut s, p[1], 15.0, 3.0);
    number(&mut s, p[2], 25.0, 3.0);
    let record = s.run("string.join", json!({ "strings": p })).unwrap();
    assert_eq!(s.project().object(p[0]).unwrap().attributes.get("strip"), Some(&AttrValue::Number(3.0)));
    assert!(record.notes.iter().all(|n| n.code != Some(Code::StringAttributesDiffer)));

    let p = pieces(&mut s);
    number(&mut s, p[1], 15.0, 3.0);
    number(&mut s, p[2], 25.0, 4.0);
    let record = s.run("string.join", json!({ "strings": p })).unwrap();
    assert_eq!(s.project().object(p[0]).unwrap().attributes.get("strip"), None);
    let note = record.notes.iter().find(|n| n.code == Some(Code::StringAttributesDiffer)).expect("said");
    assert_eq!(note.severity, Severity::Warning);
    assert_eq!(note.text, "'strip' left off 'Piece 1': the pieces differ on it ('Strip 3' 3, 'Strip 4' 4)");

    let p = pieces(&mut s);
    number(&mut s, p[0], 5.0, 7.0);
    number(&mut s, p[1], 15.0, 8.0);
    let record = s.run("string.join", json!({ "strings": p })).unwrap();
    assert_eq!(s.project().object(p[0]).unwrap().attributes.get("strip"), Some(&AttrValue::Number(7.0)));
    let note = record.notes.iter().find(|n| n.code == Some(Code::StringAttributesDiffer)).expect("said");
    assert_eq!(note.text, "'strip' kept as 'Strip 7' had it, 7; 'Strip 8' had 8");
}

/// Release timings: 15 and 150 toes of 40 vertices trimmed by a bent pit limit and run on to a line, each one run.
#[test]
#[ignore = "timing; run in release"]
fn timings_for_trim_and_extend() {
    for count in [15, 150] {
        let mut s = Session::with_builtins();
        let ids: Vec<ObjectId> = (0..count)
            .map(|k| {
                let y = 10.0 * k as f64;
                let points: Vec<[f64; 3]> = (0..40).map(|i| at(10.0 * i as f64, y + 3.0 * (i as f64 / 4.0).sin(), 100.0 + 0.1 * i as f64)).collect();
                string(&mut s, &format!("Toe {}", k + 1), &points, false)
            })
            .collect();
        let bent: Vec<[f64; 3]> = (0..60).map(|i| at(300.0 + 20.0 * (i as f64 / 5.0).sin(), -20.0 + 30.0 * i as f64, 0.0)).collect();
        let limit = string(&mut s, "Pit limit", &bent, false);
        let started = Instant::now();
        s.run("string.trim", json!({ "strings": ids, "by": limit, "side": at(0.0, 0.0, 0.0) })).unwrap();
        let trim = started.elapsed().as_secs_f64() * 1e3;
        let line_at = string(&mut s, "Line past them", &[at(400.0, -50.0, 0.0), at(400.0, 10.0 * count as f64 + 50.0, 0.0)], false);
        let started = Instant::now();
        s.run("string.extend", json!({ "strings": ids, "to": line_at, "reach": 200.0 })).unwrap();
        let extend = started.elapsed().as_secs_f64() * 1e3;
        println!("{count} toes of 40 vertices: trimmed by a 60-vertex limit in {trim:.1} ms, run on to a line in {extend:.1} ms");
    }
}
