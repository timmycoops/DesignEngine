//! Moving things whole (String tools plan, workstream 3): `object.transform` moves, rotates in plan or mirrors in plan
//! the objects picked as one run and one undo, and `object.copy` makes moved copies, leaving the objects as they were.
//! Every answer is known; mine grid coordinates throughout.

use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Attributes, Body, Change, ChangeKind, Grid, ObjectId, Polyline, Project, RunId, Solid, TriMesh};
use zd_io::ProjectStore;

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn at(x: f64, y: f64, z: f64) -> [f64; 3] {
    [E + x, N + y, z]
}

fn string(s: &mut Session, name: &str, points: &[[f64; 3]]) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points })).unwrap())[0]
}

fn line(s: &Session, id: ObjectId) -> Polyline {
    match &s.project().object(id).expect("live string").body {
        Body::String(line) => line.clone(),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

fn revision(s: &Session, id: ObjectId) -> u32 {
    s.project().object(id).unwrap().revision
}

/// A 10 m cube from RL 100 to 110, wound counter-clockwise about its outward normals.
fn cube(x: f64, y: f64) -> TriMesh {
    let ([x0, y0, z0], [x1, y1, z1]) = (at(x, y, 100.0), at(x + 10.0, y + 10.0, 110.0));
    TriMesh::new(
        vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
    )
}

fn solid(s: &mut Session, name: &str, mesh: &TriMesh) -> ObjectId {
    created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "blocks", "vertices": mesh.vertices, "triangles": mesh.triangles })).unwrap())[0]
}

fn solid_of(s: &Session, id: ObjectId) -> Solid {
    match &s.project().object(id).unwrap().body {
        Body::Solid(solid) => solid.clone(),
        other => panic!("expected a solid, found {:?}", other.kind()),
    }
}

/// A string moved 100.5 m east, 200.25 m south and 3 m up: every vertex moved by exactly that, in one undo.
#[test]
fn a_string_moved_by_distances_typed_moves_every_vertex_by_exactly_that() {
    let mut s = Session::with_builtins();
    let points = [at(0.0, 0.0, 100.0), at(50.0, 10.0, 105.25), at(100.0, 0.0, 110.5)];
    let toe = string(&mut s, "Toe", &points);
    let record = s.run("object.transform", json!({ "objects": [toe], "transform": { "kind": "move", "by": [100.5, -200.25, 3.0] } })).unwrap();
    assert_eq!(record.messages, ["moved 1 string 100.5 m east, 200.25 m south and 3 m up"]);
    let moved = line(&s, toe);
    for (v, p) in moved.vertices.iter().zip(&points) {
        assert_eq!(*v, [p[0] + 100.5, p[1] - 200.25, p[2] + 3.0], "exact");
    }
    assert_eq!(revision(&s, toe), 2, "the string's next revision");
    let object = s.project().object(toe).unwrap();
    assert_eq!((object.name.as_str(), s.project().layer(object.layer).unwrap().name.as_str()), ("Toe", "toes"), "name and layer as they were");
    s.undo();
    assert_eq!(line(&s, toe).vertices, points, "one undo puts it back");
}

/// Fifteen toes rotated a quarter turn about a point, anticlockwise: a vertex 100 m east of the point lands 100 m north
/// of it, heights as they were, all fifteen in one run and one undo.
#[test]
fn fifteen_toes_rotated_a_quarter_turn_about_a_point_in_one_undo() {
    let mut s = Session::with_builtins();
    let ids: Vec<ObjectId> = (0..15).map(|k| string(&mut s, &format!("Toe {}", k + 1), &[at(0.0, 10.0 * k as f64, 100.0), at(100.0, 10.0 * k as f64, 110.0)])).collect();
    let record = s.run("object.transform", json!({ "objects": ids, "transform": { "kind": "rotate", "about": [E, N], "angle": 90.0 } })).unwrap();
    assert_eq!(record.messages, [format!("rotated 15 strings 90° anticlockwise about {E:.3} E, {N:.3} N")]);
    for (k, &id) in ids.iter().enumerate() {
        let end = line(&s, id).vertices[1];
        let want = at(-10.0 * k as f64, 100.0, 110.0);
        assert!((end[0] - want[0]).abs() < 1e-9 && (end[1] - want[1]).abs() < 1e-9 && end[2] == 110.0, "{end:?}");
    }
    assert_eq!(line(&s, ids[0]).vertices[1], at(0.0, 100.0, 110.0), "a quarter turn is exact");
    s.undo();
    assert!(ids.iter().all(|&id| revision(&s, id) == 1), "one undo puts all fifteen back");
}

