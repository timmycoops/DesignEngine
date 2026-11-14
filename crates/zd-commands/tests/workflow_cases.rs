//! Workflow cases (decision 0066): one workflow, several sets of settings side by side, each on its own layers and tagged
//! with its case, seeing what it made and what no step made, never another case's.

use std::time::Instant;

use serde_json::json;
use zd_commands::{RunRecord, Session, WorkflowStatus};
use zd_core::{AttrValue, Body, Case, ChangeKind, ObjectId, Workflow};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn workflow(s: &Session, id: ObjectId) -> Workflow {
    s.workflow(id).unwrap().clone()
}

fn save(s: &mut Session, id: ObjectId, w: &Workflow) -> Result<RunRecord, String> {
    s.run("workflow.save", json!({ "workflow": id, "name": "Box pit", "content": w })).map_err(|e| e.to_string())
}

/// Solids on a layer by name, with their volumes, in the project's order.
fn solids_on(s: &Session, layer: &str) -> Vec<(ObjectId, f64)> {
    let Some(layer) = s.project().layer_named(layer) else { return Vec::new() };
    s.project()
        .objects()
        .filter(|o| o.layer == layer.id)
        .filter_map(|o| match &o.body {
            Body::Solid(solid) => Some((o.id, solid.volume())),
            _ => None,
        })
        .collect()
}

fn case_of(s: &Session, id: ObjectId) -> Option<String> {
    match s.project().object(id)?.attributes.get("case") {
        Some(AttrValue::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v
}

/// The box pit of the workflow tests (100 m by 60 m, RL 0 to 100, 600,000 m³) cut at RL 40, captured, with a case "60"
/// cutting at RL 60 instead.
fn box_pit_with_a_case() -> (Session, ObjectId) {
    let mut s = Session::with_builtins();
    let flat = |s: &mut Session, name: &str, z: f64| {
        let (x0, y0, x1, y1) = (E - 100.0, N - 100.0, E + 300.0, N + 300.0);
        created(&s.run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]], "triangles": [[0, 1, 2], [0, 2, 3]] })).unwrap())[0]
    };
    let top = flat(&mut s, "Top", 100.0);
    let bottom = flat(&mut s, "Bottom", 0.0);
    let outline = created(&s.run("string.create", json!({ "name": "Outline", "layer": "outline", "points": [[E, N, 0.0], [E + 100.0, N, 0.0], [E + 100.0, N + 60.0, 0.0], [E, N + 60.0, 0.0]], "closed": true })).unwrap())[0];
    let solid = s.run("solid.between_surfaces", json!({ "top": top, "bottom": bottom, "string": outline, "layer": "pit" })).unwrap();
    let cut = s.run("solid.cut_by_surface", json!({ "solids": created(&solid), "rl": 40.0, "keep": "both", "layer": "cut" })).unwrap();
    let id = created(&s.capture_workflow("Box pit", &[solid.run, cut.run]).unwrap())[0];
    let mut w = workflow(&s, id);
    let mut changes = serde_json::Map::new();
    changes.insert("rl".into(), json!(60.0));
    w.cases.push(Case { name: "60".into(), settings: [(2, changes)].into(), last: Default::default() });
    save(&mut s, id, &w).unwrap();
    (s, id)
}

