//! `string.project` (decision 0079) through the registry, as the app calls it: strings projected at a batter, up or
//! down, to an RL, a height, a grid or a surface, several in one run, with known answers.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};
use zd_commands::{Code, RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, Object, ObjectId, Polyline, TriMesh};

const E: f64 = 500_025.0;
const N: f64 = 7_000_625.0;

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

/// A string from points about the origin.
fn string(s: &mut Session, name: &str, points: &[[f64; 3]]) -> ObjectId {
    let points: Vec<[f64; 3]> = points.iter().map(|p| [E + p[0], N + p[1], p[2]]).collect();
    created(&s.run("string.create", json!({ "name": name, "layer": "toes", "points": points })).unwrap())[0]
}

/// Every vertex at a northing about the origin and an RL, to 1 µm.
fn at(line: &Polyline, y: f64, z: f64) -> bool {
    line.vertices.iter().all(|p| (p[1] - N - y).abs() < 1e-6 && (p[2] - z).abs() < 1e-6)
}

fn rl(rl: f64) -> Value {
    json!({ "kind": "rl", "rl": rl })
}

fn on(id: ObjectId) -> Value {
    json!({ "kind": "surface", "id": id })
}

/// A grid in Vulcan's plain layout: big-endian, nodes from the south-west at 25 m, x fastest, all live.
fn vulcan_grid(cols: usize, rows: usize, z: impl Fn(f64, f64) -> f64) -> Vec<u8> {
    let mut out = b"SYNTHETIC TEST GRID             ".to_vec();
    for v in [E, E + (cols - 1) as f64 * 25.0, N, N + (rows - 1) as f64 * 25.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(cols * rows) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&25.0f32.to_be_bytes());
    out.extend_from_slice(&25.0f32.to_be_bytes());
    out.extend_from_slice(&(cols as i32).to_be_bytes());
    out.extend_from_slice(&(rows as i32).to_be_bytes());
    for row in 0..rows {
        for col in 0..cols {
            out.extend_from_slice(&(z(col as f64 * 25.0, row as f64 * 25.0) as f32).to_be_bytes());
            out.extend_from_slice(&1i32.to_be_bytes());
        }
    }
    out
}

/// A seam's floor and roof as Vulcan grids, and a surface from XYZ at one RL, all over the same 300 m square.
fn seam_and_surface(s: &mut Session, floor: f64, roof: f64, surface: f64) -> (ObjectId, ObjectId, ObjectId) {
    let dir = std::env::temp_dir().join(format!("zd-project-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pitlower.sfg"), vulcan_grid(13, 13, |_, _| floor)).unwrap();
    std::fs::write(dir.join("pitlower.srg"), vulcan_grid(13, 13, |_, _| roof)).unwrap();
    let mut xyz = String::new();
    for row in 0..13 {
        for col in 0..13 {
            xyz += &format!("{} {} {surface}\n", E + col as f64 * 25.0, N + row as f64 * 25.0);
        }
    }
    std::fs::write(dir.join("upper.xyz"), xyz).unwrap();
    s.run("grid.import_vulcan_seam", json!({ "dir": dir.to_string_lossy(), "prefix": "pit", "seam": "lower", "layer": "geology" })).unwrap();
    let upper = created(&s.run("surface.import", json!({ "path": dir.join("upper.xyz").to_string_lossy(), "layer": "surfaces" })).unwrap())[0];
    std::fs::remove_dir_all(&dir).unwrap();
    (named(s, "LOWER sf").id, named(s, "LOWER sr").id, upper)
}

