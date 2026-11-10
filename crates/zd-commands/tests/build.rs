//! The manual builder's one step, `face.build_step` (decision 0048), through the registry as the app calls it: known answers
//! on level benches, and builds a step at a time against the same design run whole.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, Object, ObjectId, Polyline, TriMesh};
use zd_geom::{COINCIDENT, find_crossings};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn named(s: &Session, name: &str) -> Arc<Object> {
    s.project().objects().find(|o| o.name == name).unwrap_or_else(|| panic!("{name} wasn't made")).clone()
}

fn line_of(s: &Session, name: &str) -> Polyline {
    match &named(s, name).body {
        Body::String(line) => line.clone(),
        other => panic!("{name} is a {:?}", other.kind()),
    }
}

fn sheet_of(s: &Session, name: &str) -> TriMesh {
    match &named(s, name).body {
        Body::Surface(sheet) => sheet.mesh().clone(),
        other => panic!("{name} is a {:?}", other.kind()),
    }
}

fn string(s: &mut Session, name: &str, layer: &str, points: &[[f64; 2]], rl: f64) -> ObjectId {
    let points: Vec<[f64; 3]> = points.iter().map(|p| [E + p[0], N + p[1], rl]).collect();
    created(&s.run("string.create", json!({ "name": name, "layer": layer, "points": points })).unwrap())[0]
}

fn rl(rl: f64) -> Value {
    json!({ "kind": "rl", "rl": rl })
}

fn wall(to: f64, batter: f64) -> Value {
    json!({ "op": "project", "to": rl(to), "batter": batter })
}

/// Every vertex of each line on the other, to 1 µm in plan and height, and the same length.
fn same_line(a: &Polyline, b: &Polyline, what: &str) {
    let on = |p: &[f64; 3], line: &Polyline| {
        line.vertices.windows(2).any(|w| {
            let (d, q) = ([w[1][0] - w[0][0], w[1][1] - w[0][1]], [p[0] - w[0][0], p[1] - w[0][1]]);
            let length = d[0] * d[0] + d[1] * d[1];
            let t = if length > 0.0 { ((q[0] * d[0] + q[1] * d[1]) / length).clamp(0.0, 1.0) } else { 0.0 };
            (q[0] - t * d[0]).hypot(q[1] - t * d[1]) <= 1e-6 && (w[0][2] + t * (w[1][2] - w[0][2]) - p[2]).abs() <= 1e-6
        })
    };
    for (one, other) in [(a, b), (b, a)] {
        for p in &one.vertices {
            assert!(on(p, other), "{what}: {p:?} isn't on {:?}", other.vertices);
        }
    }
    assert!((a.plan_length() - b.plan_length()).abs() < 1e-6, "{what}: {} against {}", a.plan_length(), b.plan_length());
}

fn text(object: &Object, key: &str) -> String {
    match object.attributes.get(key) {
        Some(AttrValue::Text(text)) => text.clone(),
        other => panic!("{}: {key} is {other:?}", object.name),
    }
}

