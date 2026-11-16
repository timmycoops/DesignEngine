//! The string commands on several strings as one run (decision 0078): known answers, one undo for them all, a run
//! that can't do one string changing none, and a workflow step that takes however many strings its layer holds.

use std::time::Instant;

use serde_json::json;
use zd_commands::{Code, RunRecord, Session, Severity, WorkflowStatus};
use zd_core::{Body, ChangeKind, ObjectId, Polyline};

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

fn revision(s: &Session, id: ObjectId) -> u32 {
    s.project().object(id).unwrap().revision
}

fn string(s: &mut Session, name: &str, layer: &str, points: &[[f64; 3]], closed: bool) -> ObjectId {
    created(&s.run("string.create", json!({ "name": name, "layer": layer, "points": points, "closed": closed })).unwrap())[0]
}

/// Toe `k` (from 0): east 200 m then on 200 m east and 30 m north, 50 m down dip of the one before, at RL 100 + k.
fn toe_points(k: usize) -> [[f64; 3]; 3] {
    let (y, z) = (N - 50.0 * k as f64, 100.0 + k as f64);
    [[E, y, z], [E + 200.0, y, z], [E + 400.0, y + 30.0, z]]
}

fn toes(s: &mut Session, count: usize) -> Vec<ObjectId> {
    (0..count).map(|k| string(s, &format!("Toe {}", k + 1), "toes", &toe_points(k), false)).collect()
}

/// Fifteen toes set to RL 120 in one run: every vertex at RL 120 exactly, one record, one undo.
#[test]
fn fifteen_toes_go_to_one_rl_in_one_run_and_one_undo() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 15);
    let record = s.run("string.set_elevation", json!({ "strings": ids, "rl": 120.0 })).unwrap();
    assert_eq!(record.outputs.len(), 15);
    assert!(record.outputs.iter().all(|o| o.change == ChangeKind::Revised));
    assert!(ids.iter().all(|&id| line(&s, id).vertices.iter().all(|v| v[2] == 120.0)), "exactly RL 120");
    // What the run did to how many, then each string's own line, on it.
    assert_eq!(record.messages.len(), 16);
    assert_eq!(record.messages[0], "set to RL 120.000: 15 of 15 strings");
    assert_eq!(record.messages[1], "'Toe 1': vertices 1 to 3 set to RL 120.000");
    assert!(record.notes[1..].iter().zip(&ids).all(|(note, &id)| note.object == Some(id)));

    // Again: nothing to do, which is said and isn't a refusal (a workflow run again mustn't stop here).
    let again = s.run("string.set_elevation", json!({ "strings": ids, "rl": 120.0 })).unwrap();
    assert!(again.outputs.is_empty());
    assert!(again.messages[0].starts_with("set to RL 120.000: 0 of 15 strings; 15 came out as they were and are left: 'Toe 1', "), "{}", again.messages[0]);
    assert!(again.messages[0].ends_with("'Toe 10' and 5 more"), "{}", again.messages[0]);
    assert!(ids.iter().all(|&id| revision(&s, id) == 2));

    // The run that changed nothing is no step to undo: one undo takes back the fifteen.
    s.undo().unwrap();
    for (k, &id) in ids.iter().enumerate() {
        assert_eq!(line(&s, id).vertices, toe_points(k), "one undo puts all fifteen back");
        assert_eq!(revision(&s, id), 1);
    }
}

