//! `string.join` on several strings (decision 0078, workstream 3): a toe that came in as pieces put back together
//! exactly, gaps bridged and said with their size, the nearest ends joined first, and a run that can't be done
//! changing nothing.

use std::time::Instant;

use serde_json::json;
use zd_commands::{Code, RunRecord, Session, Severity};
use zd_core::{Body, ChangeKind, ObjectId, Polyline};

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
    created(&s.run("string.create", json!({ "name": name, "layer": "pieces", "points": points, "closed": closed })).unwrap())[0]
}

/// A point on the mine grid from local metres.
fn at(x: f64, y: f64, z: f64) -> [f64; 3] {
    [E + x, N + y, z]
}

fn strings_live(s: &Session) -> usize {
    s.project().objects().filter(|o| matches!(o.body, Body::String(_))).count()
}

/// A bent, graded toe of nine vertices on mine grid coordinates.
fn toe() -> Vec<[f64; 3]> {
    (0..9).map(|k| at(60.0 * k as f64, 7.5 * (k * k) as f64 - 20.0 * k as f64, 100.0 + 0.6 * k as f64)).collect()
}

fn turned(points: &[[f64; 3]]) -> Vec<[f64; 3]> {
    points.iter().rev().copied().collect()
}

/// A toe cut into four pieces at its vertices, given out of order and two of them turned round, comes back as the
/// toe: the same nine vertices exactly, running the way the first piece given does, in one run and one undo.
#[test]
fn a_toe_in_four_pieces_is_put_back_exactly_in_one_run() {
    let toe = toe();
    let mut s = Session::with_builtins();
    let b = string(&mut s, "Toe", &toe[2..=4], false);
    let d = string(&mut s, "Piece D", &turned(&toe[6..=8]), false);
    let a = string(&mut s, "Piece A", &turned(&toe[0..=2]), false);
    let c = string(&mut s, "Piece C", &toe[4..=6], false);
    let record = s.run("string.join", json!({ "strings": [b, d, a, c] })).unwrap();
    assert_eq!(record.messages, ["joined 4 strings into 'Toe': 9 vertices; 3 joins met"]);
    assert_eq!(line(&s, b), Polyline { vertices: toe.clone(), closed: false }, "exactly the toe, mine grid coordinates and all");
    assert_eq!(record.outputs.iter().filter(|o| o.change == ChangeKind::Revised).map(|o| o.id).collect::<Vec<_>>(), [b]);
    assert_eq!(record.outputs.iter().filter(|o| o.change == ChangeKind::Deleted).count(), 3);
    assert!([d, a, c].iter().all(|&id| s.project().object(id).is_none()), "the other pieces are gone");
    let kept = s.project().object(b).unwrap();
    assert_eq!((kept.name.as_str(), kept.revision), ("Toe", 2));

    // One undo brings all four back as they were.
    s.undo();
    assert_eq!(strings_live(&s), 4);
    assert_eq!(line(&s, b).vertices, toe[2..=4]);
    assert_eq!(line(&s, a).vertices, turned(&toe[0..=2]));

    // Given first, a piece that runs the other way turns the whole string its way.
    let record = s.run("string.join", json!({ "strings": [a, c, b, d] })).unwrap();
    assert_eq!(record.messages, ["joined 4 strings into 'Piece A': 9 vertices; 3 joins met"]);
    assert_eq!(line(&s, a).vertices, turned(&toe));
}

