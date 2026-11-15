//! `reserves.run` through the registry (decision 0072, workstream 2b): a made-up strip of two blocks at mine coordinates,
//! 100 m by 50 m each, with known answers.
//!
//! | horizon | RL      | material | B1 m³  | B2                                  | RD            | ash |
//! |---------|---------|----------|--------|-------------------------------------|---------------|-----|
//! | OB      | 20 - 30 | waste    | 50,000 | 50,000                              | 2.3 (a row)   |     |
//! | U       | 16 - 20 | coal     | 20,000 | benches 18 and 16, 10,000 m³ each   | 1.4; 1.4, 1.6 | 10  |
//! | U-L     | 15 - 16 | waste    | 5,000  | 5,000                               | 2.3 (a row)   |     |
//! | L       | 12 - 15 | coal     | 15,000 | 15,000                              | 1.5           | 20  |

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::builtin::{RunReservesParams, reserves_preview};
use zd_commands::{Code, Command, CommandError, CommandSpec, Inputs, Outcome, Registry, RunRecord, Session, Severity};
use zd_core::{Attributes, Body, Change, ChangeKind, ColumnValues, ObjectId, Table};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// Volumes from Manifold-free boxes add exactly; shares and averages to this, relative.
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

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn box_solid(s: &mut Session, name: &str, x: f64, z: (f64, f64), attributes: serde_json::Value) -> ObjectId {
    let (x0, x1, y0, y1) = (x, x + 100.0, 0.0, 50.0);
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| [E + if i & 1 == 1 { x1 } else { x0 }, N + if i >> 1 & 1 == 1 { y1 } else { y0 }, if i >> 2 & 1 == 1 { z.1 } else { z.0 }])
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let id = created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "blocks", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    s.run("test.tag", json!({ "object": id, "attributes": attributes })).unwrap();
    id
}

const STACK: [&str; 5] = ["OB", "U", "U-L", "L", "below L"];

/// The strip's blocks, B1 at x 0 and B2 at x 100; `case` tags each when given.
fn blocks(s: &mut Session, case: Option<&str>) -> Vec<ObjectId> {
    let mut ids = Vec::new();
    for (b, x) in [("B1", 0.0), ("B2", 100.0)] {
        let tag = |mut a: serde_json::Value| {
            a["strip"] = json!(3.0);
            a["block"] = json!(b);
            if let Some(c) = case {
                a["case"] = json!(c);
            }
            a
        };
        ids.push(box_solid(s, &format!("3_{b}_OB"), x, (20.0, 30.0), tag(json!({ "horizon": "OB", "material": "waste", "seam": "U" }))));
        if b == "B1" {
            ids.push(box_solid(s, &format!("3_{b}_U"), x, (16.0, 20.0), tag(json!({ "horizon": "U", "material": "coal", "seam": "U", "rd": 1.4, "ash": 10.0 }))));
        } else {
            for (floor, rd) in [(18.0, 1.4), (16.0, 1.6)] {
                let a = tag(json!({ "horizon": "U", "material": "coal", "seam": "U", "rd": rd, "ash": 10.0, "bench": floor, "bench_floor": floor, "bench_top": floor + 2.0 }));
                ids.push(box_solid(s, &format!("3_{b}_U_{floor}"), x, (floor, floor + 2.0), a));
            }
        }
        ids.push(box_solid(s, &format!("3_{b}_U-L"), x, (15.0, 16.0), tag(json!({ "horizon": "U-L", "material": "waste", "seam": "L" }))));
        ids.push(box_solid(s, &format!("3_{b}_L"), x, (12.0, 15.0), tag(json!({ "horizon": "L", "material": "coal", "seam": "L", "rd": 1.5, "ash": 20.0 }))));
    }
    ids
}

fn dataflow(s: &mut Session, steps: &[&str]) -> ObjectId {
    let steps: Vec<serde_json::Value> = steps.iter().map(|s| json!({ "step": s })).collect();
    flow(s, json!(steps), "when material,when seam,rd (t/m3),ash_cutoff (%)\nwaste,,2.3,\n,,,35\n")
}

/// A dataflow of the usual fields, these steps (with any settings) and a parameter table from this CSV.
fn flow(s: &mut Session, steps: serde_json::Value, csv: &str) -> ObjectId {
    let table = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": csv })).unwrap())[0];
    let content = json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "area", "kind": "area", "add_up": { "by": "max" }, "from": "plan_area" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
            { "name": "ash_cutoff", "kind": "setting", "add_up": { "by": "min" }, "from": "parameter", "column": "ash_cutoff" },
        ],
        "steps": steps,
        "parameters": [table],
    });
    created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": content })).unwrap())[0]
}