/// Three straight lines running east, offset 40 m to the right twice: each new line is 40 m and 80 m south of its own,
/// to 1e-9 m. A line running west goes north, its own right, and is warned of.
#[test]
fn lines_are_each_offset_to_their_own_right() {
    let mut s = Session::with_builtins();
    let east = |y: f64| [[E, N + y, 50.0], [E + 300.0, N + y, 50.0]];
    let ids: Vec<ObjectId> = [0.0, 100.0, 200.0].iter().enumerate().map(|(k, &y)| string(&mut s, &format!("L{}", k + 1), "lines", &east(y), false)).collect();
    let record = s.run("string.offset", json!({ "strings": ids, "distance": 40.0, "side": "right", "times": 2 })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 6);
    assert_eq!(record.messages, ["offset 40 m to the right, 2 times with mitred corners: 6 string(s) from 3, 12 vertices"]);
    for (k, y) in [0.0, 100.0, 200.0].into_iter().enumerate() {
        for time in 1..=2 {
            let new = made[k * 2 + time - 1];
            assert_eq!(s.project().object(new).unwrap().name, format!("L{} offset {} m right", k + 1, 40 * time));
            let v = line(&s, new).vertices;
            let want = [[E, N + y - 40.0 * time as f64, 50.0], [E + 300.0, N + y - 40.0 * time as f64, 50.0]];
            assert!(v.len() == 2 && v.iter().zip(&want).all(|(a, b)| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)), "{v:?}");
        }
    }
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), 3, "one undo takes back all six");

    // One running west among them: its right is north.
    let west = string(&mut s, "West", "lines", &[[E + 300.0, N + 300.0, 50.0], [E, N + 300.0, 50.0]], false);
    let record = s.run("string.offset", json!({ "strings": [ids[0], west], "distance": 40.0, "side": "right" })).unwrap();
    let north = line(&s, created(&record)[1]).vertices;
    assert!(north.iter().all(|v| (v[1] - (N + 340.0)).abs() < 1e-9), "{north:?}");
    let warned: Vec<_> = record.notes.iter().filter(|n| n.severity == Severity::Warning).collect();
    assert_eq!(warned.len(), 1);
    assert_eq!((warned[0].code, warned[0].object), (Some(Code::StringRunsOtherWay), Some(west)));
    assert_eq!(warned[0].text, "'West' runs the other way to 'L1', so its right is the other side: reverse it first if that's not meant");

    // Two squares, one drawn clockwise and one anticlockwise: the same warning.
    let square = |x: f64, clockwise: bool| {
        let mut p = vec![[E + x, N + 1000.0, 0.0], [E + x + 100.0, N + 1000.0, 0.0], [E + x + 100.0, N + 1100.0, 0.0], [E + x, N + 1100.0, 0.0]];
        if clockwise {
            p.reverse();
        }
        p
    };
    let (a, b) = (string(&mut s, "A", "pads", &square(0.0, false), true), string(&mut s, "B", "pads", &square(500.0, true), true));
    let record = s.run("string.offset", json!({ "strings": [a, b], "distance": 10.0, "side": "left" })).unwrap();
    assert_eq!(record.notes.iter().filter(|n| n.code == Some(Code::StringRunsOtherWay)).map(|n| n.object).collect::<Vec<_>>(), [Some(b)]);

    // A name of your own is for one string.
    let refused = s.run("string.offset", json!({ "strings": ids, "distance": 40.0, "side": "right", "name": "S" })).unwrap_err().to_string();
    assert_eq!(refused, "a name of your own (`name`) is for one string; 3 were given");
}

/// Five toes draped on a plane z = 50 + 0.1x + 0.05y (x, y from the origin): every vertex on the plane to 1e-9 m. One
/// that runs off the surface is draped where it can be and named, with how many vertices are off.
#[test]
fn strings_drape_onto_a_plane_in_one_run() {
    let mut s = Session::with_builtins();
    let plane = |x: f64, y: f64| 50.0 + 0.1 * (x - E) + 0.05 * (y - N);
    let corners = [[E - 100.0, N - 500.0], [E + 600.0, N - 500.0], [E + 600.0, N + 200.0], [E - 100.0, N + 200.0]];
    let vertices: Vec<[f64; 3]> = corners.iter().map(|c| [c[0], c[1], plane(c[0], c[1])]).collect();
    let surface = created(&s.run("surface.from_mesh", json!({ "name": "Plane", "layer": "surfaces", "vertices": vertices, "triangles": [[0, 1, 2], [0, 2, 3]] })).unwrap())[0];
    let ids = toes(&mut s, 5);
    let record = s.run("string.drape", json!({ "strings": ids, "surface": surface })).unwrap();
    assert_eq!(record.messages[0], "draped on Plane: 5 of 5 strings");
    assert_eq!(record.outputs.len(), 5);
    for &id in &ids {
        let v = line(&s, id).vertices;
        assert!(v.len() >= 3 && v.iter().all(|p| (p[2] - plane(p[0], p[1])).abs() < 1e-9), "{v:?}");
    }

    let off = string(&mut s, "Off", "toes", &[[E + 500.0, N, 7.0], [E + 700.0, N, 7.0]], false);
    let record = s.run("string.drape", json!({ "strings": [ids[0], off], "surface": surface })).unwrap();
    assert_eq!(record.messages[0], "draped on Plane: 1 of 2 strings; 1 came out as it was and is left: 'Toe 1'");
    let warned: Vec<_> = record.notes.iter().filter(|n| n.severity == Severity::Warning).collect();
    assert_eq!(warned.len(), 1);
    assert_eq!((warned[0].code, warned[0].object, warned[0].value.as_ref().map(|m| m.value)), (Some(Code::StringOffSurface), Some(off), Some(1.0)));
    assert!(warned[0].text.starts_with("'Off': draped on Plane: "), "{}", warned[0].text);
    let v = line(&s, off).vertices;
    assert!((v[0][2] - plane(E + 500.0, N)).abs() < 1e-9 && v.last().unwrap()[2] == 7.0, "{v:?}");
}