/// From a toe drawn by hand at RL 100: a wall to RL 120 at 45°, a 10 m berm, and a wall to RL 140 at 45°. A 45° wall 20 m
/// high stands 20 m out, so the crests are 20 m and 50 m out from the toe and the berm's line 30 m. The first wall's sheet is
/// 200 m by 20√2 m; the second's has the 10 m berm below it, as a berm's sheet comes with the wall above. Each step is one
/// undo, and the lines they started from stay as they were.
#[test]
fn walls_and_a_berm_from_a_toe_stand_where_their_batters_put_them() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe", "toes", &[[50.0, 50.0], [250.0, 50.0]], 100.0);
    let one = s.run("face.build_step", json!({ "from": toe, "step": wall(120.0, 45.0), "lies_on": rl(100.0), "side": "left" })).unwrap();
    assert!(one.messages[0].starts_with("Toe step 1: a wall to RL 120.0 at 45° from Toe, "), "{:?}", one.messages);
    let crest = named(&s, "Toe crest 1");
    assert!(line_of(&s, "Toe crest 1").vertices.iter().all(|p| (p[1] - N - 70.0).abs() < 1e-9 && p[2] == 120.0), "{:?}", crest.body);
    assert_eq!(s.project().layer(crest.layer).unwrap().name, "builder");
    assert_eq!(crest.attributes.get("build"), Some(&AttrValue::Text("Toe".into())));
    assert_eq!(crest.attributes.get("build_step"), Some(&AttrValue::Number(1.0)));
    assert_eq!(text(&crest, "build_on"), r#"{"kind":"rl","rl":120.0}"#);
    assert!(text(&crest, "build_record").starts_with(&format!(r#"{{"from":"{toe}","from_on":{{"kind":"rl","rl":100.0}},"side":"left","step":{{"op":"project""#)), "{}", text(&crest, "build_record"));
    assert!(text(&crest, "build_record").ends_with(r#""from_revision":1,"made_revision":1}"#), "the revisions of the line it was built from and of its own: {}", text(&crest, "build_record"));
    let first = sheet_of(&s, "Toe wall 1");
    assert!((first.area() - 200.0 * 20.0 * 2f64.sqrt()).abs() < 1e-6, "{}", first.area());
    assert_eq!(named(&s, "Toe wall 1").attributes.get("build_id"), crest.attributes.get("build_id"), "one step's pieces share its id");

    // The next steps need no side or what the line lies on: the crest says.
    let two = s.run("face.build_step", json!({ "from": crest.id, "step": { "op": "berm", "width": 10.0 } })).unwrap();
    assert_eq!(two.messages[0], "Toe step 2: a berm 10 m wide on RL 120.0, from Toe crest 1");
    assert_eq!(created(&two).len(), 1, "the berm's line, its sheet waiting for the wall above");
    assert!(line_of(&s, "Toe toe 2").vertices.iter().all(|p| (p[1] - N - 80.0).abs() < 1e-9 && p[2] == 120.0));
    let three = s.run("face.build_step", json!({ "from": named(&s, "Toe toe 2").id, "step": wall(140.0, 45.0) })).unwrap();
    assert!(three.messages[0].starts_with("Toe step 3: a wall to RL 140.0 at 45° from Toe toe 2, with the berm from Toe crest 1, "), "{:?}", three.messages);
    assert!(line_of(&s, "Toe crest 3").vertices.iter().all(|p| (p[1] - N - 100.0).abs() < 1e-9 && p[2] == 140.0));
    let second = sheet_of(&s, "Toe wall 3");
    assert!((second.area() - 200.0 * (10.0 + 20.0 * 2f64.sqrt())).abs() < 1e-6, "{}", second.area());
    assert_eq!(find_crossings(&second, COINCIDENT).pairs, 0);
    for id in [toe, crest.id, named(&s, "Toe toe 2").id] {
        assert_eq!(s.project().object(id).unwrap().revision, 1, "a step leaves the line it started from as it was");
    }
    s.undo().unwrap();
    assert!(!s.project().objects().any(|o| o.name == "Toe crest 3" || o.name == "Toe wall 3"), "one undo a step");
    assert!(s.project().objects().any(|o| o.name == "Toe toe 2"));
}

/// What's refused, and why.
#[test]
fn a_step_says_what_it_needs() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe", "toes", &[[50.0, 50.0], [250.0, 50.0]], 100.0);
    let refused = |s: &mut Session, params: Value| s.run("face.build_step", params).unwrap_err().to_string();
    assert!(refused(&mut s, json!({ "from": toe, "step": wall(120.0, 45.0), "side": "left" })).contains("say what Toe lies on"));
    assert!(refused(&mut s, json!({ "from": toe, "step": wall(120.0, 45.0), "lies_on": rl(100.0) })).contains("say which side of Toe"));
    let flat = refused(&mut s, json!({ "from": toe, "step": wall(90.0, 45.0), "lies_on": rl(100.0), "side": "left" }));
    assert!(flat.contains("the wall has no height: RL 90.0 is on or below Toe all the way along"), "{flat}");
    s.run("face.build_step", json!({ "from": toe, "step": wall(120.0, 45.0), "lies_on": rl(100.0), "side": "left" })).unwrap();
    let crest = named(&s, "Toe crest 1").id;
    let none = refused(&mut s, json!({ "from": crest, "step": { "op": "berm", "width": 10.0, "turns": ["A"] } }));
    assert!(none.contains("no road A came out on Toe crest 1: the roads on it are none"), "{none}");
}

/// A design up two walls with a 5 m berm between, from a toe at RL 100, ramp A 10 m wide at 10 % placed by a string across
/// the toe, run whole, and the same built a step at a time. `berm` is how A gets past the berm; with `turn` A turns back on
/// a box on it. Gives the session, and the names of the design's pieces.
fn whole_and_by_steps(berm: &str, turn: bool) -> Session {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "toe", "strings", &[[10.0, 30.0], [295.0, 30.0]], 0.0);
    let across = string(&mut s, "ramp string", "ramps", &[[60.0, 20.0], [60.0, 40.0]], 0.0);
    let turns: Vec<&str> = if turn { vec!["A"] } else { Vec::new() };
    let content = json!({
        "layer": "whole",
        "lines": [{ "string": toe, "profile": "Whole", "side": "left", "run_on": [0.0, 0.0] }],
        "profiles": [{
            "name": "Whole", "direction": "up", "register": rl(100.0),
            "steps": [
                { "step": "project", "to": rl(110.0), "batter": 45.0, "ramps": ["A"] },
                { "step": "berm", "width": 5.0 },
                { "step": "project", "to": rl(120.0), "batter": 45.0, "ramps": ["A"], "turns": turns },
            ],
            "ramps": [{ "name": "A", "string": across, "fixed": "start", "heading": "along", "width": 10.0, "grade": 10.0, "berm": berm }],
        }],
        "rules": [],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "Whole", "content": content })).unwrap())[0];
    s.run("face.project", json!({ "design": design })).unwrap();

    let ramp = json!({ "name": "A", "string": across, "fixed": "start", "heading": "along", "width": 10.0, "grade": 10.0, "berm": berm });
    let one = s.run("face.build_step", json!({ "from": toe, "step": { "op": "project", "to": rl(110.0), "batter": 45.0, "ramps": [ramp] }, "lies_on": rl(100.0), "side": "left", "name": "Built" })).unwrap();
    assert!(one.messages.iter().any(|m| m.starts_with("road A: ") && m.contains(" to 110.0")), "{:?}", one.messages);
    let two = s.run("face.build_step", json!({ "from": named(&s, "Built crest 1").id, "step": { "op": "berm", "width": 5.0, "turns": turns } })).unwrap();
    let says = if turn { "road A: turns back on the berm, for the wall above" } else if berm == "through" { "road A: climbs on up through the berm" } else { "road A: crosses the berm to climb on from its line" };
    assert!(two.messages.iter().any(|m| m == says), "{:?}", two.messages);
    let three = s.run("face.build_step", json!({ "from": named(&s, "Built toe 2").id, "step": wall(120.0, 45.0) })).unwrap();
    assert!(three.messages.iter().any(|m| m.starts_with("road A: ") && m.contains(" to 120.0")), "{:?}", three.messages);
    assert!(!three.messages.iter().any(|m| m.contains("edited") || m.contains("stopped")), "{:?}", three.messages);
    s
}

/// Built a step at a time, the design comes out as it does run whole: the berm's line, the top crest and every vertex of
/// the road's edges to 1 µm, and the two walls' sheets together the whole design's sheet, to 1 cm². Going after the berm,
/// up through it, and turning back on a box on it (decisions 0035, 0038, 0041).
#[test]
fn a_step_at_a_time_comes_out_as_the_design_run_whole() {
    for (berm, turn) in [("after", false), ("through", false), ("after", true)] {
        let s = whole_and_by_steps(berm, turn);
        let what = format!("{berm}{}", if turn { ", turning" } else { "" });
        same_line(&line_of(&s, "toe Whole crest 1"), &line_of(&s, "Built crest 1"), &format!("{what}: the first crest"));
        same_line(&line_of(&s, "toe Whole crest 2"), &line_of(&s, "Built crest 3"), &format!("{what}: the top crest"));
        if !turn {
            same_line(&line_of(&s, "toe Whole toe 2"), &line_of(&s, "Built toe 2"), &format!("{what}: the berm's line"));
        }
        for edge in ["pit", "wall"] {
            let mut joined = line_of(&s, &format!("Built road A {edge} edge 1"));
            joined.vertices.extend_from_slice(&line_of(&s, &format!("Built road A {edge} edge 3")).vertices);
            same_line(&line_of(&s, &format!("toe Whole road A {edge} edge")), &joined, &format!("{what}: the road's {edge} edge"));
        }
        let whole = sheet_of(&s, "toe Whole face").area();
        let built = sheet_of(&s, "Built wall 1").area() + sheet_of(&s, "Built wall 3").area();
        assert!((whole - built).abs() < 1e-4, "{what}: {whole} m² whole against {built} m² a step at a time");
        // The top crest carries the road on, as the whole design's does.
        let top = |name: &str| -> Vec<f64> { text(&named(&s, name), "road_top A pit").split(' ').map(|c| c.parse().unwrap()).collect() };
        let (built, whole) = (top("Built crest 3"), top("toe Whole crest 2"));
        assert!(built.iter().zip(&whole).all(|(a, b)| (a - b).abs() < 1e-6), "{what}: {built:?} against {whole:?}");
    }
}

/// The berm's line edited by hand before the wall above is built: the wall stands on your line, and says so. Its far end
/// dragged 3 m further out widens the berm there to 8 m, the crest above follows it out, and road A carries on.
#[test]
fn a_berm_line_you_edit_is_where_the_wall_above_stands() {
    let mut s = whole_and_by_steps("after", false);
    let toe_2 = named(&s, "Built toe 2");
    let Body::String(line) = &toe_2.body else { panic!("a line") };
    let last = line.vertices.len() - 1;
    let [x, y, z] = line.vertices[last];
    s.run("string.move_vertex", json!({ "string": toe_2.id, "index": last, "to": [x, y + 3.0, z] })).unwrap();
    // Build the wall again from the line as it now is.
    let three = named(&s, "Built crest 3").attributes.get("build_id").cloned();
    let mine: Vec<ObjectId> = s.project().objects().filter(|o| o.attributes.get("build_id") == three.as_ref()).map(|o| o.id).collect();
    s.run("object.delete_many", json!({ "objects": mine })).unwrap();
    let run = s.run("face.build_step", json!({ "from": toe_2.id, "step": wall(120.0, 45.0) })).unwrap();
    assert!(run.messages.iter().any(|m| m == "Built toe 2 isn't where the berm from Built crest 1 puts it now, as it's been edited or the crest has, so the wall stands on it as it is"), "{:?}", run.messages);
    assert!(run.messages.iter().any(|m| m.starts_with("Built toe 2 as you edited it: the berm below it is 5.0 to 8.0 m wide")), "{:?}", run.messages);
    assert!(run.messages.iter().any(|m| m.starts_with("road A: ") && m.contains(" to 120.0")), "{:?}", run.messages);
    // A 45° wall 10 m high stands 10 m out from your line, square to its last segment, which now slants out to the vertex.
    let crest = line_of(&s, "Built crest 3");
    let end = crest.vertices.last().unwrap();
    assert!(((end[0] - x).hypot(end[1] - y - 3.0) - 10.0).abs() < 1e-6 && end[2] == 120.0, "the crest follows your line out: {end:?}");
}

/// A berm's line whose crest has gone since: the wall stands on the line as it is, and says no road carries on over the berm.
#[test]
fn a_wall_from_a_berm_whose_crest_has_gone_stands_on_its_line() {
    let mut s = whole_and_by_steps("after", false);
    let three = named(&s, "Built crest 3").attributes.get("build_id").cloned();
    let mine: Vec<ObjectId> = s.project().objects().filter(|o| o.attributes.get("build_id") == three.as_ref()).map(|o| o.id).collect();
    s.run("object.delete_many", json!({ "objects": mine })).unwrap();
    s.run("object.delete", json!({ "object": named(&s, "Built crest 1").id })).unwrap();
    let run = s.run("face.build_step", json!({ "from": named(&s, "Built toe 2").id, "step": wall(120.0, 45.0) })).unwrap();
    assert_eq!(run.messages[1], "the crest Built toe 2 was built from has gone, so the wall stands on it as it is, and no road carries on over the berm", "{:?}", run.messages);
    assert!(!run.messages.iter().any(|m| m.starts_with("road ")), "{:?}", run.messages);
    // A 45° wall 10 m high from the berm's line, 10 m out from it.
    let (toe, crest) = (line_of(&s, "Built toe 2"), line_of(&s, "Built crest 3"));
    assert!((crest.vertices[0][1] - toe.vertices[0][1] - 10.0).abs() < 1e-6, "{:?} {:?}", toe.vertices[0], crest.vertices[0]);
}

/// Ending a road: on the crest, where the berm doesn't make way for it, and not once it has crossed the berm.
#[test]
fn a_road_ends_on_the_crest_it_came_out_on() {
    let mut s = whole_and_by_steps("after", false);
    let crest = named(&s, "Built crest 1").id;
    let run = s.run("face.build_step", json!({ "from": crest, "step": { "op": "berm", "width": 5.0, "stop": ["A"] }, "name": "Ended" })).unwrap();
    assert!(run.messages.iter().any(|m| m == "road A: ends on Built crest 1"), "{:?}", run.messages);
    let top = s.run("face.build_step", json!({ "from": named(&s, "Ended toe 2").id, "step": wall(120.0, 45.0) })).unwrap();
    assert!(!top.messages.iter().any(|m| m.starts_with("road ")), "{:?}", top.messages);
    assert!(named(&s, "Ended crest 3").attributes.keys().all(|key| !key.starts_with("road_top")));
    assert!(!s.project().objects().any(|o| o.name.starts_with("Ended road")));
    // Once it has crossed the berm, it climbs the wall above.
    let crossed = s.run("face.build_step", json!({ "from": named(&s, "Built toe 2").id, "step": { "op": "project", "to": rl(120.0), "batter": 45.0, "stop": ["A"] } })).unwrap_err();
    assert!(crossed.to_string().contains("road A crossed the berm to Built toe 2: to end it, end it on Built crest 1, in the berm's step"), "{crossed}");
}

/// Tim's step template (decision 0047's test pit) built a step at a time, as the builder's gate asks: from a bent toe at RL
/// 100, walls to RL 115 and 130 at 70° and on to 175 at 65° with 10 m berms, roads A and B 30 m wide at 10 % placed on the
/// wall to RL 130, both turning back on boxes on the berm at RL 155, joined as one pad. It comes out as the same design run
/// whole: every crest and the top of each road to 1 µm, and both roads to RL 175 with nothing crossing. Each step's time is
/// printed.
#[test]
fn tims_template_a_step_at_a_time_is_the_design_run_whole() {
    let (s, _, runs) = tims_template();
    assert!(runs[7].messages.iter().any(|m| m == "road A: turns back on the berm, for the wall above"), "{:?}", runs[7].messages);
    assert!(runs[7].messages.iter().any(|m| m == "the switchbacks of A and B are one pad"), "the berm's line shows the pad: {:?}", runs[7].messages);
    assert!(runs[8].messages.iter().any(|m| m.starts_with("the switchbacks of A and B are one pad")), "{:?}", runs[8].messages);
    for (k, crest) in (1..=11).step_by(2).enumerate() {
        same_line(&line_of(&s, &format!("Toe Steps crest {}", k + 1)), &line_of(&s, &format!("Built crest {crest}")), &format!("crest {crest}"));
    }
    // The berm at RL 155 as the builder shows it, one pad, is the line the wall above stands on.
    same_line(&line_of(&s, "Toe Steps toe 5"), &line_of(&s, "Built toe 8"), "the joined pad");
    for ramp in ["A", "B"] {
        assert!(runs[10].messages.iter().any(|m| m.starts_with(&format!("road {ramp}: ")) && m.contains(" to 175.0")), "{ramp}: {:?}", runs[10].messages);
        let top = |name: &str| -> Vec<f64> { text(&named(&s, name), &format!("road_top {ramp} pit")).split(' ').map(|c| c.parse().unwrap()).collect() };
        let (built, whole) = (top("Built crest 11"), top("Toe Steps crest 6"));
        assert!(built.iter().zip(&whole).all(|(a, b)| (a - b).abs() < 1e-6), "{ramp}: {built:?} against {whole:?}");
    }
    for k in (1..=11).step_by(2) {
        assert_eq!(find_crossings(&sheet_of(&s, &format!("Built wall {k}")), COINCIDENT).pairs, 0, "wall {k}");
    }
}

/// Tim's step template, run whole as the design "Steps" and built a step at a time as "Built". Gives the session, the steps
/// and each step's run.
fn tims_template() -> (Session, Vec<Value>, Vec<RunRecord>) {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe", "toes", &[[0.0, 0.0], [600.0, 30.0], [1100.0, 10.0], [1500.0, 70.0]], 0.0);
    let a = string(&mut s, "Ramp A", "ramps", &[[150.249688, 17.985104], [148.002495, 62.928959]], 115.0);
    let b = string(&mut s, "Ramp B", "ramps", &[[1099.896493, 20.561043], [1093.221172, 65.063179]], 115.0);
    let road = |name: &str, string: ObjectId, heading: &str| json!({ "name": name, "string": string, "fixed": "start", "heading": heading, "width": 30.0, "grade": 10.0 });
    let project = |to: f64, batter: f64, ramps: &[&str], turns: &[&str]| json!({ "step": "project", "to": rl(to), "batter": batter, "ramps": ramps, "turns": turns });
    let berm = |join: bool| if join { json!({ "step": "berm", "width": 10.0, "join": true }) } else { json!({ "step": "berm", "width": 10.0 }) };
    let ab = ["A", "B"];
    let content = json!({
        "layer": "whole",
        "lines": [{ "string": toe, "profile": "Steps", "side": "left", "run_on": [0.0, 0.0] }],
        "profiles": [{
            "name": "Steps", "direction": "up", "register": rl(100.0),
            "steps": [project(115.0, 70.0, &[], &[]), berm(false), project(130.0, 70.0, &ab, &[]), berm(false), project(145.0, 65.0, &ab, &[]), berm(false), project(155.0, 65.0, &ab, &[]), berm(true), project(165.0, 65.0, &ab, &ab), berm(false), project(175.0, 65.0, &ab, &[])],
            "ramps": [road("A", a, "along"), road("B", b, "back")],
        }],
        "rules": [],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "Whole", "content": content })).unwrap())[0];
    let started = Instant::now();
    let whole = s.run("face.project", json!({ "design": design })).unwrap();
    println!("Tim's template run whole: {} ms", started.elapsed().as_millis());
    assert!(whole.messages.iter().any(|m| m.contains("joins the switchbacks of A and B into one pad")), "{:?}", whole.messages);

    let steps = [
        json!({ "op": "project", "to": rl(115.0), "batter": 70.0 }),
        json!({ "op": "berm", "width": 10.0 }),
        json!({ "op": "project", "to": rl(130.0), "batter": 70.0, "ramps": [road("A", a, "along"), road("B", b, "back")] }),
        json!({ "op": "berm", "width": 10.0 }),
        wall(145.0, 65.0),
        json!({ "op": "berm", "width": 10.0 }),
        wall(155.0, 65.0),
        json!({ "op": "berm", "width": 10.0, "turns": ["A", "B"], "join": true }),
        wall(165.0, 65.0),
        json!({ "op": "berm", "width": 10.0 }),
        wall(175.0, 65.0),
    ];
    let mut from = toe;
    let mut runs = Vec::new();
    for (k, step) in steps.iter().enumerate() {
        let mut params = json!({ "from": from, "step": step });
        if k == 0 {
            params["lies_on"] = rl(100.0);
            params["side"] = json!("left");
            params["name"] = json!("Built");
        }
        let started = Instant::now();
        let run = s.run("face.build_step", params).unwrap();
        println!("step {}: {} ms", k + 1, started.elapsed().as_millis());
        assert!(!run.messages.iter().any(|m| m.contains("stopped") || m.contains("triangles cross") || m.contains("edited")), "step {}: {:?}", k + 1, run.messages);
        from = named(&s, &format!("Built {} {}", if k % 2 == 0 { "crest" } else { "toe" }, k + 1)).id;
        runs.push(run);
    }
    (s, steps.to_vec(), runs)
}