/// A gap is bridged by a segment and warned of with its size: a 3-4-5 gap is 5 m, 5 m in plan.
#[test]
fn a_gap_is_bridged_and_warned_of_with_its_size() {
    let mut s = Session::with_builtins();
    let a = string(&mut s, "a", &[at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0)], false);
    let b = string(&mut s, "b", &[at(13.0, 4.0, 50.0), at(30.0, 4.0, 50.0)], false);
    let c = string(&mut s, "c", &[at(40.0, 4.0, 50.0), at(30.0, 4.0, 50.0)], false);
    let record = s.run("string.join", json!({ "strings": [a, b, c] })).unwrap();
    assert_eq!(record.messages[0], "joined 3 strings into 'a': 5 vertices; 1 join met, 1 gap bridged, 5.000 m in all");
    assert_eq!(record.messages[1], "bridged a 5.000 m gap between 'a' and 'b' at 650010.000, 7450000.000, 5.000 m in plan");
    assert_eq!(record.notes.len(), 2);
    let gap = &record.notes[1];
    assert_eq!((gap.severity, gap.code, gap.object), (Severity::Warning, Some(Code::StringGapBridged), Some(a)));
    assert_eq!(gap.value.as_ref().map(|v| (v.value, v.unit.as_str())), Some((5.0, "m")));
    assert_eq!(line(&s, a).vertices, [at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0), at(13.0, 4.0, 50.0), at(30.0, 4.0, 50.0), at(40.0, 4.0, 50.0)]);
}

/// The nearest ends join first. `b` starts half a metre past `a`'s end, so they join; `c` starts nearer `b`'s far end
/// (√73 m) than `a`'s start (√153 m), so it goes on after `b`. The gaps are listed biggest first.
#[test]
fn the_nearest_ends_join_first_and_the_biggest_gap_is_said_first() {
    let mut s = Session::with_builtins();
    let c = string(&mut s, "c", &[at(12.0, 3.0, 50.0), at(12.0, 30.0, 50.0)], false);
    let a = string(&mut s, "a", &[at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0)], false);
    let b = string(&mut s, "b", &[at(10.5, 0.0, 50.0), at(20.0, 0.0, 50.0)], false);
    let record = s.run("string.join", json!({ "strings": [a, c, b] })).unwrap();
    assert_eq!(
        line(&s, a).vertices,
        [at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0), at(10.5, 0.0, 50.0), at(20.0, 0.0, 50.0), at(12.0, 3.0, 50.0), at(12.0, 30.0, 50.0)]
    );
    let gaps: Vec<f64> = record.notes[1..].iter().map(|n| n.value.as_ref().unwrap().value).collect();
    assert_eq!(gaps.len(), 2);
    assert!((gaps[0] - 73f64.sqrt()).abs() < 1e-9 && (gaps[1] - 0.5).abs() < 1e-9, "{gaps:?}");
    assert!(record.messages[0].ends_with("0 joins met, 2 gaps bridged, 9.044 m in all"), "{}", record.messages[0]);
    assert!(record.messages[1].starts_with("bridged a 8.544 m gap between 'b' and 'c'"), "{}", record.messages[1]);
}

/// Two ends over each other in plan at different RLs are joined by a segment straight up, and that's said: 5 m, none
/// of it in plan. A gap under half a millimetre is said in full, not as 0.000 m.
#[test]
fn a_gap_straight_up_and_a_hair_of_a_gap_are_said_as_they_are() {
    let mut s = Session::with_builtins();
    let a = string(&mut s, "a", &[at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0)], false);
    let b = string(&mut s, "b", &[at(10.0, 0.0, 55.0), at(20.0, 0.0, 55.0)], false);
    let record = s.run("string.join", json!({ "strings": [a, b] })).unwrap();
    assert_eq!(record.messages[1], "bridged a 5.000 m gap between 'a' and 'b' at 650010.000, 7450000.000, straight up: the two ends are over each other in plan");
    assert_eq!(line(&s, a).vertices.len(), 4);

    let c = string(&mut s, "c", &[at(20.0, 0.0001, 55.0), at(30.0, 0.0, 55.0)], false);
    let record = s.run("string.join", json!({ "strings": [a, c] })).unwrap();
    assert!(record.messages[1].starts_with("bridged a 0.000100 m gap between 'a' and 'c'"), "{}", record.messages[1]);
}

