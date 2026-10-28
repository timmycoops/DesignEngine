//! The command layer end to end, through the registry with JSON parameters, as the app bridge
//! and Python will call it.

use std::sync::Arc;

use serde_json::{Value, json};
use zd_commands::{CommandError, LogEntry, RunRecord, RunStatus, Session};
use zd_core::{Body, ChangeKind, Grid, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn vertices(session: &Session, id: ObjectId) -> Vec<[f64; 3]> {
    match &session.project().object(id).expect("live object").body {
        Body::String(line) => line.vertices.clone(),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

/// Axis-aligned box with outward winding, as inline mesh parameters.
fn box_mesh(name: &str, min: [f64; 3], max: [f64; 3]) -> Value {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    json!({
        "name": name,
        "layer": "seams",
        "vertices": [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        "triangles": [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
    })
}

/// A flat design surface at elevation `z`, overhanging the box by 5 m on every side.
fn plane(z: f64) -> Value {
    json!({
        "name": "pit floor",
        "layer": "design",
        "vertices": [[E - 5.0, N - 5.0, z], [E + 15.0, N - 5.0, z], [E + 15.0, N + 15.0, z], [E - 5.0, N + 15.0, z]],
        "triangles": [[0, 1, 2], [0, 2, 3]],
    })
}

#[test]
fn registry_lists_every_builtin_with_a_spec() {
    let session = Session::with_builtins();
    let names: Vec<&str> = session.registry().specs().iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        [
            "layer.create",
            "object.delete",
            "object.rename",
            "solid.cut_by_surface",
            "solid.from_mesh",
            "string.create",
            "string.delete_vertex",
            "string.insert_vertex",
            "string.move_vertex",
            "surface.from_mesh",
            "surface.import",
        ]
    );
    let specs = serde_json::to_value(session.registry().specs()).unwrap();
    assert_eq!(specs[3]["inputs"][0]["type"], json!({ "kind": "object", "object_kind": "solid" }));
}

#[test]
fn string_edits_are_exact_and_keep_every_revision() {
    let mut s = Session::with_builtins();
    let toe = [[E, N, 120.25], [E + 50.0, N, 121.5], [E + 100.0, N + 10.0, 122.75]];
    let record = s.run("string.create", json!({ "name": "toe", "layer": "design", "points": toe })).unwrap();
    let id = created(&record)[0];
    assert_eq!(record.layers_created, ["design"]);

    s.run("string.insert_vertex", json!({ "string": id, "index": 1, "point": [E + 25.0, N - 2.0, 121.0] })).unwrap();
    s.run("string.move_vertex", json!({ "string": id, "index": 3, "to": [E + 100.5, N + 10.5, 123.0] })).unwrap();
    s.run("string.delete_vertex", json!({ "string": id, "index": 2 })).unwrap();
    assert_eq!(vertices(&s, id), [[E, N, 120.25], [E + 25.0, N - 2.0, 121.0], [E + 100.5, N + 10.5, 123.0]]);

    // Commands never modify inputs: revision 1 still holds the string as created.
    let first = s.project().revision(id, 1).unwrap();
    match &first.body {
        Body::String(line) => assert_eq!(line.vertices, toe),
        _ => unreachable!(),
    }
    assert_eq!(s.project().object(id).unwrap().revision, 4);
}

#[test]
fn failed_runs_change_nothing_and_are_logged() {
    let mut s = Session::with_builtins();
    let id = created(&s.run("string.create", json!({ "name": "a", "layer": "L", "points": [[0, 0, 0], [1, 0, 0]] })).unwrap())[0];

    assert!(matches!(s.run("string.explode", json!({})), Err(CommandError::UnknownCommand(_))));
    assert!(matches!(
        s.run("string.move_vertex", json!({ "string": id, "index": 0, "too": [1, 1, 1] })),
        Err(CommandError::BadParams(_))
    ));
    assert!(matches!(s.run("string.move_vertex", json!({ "string": id, "index": 9, "to": [1, 1, 1] })), Err(CommandError::Invalid(_))));
    assert!(matches!(s.run("string.delete_vertex", json!({ "string": id, "index": 0 })), Err(CommandError::Invalid(_))));
    assert!(matches!(s.run("solid.cut_by_surface", json!({ "solid": id, "surface": id, "layer": "x" })), Err(CommandError::WrongKind { .. })));

    assert_eq!(s.project().object(id).unwrap().revision, 1, "nothing changed");
    let failed = s
        .log()
        .iter()
        .filter(|e| matches!(e, LogEntry::Run(r) if matches!(r.status, RunStatus::Failed(_))))
        .count();
    assert_eq!(failed, 5);
    assert_eq!(s.log().len(), 6);
}

#[test]
fn cutting_a_box_by_a_plane_through_the_registry() {
    let mut s = Session::with_builtins();
    // 10 m cube at mine-grid coordinates, floor 100 m, roof 110 m: 1000 m3.
    let solid = created(&s.run("solid.from_mesh", box_mesh("seam A", [E, N, 100.0], [E + 10.0, N + 10.0, 110.0])).unwrap())[0];
    let surface = created(&s.run("surface.from_mesh", plane(104.0)).unwrap())[0];
    let before = s.project().object(solid).unwrap().clone();

    let record = s.run("solid.cut_by_surface", json!({ "solid": solid, "surface": surface, "layer": "cuts" })).unwrap();
    let parts = created(&record);
    assert_eq!(parts.len(), 2);
    assert_eq!(record.layers_created, ["cuts"]);
    assert_eq!(record.inputs.len(), 2, "run record names the solid and surface revisions it read");

    let volume = |id: ObjectId| match &s.project().object(id).unwrap().body {
        Body::Solid(solid) => solid.volume(),
        _ => unreachable!(),
    };
    // Tolerance: 1e-6 m3 on a 1000 m3 solid.
    assert!((volume(parts[0]) - 400.0).abs() < 1e-6, "below: {}", volume(parts[0]));
    assert!((volume(parts[1]) - 600.0).abs() < 1e-6, "above: {}", volume(parts[1]));
    assert_eq!(s.project().object(parts[0]).unwrap().name, "seam A below pit floor");

    // The input solid is untouched: same revision, same shared mesh.
    let after = s.project().object(solid).unwrap();
    assert!(Arc::ptr_eq(&before, after));
    assert!(record.messages.iter().any(|m| m.contains("triangles in")), "{:?}", record.messages);
}

#[test]
fn undo_and_redo_step_whole_runs_and_are_logged() {
    let mut s = Session::with_builtins();
    let solid = created(&s.run("solid.from_mesh", box_mesh("seam A", [E, N, 100.0], [E + 10.0, N + 10.0, 110.0])).unwrap())[0];
    let surface = created(&s.run("surface.from_mesh", plane(104.0)).unwrap())[0];
    let cut = s.run("solid.cut_by_surface", json!({ "solid": solid, "surface": surface, "layer": "cuts" })).unwrap();
    let parts = created(&cut);

    assert_eq!(s.undo(), Some(cut.run));
    assert!(parts.iter().all(|&p| s.project().object(p).is_none()));
    assert!(s.project().layer_named("cuts").is_none());
    assert_eq!(s.redo(), Some(cut.run));
    assert!(parts.iter().all(|&p| s.project().object(p).is_some()), "redo brings back the same objects");
    assert!(matches!(s.log().last(), Some(LogEntry::Redo { run, .. }) if *run == cut.run));
}

#[test]
fn the_run_log_round_trips_through_json() {
    let mut s = Session::with_builtins();
    let id = created(&s.run("string.create", json!({ "name": "crest", "layer": "design", "points": [[E, N, 1.5], [E + 1.0, N, 2.5]] })).unwrap())[0];
    s.run("object.rename", json!({ "object": id, "name": "crest 1" })).unwrap();
    let _ = s.run("string.move_vertex", json!({ "string": id, "index": 5, "to": [0, 0, 0] }));
    s.undo();
    s.redo();
    s.run("object.delete", json!({ "object": id })).unwrap();

    let text = serde_json::to_string_pretty(s.log()).unwrap();
    let back: Vec<LogEntry> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, s.log());
    assert!(text.contains("\"event\": \"undo\""));
}

#[test]
fn json_gives_back_every_f64_bit_for_bit() {
    // Run logs replay coordinates, so JSON must not round them (serde_json's float_roundtrip).
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let values: Vec<f64> = (0..100_000)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let unit = (seed >> 11) as f64 / (1u64 << 53) as f64;
            match i % 3 {
                0 => E + unit * 10_000.0,
                1 => N + unit * 10_000.0,
                _ => (unit - 0.5) * 1_000.0,
            }
        })
        .collect();
    let back: Vec<f64> = serde_json::from_str(&serde_json::to_string(&values).unwrap()).unwrap();
    assert!(values.iter().zip(&back).all(|(a, b)| a.to_bits() == b.to_bits()));
}

#[test]
fn surface_import_triangulates_a_grid_and_logs_the_read() {
    let dir = std::env::temp_dir().join(format!("zd-commands-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("topo.xyz");
    let grid = Grid::from_fn([E, N], [5.0, 5.0], 21, 11, |x, y| 200.0 + 0.01 * (x - E) - 0.02 * (y - N));
    zd_io::write_xyz_grid(&path, &grid).unwrap();

    let mut s = Session::with_builtins();
    let record = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "topography" })).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();

    let object = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(object.name, "topo");
    match &object.body {
        Body::Surface(surface) => assert_eq!(surface.mesh().triangle_count(), 2 * 20 * 10),
        _ => unreachable!(),
    }
    assert!(record.messages[0].starts_with("XYZ grid 21 x 11 nodes"), "{:?}", record.messages);
}

