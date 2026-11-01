//! Walls through the registry, as the app calls them, then stepping between real grids. The second is
//! opt-in, because the data stays local: `ZD_WALL_GRIDS` lists Vulcan grid files, bottom of the wall
//! first, separated by `;`.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, Grid, ObjectId, Polyline, TriMesh};
use zd_geom::{COINCIDENT, OffsetSide, SectionPlane, WallStep, drape_string, find_crossings, next_toe, project_wall, slice_mesh, triangulate_grid};

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

/// A level surface on a 10 m grid, as an XYZ file, imported.
fn level_surface(s: &mut Session, name: &str, rl: f64) -> ObjectId {
    let dir = std::env::temp_dir().join(format!("zd-walls-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut xyz = String::new();
    for row in 0..=30 {
        for col in 0..=30 {
            xyz += &format!("{} {} {rl}\n", E + col as f64 * 10.0, N + row as f64 * 10.0);
        }
    }
    let path = dir.join(format!("{name}.xyz"));
    std::fs::write(&path, xyz).unwrap();
    let id = created(&s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "surfaces" })).unwrap())[0];
    std::fs::remove_dir_all(&dir).unwrap();
    id
}

#[test]
fn a_step_makes_a_crest_a_wall_and_the_next_toe_and_leaves_the_string_alone() {
    let mut s = Session::with_builtins();
    let target = level_surface(&mut s, "roof", 120.0);
    let points = [[E + 50.0, N + 50.0, 100.0], [E + 250.0, N + 50.0, 100.0]];
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];

    let record = s.run("face.project_step", json!({ "string": toe, "target": target, "batter": 45.0, "side": "left", "berm": 10.0 })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 3, "{:?}", record.messages);
    assert!(record.messages[0].starts_with("wall at 45° to "), "{:?}", record.messages);
    assert_eq!(s.project().object(toe).unwrap().revision, 1, "the string is an input, and stays as it was");

    // 45 degrees and 20 m up is 20 m out, and the berm is 10 m on from there.
    let crest = line(&s, made[0]);
    assert_eq!(crest.vertices.first().map(|p| (p[1] - N, p[2])), Some((70.0, 120.0)));
    let next = line(&s, made[2]);
    assert!(next.vertices.iter().all(|p| (p[1] - N - 80.0).abs() < 1e-6 && p[2] == 120.0), "{:?}", next.vertices);
    let Body::Surface(wall) = &s.project().object(made[1]).unwrap().body else { panic!("the wall is a surface") };
    assert!((wall.mesh().area() - 200.0 * 20.0 * 2f64.sqrt()).abs() < 1e-3, "{}", wall.mesh().area());
    assert_eq!(wall.validity().non_self_intersecting, Some(true));
    assert_eq!(s.project().layer(s.project().object(made[1]).unwrap().layer).unwrap().name, "design");

    // A second step from the next toe, at once and with the same side.
    let upper = level_surface(&mut s, "upper", 150.0);
    let second = s.run("face.project_step", json!({ "string": made[2], "target": upper, "batter": 45.0, "side": "left" })).unwrap();
    let top = line(&s, created(&second)[0]);
    assert!(top.vertices.iter().all(|p| (p[1] - N - 110.0).abs() < 1e-6), "{:?}", top.vertices);

    // The second step, the surface it rose to, then the first step.
    for _ in 0..3 {
        s.undo().unwrap();
    }
    assert!(s.project().object(made[0]).is_none(), "undone with one undo a run");
}

#[test]
fn a_bad_step_is_refused_with_its_reason() {
    let mut s = Session::with_builtins();
    let target = level_surface(&mut s, "roof", 120.0);
    let points = [[E + 50.0, N + 50.0, 100.0], [E + 250.0, N + 50.0, 100.0]];
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];
    for (batter, berm, tolerance, reason) in [(0.0, 0.0, 0.1, "the batter"), (95.0, 0.0, 0.1, "the batter"), (45.0, -1.0, 0.1, "the berm"), (45.0, 0.0, 30.0, "cut corners")] {
        let error = s.run("face.project_step", json!({ "string": toe, "target": target, "batter": batter, "side": "left", "berm": berm, "tolerance": tolerance })).unwrap_err();
        assert!(error.to_string().contains(reason), "{error}");
    }
    // Stepping again to the grid the next toe already lies on.
    let made = created(&s.run("face.project_step", json!({ "string": toe, "target": target, "batter": 45.0, "side": "left", "berm": 10.0 })).unwrap());
    let error = s.run("face.project_step", json!({ "string": made[2], "target": target, "batter": 45.0, "side": "left" })).unwrap_err();
    assert!(error.to_string().contains("no wall to make"), "{error}");
}

/// The node nearest the middle of the live grid, and the way the grid dips there.
fn middle_and_strike(grid: &Grid) -> ([f64; 2], [f64; 2]) {
    let live: Vec<[f64; 3]> = (0..grid.rows)
        .flat_map(|row| (0..grid.cols).map(move |col| (col, row)))
        .filter_map(|(col, row)| {
            let (z, [x, y]) = (grid.z_at(col, row), grid.node_xy(col, row));
            (!z.is_nan()).then_some([x, y, z])
        })
        .collect();
    let n = live.len() as f64;
    let (mx, my, mz) = (live.iter().map(|p| p[0]).sum::<f64>() / n, live.iter().map(|p| p[1]).sum::<f64>() / n, live.iter().map(|p| p[2]).sum::<f64>() / n);
    // The best-fit plane's gradient, by least squares about the middle.
    let (mut xx, mut xy, mut yy, mut xz, mut yz) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for p in &live {
        let (x, y, z) = (p[0] - mx, p[1] - my, p[2] - mz);
        (xx, xy, yy, xz, yz) = (xx + x * x, xy + x * y, yy + y * y, xz + x * z, yz + y * z);
    }
    let det = xx * yy - xy * xy;
    let (gx, gy) = ((xz * yy - yz * xy) / det, (yz * xx - xz * xy) / det);
    let length = gx.hypot(gy);
    ([mx, my], [-gy / length, gx / length])
}

/// A design that takes each string both ways with a profile for each: up to `to` at 45 degrees, a 5 m
/// berm and on to RL 160, and down to RL 50 at a batter typed on the row.
fn level_design(strings: &[ObjectId], from: ObjectId, to: ObjectId) -> serde_json::Value {
    let lines: Vec<_> = strings.iter().flat_map(|s| ["Highwall", "Highwall down"].map(|profile| json!({ "string": s, "profile": profile, "side": "left", "run_on": [10.0, 10.0] }))).collect();
    json!({
        "layer": "faces",
        "lines": lines,
        "profiles": [{
            "name": "Highwall",
            "direction": "up",
            "register": from,
            "steps": [
                { "step": "project", "to": { "kind": "surface", "id": to }, "batter": "HW batter", "condition": 0.1 },
                { "step": "berm", "width": "HW berm", "condition": 0.1 },
                { "step": "project", "to": { "kind": "rl", "rl": 160.0 }, "batter": "HW batter", "condition": 0.1 },
            ],
        }, {
            "name": "Highwall down",
            "direction": "down",
            "register": from,
            "steps": [{ "step": "project", "to": { "kind": "rl", "rl": 50.0 }, "batter": 45.0, "condition": 0.1 }],
        }],
        "rules": [
            { "name": "HW batter", "value": { "type": "fixed", "value": 45.0 } },
            { "name": "HW berm", "value": { "type": "fixed", "value": 5.0 } },
        ],
    })
}

/// Live objects on a layer, by kind: strings and surfaces.
fn on_layer(s: &Session, layer: &str) -> (usize, usize) {
    let Some(layer) = s.project().layer_named(layer).map(|l| l.id) else { return (0, 0) };
    let here: Vec<_> = s.project().objects().filter(|o| o.layer == layer).collect();
    (here.iter().filter(|o| matches!(o.body, Body::String(_))).count(), here.iter().filter(|o| matches!(o.body, Body::Surface(_))).count())
}

