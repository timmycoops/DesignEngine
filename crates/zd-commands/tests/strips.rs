//! One face a strip, and strip solids, through the registry as the app runs them (decision 0049): a toe offset 14
//! times, numbered by a line across, projected up and down to RLs, joined by strip number, and a pit sliced by them.

use std::time::Instant;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn surfaces_on(s: &Session, layer: &str) -> Vec<ObjectId> {
    let layer = s.project().layer_named(layer).expect("the layer").id;
    s.project().objects().filter(|o| o.layer == layer && matches!(o.body, Body::Surface(_))).map(|o| o.id).collect()
}

fn volume(s: &Session, id: ObjectId) -> f64 {
    match &s.project().object(id).unwrap().body {
        Body::Solid(solid) => solid.volume(),
        _ => panic!("a solid"),
    }
}

/// Volume tolerance: 1 m³ in the 50,000,000 m³ pit.
fn assert_volume(actual: f64, expected: f64, what: &str) {
    assert!((actual - expected).abs() < 1.0, "{what}: {actual:.3} m³ against {expected:.3} m³");
}

/// The pit: 1000 m east, 500 m north, RL 0 to 100.
fn pit(s: &mut Session) -> ObjectId {
    let ([x0, y0, z0], [x1, y1, z1]) = ([E, N, 0.0], [E + 1000.0, N + 500.0, 100.0]);
    let vertices = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
    let triangles = [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    created(&s.run("solid.from_mesh", json!({ "name": "PIT", "layer": "pit", "vertices": vertices, "triangles": triangles })).unwrap())[0]
}

/// Fifteen toes on RL 0, 50 m apart from 100 m east, running north past the pit, numbered 1 to 15 from the west by a
/// line drawn across; each projected up at 45° to RL 150 and down at 45° to RL -50, faces on `faces`.
fn fifteen_strips(s: &mut Session) -> Vec<ObjectId> {
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "toes", "points": [[E + 100.0, N - 50.0, 0.0], [E + 100.0, N + 550.0, 0.0]] })).unwrap())[0];
    let mut toes = vec![toe];
    toes.extend(created(&s.run("string.offset", json!({ "string": toe, "distance": 50.0, "side": "right", "times": 14, "layer": "toes" })).unwrap()));
    s.run("string.number_across", json!({ "strings": toes, "from": [E, N + 250.0, 0.0], "to": [E + 1000.0, N + 250.0, 0.0], "start": 1.0 })).unwrap();
    let lines: Vec<_> = toes.iter().flat_map(|t| ["Up", "Down"].map(|profile| json!({ "string": t, "profile": profile, "side": "right", "run_on": [60.0, 60.0] }))).collect();
    let content = json!({
        "layer": "faces",
        "lines": lines,
        "profiles": [
            { "name": "Up", "direction": "up", "register": { "kind": "rl", "rl": 0.0 }, "steps": [{ "step": "project", "to": { "kind": "rl", "rl": 150.0 }, "batter": 45.0 }] },
            { "name": "Down", "direction": "down", "register": { "kind": "rl", "rl": 0.0 }, "steps": [{ "step": "project", "to": { "kind": "rl", "rl": -50.0 }, "batter": 45.0 }] },
        ],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "strips", "content": content })).unwrap())[0];
    s.run("face.project", json!({ "design": design })).unwrap();
    toes
}

