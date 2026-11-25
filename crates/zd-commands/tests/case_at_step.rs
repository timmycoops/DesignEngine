//! A case kept at a step (the Build Plan's "Reserves case plan", decisions 8 and 9, workstream 3): every column of a
//! reserves table's case run to a step and kept, its totals there, an edit run again from the first step that reads it,
//! and the table it makes from there the table a full run makes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::builtin::{CaseAtStep, Unsaved};
use zd_commands::{Command, CommandError, CommandSpec, Inputs, Outcome, Registry, RunRecord, Session};
use zd_core::{Attributes, Body, Change, ChangeKind, ColumnValues, ObjectId, ParameterTable, Table};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// Volumes from boxes add exactly; what's taken off them to this, relative.
const TOLERANCE: f64 = 1e-9;

struct Tag;

#[derive(Serialize, Deserialize)]
struct TagParams {
    object: ObjectId,
    attributes: Attributes,
}

impl Command for Tag {
    type Params = TagParams;
    const NAME: &'static str = "test.tag";

    fn spec(&self) -> CommandSpec {
        CommandSpec { name: Self::NAME, summary: "tag", inputs: Vec::new(), params: Vec::new(), outputs: Vec::new() }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &TagParams) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.object)?;
        let mut attributes = object.attributes.clone();
        attributes.extend(p.attributes.clone());
        Ok(Outcome::new(vec![Change::Revise { id: object.id, base: object.revision, name: None, layer: None, attributes: Some(attributes), body: None }]))
    }
}

fn session() -> Session {
    let mut registry = Registry::with_builtins();
    registry.register(zd_commands::Module::Design, Tag);
    Session::new(registry)
}

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE * b.abs().max(1.0)
}

fn box_solid(s: &mut Session, name: &str, z: (f64, f64), attributes: serde_json::Value) -> ObjectId {
    let (x0, x1, y0, y1) = (0.0, 100.0, 0.0, 50.0);
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| [E + if i & 1 == 1 { x1 } else { x0 }, N + if i >> 1 & 1 == 1 { y1 } else { y0 }, if i >> 2 & 1 == 1 { z.1 } else { z.0 }])
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let id = created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "blocks", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    s.run("test.tag", json!({ "object": id, "attributes": attributes })).unwrap();
    id
}

const STACK: [&str; 5] = ["OB", "A", "A-B", "B", "below B"];

/// One block, 100 m by 50 m (5,000 m², 100 m long): OB 10 m, A coal 2 m, A-B 5 m (a break), B coal 3 m, waste below
/// 4 m; 120,000 m³ in all.
fn blocks(s: &mut Session) -> Vec<ObjectId> {
    let rows: [(&str, f64, f64, &str, &str); 5] =
        [("OB", 40.0, 50.0, "waste", "A"), ("A", 38.0, 40.0, "coal", "A"), ("A-B", 33.0, 38.0, "waste", "B"), ("B", 30.0, 33.0, "coal", "B"), ("below B", 26.0, 30.0, "waste", "")];
    let ids = rows
        .iter()
        .map(|&(h, z0, z1, material, seam)| {
            let mut a = json!({ "strip": 1.0, "block": "B1", "horizon": h, "material": material, "seam": seam });
            if material == "coal" {
                a["rd"] = json!(1.4);
                a["ash"] = json!(10.0);
            }
            box_solid(s, &format!("1_B1_{h}"), (z0, z1), a)
        })
        .collect::<Vec<_>>();
    s.run("solid.measure", json!({ "solids": ids })).unwrap();
    ids
}

/// The settings: roof loss `roof` m, floor 0.2 m, edge 0.05 m, other 2 %, sections by a 1 m parting; no dilution.
fn csv(roof: f64, min_parting: f64, roof_dilution: f64) -> String {
    format!(
        "when material,rd (t/m3),min_parting (m),ws_strip_ratio,min_coal_thickness (m),roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%),roof_dilution (m),floor_dilution (m),edge_dilution (m),other_dilution (%)\nwaste,2.3,,,,,,,,,,,\ncoal,,{min_parting},0.5,0.5,{roof},0.2,0.05,2,{roof_dilution},0,0,0\n"
    )
}

