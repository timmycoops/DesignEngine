//! Workflows (decision 0065): runs captured from the run log as steps, their object ids turned into rules, saved as a
//! workflow object; every rule found again gives the objects the runs were given.

use std::time::Instant;

use serde_json::json;
use zd_commands::workflow::resolve;
use zd_commands::{Code, LogEntry, RunRecord, Session, Severity, StepOutcome, WorkflowStatus};
use zd_core::{AttrValue, Body, ChangeKind, InputRule, ObjectId, ObjectKind, Source, StepAction, Workflow};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn workflow(s: &Session, id: ObjectId) -> Workflow {
    match &s.project().object(id).expect("the workflow").body {
        Body::Workflow(w) => (**w).clone(),
        _ => panic!("a workflow"),
    }
}

fn volume(s: &Session, id: ObjectId) -> f64 {
    match &s.project().object(id).unwrap().body {
        Body::Solid(solid) => solid.volume(),
        _ => panic!("a solid"),
    }
}

fn inputs_of(w: &Workflow, step: usize) -> std::collections::BTreeMap<String, InputRule> {
    match &w.steps[step].action {
        StepAction::Command { inputs, .. } => inputs.clone(),
        StepAction::Pause => panic!("a command step"),
    }
}

/// A flat surface at `z` over 400 m by 400 m round the pit.
fn flat(s: &mut Session, name: &str, z: f64) -> ObjectId {
    let (x0, y0, x1, y1) = (E - 100.0, N - 100.0, E + 300.0, N + 300.0);
    let record = s
        .run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]], "triangles": [[0, 1, 2], [0, 2, 3]] }))
        .unwrap();
    created(&record)[0]
}

/// A 100 m by 60 m pit from RL 0 to 100 (600,000 m³), cut at RL 40 into 240,000 m³ below and 360,000 m³ above. Returns
/// the session, the solid run and the cut run.
fn box_pit() -> (Session, RunRecord, RunRecord) {
    let mut s = Session::with_builtins();
    let top = flat(&mut s, "Top", 100.0);
    let bottom = flat(&mut s, "Bottom", 0.0);
    let outline = s
        .run("string.create", json!({ "name": "Outline", "layer": "outline", "points": [[E, N, 0.0], [E + 100.0, N, 0.0], [E + 100.0, N + 60.0, 0.0], [E, N + 60.0, 0.0]], "closed": true }))
        .unwrap();
    let solid = s.run("solid.between_surfaces", json!({ "top": top, "bottom": bottom, "string": created(&outline)[0], "layer": "pit" })).unwrap();
    let pit = created(&solid)[0];
    // Volume tolerance 1e-6 m³: flat faces and a rectangle, so the solid is exact.
    assert!((volume(&s, pit) - 600_000.0).abs() < 1e-6, "pit {}", volume(&s, pit));
    // Something run in between, not captured.
    s.run("string.create", json!({ "name": "Scratch", "layer": "scratch", "points": [[E, N, 0.0], [E + 10.0, N, 0.0]] })).unwrap();
    let cut = s.run("solid.cut_by_surface", json!({ "solids": [pit], "rl": 40.0, "keep": "both", "layer": "cut" })).unwrap();
    (s, solid, cut)
}