#[test]
fn a_design_runs_every_line_in_one_run_and_runs_again_over_itself() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let string = |s: &mut Session, name: &str, y: f64| {
        let points = [[E + 50.0, N + y, 0.0], [E + 250.0, N + y, 0.0]];
        created(&s.run("string.create", json!({ "name": name, "layer": "design", "points": points })).unwrap())[0]
    };
    let (one, two) = (string(&mut s, "strip 1", 60.0), string(&mut s, "strip 2", 200.0));
    let saved = s.run("face.save_design", json!({ "name": "Pit design", "content": level_design(&[one, two], floor, roof) })).unwrap();
    let design = created(&saved)[0];

    let record = s.run("face.project", json!({ "design": design })).unwrap();
    // Each string goes up in two walls and down in one, each with a toe and a crest, and one face each way.
    assert_eq!(on_layer(&s, "faces"), (12, 4), "{:?}", record.messages);
    assert_eq!(record.messages.len(), 4, "{:?}", record.messages);
    assert!(record.messages[0].starts_with("strip 1 Highwall: 2 walls"), "{:?}", record.messages);
    assert_eq!(s.project().object(one).unwrap().revision, 1, "a line's string stays as it was");
    let face = s.project().objects().find(|o| o.name == "strip 1 Highwall face").unwrap().clone();
    let Body::Surface(sheet) = &face.body else { panic!("a face is a surface") };
    // 220 m of toe with the run-on: walls 20 and 40 m high at 45 degrees, and the berm.
    let want = 220.0 * ((20.0 + 40.0) * 2f64.sqrt() + 5.0);
    assert!((sheet.mesh().area() - want).abs() < 1e-3, "{} against {want}", sheet.mesh().area());
    assert_eq!(sheet.validity().non_self_intersecting, Some(true));
    // Down is 50 m at 45 degrees, from the same toe as up.
    let below = s.project().objects().find(|o| o.name == "strip 1 Highwall down face").unwrap();
    let Body::Surface(sheet) = &below.body else { panic!("a face is a surface") };
    assert!((sheet.mesh().area() - 220.0 * 50.0 * 2f64.sqrt()).abs() < 1e-3, "{}", sheet.mesh().area());
    let toe_of = |name: &str| match &s.project().objects().find(|o| o.name == name).unwrap().body {
        Body::String(line) => line.vertices.clone(),
        _ => panic!("a toe is a string"),
    };
    assert_eq!(toe_of("strip 1 Highwall toe 1"), toe_of("strip 1 Highwall down toe 1"));

    // Change a rule and run again: it replaces the last run, and one undo brings that back.
    let mut content = level_design(&[one, two], floor, roof);
    content["rules"][1]["value"]["value"] = json!(20.0);
    s.run("face.save_design", json!({ "design": design, "name": "Pit design", "content": content })).unwrap();
    s.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&s, "faces"), (12, 4));
    let wider = s.project().objects().find(|o| o.name == "strip 1 Highwall face").unwrap();
    let Body::Surface(sheet) = &wider.body else { panic!("a face is a surface") };
    assert!((sheet.mesh().area() - 220.0 * ((20.0 + 40.0) * 2f64.sqrt() + 20.0)).abs() < 1e-3, "{}", sheet.mesh().area());
    s.undo().unwrap();
    assert_eq!(on_layer(&s, "faces"), (12, 4), "undo brings the first run back");
    assert_eq!(s.project().objects().find(|o| o.name == "strip 1 Highwall face").unwrap().id, face.id);
}

/// A rough toe, cleaned on its Register row, and a berm that names the surface it registers on.
#[test]
fn a_row_can_condition_the_toe_and_name_a_berms_surface() {
    let mut s = Session::with_builtins();
    let (floor, roof, shelf) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0), level_surface(&mut s, "shelf", 123.0));
    let points = [[E + 50.0, N + 60.0, 0.0], [E + 100.0, N + 60.04, 0.0], [E + 150.0, N + 59.97, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];
    let content = json!({
        "layer": "faces",
        "lines": [{ "string": toe, "profile": "Highwall", "side": "left" }],
        "profiles": [{
            "name": "Highwall",
            "direction": "up",
            "register": floor,
            "condition": 0.1,
            "steps": [
                { "step": "project", "to": { "kind": "surface", "id": roof }, "batter": 45.0, "condition": 0.1 },
                { "step": "berm", "on": { "kind": "surface", "id": shelf }, "width": 5.0, "condition": 0.1 },
                { "step": "project", "to": { "kind": "rl", "rl": 160.0 }, "batter": 45.0 },
            ],
        }],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "Pit design", "content": content })).unwrap())[0];
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert!(record.messages[0].contains("toe conditioned from 4 to 2 vertices"), "{:?}", record.messages);
    let string = |name: &str| match &s.project().objects().find(|o| o.name == name).unwrap().body {
        Body::String(line) => line.vertices.clone(),
        _ => panic!("a toe is a string"),
    };
    assert!(string("toe Highwall toe 1").iter().all(|p| (p[1] - N - 60.0).abs() < 1e-9), "cleaned to one straight line");
    assert!(string("toe Highwall toe 2").iter().all(|p| (p[2] - 123.0).abs() < 1e-6));
    assert_eq!(s.project().object(toe).unwrap().revision, 1, "the string stays as it was drawn");
}

/// A line on a layer takes what's there at each run. The design's own results land beside the strings
/// here, and aren't taken as more strings when it runs again.
#[test]
fn a_line_that_names_a_layer_takes_what_is_on_it_at_each_run() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let string = |s: &mut Session, name: &str, y: f64| {
        let points = [[E + 50.0, N + y, 0.0], [E + 250.0, N + y, 0.0]];
        created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points })).unwrap())[0]
    };
    let one = string(&mut s, "strip 1", 60.0);
    let two = string(&mut s, "strip 2", 200.0);
    let layer = s.project().layer_named("toes").unwrap().id;
    let mut content = level_design(&[], floor, roof);
    content["layer"] = json!("toes");
    content["lines"] = json!([{ "layer": layer, "profile": "Highwall", "side": "left", "run_on": [10.0, 10.0] }]);
    let design = created(&s.run("face.save_design", json!({ "name": "By layer", "content": content.clone() })).unwrap())[0];

    // Each string goes up in two walls, a toe and a crest each, with a face: 2 strings and 2 x 4 made.
    s.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&s, "toes"), (2 + 8, 2));
    s.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&s, "toes"), (2 + 8, 2), "its own results aren't taken for strings");

    string(&mut s, "strip 3", 130.0);
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&s, "toes"), (3 + 12, 3), "a string added since comes in: {:?}", record.messages);
    s.run("object.delete", json!({ "object": one })).unwrap();
    s.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&s, "toes"), (2 + 8, 2), "and one deleted goes");

    s.run("layer.rename", json!({ "layer": "toes", "name": "highwall toes" })).unwrap();
    s.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&s, "highwall toes"), (2, 0), "the layer is held by id, and what a run makes goes to the design's own layer");
    assert_eq!(on_layer(&s, "toes"), (8, 2));

    // Named as well on a line of its own, a string still runs once under a profile.
    content["lines"] = json!([
        { "layer": layer, "profile": "Highwall", "side": "left", "run_on": [10.0, 10.0] },
        { "string": two, "profile": "Highwall", "side": "left", "run_on": [10.0, 10.0] },
    ]);
    let both = created(&s.run("face.save_design", json!({ "name": "Both", "content": content.clone() })).unwrap())[0];
    let record = s.run("face.project", json!({ "design": both })).unwrap();
    assert_eq!(record.messages.iter().filter(|m| m.starts_with("strip 2 Highwall")).count(), 1, "{:?}", record.messages);

    let empty = s.run("layer.create", json!({ "name": "nothing" })).unwrap();
    assert!(empty.outputs.is_empty());
    content["lines"][0]["layer"] = json!(s.project().layer_named("nothing").unwrap().id);
    content["lines"] = json!([content["lines"][0]]);
    let none = created(&s.run("face.save_design", json!({ "name": "Empty", "content": content })).unwrap())[0];
    let refused = s.run("face.project", json!({ "design": none })).unwrap_err();
    assert!(refused.to_string().contains("no strings on it"), "{refused}");
}