/// Five strings, one that can't be closed (two vertices): the run is refused, the one named with why, and all five are
/// as they were, at the same revisions. The same for an offset that leaves nothing of one string.
#[test]
fn one_string_that_cannot_be_done_refuses_the_run_and_changes_nothing() {
    let mut s = Session::with_builtins();
    let mut ids = toes(&mut s, 4);
    ids.insert(2, string(&mut s, "Short", "toes", &[[E, N + 500.0, 0.0], [E + 100.0, N + 500.0, 0.0]], false));
    let refused = s.run("string.set_closed", json!({ "strings": ids, "closed": true })).unwrap_err();
    assert_eq!(refused.to_string(), "1 of 5 strings can't be done, so nothing was changed. 'Short': a closed string needs at least 3 vertices");
    assert!(ids.iter().all(|&id| revision(&s, id) == 1 && !line(&s, id).closed));
    assert_eq!(s.project().objects().count(), 5);

    // A U whose inside is narrower than twice 10 m, among the toes.
    let u = string(&mut s, "U", "toes", &[[E, N + 900.0, 0.0], [E, N + 800.0, 0.0], [E + 15.0, N + 800.0, 0.0], [E + 15.0, N + 900.0, 0.0]], false);
    let before = s.project().objects().count();
    let refused = s.run("string.offset", json!({ "strings": [ids[0], u, ids[1]], "distance": 10.0, "side": "left", "times": 2 })).unwrap_err().to_string();
    assert!(refused.starts_with("1 of 3 strings can't be done, so nothing was changed. 'U': at 10 m: "), "{refused}");
    assert_eq!(s.project().objects().count(), before);

    // Settings that name a vertex are for one string, and an id is given once, one way.
    for (command, params, why) in [
        ("string.set_grade", json!({ "strings": [ids[0], ids[1]], "grade": 2.0, "from": 1 }), "a grade between two vertices (`from`, `to`) is for one string; 2 were given"),
        ("string.set_elevation", json!({ "strings": [ids[0], ids[1]], "rl": 5.0, "to": 1 }), "an elevation between two vertices (`from`, `to`) is for one string; 2 were given"),
        ("string.reverse", json!({ "string": ids[0], "strings": [ids[1]] }), "give `string` or `strings`, not both"),
        ("string.reverse", json!({}), "no string to work on: give one as `string`, or several as `strings`"),
    ] {
        assert_eq!(s.run(command, params).unwrap_err().to_string(), why);
    }
    assert!(s.run("string.smooth", json!({ "strings": [ids[0], ids[0]] })).unwrap_err().to_string().ends_with("is in `strings` twice"));
    assert!(ids.iter().all(|&id| revision(&s, id) == 1));
}

/// One string given as `string` and as `strings` comes out the same, vertex for vertex. Only what the run says differs.
#[test]
fn one_string_is_the_same_given_either_way() {
    let run = |params: fn(ObjectId) -> serde_json::Value| {
        let mut s = Session::with_builtins();
        let id = string(&mut s, "toe", "design", &[[E, N, 50.0], [E + 100.0, N, 50.0], [E + 100.0, N + 100.0, 50.0]], false);
        let record = s.run("string.smooth", params(id)).unwrap();
        (line(&s, id), record.messages)
    };
    let (one, said_one) = run(|id| json!({ "string": id, "spacing": 10.0 }));
    let (listed, said_listed) = run(|id| json!({ "strings": [id], "spacing": 10.0 }));
    assert_eq!(one, listed);
    assert_eq!(one.vertices.len(), 21);
    assert_eq!(said_one, ["smoothed at 10 m: 21 vertices, was 3"]);
    assert_eq!(said_listed, ["smoothed at 10 m: 1 of 1 strings", "'toe': smoothed at 10 m: 21 vertices, was 3"]);
}

