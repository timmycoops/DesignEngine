//! One face a strip, and strip solids, through the registry as the app runs them (decision 0049): a toe offset 14
//! times, numbered by a line across, projected up and down to RLs, joined by strip number, and a pit sliced by them.

mod common;

use std::time::Instant;

use serde_json::json;
use zd_commands::{Code, RunRecord, Session, Severity};
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
    // Each strip's line is said of the solid the run made for it, by id once it has one (0059).
    let said = record.notes.iter().find(|n| n.text.starts_with("strip 1: ") && n.text.ends_with(" m3")).unwrap_or_else(|| panic!("{:?}", record.messages));
    assert_eq!(said.object, Some(strips[0]));
    assert_eq!(said.value.as_ref().map(|v| v.unit.as_str()), Some("m3"));
    assert_volume(said.value.as_ref().unwrap().value, 7_500_000.0, "strip 1's note");
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

    // The pit and its sixteen strips each go out as DXF and come back the same solid.
    assert_eq!(common::assert_solids_round_trip(&s), 17);
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

#[test]
fn the_pit_made_between_two_surfaces_inside_its_limit_slices_the_same() {
    let mut s = Session::with_builtins();
    let flat = |s: &mut Session, name: &str, rl: f64| {
        let corners = [[E - 100.0, N - 100.0, rl], [E + 1100.0, N - 100.0, rl], [E + 1100.0, N + 600.0, rl], [E - 100.0, N + 600.0, rl]];
        created(&s.run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": corners, "triangles": [[0, 1, 2], [0, 2, 3]] })).unwrap())[0]
    };
    let (topography, floor) = (flat(&mut s, "topography", 100.0), flat(&mut s, "floor", 0.0));
    let limit = created(
        &s.run("string.create", json!({ "name": "pit limit", "layer": "limits", "closed": true, "points": [[E, N, 0.0], [E + 1000.0, N, 0.0], [E + 1000.0, N + 500.0, 0.0], [E, N + 500.0, 0.0]] }))
            .unwrap(),
    )[0];
    let record = s.run("solid.between_surfaces", json!({ "top": topography, "bottom": floor, "string": limit, "layer": "pit", "name": "PIT" })).unwrap();
    record.messages.iter().for_each(|m| println!("  {m}"));
    let pit = created(&record)[0];
    assert_volume(volume(&s, pit), 50_000_000.0, "the pit between the two");

    fifteen_strips(&mut s);
    let strips = created(&s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": surfaces_on(&s, "faces"), "layer": "strips" })).unwrap());
    assert_eq!(strips.len(), 16);
    assert_volume(volume(&s, strips[7]), 2_500_000.0, "strip 8");
    // The pit made between two surfaces and its strips go out as DXF and come back the same.
    assert_eq!(common::assert_solids_round_trip(&s), 17);
}

/// The made-up grid pit (`testdata/synthetic/Grid template.zdp`, or the older folder; opened and never saved): each of its designs run,
/// its faces joined by strip, a pit made between the topography and the L floor inside a limit across the strips, and
/// sliced by them. Prints what each run said, with timings.
#[test]
#[ignore = "runs every design of the grid template; opt in with --ignored"]
fn the_made_up_grid_pit_sliced_by_its_strips() {
    let path = grid_template();
    let mut s = Session::open(&path).unwrap();
    let named = |s: &Session, name: &str| s.project().objects().find(|o| o.name == name).unwrap_or_else(|| panic!("no '{name}'")).id;
    let (topo, floor) = (named(&s, "Topo"), named(&s, "L floor"));
    let limit = created(
        &s.run(
            "string.create",
            json!({ "name": "pit limit", "layer": "limits", "closed": true, "points": [[E + 100.0, N - 100.0, 0.0], [E + 3400.0, N - 100.0, 0.0], [E + 3400.0, N + 1070.0, 0.0], [E + 100.0, N + 1070.0, 0.0]] }),
        )
        .unwrap(),
    )[0];
    let start = Instant::now();
    let record = s.run("solid.between_surfaces", json!({ "top": topo, "bottom": floor, "string": limit, "layer": "pit", "name": "PIT" })).unwrap();
    println!("solid.between_surfaces: {:.0} ms", start.elapsed().as_secs_f64() * 1e3);
    record.messages.iter().for_each(|m| println!("  {m}"));
    let pit = created(&record)[0];
    let designs: Vec<(String, ObjectId)> = s.project().objects().filter(|o| matches!(o.body, Body::FaceDesign(_))).map(|o| (o.name.clone(), o.id)).collect();
    for (name, design) in designs {
        match s.run("face.project", json!({ "design": design })) {
            Err(e) => {
                println!("\n{name}: refused: {e}");
                continue;
            }
            // Each fault the run names, with where it is.
            Ok(record) => {
                record.messages.iter().filter(|m| ["cross", "isn't one sheet", "stopped on step", "Nothing is built"].iter().any(|w| m.contains(w))).for_each(|m| println!("\n{name}: {m}"));
                // Every fault the geometry can place is said with its place (0059, workstream 2).
                let placeable = [Code::FaceCrossing, Code::FaceNotOneSheet, Code::RampStopped, Code::RampSteepFlat, Code::WallNoTarget];
                let unplaced: Vec<&str> = record.notes.iter().filter(|n| n.code.is_some_and(|c| placeable.contains(&c)) && n.place.is_none()).map(|n| n.text.as_str()).collect();
                assert!(unplaced.is_empty(), "{name}: faults with no place: {unplaced:?}");
                let warnings = record.notes.iter().filter(|n| n.severity == Severity::Warning).count();
                let placed = record.notes.iter().filter(|n| n.severity == Severity::Warning && n.place.is_some()).count();
                let without: Vec<&str> = record.notes.iter().filter(|n| n.severity == Severity::Warning && n.place.is_none()).filter_map(|n| n.code.map(|c| c.name())).collect();
                println!("{name}: {warnings} warnings, {placed} with a place; without: {without:?}");
            }
        }
        let faces = surfaces_on(&s, &format!("faces {name}"));
        let start = Instant::now();
        match s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": faces, "layer": format!("strips {name}") })) {
            Ok(record) => {
                println!("\n{name}: sliced in {:.0} ms", start.elapsed().as_secs_f64() * 1e3);
                record.messages.iter().filter(|m| !m.contains("checked for crossings")).for_each(|m| println!("  {m}"));
                let strips = created(&record);
                let total: f64 = strips.iter().map(|&id| volume(&s, id)).sum();
                assert!((total - volume(&s, pit)).abs() < 1e-6 * volume(&s, pit), "{name}: strips {total} against the pit {}", volume(&s, pit));
            }
            Err(e) => println!("\n{name}: slice refused in {:.0} ms: {e}", start.elapsed().as_secs_f64() * 1e3),
        }
    }
}

/// Each design of the made-up grid pit run in two sessions opened fresh from the same project gives the same faces,
/// vertex for vertex and triangle for triangle (the review of 10 November: run to run the counts differed, 0034).
#[test]
#[ignore = "runs every design of the grid template twice; opt in with --ignored"]
fn the_made_up_grid_pit_runs_the_same_twice() {
    let path = grid_template();
    let designs: Vec<(String, ObjectId)> = Session::open(&path).unwrap().project().objects().filter(|o| matches!(o.body, Body::FaceDesign(_))).map(|o| (o.name.clone(), o.id)).collect();
    let faces = |design: ObjectId, name: &str| -> Option<Vec<(String, zd_core::TriMesh)>> {
        let mut s = Session::open(&path).unwrap();
        s.run("face.project", json!({ "design": design })).ok()?;
        let mut faces: Vec<(String, zd_core::TriMesh)> = surfaces_on(&s, &format!("faces {name}"))
            .into_iter()
            .map(|id| {
                let o = s.project().object(id).unwrap();
                let Body::Surface(surface) = &o.body else { unreachable!() };
                (o.name.clone(), surface.mesh().clone())
            })
            .collect();
        faces.sort_by(|a, b| a.0.cmp(&b.0));
        Some(faces)
    };
    let mut differ = Vec::new();
    for (name, design) in designs {
        let (Some(one), Some(two)) = (faces(design, &name), faces(design, &name)) else { continue };
        assert_eq!(one.len(), two.len(), "{name}: the number of faces");
        for ((face, a), (_, b)) in one.iter().zip(&two) {
            if a != b {
                differ.push(format!("{name} / {face}: {} against {} triangles", a.triangles.len(), b.triangles.len()));
            }
        }
    }
    assert!(differ.is_empty(), "faces that differ run to run:\n{}", differ.join("\n"));
}

/// A real pit as Tim works one: his toe on a seam floor offset downdip into `ZD_STRIPS` strips (15 by default),
/// `ZD_STRIP_WIDTH` apart (60 m by default), and numbered by a line across; each toe projected up through the seam
/// roofs at 70° with 10 m berms, to the topography and on to RL 500 at 45°, and down to RL -500 at 80°, run on 100 m
/// each end; the faces joined a face a strip; the pit made between the topography and the floor inside a limit across
/// the strips, checked against a column sum on the floor's nodes; and the pit sliced into a solid a strip. Takes
/// `ZD_FACE_GRIDS` (the floor first, then the roofs, `;` between), `ZD_FACE_TOPO` and `ZD_WALL_STRING` as the wall
/// tests do; with `ZD_SAVE` the project is saved there, to open in the app, and with `ZD_SECTIONS` the toes in plan
/// and three sections are written there to draw. Prints what each run said, with timings.
#[test]
#[ignore = "needs ZD_FACE_GRIDS, ZD_FACE_TOPO and ZD_WALL_STRING"]
fn fifteen_strips_on_real_grids() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is needed"));
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1e3;
    let width: f64 = std::env::var("ZD_STRIP_WIDTH").map_or(60.0, |w| w.trim().parse().unwrap());
    let count: u32 = std::env::var("ZD_STRIPS").map_or(15, |n| n.trim().parse().unwrap());
    let mut s = Session::with_builtins();
    let start = Instant::now();
    let mut import = |path: &str| created(&s.run("surface.import", json!({ "path": path, "layer": "surfaces" })).unwrap())[0];
    let grids: Vec<ObjectId> = var("ZD_FACE_GRIDS").split(';').map(&mut import).collect();
    let topo = import(&var("ZD_FACE_TOPO"));
    println!("{} grids and the topography imported in {:.0} ms", grids.len(), ms(start));

    let numbers = |p: &str| p.split(',').map(|n| n.trim().parse::<f64>().unwrap()).collect::<Vec<_>>();
    let points: Vec<Vec<f64>> = var("ZD_WALL_STRING").split(';').map(numbers).collect();
    let (a, b) = (&points[0], &points[points.len() - 1]);
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    let along = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    let right = [along[1], -along[0]];
    // The floor as its file holds it, nodes flagged off at the heights they hold (decision 0025), as the import has it.
    let read = zd_io::read_vulcan_grid(std::path::Path::new(var("ZD_FACE_GRIDS").split(';').next().unwrap())).unwrap();
    let mut floor = read.grid;
    for (node, z) in read.held {
        floor.z[node] = z;
    }
    let floor_rl = |p: [f64; 2]| {
        let (col, row) = (((p[0] - floor.origin[0]) / floor.cell_size[0]).round(), ((p[1] - floor.origin[1]) / floor.cell_size[1]).round());
        let inside = col >= 0.0 && row >= 0.0 && (col as usize) < floor.cols && (row as usize) < floor.rows;
        inside.then(|| floor.z_at(col as usize, row as usize)).filter(|z| z.is_finite())
    };
    // Downdip is the side the floor falls to, 100 m to 300 m out from the middle of the toe.
    let mid = [a[0] + 0.5 * (b[0] - a[0]), a[1] + 0.5 * (b[1] - a[1])];
    let fall = (1..=3)
        .find_map(|k| {
            let d = 100.0 * f64::from(k);
            Some(floor_rl([mid[0] - d * right[0], mid[1] - d * right[1]])? - floor_rl([mid[0] + d * right[0], mid[1] + d * right[1]])?)
        })
        .expect("the floor under the middle of the toe, each side");
    let (side, out) = if fall > 0.0 { ("right", 1.0) } else { ("left", -1.0) };
    println!("the floor falls {:.1} m to the {side} over the toe's middle", fall.abs());
    let at = |metres_along: f64, metres_out: f64| [a[0] + metres_along * along[0] + out * metres_out * right[0], a[1] + metres_along * along[1] + out * metres_out * right[1], 0.0];

    // Strips: the toe and its offsets downdip, numbered from 1 by a line across from the first.
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "toes", "points": points })).unwrap())[0];
    let mut toes = vec![toe];
    toes.extend(created(&s.run("string.offset", json!({ "string": toe, "distance": width, "side": side, "times": count - 1, "layer": "toes" })).unwrap()));
    let numbered = s.run("string.number_across", json!({ "strings": toes, "from": at(0.5 * length, -30.0), "to": at(0.5 * length, f64::from(count) * width), "start": 1.0 })).unwrap();
    println!("{}", numbered.messages.join("; "));

    let mut up = Vec::new();
    for grid in &grids[1..] {
        up.push(json!({ "step": "project", "to": { "kind": "surface", "id": grid }, "batter": 70.0, "condition": 0.1 }));
        up.push(json!({ "step": "berm", "width": 10.0, "condition": 0.1 }));
    }
    up.push(json!({ "step": "project", "to": { "kind": "surface", "id": topo }, "batter": 45.0, "condition": 0.1 }));
    up.push(json!({ "step": "project", "to": { "kind": "rl", "rl": 500.0 }, "batter": 45.0, "condition": 0.1 }));
    let lines: Vec<_> = toes.iter().flat_map(|t| ["Up", "Down"].map(|profile| json!({ "string": t, "profile": profile, "side": side, "run_on": [100.0, 100.0] }))).collect();
    let content = json!({
        "layer": "faces",
        "lines": lines,
        "profiles": [
            { "name": "Up", "direction": "up", "register": grids[0], "steps": up },
            { "name": "Down", "direction": "down", "register": grids[0], "steps": [{ "step": "project", "to": { "kind": "rl", "rl": -500.0 }, "batter": 80.0, "condition": 0.1 }] },
        ],
    });
    let design = created(&s.run("face.save_design", json!({ "name": "strips", "content": content })).unwrap())[0];
    let start = Instant::now();
    let record = s.run("face.project", json!({ "design": design })).unwrap();
    println!("\nface.project: {:.0} ms, {} messages", ms(start), record.messages.len());
    record.messages.iter().take(12).for_each(|m| println!("  {m}"));
    let faces = surfaces_on(&s, "faces");
    let crossing = faces
        .iter()
        .filter(|&&id| match &s.project().object(id).unwrap().body {
            Body::Surface(sheet) => sheet.validity().non_self_intersecting != Some(true),
            _ => false,
        })
        .count();
    println!("  {} faces, {crossing} with crossing triangles", faces.len());

    let start = Instant::now();
    match s.run("face.join", json!({ "faces": faces, "layer": "strip faces" })) {
        Ok(record) => {
            println!("\nface.join: {:.0} ms", ms(start));
            record.messages.iter().for_each(|m| println!("  {m}"));
        }
        Err(e) => println!("\nface.join refused in {:.0} ms: {e}", ms(start)),
    }

    // The pit: inside a limit 100 m behind the first toe to 150 m past the last, and 20 m in from where the toes end.
    let reach = |id: &ObjectId| {
        let line = match &s.project().object(*id).unwrap().body {
            Body::String(line) => line.vertices.clone(),
            _ => panic!("a toe is a string"),
        };
        let along_of = |v: [f64; 3]| (v[0] - a[0]) * along[0] + (v[1] - a[1]) * along[1];
        (along_of(line[0]), along_of(line[line.len() - 1]))
    };
    let from = toes.iter().map(|t| reach(t).0).fold(f64::NEG_INFINITY, f64::max) + 20.0;
    let to = toes.iter().map(|t| reach(t).1).fold(f64::INFINITY, f64::min) - 20.0;
    let (behind, past) = (-100.0, f64::from(count - 1) * width + 150.0);
    let corners = [at(from, behind), at(to, behind), at(to, past), at(from, past)];
    println!("\nthe limit: {:.0} m along the strike by {:.0} m across", to - from, past - behind);
    let limit = created(&s.run("string.create", json!({ "name": "pit limit", "layer": "limits", "closed": true, "points": corners })).unwrap())[0];
    let start = Instant::now();
    let record = s.run("solid.between_surfaces", json!({ "top": topo, "bottom": grids[0], "string": limit, "layer": "pit", "name": "PIT" })).unwrap();
    println!("solid.between_surfaces: {:.0} ms", ms(start));
    record.messages.iter().for_each(|m| println!("  {m}"));
    let pit = created(&record)[0];
    let pit_volume = volume(&s, pit);

    // A column sum on the floor's nodes inside the limit: each node's cell times the topography over the floor there.
    let inside = |p: [f64; 2]| {
        let mut odd = false;
        for i in 0..corners.len() {
            let (u, v) = (corners[i], corners[(i + 1) % corners.len()]);
            if (u[1] > p[1]) != (v[1] > p[1]) && p[0] < u[0] + (p[1] - u[1]) / (v[1] - u[1]) * (v[0] - u[0]) {
                odd = !odd;
            }
        }
        odd
    };
    let nodes: Vec<[f64; 3]> = (0..floor.rows)
        .flat_map(|row| (0..floor.cols).map(move |col| (col, row)))
        .filter_map(|(col, row)| {
            let p = floor.node_xy(col, row);
            let z = floor.z_at(col, row);
            (z.is_finite() && inside(p)).then_some([p[0], p[1], f64::NAN])
        })
        .collect();
    let Body::Surface(top) = &s.project().object(topo).unwrap().body else { panic!("the topography is a surface") };
    let start = Instant::now();
    let (draped, _) = zd_geom::drape_string(&zd_core::Polyline { vertices: nodes.clone(), closed: false }, top.mesh(), false).unwrap();
    let cell = floor.cell_size[0] * floor.cell_size[1];
    let columns: f64 = draped.vertices.iter().filter(|v| v[2].is_finite()).map(|v| cell * (v[2] - floor_rl([v[0], v[1]]).unwrap()).max(0.0)).sum();
    println!(
        "  a column sum on {} floor nodes: {columns:.0} m³ against the solid's {pit_volume:.0} m³, {:+.2}% ({:.0} ms)",
        nodes.len(),
        100.0 * (pit_volume - columns) / columns,
        ms(start)
    );

    let start = Instant::now();
    match s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": faces, "layer": "strips" })) {
        Ok(record) => {
            println!("\nsolid.slice_by_faces: {:.0} ms", ms(start));
            record.messages.iter().for_each(|m| println!("  {m}"));
            let strips = created(&record);
            let mut total = 0.0;
            for &id in &strips {
                let object = s.project().object(id).unwrap();
                let Body::Solid(solid) = &object.body else { panic!("a strip is a solid") };
                let v = solid.validity();
                total += solid.volume();
                println!(
                    "  {:<20} {:>14.0} m³ {:>8.1} m deep on average, {:>6} triangles, closed {} manifold {}",
                    object.name,
                    solid.volume(),
                    solid.volume() / (width * (to - from)),
                    solid.mesh().triangles.len(),
                    v.closed,
                    v.manifold
                );
                assert!(v.closed && v.manifold, "{} is a solid", object.name);
            }
            println!("  {} strips add up to {total:.3} m³ against the pit's {pit_volume:.3} m³", strips.len());
            assert!((total - pit_volume).abs() < 1e-6 * pit_volume, "the strips add up to the pit");

            // With `ZD_SECTIONS`, three sections down the dip, a quarter, half and three quarters along the limit, written
            // there as JSON to draw: each strip's fill, and the lines of the topography, the floor and the faces.
            if let Ok(path) = std::env::var("ZD_SECTIONS") {
                let mut sections = Vec::new();
                for share in [0.25, 0.5, 0.75] {
                    let m = from + share * (to - from);
                    let (p, q) = (at(m, behind), at(m, past));
                    let plane = zd_geom::SectionPlane::through([p[0], p[1]], [q[0], q[1]]).unwrap();
                    let flat = |v: [f64; 3]| [plane.along(v), v[2]];
                    let fills: Vec<_> = strips
                        .iter()
                        .map(|&id| {
                            let object = s.project().object(id).unwrap();
                            let Body::Solid(solid) = &object.body else { panic!("a strip is a solid") };
                            let fill = zd_geom::fill_section_loops(&zd_geom::slice_mesh(solid.mesh(), &plane), &plane);
                            json!({ "name": object.name, "triangles": fill.triangles.iter().map(|t| t.map(flat)).collect::<Vec<_>>() })
                        })
                        .collect();
                    let lines = |id: ObjectId| -> Vec<Vec<[f64; 2]>> {
                        match &s.project().object(id).unwrap().body {
                            Body::Surface(sheet) => zd_geom::slice_mesh(sheet.mesh(), &plane).iter().map(|l| l.points.iter().map(|&v| flat(v)).collect()).collect(),
                            _ => Vec::new(),
                        }
                    };
                    let face_lines: Vec<_> = faces.iter().flat_map(|&id| lines(id)).collect();
                    sections.push(json!({ "share": share, "strips": fills, "topography": lines(topo), "floor": lines(grids[0]), "faces": face_lines }));
                }
                // The toes in plan, as metres along the strike and out downdip from the first toe's start.
                let plan: Vec<Vec<[f64; 2]>> = toes
                    .iter()
                    .map(|&id| match &s.project().object(id).unwrap().body {
                        Body::String(line) => line.vertices.iter().map(|v| [(v[0] - a[0]) * along[0] + (v[1] - a[1]) * along[1], out * ((v[0] - a[0]) * right[0] + (v[1] - a[1]) * right[1])]).collect(),
                        _ => panic!("a toe is a string"),
                    })
                    .collect();
                let limit_plan: Vec<[f64; 2]> = [(from, behind), (to, behind), (to, past), (from, past)].into_iter().map(|(m, o)| [m, o]).collect();
                std::fs::write(&path, serde_json::to_string(&json!({ "sections": sections, "toes": plan, "limit": limit_plan })).unwrap()).unwrap();
                println!("  sections written to {path}");
            }
        }
        Err(e) => panic!("solid.slice_by_faces refused in {:.0} ms: {e}", ms(start)),
    }
    if let Ok(dir) = std::env::var("ZD_SAVE") {
        s.save(&dir).unwrap();
        println!("\nsaved to {dir}");
    }
}