/// One wall of a design from a floor to a roof 20 m above it, from a string running east along y = 60 and
/// rising to the north, so it faces south, with the rules given. What the run made, by name.
fn run_with_rule(s: &mut Session, name: &str, points: &[[f64; 3]], steps: serde_json::Value, rules: serde_json::Value, floor: ObjectId) -> Result<Polyline, zd_commands::CommandError> {
    let string = created(&s.run("string.create", json!({ "name": name, "layer": "rule toes", "points": points })).unwrap())[0];
    let content = json!({
        "layer": name,
        "lines": [{ "string": string, "profile": "Wall", "side": "left", "run_on": [0.0, 0.0] }],
        "profiles": [{ "name": "Wall", "direction": "up", "register": floor, "steps": steps }],
        "rules": rules,
    });
    let design = created(&s.run("face.save_design", json!({ "name": name, "content": content }))?)[0];
    s.run("face.project", json!({ "design": design }))?;
    Ok(line(s, s.project().objects().find(|o| o.name == format!("{name} Wall crest 1")).unwrap().id))
}

fn has(line: &Polyline, x: f64, y: f64) -> bool {
    line.vertices.iter().any(|p| (p[0] - E - x).abs() < 1e-5 && (p[1] - N - y).abs() < 1e-5)
}

/// Rise of 2 over 1: 10 m out in a 20 m rise, against 20 m at 45 degrees.
fn steep() -> f64 {
    2f64.atan().to_degrees()
}