/// Close among several: the ones already closed are left and said; one string given alone that's already closed is
/// still a mistake to say.
#[test]
fn closing_several_leaves_the_closed_ones_and_says_so() {
    let mut s = Session::with_builtins();
    let ring = string(&mut s, "Ring", "toes", &toe_points(9), true);
    let open = toes(&mut s, 2);
    let all = [open[0], ring, open[1]];
    let record = s.run("string.set_closed", json!({ "strings": all, "closed": true })).unwrap();
    assert_eq!(record.messages, ["closed: 2 of 3 strings; 1 came out as it was and is left: 'Ring'"]);
    assert_eq!(record.outputs.iter().map(|o| o.id).collect::<Vec<_>>(), open);
    assert!(all.iter().all(|&id| line(&s, id).closed));
    assert!(s.run("string.set_closed", json!({ "strings": all, "closed": true })).unwrap().outputs.is_empty());
    assert_eq!(s.run("string.set_closed", json!({ "string": ring, "closed": true })).unwrap_err().to_string(), "'Ring' is already closed");
    let opened = s.run("string.set_closed", json!({ "strings": all, "closed": false })).unwrap();
    assert_eq!(opened.messages, ["opened: 3 of 3 strings"]);
}

/// Grade, reverse, smooth and filter on several. Each toe graded at 2 % from its own RL ends 0.02 x its plan length
/// higher (200 m + the 202.237 m leg), to 1e-9 m; with an RL given, each starts from that.
#[test]
fn grade_reverse_smooth_and_filter_take_several() {
    let mut s = Session::with_builtins();
    let ids = toes(&mut s, 3);
    let length = 200.0 + (200.0f64 * 200.0 + 30.0 * 30.0).sqrt();
    s.run("string.set_grade", json!({ "strings": ids, "grade": 2.0 })).unwrap();
    for (k, &id) in ids.iter().enumerate() {
        let v = line(&s, id).vertices;
        assert_eq!(v[0][2], 100.0 + k as f64);
        assert!((v[1][2] - (100.0 + k as f64 + 4.0)).abs() < 1e-9 && (v[2][2] - (100.0 + k as f64 + 0.02 * length)).abs() < 1e-9, "{v:?}");
    }
    let record = s.run("string.set_grade", json!({ "strings": ids, "grade": -1.0, "rl": 90.0 })).unwrap();
    assert_eq!(record.messages[0], "graded at -1 %: 3 of 3 strings");
    assert!(ids.iter().all(|&id| line(&s, id).vertices[0][2] == 90.0 && (line(&s, id).vertices[2][2] - (90.0 - 0.01 * length)).abs() < 1e-9));

    let firsts: Vec<[f64; 3]> = ids.iter().map(|&id| line(&s, id).vertices[0]).collect();
    assert_eq!(s.run("string.reverse", json!({ "strings": ids })).unwrap().messages, ["reversed: 3 of 3 strings"]);
    assert!(ids.iter().zip(&firsts).all(|(&id, first)| line(&s, id).vertices[2] == *first));

    let smoothed = s.run("string.smooth", json!({ "strings": ids, "spacing": 10.0 })).unwrap();
    assert_eq!(smoothed.messages[0], "smoothed at 10 m: 3 of 3 strings");
    assert!(ids.iter().all(|&id| line(&s, id).vertices.len() > 40));
    let filtered = s.run("string.filter", json!({ "strings": ids, "tolerance": 50.0 })).unwrap();
    assert_eq!(filtered.messages[0], "filtered at 50 m: 3 of 3 strings");
    assert!(ids.iter().all(|&id| line(&s, id).vertices.len() < 10));
    // A tolerance no string can take refuses the run, each string listed.
    let refused = s.run("string.filter", json!({ "strings": ids, "tolerance": -1.0 })).unwrap_err().to_string();
    assert!(refused.starts_with("3 of 3 strings can't be done, so nothing was changed. 'Toe 1': "), "{refused}");
}