/// Three toes 200 m long at RL 100, 80 m apart, numbered by a line across. One run projects them all up at 45° to RL
/// 120: each new string stands 20 m out on the side asked for, at RL 120, named for its toe and keeping its strip
/// number; each wall is 200 × 20√2 m², to 1e-6 m². The toes are left as they were, and one undo takes it all back.
#[test]
fn toes_projected_to_an_rl_in_one_run() {
    let mut s = Session::with_builtins();
    let toes: Vec<ObjectId> = (0..3).map(|k| string(&mut s, &format!("t{k}"), &[[0.0, k as f64 * -80.0, 100.0], [200.0, k as f64 * -80.0, 100.0]])).collect();
    s.run("string.number_across", json!({ "strings": toes, "from": [E + 50.0, N + 10.0, 0.0], "to": [E + 50.0, N - 200.0, 0.0], "start": 1.0 })).unwrap();
    let before = s.project().objects().count();

    let record = s.run("string.project", json!({ "strings": toes, "to": rl(120.0), "batter": 45.0, "side": "left", "wall": true })).unwrap();
    assert_eq!(created(&record).len(), 6, "a string and a wall for each toe");
    assert!(record.messages[0].starts_with("projected 3 strings up at 45° to RL 120, on the left: 3 strings, 6 vertices; 3 walls, "), "{:?}", record.messages);
    for k in 0..3 {
        let name = format!("Strip {} to RL 120", k + 1);
        assert!(at(&line_of(&s, &name), k as f64 * -80.0 + 20.0, 120.0), "{:?}", line_of(&s, &name).vertices);
        assert_eq!(named(&s, &name).attributes.get("strip"), Some(&AttrValue::Number(k as f64 + 1.0)), "it keeps its strip");
        let wall = sheet_of(&s, &format!("{name} wall"));
        assert!((wall.area() - 200.0 * 20.0 * 2f64.sqrt()).abs() < 1e-6, "{}", wall.area());
        assert_eq!(named(&s, &format!("{name} wall")).attributes.get("strip"), Some(&AttrValue::Number(k as f64 + 1.0)));
        assert!(at(&line_of(&s, &format!("Strip {}", k + 1)), k as f64 * -80.0, 100.0), "the toe is left as it was");
    }
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), before, "one undo takes back everything the run made");

    // Down on the right, with no walls kept and names of its own.
    let down = s.run("string.project", json!({ "strings": toes, "to": rl(85.0), "batter": 45.0, "side": "right", "direction": "down", "name": "Floor", "layer": "floors" })).unwrap();
    assert_eq!(created(&down).len(), 3, "no walls unless asked");
    for k in 0..3 {
        assert!(at(&line_of(&s, &format!("Floor {}", k + 1)), k as f64 * -80.0 - 15.0, 85.0));
    }
    assert!(s.project().layers().iter().any(|l| l.name == "floors"));
}

/// To grids and surfaces, up and down. A seam's floor and roof as Vulcan grids at RL 100 and 120 and a surface at RL
/// 140; a string at RL 110 between the floor and the roof. At 45°: up to the roof grid stands 10 m out at RL 120, up to
/// the surface 30 m out at RL 140, and down to the floor grid 10 m out at RL 100, each on the side asked for, to 1 µm.
#[test]
fn a_string_goes_to_grids_and_surfaces_up_and_down() {
    let mut s = Session::with_builtins();
    let (floor, roof, upper) = seam_and_surface(&mut s, 100.0, 120.0, 140.0);
    let line = string(&mut s, "Line", &[[75.0, 150.0, 110.0], [225.0, 150.0, 110.0]]);
    let project = |s: &mut Session, to: ObjectId, side: &str, direction: &str| s.run("string.project", json!({ "strings": [line], "to": on(to), "batter": 45.0, "side": side, "direction": direction, "wall": true })).unwrap();

    project(&mut s, roof, "left", "up");
    assert!(at(&line_of(&s, "Line to LOWER sr"), 160.0, 120.0), "{:?}", line_of(&s, "Line to LOWER sr").vertices);
    project(&mut s, upper, "right", "up");
    assert!(at(&line_of(&s, "Line to upper"), 120.0, 140.0), "{:?}", line_of(&s, "Line to upper").vertices);
    assert!((sheet_of(&s, "Line to upper wall").area() - 150.0 * 30.0 * 2f64.sqrt()).abs() < 1e-6);
    project(&mut s, floor, "left", "down");
    assert!(at(&line_of(&s, "Line to LOWER sf"), 160.0, 100.0), "{:?}", line_of(&s, "Line to LOWER sf").vertices);
    assert!((sheet_of(&s, "Line to LOWER sf wall").area() - 150.0 * 10.0 * 2f64.sqrt()).abs() < 1e-6);

    // The floor is below the string, so there's nothing above it to go up to; and a string is no target.
    let why = s.run("string.project", json!({ "strings": [line], "to": on(floor), "batter": 45.0, "side": "left" })).unwrap_err().to_string();
    assert!(why.contains("'Line': the string is on or above the surface all the way along, so there's nothing to project up to"), "{why}");
    assert!(s.run("string.project", json!({ "strings": [line], "to": on(line), "batter": 45.0, "side": "left" })).is_err());
}