#[test]
fn capture_turns_ids_into_rules_that_find_them_again() {
    let (mut s, solid, cut) = box_pit();
    let started = Instant::now();
    let record = s.capture_workflow("Box pit", &[solid.run, cut.run]).unwrap();
    println!("captured 2 runs in {:.2} ms", started.elapsed().as_secs_f64() * 1e3);
    let id = created(&record)[0];
    assert_eq!(s.project().layer(s.project().object(id).unwrap().layer).unwrap().name, "workflows");
    assert!(record.notes.iter().all(|n| n.severity == Severity::Info), "no picked fallback: {:?}", record.notes);
    let w = workflow(&s, id);
    assert_eq!(w.steps.len(), 2);

    // The solid: the top and bottom share a layer, so they're found by name; the outline is alone on its layer.
    let first = inputs_of(&w, 0);
    assert_eq!(first["top"], InputRule { from: Source::Named("Top".into()), kind: Some(ObjectKind::Surface), filter: Default::default(), many: false });
    assert_eq!(first["bottom"].from, Source::Named("Bottom".into()));
    assert_eq!(first["string"].from, Source::Layer("outline".into()));
    let StepAction::Command { command, settings, .. } = &w.steps[0].action else { panic!() };
    assert_eq!(command, "solid.between_surfaces");
    assert_eq!(settings["layer"], json!("pit"));
    assert!(settings.get("top").is_none(), "inputs aren't settings");

    // The cut takes what the first step made.
    let second = inputs_of(&w, 1);
    assert_eq!(second["solids"], InputRule { from: Source::Step(1), kind: None, filter: Default::default(), many: true });
    let StepAction::Command { settings, .. } = &w.steps[1].action else { panic!() };
    assert_eq!((settings["rl"].as_f64(), settings["keep"].as_str()), (Some(40.0), Some("both")));

    // Each step keeps its run as its last, and every rule finds what the run was given.
    assert_eq!(w.steps[0].last.as_ref().unwrap().made, created(&solid));
    assert_eq!(w.steps[1].last.as_ref().unwrap().made, created(&cut));
    for (at, step) in w.steps.iter().enumerate() {
        let StepAction::Command { inputs, .. } = &step.action else { panic!() };
        let run = if at == 0 { &solid } else { &cut };
        for (field, rule) in inputs {
            let found = resolve(s.project(), &w, at, rule).unwrap();
            let given: Vec<ObjectId> = match &run.params[field] {
                serde_json::Value::Array(ids) => ids.iter().map(|v| ObjectId::parse(v.as_str().unwrap()).unwrap()).collect(),
                v => vec![ObjectId::parse(v.as_str().unwrap()).unwrap()],
            };
            assert_eq!(found, given, "step {} input {field}", at + 1);
        }
    }
    // The pieces add back to the pit, 240,000 + 360,000 m³ (to 1e-6 m³).
    let pieces: f64 = created(&cut).iter().map(|&p| volume(&s, p)).sum();
    assert!((pieces - 600_000.0).abs() < 1e-6, "pieces {pieces}");
}