/// The line of step `k` (from 1) of a build: a crest after a wall, a berm's line after a berm.
fn step_line(build: &str, k: usize) -> String {
    format!("{build} {} {k}", if k % 2 == 1 { "crest" } else { "toe" })
}

/// Takes out steps `from..=to` of "Built", every piece of each, in one run.
fn take_out(s: &mut Session, from: usize, to: usize) {
    let ids: Vec<Option<AttrValue>> = (from..=to).map(|k| named(s, &step_line("Built", k)).attributes.get("build_id").cloned()).collect();
    let pieces: Vec<ObjectId> = s.project().objects().filter(|o| ids.contains(&o.attributes.get("build_id").cloned())).map(|o| o.id).collect();
    s.run("object.delete_many", json!({ "objects": pieces })).unwrap();
}

/// Moves one vertex of a line in plan, as a drag does.
fn drag(s: &mut Session, name: &str, index: usize, by: [f64; 2]) {
    let object = named(s, name);
    let Body::String(line) = &object.body else { panic!("{name} is a line") };
    let [x, y, z] = line.vertices[index];
    s.run("string.move_vertex", json!({ "string": object.id, "index": index, "to": [x + by[0], y + by[1], z] })).unwrap();
}

/// The record a step keeps on its line, as JSON.
fn record_of(s: &Session, name: &str) -> Value {
    serde_json::from_str(&text(&named(s, name), "build_record")).unwrap()
}

