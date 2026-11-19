//! Polygons (String tools plan, workstream 6; decision 0083): `string.clip` keeping the parts of strings inside or
//! outside a closed string, and `string.polygon_boolean`, the union, overlap or difference of closed strings in plan.
//! Every answer is known; mine grid coordinates throughout.

use std::time::Instant;

use serde_json::json;
use zd_commands::{RunRecord, Session};
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

fn name(s: &Session, id: ObjectId) -> String {
    s.project().object(id).unwrap().name.clone()
}

fn string(s: &mut Session, name: &str, points: &[[f64; 3]], closed: bool) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points, "closed": closed })).unwrap())[0]
}

fn at(x: f64, y: f64, z: f64) -> [f64; 3] {
    [E + x, N + y, z]
}

/// A rectangle from `x0`, `y0` to `x1`, `y1` m off (E, N), flat at `rl`.
fn rect(s: &mut Session, name: &str, [x0, y0, x1, y1]: [f64; 4], rl: f64) -> ObjectId {
    string(s, name, &[at(x0, y0, rl), at(x1, y0, rl), at(x1, y1, rl), at(x0, y1, rl)], true)
}

fn plan_area(l: &Polyline) -> f64 {
    let o = l.vertices[0];
    let n = l.vertices.len();
    (0..n).map(|k| {
        let (p, q) = (l.vertices[k], l.vertices[(k + 1) % n]);
        (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1])
    }).sum::<f64>().abs() / 2.0
}

/// `count` toes 10 m apart running due east from 50 m west of the pit limit to 150 m east of its west edge, at 10 %
/// from RL 100, a vertex every 50 m.
fn toes(s: &mut Session, count: usize) -> Vec<ObjectId> {
    (0..count)
        .map(|k| {
            let y = 5.0 + 10.0 * k as f64;
            string(s, &format!("Toe {}", k + 1), &[at(-50.0, y, 100.0), at(0.0, y, 105.0), at(50.0, y, 110.0), at(100.0, y, 115.0), at(150.0, y, 120.0)], false)
        })
        .collect()
}

/// Fifteen toes clipped to a 100 m by 150 m pit limit: each keeps its 100 m inside, from RL 105 to RL 115, at its own
/// heights, in one run and one undo; outside, each keeps two ends, the second a new string.
#[test]
fn fifteen_toes_clipped_to_a_pit_limit() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 15);
    let limit = rect(&mut s, "Pit limit", [0.0, 0.0, 100.0, 150.0], 0.0);
    let record = s.run("string.clip", json!({ "strings": ids, "by": limit })).unwrap();
    assert!(created(&record).is_empty());
    for &id in &ids {
        let l = line(&s, id);
        assert_eq!(l.vertices.len(), 3, "the vertex at 50 m kept, and one at each cut");
        assert!((l.vertices[0][0] - E).abs() < 1e-9 && (l.vertices[0][2] - 105.0).abs() < 1e-9);
        assert!((l.vertices[2][0] - (E + 100.0)).abs() < 1e-9 && (l.vertices[2][2] - 115.0).abs() < 1e-9);
    }
    assert_eq!(record.messages, ["clipped 15 of 15 strings to inside 'Pit limit': cut in 30 places, 30 parts dropped"]);
    s.undo().unwrap();
    assert_eq!(line(&s, ids[0]).vertices.len(), 5, "one undo puts every toe back");

    let record = s.run("string.clip", json!({ "strings": ids, "by": limit, "keep": "outside" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 15, "the east end of each, as a new string");
    assert_eq!(name(&s, made[0]), "Toe 1 part 2");
    assert_eq!(line(&s, ids[0]).vertices[0], at(-50.0, 5.0, 100.0));
}

/// What can't be clipped: an open string to clip to, a string wholly outside the limit when inside is kept (the run
/// refused, nothing changed), and a string wholly inside already, alone, which has nothing to clip.
#[test]
fn what_cant_be_clipped_is_refused() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 2);
    let limit = rect(&mut s, "Pit limit", [0.0, 0.0, 100.0, 150.0], 0.0);
    let open = string(&mut s, "Open", &[at(0.0, 0.0, 0.0), at(100.0, 0.0, 0.0), at(100.0, 150.0, 0.0)], false);
    let far = string(&mut s, "Far", &[at(300.0, 0.0, 0.0), at(400.0, 0.0, 0.0)], false);
    let inside = string(&mut s, "Inside", &[at(10.0, 10.0, 0.0), at(90.0, 10.0, 0.0)], false);
    let revisions = ids.iter().map(|&id| s.project().object(id).unwrap().revision).collect::<Vec<_>>();

    let refused = |s: &mut Session, params: serde_json::Value| s.run("string.clip", params).unwrap_err().to_string();
    assert!(refused(&mut s, json!({ "strings": ids, "by": open })).contains("isn't closed"));
    let words = refused(&mut s, json!({ "strings": [ids[0], ids[1], far], "by": limit }));
    assert!(words.contains("1 of 3 strings can't be done") && words.contains("'Far': it lies wholly outside"), "{words}");
    assert_eq!(ids.iter().map(|&id| s.project().object(id).unwrap().revision).collect::<Vec<_>>(), revisions, "nothing changed");
    assert!(refused(&mut s, json!({ "string": inside, "by": limit })).contains("nothing to clip"));
    assert!(refused(&mut s, json!({ "strings": [limit], "by": limit })).contains("clipped by itself"));
}