#[test]
fn a_rule_gives_a_wall_a_batter_for_each_edge() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let toe = [[E + 50.0, N + 60.0, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let up = |batter: &str| json!([{ "step": "project", "to": { "kind": "surface", "id": roof }, "batter": batter, "condition": null }]);
    let square = [[E + 100.0, N, 0.0], [E + 150.0, N, 0.0], [E + 150.0, N + 300.0, 0.0], [E + 100.0, N + 300.0, 0.0]];
    let polygon = created(&s.run("string.create", json!({ "name": "domain", "layer": "domains", "points": square, "closed": true })).unwrap())[0];

    // Inside the polygon it's 45 degrees and outside steeper. The change falls on the polygon's edges,
    // though the string has no vertex there, and the crest bevels into the steeper side each time.
    let rules = json!([{ "name": "Batter", "value": { "type": "inside_polygon", "polygon": polygon, "inside": 45.0, "outside": steep() } }]);
    let crest = run_with_rule(&mut s, "by polygon", &toe, up("Batter"), rules, floor).unwrap();
    assert!(has(&crest, 50.0, 70.0) && has(&crest, 250.0, 70.0), "{:?}", crest.vertices);
    assert!(has(&crest, 100.0, 80.0) && has(&crest, 150.0, 80.0), "{:?}", crest.vertices);
    assert!(has(&crest, 90.0, 70.0) && has(&crest, 160.0, 70.0), "{:?}", crest.vertices);

    // Facing south, in the sector; and a sector that doesn't hold it gives the other value.
    let rules = json!([{ "name": "Batter", "value": { "type": "segment_azimuth", "sectors": [{ "from": 135.0, "to": 225.0, "value": steep() }], "otherwise": 45.0 } }]);
    let crest = run_with_rule(&mut s, "facing south", &toe, up("Batter"), rules, floor).unwrap();
    assert!(crest.vertices.iter().all(|p| (p[1] - N - 70.0).abs() < 1e-5), "{:?}", crest.vertices);
    let rules = json!([{ "name": "Batter", "value": { "type": "segment_azimuth", "sectors": [{ "from": 315.0, "to": 45.0, "value": steep() }], "otherwise": 45.0 } }]);
    let crest = run_with_rule(&mut s, "not north", &toe, up("Batter"), rules, floor).unwrap();
    assert!(crest.vertices.iter().all(|p| (p[1] - N - 80.0).abs() < 1e-5), "{:?}", crest.vertices);

    // A value for each of a string's two segments.
    let two = [toe[0], [E + 150.0, N + 60.0, 0.0], toe[1]];
    let rules = json!([{ "name": "Batter", "value": { "type": "fixed_list", "values": [45.0, steep()] } }]);
    let crest = run_with_rule(&mut s, "by segment", &two, up("Batter"), rules.clone(), floor).unwrap();
    assert!(has(&crest, 150.0, 80.0) && has(&crest, 160.0, 70.0), "{:?}", crest.vertices);
    let refused = run_with_rule(&mut s, "wrong count", &toe, up("Batter"), rules, floor).unwrap_err();
    assert!(refused.to_string().contains("Batter has 2 values for the 1 segments of wrong count"), "{refused}");

    // A solid from 150 to 180 east, around the toe's RL: the wall goes by where its string is.
    let v: Vec<[f64; 3]> = (0..8).map(|i| [E + 150.0 + (i & 1) as f64 * 30.0, N + (i >> 1 & 1) as f64 * 300.0, 90.0 + (i >> 2) as f64 * 20.0]).collect();
    let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let solid = s.run("solid.from_mesh", json!({ "name": "weak zone", "layer": "domains", "vertices": v, "triangles": t })).unwrap().outputs[0].id;
    let rules = json!([{ "name": "Batter", "value": { "type": "inside_solid", "solid": solid, "inside": 45.0, "outside": steep() } }]);
    let crest = run_with_rule(&mut s, "by solid", &toe, up("Batter"), rules, floor).unwrap();
    assert!(has(&crest, 150.0, 80.0) && has(&crest, 180.0, 80.0), "{:?}", crest.vertices);
}

/// By horizon goes row by row: a wall takes the value for what it finishes on, and a berm the value for
/// what the wall below it reached.
#[test]
fn by_horizon_gives_each_row_the_value_for_the_surface_it_finishes_on() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let toe = [[E + 50.0, N + 60.0, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let rules = json!([
        { "name": "Batter", "value": { "type": "by_horizon", "values": [{ "surface": roof, "value": 45.0 }], "otherwise": steep() } },
        { "name": "Berm", "value": { "type": "by_horizon", "values": [{ "surface": roof, "value": 7.0 }], "otherwise": 1.0 } },
    ]);
    let steps = json!([
        { "step": "project", "to": { "kind": "surface", "id": roof }, "batter": "Batter", "condition": null },
        { "step": "berm", "width": "Berm", "condition": null },
        { "step": "project", "to": { "kind": "rl", "rl": 140.0 }, "batter": "Batter", "condition": null },
    ]);
    run_with_rule(&mut s, "by horizon", &toe, steps, rules, floor).unwrap();
    let crest = |s: &Session, n: usize| line(s, s.project().objects().find(|o| o.name == format!("by horizon Wall crest {n}")).unwrap().id);
    assert!(crest(&s, 1).vertices.iter().all(|p| (p[1] - N - 80.0).abs() < 1e-5), "the roof's batter, 20 m out");
    // Then a 7 m berm, and from the roof to RL 140 at the other batter, 10 m out.
    assert!(crest(&s, 2).vertices.iter().all(|p| (p[1] - N - 97.0).abs() < 1e-5), "{:?}", crest(&s, 2).vertices);
}

/// A berm wider inside a polygon, on a straight crest: the line steps out at the polygon's edge, reaching
/// back over the narrower stretch, so the change only ever widens.
#[test]
fn a_rule_gives_a_berm_a_width_for_each_edge() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let toe = [[E + 50.0, N + 60.0, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let square = [[E + 150.0, N, 0.0], [E + 300.0, N, 0.0], [E + 300.0, N + 300.0, 0.0], [E + 150.0, N + 300.0, 0.0]];
    let polygon = created(&s.run("string.create", json!({ "name": "wide", "layer": "domains", "points": square, "closed": true })).unwrap())[0];
    let rules = json!([{ "name": "Berm", "value": { "type": "inside_polygon", "polygon": polygon, "inside": 10.0, "outside": 5.0 } }]);
    let steps = json!([
        { "step": "project", "to": { "kind": "surface", "id": roof }, "batter": 45.0, "condition": null },
        { "step": "berm", "width": "Berm", "condition": null },
        { "step": "project", "to": { "kind": "rl", "rl": 130.0 }, "batter": 45.0, "condition": null },
    ]);
    run_with_rule(&mut s, "wider berm", &toe, steps, rules, floor).unwrap();
    let toe_two = line(&s, s.project().objects().find(|o| o.name == "wider berm Wall toe 2").unwrap().id);
    assert!(has(&toe_two, 50.0, 85.0) && has(&toe_two, 250.0, 90.0), "{:?}", toe_two.vertices);
    assert!(has(&toe_two, 145.0, 85.0) && has(&toe_two, 150.0, 90.0), "{:?}", toe_two.vertices);
}

/// A surface that's on the floor west of x = 100, and climbs to `top` by x = 200.
fn parting_surface(s: &mut Session, name: &str, top: f64) -> ObjectId {
    let dir = std::env::temp_dir().join(format!("zd-walls-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut xyz = String::new();
    for row in 0..=30 {
        for col in 0..=30 {
            let x = col as f64 * 10.0;
            xyz += &format!("{} {} {}\n", E + x, N + row as f64 * 10.0, 100.0 + top * ((x - 100.0) / 100.0).clamp(0.0, 1.0));
        }
    }
    let path = dir.join(format!("{name}.xyz"));
    std::fs::write(&path, xyz).unwrap();
    let id = created(&s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "grids" })).unwrap())[0];
    std::fs::remove_dir_all(&dir).unwrap();
    id
}

/// Three grids on one another to the west that part to the east: a wall and a 30 m berm to each. With no
/// minimum bench a berm runs out flat for every grid, wall or no wall, and with one the line stays put until
/// there's height to stand it on. Where the berm starts it tapers in at 45 degrees.
#[test]
fn a_berm_needs_a_wall_under_it_where_the_grids_sit_on_each_other() {
    let mut s = Session::with_builtins();
    let floor = level_surface(&mut s, "floor", 100.0);
    let (one, two) = (parting_surface(&mut s, "one", 20.0), parting_surface(&mut s, "two", 40.0));
    let toe = [[E + 20.0, N + 60.0, 0.0], [E + 280.0, N + 60.0, 0.0]];
    let steps = |min_bench: Option<f64>| {
        json!([
            { "step": "project", "to": { "kind": "surface", "id": one }, "batter": 45.0, "condition": null },
            { "step": "berm", "width": 30.0, "condition": null, "min_bench": min_bench },
            { "step": "project", "to": { "kind": "surface", "id": two }, "batter": 45.0, "condition": null },
            { "step": "berm", "width": 30.0, "condition": null, "min_bench": min_bench },
            { "step": "project", "to": { "kind": "rl", "rl": 160.0 }, "batter": 45.0, "condition": null },
        ])
    };
    let west_of = |line: &Polyline, x: f64| line.vertices.iter().filter(|p| p[0] - E < x).map(|p| p[1] - N).fold(f64::NEG_INFINITY, f64::max);
    let toe_three = |s: &Session, name: &str| line(s, s.project().objects().find(|o| o.name == format!("{name} Wall toe 3")).unwrap().id);

    run_with_rule(&mut s, "no limit", &toe, steps(None), json!([]), floor).unwrap();
    assert!((west_of(&toe_three(&s, "no limit"), 50.0) - 120.0).abs() < 1e-3, "two 30 m berms over nothing");

    run_with_rule(&mut s, "with limit", &toe, steps(Some(1.0)), json!([]), floor).unwrap();
    let third = toe_three(&s, "with limit");
    assert!((west_of(&third, 50.0) - 60.0).abs() < 1e-3, "no berm at all to the west: {:?}", third.vertices);
    // East of the parting there's the full pair: 20 m of wall, 30 of berm, 20 of wall and 30 more.
    let east = third.vertices.iter().filter(|p| p[0] - E > 240.0).map(|p| p[1] - N).fold(f64::INFINITY, f64::min);
    assert!((east - 160.0).abs() < 1e-3, "{east}");
    let record = s.project().objects().find(|o| o.name == "with limit Wall face").unwrap().clone();
    let Body::Surface(sheet) = &record.body else { panic!("a face is a surface") };
    assert_eq!(sheet.validity().non_self_intersecting, Some(true));

    // The first berm tapers out towards the west at 45 degrees, not in a square step.
    let second = line(&s, s.project().objects().find(|o| o.name == "with limit Wall toe 2").unwrap().id);
    let turned = second.vertices.windows(2).filter(|w| (w[1][1] - w[0][1]).abs() > 1.0).map(|w| ((w[1][1] - w[0][1]) / (w[1][0] - w[0][0])).abs()).fold(0.0, f64::max);
    assert!(turned < 1.5, "the steepest stretch in plan is {turned} in 1: {:?}", second.vertices);

    // Conditioned, as the table starts a row, a line stands clear of its crest by the tolerance instead.
    let conditioned = |h: f64| {
        json!([
            { "step": "project", "to": { "kind": "surface", "id": one }, "batter": 45.0, "condition": 0.1 },
            { "step": "berm", "width": 30.0, "condition": 0.1, "min_bench": h },
            { "step": "project", "to": { "kind": "surface", "id": two }, "batter": 45.0, "condition": 0.1 },
            { "step": "berm", "width": 30.0, "condition": 0.1, "min_bench": h },
            { "step": "project", "to": { "kind": "rl", "rl": 160.0 }, "batter": 45.0, "condition": 0.1 },
        ])
    };
    run_with_rule(&mut s, "conditioned", &toe, conditioned(1.0), json!([]), floor).unwrap();
    let third = toe_three(&s, "conditioned");
    assert!(west_of(&third, 50.0) - 60.0 < 0.5, "{:?}", third.vertices);
}

#[test]
fn a_rule_that_cant_work_says_why() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let toe = [[E + 50.0, N + 60.0, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let up = json!([{ "step": "project", "to": { "kind": "surface", "id": roof }, "batter": "Batter", "condition": null }]);
    let open = created(&s.run("string.create", json!({ "name": "open", "layer": "domains", "points": toe })).unwrap())[0];
    let rules = json!([{ "name": "Batter", "value": { "type": "inside_polygon", "polygon": open, "inside": 45.0, "outside": 60.0 } }]);
    let refused = run_with_rule(&mut s, "open polygon", &toe, up.clone(), rules, floor).unwrap_err();
    assert!(refused.to_string().contains("needs a closed string"), "{refused}");
    let rules = json!([{ "name": "Batter", "value": { "type": "segment_azimuth", "sectors": [{ "from": 0.0, "to": 90.0, "value": 95.0 }], "otherwise": 60.0 } }]);
    let refused = run_with_rule(&mut s, "too steep", &toe, up, rules, floor).unwrap_err();
    assert!(refused.to_string().contains("a batter must be between 0 and 90 degrees"), "{refused}");
}

#[test]
fn a_design_that_cant_run_says_why_and_changes_nothing() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let points = [[E + 50.0, N + 60.0, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];
    let mut content = level_design(&[toe], floor, roof);
    content["lines"][0]["profile"] = json!("Endwall");
    let design = created(&s.run("face.save_design", json!({ "name": "Pit design", "content": content })).unwrap())[0];
    let error = s.run("face.project", json!({ "design": design })).unwrap_err();
    assert!(error.to_string().contains("profile that isn't there: Endwall"), "{error}");
    assert_eq!(on_layer(&s, "faces"), (0, 0));
}

#[test]
fn a_design_saves_and_opens_with_the_project() {
    let mut s = Session::with_builtins();
    let (floor, roof) = (level_surface(&mut s, "floor", 100.0), level_surface(&mut s, "roof", 120.0));
    let points = [[E + 50.0, N + 60.0, 0.0], [E + 250.0, N + 60.0, 0.0]];
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];
    let design = created(&s.run("face.save_design", json!({ "name": "Pit design", "content": level_design(&[toe], floor, roof) })).unwrap())[0];
    let dir = std::env::temp_dir().join(format!("zd-face-{}-{}.zdproj", std::process::id(), zd_core::RunId::new()));
    s.save(&dir).unwrap();
    let mut opened = Session::open(&dir).unwrap();
    let body = |s: &Session| match &s.project().object(design).unwrap().body {
        Body::FaceDesign(d) => (**d).clone(),
        _ => panic!("a design comes back a design"),
    };
    assert_eq!(body(&opened), body(&s));
    opened.run("face.project", json!({ "design": design })).unwrap();
    assert_eq!(on_layer(&opened, "faces").1, 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A string running along strike through the middle, bending gently in plan.
fn along_strike(middle: [f64; 2], strike: [f64; 2], half_length: f64) -> Polyline {
    let across = [-strike[1], strike[0]];
    let vertices = (-20..=20)
        .map(|k| {
            let along = half_length * k as f64 / 20.0;
            let bend = 40.0 * (along / 300.0).sin();
            [middle[0] + along * strike[0] + bend * across[0], middle[1] + along * strike[1] + bend * across[1], 0.0]
        })
        .collect();
    Polyline { vertices, closed: false }
}

fn turns_over(line: &Polyline, degrees: f64) -> usize {
    let bearing = |a: &[f64; 3], b: &[f64; 3]| (b[1] - a[1]).atan2(b[0] - a[0]);
    line.vertices
        .windows(3)
        .filter(|w| {
            let turn = (bearing(&w[1], &w[2]) - bearing(&w[0], &w[1])).to_degrees().rem_euclid(360.0);
            turn.min(360.0 - turn) > degrees
        })
        .count()
}

fn report(name: &str, toe: &Polyline, target: &TriMesh, side: OffsetSide) -> Option<WallStep> {
    let start = Instant::now();
    let step = project_wall(toe, side, 70.0, target);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    match step {
        Ok(step) => {
            let v = step.wall.validity();
            let crossing_pairs = find_crossings(&step.wall, COINCIDENT).pairs;
            println!(
                "{name} {side:?}: {} target triangles, {} toe vertices, {ms:.1} ms, {} walls, {} squeezed out, {} crest vertices, {} wall triangles, {} boundary edges, {} non-manifold, {crossing_pairs} crossing pairs, {} no-height stretches, crest turns over 30 deg: {}",
                target.triangles.len(),
                toe.vertices.len(),
                step.log.walls,
                step.log.squeezed_out,
                step.crest.vertices.len(),
                step.wall.triangles.len(),
                v.boundary_edges,
                v.non_manifold_edges,
                step.log.no_height.len(),
                turns_over(&step.crest, 30.0),
            );
            Some(step)
        }
        Err(e) => {
            println!("{name} {side:?}: refused after {ms:.1} ms: {e}");
            None
        }
    }
}

/// Heights of a mesh along a line in plan, by a section through it.
fn height_on(mesh: &TriMesh, plane: &SectionPlane, p: [f64; 3]) -> Option<f64> {
    let along = plane.along(p);
    slice_mesh(mesh, plane).iter().flat_map(|l| l.points.windows(2)).find_map(|w| {
        let (a, b) = (plane.along(w[0]), plane.along(w[1]));
        ((a - along) * (b - along) <= 0.0 && a != b).then(|| w[0][2] + (along - a) / (b - a) * (w[1][2] - w[0][2]))
    })
}

/// What a section square to each stretch of a wall reads: the batter, and how far each end sits off the
/// grid it should be on. Returns the lowest and highest batter and the furthest end, for the sections
/// that cut one wall line from toe to crest.
fn read_in_section(plan: &Polyline, step: &WallStep, from: &TriMesh, to: &TriMesh, side: OffsetSide) -> (usize, f64, f64, f64) {
    let (mut count, mut low, mut high, mut off) = (0, f64::MAX, f64::MIN, 0.0f64);
    let sign = if side == OffsetSide::Left { 1.0 } else { -1.0 };
    for w in plan.vertices.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let length = dx.hypot(dy);
        let middle = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0];
        let out = [-dy / length * sign, dx / length * sign];
        let plane = SectionPlane::through(middle, [middle[0] + out[0], middle[1] + out[1]]).unwrap();
        let lines = slice_mesh(&step.wall, &plane);
        let [wall_line] = lines.as_slice() else { continue };
        let (Some(first), Some(last)) = (wall_line.points.first(), wall_line.points.last()) else { continue };
        let (toe, crest) = if first[2] < last[2] { (*first, *last) } else { (*last, *first) };
        let run = (plane.along(crest) - plane.along(toe)).abs();
        let batter = (crest[2] - toe[2]).atan2(run).to_degrees();
        let (Some(floor), Some(roof)) = (height_on(from, &plane, toe), height_on(to, &plane, crest)) else { continue };
        (count, low, high, off) = (count + 1, low.min(batter), high.max(batter), off.max((floor - toe[2]).abs()).max((roof - crest[2]).abs()));
    }
    (count, low, high, off)
}

/// One string on one grid, to look into a run from the app. `ZD_WALL_STRING` is `x,y,z;x,y,z`,
/// `ZD_WALL_TARGET` a Vulcan grid file and `ZD_WALL_RUNS` is `side,batter,berm,tolerance;...`. Writes each
/// run's lines to `ZD_WALL_OUT` as CSV, which stays local with the data.
#[test]
#[ignore = "needs ZD_WALL_STRING, ZD_WALL_TARGET and ZD_WALL_RUNS"]
fn one_string_on_one_real_grid() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is needed"));
    let numbers = |s: &str| s.split(',').map(|n| n.trim().parse::<f64>().unwrap()).collect::<Vec<_>>();
    let vertices = var("ZD_WALL_STRING").split(';').map(|p| <[f64; 3]>::try_from(numbers(p)).unwrap()).collect();
    let toe = Polyline { vertices, closed: false };
    let grid = zd_io::read_vulcan_grid(Path::new(&var("ZD_WALL_TARGET"))).unwrap().grid;
    let target = triangulate_grid(&grid);
    let out = var("ZD_WALL_OUT");
    let write = |name: String, line: &Polyline| {
        let rows: String = line.vertices.iter().map(|p| format!("{},{},{}\n", p[0], p[1], p[2])).collect();
        std::fs::write(Path::new(&out).join(name), rows).unwrap();
    };
    write("string.csv".into(), &toe);
    for (k, run) in var("ZD_WALL_RUNS").split(';').enumerate() {
        let (word, rest) = run.split_once(',').unwrap();
        let side = if word == "left" { OffsetSide::Left } else { OffsetSide::Right };
        let [batter, berm, tolerance] = <[f64; 3]>::try_from(numbers(rest)).unwrap();
        println!("--- run {k}: {side:?}, batter {batter}, berm {berm}, tolerance {tolerance}");
        let start = Instant::now();
        let step = match project_wall(&toe, side, batter, &target) {
            Ok(step) => step,
            Err(e) => {
                println!("refused after {:.1} ms: {e}", start.elapsed().as_secs_f64() * 1e3);
                continue;
            }
        };
        println!("{:.1} ms, {:?}", start.elapsed().as_secs_f64() * 1e3, step.log);
        let (toes, log) = next_toe(&step.crest, berm, side, tolerance, &target).unwrap();
        println!("crest {} vertices, length {:.1} m in plan; next toe {log:?}, {} piece(s), {:.1} m in plan", step.crest.vertices.len(), step.crest.plan_length(), toes.len(), toes.iter().map(Polyline::plan_length).sum::<f64>());
        write(format!("run{k}-crest.csv"), &step.crest);
        for (p, piece) in toes.iter().enumerate() {
            write(format!("run{k}-next{p}.csv"), piece);
        }
    }
}

/// Every mix of side, batter, berm and tolerance on one string, through a second step to
/// `ZD_WALL_SECOND`, printing only what fails. The string and target are as in the test above.
#[test]
#[ignore = "needs ZD_WALL_STRING, ZD_WALL_TARGET and ZD_WALL_SECOND"]
fn sweep_one_string_on_real_grids() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is needed"));
    let numbers = |s: &str| s.split(',').map(|n| n.trim().parse::<f64>().unwrap()).collect::<Vec<_>>();
    let vertices = var("ZD_WALL_STRING").split(';').map(|p| <[f64; 3]>::try_from(numbers(p)).unwrap()).collect();
    let toe = Polyline { vertices, closed: false };
    let mesh = |name: &str| triangulate_grid(&zd_io::read_vulcan_grid(Path::new(&var(name))).unwrap().grid);
    let (first, second) = (mesh("ZD_WALL_TARGET"), mesh("ZD_WALL_SECOND"));
    let (mut steps, mut refused) = (0, std::collections::BTreeMap::<String, (usize, String)>::new());
    let mut note = |reason: String, case: String| {
        // The place in a message varies, so messages count by their words.
        let key: String = reason.split(" at ").next().unwrap().split(" near ").next().unwrap().to_string();
        let entry = refused.entry(key).or_insert((0, format!("{case}: {reason}")));
        entry.0 += 1;
    };
    for side in [OffsetSide::Left, OffsetSide::Right] {
        for batter in (60..=176).map(|k| f64::from(k) * 0.5) {
            let case = format!("{side:?} {batter}");
            steps += 1;
            let step = match project_wall(&toe, side, batter, &first) {
                Ok(step) => step,
                Err(e) => {
                    note(e.to_string(), case);
                    continue;
                }
            };
            for tolerance in [0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 30.0] {
                for berm in [0.0, 3.0, 5.0, 10.0, 20.0, 30.0, 50.0] {
                    let case = format!("{case}, tolerance {tolerance}, berm {berm}");
                    match next_toe(&step.crest, berm, side, tolerance, &first) {
                        Err(e) => note(e.to_string(), case),
                        Ok((toes, _)) => {
                            for piece in &toes {
                                steps += 1;
                                if let Err(e) = project_wall(piece, side, batter, &second) {
                                    note(e.to_string(), case.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!("{steps} steps");
    for (key, (count, first)) in &refused {
        println!("{count} x {key}\n    first: {first}");
    }
}

/// The whole face on real data through the commands, as the panel runs it. `ZD_WALL_STRING` is
/// `x,y,z;x,y,z`, `ZD_FACE_GRIDS` lists grids bottom first with `;`, and `ZD_FACE_TOPO` is a topography.
#[test]
#[ignore = "needs ZD_WALL_STRING, ZD_FACE_GRIDS and ZD_FACE_TOPO"]
fn a_whole_face_on_real_grids() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is needed"));
    let mut s = Session::with_builtins();
    let mut import = |path: &str| created(&s.run("surface.import", json!({ "path": path, "layer": "surfaces" })).unwrap())[0];
    let grids: Vec<ObjectId> = var("ZD_FACE_GRIDS").split(';').map(&mut import).collect();
    let topo = import(&var("ZD_FACE_TOPO"));
    let numbers = |p: &str| p.split(',').map(|n| n.trim().parse::<f64>().unwrap()).collect::<Vec<_>>();
    let points: Vec<Vec<f64>> = var("ZD_WALL_STRING").split(';').map(numbers).collect();
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];
    for side in ["right", "left"] {
        let mut up = Vec::new();
        for grid in &grids[1..] {
            up.push(json!({ "step": "project", "to": { "kind": "surface", "id": grid }, "batter": "HW batter", "condition": 0.1 }));
            up.push(json!({ "step": "berm", "width": "HW berm", "condition": 0.1 }));
        }
        up.push(json!({ "step": "project", "to": { "kind": "surface", "id": topo }, "batter": "Upper batter", "condition": 0.1 }));
        up.push(json!({ "step": "project", "to": { "kind": "rl", "rl": 500.0 }, "batter": "Upper batter", "condition": 0.1 }));
        let content = json!({
            "layer": format!("faces {side}"),
            "lines": [
                { "string": toe, "profile": "Up", "side": side, "run_on": [100.0, 100.0] },
                { "string": toe, "profile": "Down", "side": side, "run_on": [100.0, 100.0] },
            ],
            "profiles": [
                { "name": "Up", "direction": "up", "register": grids[0], "steps": up },
                { "name": "Down", "direction": "down", "register": grids[0], "steps": [{ "step": "project", "to": { "kind": "rl", "rl": -500.0 }, "batter": "HW batter", "condition": 0.1 }] },
            ],
            "rules": [
                { "name": "HW batter", "value": { "type": "fixed", "value": 70.0 } },
                { "name": "HW berm", "value": { "type": "fixed", "value": 10.0 } },
                { "name": "Upper batter", "value": { "type": "fixed", "value": 45.0 } },
            ],
        });
        let design = created(&s.run("face.save_design", json!({ "name": format!("design {side}"), "content": content })).unwrap())[0];
        let start = Instant::now();
        match s.run("face.project", json!({ "design": design })) {
            Ok(record) => {
                println!("{side}: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
                record.messages.iter().for_each(|m| println!("  {m}"));
                let (strings, faces) = on_layer(&s, &format!("faces {side}"));
                println!("  {strings} strings, {faces} faces");
                for name in ["toe Up face", "toe Down face"] {
                    let face = s.project().objects().find(|o| o.name == name && s.project().layer(o.layer).is_some_and(|l| l.name == format!("faces {side}"))).unwrap();
                    let Body::Surface(sheet) = &face.body else { panic!("a face is a surface") };
                    let v = sheet.validity();
                    let b = sheet.mesh().bounds().unwrap();
                    println!("  {name}: RL {:.3} to {:.3}, {} triangles, {} boundary loop(s), {v:?}", b.min[2], b.max[2], sheet.mesh().triangles.len(), zd_geom::boundary_loops(sheet.mesh()).unwrap().len());
                    assert!(v.non_self_intersecting == Some(true) && v.consistent_winding);
                }
            }
            Err(e) => println!("{side}: refused after {:.1} ms: {e}", start.elapsed().as_secs_f64() * 1e3),
        }
    }
}

/// Roads up a real face, through the commands. `ZD_WALL_STRING`, `ZD_FACE_GRIDS` and `ZD_FACE_TOPO` are as
/// above. Each road starts part way along the toe of a wall, in turn, each way along, on each side, and
/// runs to the top of the last wall. Then each is run again with its end held where that one finished.
#[test]
#[ignore = "needs ZD_WALL_STRING, ZD_FACE_GRIDS and ZD_FACE_TOPO"]
fn a_road_on_real_grids() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is needed"));
    let mut s = Session::with_builtins();
    let mut import = |path: &str| created(&s.run("surface.import", json!({ "path": path, "layer": "surfaces" })).unwrap())[0];
    let grids: Vec<ObjectId> = var("ZD_FACE_GRIDS").split(';').map(&mut import).collect();
    let topo = import(&var("ZD_FACE_TOPO"));
    let numbers = |p: &str| p.split(',').map(|n| n.trim().parse::<f64>().unwrap()).collect::<Vec<_>>();
    let points: Vec<Vec<f64>> = var("ZD_WALL_STRING").split(';').map(numbers).collect();
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points.clone() })).unwrap())[0];
    let wall = |to: serde_json::Value, batter: f64, on: bool| {
        let mut row = json!({ "step": "project", "to": to, "batter": batter, "condition": 0.1 });
        if on {
            row["ramp"] = json!("A");
        }
        row
    };
    let walls = grids.len();
    for side in ["right", "left"] {
        for (heading, at) in [("along", 0.3), ("back", 0.7)] {
            for from in 0..walls {
                let mut steps = Vec::new();
                for (k, grid) in grids[1..].iter().enumerate() {
                    steps.push(wall(json!({ "kind": "surface", "id": grid }), 70.0, k >= from));
                    steps.push(json!({ "step": "berm", "width": 10.0, "condition": 0.1, "min_bench": 5.0 }));
                }
                steps.push(wall(json!({ "kind": "surface", "id": topo }), 45.0, true));
                let (a, b) = (&points[0], &points[points.len() - 1]);
                let start = [a[0] + at * (b[0] - a[0]), a[1] + at * (b[1] - a[1])];
                let run = |s: &mut Session, label: &str, fixed: &str, point: [f64; 2]| {
                    let layer = format!("road {side} {heading} from wall {} {label}", from + 1);
                    let ramp = json!({ "name": "A", "string": toe, "fixed": fixed, "at": point, "heading": heading, "width": 30.0, "grade": 10.0 });
                    let content = json!({
                        "layer": layer,
                        "lines": [{ "string": toe, "profile": "Up", "side": side, "run_on": [300.0, 300.0] }],
                        "profiles": [{ "name": "Up", "direction": "up", "register": grids[0], "steps": steps, "ramps": [ramp] }],
                        "rules": [],
                    });
                    let design = created(&s.run("face.save_design", json!({ "name": layer, "content": content })).unwrap())[0];
                    let started = Instant::now();
                    match s.run("face.project", json!({ "design": design })) {
                        Ok(record) => {
                            let ms = started.elapsed().as_secs_f64() * 1e3;
                            let face = record.outputs.iter().filter_map(|o| s.project().object(o.id)).find(|o| o.name.ends_with("face")).unwrap();
                            let Body::Surface(sheet) = &face.body else { panic!("a face is a surface") };
                            let v = sheet.validity();
                            let loops = zd_geom::boundary_loops(sheet.mesh()).map(|l| l.len());
                            let road = record.messages.iter().find(|m| m.starts_with("Ramp A")).cloned().unwrap_or_default();
                            println!("{layer}: {ms:.0} ms, {} triangles, {loops:?} loop(s), crossing free {:?}: {road}", sheet.mesh().triangles.len(), v.non_self_intersecting);
                            let top = s.project().objects().find(|o| o.name == "toe Up road A pit edge" && s.project().layer(o.layer).is_some_and(|l| l.name == layer)).map(|o| line(s, o.id));
                            top.and_then(|edge| edge.vertices.last().copied())
                        }
                        Err(e) => {
                            println!("{layer}: refused after {:.0} ms: {e}", started.elapsed().as_secs_f64() * 1e3);
                            None
                        }
                    }
                };
                if let Some(top) = run(&mut s, "held at the start", "start", start) {
                    run(&mut s, "held at the end", "end", [top[0], top[1]]);
                }
            }
        }
    }
}

/// A wall row names its ramp, as the panel saves it.
fn ramped_content(string: ObjectId, floor: ObjectId, ramp: serde_json::Value) -> serde_json::Value {
    json!({
        "layer": "faces",
        "lines": [{ "string": string, "profile": "Wall", "side": "left", "run_on": [0.0, 0.0] }],
        "profiles": [{
            "name": "Wall", "direction": "up", "register": floor,
            "steps": [{ "step": "project", "to": { "kind": "rl", "rl": 120.0 }, "batter": 45.0, "ramp": "A" }],
            "ramps": [ramp],
        }],
        "rules": [],
    })
}

/// A wall 20 m high with a road 200 m long up it. Running it again rebuilds the lot from the same design, as
/// a road is part of how the wall is built.
#[test]
fn a_ramp_cuts_a_road_and_a_rerun_rebuilds_it() {
    let mut s = Session::with_builtins();
    let floor = level_surface(&mut s, "floor", 100.0);
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "strings", "points": [[E + 10.0, N + 30.0, 0.0], [E + 290.0, N + 30.0, 0.0]] })).unwrap())[0];
    let content = |grade: f64| ramped_content(toe, floor, json!({ "name": "A", "string": toe, "fixed": "start", "at": [E + 40.0, N + 30.0], "heading": "along", "width": 10.0, "grade": grade }));
    let design = created(&s.run("face.save_design", json!({ "name": "with a road", "content": content(10.0) })).unwrap())[0];
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert!(record.messages.iter().any(|m| m.starts_with("Ramp A: step 1, 200.0 m from RL 100.0 to 120.0")), "{:?}", record.messages);
    let edge = |s: &Session, name: &str| line(s, s.project().objects().find(|o| o.name == name).unwrap().id);
    let pit = edge(&s, "toe Wall road A pit edge");
    assert!((pit.plan_length() - 200.0).abs() < 1e-3, "{}", pit.plan_length());
    assert!(!edge(&s, "toe Wall road A wall edge").vertices.is_empty());
    let marked = s.project().objects().find(|o| o.name == "toe Wall road A pit edge").unwrap();
    assert_eq!(marked.attributes.get("road"), Some(&AttrValue::Text("A".into())));
    assert_eq!(marked.attributes.get("road_face"), Some(&AttrValue::Text("toe Wall face".into())));
    let face = s.project().objects().find(|o| o.name == "toe Wall face").unwrap();
    let Body::Surface(sheet) = &face.body else { panic!("a face is a surface") };
    assert_eq!(sheet.validity().non_self_intersecting, Some(true));

    // Steeper, the same height takes half the road.
    s.run("face.save_design", json!({ "design": design, "name": "with a road", "content": content(20.0) })).unwrap();
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert!(record.messages.iter().any(|m| m.starts_with("Ramp A: step 1, 100.0 m")), "{:?}", record.messages);
    assert_eq!(s.project().objects().filter(|o| o.name == "toe Wall road A pit edge").count(), 1, "the last run's road went with it");
    s.undo().unwrap();
    assert!((edge(&s, "toe Wall road A pit edge").plan_length() - 200.0).abs() < 1e-3, "a run undoes in one go");
}