/// Rebuild from an edited crest (decision 0048): from a toe at RL 100, a wall to RL 120 at 45°, a 10 m berm and a wall to
/// RL 140 at 45°. With every vertex of the first crest dragged 5 m further out, the berm's line stands 15 m out from it and the
/// top crest 35 m, so they move 5 m out too, the top wall's sheet keeping its size. The steps keep their objects, each one
/// revision on, and their records name the revisions they were built from and made, so nothing shows as edited or stale.
/// One undo takes it back. From the toe, the edited crest is built again too, and the rebuild says so.
#[test]
fn rebuild_from_an_edited_crest_moves_everything_above() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe", "toes", &[[50.0, 50.0], [250.0, 50.0]], 100.0);
    s.run("face.build_step", json!({ "from": toe, "step": wall(120.0, 45.0), "lies_on": rl(100.0), "side": "left" })).unwrap();
    s.run("face.build_step", json!({ "from": named(&s, "Toe crest 1").id, "step": { "op": "berm", "width": 10.0 } })).unwrap();
    s.run("face.build_step", json!({ "from": named(&s, "Toe toe 2").id, "step": wall(140.0, 45.0) })).unwrap();
    let ids: Vec<ObjectId> = ["Toe toe 2", "Toe crest 3", "Toe wall 3"].iter().map(|name| named(&s, name).id).collect();
    let refused = s.run("face.rebuild", json!({ "from": named(&s, "Toe crest 3").id })).unwrap_err().to_string();
    assert!(refused.contains("no step has been built from Toe crest 3: build one from it first"), "{refused}");

    let dragged = line_of(&s, "Toe crest 1").vertices.len();
    for k in 0..dragged {
        drag(&mut s, "Toe crest 1", k, [0.0, 5.0]);
    }
    let started = Instant::now();
    let run = s.run("face.rebuild", json!({ "from": named(&s, "Toe crest 1").id })).unwrap();
    println!("rebuild of 2 steps: {} ms", started.elapsed().as_millis());
    assert_eq!(run.messages[0], "2 steps built again from Toe crest 1 as it is now");
    assert!(run.outputs.iter().all(|o| o.change == ChangeKind::Revised && o.revision == 2), "{:?}", run.outputs);
    let at = |s: &Session, name: &str, y: f64, z: f64| line_of(s, name).vertices.iter().all(|p| (p[1] - N - y).abs() < 1e-9 && p[2] == z);
    assert!(at(&s, "Toe toe 2", 85.0, 120.0), "{:?}", line_of(&s, "Toe toe 2").vertices);
    assert!(at(&s, "Toe crest 3", 105.0, 140.0), "{:?}", line_of(&s, "Toe crest 3").vertices);
    let area = sheet_of(&s, "Toe wall 3").area();
    assert!((area - 200.0 * (10.0 + 20.0 * 2f64.sqrt())).abs() < 1e-6, "{area}");
    assert_eq!(ids, ["Toe toe 2", "Toe crest 3", "Toe wall 3"].iter().map(|name| named(&s, name).id).collect::<Vec<_>>(), "the same objects");
    let (berm, top) = (record_of(&s, "Toe toe 2"), record_of(&s, "Toe crest 3"));
    assert_eq!((berm["from_revision"].as_u64(), berm["made_revision"].as_u64()), (Some(1 + dragged as u64), Some(2)), "built from the crest as edited: {berm}");
    assert_eq!((top["from_revision"].as_u64(), top["made_revision"].as_u64()), (Some(2), Some(2)), "built from the berm's line as rebuilt: {top}");

    s.undo().unwrap();
    assert!(at(&s, "Toe toe 2", 80.0, 120.0) && at(&s, "Toe crest 3", 100.0, 140.0), "one undo takes the rebuild back");
    assert!(at(&s, "Toe crest 1", 75.0, 120.0), "and leaves your edit");

    let run = s.run("face.rebuild", json!({ "from": toe })).unwrap();
    assert_eq!(run.messages[0], "3 steps built again from Toe as it is now");
    assert!(run.messages.contains(&"Toe crest 1 had been edited by hand: it's built again, and undo brings your edit back".to_string()), "{:?}", run.messages);
    assert!(at(&s, "Toe crest 1", 70.0, 120.0) && at(&s, "Toe toe 2", 80.0, 120.0) && at(&s, "Toe crest 3", 100.0, 140.0), "from the toe as it is");
}

