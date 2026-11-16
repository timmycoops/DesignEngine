//! The Strings panel's tools as workflow steps (decision 0078, workstream 4): a step added on the strings picked finds
//! them again by rule, the strings of a layer by the layer, so it runs on however many are there the next time; what an
//! earlier step made by that step; and strings picked from among others on their layer as picked, said.

use serde_json::json;
use zd_commands::{Code, RunRecord, Session, WorkflowStatus};
use zd_core::{Body, ChangeKind, InputRule, ObjectId, Polyline, Source, StepAction, Workflow};

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

fn workflow(s: &Session, id: ObjectId) -> Workflow {
    match &s.project().object(id).expect("the workflow").body {
        Body::Workflow(w) => (**w).clone(),
        _ => panic!("a workflow"),
    }
}

fn rule(s: &Session, id: ObjectId, step: usize, input: &str) -> InputRule {
    match &workflow(s, id).steps[step].action {
        StepAction::Command { inputs, .. } => inputs[input].clone(),
        StepAction::Pause => panic!("a command step"),
    }
}

/// A new workflow with no steps yet, as the panel adds to.
fn empty_workflow(s: &mut Session, name: &str) -> ObjectId {
    created(&s.run("workflow.save", json!({ "name": name, "content": { "steps": [], "cases": [] } })).unwrap())[0]
}

/// A toe 500 m long running east at RL 0, `y` metres north of the pit's corner.
fn toe(s: &mut Session, name: &str, layer: &str, y: f64) -> ObjectId {
    let points: Vec<[f64; 3]> = (0..6).map(|k| [E + 100.0 * k as f64, N + y, 0.0]).collect();
    created(&s.run("string.create", json!({ "name": name, "layer": layer, "points": points })).unwrap())[0]
}

/// The plane z = 100 + 0.1 x + 0.05 y over 1 km square round the toes, x and y from the pit's corner.
fn plane_z(x: f64, y: f64) -> f64 {
    100.0 + 0.1 * (x - E) + 0.05 * (y - N)
}

fn ground(s: &mut Session) -> ObjectId {
    let corners: Vec<[f64; 3]> = [(-200.0, -200.0), (800.0, -200.0), (800.0, 800.0), (-200.0, 800.0)].iter().map(|&(x, y)| [E + x, N + y, plane_z(E + x, N + y)]).collect();
    created(&s.run("surface.from_mesh", json!({ "name": "Ground", "layer": "surfaces", "vertices": corners, "triangles": [[0, 1, 2], [0, 2, 3]] })).unwrap())[0]
}

fn on_the_plane(s: &Session, id: ObjectId) -> bool {
    // Tolerance 1e-9 m: a plane of two triangles, so a draped vertex sits on it to rounding.
    line(s, id).vertices.iter().all(|v| (v[2] - plane_z(v[0], v[1])).abs() < 1e-9)
}

/// Three toes picked, the only strings on their layer: the step finds them by the layer, so after two more toes are
/// drawn on it the workflow drapes all five, in one run and one undo.
#[test]
fn a_drape_added_on_the_toes_of_a_layer_drapes_every_toe_there_next_time() {
    let mut s = Session::with_builtins();
    let surface = ground(&mut s);
    let toes: Vec<ObjectId> = (0..3).map(|k| toe(&mut s, &format!("Toe {}", k + 1), "toes", 60.0 * k as f64)).collect();
    let id = empty_workflow(&mut s, "Toes");
    let record = s.add_workflow_step(id, None, "string.drape", &json!({ "strings": toes, "surface": surface, "crossings": false }), "").unwrap();
    assert!(record.notes.iter().all(|n| n.code != Some(Code::WorkflowPicked)), "found by rule: {:?}", record.notes);
    assert_eq!(rule(&s, id, 0, "strings"), InputRule { from: Source::Layer("toes".into()), kind: None, filter: Default::default(), many: true });
    assert_eq!(rule(&s, id, 0, "surface").from, Source::Layer("surfaces".into()));

    let more: Vec<ObjectId> = (3..5).map(|k| toe(&mut s, &format!("Toe {}", k + 1), "toes", 60.0 * k as f64)).collect();
    let run = s.run_workflow(id, 1).unwrap();
    assert_eq!(run.status, WorkflowStatus::Finished);
    let all: Vec<ObjectId> = toes.iter().chain(&more).copied().collect();
    assert!(all.iter().all(|&t| on_the_plane(&s, t)), "every toe on the layer draped");
    let last = workflow(&s, id).steps[0].last.clone().unwrap();
    assert_eq!(last.revised.len(), 5, "five toes revised");

    // One undo takes the whole run back: every toe at RL 0 again.
    s.undo().unwrap();
    assert!(all.iter().all(|&t| line(&s, t).vertices.iter().all(|v| v[2] == 0.0)));
}

/// Three toes picked from five on their layer: no rule finds those three alone, so the step keeps them as picked and
/// the save says so; the run drapes those three and leaves the other two.
#[test]
fn toes_picked_from_among_others_on_their_layer_are_kept_as_picked_and_said() {
    let mut s = Session::with_builtins();
    let surface = ground(&mut s);
    let toes: Vec<ObjectId> = (0..5).map(|k| toe(&mut s, &format!("Toe {}", k + 1), "toes", 60.0 * k as f64)).collect();
    let id = empty_workflow(&mut s, "Toes");
    let picked = &toes[1..4];
    let record = s.add_workflow_step(id, None, "string.drape", &json!({ "strings": picked, "surface": surface, "crossings": false }), "").unwrap();
    let warned: Vec<_> = record.notes.iter().filter(|n| n.code == Some(Code::WorkflowPicked)).collect();
    assert_eq!(warned.len(), 1, "{:?}", record.notes);
    assert!(warned[0].text.contains("'strings' is the 3 objects as picked"), "{}", warned[0].text);
    assert_eq!(rule(&s, id, 0, "strings").from, Source::Picked(picked.to_vec()));

    assert_eq!(s.run_workflow(id, 1).unwrap().status, WorkflowStatus::Finished);
    assert!(picked.iter().all(|&t| on_the_plane(&s, t)));
    assert!([toes[0], toes[4]].iter().all(|&t| line(&s, t).vertices.iter().all(|v| v[2] == 0.0)), "the two not picked are left");
}