/// What can't be joined refuses the run, and nothing is changed: a closed string among them, one string alone, a
/// string twice, both ways of giving the strings, and three ends meeting at a place.
#[test]
fn what_cannot_be_joined_is_refused_and_nothing_is_changed() {
    let mut s = Session::with_builtins();
    let a = string(&mut s, "a", &[at(0.0, 0.0, 50.0), at(10.0, 0.0, 50.0)], false);
    let b = string(&mut s, "b", &[at(10.0, 0.0, 50.0), at(20.0, 0.0, 50.0)], false);
    let ring = string(&mut s, "ring", &[at(50.0, 0.0, 50.0), at(60.0, 0.0, 50.0), at(60.0, 10.0, 50.0)], true);
    let spur = string(&mut s, "spur", &[at(10.0, 0.0, 50.0), at(10.0, 20.0, 50.0)], false);
    let refused = |s: &mut Session, params: serde_json::Value| s.run("string.join", params).unwrap_err().to_string();

    assert_eq!(refused(&mut s, json!({ "strings": [a, ring, b] })), "1 of 3 strings can't be done, so nothing was changed. 'ring': a closed string has no ends to join; open it first");
    assert_eq!(refused(&mut s, json!({ "strings": [a] })), "joining takes two strings or more; 1 was given");
    assert!(refused(&mut s, json!({ "strings": [a, b, a] })).ends_with("is in `strings` twice"));
    assert_eq!(refused(&mut s, json!({ "string": a, "other": b, "strings": [a, b] })), "give `string` and `other`, or `strings`, not both");
    assert_eq!(refused(&mut s, json!({ "string": a })), "joining one string onto another takes both `string` and `other`");
    assert_eq!(refused(&mut s, json!({})), "no strings to join: give `string` and `other`, or two or more as `strings`");
    assert_eq!(
        refused(&mut s, json!({ "strings": [a, b, spur] })),
        "3 ends meet at 650010.000, 7450000.000 ('a', 'b', 'spur'), so which two join isn't known and nothing was changed. Join two of them first, or leave one out"
    );

    assert_eq!(strings_live(&s), 4);
    assert!([a, b, ring, spur].iter().all(|&id| s.project().object(id).unwrap().revision == 1), "every string as it was");
    assert_eq!(s.project().undo_steps(), 4, "only the four strings made");

    // One onto another still runs as it did, in the same words.
    let record = s.run("string.join", json!({ "string": a, "other": b })).unwrap();
    assert_eq!(record.messages, ["joined 'b' on: 3 vertices"]);
}

/// Release timings: a toe in pieces of 40 vertices, joined in one run. Meeting pieces are joined by place; pieces a
/// metre apart are every pair of loose ends measured.
#[test]
#[ignore = "timing; run in release"]
fn timings_for_joining_pieces() {
    for (pieces, gap) in [(15, 0.0), (150, 0.0), (1_500, 0.0), (15, 1.0), (150, 1.0), (1_500, 1.0)] {
        let mut s = Session::with_builtins();
        let ids: Vec<ObjectId> = (0..pieces)
            .map(|k| {
                // Every other piece is drawn the other way.
                let x0 = k as f64 * (390.0 + gap);
                let mut points: Vec<[f64; 3]> = (0..40).map(|i| at(x0 + 10.0 * i as f64, 25.0 * ((x0 + 10.0 * i as f64) / 300.0).sin(), 100.0)).collect();
                if k % 2 == 1 {
                    points.reverse();
                }
                string(&mut s, &format!("Piece {}", k + 1), &points, false)
            })
            .collect();
        let started = Instant::now();
        let record = s.run("string.join", json!({ "strings": ids })).unwrap();
        let took = started.elapsed().as_secs_f64() * 1e3;
        let out = line(&s, ids[0]).vertices.len();
        println!("{pieces} pieces, {} m apart: {took:.1} ms, {} vertices in, {out} out, {} gaps", gap, pieces * 40, record.notes.len() - 1);
        assert_eq!(out, if gap == 0.0 { pieces * 39 + 1 } else { pieces * 40 });
    }
}