/// A rebuild is the steps built again by hand. With road A carried up a wall, over a 5 m berm and up the next, the first
/// crest's far end dragged 3 m out, then Rebuild: the berm's line, the top crest, the road's edges to 1 µm and the top
/// wall's sheet to 1 cm² are what taking out the two steps and building them again from the edited crest makes.
#[test]
fn a_rebuild_is_the_steps_built_again_by_hand() {
    let mut s = whole_and_by_steps("after", false);
    let end = line_of(&s, "Built crest 1").vertices.len() - 1;
    drag(&mut s, "Built crest 1", end, [0.0, 3.0]);
    let run = s.run("face.rebuild", json!({ "from": named(&s, "Built crest 1").id })).unwrap();
    assert!(run.messages.iter().any(|m| m.starts_with("road A: ") && m.contains(" to 120.0")), "{:?}", run.messages);
    let lines = ["Built toe 2", "Built crest 3", "Built road A pit edge 3", "Built road A wall edge 3"];
    let rebuilt: Vec<Polyline> = lines.iter().map(|name| line_of(&s, name)).collect();
    let sheet = sheet_of(&s, "Built wall 3").area();
    s.undo().unwrap();

    take_out(&mut s, 2, 3);
    s.run("face.build_step", json!({ "from": named(&s, "Built crest 1").id, "step": { "op": "berm", "width": 5.0 } })).unwrap();
    s.run("face.build_step", json!({ "from": named(&s, "Built toe 2").id, "step": wall(120.0, 45.0) })).unwrap();
    for (name, line) in lines.iter().zip(&rebuilt) {
        same_line(&line_of(&s, name), line, name);
    }
    let by_hand = sheet_of(&s, "Built wall 3").area();
    assert!((by_hand - sheet).abs() < 1e-4, "{sheet} m² rebuilt against {by_hand} m² by hand");
}