const STEPS: [&str; 6] = ["initialise", "join_benches", "working_sections", "rom_loss", "rom_dilution", "final"];

/// The pit, its settings table and dataflow (the starter's formulas in), and its reserves table made by a run.
fn case(s: &mut Session) -> (ObjectId, ObjectId, ObjectId, Vec<ObjectId>) {
    let ids = blocks(s);
    let settings = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": csv(0.1, 1.0, 0.0) })).unwrap())[0];
    let steps: Vec<serde_json::Value> = STEPS.iter().map(|s| json!({ "step": s })).collect();
    let content = json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "area", "kind": "area", "add_up": { "by": "max" }, "from": "plan_area" },
            { "name": "length", "kind": "length", "add_up": { "by": "max" }, "from": "attribute", "attribute": "length" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
        ],
        "steps": steps,
        "parameters": [settings],
    });
    let mut d: zd_core::Dataflow = serde_json::from_value(content).unwrap();
    zd_commands::builtin::with_starter_formulas(&mut d);
    let flow = created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": d })).unwrap())[0];
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    (created(&record)[0], settings, flow, ids)
}

fn edited(settings: ObjectId, csv: &str) -> Unsaved {
    Unsaved { dataflow: None, tables: vec![(settings, Arc::new(ParameterTable::from_csv(csv).unwrap()))] }
}

fn open_at(s: &Session, table: ObjectId, step: &str) -> CaseAtStep {
    let mut case = CaseAtStep::open(s.project(), s.log(), table, 0, &Unsaved::default()).unwrap();
    let n = case.step_number(step).unwrap();
    case.go_to(n).unwrap();
    case
}

fn said(t: &zd_commands::builtin::StepTotals, what: &str) -> (usize, f64) {
    t.said.iter().find(|s| s.what == what).map(|s| (s.units, s.volume.unwrap_or(f64::NAN))).unwrap_or((0, 0.0))
}

fn group(groups: &[zd_commands::builtin::Totals], name: &str) -> f64 {
    groups.iter().find(|g| g.group == name).and_then(|g| g.values["volume"]).unwrap_or(0.0)
}

/// At `rom_loss`, roof 0.1 m: A gives 500 m³ roof, 1,000 floor, 10 edge and 200 other, 8,290 m³ of coal left; B 500,
/// 1,000, 15 and 300, 13,185 left. The step says each loss over the case, writes its coal losses, and keeps the volume.
#[test]
fn a_case_at_a_step_adds_up_what_the_step_did() {
    let mut s = session();
    let (table, _, _, _) = case(&mut s);
    let case = open_at(&s, table, "rom_loss");
    let t = case.totals();
    assert_eq!(t.name, "rom_loss");
    assert_eq!(t.steps[t.step - 1], "rom_loss");
    assert_eq!((t.columns, t.stopped.len()), (1, 0));
    assert_eq!(said(&t, "roof loss").0, 2);
    assert!(close(said(&t, "roof loss").1, 1_000.0) && close(said(&t, "floor loss").1, 2_000.0), "{:?}", t.said);
    assert!(close(said(&t, "edge loss").1, 25.0) && close(said(&t, "other loss").1, 500.0), "{:?}", t.said);
    assert!(close(group(&t.written, "Loss\\Roof"), 1_000.0) && close(group(&t.written, "Loss\\Other"), 500.0), "{:?}", t.written);
    assert!(close(group(&t.units, "coal"), 8_290.0 + 13_185.0), "{:?}", t.units);
    assert!(close(group(&t.units, "coal, loss (roof)"), 1_000.0), "{:?}", t.units);
    assert!(t.balance.balanced && close(t.balance.volume_in, 120_000.0) && close(t.balance.volume_now - t.balance.added, 120_000.0), "{:?}", t.balance);
    // Every step from the blocks ran to get here, and the units are its table, a row a unit.
    assert_eq!(t.ran.iter().map(|r| r.step).collect::<Vec<_>>(), (0..=t.step).collect::<Vec<_>>());
    assert_eq!(case.units().table.rows(), 13);
    assert!(t.kept_mb > 0.0);
}

