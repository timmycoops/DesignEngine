//! Walls through the registry, as the app calls them, then stepping between real grids. The second is
//! opt-in, because the data stays local: `ZD_WALL_GRIDS` lists Vulcan grid files, bottom of the wall
//! first, separated by `;`.

use std::path::Path;
use std::time::Instant;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, Grid, ObjectId, Polyline, TriMesh};
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