#[test]
fn every_case_runs_on_its_own_layers_tagged_with_its_case_and_adds_up() {
    let (mut s, id) = box_pit_with_a_case();
    let base_pieces: Vec<ObjectId> = solids_on(&s, "cut").into_iter().map(|(p, _)| p).collect();
    let undo_before = s.project().undo_steps();
    let started = Instant::now();
    let records = s.run_all_cases(id).unwrap();
    println!("two cases of the box pit run in {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
    assert_eq!(records.iter().map(|r| (r.case.as_str(), r.status)).collect::<Vec<_>>(), [("Base", WorkflowStatus::Finished), ("60", WorkflowStatus::Finished)]);

    // Base: the same pieces made again, tagged Base: 240,000 below RL 40 and 360,000 above (to 1e-6 m³).
    let base = solids_on(&s, "cut");
    assert_eq!(base.iter().map(|(p, _)| *p).collect::<Vec<_>>(), base_pieces);
    let v = sorted(base.iter().map(|(_, v)| *v).collect());
    assert!((v[0] - 240_000.0).abs() < 1e-6 && (v[1] - 360_000.0).abs() < 1e-6, "{v:?}");
    assert!(base.iter().all(|(p, _)| case_of(&s, *p).as_deref() == Some("Base")));

    // The case: its own pit on 'pit 60' and its own pieces on 'cut 60', cut at RL 60: 360,000 below, 240,000 above.
    let pit = solids_on(&s, "pit 60");
    assert_eq!(pit.len(), 1);
    assert!((pit[0].1 - 600_000.0).abs() < 1e-6);
    let case = solids_on(&s, "cut 60");
    let v = sorted(case.iter().map(|(_, v)| *v).collect());
    assert!((v[0] - 240_000.0).abs() < 1e-6 && (v[1] - 360_000.0).abs() < 1e-6, "{v:?}");
    assert!(case.iter().chain(&pit).all(|(p, _)| case_of(&s, *p).as_deref() == Some("60")));
    let total = |pieces: &[(ObjectId, f64)]| pieces.iter().map(|(_, v)| v).sum::<f64>();
    assert!((total(&case) - total(&base)).abs() < 1e-6, "each case adds up to its pit");
    let below = case.iter().find(|(p, _)| s.project().object(*p).unwrap().name.contains("below")).unwrap().1;
    assert!((below - 360_000.0).abs() < 1e-6, "below RL 60 is 360,000 m³: {below}");

    // Each case keeps its own last runs: the case's step 2 made the case's pieces, Base's made Base's.
    let w = workflow(&s, id);
    assert_eq!(w.steps[1].last.as_ref().unwrap().made, base_pieces);
    let mut case_made = w.case("60").unwrap().last[&2].made.clone();
    case_made.sort();
    let mut on_layer: Vec<ObjectId> = case.iter().map(|(p, _)| *p).collect();
    on_layer.sort();
    assert_eq!(case_made, on_layer);

    // Run all cases is one undo.
    assert_eq!(s.project().undo_steps(), undo_before + 1);
    s.undo();
    assert!(solids_on(&s, "cut 60").is_empty() && solids_on(&s, "pit 60").is_empty());
    assert!(base_pieces.iter().all(|&p| s.project().object(p).unwrap().revision == 1));
}

#[test]
fn a_case_run_again_makes_its_own_objects_again_and_never_touches_base() {
    let (mut s, id) = box_pit_with_a_case();
    s.run_case(id, 1, "60").unwrap();
    let first: Vec<ObjectId> = solids_on(&s, "cut 60").into_iter().map(|(p, _)| p).collect();
    let base: Vec<(ObjectId, u32)> = solids_on(&s, "cut").into_iter().map(|(p, _)| (p, s.project().object(p).unwrap().revision)).collect();
    let record = s.run_case(id, 1, "60").unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished);
    let again: Vec<ObjectId> = solids_on(&s, "cut 60").into_iter().map(|(p, _)| p).collect();
    assert_eq!(again, first, "the same objects, as their next revisions");
    assert!(again.iter().all(|&p| s.project().object(p).unwrap().revision == 2));
    assert_eq!(solids_on(&s, "cut").into_iter().map(|(p, _)| (p, s.project().object(p).unwrap().revision)).collect::<Vec<_>>(), base, "Base untouched");
    // Run from step 2 in the case takes the case's own pit.
    let record = s.run_case(id, 2, "60").unwrap();
    assert_eq!(record.status, WorkflowStatus::Finished);
    assert_eq!(solids_on(&s, "pit 60")[0].0, workflow(&s, id).case("60").unwrap().last[&1].made[0]);
    assert!(s.run_case(id, 1, "no such case").unwrap_err().to_string().contains("no case named"));
}

#[test]
fn a_save_refuses_cases_it_cant_run() {
    let (mut s, id) = box_pit_with_a_case();
    let with = |s: &Session, change: &dyn Fn(&mut Workflow)| {
        let mut w = workflow(s, id);
        change(&mut w);
        w
    };
    let named_base = with(&s, &|w| w.cases[0].name = "base".into());
    assert!(save(&mut s, id, &named_base).unwrap_err().contains("another name"));
    let twice = with(&s, &|w| w.cases.push(w.cases[0].clone()));
    assert!(save(&mut s, id, &twice).unwrap_err().contains("two cases"));
    let no_step = with(&s, &|w| {
        let changes = w.cases[0].settings.remove(&2).unwrap();
        w.cases[0].settings.insert(9, changes);
    });
    assert!(save(&mut s, id, &no_step).unwrap_err().contains("isn't there"));
    let an_input = with(&s, &|w| {
        w.cases[0].settings.get_mut(&2).unwrap().insert("solids".into(), json!([]));
    });
    assert!(save(&mut s, id, &an_input).unwrap_err().contains("an input found by rule"));
}