#[test]
fn a_workflow_saves_and_opens_as_it_was() {
    let (mut s, solid, cut) = box_pit();
    let id = created(&s.capture_workflow("Box pit", &[solid.run, cut.run]).unwrap())[0];
    let before = workflow(&s, id);
    let dir = std::env::temp_dir().join(format!("zd-workflow-{}", ObjectId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Box pit.zdp");
    s.save(&path).unwrap();
    let opened = Session::open(&path).unwrap();
    assert_eq!(workflow(&opened, id), before);
    assert_eq!(opened.project().object(id).unwrap().name, "Box pit");
    // Capture still works after opening: the log carries on, the revisions before don't.
    let again = opened.captured(&[solid.run, cut.run]).unwrap();
    assert_eq!(inputs_of(&again, 1)["solids"].from, Source::Step(1));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn capture_picks_one_of_a_step_by_an_attribute_it_alone_has() {
    let mut s = Session::with_builtins();
    let mut toes = Vec::new();
    for k in 0..3 {
        let y = N + 60.0 * k as f64;
        toes.extend(created(&s.run("string.create", json!({ "name": format!("Toe {k}"), "layer": "toes", "points": [[E, y, 0.0], [E + 500.0, y, 0.0]] })).unwrap()));
    }
    let numbered = s.run("string.number_across", json!({ "strings": toes, "from": [E + 250.0, N - 30.0, 0.0], "to": [E + 250.0, N + 200.0, 0.0], "start": 1.0 })).unwrap();
    assert_eq!(numbered.outputs.iter().filter(|o| o.change == ChangeKind::Revised).count(), 3);
    let strip2 = s.project().objects().find(|o| o.attributes.get("strip") == Some(&AttrValue::Number(2.0))).unwrap().id;
    let offset = s.run("string.offset", json!({ "string": strip2, "distance": 10.0, "side": "left", "layer": "offsets" })).unwrap();
    let w = s.captured(&[numbered.run, offset.run]).unwrap();
    // The numbering is given the three toes, all that's on their layer.
    assert_eq!(inputs_of(&w, 0)["strings"].from, Source::Layer("toes".into()));
    // The offset is given what the numbering changed, where strip is 2.
    let rule = &inputs_of(&w, 1)["string"];
    assert_eq!((&rule.from, rule.filter.get("strip"), rule.many), (&Source::Step(1), Some(&AttrValue::Number(2.0)), false));
    assert_eq!(resolve(s.project(), &w, 1, rule).unwrap(), vec![strip2]);
    assert_eq!(w.steps[0].last.as_ref().unwrap().revised.len(), 3);
}

#[test]
fn capture_falls_back_on_the_objects_as_picked_and_says_so() {
    let mut s = Session::with_builtins();
    let mut lines = Vec::new();
    for k in 0..3 {
        let y = N + 10.0 * k as f64;
        lines.extend(created(&s.run("string.create", json!({ "name": "Line", "layer": "lines", "points": [[E, y, 0.0], [E + 50.0, y, 0.0]] })).unwrap()));
    }
    let offset = s.run("string.offset", json!({ "string": lines[1], "distance": 5.0, "side": "left", "layer": "offsets" })).unwrap();
    let record = s.capture_workflow("Picked", &[offset.run]).unwrap();
    let w = workflow(&s, created(&record)[0]);
    assert_eq!(inputs_of(&w, 0)["string"].from, Source::Picked(vec![lines[1]]));
    let warned: Vec<_> = record.notes.iter().filter(|n| n.code == Some(Code::WorkflowPicked)).collect();
    assert_eq!(warned.len(), 1, "{:?}", record.notes);
    // Picked objects that are gone refuse the step, said.
    s.run("object.delete", json!({ "object": lines[1] })).unwrap();
    let refused = resolve(s.project(), &w, 0, &inputs_of(&w, 0)["string"]).unwrap_err();
    assert!(refused.contains("gone"), "{refused}");
}

#[test]
fn capture_refuses_runs_undone_or_refused() {
    let (mut s, solid, cut) = box_pit();
    let refused = s.run("solid.cut_by_surface", json!({ "solids": [created(&solid)[0]], "layer": "cut" })).unwrap_err();
    let failed = s.log().iter().rev().find_map(|e| match e {
        zd_commands::LogEntry::Run(r) => Some(r.run),
        _ => None,
    });
    assert!(!refused.to_string().is_empty());
    let said = s.captured(&[failed.unwrap()]).unwrap_err().to_string();
    assert!(said.contains("was refused"), "{said}");
    s.undo();
    let said = s.captured(&[solid.run, cut.run]).unwrap_err().to_string();
    assert!(said.contains("has been undone"), "{said}");
    s.redo();
    assert!(s.captured(&[solid.run, cut.run]).is_ok(), "a redo puts it back");
    assert!(s.captured(&[]).is_err());
}

#[test]
fn a_save_refuses_a_rule_naming_a_step_not_before_it() {
    let (mut s, solid, cut) = box_pit();
    let mut w = s.captured(&[solid.run, cut.run]).unwrap();
    w.steps.swap(0, 1);
    let refused = s.run("workflow.save", json!({ "name": "Backwards", "content": w })).unwrap_err().to_string();
    assert!(refused.contains("isn't before it"), "{refused}");
    let mut twice = s.captured(&[solid.run, cut.run]).unwrap();
    twice.steps[1].id = 1;
    assert!(s.run("workflow.save", json!({ "name": "Twice", "content": twice })).unwrap_err().to_string().contains("two steps"));
    // A rule naming a step that hasn't run says so.
    let mut fresh = s.captured(&[solid.run, cut.run]).unwrap();
    fresh.steps[0].last = None;
    let said = resolve(s.project(), &fresh, 1, &inputs_of(&fresh, 1)["solids"]).unwrap_err();
    assert!(said.contains("hasn't run"), "{said}");
}

#[test]
fn a_saved_workflow_is_revised_and_undone_as_one_object() {
    let (mut s, solid, cut) = box_pit();
    let id = created(&s.capture_workflow("Box pit", &[solid.run, cut.run]).unwrap())[0];
    let mut w = workflow(&s, id);
    w.steps[1].note = "cut at the base of weathering".into();
    s.run("workflow.save", json!({ "workflow": id, "name": "Box pit", "content": w })).unwrap();
    assert_eq!(s.project().object(id).unwrap().revision, 2);
    assert_eq!(workflow(&s, id).steps[1].note, "cut at the base of weathering");
    s.undo();
    assert_eq!(workflow(&s, id).steps[1].note, "");
}

// Running (workstream 2).

fn revision(s: &Session, id: ObjectId) -> u32 {
    s.project().object(id).unwrap().revision
}

fn save(s: &mut Session, id: ObjectId, w: &Workflow) {
    let name = s.project().object(id).unwrap().name.clone();
    s.run("workflow.save", json!({ "workflow": id, "name": name, "content": w })).unwrap();
}

/// The box pit captured: the session, the workflow, the pit and the two pieces.
fn captured_box_pit() -> (Session, ObjectId, ObjectId, Vec<ObjectId>) {
    let (mut s, solid, cut) = box_pit();
    let id = created(&s.capture_workflow("Box pit", &[solid.run, cut.run]).unwrap())[0];
    (s, id, created(&solid)[0], created(&cut))
}

#[test]
fn a_rerun_makes_the_same_objects_again_as_their_next_revisions_and_undoes_in_one() {
    let (mut s, id, pit, pieces) = captured_box_pit();
    let volumes: Vec<f64> = pieces.iter().map(|&p| volume(&s, p)).collect();
    let started = Instant::now();
    let record = s.run_workflow(id, 1).unwrap();
    println!("box pit run again in {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
    assert_eq!(record.status, WorkflowStatus::Finished);
    assert_eq!(record.steps.iter().map(|r| r.outcome).collect::<Vec<_>>(), [StepOutcome::Ran, StepOutcome::Ran]);
    // The same objects, at their next revisions, with the same volumes (to 1e-6 m³): 240,000 and 360,000 m³.
    assert_eq!((revision(&s, pit), revision(&s, pieces[0]), revision(&s, pieces[1])), (2, 2, 2));
    for (&p, v) in pieces.iter().zip(&volumes) {
        assert!((volume(&s, p) - v).abs() < 1e-6, "{} against {v}", volume(&s, p));
    }
    let mut sorted = volumes.clone();
    sorted.sort_by(f64::total_cmp);
    assert!((sorted[0] - 240_000.0).abs() < 1e-6 && (sorted[1] - 360_000.0).abs() < 1e-6, "{sorted:?}");
    assert_eq!(s.project().objects().filter(|o| matches!(o.body, Body::Solid(_))).count(), 3, "nothing new made");
    // Each step's last run is this one's, and the log holds the workflow run after its steps.
    let w = workflow(&s, id);
    assert!(w.steps.iter().all(|step| step.last.as_ref().unwrap().workflow_run == Some(record.run)));
    assert_eq!(w.steps[1].last.as_ref().unwrap().made, pieces);
    assert!(matches!(s.log().last(), Some(LogEntry::Workflow(r)) if r.run == record.run));
    // One undo takes the whole run back, the workflow's last runs with it; redo puts it back.
    assert_eq!(s.undo(), Some(record.run));
    assert_eq!((revision(&s, pit), revision(&s, pieces[0]), revision(&s, pieces[1])), (1, 1, 1));
    assert_eq!(workflow(&s, id).steps[0].last.as_ref().unwrap().workflow_run, None);
    assert_eq!(s.redo(), Some(record.run));
    assert_eq!(revision(&s, pieces[1]), 2);
}

#[test]
fn a_rerun_picks_up_a_revised_outline() {
    let (mut s, id, pit, pieces) = captured_box_pit();
    // Not part of the workflow: the outline's south-east corner moved 10 m east, so the pit is a trapezoid 100 m and
    // 110 m along its parallel sides and 60 m across, 6,300 m² by 100 m.
    let outline = s.project().objects().find(|o| o.name == "Outline").unwrap().id;
    s.run("string.move_vertex", json!({ "string": outline, "index": 1, "to": [E + 110.0, N, 0.0] })).unwrap();
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished);
    // Volume tolerance 1e-6 m³.
    assert!((volume(&s, pit) - 630_000.0).abs() < 1e-6, "pit {}", volume(&s, pit));
    let mut v: Vec<f64> = pieces.iter().map(|&p| volume(&s, p)).collect();
    v.sort_by(f64::total_cmp);
    assert!((v[0] - 252_000.0).abs() < 1e-6 && (v[1] - 378_000.0).abs() < 1e-6, "{v:?}");
}

#[test]
fn run_from_a_step_keeps_what_the_steps_before_it_made() {
    let (mut s, id, pit, pieces) = captured_box_pit();
    let record = s.run_workflow(id, 2).unwrap();
    assert_eq!((record.from, record.steps.len(), record.status), (2, 1, WorkflowStatus::Finished));
    assert_eq!((revision(&s, pit), revision(&s, pieces[0])), (1, 2), "the pit as it was, its pieces cut again");
    assert!(s.run_workflow(id, 3).unwrap_err().to_string().contains("can't run from step 3"));
    assert!(s.run_workflow(id, 0).is_err());
}

#[test]
fn a_pause_stops_the_run_and_continue_runs_on_from_the_next_step() {
    let (mut s, id, pit, pieces) = captured_box_pit();
    let mut w = workflow(&s, id);
    let pause = zd_core::WorkflowStep { id: w.next_id(), note: "delete what's under 50 BCM".into(), on: true, action: StepAction::Pause, last: None };
    w.steps.insert(1, pause);
    save(&mut s, id, &w);
    let before = s.project().undo_steps();
    let paused = s.run_workflow(id, 1).unwrap();
    assert_eq!(paused.status, WorkflowStatus::Paused { at: 2 });
    assert_eq!((revision(&s, pit), revision(&s, pieces[0])), (2, 1), "stopped before the cut");
    assert_eq!(s.project().undo_steps(), before + 1, "the part before the pause is one undo");
    // Hand work at the pause is its own run; Continue runs from the step after the pause, another undo.
    s.run("object.rename", json!({ "object": pit, "name": "Pit, looked at" })).unwrap();
    let rest = s.run_workflow(id, 3).unwrap();
    assert_eq!((rest.status, revision(&s, pieces[0])), (WorkflowStatus::Finished, 2));
    assert_eq!(s.project().undo_steps(), before + 3);
    // Run from the pause itself passes over it.
    let again = s.run_workflow(id, 2).unwrap();
    assert_eq!((again.steps[0].outcome, again.status), (StepOutcome::Off, WorkflowStatus::Finished));
    // A step switched off is passed over.
    let mut off = workflow(&s, id);
    off.steps[0].on = false;
    off.steps[1].on = false;
    save(&mut s, id, &off);
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.steps.iter().map(|r| r.outcome).collect::<Vec<_>>(), [StepOutcome::Off, StepOutcome::Off, StepOutcome::Ran]);
    assert_eq!(revision(&s, pit), 3, "renamed by hand, then left alone");
}

fn workflow_with_rl(s: &Session, id: ObjectId, rl: Option<f64>) -> Workflow {
    let mut w = workflow(s, id);
    let StepAction::Command { settings, .. } = &mut w.steps[1].action else { panic!() };
    settings["rl"] = json!(rl);
    w
}

#[test]
fn the_first_refusal_stops_the_run_and_says_why() {
    let (mut s, id, pit, pieces) = captured_box_pit();
    // An input its rule can't find.
    let outline = s.project().objects().find(|o| o.name == "Outline").unwrap().id;
    s.run("object.delete", json!({ "object": outline })).unwrap();
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Refused { at: 1 });
    assert_eq!(record.steps.len(), 1);
    let why = record.steps[0].refused.as_deref().unwrap();
    assert!(why.contains("'string'") && why.contains("finds nothing") && why.contains("layer 'outline'"), "{why}");
    assert_eq!((revision(&s, pit), revision(&s, pieces[0])), (1, 1), "nothing run");
    assert!(workflow(&s, id).steps[0].last.as_ref().unwrap().refused);
    s.undo();
    s.undo();
    // The command refusing: a cut with neither a surface nor an RL. The step before it stays applied.
    let w = workflow_with_rl(&s, id, None);
    save(&mut s, id, &w);
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Refused { at: 2 });
    assert!(record.steps[1].run.is_some() && record.steps[1].refused.is_some());
    assert_eq!((revision(&s, pit), revision(&s, pieces[0])), (2, 1));
    // Once fixed, the run after makes again what the refused step made the time before.
    let w = workflow_with_rl(&s, id, Some(40.0));
    save(&mut s, id, &w);
    let fixed = s.run_workflow(id, 2).unwrap();
    assert_eq!((fixed.status, revision(&s, pieces[0])), (WorkflowStatus::Finished, 2));
}