/// Two 100 m squares overlapping by half, one at RL 100 and one at RL 120: union 15,000 m², overlap 5,000 m² and
/// difference 5,000 m², each one new closed string on the layer asked for, named for the run; the squares as they were.
#[test]
fn two_squares_overlapping_by_half() {
    let mut s = Session::with_builtins();
    let a = rect(&mut s, "Block A", [0.0, 0.0, 100.0, 100.0], 100.0);
    let b = rect(&mut s, "Block B", [50.0, 0.0, 150.0, 100.0], 120.0);
    let revisions = [a, b].map(|id| s.project().object(id).unwrap().revision);
    for (operation, area, word) in [("union", 15_000.0, "Union"), ("overlap", 5_000.0, "Overlap"), ("difference", 5_000.0, "Difference")] {
        let made = created(&s.run("string.polygon_boolean", json!({ "strings": [a], "with": [b], "operation": operation, "layer": "limits" })).unwrap());
        assert_eq!(made.len(), 1, "{operation}");
        assert_eq!(name(&s, made[0]), word);
        let object = s.project().object(made[0]).unwrap();
        assert_eq!(s.project().layer(object.layer).unwrap().name, "limits");
        let l = line(&s, made[0]);
        assert!(l.closed);
        assert!((plan_area(&l) - area).abs() < 1e-6, "{operation}: {}", plan_area(&l));
    }
    // The overlap's corners: Block B's two inside Block A at RL 120, Block A's two inside Block B at RL 100.
    let made = created(&s.run("string.polygon_boolean", json!({ "strings": [a], "with": [b], "operation": "overlap", "name": "Shared" })).unwrap());
    let l = line(&s, made[0]);
    assert_eq!(name(&s, made[0]), "Shared");
    for (x, rl) in [(50.0, 120.0), (100.0, 100.0)] {
        for y in [0.0, 100.0] {
            assert!(l.vertices.contains(&at(x, y, rl)), "({x}, {y}) at RL {rl}: {:?}", l.vertices);
        }
    }
    assert_eq!([a, b].map(|id| s.project().object(id).unwrap().revision), revisions, "the squares as they were");
}