/// A 10 m cube mirrored across a line: 1,000 m³ still, closed, manifold and consistently wound, its faces out. Moved and
/// turned 30°, the same.
#[test]
fn a_cube_mirrored_moved_or_turned_keeps_its_volume_and_faces_out() {
    let mut s = Session::with_builtins();
    let id = solid(&mut s, "Block 1", &cube(0.0, 0.0));
    let close = |v: f64| ((v - 1_000.0) / 1_000.0).abs() < 1e-9;
    let record = s.run("object.transform", json!({ "objects": [id], "transform": { "kind": "mirror", "from": [E - 20.0, N], "to": [E - 20.0, N + 50.0] } })).unwrap();
    assert_eq!(record.messages, [format!("mirrored 1 solid across the line from {:.3} E, {N:.3} N to {:.3} E, {:.3} N", E - 20.0, E - 20.0, N + 50.0)]);
    let mirrored = solid_of(&s, id);
    let flags = mirrored.validity();
    assert!(flags.closed && flags.manifold && flags.consistent_winding, "{flags:?}");
    assert!(close(mirrored.volume()), "faces out: a positive 1,000 m³, got {}", mirrored.volume());
    let x: Vec<f64> = mirrored.mesh().vertices.iter().map(|v| v[0]).collect();
    assert_eq!((x.iter().cloned().fold(f64::INFINITY, f64::min), x.iter().cloned().fold(f64::NEG_INFINITY, f64::max)), (E - 50.0, E - 40.0), "40 to 50 m west, as far the other side");
    s.run("object.transform", json!({ "objects": [id], "transform": { "kind": "move", "by": [1_000.25, 2_000.5, -50.0] } })).unwrap();
    assert!(close(solid_of(&s, id).volume()));
    s.run("object.transform", json!({ "objects": [id], "transform": { "kind": "rotate", "about": [E + 3.0, N + 7.0], "angle": 30.0 } })).unwrap();
    assert!(close(solid_of(&s, id).volume()), "{}", solid_of(&s, id).volume());
}

/// A copy: the original at the same revision, and a new object with the same vertices, named for it with " copy", on
/// its layer and carrying its attributes. Copied with a move, the copy is moved and the original stays.
#[test]
fn a_copy_is_a_new_object_and_the_original_stays_as_it_was() {
    let mut s = Session::with_builtins();
    let points = [at(0.0, 0.0, 100.0), at(100.0, 0.0, 110.0)];
    let toe = string(&mut s, "Toe 4", &points);
    s.run("string.number_across", json!({ "strings": [toe], "from": at(50.0, -5.0, 0.0), "to": at(50.0, 5.0, 0.0), "start": 4.0 })).unwrap();
    let before = revision(&s, toe);
    let record = s.run("object.copy", json!({ "objects": [toe] })).unwrap();
    assert_eq!(record.messages, ["copied 1 string in place"]);
    let copy = created(&record)[0];
    assert_ne!(copy, toe);
    assert_eq!(revision(&s, toe), before, "the original untouched");
    assert_eq!(line(&s, copy).vertices, points);
    let object = s.project().object(copy).unwrap();
    assert_eq!(object.name, "Strip 4 copy", "named for the string, which numbering named Strip 4");
    assert_eq!(s.project().layer(object.layer).unwrap().name, "toes");
    assert_eq!(object.attributes.get("strip"), Some(&AttrValue::Number(4.0)));

    let record = s.run("object.copy", json!({ "objects": [toe], "transform": { "kind": "move", "by": [0.0, 50.0, 0.0] } })).unwrap();
    assert_eq!(record.messages, ["copied 1 string, moved 50 m north"]);
    assert_eq!(line(&s, created(&record)[0]).vertices, [at(0.0, 50.0, 100.0), at(100.0, 50.0, 110.0)]);
    assert_eq!(line(&s, toe).vertices, points, "the original where it was");
    assert_eq!(revision(&s, toe), before);
}

/// A mirrored copy of a solid is a solid of the same volume beside it, the original as it was.
#[test]
fn a_mirrored_copy_of_a_solid_is_whole() {
    let mut s = Session::with_builtins();
    let id = solid(&mut s, "Block 1", &cube(0.0, 0.0));
    let record = s.run("object.copy", json!({ "objects": [id], "transform": { "kind": "mirror", "from": [E, N], "to": [E, N + 1.0] } })).unwrap();
    let copy = solid_of(&s, created(&record)[0]);
    assert!(((copy.volume() - 1_000.0) / 1_000.0).abs() < 1e-9 && copy.validity().is_watertight());
    assert_eq!(revision(&s, id), 1);
}