/// The builder's gate (Build Plan, Manual builder plan): on Tim's step template, the crest at RL 145 edited by dragging a
/// vertex 2 m out, then Rebuild from here: everything above follows, roads and all. Six steps are built again; A and B still
/// turn and join into one pad on the berm at RL 155 and both reach RL 175 with nothing crossing. It's what taking out those
/// steps and building them again from the edited crest makes, every line to 1 µm, and one undo takes it back. Timed.
#[test]
fn tims_template_rebuilt_from_an_edited_crest() {
    let (mut s, steps, _) = tims_template();
    let before: Vec<Polyline> = (6..=11).map(|k| line_of(&s, &step_line("Built", k))).collect();
    let crest = line_of(&s, "Built crest 5");
    let middle = crest.vertices.iter().enumerate().min_by(|a, b| (a.1[0] - E - 600.0).abs().total_cmp(&(b.1[0] - E - 600.0).abs())).unwrap().0;
    drag(&mut s, "Built crest 5", middle, [0.0, 2.0]);
    let started = Instant::now();
    let run = s.run("face.rebuild", json!({ "from": named(&s, "Built crest 5").id })).unwrap();
    println!("Tim's template, 6 steps rebuilt from crest 5: {} ms", started.elapsed().as_millis());
    assert_eq!(run.messages[0], "6 steps built again from Built crest 5 as it is now");
    assert!(!run.messages.iter().any(|m| m.contains("stopped") || m.contains("triangles cross") || m.contains("edited")), "{:?}", run.messages);
    assert!(run.messages.iter().any(|m| m == "the switchbacks of A and B are one pad"), "{:?}", run.messages);
    for ramp in ["A", "B"] {
        assert!(run.messages.iter().any(|m| m.starts_with(&format!("road {ramp}: ")) && m.contains(" to 175.0")), "{ramp}: {:?}", run.messages);
    }
    let rebuilt: Vec<Polyline> = (6..=11).map(|k| line_of(&s, &step_line("Built", k))).collect();
    let tops: Vec<String> = ["A", "B"].iter().map(|ramp| text(&named(&s, "Built crest 11"), &format!("road_top {ramp} pit"))).collect();
    s.undo().unwrap();
    for (k, line) in (6..=11).zip(&before) {
        same_line(&line_of(&s, &step_line("Built", k)), line, &format!("step {k} after the undo"));
    }

    take_out(&mut s, 6, 11);
    let mut from = named(&s, "Built crest 5").id;
    for (k, step) in steps.iter().enumerate().skip(5) {
        let run = s.run("face.build_step", json!({ "from": from, "step": step })).unwrap();
        from = created(&run)[0];
        same_line(&line_of(&s, &step_line("Built", k + 1)), &rebuilt[k - 5], &format!("step {}", k + 1));
    }
    for (ramp, top) in ["A", "B"].iter().zip(&tops) {
        let by_hand: Vec<f64> = text(&named(&s, "Built crest 11"), &format!("road_top {ramp} pit")).split(' ').map(|c| c.parse().unwrap()).collect();
        let rebuilt: Vec<f64> = top.split(' ').map(|c| c.parse().unwrap()).collect();
        assert!(by_hand.iter().zip(&rebuilt).all(|(a, b)| (a - b).abs() < 1e-6), "{ramp}: {by_hand:?} against {rebuilt:?}");
    }
}

