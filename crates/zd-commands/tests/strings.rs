//! The string commands through the registry, as the app calls them: the Phase 1 gate's toe string
//! (draw, grade, offset, drape) with exact answers, undone back to an empty project and saved.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn vertices(s: &Session, id: ObjectId) -> Vec<[f64; 3]> {
    match &s.project().object(id).expect("live string").body {
        Body::String(line) => line.vertices.clone(),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

/// Plan coordinates about the origin, to the micrometre, and elevations.
fn local(points: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let r = |v: f64| (v * 1e6).round() / 1e6;
    points.iter().map(|p| [r(p[0] - E), r(p[1] - N), r(p[2])]).collect()
}

/// A base string offset down dip in one go: each is measured from the base, so nothing builds up.
#[test]
fn offset_makes_as_many_strings_as_it_is_asked() {
    let mut s = Session::with_builtins();
    let id = toe(&mut s);
    let record = s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "right", "times": 3 })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 3);
    for (k, &string) in made.iter().enumerate() {
        let d = 10.0 * (k as f64 + 1.0);
        assert_eq!(local(&vertices(&s, string)), [[0.0, -d, 50.0], [100.0 + d, -d, 50.0], [100.0 + d, 100.0, 50.0]]);
        assert_eq!(s.project().object(string).unwrap().name, format!("toe offset {d} m right"));
    }
    let named = s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "right", "times": 2, "name": "S" })).unwrap();
    let names: Vec<String> = created(&named).iter().map(|&o| s.project().object(o).unwrap().name.clone()).collect();
    assert_eq!(names, ["S 1", "S 2"]);
    assert!(s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "right", "times": 0 })).is_err());
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), 1, "one undo takes back every string a run made");
}

/// Three parallel strings and one off to the side. A line drawn across numbers them in the order it crosses,
/// whichever order they came in or the way the line is drawn.
#[test]
fn a_line_across_numbers_strings_in_the_order_it_crosses_them() {
    let mut s = Session::with_builtins();
    let string = |s: &mut Session, name: &str, y: f64, x: f64| created(&s.run("string.create", json!({ "name": name, "layer": "strips", "points": [[E + x, N + y, 50.0], [E + x + 100.0, N + y, 50.0]] })).unwrap())[0];
    let (far, near, middle) = (string(&mut s, "far", 120.0, 0.0), string(&mut s, "near", 0.0, 0.0), string(&mut s, "middle", 60.0, 0.0));
    let aside = string(&mut s, "aside", 60.0, 500.0);
    let strings = [far, near, middle, aside];
    let named = |s: &Session, id: ObjectId| s.project().object(id).unwrap().name.clone();
    let number = |s: &Session, id: ObjectId, key: &str| s.project().object(id).unwrap().attributes.get(key).cloned();

    let record = s.run("string.number_across", json!({ "strings": strings, "from": [E + 50.0, N - 10.0, 0.0], "to": [E + 50.0, N + 130.0, 0.0], "start": 20.0 })).unwrap();
    assert_eq!([near, middle, far].map(|id| named(&s, id)), ["Strip 20", "Strip 21", "Strip 22"]);
    assert_eq!(number(&s, middle, "strip"), Some(AttrValue::Number(21.0)));
    assert_eq!(named(&s, aside), "aside", "the line doesn't reach it");
    assert_eq!(number(&s, aside, "strip"), None);
    assert!(record.messages.iter().any(|m| m == "Numbered 3 strings from 20 to 22"), "{:?}", record.messages);
    assert!(record.messages.iter().any(|m| m == "The line didn't cross aside: left as they were"), "{:?}", record.messages);

    // Drawn the other way, counting down, in a name of Tim's own. Geometry doesn't change.
    let before = vertices(&s, far);
    s.run("string.number_across", json!({ "strings": strings, "from": [E + 50.0, N + 130.0, 0.0], "to": [E + 50.0, N - 10.0, 0.0], "start": 9.0, "step": -2.0, "name": "S#N" })).unwrap();
    assert_eq!([far, middle, near].map(|id| named(&s, id)), ["S9N", "S7N", "S5N"]);
    assert_eq!(vertices(&s, far), before);
    s.undo().unwrap();
    assert_eq!(named(&s, far), "Strip 22", "one undo takes the whole numbering back");

    assert!(s.run("string.number_across", json!({ "strings": strings, "from": [E, N, 0.0], "to": [E, N, 5.0], "start": 1.0 })).is_err(), "a line needs length in plan");
    assert!(s.run("string.number_across", json!({ "strings": strings, "from": [E + 50.0, N - 10.0, 0.0], "to": [E + 50.0, N + 130.0, 0.0], "start": 1.0, "step": 0.0 })).is_err());
    let error = s.run("string.number_across", json!({ "strings": [aside], "from": [E + 50.0, N - 10.0, 0.0], "to": [E + 50.0, N + 130.0, 0.0], "start": 1.0 })).unwrap_err();
    assert!(error.to_string().contains("doesn't cross any"), "{error}");
}