/// Block polygons joined into one, and a square less a smaller one inside it: the outside first, then its hole, the
/// attributes every string picked has alike carried; what can't be done refused.
#[test]
fn joined_with_holes_and_refusals() {
    let mut s = Session::with_builtins();
    // Each block numbered strip 4, and its own block number, by a line across it.
    let strip = |s: &mut Session, name: &str, x: f64, block: f64| {
        let id = rect(s, name, [x, 0.0, x + 100.0, 100.0], 100.0);
        let across = |attribute: &str, start: f64| json!({ "strings": [id], "from": at(x + 50.0, -5.0, 0.0), "to": at(x + 50.0, 5.0, 0.0), "start": start, "attribute": attribute, "name": name });
        s.run("string.number_across", across("strip", 4.0)).unwrap();
        s.run("string.number_across", across("block", block)).unwrap();
        id
    };
    let blocks = [strip(&mut s, "B1", 0.0, 1.0), strip(&mut s, "B2", 100.0, 2.0), strip(&mut s, "B3", 200.0, 3.0), strip(&mut s, "B9", 500.0, 9.0)];
    let made = created(&s.run("string.polygon_boolean", json!({ "strings": blocks, "name": "Strip 4" })).unwrap());
    assert_eq!(made.iter().map(|&id| name(&s, id)).collect::<Vec<_>>(), ["Strip 4", "Strip 4 2"]);
    assert!((plan_area(&line(&s, made[0])) - 30_000.0).abs() < 1e-6);
    let attributes = &s.project().object(made[0]).unwrap().attributes;
    assert_eq!(attributes.get("strip"), Some(&AttrValue::Number(4.0)));
    assert_eq!(attributes.get("block"), None, "the blocks' names differ");

    let outer = rect(&mut s, "Pad", [0.0, 300.0, 100.0, 400.0], 90.0);
    let shaft = rect(&mut s, "Shaft", [40.0, 340.0, 60.0, 360.0], 90.0);
    let made = created(&s.run("string.polygon_boolean", json!({ "strings": [outer], "with": [shaft], "operation": "difference" })).unwrap());
    assert_eq!(made.iter().map(|&id| name(&s, id)).collect::<Vec<_>>(), ["Difference", "Difference hole 1"]);
    assert!((plan_area(&line(&s, made[1])) - 400.0).abs() < 1e-6);

    let refused = |s: &mut Session, params: serde_json::Value| s.run("string.polygon_boolean", params).unwrap_err().to_string();
    let open = string(&mut s, "Open", &[at(0.0, 0.0, 0.0), at(10.0, 0.0, 0.0), at(10.0, 10.0, 0.0)], false);
    let words = refused(&mut s, json!({ "strings": [outer, open] }));
    assert!(words.contains("1 of 2 strings can't be worked") && words.contains("'Open' isn't closed"), "{words}");
    assert!(refused(&mut s, json!({ "strings": [outer], "operation": "overlap" })).contains("second set"));
    assert!(refused(&mut s, json!({ "strings": [outer], "with": [outer], "operation": "overlap" })).contains("both sets"));
    assert!(refused(&mut s, json!({ "strings": [outer], "with": [blocks[0]], "operation": "overlap" })).contains("don't overlap anywhere"));
    assert!(refused(&mut s, json!({ "strings": [shaft], "with": [outer], "operation": "difference" })).contains("nothing is left"));
}

/// Timings through the commands, made-up data: 150 toes clipped to a limit, and 150 block polygons joined.
/// `cargo test -p zd-commands --release --test strings_polygons timing -- --ignored --nocapture`.
#[test]
#[ignore]
fn timing_polygons_through_the_commands() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 150);
    let limit = rect(&mut s, "Pit limit", [0.0, 0.0, 100.0, 1_500.0], 0.0);
    let start = Instant::now();
    s.run("string.clip", json!({ "strings": ids, "by": limit })).unwrap();
    println!("150 toes clipped: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    let blocks: Vec<ObjectId> = (0..150).map(|k| rect(&mut s, &format!("B{k}"), [(k % 15) as f64 * 100.0, (k / 15) as f64 * 100.0 + 2_000.0, (k % 15) as f64 * 100.0 + 100.0, (k / 15) as f64 * 100.0 + 2_100.0], 100.0)).collect();
    let start = Instant::now();
    let made = created(&s.run("string.polygon_boolean", json!({ "strings": blocks })).unwrap());
    println!("150 blocks joined: {:.1} ms, {} strings", start.elapsed().as_secs_f64() * 1e3, made.len());
}