/// Copies a folder, for a project opened here to leave the original alone.
fn copy_folder(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The made-up grid pit (decision 0034), where it's been made (`testdata/synthetic/grid_template.py`; it isn't in git): Strip
/// 5 of Grid switchbacks built a step at a time, a copy of the project opened so the original stays as it is. From the toe on
/// the L floor, run on 100 m each way, walls at 65° to the L roof and M floor, M roof and U floor, U roof and BOW, and at 45° to
/// the topography, with 30 m berms, ramp C 30 m wide at 10 % placed on the first wall and turning back on every berm, its pads
/// on the M floor, the U floor and BOW. Every step builds with nothing crossing and C goes to the top. Each step's time is
/// printed against the design run whole over all ten strips; the builder's gate is a second a step. Then the first crest is
/// edited and the ten steps above it rebuilt, timed.
#[test]
#[ignore = "needs the made-up grid pit, which isn't in git; opt in with --ignored"]
fn a_grid_strip_built_a_step_at_a_time() {
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic/Grid template.zdproj");
    assert!(original.join("manifest.json").exists(), "no made-up grid pit at {}: run testdata/synthetic/grid_template.py to make it", original.display());
    let copy = std::env::temp_dir().join(format!("zd-build-grid-{}", std::process::id()));
    copy_folder(&original, &copy);
    let mut s = Session::open(&copy).unwrap();
    let id = |s: &Session, name: &str| named(s, name).id;
    let grid = |s: &Session, name: &str| json!({ "kind": "surface", "id": id(s, name) });
    let up = |s: &Session, to: &str, batter: f64| json!({ "op": "project", "to": grid(s, to), "batter": batter });
    let berm = json!({ "op": "berm", "width": 30.0, "turns": ["C"] });
    let road = json!({ "name": "C", "string": id(&s, "Ramp C"), "fixed": "start", "heading": "along", "width": 30.0, "grade": 10.0 });
    let mut first = up(&s, "L roof", 65.0);
    first["ramps"] = json!([road]);
    let steps = [first, up(&s, "M floor", 65.0), berm.clone(), up(&s, "M roof", 65.0), up(&s, "U floor", 65.0), berm.clone(), up(&s, "U roof", 65.0), up(&s, "BOW", 65.0), berm, up(&s, "Topo", 45.0)];

    let started = Instant::now();
    s.run("face.project", json!({ "design": id(&s, "Grid switchbacks") })).unwrap();
    println!("Grid switchbacks run whole, ten strips: {} ms", started.elapsed().as_millis());
    let mut from = id(&s, "Strip 5");
    let mut slowest = 0;
    for (k, step) in steps.iter().enumerate() {
        let mut params = json!({ "from": from, "step": step });
        if k == 0 {
            params["lies_on"] = grid(&s, "L floor");
            params["side"] = json!("left");
            params["run_on"] = json!([100.0, 100.0]);
            params["condition"] = json!(0.1);
            params["name"] = json!("Built");
        }
        let started = Instant::now();
        let run = s.run("face.build_step", params).unwrap_or_else(|e| panic!("step {}: {e}", k + 1));
        let ms = started.elapsed().as_millis();
        slowest = slowest.max(ms);
        println!("step {}: {ms} ms: {:?}", k + 1, run.messages);
        assert!(!run.messages.iter().any(|m| m.contains("stopped") || m.contains("triangles cross")), "step {}: {:?}", k + 1, run.messages);
        from = created(&run)[0];
    }
    let top = &s.run("face.build_step", json!({ "from": from, "step": { "op": "berm", "width": 30.0 } })).unwrap();
    assert!(top.messages.iter().any(|m| m.starts_with("road C: crosses the berm")), "C came out on the top crest: {:?}", top.messages);
    println!("slowest step: {slowest} ms");
    // Rebuild from the first crest with a vertex near its middle dragged 2 m out: the ten steps above it built again, the berm on the top crest too.
    let first = named(&s, "Built crest 1");
    let Body::String(crest) = &first.body else { panic!("a line") };
    let middle = crest.vertices.len() / 2;
    drag(&mut s, "Built crest 1", middle, [0.0, 2.0]);
    let started = Instant::now();
    let run = s.run("face.rebuild", json!({ "from": first.id })).unwrap();
    println!("ten steps rebuilt from crest 1: {} ms", started.elapsed().as_millis());
    assert_eq!(run.messages[0], "10 steps built again from Built crest 1 as it is now");
    assert!(!run.messages.iter().any(|m| m.contains("stopped") || m.contains("triangles cross")), "{:?}", run.messages);
    assert!(run.messages.iter().any(|m| m.starts_with("road C: ") && m.contains(" to ")), "{:?}", run.messages);
    drop(s);
    let _ = std::fs::remove_dir_all(&copy);
}