fn session() -> Session {
    let mut registry = Registry::with_builtins();
    registry.register(Tag);
    Session::new(registry)
}

fn table(s: &Session, id: ObjectId) -> Table {
    let Body::Table(t) = &s.project().object(id).unwrap().body else { panic!("a table") };
    (**t).clone()
}

fn texts(t: &Table, name: &str) -> Vec<Option<String>> {
    let ColumnValues::Text(v) = &t.column(name).unwrap().values else { panic!("words") };
    v.clone()
}

fn numbers(t: &Table, name: &str) -> Vec<Option<f64>> {
    let ColumnValues::Number(v) = &t.column(name).unwrap().values else { panic!("numbers") };
    v.clone()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE * b.abs().max(1.0)
}

/// The row of a block, horizon and bench.
fn row(t: &Table, block: &str, horizon: &str, bench: Option<&str>) -> usize {
    let (blocks, horizons, benches) = (texts(t, "block"), texts(t, "horizon"), texts(t, "bench"));
    (0..t.rows()).find(|&i| blocks[i].as_deref() == Some(block) && horizons[i].as_deref() == Some(horizon) && benches[i].as_deref() == bench).expect("a row")
}

#[test]
fn each_block_s_horizons_and_benches_are_written_in_situ_with_their_own_values() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = dataflow(&mut s, &["initialise", "in_situ", "join_benches"]);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    println!("{}", record.messages.join("\n"));
    let made = created(&record);
    assert_eq!(made.len(), 1, "one table, Base");
    assert_eq!(s.project().object(made[0]).unwrap().name, "Reserves Base");
    let t = table(&s, made[0]);
    assert_eq!(t.rows(), 9, "B1's four horizons, B2's with U's two benches");
    assert!(record.messages[0].starts_with("2 of 2 blocks ran through initialise, in_situ, join_benches; 180000.0 m3 in"), "{}", record.messages[0]);

    // Rows top down by the stack in each block.
    let b1: Vec<String> = (0..t.rows()).filter(|&i| texts(&t, "block")[i].as_deref() == Some("B1")).map(|i| texts(&t, "horizon")[i].clone().unwrap()).collect();
    assert_eq!(b1, ["OB", "U", "U-L", "L"]);
    let (volume, thickness, area, rd, ash, tonnes, cutoff) =
        (numbers(&t, "volume"), numbers(&t, "thickness"), numbers(&t, "area"), numbers(&t, "rd"), numbers(&t, "ash"), numbers(&t, "tonnes"), numbers(&t, "ash_cutoff"));
    let u = row(&t, "B1", "U", None);
    assert!(close(volume[u].unwrap(), 20_000.0) && close(thickness[u].unwrap(), 4.0) && close(area[u].unwrap(), 5_000.0));
    assert!(close(rd[u].unwrap(), 1.4) && close(tonnes[u].unwrap(), 28_000.0) && close(ash[u].unwrap(), 10.0) && close(cutoff[u].unwrap(), 35.0));

    // B2's U benches written as they are, each with its own RD and tonnes: in situ comes before the join.
    let (top, bottom) = (row(&t, "B2", "U", Some("18")), row(&t, "B2", "U", Some("16")));
    assert!(top < bottom, "benches top down");
    assert!(close(volume[top].unwrap(), 10_000.0) && close(rd[top].unwrap(), 1.4) && close(tonnes[top].unwrap(), 14_000.0));
    assert!(close(volume[bottom].unwrap(), 10_000.0) && close(rd[bottom].unwrap(), 1.6) && close(tonnes[bottom].unwrap(), 16_000.0));
    assert_eq!(texts(&t, "from")[top].as_deref(), Some("3_B2_U_18"));

    // Waste RD from the parameter row; waste has no ash, said once; everything adds up to the blocks.
    let ob = row(&t, "B1", "OB", None);
    assert!(close(rd[ob].unwrap(), 2.3) && close(tonnes[ob].unwrap(), 115_000.0) && ash[ob].is_none());
    let total: f64 = volume.iter().map(|v| v.unwrap()).sum();
    assert!(close(total, 180_000.0));
    let missing: Vec<&str> = record.notes.iter().filter(|n| n.code == Some(Code::ReservesValueMissing)).map(|n| n.text.as_str()).collect();
    assert_eq!(missing, ["4 rows written with no ash: no block attribute or parameter row gave it"]);
    assert!(record.notes.iter().all(|n| n.severity < Severity::Warning), "{:?}", record.notes);
    // How each column adds up goes with it, for the report.
    assert_eq!(t.column("ash").unwrap().add_up, Some(zd_reserves::AddUp::Mass { density: "rd".into() }));
    assert_eq!(t.column("thickness").unwrap().add_up, None, "a thickness doesn't add up across blocks");
}