/// With its end held, the run says where the road had to start to come out there.
#[test]
fn a_ramp_held_at_its_end_finds_its_start() {
    let mut s = Session::with_builtins();
    let floor = level_surface(&mut s, "floor", 100.0);
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "strings", "points": [[E + 10.0, N + 30.0, 0.0], [E + 290.0, N + 30.0, 0.0]] })).unwrap())[0];
    let content = ramped_content(toe, floor, json!({ "name": "A", "string": toe, "fixed": "end", "at": [E + 260.0, N + 50.0], "heading": "along", "width": 10.0, "grade": 10.0 }));
    let design = created(&s.run("face.save_design", json!({ "name": "held", "content": content })).unwrap())[0];
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert!(record.messages.iter().any(|m| m.starts_with("Ramp A: step 1, 200.0 m")), "{:?}", record.messages);
    let pit = line(&s, s.project().objects().find(|o| o.name == "toe Wall road A pit edge").unwrap().id);
    assert!((pit.vertices.last().unwrap()[0] - E - 260.0).abs() <= 0.05, "{:?}", pit.vertices.last());
}

/// A road across a berm through the commands, in the table as the panel saves it.
#[test]
fn a_ramp_runs_up_the_walls_that_name_it_across_a_berm() {
    let mut s = Session::with_builtins();
    let floor = level_surface(&mut s, "floor", 100.0);
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "strings", "points": [[E + 10.0, N + 30.0, 0.0], [E + 295.0, N + 30.0, 0.0]] })).unwrap())[0];
    let content = json!({
        "layer": "faces",
        "lines": [{ "string": toe, "profile": "Wall", "side": "left", "run_on": [0.0, 0.0] }],
        "profiles": [{
            "name": "Wall", "direction": "up", "register": floor,
            "steps": [
                { "step": "project", "to": { "kind": "rl", "rl": 110.0 }, "batter": 45.0, "ramp": "A" },
                { "step": "berm", "width": 5.0, "register": false },
                { "step": "project", "to": { "kind": "rl", "rl": 120.0 }, "batter": 45.0, "ramp": "A" },
            ],
            "ramps": [{ "name": "A", "string": toe, "fixed": "start", "at": [E + 30.0, N + 30.0], "heading": "along", "width": 10.0, "grade": 10.0 }],
        }],
        "rules": [],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "across a berm", "content": content })).unwrap())[0];
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert!(record.messages.iter().any(|m| m.starts_with("Ramp A: steps 1 to 3, 200.0 m from RL 100.0 to 120.0")), "{:?}", record.messages);
}