/// Roof 0.1 m to 0.15 m: only `rom_loss` runs again, from the columns kept before it; roof loss is 750 m³ a section,
/// 1,500 in all, and 8,040 + 12,935 m³ of coal is left. A parting rule `working_sections` reads runs the case again from
/// its blocks; a setting only a later step reads runs nothing.
#[test]
fn an_edit_runs_again_from_the_first_step_that_reads_it() {
    let mut s = session();
    let (table, settings, _, _) = case(&mut s);
    let mut case = open_at(&s, table, "rom_loss");
    let k = case.step_number("rom_loss").unwrap();

    case.rerun(s.project(), s.log(), &edited(settings, &csv(0.15, 1.0, 0.0))).unwrap();
    let t = case.totals();
    assert_eq!(t.ran_from, Some(k));
    assert_eq!(t.ran.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["rom_loss"]);
    assert_eq!(said(&t, "roof loss").0, 2);
    assert!(close(said(&t, "roof loss").1, 1_500.0), "{:?}", t.said);
    assert!(close(group(&t.units, "coal"), 8_040.0 + 12_935.0), "{:?}", t.units);
    assert!(t.balance.balanced);

    // A setting `rom_dilution`, after it, reads: nothing up to here reads it.
    case.rerun(s.project(), s.log(), &edited(settings, &csv(0.15, 1.0, 0.05))).unwrap();
    let t = case.totals();
    assert_eq!((t.ran_from, t.ran.len()), (None, 0));
    assert!(close(said(&t, "roof loss").1, 1_500.0));

    // The parting rule, read by `working_sections` before it: from the blocks.
    case.rerun(s.project(), s.log(), &edited(settings, &csv(0.15, 6.0, 0.05))).unwrap();
    let t = case.totals();
    assert_eq!(t.ran_from, Some(0));
    assert_eq!(t.ran.len(), k + 1);

    // Back to what's saved: from the blocks again, as the parting rule changed back.
    case.rerun(s.project(), s.log(), &Unsaved::default()).unwrap();
    let t = case.totals();
    assert_eq!(t.ran_from, Some(0));
    assert!(close(said(&t, "roof loss").1, 1_000.0));
}

/// The dataflow as edited: `rom_loss` told to read its roof loss from `floor_loss` (0.2 m): only `rom_loss` runs
/// again, 1,000 m³ of roof loss a section. A settings column added (a cell filled) changes which settings each unit has,
/// which the starter's limits formula reads with `dir(p)` before it: the case runs again from its blocks.
#[test]
fn an_edited_step_runs_again_from_itself() {
    let mut s = session();
    let (table, settings, flow, _) = case(&mut s);
    let mut case = open_at(&s, table, "rom_loss");
    let k = case.step_number("rom_loss").unwrap();
    let Body::Dataflow(d) = &s.project().object(flow).unwrap().body else { panic!("a dataflow") };
    let mut d = (**d).clone();
    let step = d.steps.iter_mut().find(|st| st.step == "rom_loss").unwrap();
    step.settings.insert("roof_loss".into(), "floor_loss".into());
    let mut unsaved = Unsaved { dataflow: Some(Arc::new(d)), tables: Vec::new() };
    case.rerun(s.project(), s.log(), &unsaved).unwrap();
    let t = case.totals();
    assert_eq!(t.ran_from, Some(k), "{:?}", t.ran);
    assert!(close(said(&t, "roof loss").1, 2_000.0), "{:?}", t.said);

    let with_b = csv(0.1, 1.0, 0.0).replace("roof_loss (m),", "roof_loss (m),roof_loss_b (m),").replace(",0.1,0.2,", ",0.1,0.15,0.2,").replace("waste,2.3,", "waste,2.3,,");
    unsaved.tables = edited(settings, &with_b).tables;
    case.rerun(s.project(), s.log(), &unsaved).unwrap();
    assert_eq!(case.totals().ran_from, Some(0));
}