/// The Builder's steps from a toe drawn by hand, with no strip number: their sheets join into one face by the build's name,
/// and that face slices a pit as one strip.
#[test]
fn a_builds_steps_join_into_one_face_by_its_name() {
    let mut s = Session::with_builtins();
    let toe = created(&s.run("string.create", json!({ "name": "Toe", "layer": "toes", "points": [[E - 50.0, N + 100.0, 0.0], [E + 1050.0, N + 100.0, 0.0]] })).unwrap())[0];
    let wall = |rl: f64| json!({ "op": "project", "to": { "kind": "rl", "rl": rl }, "batter": 45.0 });
    s.run("face.build_step", json!({ "from": toe, "step": wall(60.0), "lies_on": { "kind": "rl", "rl": -10.0 }, "side": "left" })).unwrap();
    let crest = s.project().objects().find(|o| o.name == "Toe crest 1").unwrap().id;
    s.run("face.build_step", json!({ "from": crest, "step": { "op": "berm", "width": 10.0 } })).unwrap();
    let line = s.project().objects().find(|o| o.name == "Toe toe 2").unwrap().id;
    s.run("face.build_step", json!({ "from": line, "step": wall(150.0) })).unwrap();
    let sheets: Vec<ObjectId> = s.project().objects().filter(|o| matches!(o.body, Body::Surface(_)) && o.attributes.get("build") == Some(&AttrValue::Text("Toe".into()))).map(|o| o.id).collect();
    assert_eq!(sheets.len(), 2, "a sheet a wall, the berm's with the wall above");
    let refused = s.run("face.join", json!({ "faces": sheets, "layer": "faces" })).unwrap_err().to_string();
    assert!(refused.contains("has no strip number"), "{refused}");
    let record = s.run("face.join", json!({ "faces": sheets, "layer": "faces", "attribute": "build" })).unwrap();
    record.messages.iter().for_each(|m| println!("  {m}"));
    let face = created(&record)[0];
    let object = s.project().object(face).unwrap();
    assert_eq!(object.name, "Toe face");
    let Body::Surface(surface) = &object.body else { panic!("a face is a surface") };
    assert_eq!(zd_geom::boundary_loops(surface.mesh()).unwrap().len(), 1, "one piece");
    let areas: f64 = sheets.iter().map(|&id| match &s.project().object(id).unwrap().body {
        Body::Surface(sheet) => sheet.mesh().area(),
        _ => 0.0,
    }).sum();
    assert!((surface.mesh().area() - areas).abs() < 1e-4, "area kept to 1 cm²");

    // The pit sliced by it as one strip: the toe on RL -10 100 m north, so the 45° wall stands 110 + z m north up to RL 60,
    // and 120 + z past the 10 m berm.
    let pit = pit(&mut s);
    let strips = created(&s.run("solid.slice_by_faces", json!({ "solid": pit, "faces": [face], "layer": "strips", "attribute": "build" })).unwrap());
    assert_eq!(strips.len(), 2);
    assert_eq!(s.project().object(strips[0]).unwrap().name, "PIT build Toe");
    // In front: 1000 x (the integral of 110 + z over RL 0 to 60, and of 120 + z over RL 60 to 100).
    assert_volume(volume(&s, strips[0]), 1000.0 * (110.0 * 60.0 + 1800.0 + 120.0 * 40.0 + 3200.0), "in front of the build");
}

/// The made-up grid pit: `Grid template.zdp` as `testdata/synthetic/grid_template.py` writes it now, else the
/// `.zdproj` folder it wrote before (decision 0058).
fn grid_template() -> std::path::PathBuf {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/synthetic");
    [folder.join("Grid template.zdp"), folder.join("Grid template.zdproj")].into_iter().find(|p| p.exists()).expect("run testdata/synthetic/grid_template.py first")
}