/// By a height. A road's edge straight in plan, level for 100 m then up at 10 %: 10 m up at 45° is the same line 10 m
/// out and 10 m higher with its grade break kept, and 10 m down on the other side is 10 m out and 10 m lower. 1 µm.
#[test]
fn a_graded_string_by_a_height_keeps_its_grades() {
    let mut s = Session::with_builtins();
    let edge = string(&mut s, "Edge", &[[0.0, 0.0, 100.0], [100.0, 0.0, 100.0], [200.0, 0.0, 110.0]]);
    let local = |line: &Polyline| -> Vec<[f64; 3]> { line.vertices.iter().map(|p| [((p[0] - E) * 1e6).round() / 1e6, ((p[1] - N) * 1e6).round() / 1e6, (p[2] * 1e6).round() / 1e6]).collect() };

    let up = s.run("string.project", json!({ "strings": [edge], "to": { "kind": "height", "height": 10.0 }, "batter": 45.0, "side": "left" })).unwrap();
    assert!(up.messages[0].starts_with("projected 1 string up at 45° up 10 m, on the left: 1 string, 3 vertices"), "{:?}", up.messages);
    assert_eq!(local(&line_of(&s, "Edge up 10 m")), [[0.0, 10.0, 110.0], [100.0, 10.0, 110.0], [200.0, 10.0, 120.0]]);
    s.run("string.project", json!({ "strings": [edge], "to": { "kind": "height", "height": 10.0 }, "batter": 45.0, "side": "right", "direction": "down" })).unwrap();
    assert_eq!(local(&line_of(&s, "Edge down 10 m")), [[0.0, -10.0, 90.0], [100.0, -10.0, 90.0], [200.0, -10.0, 100.0]]);
}

/// A string that can't be projected refuses the run, each one listed with why, and nothing is made: of three toes,
/// one is already above the RL.
#[test]
fn one_string_that_cannot_go_refuses_the_run() {
    let mut s = Session::with_builtins();
    let low = string(&mut s, "low", &[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0]]);
    let high = string(&mut s, "high", &[[0.0, -80.0, 130.0], [200.0, -80.0, 130.0]]);
    let before = s.project().objects().count();
    let why = s.run("string.project", json!({ "strings": [low, high], "to": rl(120.0), "batter": 45.0, "side": "left" })).unwrap_err().to_string();
    assert!(why.contains("1 of 2 strings can't be projected, so nothing was made. 'high': the string is on or above RL 120 all the way along"), "{why}");
    assert_eq!(s.project().objects().count(), before);
    assert!(s.run("string.project", json!({ "strings": [], "to": rl(120.0), "batter": 45.0, "side": "left" })).is_err());
    assert!(s.run("string.project", json!({ "strings": [low, low], "to": rl(120.0), "batter": 45.0, "side": "left" })).is_err());
    assert!(s.run("string.project", json!({ "strings": [low], "to": rl(120.0), "batter": 95.0, "side": "left" })).is_err());
}