#[test]
fn a_toe_offset_and_numbered_slices_the_pit_into_a_solid_a_strip() {
    let mut s = Session::with_builtins();
    let pit = pit(&mut s);
    fifteen_strips(&mut s);
    let faces = surfaces_on(&s, "faces");
    assert_eq!(faces.len(), 30, "an up and a down face for each toe");
    for &face in &faces {
        assert!(matches!(s.project().object(face).unwrap().attributes.get("strip"), Some(AttrValue::Number(_))), "a face keeps its toe's strip number");
    }

    // Joined: one face a strip, each one piece with one edge all round.
    let start = Instant::now();
    let record = s.run("face.join", json!({ "faces": faces, "layer": "strip faces" })).unwrap();
    println!("face.join: {:.0} ms", start.elapsed().as_secs_f64() * 1e3);
    record.messages.iter().for_each(|m| println!("  {m}"));
    let joined = created(&record);
    assert_eq!(joined.len(), 15);
    for (k, &face) in joined.iter().enumerate() {
        let object = s.project().object(face).unwrap();
        assert_eq!(object.name, format!("strip {} face", k + 1));
        assert_eq!(object.attributes.get("strip"), Some(&AttrValue::Number((k + 1) as f64)));
        let Body::Surface(surface) = &object.body else { panic!("a face is a surface") };
        assert_eq!(zd_geom::boundary_loops(surface.mesh()).unwrap().len(), 1, "strip {} face is one piece", k + 1);
    }

    // Sliced by the faces as they came out of the run: the slice joins each strip's up and down itself.
    let start = Instant::now();
    let record = s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": faces, "layer": "strips" })).unwrap();
    println!("solid.slice_by_faces: {:.0} ms", start.elapsed().as_secs_f64() * 1e3);
    record.messages.iter().for_each(|m| println!("  {m}"));
    let strips = created(&record);
    assert_eq!(strips.len(), 16);
    let names: Vec<String> = strips.iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    assert_eq!(names[0], "PIT strip 1");
    assert_eq!(names[14], "PIT strip 15");
    assert_eq!(names[15], "PIT past strip 15");
    // Strip 1 is in front of the first toe, at 100 m east: 500 x the integral of (100 + z) over RL 0 to 100.
    assert_volume(volume(&s, strips[0]), 7_500_000.0, "strip 1");
    for (k, &strip) in strips.iter().enumerate().take(15).skip(1) {
        assert_volume(volume(&s, strip), 2_500_000.0, &format!("strip {}", k + 1));
        assert_eq!(s.project().object(strip).unwrap().attributes.get("strip"), Some(&AttrValue::Number((k + 1) as f64)));
    }
    assert_volume(volume(&s, strips[15]), 7_500_000.0, "past strip 15");
    assert_eq!(s.project().object(strips[15]).unwrap().attributes.get("past_strip"), Some(&AttrValue::Number(15.0)));
    let total: f64 = strips.iter().map(|&id| volume(&s, id)).sum();
    assert_volume(total, volume(&s, pit), "the strips add up to the pit");

    // The joined faces slice it the same, and one undo takes the slice back.
    let again = created(&s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": joined, "layer": "strips again" })).unwrap());
    for (a, b) in strips.iter().zip(&again) {
        assert_volume(volume(&s, *b), volume(&s, *a), "joined first or not");
    }
    s.undo().unwrap();
    assert!(again.iter().all(|&id| s.project().object(id).is_none()));
    assert!(strips.iter().all(|&id| s.project().object(id).is_some()));
}

#[test]
fn a_face_with_no_strip_number_is_refused_by_name() {
    let mut s = Session::with_builtins();
    let pit = pit(&mut s);
    fifteen_strips(&mut s);
    let mut faces = surfaces_on(&s, "faces");
    let loose = created(
        &s.run(
            "surface.from_mesh",
            json!({ "name": "loose face", "layer": "faces", "vertices": [[E + 500.0, N - 50.0, 0.0], [E + 600.0, N - 50.0, 100.0], [E + 600.0, N + 550.0, 100.0]], "triangles": [[0, 1, 2]] }),
        )
        .unwrap(),
    )[0];
    faces.push(loose);
    let error = s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": faces, "layer": "strips" })).unwrap_err().to_string();
    assert!(error.contains("'loose face' has no strip number"), "{error}");
    let error = s.run("face.join", json!({ "faces": [loose], "layer": "strip faces" })).unwrap_err().to_string();
    assert!(error.contains("'loose face' has no strip number"), "{error}");
}

#[test]
fn a_strip_whose_face_stops_short_is_refused_with_the_run_on_it_needs() {
    let mut s = Session::with_builtins();
    let pit = pit(&mut s);
    // One toe that starts 100 m into the pit from its south side.
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "toes", "points": [[E + 300.0, N + 100.0, 0.0], [E + 300.0, N + 550.0, 0.0]] })).unwrap())[0];
    s.run("string.number_across", json!({ "strings": [toe], "from": [E, N + 250.0, 0.0], "to": [E + 1000.0, N + 250.0, 0.0], "start": 7.0 })).unwrap();
    let content = json!({
        "layer": "faces",
        "lines": [{ "string": toe, "profile": "Up", "side": "right" }],
        "profiles": [{ "name": "Up", "direction": "up", "register": { "kind": "rl", "rl": -10.0 }, "steps": [{ "step": "project", "to": { "kind": "rl", "rl": 150.0 }, "batter": 45.0 }] }],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "short", "content": content })).unwrap())[0];
    s.run("face.project", json!({ "design": design })).unwrap();
    let error = s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": surfaces_on(&s, "faces"), "layer": "strips" })).unwrap_err().to_string();
    assert!(error.starts_with("strip 7 face"), "{error}");
    assert!(error.contains("stops short of the solid at its end") && error.contains("m more run-on"), "{error}");
}