#[test]
fn a_cut_is_checked_before_it_runs_and_refused_with_what_the_check_found() {
    let (mut s, _, pit, _) = captured_box_pit();
    let polygon = |s: &mut Session, name: &str, x0: f64, x1: f64| {
        let points = [[x0, N - 10.0, 0.0], [x1, N - 10.0, 0.0], [x1, N + 70.0, 0.0], [x0, N + 70.0, 0.0]];
        created(&s.run("string.create", json!({ "name": name, "layer": "polygons", "points": points, "closed": true })).unwrap())[0]
    };
    // One polygon over the west half only: the check finds the east half outside it.
    polygon(&mut s, "B1", E - 10.0, E + 50.0);
    let rule = |from: Source, kind| InputRule { from, kind, filter: Default::default(), many: true };
    let mut inputs = std::collections::BTreeMap::new();
    inputs.insert("solids".to_string(), rule(Source::Named("Pit".into()), Some(ObjectKind::Solid)));
    inputs.insert("polygons".to_string(), rule(Source::Layer("polygons".into()), Some(ObjectKind::String)));
    let w = Workflow { steps: vec![zd_core::WorkflowStep { id: 1, note: String::new(), on: true, action: StepAction::Command { command: "solid.cut_by_polygons".into(), settings: json!({ "layer": "blocks" }), inputs }, last: None }] };
    s.run("object.rename", json!({ "object": pit, "name": "Pit" })).unwrap();
    let id = created(&s.run("workflow.save", json!({ "name": "Blocks", "content": w })).unwrap())[0];
    let runs = s.log().iter().filter(|e| matches!(e, LogEntry::Run(_))).count();
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Refused { at: 1 });
    let step = &record.steps[0];
    assert!(step.checked.iter().any(|n| n.code == Some(Code::SolidOutsidePolygons)), "{:?}", step.checked);
    assert!(step.refused.as_deref().unwrap().contains("check before the cut"), "{:?}", step.refused);
    assert_eq!(step.run, None, "the cut never ran");
    assert_eq!(s.log().iter().filter(|e| matches!(e, LogEntry::Run(_))).count(), runs + 1, "only the workflow's own save");
    // The east half added: two blocks of 300,000 m³ (to 1e-6 m³), named from their polygons.
    polygon(&mut s, "B2", E + 50.0, E + 110.0);
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished, "{:?}", record.steps[0].refused);
    let blocks = workflow(&s, id).steps[0].last.as_ref().unwrap().made.clone();
    assert_eq!(blocks.len(), 2);
    for b in &blocks {
        assert!((volume(&s, *b) - 300_000.0).abs() < 1e-6, "{}", volume(&s, *b));
    }
}