/// A toe string: east 100 m, then north 100 m, all at RL 50.
fn toe(s: &mut Session) -> ObjectId {
    let points = [[E, N, 50.0], [E + 100.0, N, 50.0], [E + 100.0, N + 100.0, 50.0]];
    created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0]
}

#[test]
fn grade_then_offset_keeps_the_grade_on_the_new_string() {
    let mut s = Session::with_builtins();
    let id = toe(&mut s);
    let graded = s.run("string.set_grade", json!({ "string": id, "from": 0, "grade": 10.0 })).unwrap();
    assert_eq!(graded.messages, ["vertices 1 to 3 at 10 % from RL 50.000: RL 70.000 at vertex 3"]);
    assert_eq!(local(&vertices(&s, id)), [[0.0, 0.0, 50.0], [100.0, 0.0, 60.0], [100.0, 100.0, 70.0]]);

    // A 10 m berm on the outside of the corner: mitred, its own string on the toe's layer.
    let record = s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "right" })).unwrap();
    let crest = created(&record)[0];
    let object = s.project().object(crest).unwrap();
    assert_eq!((object.name.as_str(), s.project().layer(object.layer).unwrap().name.as_str()), ("toe offset 10 m right", "design"));
    // (0, -10) beside the start (RL 50); (110, -10) nearest the corner (RL 60); (110, 100) beside the end.
    assert_eq!(local(&vertices(&s, crest)), [[0.0, -10.0, 50.0], [110.0, -10.0, 60.0], [110.0, 100.0, 70.0]]);
    assert_eq!(s.project().object(id).unwrap().revision, 2, "the toe itself is not changed by the offset");

    // Round corners and a fixed RL.
    let round = created(&s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "right", "corners": "round", "rl": 45.0 })).unwrap())[0];
    let pts = vertices(&s, round);
    assert!(pts.len() > 10 && pts.iter().all(|p| p[2] == 45.0));
}

#[test]
fn an_offset_that_cannot_be_made_is_refused_and_changes_nothing() {
    let mut s = Session::with_builtins();
    // Down, east 15 m, up: a U whose inside (the left) is narrower than twice 10 m.
    let u = [[E, N + 100.0, 0.0], [E, N, 0.0], [E + 15.0, N, 0.0], [E + 15.0, N + 100.0, 0.0]];
    let id = created(&s.run("string.create", json!({ "name": "u", "layer": "design", "points": u })).unwrap())[0];
    let error = s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "left" })).unwrap_err();
    assert!(error.to_string().contains("nothing is left"), "{error}");
    assert_eq!(s.project().objects().count(), 1);
    assert!(s.run("string.offset", json!({ "string": id, "distance": 10.0, "side": "up" })).is_err(), "side must be left or right");
}