/// A base toe offset three times by a step, then the offsets set to RL 120 by a step added on what the first made: the
/// second finds them as what step 1 made, and a run again makes the same three toes as their next revisions, at RL 120.
#[test]
fn a_step_on_what_an_earlier_step_made_takes_it_from_that_step() {
    let mut s = Session::with_builtins();
    let base = toe(&mut s, "Base toe", "base", 0.0);
    let id = empty_workflow(&mut s, "Strips");
    s.add_workflow_step(id, None, "string.offset", &json!({ "strings": [base], "distance": 60.0, "side": "left", "times": 3, "corners": "mitre" }), "").unwrap();
    assert_eq!(rule(&s, id, 0, "strings").from, Source::Layer("base".into()));
    assert_eq!(s.run_workflow(id, 1).unwrap().status, WorkflowStatus::Finished);
    let made = workflow(&s, id).steps[0].last.clone().unwrap().made;
    assert_eq!(made.len(), 3);

    s.add_workflow_step(id, None, "string.set_elevation", &json!({ "strings": made, "rl": 120.0 }), "").unwrap();
    assert_eq!(rule(&s, id, 1, "strings"), InputRule { from: Source::Step(1), kind: None, filter: Default::default(), many: true });
    assert_eq!(s.run_workflow(id, 1).unwrap().status, WorkflowStatus::Finished);
    let w = workflow(&s, id);
    assert_eq!(w.steps[0].last.as_ref().unwrap().made, made, "the same three toes, as their next revisions");
    for (k, &t) in made.iter().enumerate() {
        let toe = line(&s, t);
        // Offsets of a straight toe: exactly 60 m apart to the left (north), each at RL 120.
        assert!(toe.vertices.iter().all(|v| v[2] == 120.0 && (v[1] - (N + 60.0 * (k + 1) as f64)).abs() < 1e-9), "toe {k}: {:?}", toe.vertices);
    }
}

/// A toe in three pieces on a layer, joined by a step found by the layer: the pieces come in the project's order, so the
/// first drawn keeps the toe and the way it runs.
#[test]
fn a_join_step_takes_the_pieces_on_a_layer_the_first_drawn_keeping_the_toe() {
    let mut s = Session::with_builtins();
    let toe: Vec<[f64; 3]> = (0..7).map(|k| [E + 80.0 * k as f64, N + 5.0 * (k * k) as f64, 50.0 + k as f64]).collect();
    let piece = |s: &mut Session, name: &str, points: &[[f64; 3]]| created(&s.run("string.create", json!({ "name": name, "layer": "pieces", "points": points })).unwrap())[0];
    let a = piece(&mut s, "Toe", &toe[0..=2]);
    let b = piece(&mut s, "Piece B", &toe[2..=4]);
    let c = piece(&mut s, "Piece C", &toe[4..=6]);
    let id = empty_workflow(&mut s, "Join");
    // As the panel sends it: the one picked last first.
    s.add_workflow_step(id, None, "string.join", &json!({ "strings": [c, a, b] }), "").unwrap();
    assert_eq!(rule(&s, id, 0, "strings").from, Source::Layer("pieces".into()));
    assert_eq!(s.run_workflow(id, 1).unwrap().status, WorkflowStatus::Finished);
    assert_eq!(line(&s, a), Polyline { vertices: toe, closed: false }, "the toe exactly, in the first drawn");
    assert!(s.project().object(b).is_none() && s.project().object(c).is_none());
}

/// Number across as a step: the line drawn across is a setting, and the toes are found by their layer, so five toes
/// drawn where there were three are numbered 1 to 5 from where the line starts.
#[test]
fn number_across_as_a_step_numbers_every_toe_on_the_layer_next_time() {
    let mut s = Session::with_builtins();
    let toes: Vec<ObjectId> = (0..3).map(|k| toe(&mut s, &format!("t{k}"), "toes", 60.0 * k as f64)).collect();
    let id = empty_workflow(&mut s, "Numbers");
    let params = json!({ "strings": toes, "from": [E + 250.0, N - 30.0, 0.0], "to": [E + 250.0, N + 400.0, 0.0], "start": 1, "step": 1, "name": "Strip #" });
    s.add_workflow_step(id, None, "string.number_across", &params, "").unwrap();
    assert_eq!(rule(&s, id, 0, "strings").from, Source::Layer("toes".into()));
    let StepAction::Command { settings, .. } = &workflow(&s, id).steps[0].action else { panic!() };
    assert_eq!(settings["from"], params["from"], "the line across is a setting");

    let more: Vec<ObjectId> = (3..5).map(|k| toe(&mut s, &format!("t{k}"), "toes", 60.0 * k as f64)).collect();
    assert_eq!(s.run_workflow(id, 1).unwrap().status, WorkflowStatus::Finished);
    let names: Vec<String> = toes.iter().chain(&more).map(|&t| s.project().object(t).unwrap().name.clone()).collect();
    assert_eq!(names, ["Strip 1", "Strip 2", "Strip 3", "Strip 4", "Strip 5"]);
}