#[test]
fn a_workflow_run_in_the_log_saves_and_opens() {
    let (mut s, id, _, _) = captured_box_pit();
    let record = s.run_workflow(id, 1).unwrap();
    let dir = std::env::temp_dir().join(format!("zd-workflow-{}", ObjectId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Run.zdp");
    s.save(&path).unwrap();
    let opened = Session::open(&path).unwrap();
    assert!(opened.log().iter().any(|e| matches!(e, LogEntry::Workflow(r) if *r == record)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_step_added_from_a_tool_takes_an_earlier_steps_output_by_rule() {
    let (mut s, id, pit, pieces) = captured_box_pit();
    // As the Cut solids panel would run it: the pieces cut again at RL 70 onto their own layer.
    let params = json!({ "solids": pieces, "rl": 70.0, "keep": "both", "layer": "cut again" });
    let (at, step) = s.step_from(id, None, "solid.cut_by_surface", &params, "the upper bench").unwrap();
    assert_eq!((at, step.id, step.note.as_str()), (2, 3, "the upper bench"));
    let StepAction::Command { inputs, settings, .. } = &step.action else { panic!() };
    assert_eq!(inputs["solids"].from, Source::Step(2));
    assert_eq!(settings["rl"], json!(70.0));
    assert!(s.step_from(id, None, "solid.no_such_thing", &params, "").is_err());
    s.add_workflow_step(id, None, "solid.cut_by_surface", &params, "the upper bench").unwrap();
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished);
    // 240,000 below RL 40 and 360,000 above, the upper cut again at RL 70 into 180,000 and 180,000 (to 1e-6 m³); the
    // lower piece lies wholly below RL 70, so it comes through whole.
    let made = workflow(&s, id).steps[2].last.as_ref().unwrap().made.clone();
    let total: f64 = made.iter().map(|&b| volume(&s, b)).sum();
    assert!((total - 600_000.0).abs() < 1e-6, "{total}");
    assert!(made.iter().any(|&b| (volume(&s, b) - 180_000.0).abs() < 1e-6));
    assert_eq!(revision(&s, pit), 2);
}

#[test]
fn an_object_deleted_before_the_project_was_opened_is_found_by_its_name() {
    let mut s = Session::with_builtins();
    let base = created(&s.run("string.create", json!({ "name": "Base toe", "layer": "toes", "points": [[E, N, 0.0], [E + 500.0, N, 0.0]] })).unwrap())[0];
    let offsets = s.run("string.offset", json!({ "string": base, "distance": 60.0, "side": "left", "times": 3, "name": "Toe", "layer": "toes" })).unwrap();
    let made = created(&offsets);
    assert_eq!(made.len(), 3);
    let last = *made.last().unwrap();
    let name = s.project().object(last).unwrap().name.clone();
    let deleted = s.run("object.delete", json!({ "object": last })).unwrap();
    let dir = std::env::temp_dir().join(format!("zd-workflow-{}", ObjectId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Toes.zdp");
    s.save(&path).unwrap();
    // Opened again, the deleted toe's revisions are gone; the log still names it.
    let mut s = Session::open(&path).unwrap();
    let id = created(&s.capture_workflow("Toes", &[offsets.run, deleted.run]).unwrap())[0];
    let w = workflow(&s, id);
    assert_eq!(inputs_of(&w, 1)["object"].from, Source::Named(name.clone()));
    // Run again: the offset makes the last toe anew (the one it made is deleted), and the delete finds it by name.
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished, "{:?}", record.steps.last().unwrap().refused);
    assert!(s.project().objects().all(|o| o.name != name));
    assert_eq!(made[..2].iter().map(|&t| revision(&s, t)).collect::<Vec<_>>(), [2, 2], "the other toes made again as revisions");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_input_renamed_since_is_not_found_by_its_old_name() {
    // The base toe drawn by hand and offset, then numbering renames every toe "Strip 1" to "Strip 3": run again, the
    // offset would find no "Base toe", so capture keeps the toe as picked, and the run goes through.
    let mut s = Session::with_builtins();
    let base = created(&s.run("string.create", json!({ "name": "Base toe", "layer": "toes", "points": [[E, N, 0.0], [E + 500.0, N, 0.0]] })).unwrap())[0];
    let offsets = s.run("string.offset", json!({ "string": base, "distance": 60.0, "side": "left", "times": 2, "name": "Toe", "layer": "toes" })).unwrap();
    let toes: Vec<ObjectId> = std::iter::once(base).chain(created(&offsets)).collect();
    let numbered = s.run("string.number_across", json!({ "strings": toes, "from": [E + 250.0, N - 30.0, 0.0], "to": [E + 250.0, N + 200.0, 0.0], "start": 1.0 })).unwrap();
    assert_ne!(s.project().object(base).unwrap().name, "Base toe", "numbering renames it");
    let id = created(&s.capture_workflow("Toes", &[offsets.run, numbered.run]).unwrap())[0];
    let w = workflow(&s, id);
    assert_eq!(inputs_of(&w, 0)["string"].from, Source::Picked(vec![base]));
    let record = s.run_workflow(id, 1).unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished, "{:?}", record.steps.iter().find_map(|r| r.refused.clone()));
    // Made (1) and numbered (2), then made again (3) and numbered again (4).
    assert_eq!(created(&offsets).iter().map(|&t| revision(&s, t)).collect::<Vec<_>>(), [4, 4]);
}