#[test]
fn a_string_drapes_onto_a_grid_and_follows_it() {
    let mut s = Session::with_builtins();
    // A plane z = 100 + 0.1x on a 10 m grid over 0..100 m, as an XYZ file.
    let dir = std::env::temp_dir().join(format!("zd-strings-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut xyz = String::new();
    for row in 0..=10 {
        for col in 0..=10 {
            xyz += &format!("{} {} {}\n", E + col as f64 * 10.0, N + row as f64 * 10.0, 100.0 + col as f64);
        }
    }
    std::fs::write(dir.join("topo.xyz"), xyz).unwrap();
    let surface = created(&s.run("surface.import", json!({ "path": dir.join("topo.xyz").to_string_lossy(), "layer": "surfaces" })).unwrap())[0];
    let line = [[E + 5.0, N + 5.0, 0.0], [E + 95.0, N + 5.0, 0.0], [E + 150.0, N + 5.0, 3.0]];
    let id = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": line })).unwrap())[0];

    let record = s.run("string.drape", json!({ "string": id, "surface": surface })).unwrap();
    assert!(record.messages[0].contains("1 vertices have no surface under them"), "{:?}", record.messages);
    let pts = vertices(&s, id);
    // Along y = 5 the string crosses x = 10, 20, ..., 90 and each cell's diagonal; on the plane
    // throughout, and the vertex past the grid keeps RL 3.
    assert!(pts.len() > 11);
    assert!(pts.iter().filter(|p| p[0] - E <= 100.0).all(|p| (p[2] - (100.0 + (p[0] - E) / 10.0)).abs() < 1e-9));
    assert_eq!(pts.last().unwrap()[2], 3.0);

    let plain = created(&s.run("string.create", json!({ "name": "two", "layer": "design", "points": line[..2] })).unwrap())[0];
    s.run("string.drape", json!({ "string": plain, "surface": surface, "crossings": false })).unwrap();
    assert_eq!(local(&vertices(&s, plain)), [[5.0, 5.0, 100.5], [95.0, 5.0, 109.5]]);
    assert!(s.run("string.drape", json!({ "string": plain, "surface": id })).is_err(), "a string is not a surface");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn elevation_reverse_and_close_then_undo_to_empty_and_save() {
    let mut s = Session::with_builtins();
    let id = toe(&mut s);
    s.run("string.set_elevation", json!({ "string": id, "rl": 42.25, "from": 1 })).unwrap();
    assert_eq!(vertices(&s, id).iter().map(|p| p[2]).collect::<Vec<_>>(), [50.0, 42.25, 42.25]);
    s.run("string.reverse", json!({ "string": id })).unwrap();
    assert_eq!(local(&vertices(&s, id))[0], [100.0, 100.0, 42.25]);
    s.run("string.set_closed", json!({ "string": id, "closed": true })).unwrap();
    assert!(s.run("string.set_closed", json!({ "string": id, "closed": true })).is_err(), "already closed");
    match &s.project().object(id).unwrap().body {
        Body::String(line) => assert!(line.closed),
        _ => unreachable!(),
    }
    let offset = created(&s.run("string.offset", json!({ "string": id, "distance": 2.5, "side": "left" })).unwrap())[0];

    // Save and reopen: every coordinate bit for bit.
    let dir = std::env::temp_dir().join(format!("zd-strings-save-{}-{}", std::process::id(), zd_core::RunId::new()));
    s.save(dir.join("pit.zdproj")).unwrap();
    let reopened = Session::open(dir.join("pit.zdproj")).unwrap();
    for object in [id, offset] {
        let (a, b) = (vertices(&s, object), vertices(&reopened, object));
        assert!(a.iter().flatten().zip(b.iter().flatten()).all(|(x, y)| x.to_bits() == y.to_bits()));
    }
    std::fs::remove_dir_all(&dir).unwrap();

    // Undo every step, back to the empty project.
    while s.undo().is_some() {}
    assert_eq!(s.project().objects().count(), 0);
    assert!(s.project().layers().is_empty());
}

#[test]
fn split_then_join_gives_back_the_string_and_undo_restores_both() {
    let mut s = Session::with_builtins();
    let id = toe(&mut s);
    let split = s.run("string.split", json!({ "string": id, "at": 1 })).unwrap();
    assert_eq!(split.messages, ["split at vertex 2: 2 and 2 vertices"]);
    let second = created(&split)[0];
    assert_eq!(s.project().object(second).unwrap().name, "toe part 2");
    assert_eq!(local(&vertices(&s, second)), [[100.0, 0.0, 50.0], [100.0, 100.0, 50.0]]);

    // Joining the part back on shares the meeting vertex, keeps the toe and deletes the part.
    let joined = s.run("string.join", json!({ "string": id, "other": second })).unwrap();
    assert_eq!(joined.messages, ["joined 'toe part 2' on: 3 vertices"]);
    assert_eq!(local(&vertices(&s, id)), [[0.0, 0.0, 50.0], [100.0, 0.0, 50.0], [100.0, 100.0, 50.0]]);
    assert!(s.project().object(second).is_none());
    s.undo();
    assert!(s.project().object(second).is_some() && vertices(&s, id).len() == 2, "undo brings the part back");
    assert!(s.run("string.join", json!({ "string": id, "other": id })).is_err());
    assert!(s.run("string.split", json!({ "string": id, "at": 0 })).is_err(), "not at an end");
}

#[test]
fn a_gap_is_bridged_and_reported_when_joining() {
    let mut s = Session::with_builtins();
    let id = toe(&mut s);
    let far = [[E + 103.0, N + 104.0, 50.0], [E + 103.0, N + 200.0, 50.0]];
    let other = created(&s.run("string.create", json!({ "name": "spur", "layer": "design", "points": far })).unwrap())[0];
    let record = s.run("string.join", json!({ "string": id, "other": other })).unwrap();
    assert_eq!(record.messages, ["joined 'spur' on, bridging a 5.000 m gap: 5 vertices"]);
}

#[test]
fn smoothing_then_filtering_through_the_registry() {
    let mut s = Session::with_builtins();
    let id = toe(&mut s);
    let smoothed = s.run("string.smooth", json!({ "string": id, "spacing": 10.0 })).unwrap();
    assert_eq!(smoothed.messages, ["smoothed at 10 m: 21 vertices, was 3"]);
    let pts = vertices(&s, id);
    assert_eq!((local(&pts)[0], local(&pts)[10], local(&pts)[20]), ([0.0, 0.0, 50.0], [100.0, 0.0, 50.0], [100.0, 100.0, 50.0]));
    // The curve passes through the corner vertex, so it swings just outside it on both legs.
    assert!(local(&pts)[..10].iter().any(|p| p[1] < 0.0) && local(&pts)[11..].iter().any(|p| p[0] > 100.0));

    // Filter it back at 20 m: the curve is within 20 m of the chord ends, so few vertices remain.
    let filtered = s.run("string.filter", json!({ "string": id, "tolerance": 20.0 })).unwrap();
    assert!(filtered.messages[0].starts_with("filtered at 20 m: "), "{:?}", filtered.messages);
    assert!(vertices(&s, id).len() < 21);
    assert!(s.run("string.filter", json!({ "string": id, "tolerance": -1.0 })).is_err());
    while s.undo().is_some() {}
    assert_eq!(s.project().objects().count(), 0);
}