/// A small grid in Vulcan's plain layout: big-endian, nodes from the south-west at 25 m, x fastest, all live, at one height.
fn level_vulcan_grid(z: f32, cols: usize, rows: usize) -> Vec<u8> {
    let mut out = b"SYNTHETIC TEST GRID             ".to_vec();
    let (x0, y0) = (500_025.0f64, 7_000_625.0f64);
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

/// Walls and berms to grids and surfaces, as Tim needs (2026-11-07: "the manual face builder allows project to grids and
/// surfaces"). A seam's floor and roof as Vulcan grids at RL 100 and 120, and a surface at RL 140 imported from XYZ. A toe
/// drawn at RL 0 lies on the floor grid, so it's registered to RL 100. A wall at 45° to the roof grid stands 20 m out at RL
/// 120, a 10 m berm lies on the roof grid 30 m out, and a wall at 45° from it to the surface stands 50 m out at RL 140. The
/// sheets are 150 × 20√2 m² and 150 × (10 + 20√2) m², to 1e-6 m², and each line says what it lies on.
#[test]
fn walls_and_berms_go_to_grids_and_surfaces() {
    let dir = std::env::temp_dir().join(format!("zd-build-grids-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pitlower.sfg"), level_vulcan_grid(100.0, 13, 13)).unwrap();
    std::fs::write(dir.join("pitlower.srg"), level_vulcan_grid(120.0, 13, 13)).unwrap();
    let mut xyz = String::new();
    for row in 0..13 {
        for col in 0..13 {
            xyz += &format!("{} {} 140\n", 500_025.0 + col as f64 * 25.0, 7_000_625.0 + row as f64 * 25.0);
        }
    }
    std::fs::write(dir.join("upper.xyz"), xyz).unwrap();
    let mut s = Session::with_builtins();
    s.run("grid.import_vulcan_seam", json!({ "dir": dir.to_string_lossy(), "prefix": "pit", "seam": "lower", "layer": "geology" })).unwrap();
    let upper = created(&s.run("surface.import", json!({ "path": dir.join("upper.xyz").to_string_lossy(), "layer": "surfaces" })).unwrap())[0];
    std::fs::remove_dir_all(&dir).unwrap();
    let (floor, roof) = (named(&s, "LOWER sf"), named(&s, "LOWER sr"));
    assert!(matches!(floor.body, Body::Grid(_)) && matches!(roof.body, Body::Grid(_)), "the seam's floor and roof are grids");
    let on = |id: ObjectId| json!({ "kind": "surface", "id": id });
    let points = [[500_100.0, 7_000_700.0, 0.0], [500_250.0, 7_000_700.0, 0.0]];
    let toe = created(&s.run("string.create", json!({ "name": "Toe", "layer": "toes", "points": points })).unwrap())[0];

    let one = s.run("face.build_step", json!({ "from": toe, "step": { "op": "project", "to": on(roof.id), "batter": 45.0 }, "lies_on": on(floor.id), "side": "left" })).unwrap();
    assert!(one.messages[0].starts_with("Toe step 1: a wall to LOWER sr at 45° from Toe, "), "{:?}", one.messages);
    let at = |line: &Polyline, y: f64, z: f64| line.vertices.iter().all(|p| (p[1] - y).abs() < 1e-6 && (p[2] - z).abs() < 1e-6);
    assert!(at(&line_of(&s, "Toe crest 1"), 7_000_720.0, 120.0), "{:?}", line_of(&s, "Toe crest 1").vertices);
    assert_eq!(text(&named(&s, "Toe crest 1"), "build_on"), format!(r#"{{"kind":"surface","id":"{}"}}"#, roof.id));
    let first = sheet_of(&s, "Toe wall 1").area();
    assert!((first - 150.0 * 20.0 * 2f64.sqrt()).abs() < 1e-6, "{first}");

    let two = s.run("face.build_step", json!({ "from": named(&s, "Toe crest 1").id, "step": { "op": "berm", "width": 10.0 } })).unwrap();
    assert_eq!(two.messages[0], "Toe step 2: a berm 10 m wide on LOWER sr, from Toe crest 1");
    assert!(at(&line_of(&s, "Toe toe 2"), 7_000_730.0, 120.0), "{:?}", line_of(&s, "Toe toe 2").vertices);

    let three = s.run("face.build_step", json!({ "from": named(&s, "Toe toe 2").id, "step": { "op": "project", "to": on(upper), "batter": 45.0 } })).unwrap();
    assert!(three.messages[0].starts_with("Toe step 3: a wall to upper at 45° from Toe toe 2, with the berm from Toe crest 1, "), "{:?}", three.messages);
    assert!(at(&line_of(&s, "Toe crest 3"), 7_000_750.0, 140.0), "{:?}", line_of(&s, "Toe crest 3").vertices);
    let second = sheet_of(&s, "Toe wall 3").area();
    assert!((second - 150.0 * (10.0 + 20.0 * 2f64.sqrt())).abs() < 1e-6, "{second}");
}