/// A session on a saved project holding a 5 m grid at RL 104 with one null node, and a string.
fn with_a_grid() -> (Session, ObjectId, ObjectId, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("zd-transform-{}-{}", std::process::id(), RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut project = Project::new();
    let mut z = vec![104.0; 12];
    z[5] = f64::NAN;
    let grid = Grid::new([E, N], [5.0, 5.0], 4, 3, z);
    let toe = Body::String(Polyline { vertices: vec![at(0.0, 0.0, 104.0), at(15.0, 0.0, 104.0)], closed: false });
    project
        .apply(
            RunId::new(),
            vec![
                Change::Create { name: "Floor".into(), layer: "grids".into(), attributes: Attributes::new(), body: Body::Grid(Arc::new(grid)) },
                Change::Create { name: "Toe".into(), layer: "toes".into(), attributes: Attributes::new(), body: toe },
            ],
        )
        .unwrap();
    let named = |name: &str| project.objects().find(|o| o.name == name).unwrap().id;
    let (grid, toe) = (named("Floor"), named("Toe"));
    ProjectStore::create(dir.join("pit.zdproj")).unwrap().save(&project, "[]").unwrap();
    (Session::open(dir.join("pit.zdproj")).unwrap(), grid, toe, dir)
}

/// A grid moves with its nulls kept; it doesn't rotate or mirror, and a run with one refuses, listing it, with nothing
/// changed.
#[test]
fn a_grid_moves_but_a_rotation_with_one_in_it_is_refused_whole() {
    let (mut s, grid, toe, dir) = with_a_grid();
    s.run("object.transform", json!({ "objects": [grid, toe], "transform": { "kind": "move", "by": [100.5, -200.25, 3.0] } })).unwrap();
    let Body::Grid(moved) = &s.project().object(grid).unwrap().body else { panic!("a grid") };
    assert_eq!(moved.origin, [E + 100.5, N - 200.25]);
    assert_eq!(moved.z[0], 107.0);
    assert!(moved.z[5].is_nan(), "a null stays null");
    let error = s.run("object.transform", json!({ "objects": [toe, grid], "transform": { "kind": "rotate", "about": [E, N], "angle": 45.0 } })).unwrap_err();
    assert_eq!(error.to_string(), "1 of 2 objects can't be moved, so nothing was changed. 'Floor': a grid moves but doesn't rotate or mirror: its rows run east and north");
    assert_eq!((revision(&s, toe), revision(&s, grid)), (2, 2), "nothing changed");
    std::fs::remove_dir_all(dir).ok();
}

/// What can't be done is refused before anything changes: a mirror line with no length, a move by nothing.
#[test]
fn a_mirror_line_with_no_length_and_a_move_by_nothing_are_refused() {
    let mut s = Session::with_builtins();
    let toe = string(&mut s, "Toe", &[at(0.0, 0.0, 100.0), at(100.0, 0.0, 110.0)]);
    let error = s.run("object.transform", json!({ "objects": [toe], "transform": { "kind": "mirror", "from": [E, N], "to": [E, N] } })).unwrap_err();
    assert!(error.to_string().contains("the mirror line has no length"), "{error}");
    let error = s.run("object.transform", json!({ "objects": [toe], "transform": { "kind": "rotate", "about": [E, N], "angle": 360.0 } })).unwrap_err();
    assert!(error.to_string().contains("that moves nothing"), "{error}");
    assert_eq!(revision(&s, toe), 1);
}

/// Timings on made-up data: 150 toes and 1,000 cubes moved, then rotated, then copied.
#[test]
fn timings_for_150_toes_and_1000_cubes() {
    let mut s = Session::with_builtins();
    let toes: Vec<ObjectId> = (0..150).map(|k| string(&mut s, &format!("Toe {k}"), &(0..41).map(|i| at(5.0 * i as f64, 10.0 * k as f64, 100.0 + 0.1 * i as f64)).collect::<Vec<_>>())).collect();
    let cubes: Vec<ObjectId> = (0..1_000).map(|k| solid(&mut s, &format!("Block {k}"), &cube(20.0 * (k % 40) as f64, 20.0 * (k / 40) as f64))).collect();
    let mut all = toes.clone();
    all.extend(&cubes);
    for transform in [json!({ "kind": "move", "by": [100.5, -200.25, 3.0] }), json!({ "kind": "rotate", "about": [E, N], "angle": 30.0 })] {
        let start = Instant::now();
        s.run("object.transform", json!({ "objects": all, "transform": transform })).unwrap();
        eprintln!("object.transform {}: 150 toes (6,150 vertices) and 1,000 cubes (12,000 triangles) in {:.1} ms", transform["kind"], start.elapsed().as_secs_f64() * 1e3);
    }
    let start = Instant::now();
    let record = s.run("object.copy", json!({ "objects": all, "transform": { "kind": "mirror", "from": [E, N], "to": [E, N + 1.0] } })).unwrap();
    eprintln!("object.copy mirrored: {} objects in {:.1} ms", created(&record).len(), start.elapsed().as_secs_f64() * 1e3);
    assert_eq!(created(&record).len(), 1_150);
    let total: f64 = cubes.iter().map(|&id| solid_of(&s, id).volume()).sum();
    assert!(((total - 1_000_000.0) / 1_000_000.0).abs() < 1e-9, "{total}");
}
