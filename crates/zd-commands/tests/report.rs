//! The workflow report (decision 0068) on a made-up pit with known answers: a strip 300 × 60 m from RL 80 to 110 cut by
//! two seams (cu 4 m at RD 1.4 and ash 10, cl 4 m at RD 1.5 and ash 20, 2 m of interburden between, 10 m of
//! overburden and 10 m below), then into three blocks along it, the coal interrogated. Captured as a workflow, run as
//! Base and as a case, reported. Volumes to 1e-6 m³, tonnes to 1e-6 t, composites to 1e-12.

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::report::RECONCILE_TOLERANCE;
use zd_commands::{RunRecord, Session, WorkflowStatus};
use zd_core::{AttrValue, Case, ChangeKind, ObjectId, RunId, StepAction, Workflow, WorkflowStep};

const E: f64 = 500_000.0;
const N: f64 = 7_000_000.0;
const STEP: f64 = 25.0;
const COLS: usize = 13;
const ROWS: usize = 9;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-report-{tag}-{}-{}", std::process::id(), RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A flat grid in Vulcan's plain layout on the 25 m lattice.
fn vulcan_grid(z: f32) -> Vec<u8> {
    let mut out = b"SYNTHETIC SEAM GRID             ".to_vec();
    for v in [E, E + (COLS - 1) as f64 * STEP, N, N + (ROWS - 1) as f64 * STEP] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(COLS * ROWS) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [STEP as f32, STEP as f32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [COLS as i32, ROWS as i32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for _ in 0..ROWS * COLS {
        out.extend_from_slice(&z.to_be_bytes());
        out.extend_from_slice(&1i32.to_be_bytes());
    }
    out
}

fn write_model(dir: &Path) {
    let files: [(&str, f32); 8] = [
        ("grncu.srg", 100.0),
        ("grncu.sfg", 96.0),
        ("grncu.rdg", 1.4),
        ("grncu.ashg", 10.0),
        ("grncl.srg", 94.0),
        ("grncl.sfg", 90.0),
        ("grncl.rdg", 1.5),
        ("grncl.ashg", 20.0),
    ];
    for (name, z) in files {
        std::fs::write(dir.join(name), vulcan_grid(z)).unwrap();
    }
    std::fs::write(
        dir.join("stack.csv"),
        "horizon,parent,material,working,roof,floor,rd,ash\ncu,,coal,yes,grncu.srg,grncu.sfg,grncu.rdg,grncu.ashg\ncl,,coal,yes,grncl.srg,grncl.sfg,grncl.rdg,grncl.ashg\n",
    )
    .unwrap();
}

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn text(s: &Session, id: ObjectId, key: &str) -> Option<String> {
    match s.project().object(id)?.attributes.get(key) {
        Some(AttrValue::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

/// The pit worked by hand and captured: the seam cut, the cut into three 100 m blocks and the interrogation of the coal.
fn captured_pit(dir: &Path) -> (Session, ObjectId) {
    write_model(dir);
    let mut s = Session::with_builtins();
    let model = created(&s.run("seam.build_model", json!({ "definition": dir.join("stack.csv").to_string_lossy(), "layer": "geology" })).unwrap())[0];
    let vertices: Vec<[f64; 3]> = (0..8).map(|i| [E + if i & 1 == 1 { 300.0 } else { 0.0 }, N + if i >> 1 & 1 == 1 { 110.0 } else { 50.0 }, if i >> 2 & 1 == 1 { 110.0 } else { 80.0 }]).collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let strip = created(&s.run("solid.from_mesh", json!({ "name": "Strip 5", "layer": "strips", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    for k in 0..3 {
        let (x0, x1) = (100.0 * k as f64, 100.0 * (k + 1) as f64);
        let points = [[E + x0, N, 0.0], [E + x1, N, 0.0], [E + x1, N + 200.0, 0.0], [E + x0, N + 200.0, 0.0]];
        s.run("string.create", json!({ "name": format!("B{}", k + 1), "layer": "block polygons", "closed": true, "points": points })).unwrap();
    }
    let seams = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers" })).unwrap();
    let polygons: Vec<ObjectId> = s.project().objects().filter(|o| o.name.starts_with('B') && o.name.len() == 2).map(|o| o.id).collect();
    let blocks = s.run("solid.cut_by_polygons", json!({ "solids": created(&seams), "polygons": polygons, "layer": "blocks" })).unwrap();
    let coal: Vec<ObjectId> = created(&blocks).into_iter().filter(|&id| text(&s, id, "material").as_deref() == Some("coal")).collect();
    let interrogated = s.run("solid.interrogate", json!({ "solids": coal, "model": model })).unwrap();
    let id = created(&s.capture_workflow("Two seams", &[seams.run, blocks.run, interrogated.run]).unwrap())[0];
    (s, id)
}

fn save(s: &mut Session, id: ObjectId, w: &Workflow) {
    let name = s.project().object(id).unwrap().name.clone();
    s.run("workflow.save", json!({ "workflow": id, "name": name, "content": w })).unwrap();
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn a_report_adds_each_case_up_by_seam_and_reconciles_to_the_pit() {
    let t = TempDir::new("cases");
    let (mut s, id) = captured_pit(&t.0);
    let mut w = s.workflow(id).unwrap().clone();
    w.cases.push(Case { name: "Again".into(), ..Case::default() });
    save(&mut s, id, &w);
    let records = s.run_all_cases(id).unwrap();
    assert!(records.iter().all(|r| r.status == WorkflowStatus::Finished), "{records:?}");

    let started = std::time::Instant::now();
    let report = s.workflow_report(id, 600).unwrap();
    println!("report figures in {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
    assert_eq!(report.cases.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Base", "Again"]);
    for c in &report.cases {
        // Five layers × three blocks, each 100 × 60 m.
        assert_eq!(c.blocks.len(), 15, "{}", c.name);
        assert_eq!(c.density.as_deref(), Some("rd"));
        assert_eq!(c.qualities, ["ash"]);
        let rows: Vec<(&str, &str)> = c.horizons.iter().map(|r| (r.label.as_str(), r.material.as_str())).collect();
        assert_eq!(rows, [("OB", "waste"), ("cu", "coal"), ("cu-cl", "waste"), ("cl", "coal"), ("below cl", "waste")], "top down");
        let volumes: Vec<f64> = c.horizons.iter().map(|r| r.volume).collect();
        for (v, want) in volumes.iter().zip([180_000.0, 72_000.0, 36_000.0, 72_000.0, 180_000.0]) {
            assert!(close(*v, want, 1e-6), "{} {volumes:?}", c.name);
        }
        // Coal: 72,000 m³ at 1.4 and 72,000 at 1.5 is 208,800 t, RD 1.45, ash by tonnes; the grids hold f32, so 1.4 is
        // 1.39999998 and the tonnes 0.0017 t short of 208,800.
        let (cu, cl) = (72_000.0 * f64::from(1.4f32), 72_000.0 * 1.5);
        assert!(close(c.coal.volume, 144_000.0, 1e-6));
        assert!(close(c.coal.tonnes.unwrap(), cu + cl, 1e-6), "{:?}", c.coal.tonnes);
        assert!(close(c.coal.tonnes.unwrap(), 208_800.0, 0.01));
        assert!(close(c.coal.rd.unwrap(), (cu + cl) / 144_000.0, 1e-12));
        assert!(close(c.coal.qualities["ash"], (cu * 10.0 + cl * 20.0) / (cu + cl), 1e-9), "{:?}", c.coal.qualities);
        assert!(close(c.horizons[1].qualities["ash"], 10.0, 1e-9) && close(c.horizons[3].qualities["ash"], 20.0, 1e-9));
        // Waste above the last floor counts in the strip ratio; the 10 m below it doesn't.
        assert!(close(c.waste.volume, 216_000.0, 1e-6) && close(c.below.volume, 180_000.0, 1e-6));
        assert!(close(c.strip_ratio.unwrap(), 216_000.0 / (cu + cl), 1e-12));
        assert_eq!(c.no_density, 0);
        // The strip the cut started from: 540,000 m³, all of it in blocks.
        let pit = c.pit.as_ref().expect("a pit");
        assert!(close(pit.volume, 540_000.0, 1e-6) && pit.within, "{pit:?}");
        assert!(pit.difference.abs() <= RECONCILE_TOLERANCE * pit.volume);
        assert!(c.ran.as_ref().is_some_and(|r| r.steps_run == 3 && r.refused_at.is_none()));
    }
    // Each case's own blocks: the case's are tagged with it and on its own layers.
    let names = |k: usize| report.cases[k].blocks.iter().map(|b| b.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(0), names(1), "the same names");
    assert_eq!(report.steps.len(), 3);
    assert!(report.steps[1].inputs.iter().any(|(_, words)| words == "what step 1 made, solids" || words.starts_with("what step 1 made")), "{:?}", report.steps[1].inputs);
    assert!(report.inputs.iter().any(|i| !i.files.is_empty()), "the seam model's grid files are listed with their fingerprints");

    // The page: one file beside nothing (a folder given here), fetching nothing, holding the totals.
    let path = s.write_workflow_report(id, 600, Some(&t.0.join("reports"))).unwrap();
    let page = std::fs::read_to_string(&path).unwrap();
    println!("report page {} bytes", page.len());
    assert!(path.file_name().unwrap().to_string_lossy().starts_with("Two seams 20"), "{path:?}");
    for total in ["208,800", "144,000", "216,000", "540,000.000", "Reconciles to the pit", "B2_cu"] {
        assert!(page.contains(total), "the page says {total}");
    }
    for outside in ["http://", "https://", "<link", " src="] {
        assert!(!page.contains(outside), "the page fetches nothing: found {outside}");
    }
    let again = s.write_workflow_report(id, 600, Some(&t.0.join("reports"))).unwrap();
    assert_ne!(path, again, "a second report the same minute doesn't overwrite the first");
    // No project file, no folder given: refused, saying why.
    assert!(s.write_workflow_report(id, 600, None).unwrap_err().to_string().contains("save the project first"));
}

#[test]
fn solids_deleted_at_a_pause_are_left_out_and_still_reconcile() {
    let t = TempDir::new("pause");
    let (mut s, id) = captured_pit(&t.0);
    let mut w = s.workflow(id).unwrap().clone();
    let pause = WorkflowStep { id: w.next_id(), note: "delete what's under 50 BCM".into(), on: true, action: StepAction::Pause, last: None };
    w.steps.insert(2, pause);
    save(&mut s, id, &w);
    let paused = s.run_workflow(id, 1).unwrap();
    assert_eq!(paused.status, WorkflowStatus::Paused { at: 3 });
    // By hand: B3's overburden deleted, 100 × 60 × 10 = 60,000 m³; the delete says so in its note.
    let b3_ob = s.project().objects().find(|o| o.name == "B3_OB").unwrap().id;
    let deleted = s.run("object.delete", json!({ "object": b3_ob })).unwrap();
    let note = deleted.notes.iter().find(|n| n.code == Some(zd_commands::Code::ObjectSolidsDeleted)).expect("the delete says what it took out");
    assert!(close(note.value.as_ref().unwrap().value, 60_000.0, 1e-6));
    assert_eq!(s.run_workflow(id, 4).unwrap().status, WorkflowStatus::Finished);

    let report = s.workflow_report(id, 0).unwrap();
    let base = &report.cases[0];
    assert_eq!(base.blocks.len(), 14);
    let by_hand: Vec<_> = base.left_out.iter().filter(|l| l.step.is_none()).collect();
    assert_eq!(by_hand.len(), 1, "{:?}", base.left_out);
    assert!(close(by_hand[0].volume, 60_000.0, 1e-6));
    let pit = base.pit.as_ref().unwrap();
    assert!(pit.within && close(pit.accounted, 540_000.0, 1e-6), "{pit:?}");
    assert!(close(base.waste.volume, 156_000.0, 1e-6), "the deleted overburden is out of the strip ratio's waste");

    // Undone, the delete no longer counts: the run after it is undone first, then the delete.
    s.undo();
    s.undo();
    let undone = s.workflow_report(id, 0).unwrap();
    assert!(undone.cases[0].left_out.iter().all(|l| l.step.is_some()), "{:?}", undone.cases[0].left_out);
}