fn number_columns(t: &Table) -> Vec<(String, Vec<Option<f64>>)> {
    t.columns
        .iter()
        .filter(|c| c.name != "Summary\\Audit\\run")
        .filter_map(|c| match &c.values {
            ColumnValues::Number(v) => Some((c.name.clone(), v.clone())),
            _ => None,
        })
        .collect()
}

fn word_columns(t: &Table) -> Vec<(String, Vec<Option<String>>)> {
    t.columns
        .iter()
        .filter(|c| c.name != "Summary\\Audit\\run")
        .filter_map(|c| match &c.values {
            ColumnValues::Text(v) => Some((c.name.clone(), v.clone())),
            _ => None,
        })
        .collect()
}

/// Roof 0.15 m edited at `rom_loss` and the case run on from there: the same table, every column but the audit's time
/// and the settings' revision, as a full run once 0.15 m is saved (exact). The totals at `final` are the table's.
#[test]
fn the_table_from_a_step_is_the_table_a_full_run_makes() {
    let mut s = session();
    let (table, settings, flow, ids) = case(&mut s);
    let mut case = open_at(&s, table, "rom_loss");
    case.rerun(s.project(), s.log(), &edited(settings, &csv(0.15, 1.0, 0.0))).unwrap();
    let from_step = case.table(s.project()).unwrap();

    s.run("parameters.save", json!({ "table": settings, "name": "assumptions", "csv": csv(0.15, 1.0, 0.0) })).unwrap();
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let Body::Table(full) = &s.project().object(created(&record)[0]).unwrap().body else { panic!("a table") };
    assert_eq!(from_step.rows(), full.rows());
    assert_eq!(number_columns(&from_step), number_columns(full));
    // The words but the audit's: the settings table's revision is 1 as edited, 2 as saved.
    let audit = |t: &Table| word_columns(t).into_iter().filter(|(n, _)| !n.starts_with("Summary\\Audit")).collect::<Vec<_>>();
    assert_eq!(audit(&from_step), audit(full));

    // At `final`, what's written is the table's columns added up.
    let n = case.step_number("final").unwrap();
    case.go_to(n).unwrap();
    let t = case.totals();
    let column_total = |name: &str| match &full.column(name).unwrap().values {
        ColumnValues::Number(v) => v.iter().flatten().sum::<f64>(),
        _ => panic!("numbers"),
    };
    assert!(close(group(&t.written, "ROM\\Coal"), column_total("ROM\\Coal\\volume")), "{:?}", t.written);
    assert!(close(group(&t.written, "Loss\\Roof"), column_total("Loss\\Roof\\volume")));
    let rom_coal = t.written.iter().find(|g| g.group == "ROM\\Coal").and_then(|g| g.tonnes).unwrap();
    assert!(close(rom_coal, column_total("ROM\\Coal\\tonnes")), "{rom_coal} t");
}

/// No ground made or lost at any step: each step's columns hold what came in from the blocks, plus what came from
/// outside the model.
#[test]
fn no_ground_is_made_or_lost_at_any_step() {
    let mut s = session();
    let (table, _, _, _) = case(&mut s);
    let mut case = CaseAtStep::open(s.project(), s.log(), table, 0, &Unsaved::default()).unwrap();
    let steps = case.totals().steps.len();
    for n in 0..=steps {
        case.go_to(n).unwrap();
        let t = case.totals();
        assert!(t.balance.balanced, "{} {:?}", t.name, t.balance);
        assert!(close(t.balance.volume_in, 120_000.0), "{} {:?}", t.name, t.balance);
        // Going on runs one step at a time.
        if n > 0 {
            assert_eq!(t.ran.last().map(|r| r.step), Some(n));
        }
    }
    // Back a step runs from the blocks again; a step the dataflow hasn't is refused.
    case.go_to(2).unwrap();
    assert_eq!(case.totals().ran.first().map(|r| r.step), Some(0));
    assert!(case.go_to(steps + 1).is_err());
    assert!(case.step_number("no such step").is_err());
}