/// A step captured on the three toes on a layer finds its strings by the layer, so with two more toes there it runs on
/// five.
#[test]
fn a_workflow_step_captured_on_three_toes_runs_on_five() {
    let mut s = Session::with_builtins();
    let three = toes(&mut s, 3);
    let run = s.run("string.set_elevation", json!({ "strings": three, "rl": 120.0 })).unwrap();
    let workflow = created(&s.capture_workflow("Level the toes", &[run.run]).unwrap())[0];
    let more: Vec<ObjectId> = (3..5).map(|k| string(&mut s, &format!("Toe {}", k + 1), "toes", &toe_points(k), false)).collect();
    let record = s.run_workflow(workflow, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished);
    let on_the_layer = three.iter().chain(&more).filter(|&&id| line(&s, id).vertices.iter().all(|v| v[2] == 120.0)).count();
    assert_eq!(on_the_layer, 5, "the two toes added after the capture are set too");
}

/// Timings for the decision record: 15 and 150 strings of 40 vertices in one run, on a grid of 97 x 65 nodes and on a
/// surface of about a million triangles. Run with `cargo test --release -p zd-commands --test strings_many -- --ignored --nocapture`.
#[test]
#[ignore = "timing; run in release"]
fn timings_for_many_strings_in_one_run() {
    let mut s = Session::with_builtins();
    let dir = std::env::temp_dir().join(format!("zd-strings-many-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut xyz = String::new();
    for row in 0..65 {
        for col in 0..97 {
            let (x, y) = (col as f64 * 25.0, row as f64 * 25.0);
            xyz += &format!("{} {} {}\n", E + x, N + y, 80.0 + 10.0 * (x / 300.0).sin() + 8.0 * (y / 250.0).cos());
        }
    }
    std::fs::write(dir.join("grid.xyz"), xyz).unwrap();
    let grid = created(&s.run("surface.import", json!({ "path": dir.join("grid.xyz").to_string_lossy(), "layer": "surfaces" })).unwrap())[0];
    std::fs::remove_dir_all(&dir).unwrap();
    let n = 708usize;
    let step = 2400.0 / (n - 1) as f64;
    let vertices: Vec<[f64; 3]> = (0..n * n).map(|i| ((i % n) as f64 * step, (i / n) as f64 * step)).map(|(x, y)| [E + x, N + y, 80.0 + 10.0 * (x / 300.0).sin() + 8.0 * (y / 250.0).cos()]).collect();
    let triangles: Vec<[usize; 3]> = (0..(n - 1) * (n - 1)).map(|i| (i / (n - 1)) * n + i % (n - 1)).flat_map(|i| [[i, i + 1, i + n + 1], [i, i + n + 1, i + n]]).collect();
    let count = triangles.len();
    let big = created(&s.run("surface.from_mesh", json!({ "name": "Big", "layer": "surfaces", "vertices": vertices, "triangles": triangles })).unwrap())[0];

    for strings in [15usize, 150] {
        let ids: Vec<ObjectId> = (0..strings)
            .map(|k| {
                let y = N + 100.0 + 1300.0 * k as f64 / strings as f64;
                let points: Vec<[f64; 3]> = (0..40).map(|i| [E + 100.0 + 2200.0 * i as f64 / 39.0, y + 20.0 * (i as f64 / 4.0).sin(), 0.0]).collect();
                string(&mut s, &format!("T{strings}-{k}"), &format!("toes {strings}"), &points, false)
            })
            .collect();
        let mut time = |what: &str, command: &str, params: serde_json::Value| {
            let started = Instant::now();
            let record = s.run(command, params).unwrap();
            let after: usize = ids.iter().map(|&id| line(&s, id).vertices.len()).sum();
            println!("{strings} strings, {what}: {:.1} ms, {after} vertices after, {} revised", started.elapsed().as_secs_f64() * 1e3, record.outputs.len());
        };
        time("drape on the 97 x 65 grid", "string.drape", json!({ "strings": ids, "surface": grid }));
        time(&format!("drape on the {count} triangle surface"), "string.drape", json!({ "strings": ids, "surface": big }));
        time("set elevation", "string.set_elevation", json!({ "strings": ids, "rl": 100.0 }));
        time("filter at 0.5 m", "string.filter", json!({ "strings": ids, "tolerance": 0.5 }));
        time("smooth at 5 m", "string.smooth", json!({ "strings": ids, "spacing": 5.0 }));
        time("reverse", "string.reverse", json!({ "strings": ids }));
    }
}