/// A closed string by a height that pinches off comes back in pieces, said, with no wall: a dumbbell, two 60 m squares
/// joined by a neck 10 m wide, projected 8 m inwards closes the neck and leaves the two ends.
#[test]
fn a_projection_that_pinches_off_comes_back_in_pieces() {
    let mut s = Session::with_builtins();
    let points = [[0.0, 0.0], [60.0, 0.0], [60.0, 25.0], [100.0, 25.0], [100.0, 0.0], [160.0, 0.0], [160.0, 60.0], [100.0, 60.0], [100.0, 35.0], [60.0, 35.0], [60.0, 60.0], [0.0, 60.0]];
    let points: Vec<[f64; 3]> = points.iter().map(|p| [E + p[0], N + p[1], 100.0]).collect();
    let pit = created(&s.run("string.create", json!({ "name": "Pit", "layer": "toes", "points": points, "closed": true })).unwrap())[0];
    // Anticlockwise, so left is inside.
    let record = s.run("string.project", json!({ "strings": [pit], "to": { "kind": "height", "height": 8.0 }, "batter": 45.0, "side": "left", "direction": "down", "wall": true })).unwrap();
    assert_eq!(created(&record).len(), 2, "two pieces and no wall");
    let note = record.notes.iter().find(|n| n.code == Some(Code::StringProjectedInPieces)).expect("said");
    assert_eq!(note.text, "'Pit' down 8 m: the new string came back in 2 pieces, as it pinches off, so no wall was made for it");
    for k in 1..=2 {
        let piece = line_of(&s, &format!("Pit down 8 m ({k})"));
        assert!(piece.closed && piece.vertices.iter().all(|p| (p[2] - 92.0).abs() < 1e-9), "{:?}", piece.vertices);
    }
}

/// Timing, and a wavy grid (run with `--ignored --nocapture`, in release): fifteen toes 1.5 km long, 80 m apart, under
/// a grid of 709 × 709 nodes at 25 m, a million triangles, projected up at 45° in one run. Every vertex of each new
/// string is on its toe's batter, to 1 µm: as far north of the toe as it is above it.
#[test]
#[ignore = "timing: a million-triangle grid"]
fn fifteen_toes_to_a_million_triangle_grid() {
    let dir = std::env::temp_dir().join(format!("zd-project-big-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let wavy = |x: f64, y: f64| 150.0 + 6.0 * (x / 160.0).sin() + 5.0 * (y / 120.0).cos();
    std::fs::write(dir.join("pitlower.sfg"), vulcan_grid(709, 709, |_, _| 50.0)).unwrap();
    std::fs::write(dir.join("pitlower.srg"), vulcan_grid(709, 709, wavy)).unwrap();
    let mut s = Session::with_builtins();
    s.run("grid.import_vulcan_seam", json!({ "dir": dir.to_string_lossy(), "prefix": "pit", "seam": "lower", "layer": "geology" })).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let roof = named(&s, "LOWER sr").id;
    let toes: Vec<ObjectId> = (0..15).map(|k| string(&mut s, &format!("Toe {}", k + 1), &[[2000.0, 2000.0 + k as f64 * 80.0, 100.0], [3500.0, 2000.0 + k as f64 * 80.0, 100.0]])).collect();
    for wall in [false, true] {
        let started = Instant::now();
        let record = s.run("string.project", json!({ "strings": toes, "to": on(roof), "batter": 45.0, "side": "left", "wall": wall })).unwrap();
        println!("fifteen toes to a 709 x 709 grid, walls {wall}: {:.0} ms. {}", started.elapsed().as_secs_f64() * 1e3, record.messages[0]);
        for k in 0..15 {
            let line = line_of(&s, &format!("Toe {} to LOWER sr", k + 1));
            let y0 = N + 2000.0 + k as f64 * 80.0;
            assert!(line.vertices.len() > 60 && line.vertices.iter().all(|p| ((p[1] - y0) - (p[2] - 100.0)).abs() < 1e-6 && (p[2] - wavy(p[0] - E, p[1] - N)).abs() < 0.2), "toe {}", k + 1);
        }
        s.undo().unwrap();
    }
}