/// Too steep a climb for the string's length leaves the face standing and says why, by step and place.
#[test]
fn a_ramp_that_cant_reach_names_the_step_and_the_face_still_builds() {
    let mut s = Session::with_builtins();
    let floor = level_surface(&mut s, "floor", 100.0);
    let points = [[E + 10.0, N + 30.0, 0.0], [E + 100.0, N + 30.0, 0.0]];
    let string = created(&s.run("string.create", json!({ "name": "short", "layer": "strings", "points": points })).unwrap())[0];
    let content = ramped_content(string, floor, json!({ "name": "A", "string": string, "fixed": "start", "at": [E + 40.0, N + 30.0], "heading": "along", "width": 10.0, "grade": 10.0 }));
    let design = created(&s.run("face.save_design", json!({ "name": "too short", "content": content })).unwrap())[0];
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    let line = record.messages.iter().find(|m| m.starts_with("Ramp A:")).unwrap();
    assert!(line.contains("stopped on step 1 at"), "{line}");
    assert!(line.contains("runs out of wall at RL 106.0"), "60 m along at 10 %: {line}");
    assert!(s.project().objects().any(|o| o.name == "short Wall face"));
    assert!(!s.project().objects().any(|o| o.name.contains("road")));
}

/// A ramp goes on the string it names, wherever its point is, and the others run plain. A ramp whose string
/// isn't on a line says so, as a ramp that doesn't appear is the hardest thing to find.
#[test]
fn a_ramp_goes_on_the_string_it_names() {
    let mut s = Session::with_builtins();
    let floor = level_surface(&mut s, "floor", 100.0);
    let string = |s: &mut Session, name: &str, y: f64| created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": [[E + 10.0, N + y, 0.0], [E + 290.0, N + y, 0.0]] })).unwrap())[0];
    let south = string(&mut s, "south", 30.0);
    let north = string(&mut s, "north", 150.0);
    let layer = s.project().object(south).unwrap().layer;
    let content = |on: ObjectId| {
        let mut content = ramped_content(south, floor, json!({ "name": "A", "string": on, "fixed": "start", "at": [E + 40.0, N + 140.0], "heading": "along", "width": 10.0, "grade": 10.0 }));
        content["lines"] = json!([{ "layer": layer, "profile": "Wall", "side": "left", "run_on": [0.0, 0.0] }]);
        content
    };
    let design = created(&s.run("face.save_design", json!({ "name": "layer", "content": content(south) })).unwrap())[0];
    s.run("face.project", json!({ "design": design })).unwrap();
    assert!(s.project().objects().any(|o| o.name == "south Wall road A pit edge"), "the point's nearer north, but it names south");
    assert!(!s.project().objects().any(|o| o.name == "north Wall road A pit edge"));
    let _ = north;
    let elsewhere = string(&mut s, "elsewhere", 200.0);
    s.run("object.move", json!({ "objects": [elsewhere], "layer": "other" })).unwrap();
    s.run("face.save_design", json!({ "design": design, "name": "layer", "content": content(elsewhere) })).unwrap();
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    assert!(record.messages.iter().any(|m| m == "Ramp A: its string isn't on a line using Wall"), "{:?}", record.messages);
}

