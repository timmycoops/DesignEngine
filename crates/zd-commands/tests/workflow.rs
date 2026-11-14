//! Workflows (decision 0065): runs captured from the run log as steps, their object ids turned into rules, saved as a
//! workflow object; every rule found again gives the objects the runs were given.

use std::time::Instant;

use serde_json::json;
use zd_commands::workflow::resolve;
use zd_commands::{Code, RunRecord, Session, Severity};
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