#[test]
fn a_step_s_own_rows_are_read_first_and_cases_get_a_table_each() {
    let mut s = session();
    let mut ids = blocks(&mut s, None);
    ids.extend(blocks(&mut s, Some("50m")));
    let flow = dataflow(&mut s, &["initialise", "in_situ"]);
    let own = json!({ "filters": ["seam"], "columns": [{ "name": "ash_cutoff", "unit": "%" }], "rows": [{ "when": { "seam": "U" }, "values": { "ash_cutoff": 30.0 } }] });
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves", "parameters": own })).unwrap();
    let made = created(&record);
    let names: Vec<String> = made.iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    assert_eq!(names, ["Reserves 50m", "Reserves Base"]);
    let t = table(&s, made[1]);
    let cutoff = numbers(&t, "ash_cutoff");
    assert!(close(cutoff[row(&t, "B1", "U", None)].unwrap(), 30.0), "the step's row for U");
    assert!(close(cutoff[row(&t, "B1", "L", None)].unwrap(), 35.0), "the dataflow's table for L");
    assert_eq!(texts(&table(&s, made[0]), "case")[0].as_deref(), Some("50m"));
}

#[test]
fn a_block_that_can_t_run_is_left_out_with_why_and_the_rest_go_on() {
    let mut s = session();
    let mut ids = blocks(&mut s, None);
    // A horizon the stack doesn't have, in B1.
    ids.push(box_solid(&mut s, "3_B1_X", 0.0, (10.0, 12.0), json!({ "strip": 3.0, "block": "B1", "horizon": "X", "material": "waste" })));
    let flow = dataflow(&mut s, &["initialise", "in_situ"]);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    assert!(record.messages[0].starts_with("1 of 2 blocks ran"), "{}", record.messages[0]);
    let stopped: Vec<&str> = record.notes.iter().filter(|n| n.code == Some(Code::ReservesBlockStopped)).map(|n| n.text.as_str()).collect();
    assert_eq!(stopped, ["1 block left out: 3_B1_X: its horizon 'X' isn't in the stack (OB, U, U-L, L, below L)"]);
    let t = table(&s, created(&record)[0]);
    assert!(texts(&t, "block").iter().all(|b| b.as_deref() == Some("B2")));
}

#[test]
fn a_step_this_build_doesn_t_have_or_no_stack_is_refused() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = dataflow(&mut s, &["initialise", "working_sections"]);
    let err = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap_err();
    assert!(err.to_string().contains("names a step working_sections, which this build doesn't have; it has small_volumes, initialise, in_situ, join_benches"), "{err}");
    let flow = dataflow(&mut s, &["initialise"]);
    let err = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "layer": "reserves" })).unwrap_err();
    assert!(err.to_string().contains("no stack order"), "{err}");
}

#[test]
fn the_preview_shows_a_block_s_column_after_each_step() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = dataflow(&mut s, &["initialise", "in_situ", "join_benches"]);
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let project = s.project();
    let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
    let views = reserves_preview(&get, &p, "Base", "3", "B2").unwrap();
    let steps: Vec<&str> = views.iter().map(|v| v.step.as_str()).collect();
    assert_eq!(steps, ["as input", "initialise", "in_situ", "join_benches"]);
    let horizons = |i: usize| views[i].units.iter().map(|u| u.horizon.clone()).collect::<Vec<_>>();
    assert_eq!(horizons(1), ["OB", "U", "U", "U-L", "L", "underburden"], "sorted, U's benches together, underburden below L");
    assert_eq!(horizons(3), ["OB", "U", "U-L", "L", "underburden"], "U's benches joined");
    let u = &views[3].units[1];
    assert_eq!(u.joined, [("18".to_string(), 0.5), ("16".to_string(), 0.5)]);
    assert!(close(u.values["rd"], 1.5) && close(u.values["volume"], 20_000.0));
}

// Decision 0073, workstream 3a: small volumes, initialise's checks, underburden and quality defaults.

const STEPS_3A: [&str; 3] = ["small_volumes", "initialise", "in_situ"];

fn notes_of(record: &RunRecord, code: Code) -> Vec<String> {
    record.notes.iter().filter(|n| n.code == Some(code)).map(|n| n.text.clone()).collect()
}