/// Every real solid cut by real grids. `ZD_CUT_SOLIDS` is a zip or folder of solids and `ZD_CUT_GRIDS`
/// lists grids with `;`. What's kept and any solid left uncut must add up to what went in, to one part in
/// a million, and every solid must lie under the grid.
#[test]
#[ignore = "needs ZD_CUT_SOLIDS and ZD_CUT_GRIDS"]
fn a_cut_by_real_grids() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is needed"));
    let mut s = Session::with_builtins();
    let start = Instant::now();
    s.run("solid.import_many", json!({ "paths": [var("ZD_CUT_SOLIDS")], "layer": "solids" })).unwrap();
    let volume_of = |o: &zd_core::Object| if let Body::Solid(solid) = &o.body { Some(solid.volume()) } else { None };
    let solids: Vec<ObjectId> = s.project().objects().filter(|o| matches!(o.body, Body::Solid(_))).map(|o| o.id).collect();
    let total: f64 = s.project().objects().filter_map(|o| volume_of(o)).sum();
    println!("{} solids, {total:.1} m3, imported in {:.1} s", solids.len(), start.elapsed().as_secs_f64());
    for (k, path) in var("ZD_CUT_GRIDS").split(';').enumerate() {
        let imported = s.run("surface.import", json!({ "path": path, "layer": "grids" })).unwrap();
        imported.messages.iter().take(2).for_each(|m| println!("  {m}"));
        let layer = format!("cut {k}");
        let start = Instant::now();
        let record = s.run("solid.cut_by_surface", json!({ "solids": solids, "surface": created(&imported)[0], "layer": layer })).unwrap();
        println!("grid {k}: {:.1} s", start.elapsed().as_secs_f64());
        let on_layer = s.project().layer_named(&layer).map(|l| l.id);
        let parts: Vec<_> = s.project().objects().filter(|o| Some(o.layer) == on_layer).collect();
        let kept: f64 = parts.iter().filter_map(|o| volume_of(o)).sum();
        let cut: BTreeSet<&str> = parts.iter().filter_map(|o| o.name.rsplit_once(" below ").or(o.name.rsplit_once(" above ")).map(|(solid, _)| solid)).collect();
        let uncut: Vec<_> = s.project().objects().filter(|o| solids.contains(&o.id) && !cut.contains(o.name.as_str())).collect();
        let left: f64 = uncut.iter().filter_map(|o| volume_of(o)).sum();
        println!("  {} parts, {kept:.1} m3, and {} solids left uncut, {left:.1} m3, of {total:.1} m3: difference {:.3}", parts.len(), uncut.len(), kept + left - total);
        assert!((kept + left - total).abs() <= 1e-6 * total, "{kept} and {left} of {total}");
        assert!(!record.messages.iter().any(|m| m.contains("does not cover")), "{:?}", record.messages);
    }
}