#[test]
fn save_and_open_carry_the_project_and_its_run_log() {
    let dir = std::env::temp_dir().join(format!("zd-commands-save-{}-{}", std::process::id(), zd_core::RunId::new()));
    let path = dir.join("pit.zdproj");
    let mut s = Session::with_builtins();
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": [[E, N, 1.25], [E + 9.5, N, 2.5]] })).unwrap())[0];
    let solid = created(&s.run("solid.from_mesh", box_mesh("seam A", [E, N, 100.0], [E + 10.0, N + 10.0, 110.0])).unwrap())[0];
    let surface = created(&s.run("surface.from_mesh", plane(104.0)).unwrap())[0];
    s.run("solid.cut_by_surface", json!({ "solid": solid, "surface": surface, "layer": "cuts" })).unwrap();
    let first = s.save(&path).unwrap();
    assert_eq!(first.object_files_written, 8, "solid, surface and two cut parts: vertices + triangles each");

    let mut t = Session::open(&path).unwrap();
    let ids = |s: &Session| s.project().objects().map(|o| (o.id, o.revision, o.name.clone())).collect::<Vec<_>>();
    assert_eq!(ids(&t), ids(&s));
    assert_eq!(&t.log()[..s.log().len()], s.log(), "run log carries on, including the save");
    assert!(matches!(t.log().last(), Some(LogEntry::Opened { .. })));
    assert!(!t.project().can_undo());

    t.run("string.move_vertex", json!({ "string": toe, "index": 1, "to": [E + 10.0, N + 1.0, 2.75] })).unwrap();
    let second = t.save(&path).unwrap();
    assert_eq!((second.object_files_written, second.object_files_reused), (0, 8), "only the string changed");
    assert_eq!(vertices(&Session::open(&path).unwrap(), toe)[1], [E + 10.0, N + 1.0, 2.75]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_open_mesh_is_refused_as_a_solid() {
    let mut s = Session::with_builtins();
    let mut open = box_mesh("leaky", [0.0; 3], [1.0; 3]);
    open["triangles"].as_array_mut().unwrap().pop();
    assert!(matches!(s.run("solid.from_mesh", open), Err(CommandError::NotWatertight(v)) if v.boundary_edges == 3));
    assert_eq!(s.project().objects().count(), 0);
}