/// Waste under 6,000 m³ is a small volume: each block's U-L (5,000 m³) is set aside and written as such, never in situ.
/// The step's setting names the parameter column: here `smallest`, a site's own name.
#[test]
fn small_volumes_are_set_aside_and_written_never_dropped() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let steps = json!([{ "step": "small_volumes", "settings": { "min_volume": "smallest" } }, { "step": "initialise" }, { "step": "in_situ" }]);
    let flow = flow(&mut s, steps, "when material,rd (t/m3),smallest (m3),underburden_thickness (m)
waste,2.3,6000,
,,,0
");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let (write, horizon, became, volume) = (texts(&t, "write"), texts(&t, "horizon"), texts(&t, "became"), numbers(&t, "volume"));
    let small: Vec<usize> = (0..t.rows()).filter(|&i| write[i].as_deref() == Some("small_volume")).collect();
    assert_eq!(small.len(), 2);
    assert!(small.iter().all(|&i| horizon[i].as_deref() == Some("U-L") && became[i].as_deref() == Some("small volume") && close(volume[i].unwrap(), 5_000.0)));
    assert!((0..t.rows()).all(|i| write[i].as_deref() != Some("in_situ") || horizon[i].as_deref() != Some("U-L")), "not in situ");
    let total: f64 = volume.iter().map(|v| v.unwrap()).sum();
    assert!(close(total, 180_000.0), "every block written once, at one write point or the other");
    assert_eq!(notes_of(&record, Code::ReservesSmallVolume), ["2 units set aside as small volumes, 10000.0 m3"]);
}

/// Each column's last unit is L coal, so 10 m of underburden goes under it, 5,000 m² x 10 m = 50,000 m³ a block, out of
/// the model: never written, not in the balance. 0 m turns it off; real waste below L means none is added.
#[test]
fn underburden_goes_below_the_last_coal_out_of_the_model() {
    let mut s = session();
    let mut ids = blocks(&mut s, None);
    let flow = dataflow(&mut s, &STEPS_3A);
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let project = s.project();
    let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
    let views = reserves_preview(&get, &p, "Base", "3", "B1").unwrap();
    let ub = views[2].units.last().unwrap();
    assert_eq!((ub.horizon.as_str(), ub.material.as_str()), ("underburden", "waste"));
    assert!(close(ub.values["volume"], 50_000.0) && close(ub.values["thickness"], 10.0) && close(ub.values["rd"], 2.3), "{:?}", ub.values);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    assert_eq!(notes_of(&record, Code::ReservesUnderburden), ["2 units underburden added below the last coal, out of model, 100000.0 m3"]);
    assert!(record.messages[0].contains("180000.0 m3 in, every column balanced to 0.0e0"), "{}", record.messages[0]);
    let t = table(&s, created(&record)[0]);
    assert!(texts(&t, "horizon").iter().all(|h| h.as_deref() != Some("underburden")), "never written");

    // Real waste below L in B1: none added there.
    ids.push(box_solid(&mut s, "3_B1_below L", 0.0, (5.0, 12.0), json!({ "strip": 3.0, "block": "B1", "horizon": "below L", "material": "waste" })));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    assert_eq!(notes_of(&record, Code::ReservesUnderburden), ["1 unit underburden added below the last coal, out of model, 50000.0 m3"]);
}

/// Ash limits 5 to 10 % and a default of 12 % for coal, 60 % for waste: L's 20 % is outside and takes 12; U's 10 % is
/// on the limit and stays, limits being inclusive; waste takes 60 outright. Coal thicker than 3.5 m is named.
#[test]
fn qualities_take_their_limits_and_defaults_and_sizes_are_checked() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let csv = "when material,rd (t/m3),ash_min (%),ash_max (%),ash_default (%),max_thickness (m)
waste,2.3,,,60,
coal,,5,10,12,3.5
";
    let flow = flow(&mut s, json!(STEPS_3A.map(|s| json!({ "step": s }))), csv);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let ash = numbers(&t, "ash");
    assert!(close(ash[row(&t, "B1", "L", None)].unwrap(), 12.0), "outside its limits");
    assert!(close(ash[row(&t, "B1", "U", None)].unwrap(), 10.0), "on its limit, kept");
    assert!(close(ash[row(&t, "B1", "OB", None)].unwrap(), 60.0), "waste takes its default");
    let mut said = notes_of(&record, Code::ReservesDefaulted);
    said.sort();
    assert_eq!(said, ["2 units ash: coal outside its limits given its default, 30000.0 m3", "6 units ash: waste given its default, 210000.0 m3"], "the underburden too, as Spry");
    assert_eq!(notes_of(&record, Code::ReservesOutOfRange), ["1 block thicker than max_thickness: 3_B1_U"]);
}