#[test]
#[ignore = "needs ZD_WALL_GRIDS"]
fn a_wall_steps_between_real_grids() {
    let files = std::env::var("ZD_WALL_GRIDS").expect("ZD_WALL_GRIDS lists the grids, bottom first");
    let grids: Vec<Grid> = files.split(';').map(|f| zd_io::read_vulcan_grid(Path::new(f.trim())).unwrap().grid).collect();
    for (k, g) in grids.iter().enumerate() {
        let live = g.z.iter().filter(|z| !z.is_nan()).count();
        let (low, high) = g.z.iter().filter(|z| !z.is_nan()).fold((f64::MAX, f64::MIN), |(a, b), &z| (a.min(z), b.max(z)));
        println!("grid {k}: {} x {} at {} x {} m, {live} live of {}, RL range {:.0} m", g.cols, g.rows, g.cell_size[0], g.cell_size[1], g.z.len(), high - low);
    }
    let meshes: Vec<TriMesh> = grids.iter().map(triangulate_grid).collect();
    let (middle, strike) = middle_and_strike(&grids[0]);
    for half in [300.0, 1000.0, 2500.0] {
        println!("--- string {} m long", 2.0 * half);
        let plan = along_strike(middle, strike, half);
        let (toe, log) = drape_string(&plan, &meshes[0], true).unwrap();
        println!("toe: {} vertices, {} off the grid", toe.vertices.len(), log.off_surface);
        for side in [OffsetSide::Left, OffsetSide::Right] {
            for (tolerance, berm) in [(0.0, 0.0), (0.1, 10.0), (0.25, 10.0), (0.5, 20.0)] {
                println!("- {side:?}, tolerance {tolerance} m, berm {berm} m");
                let mut from = toe.clone();
                let mut shape = plan.clone();
                for (k, target) in meshes.iter().enumerate().skip(1) {
                    let Some(step) = report(&format!("step {k}"), &from, target, side) else { break };
                    let (count, low, high, off) = read_in_section(&shape, &step, &meshes[k - 1], target, side);
                    println!("   in section: {count} sections read {low:.4} to {high:.4} degrees, ends within {:.3} mm of their grids", off * 1e3);
                    if count > 0 && tolerance > 0.0 {
                        assert!((low - 70.0).abs() < 1e-6 && (high - 70.0).abs() < 1e-6 && off < 1e-3, "{low} {high} {off}");
                    }
                    let (toes, log) = next_toe(&step.crest, berm, side, tolerance, target).unwrap();
                    println!("   next toe: {log:?}, {} piece(s)", toes.len());
                    from = toes.into_iter().max_by_key(|t| t.vertices.len()).unwrap();
                    shape = zd_geom::filter_string(&Polyline { vertices: from.vertices.iter().map(|v| [v[0], v[1], 0.0]).collect(), closed: false }, 1e-6).unwrap();
                }
            }
        }
    }
}
