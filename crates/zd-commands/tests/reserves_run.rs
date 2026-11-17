//! `reserves.run` through the registry (decision 0072, workstream 2b): a made-up strip of two blocks at mine coordinates,
//! 100 m by 50 m each, with known answers.
//!
//! | horizon | RL      | material | B1 m³  | B2                                  | RD            | ash |
//! |---------|---------|----------|--------|-------------------------------------|---------------|-----|
//! | OB      | 20 - 30 | waste    | 50,000 | 50,000                              | 2.3 (a row)   |     |
//! | U       | 16 - 20 | coal     | 20,000 | benches 18 and 16, 10,000 m³ each   | 1.4; 1.4, 1.6 | 10  |
//! | U-L     | 15 - 16 | waste    | 5,000  | 5,000                               | 2.3 (a row)   |     |
//! | L       | 12 - 15 | coal     | 15,000 | 15,000                              | 1.5           | 20  |
//!
//! Since decision 0074 the results are a reserves table: a row a leaf (case, pit, strip, block, seam, bench), the waste
//! above a seam on the seam's leaf (OB on U's, U-L on L's), and a column a field under its folder, `Insitu\Coal\volume`.
//! B1 has two leaves, U and L; B2 four, U unbenched (holding OB), U's benches 18 and 16, and L.

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::builtin::{RunReservesParams, StepKind, column_trace, reserves_preview, table_column};
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
            { "name": "length", "kind": "length", "add_up": { "by": "max" }, "from": "attribute", "attribute": "length" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
            { "name": "ash_cutoff", "kind": "setting", "add_up": { "by": "min" }, "from": "parameter", "column": "ash_cutoff" },
        ],
        "steps": steps,
        "parameters": [table],
    });
    created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": starter(content) })).unwrap())[0]
}

/// A dataflow's content given the starter formulas after `initialise`, as `dataflow_starter` gives them (decision 0077,
/// 7c): limits and defaults, size filled in, and the densities where it has moistures.
fn starter(content: serde_json::Value) -> serde_json::Value {
    let mut d: zd_core::Dataflow = serde_json::from_value(content).unwrap();
    zd_commands::builtin::with_starter_formulas(&mut d);
    serde_json::to_value(d).unwrap()
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

/// The leaf of a block, seam and bench.
fn leaf(t: &Table, block: &str, seam: &str, bench: Option<&str>) -> usize {
    let (blocks, seams, benches) = (texts(t, "block"), texts(t, "seam"), texts(t, "bench"));
    (0..t.rows()).find(|&i| blocks[i].as_deref() == Some(block) && seams[i].as_deref() == Some(seam) && benches[i].as_deref() == bench).expect("a leaf")
}

/// A field's value on a leaf, by its folder path; none where the leaf has none or no leaf has the column.
fn value(t: &Table, at: usize, path: &str) -> Option<f64> {
    t.column(path).and_then(|c| match &c.values {
        ColumnValues::Number(v) => v[at],
        _ => None,
    })
}

/// A column summed over every leaf.
fn total(t: &Table, path: &str) -> f64 {
    t.column(path).map_or(0.0, |c| match &c.values {
        ColumnValues::Number(v) => v.iter().flatten().sum(),
        _ => 0.0,
    })
}

const COAL: &str = "Insitu\\Coal";
const WASTE: &str = "Insitu\\Waste";

fn at(folder: &str, field: &str) -> String {
    format!("{folder}\\{field}")
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
    assert_eq!(t.rows(), 6, "B1's U and L; B2's U, its benches 18 and 16, and L");
    assert!(record.messages[0].starts_with("2 of 2 blocks ran through initialise, fill_size, limits, in_situ, join_benches; 180000.0 m3 in"), "{}", record.messages[0]);
    assert_eq!(record.messages[1], "Reserves Base: 2 blocks, 6 leaves");
    let levels: Vec<&str> = t.columns.iter().take(6).map(|c| c.name.as_str()).collect();
    assert_eq!(levels, ["case", "pit", "strip", "block", "seam", "bench"], "the default levels");
    assert_eq!(texts(&t, "pit")[0].as_deref(), Some("Pit"), "no pit attribute: the level's default");

    // B1's U leaf: its coal and the overburden above it, side by side.
    let u = leaf(&t, "B1", "U", None);
    let coal = |f: &str| value(&t, u, &at(COAL, f)).unwrap();
    assert!(close(coal("volume"), 20_000.0) && close(coal("thickness"), 4.0) && close(coal("area"), 5_000.0));
    assert!(close(coal("rd"), 1.4) && close(coal("tonnes"), 28_000.0) && close(coal("ash"), 10.0) && close(coal("ash_cutoff"), 35.0));
    assert!(close(value(&t, u, &at(WASTE, "volume")).unwrap(), 50_000.0) && close(value(&t, u, &at(WASTE, "tonnes")).unwrap(), 115_000.0));
    assert!(close(value(&t, u, &at(WASTE, "rd")).unwrap(), 2.3), "waste RD from the parameter row");
    assert!(t.column(&at(WASTE, "ash")).is_none(), "no waste has an ash, so no column");

    // B2's U benches, each with its own RD and tonnes: in situ comes before the join. B2's OB on U unbenched.
    let (top, bottom, unbenched) = (leaf(&t, "B2", "U", Some("18")), leaf(&t, "B2", "U", Some("16")), leaf(&t, "B2", "U", None));
    assert!(unbenched < top && top < bottom, "top down");
    assert!(close(value(&t, top, &at(COAL, "volume")).unwrap(), 10_000.0) && close(value(&t, top, &at(COAL, "rd")).unwrap(), 1.4) && close(value(&t, top, &at(COAL, "tonnes")).unwrap(), 14_000.0));
    assert!(close(value(&t, bottom, &at(COAL, "tonnes")).unwrap(), 16_000.0) && value(&t, bottom, &at(WASTE, "volume")).is_none());
    assert!(close(value(&t, unbenched, &at(WASTE, "volume")).unwrap(), 50_000.0) && value(&t, unbenched, &at(COAL, "volume")).is_none());

    // Imported: each block as read; everything adds up to the blocks either way.
    assert!(close(total(&t, "Imported\\Coal\\volume"), 70_000.0) && close(total(&t, "Imported\\Waste\\volume"), 110_000.0));
    assert!(close(total(&t, &at(COAL, "volume")) + total(&t, &at(WASTE, "volume")), 180_000.0));
    let missing: Vec<&str> = record.notes.iter().filter(|n| n.code == Some(Code::ReservesValueMissing)).map(|n| n.text.as_str()).collect();
    assert_eq!(missing, ["9 in situ pieces with no length: no block attribute or parameter row gave it", "4 in situ pieces with no ash: no block attribute or parameter row gave it"], "these blocks were never measured");
    assert!(record.notes.iter().all(|n| n.severity < Severity::Warning), "{:?}", record.notes);
    // How each column adds up goes with it, for any summary.
    assert_eq!(t.column(&at(COAL, "ash")).unwrap().add_up, Some(zd_reserves::AddUp::Mass { density: "rd".into() }));
    assert_eq!(t.column(&at(COAL, "thickness")).unwrap().add_up, None, "a thickness doesn't add up across leaves");
}

/// The levels are the dataflow's: here pit, strip, block and seam, no bench, each block in pit West, so B2's U benches
/// share one leaf, 20,000 m³ at RD 1.5 by volume; a block with no strip and no default is left out, said.
#[test]
fn the_levels_set_the_columns_and_the_leaves() {
    let mut s = session();
    let mut ids = blocks(&mut s, None);
    ids.push(box_solid(&mut s, "stray", 300.0, (12.0, 15.0), json!({ "block": "B9", "horizon": "L", "material": "coal", "seam": "L", "rd": 1.5 })));
    let table_id = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": "when material,rd (t/m3),underburden_thickness (m)\nwaste,2.3,\n,,0\n" })).unwrap())[0];
    let content = json!({
        "levels": [{ "attribute": "pit", "default": "West" }, { "attribute": "strip" }, { "attribute": "block" }, { "attribute": "seam" }],
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
        ],
        "steps": [{ "step": "initialise" }, { "step": "in_situ" }],
        "parameters": [table_id],
    });
    let flow = created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": starter(content) })).unwrap())[0];
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let levels: Vec<&str> = t.columns.iter().take(5).map(|c| c.name.as_str()).collect();
    assert_eq!(levels, ["case", "pit", "strip", "block", "seam"]);
    assert_eq!(t.rows(), 4, "B1 and B2, U and L each");
    assert!(texts(&t, "pit").iter().all(|p| p.as_deref() == Some("West")));
    let blocks = texts(&t, "block");
    let u2 = (0..t.rows()).find(|&i| blocks[i].as_deref() == Some("B2") && texts(&t, "seam")[i].as_deref() == Some("U")).unwrap();
    assert!(close(value(&t, u2, &at(COAL, "volume")).unwrap(), 20_000.0) && close(value(&t, u2, &at(COAL, "rd")).unwrap(), 1.5));
    let stopped: Vec<&str> = record.notes.iter().filter(|n| n.code == Some(Code::ReservesBlockStopped)).map(|n| n.text.as_str()).collect();
    assert_eq!(stopped, ["1 solid with no strip and no default for it left out: stray"]);
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
    assert!(close(value(&t, leaf(&t, "B1", "U", None), &at(COAL, "ash_cutoff")).unwrap(), 30.0), "the step's row for U");
    assert!(close(value(&t, leaf(&t, "B1", "L", None), &at(COAL, "ash_cutoff")).unwrap(), 35.0), "the dataflow's table for L");
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
    let flow = dataflow(&mut s, &["initialise", "product"]);
    let err = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap_err();
    assert!(err.to_string().contains("names a step product, which this build doesn't have; it has small_volumes, initialise, in_situ, join_benches, working_sections, rom_loss, rom_dilution, rom_ash_cutoff, rom, final"), "{err}");
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
    let path = ["Pit".to_string(), "3".to_string(), "B2".to_string()];
    let views = reserves_preview(&get, &p, "Base", &path).unwrap();
    let steps: Vec<&str> = views.iter().map(|v| v.step.as_str()).collect();
    assert_eq!(steps, ["as input", "initialise", "fill_size", "limits", "in_situ", "join_benches"], "the starter formulas after initialise");
    let horizons = |i: usize| views[i].units.iter().map(|u| u.horizon.clone()).collect::<Vec<_>>();
    assert_eq!(horizons(1), ["OB", "U", "U", "U-L", "L", "underburden"], "sorted, U's benches together, underburden below L");
    assert_eq!(horizons(5), ["OB", "U", "U-L", "L", "underburden"], "U's benches joined");
    let u = &views[5].units[1];
    assert_eq!(u.joined, [("18".to_string(), 0.5), ("16".to_string(), 0.5)]);
    assert!(close(u.values["rd"], 1.5) && close(u.values["volume"], 20_000.0));
}

/// Open its column (decision 0075, 5b): a bench of a reserves table opens its block's column, run again with the table's
/// own run's settings from the log. B2's units keep their ids through the steps but for U's two benches, which
/// `join_benches` takes away and makes one new unit of, and the underburden `initialise` adds; each step says what it
/// did to this column alone. A strip is above a column and refused; a block revised since is named as changed.
#[test]
fn a_table_s_bench_opens_its_block_s_column_step_by_step() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = dataflow(&mut s, &["initialise", "in_situ", "join_benches"]);
    let params = json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" });
    let record = s.run("reserves.run", params.clone()).unwrap();
    let id = created(&record)[0];
    let node: Vec<String> = ["Pit", "3", "B2", "U", "18"].map(String::from).to_vec();
    let opened = table_column(s.project(), s.log(), id, &node, None).unwrap();
    assert_eq!(opened.column, ["Pit", "3", "B2"]);
    assert_eq!((opened.case.as_str(), opened.dataflow.as_str()), ("Base", "Reserves"));
    assert_eq!(opened.dataflow_id, flow, "the saved dataflow, where its formulas are edited");
    assert!(opened.changed.is_empty(), "{:?}", opened.changed);
    // Its inputs (decision 0084): B2's five block solids top down, and the run's own stack, no seam model.
    assert_eq!(opened.blocks, ["3_B2_OB", "3_B2_U_18", "3_B2_U_16", "3_B2_U-L", "3_B2_L"]);
    assert_eq!((opened.model.as_deref(), opened.horizons), (None, STACK.len()));

    // The same column as the preview gives, step for step.
    let p: RunReservesParams = serde_json::from_value(params).unwrap();
    let project = s.project();
    let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
    let preview = reserves_preview(&get, &p, "Base", &opened.column).unwrap();
    let steps: Vec<&str> = opened.steps.iter().map(|v| v.step.as_str()).collect();
    assert_eq!(steps, ["as input", "initialise", "fill_size", "limits", "in_situ", "join_benches"]);
    for (a, b) in opened.steps.iter().zip(&preview) {
        assert_eq!(a.units.len(), b.units.len());
        for (u, w) in a.units.iter().zip(&b.units) {
            assert_eq!((u.id, &u.horizon, &u.bench), (w.id, &w.horizon, &w.bench));
            assert!(u.values.iter().all(|(k, v)| v.to_bits() == w.values[k].to_bits() || (v.is_nan() && w.values[k].is_nan())), "{k:?}", k = a.step);
        }
    }

    // Ids: input OB, U 18, U 16, U-L, L; underburden new at initialise; U's benches gone at join_benches, one new U.
    let ids_at = |i: usize| opened.steps[i].units.iter().map(|u| (u.id, u.horizon.clone(), u.bench.clone())).collect::<Vec<_>>();
    let input = ids_at(0);
    assert_eq!(input.iter().map(|(_, h, b)| format!("{h} {}", b.clone().unwrap_or_default())).collect::<Vec<_>>(), ["OB ", "U 18", "U 16", "U-L ", "L "]);
    let initialised = ids_at(1);
    assert_eq!(&initialised[..5], &input[..], "initialise keeps every unit, in order");
    assert!(initialised[5].1 == "underburden" && !input.iter().any(|(id, ..)| *id == initialised[5].0), "the underburden is a new unit");
    let joined = ids_at(5);
    let kept: Vec<u32> = [0, 3, 4].map(|i| input[i].0).to_vec();
    assert_eq!(joined.iter().filter(|(id, ..)| kept.contains(id)).count(), 3, "OB, U-L and L keep their ids");
    assert!(!joined.iter().any(|(id, ..)| *id == input[1].0 || *id == input[2].0), "U's benches are gone after the join");
    let u = &opened.steps[5].units[1];
    assert!(!input.iter().any(|(id, ..)| *id == u.id) && u.joined == [("18".to_string(), 0.5), ("16".to_string(), 0.5)]);
    assert!(close(u.values["volume"], 20_000.0) && close(u.values["rd"], 1.5));

    // What each step said, of this column only: one underburden of 5,000 m² x 10 m.
    assert!(opened.steps[0].said.is_empty() && opened.steps[5].said.is_empty(), "{:?}", opened.steps[0].said);
    assert_eq!(opened.steps[1].said, ["1 unit underburden added below the last coal, out of model, 50000.0 m3"]);

    // A strip is above a column.
    let err = table_column(s.project(), s.log(), id, &["Pit".to_string(), "3".to_string()], None).unwrap_err();
    assert!(err.to_string().contains("pick a pit, strip, block or below it"), "{err}");

    // A block revised since the table was made is named, and the column runs on it as it is now.
    let l = ids[ids.len() - 1];
    s.run("test.tag", json!({ "object": l, "attributes": { "ash": 25.0 } })).unwrap();
    let opened = table_column(s.project(), s.log(), id, &node, None).unwrap();
    assert_eq!(opened.changed, ["3_B2_L"]);
    let l_unit = opened.steps[0].units.iter().find(|u| u.horizon == "L").unwrap();
    assert!(close(l_unit.values["ash"], 25.0));
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
    for block in ["B1", "B2"] {
        let l = leaf(&t, block, "L", None);
        assert!(close(value(&t, l, "Small volume\\Waste\\volume").unwrap(), 5_000.0), "U-L on L's leaf as a small volume");
        assert!(value(&t, l, &at(WASTE, "volume")).is_none(), "and not in situ");
    }
    let written = total(&t, &at(COAL, "volume")) + total(&t, &at(WASTE, "volume")) + total(&t, "Small volume\\Waste\\volume");
    assert!(close(written, 180_000.0), "every block written once, in situ or as a small volume");
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
    let views = reserves_preview(&get, &p, "Base", &["Pit".to_string(), "3".to_string(), "B1".to_string()]).unwrap();
    let ub = views[2].units.last().unwrap();
    assert_eq!((ub.horizon.as_str(), ub.material.as_str()), ("underburden", "waste"));
    assert!(close(ub.values["volume"], 50_000.0) && close(ub.values["thickness"], 10.0) && close(ub.values["rd"], 2.3), "{:?}", ub.values);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    assert_eq!(notes_of(&record, Code::ReservesUnderburden), ["2 units underburden added below the last coal, out of model, 100000.0 m3"]);
    assert!(record.messages[0].contains("180000.0 m3 in, every column balanced to 0.0e0"), "{}", record.messages[0]);
    let t = table(&s, created(&record)[0]);
    assert!(close(total(&t, &at(COAL, "volume")) + total(&t, &at(WASTE, "volume")), 180_000.0), "the underburden never written");
    assert_eq!(t.rows(), 6);

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
    assert!(close(value(&t, leaf(&t, "B1", "L", None), &at(COAL, "ash")).unwrap(), 12.0), "outside its limits");
    assert!(close(value(&t, leaf(&t, "B1", "U", None), &at(COAL, "ash")).unwrap(), 10.0), "on its limit, kept");
    assert!(close(value(&t, leaf(&t, "B1", "U", None), &at(WASTE, "ash")).unwrap(), 60.0), "waste takes its default");
    // Said by the starter's limits formula (decision 0077, 7c), as the Rust said it before.
    let mut said = notes_of(&record, Code::ReservesFormulaSaid);
    said.sort();
    assert_eq!(said, ["2 units printed by limits: ash: coal outside its limits given its default, 30000.0 m3", "6 units printed by limits: ash: waste given its default, 210000.0 m3"], "the underburden too, as Spry");
    assert_eq!(notes_of(&record, Code::ReservesOutOfRange), ["1 block thicker than max_thickness: 3_B1_U"]);
}

// Decision 0073, workstream 3b: moisture, Preston-Sanders and the in situ basis.

/// The plan's hand check: RD 1.45, Mad 3 %, Mis 8 % gives 1.45 × 0.97 ÷ (1 + 1.45 × 0.05 − 0.08) = 1.4065 ÷ 0.9925.
#[test]
fn preston_sanders_by_hand() {
    let dis = zd_commands::builtin::preston_sanders(1.45, 0.03, 0.08);
    assert!(close(dis, 1.4065 / 0.9925) && (dis - 1.4171).abs() < 5e-5, "{dis}");
}

/// The usual fields, the moistures and the worked densities, every step to in situ, and a table from this CSV.
fn moist_flow(s: &mut Session, csv: &str) -> ObjectId {
    let table = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": csv })).unwrap())[0];
    let content = json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "area", "kind": "area", "add_up": { "by": "max" }, "from": "plan_area" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
            { "name": "mad", "kind": "moisture", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "mad" },
            { "name": "rd_is", "kind": "density", "add_up": { "by": "volume" }, "from": "worked" },
            { "name": "rd_rom", "kind": "density", "add_up": { "by": "volume" }, "from": "worked" },
            { "name": "mis", "kind": "moisture", "add_up": { "by": "mass", "density": "rd_is" }, "from": "attribute", "attribute": "mis" },
            { "name": "mrom", "kind": "moisture", "add_up": { "by": "mass", "density": "rd_rom" }, "from": "parameter", "column": "mrom" },
        ],
        "steps": [{ "step": "initialise" }, { "step": "in_situ" }],
        "parameters": [table],
    });
    created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": starter(content) })).unwrap())[0]
}

/// Coal at Mad 3 %, Mis 8 %, Mrom 10 %; waste RD 2.3 in situ as given. B1's U, RD 1.4: in situ density
/// 1.4 × 0.97 ÷ (1 + 1.4 × 0.05 − 0.08) = 1.358 ÷ 0.99, so 20,000 m³ is 27,434.3 t in situ against 28,000 t air dried;
/// its ash of 10 % air dried is 10 × 0.92 ÷ 0.97 in situ; its ROM density the in situ × 0.92 ÷ 0.90. B2's U benches each
/// on their own RD. OB 50,000 m³ at 2.3 is 115,000 t either way.
#[test]
fn coal_is_written_on_the_in_situ_basis_by_preston_sanders() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = moist_flow(&mut s, "when material,rd (t/m3),mad (%),mis (%),mrom (%),underburden_thickness (m)\nwaste,2.3,,,,0\ncoal,,3,8,10,\n");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let dis = |rd: f64| rd * 0.97 / (1.0 + rd * 0.05 - 0.08);
    let u = leaf(&t, "B1", "U", None);
    let coal = |f: &str| value(&t, u, &at(COAL, f)).unwrap();
    // Air-dried tonnes Spry's way (decision 0077, point 5): the in situ tonnes × (100 − Mis) ÷ (100 − Mad), not volume × RD.
    assert!(close(coal("rd_is"), 1.358 / 0.99) && close(coal("tonnes"), 20_000.0 * 1.358 / 0.99) && close(coal("tonnes_ad"), 20_000.0 * 1.358 / 0.99 * 92.0 / 97.0));
    assert!(close(coal("ash_is"), 10.0 * 0.92 / 0.97) && close(coal("rd_rom"), dis(1.4) * 0.92 / 0.90));
    for (bench, rd) in [("18", 1.4), ("16", 1.6)] {
        let b = leaf(&t, "B2", "U", Some(bench));
        assert!(close(value(&t, b, &at(COAL, "tonnes")).unwrap(), 10_000.0 * dis(rd)), "bench {bench}");
    }
    assert!(close(value(&t, u, &at(WASTE, "tonnes")).unwrap(), 115_000.0) && close(value(&t, u, &at(WASTE, "rd_rom")).unwrap(), 2.3));
    assert_eq!(t.column(&at(COAL, "ash_is")).unwrap().add_up, Some(zd_reserves::AddUp::Mass { density: at(COAL, "rd_is") }));
}

/// The notebook's formula cells (decision 0081): `densities` traced on B1's U (RD 1.4, Mad 3 %, Mis 8 %) sets rd_is to
/// Preston-Sanders' 1.358 ÷ 0.99 on a line that ran. A built-in step has no lines to trace. Edited and not saved (rd_is
/// doubled, the filter coal only), the cell gives the edited value and OB isn't taken, with no lines run; the worked
/// examples it now fails are said, not refused, though saving it is; the saved dataflow still gives 1.358 ÷ 0.99.
#[test]
fn a_notebook_traces_a_formula_cell_saved_or_as_edited() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = moist_flow(&mut s, "when material,rd (t/m3),mad (%),mis (%),mrom (%),underburden_thickness (m)\nwaste,2.3,,,,0\ncoal,,3,8,10,\n");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let id = created(&record)[0];
    let node: Vec<String> = ["Pit", "3", "B1", "U"].map(String::from).to_vec();
    let opened = table_column(s.project(), s.log(), id, &node, None).unwrap();
    let at = opened.steps.iter().position(|c| c.step == "densities").unwrap();
    let cell = &opened.steps[at];
    assert_eq!(cell.kind, StepKind::Formula);
    assert!(cell.source.as_deref().is_some_and(|f| f.contains("in_situ_rd(")) && cell.rule.is_empty() && cell.reads.is_empty());
    assert_eq!(cell.fits.len(), opened.steps[at - 1].units.len(), "no filter: every unit");
    // The fields it reads, off its source (decision 0084): RD and the moistures, not the densities it only sets.
    assert_eq!(cell.fields_read[..3], ["rd", "mad", "mis"], "{:?}", cell.fields_read);
    assert!(!cell.fields_read.iter().any(|f| f.starts_with("rd_")), "{:?}", cell.fields_read);
    let unit_id =|c: &zd_commands::builtin::StepView, h: &str| c.units.iter().find(|u| u.horizon == h).unwrap().id;
    let u = unit_id(&opened.steps[at - 1], "U");
    let rd_is = 1.358 / 0.99;
    let trace = column_trace(s.project(), s.log(), id, &node, at, u, None).unwrap();
    let set = |t: &zd_commands::builtin::FormulaTraceView| t.sets.iter().find(|(k, _)| k == "rd_is").map(|(_, v)| v.parse::<f64>().unwrap());
    assert!(trace.fits && set(&trace).is_some_and(|v| close(v, rd_is)), "{:?}", trace.sets);
    assert!(trace.lines.iter().any(|l| l.text.contains("in_situ_rd(")), "the line that worked it ran");
    let initialise = opened.steps.iter().position(|c| c.step == "initialise").unwrap();
    let err = column_trace(s.project(), s.log(), id, &node, initialise, u, None).unwrap_err();
    assert!(err.to_string().contains("initialise is built in, not a formula"), "{err}");

    // Edited, not saved: rd_is doubled and the filter coal only. Its worked examples now fail: the column runs anyway,
    // each failing example said (Tim, 17 November), where saving it is still refused.
    let Body::Dataflow(saved) = &s.project().object(flow).unwrap().body else { panic!("a dataflow") };
    let mut edited = (**saved).clone();
    let densities = edited.steps.iter_mut().find(|st| st.step == "densities").unwrap().formula.as_mut().unwrap();
    densities.script = densities.script.replace("rd_is = in_situ_rd(", "rd_is = 2 * in_situ_rd(");
    densities.when = [("material".to_string(), "coal".to_string())].into_iter().collect();
    let edited = std::sync::Arc::new(edited);
    let opened_edited = table_column(s.project(), s.log(), id, &node, Some(&edited)).unwrap();
    assert!(opened_edited.edited && !opened.edited && opened.failing.is_empty());
    assert!(!opened_edited.failing.is_empty() && opened_edited.failing.iter().all(|f| f.starts_with("the formula step densities, test ")), "{:?}", opened_edited.failing);
    let refused = s.run("dataflow.save", json!({ "name": "Reserves", "content": &*edited })).unwrap_err();
    assert!(refused.to_string().contains("the formula step densities, test "), "saving it is still refused: {refused}");
    let after = |o: &zd_commands::builtin::ColumnSteps| o.steps[at].units.iter().find(|u| u.horizon == "U").unwrap().values["rd_is"];
    assert!(close(after(&opened_edited), 2.0 * rd_is) && close(after(&opened), rd_is));
    assert_eq!(opened_edited.steps[at].fits, [u, unit_id(&opened_edited.steps[at - 1], "L")], "U and L are B1's coal");
    let ob = unit_id(&opened_edited.steps[at - 1], "OB");
    let skipped = column_trace(s.project(), s.log(), id, &node, at, ob, Some(&edited)).unwrap();
    assert!(!skipped.fits && skipped.lines.is_empty(), "{:?}", skipped.lines.len());
    let again = table_column(s.project(), s.log(), id, &node, None).unwrap();
    assert!(close(after(&again), rd_is), "the saved dataflow is as it was");
}

/// A built-in step's reads (decision 0081): `rom_dilution` on the loss column reads each coal unit's dilution settings,
/// A's from the assumptions' row 3 (any coal) and B's from row 2 (coal, B), table 0; `small_volumes` with no
/// `min_volume` column reads none for every unit, no table or row.
#[test]
fn a_notebook_says_which_row_each_built_in_setting_came_from() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let id = created(&record)[0];
    let opened = table_column(s.project(), s.log(), id, &["Pit".to_string(), "1".to_string(), "B1".to_string()], None).unwrap();
    assert_eq!(opened.tables, ["assumptions"]);
    let cell = opened.steps.iter().find(|c| c.step == "rom_dilution").unwrap();
    assert_eq!(cell.kind, StepKind::BuiltIn);
    assert!(cell.rule.starts_with("cuts each waste interval") && cell.source.is_none());
    let coal = |seam: &str| cell.units.iter().find(|u| u.material == "coal" && u.seam == seam && u.became.is_none()).unwrap().id;
    let read = |unit: u32, column: &str| cell.reads.iter().find(|r| r.unit == unit && r.column == column).cloned();
    for (seam, row) in [("A", 2), ("B", 1)] {
        for (column, value) in [("roof_dilution", 0.05), ("floor_dilution", 0.1), ("edge_dilution", 0.02), ("other_dilution", 1.0)] {
            let r = read(coal(seam), column).unwrap_or_else(|| panic!("{seam} reads {column}: {:?}", cell.reads));
            assert_eq!((r.value, r.table, r.row), (Some(value), Some(0), Some(row)), "{seam} {column}");
        }
    }

    let ids = blocks(&mut s, None);
    let flow = dataflow(&mut s, &["small_volumes", "initialise"]);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let opened = table_column(s.project(), s.log(), created(&record)[0], &["Pit".to_string(), "3".to_string(), "B1".to_string()], None).unwrap();
    let small = opened.steps.iter().find(|c| c.step == "small_volumes").unwrap();
    assert_eq!(small.reads.len(), small.units.len(), "one read a unit");
    assert!(small.reads.iter().all(|r| r.column == "min_volume" && r.value.is_none() && r.table.is_none() && r.row.is_none()), "{:?}", small.reads);
    assert!(opened.steps[0].reads.is_empty() && opened.steps[0].kind == StepKind::Input);
}

/// Coal with no in situ moisture can't be put on the in situ basis: its block stops, said with what's missing.
#[test]
fn coal_with_no_in_situ_moisture_stops_its_block() {
    let mut s = session();
    let ids = blocks(&mut s, None);
    let flow = moist_flow(&mut s, "when material,when seam,rd (t/m3),mad (%),mis (%),underburden_thickness (m)\nwaste,,2.3,,,0\ncoal,L,,3,8,\ncoal,U,,3,,\n");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    assert!(record.messages[0].starts_with("0 of 2 blocks ran"), "{}", record.messages[0]);
    let stopped = record.notes.iter().find(|n| n.code == Some(Code::ReservesBlockStopped)).unwrap();
    // The starter's densities formula stops it (decision 0077, 7c), with its line.
    assert!(stopped.text.contains("Pit/3/B1 at densities: line 13: coal with no mis on 3_B1_U"), "{}", stopped.text);
}

// Decision 0073, workstream 3c: working sections, a hand-worked column.

/// One block, 100 m by 50 m (5,000 m²), top down:
///
/// | horizon | m    | m³     | what happens                                                                        |
/// |---------|------|--------|-------------------------------------------------------------------------------------|
/// | OB      | 10   | 50,000 | waste                                                                               |
/// | A       | 2    | 10,000 | starts a section                                                                    |
/// | A-B     | 0.3  | 1,500  | under 1 m: B's 7,500 m³ × 0.5 = 3,750 covers 1,500, so it joins as parting dilution |
/// | B       | 1.5  | 7,500  | section A-B, 3.8 m                                                                  |
/// | B-C     | 5    | 25,000 | over 1 m: a break                                                                   |
/// | C       | 0.4  | 2,000  | alone, 0.4 m ≤ 0.45 m: thin loss                                                    |
/// | C-D     | 0.2  | 1,000  | waste                                                                               |
/// | D       | 3    | 15,000 | ash 40 % over the 30 % cut-off: discarded, a break                                  |
/// | D-E     | 0.25 | 1,250  | waste                                                                               |
/// | E       | 2    | 10,000 | section E                                                                           |
/// | E-F     | 0.9  | 4,500  | under 1 m, but F's 2,500 × 0.5 = 1,250 doesn't cover 4,500: stays waste             |
/// | F       | 0.5  | 2,500  | section F, 0.5 m > 0.45 m                                                           |
/// | below F | 3.95 | 19,750 | real waste below: no underburden                                                    |
fn section_column(s: &mut Session) -> Vec<ObjectId> {
    let rows: [(&str, f64, f64, &str, &str, f64); 13] = [
        ("OB", 40.0, 50.0, "waste", "A", f64::NAN),
        ("A", 38.0, 40.0, "coal", "A", 10.0),
        ("A-B", 37.7, 38.0, "waste", "B", f64::NAN),
        ("B", 36.2, 37.7, "coal", "B", 12.0),
        ("B-C", 31.2, 36.2, "waste", "C", f64::NAN),
        ("C", 30.8, 31.2, "coal", "C", 15.0),
        ("C-D", 30.6, 30.8, "waste", "D", f64::NAN),
        ("D", 27.6, 30.6, "coal", "D", 40.0),
        ("D-E", 27.35, 27.6, "waste", "E", f64::NAN),
        ("E", 25.35, 27.35, "coal", "E", 11.0),
        ("E-F", 24.45, 25.35, "waste", "F", f64::NAN),
        ("F", 23.95, 24.45, "coal", "F", 11.0),
        ("below F", 20.0, 23.95, "waste", "", f64::NAN),
    ];
    rows.iter()
        .map(|&(h, z0, z1, material, seam, ash)| {
            let mut a = json!({ "strip": 1.0, "block": "B1", "horizon": h, "material": material, "seam": seam });
            if material == "coal" {
                a["rd"] = json!(1.4);
                a["ash"] = json!(ash);
            }
            box_solid(s, &format!("1_B1_{h}"), 0.0, (z0, z1), a)
        })
        .collect()
}

const SECTION_STACK: [&str; 13] = ["OB", "A", "A-B", "B", "B-C", "C", "C-D", "D", "D-E", "E", "E-F", "F", "below F"];

const SECTION_CSV: &str = "when material,when seam,rd (t/m3),min_parting (m),ws_strip_ratio,min_coal_thickness (m),insitu_ash_cutoff (%),ws_split\nwaste,,2.3,,,,,\ncoal,,,1,0.5,0.45,30,\n";

fn section_steps() -> serde_json::Value {
    json!([{ "step": "initialise" }, { "step": "join_benches" }, { "step": "working_sections" }])
}

#[test]
fn working_sections_form_by_the_seam_rules_in_spry_order() {
    let mut s = session();
    let ids = section_column(&mut s);
    let flow = flow(&mut s, section_steps(), SECTION_CSV);
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let views = {
        let project = s.project();
        let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
        reserves_preview(&get, &p, "Base", &["Pit".to_string(), "1".to_string(), "B1".to_string()]).unwrap()
    };
    let last = views.last().unwrap();
    let labels: Vec<(String, Option<String>, Option<String>)> = last.units.iter().map(|u| (u.horizon.clone(), u.section.clone(), u.became.clone())).collect();
    let expect = |h: &str, section: Option<&str>, became: Option<&str>| (h.to_string(), section.map(str::to_string), became.map(str::to_string));
    assert_eq!(
        labels,
        [
            expect("OB", Some("1 waste A"), None),
            expect("A", Some("2 coal A-B"), None),
            expect("A-B", Some("2 coal A-B"), Some("dilution (parting)")),
            expect("B", Some("2 coal A-B"), None),
            expect("B-C", Some("3 waste C"), None),
            expect("C", None, Some("thinloss (thin)")),
            // Thin C is set aside, so the waste either side of it is one run, as Spry puts thin loss in its waste.
            expect("C-D", Some("3 waste D"), None),
            expect("D", Some("3 waste D"), Some("discarded (high in situ ash)")),
            expect("D-E", Some("3 waste E"), None),
            expect("E", Some("4 coal E"), None),
            expect("E-F", Some("5 waste F"), None),
            expect("F", Some("6 coal F"), None),
            expect("below F", Some("7 waste"), None),
        ]
    );

    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    // Each seam's leaf: its coal in a working section with the section's number and name, the parting in A-B on B's
    // leaf as working section waste, thin C and discarded D under Loss.
    let seam = |s: &str| leaf(&t, "B1", s, None);
    let ws = |s: &str, f: &str| value(&t, seam(s), &format!("Working section\\Coal\\{f}"));
    let name = |s: &str| texts(&t, "Working section\\name")[seam(s)].clone();
    let id = |s: &str| texts(&t, "Working section\\id")[seam(s)].clone();
    assert!(close(ws("A", "volume").unwrap(), 10_000.0) && close(ws("B", "volume").unwrap(), 7_500.0));
    assert!(close(value(&t, seam("B"), "Working section\\Waste\\volume").unwrap(), 1_500.0), "the parting, on B's leaf");
    assert_eq!((name("A"), id("A"), name("B")), (Some("A-B".into()), Some("2".into()), Some("A-B".into())));
    assert_eq!((name("E"), id("E"), name("F"), id("F")), (Some("E".into()), Some("4".into()), Some("F".into()), Some("6".into())));
    assert!(close(ws("E", "volume").unwrap(), 10_000.0) && close(ws("F", "volume").unwrap(), 2_500.0));
    assert!(close(value(&t, seam("C"), "Loss\\Thin\\volume").unwrap(), 2_000.0) && ws("C", "volume").is_none());
    assert!(close(value(&t, seam("D"), "Loss\\High insitu ash\\volume").unwrap(), 15_000.0) && ws("D", "volume").is_none());
    assert!(record.messages[0].contains("every column balanced to 0.0e0"), "{}", record.messages[0]);
    let said: Vec<String> = record.notes.iter().filter(|n| n.code == Some(Code::ReservesDefaulted)).map(|n| n.text.clone()).collect();
    assert!(said.iter().any(|t| t == "3 units coal formed into working sections, 31500.0 m3"), "{said:?}");
}

/// Forced splits: A named A1 and B named B1 split section A-B into A, with the parting (which goes with the ply above),
/// and B.
#[test]
fn a_forced_split_puts_the_parting_with_the_ply_above() {
    let mut s = session();
    let ids = section_column(&mut s);
    let csv = format!("{SECTION_CSV},A,,,,,,A1\n,B,,,,,,B1\n");
    let flow = flow(&mut s, section_steps(), &csv);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let (seams, names) = (texts(&t, "seam"), texts(&t, "Working section\\name"));
    let at_seam = |s: &str| (0..t.rows()).find(|&i| seams[i].as_deref() == Some(s)).unwrap();
    assert_eq!(names[at_seam("A")].as_deref(), Some("A"));
    // B's leaf holds the parting, which went with A above it, and B's own coal.
    assert_eq!(names[at_seam("B")].as_deref(), Some("A; B"));
    // Each section's volume, from the preview: A with the parting 11,500 m³, B 7,500.
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let views = {
        let project = s.project();
        let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
        reserves_preview(&get, &p, "Base", &["Pit".to_string(), "1".to_string(), "B1".to_string()]).unwrap()
    };
    let in_section = |name: &str| views.last().unwrap().units.iter().filter(|u| u.section.as_deref() == Some(name)).map(|u| u.values["volume"]).sum::<f64>();
    assert!(close(in_section("2 coal A"), 11_500.0) && close(in_section("3 coal B"), 7_500.0));
}

/// A coal ply no row gives a min_coal_thickness can't be judged: its block stops, said.
#[test]
fn a_section_rule_no_row_gives_stops_the_block() {
    let mut s = session();
    let ids = section_column(&mut s);
    let csv = "when material,rd (t/m3),min_parting (m),ws_strip_ratio\nwaste,2.3,,\ncoal,,1,0.5\n";
    let flow = flow(&mut s, section_steps(), csv);
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let stopped = record.notes.iter().find(|n| n.code == Some(Code::ReservesBlockStopped)).unwrap();
    assert!(stopped.text.contains("1_B1_A has no min_coal_thickness"), "{}", stopped.text);
}

// Decision 0074, workstream 4b: ROM loss, a hand-worked column.

/// A block's length on its own (no strip) is the longer side of the smallest rectangle round it in plan: 80 m for an 80 m by 20 m footprint
/// turned 30° at mine coordinates, whatever its corners.
#[test]
fn a_block_s_length_is_the_long_side_of_its_smallest_rectangle() {
    let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let corner = |u: f64, v: f64, z: f64| [E + u * c - v * s, N + u * s + v * c, z];
    let vertices = vec![corner(0.0, 0.0, 0.0), corner(80.0, 0.0, 0.0), corner(80.0, 20.0, 0.0), corner(0.0, 20.0, 0.0), corner(40.0, 10.0, 5.0)];
    let mesh = zd_core::TriMesh::new(vertices, vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4], [0, 2, 1], [0, 3, 2]]);
    let length = zd_commands::builtin::block_length(&mesh);
    assert!((length - 80.0).abs() < 1e-9, "{length}");
}

const LOSS_STACK: [&str; 5] = ["OB", "A", "A-B", "B", "below B"];

/// One block, 100 m by 50 m (5,000 m², 100 m long): OB 10 m, A coal 2 m, A-B 5 m (a break), B coal 3 m, waste below.
fn loss_column(s: &mut Session) -> Vec<ObjectId> {
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
            box_solid(s, &format!("1_B1_{h}"), 0.0, (z0, z1), a)
        })
        .collect::<Vec<_>>();
    // The block's geometry measured onto it, as the dataflow reads it (decision 0074).
    s.run("solid.measure", json!({ "solids": ids })).unwrap();
    ids
}

fn loss_csv(roof: f64, floor: f64) -> String {
    format!(
        "when material,rd (t/m3),min_parting (m),ws_strip_ratio,min_coal_thickness (m),roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%)\nwaste,2.3,,,,,,,\ncoal,,1,0.5,0.5,{roof},{floor},0.05,2\n"
    )
}

fn loss_steps() -> serde_json::Value {
    json!([{ "step": "initialise" }, { "step": "join_benches" }, { "step": "working_sections" }, { "step": "rom_loss" }])
}

/// Roof 0.1 m, floor 0.2 m, edge 0.05 m, other 2 %. A (10,000 m³): roof 500 m³ into OB, edge 0.05 × 100 m × 2 m = 10
/// m³ and other 200 m³ into OB, floor 1,000 m³ into A-B; 8,290 m³ left. B (15,000 m³): roof 500 m³, edge 15 m³ and
/// other 300 m³ into A-B, floor 1,000 m³ into the waste below; 13,185 m³ left.
#[test]
fn rom_loss_takes_roof_floor_edge_and_other_loss_into_the_waste_beside() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, loss_steps(), &loss_csv(0.1, 0.2));
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let views = {
        let project = s.project();
        let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
        reserves_preview(&get, &p, "Base", &["Pit".to_string(), "1".to_string(), "B1".to_string()]).unwrap()
    };
    let last = views.last().unwrap();
    assert_eq!(last.step, "rom_loss");
    let pieces: Vec<(String, Option<String>, Option<String>, f64)> = last.units.iter().map(|u| (u.horizon.clone(), u.became.clone(), u.rom.clone(), u.values["volume"])).collect();
    let want = |h: &str, became: Option<&str>, rom: Option<&str>, v: f64| (h.to_string(), became.map(str::to_string), rom.map(str::to_string), v);
    let expected = [
        want("OB", None, None, 50_000.0),
        want("A", Some("loss (roof)"), Some("1 waste A"), 500.0),
        want("A", Some("loss (edge)"), Some("1 waste A"), 10.0),
        want("A", Some("loss (other)"), Some("1 waste A"), 200.0),
        want("A", None, None, 8_290.0),
        want("A", Some("loss (floor)"), Some("3 waste B"), 1_000.0),
        want("A-B", None, None, 25_000.0),
        want("B", Some("loss (roof)"), Some("3 waste B"), 500.0),
        want("B", Some("loss (edge)"), Some("3 waste B"), 15.0),
        want("B", Some("loss (other)"), Some("3 waste B"), 300.0),
        want("B", None, None, 13_185.0),
        want("B", Some("loss (floor)"), Some("5 waste"), 1_000.0),
        want("below B", None, None, 20_000.0),
    ];
    assert_eq!(pieces.len(), expected.len(), "{pieces:?}");
    for (got, want) in pieces.iter().zip(&expected) {
        assert_eq!((&got.0, &got.1, &got.2), (&want.0, &want.1, &want.2), "{got:?}");
        assert!(close(got.3, want.3), "{got:?} against {want:?}");
    }

    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    assert!(record.messages[0].contains("every column balanced to 0.0e0"), "{}", record.messages[0]);
    let t = table(&s, created(&record)[0]);
    let loss = |seam: &str, kind: &str| value(&t, leaf(&t, "B1", seam, None), &format!("Loss\\{kind}\\volume")).unwrap_or(0.0);
    assert!(close(loss("A", "Roof"), 500.0) && close(loss("A", "Floor"), 1_000.0) && close(loss("A", "Edge"), 10.0) && close(loss("A", "Other"), 200.0));
    assert!(close(loss("B", "Roof"), 500.0) && close(loss("B", "Floor"), 1_000.0) && close(loss("B", "Edge"), 15.0) && close(loss("B", "Other"), 300.0));
    let said = notes_of(&record, Code::ReservesDefaulted);
    assert!(said.iter().any(|n| n == "2 units roof loss, 1000.0 m3") && said.iter().any(|n| n == "2 units other loss, 500.0 m3"), "{said:?}");
}

/// Roof 1 m and floor 1.5 m: A (10,000 m³) can't give 5,000 + 7,500 m³, so it is all thin loss, written under Loss\Thin;
/// B (15,000 m³) gives its roof loss into A-B, the nearest waste above, with A's.
#[test]
fn a_section_short_of_its_loss_is_thin_loss() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, loss_steps(), &loss_csv(1.0, 1.5));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let a = leaf(&t, "B1", "A", None);
    assert!(close(value(&t, a, "Loss\\Thin\\volume").unwrap(), 10_000.0) && value(&t, a, "Loss\\Roof\\volume").is_none());
    let b = leaf(&t, "B1", "B", None);
    assert!(close(value(&t, b, "Loss\\Roof\\volume").unwrap(), 5_000.0) && close(value(&t, b, "Loss\\Floor\\volume").unwrap(), 7_500.0));
    let said = notes_of(&record, Code::ReservesDefaulted);
    assert!(said.iter().any(|n| n == "1 unit coal in a working section short of its roof and floor loss, thin loss, 10000.0 m3"), "{said:?}");
}

/// Two coal sections touching (A and B split by name, the parting with A): nothing is lost across their boundary
/// unless `loss_between_coal` says so, and then A's floor loss goes into B.
#[test]
fn no_loss_across_a_coal_boundary_unless_asked() {
    let mut s = session();
    let ids = section_column(&mut s);
    let csv = "when material,when seam,rd (t/m3),min_parting (m),ws_strip_ratio,min_coal_thickness (m),insitu_ash_cutoff (%),ws_split,roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%),loss_between_coal\nwaste,,2.3,,,,,,,,,,\ncoal,A,,1,0.5,0.45,30,A1,0.1,0.1,0,0,\ncoal,B,,1,0.5,0.45,30,B1,0.1,0.1,0,0,\ncoal,,,1,0.5,0.45,30,,0.1,0.1,0,0,\n";
    let steps = json!([{ "step": "initialise" }, { "step": "join_benches" }, { "step": "working_sections" }, { "step": "rom_loss" }]);
    let flow_off = flow(&mut s, steps.clone(), csv);
    let record = s.run("reserves.run", json!({ "dataflow": flow_off, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let at_seam = |t: &Table, s: &str| (0..t.rows()).find(|&i| texts(t, "seam")[i].as_deref() == Some(s)).unwrap();
    assert!(value(&t, at_seam(&t, "A"), "Loss\\Floor\\volume").is_none(), "A's floor is B, coal");
    assert!(value(&t, at_seam(&t, "B"), "Loss\\Roof\\volume").is_none(), "B's roof is A, coal");
    assert!(close(value(&t, at_seam(&t, "A"), "Loss\\Roof\\volume").unwrap(), 500.0), "A's roof is OB");

    // Asked on A's row: A's floor loss, 0.1 m over 5,000 m² = 500 m³ off the bottom of section A (its parting), goes into
    // section B, coal; B's own row doesn't ask, so B's roof still takes nothing from A.
    let yes = csv.replace("0.1,0.1,0,0,\ncoal,B", "0.1,0.1,0,0,yes\ncoal,B");
    let flow_on = flow(&mut s, steps, &yes);
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow_on, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let views = {
        let project = s.project();
        let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
        reserves_preview(&get, &p, "Base", &["Pit".to_string(), "1".to_string(), "B1".to_string()]).unwrap()
    };
    let into_b: Vec<(String, f64)> = views.last().unwrap().units.iter().filter(|u| u.rom.as_deref() == Some("3 coal B")).map(|u| (u.became.clone().unwrap_or_default(), u.values["volume"])).collect();
    assert_eq!(into_b.len(), 1, "{into_b:?}");
    assert_eq!(into_b[0].0, "loss (floor)");
    assert!(close(into_b[0].1, 500.0));
    assert!(views.last().unwrap().units.iter().all(|u| u.rom.as_deref() != Some("2 coal A")), "nothing from B into A");
}

// Decision 0074, workstream 4c: ROM dilution and the ROM ash cut-off, on the loss column.

/// The loss column's parameters with dilution and a ROM ash cut-off: losses `l` (roof, floor, edge, other), dilution `d`
/// (roof, floor, edge m, other %), waste ash 80 by default, and B's cut-off.
fn dilution_csv(l: [f64; 4], d: [f64; 4], b_cutoff: &str) -> String {
    format!(
        "when material,when seam,rd (t/m3),ash_default (%),min_parting (m),ws_strip_ratio,min_coal_thickness (m),roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%),roof_dilution (m),floor_dilution (m),edge_dilution (m),other_dilution (%),rom_ash_cutoff (%)\nwaste,,2.3,80,,,,,,,,,,,,\ncoal,B,,,1,0.5,0.5,{},{},{},{},{},{},{},{},{b_cutoff}\ncoal,,,,1,0.5,0.5,{},{},{},{},{},{},{},{},\n",
        l[0], l[1], l[2], l[3], d[0], d[1], d[2], d[3], l[0], l[1], l[2], l[3], d[0], d[1], d[2], d[3]
    )
}

fn dilution_steps() -> serde_json::Value {
    json!([{ "step": "initialise" }, { "step": "join_benches" }, { "step": "working_sections" }, { "step": "rom_loss" }, { "step": "rom_dilution" }, { "step": "rom_ash_cutoff" }])
}

fn preview_of(s: &Session, flow: ObjectId, ids: &[ObjectId], stack: &[&str]) -> Vec<zd_commands::builtin::StepView> {
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": stack, "layer": "reserves" })).unwrap();
    let project = s.project();
    let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
    reserves_preview(&get, &p, "Base", &["Pit".to_string(), "1".to_string(), "B1".to_string()]).unwrap()
}

/// A ROM section's volume from the preview: what was sent to it, and its own units not sent away or set aside.
fn rom_volume(view: &zd_commands::builtin::StepView, section: &str) -> f64 {
    view.units
        .iter()
        .filter(|u| match &u.rom {
            Some(r) => r == section,
            None => u.section.as_deref() == Some(section) && u.became.as_deref().is_none_or(|b| b.starts_with("dilution (parting)")),
        })
        .map(|u| u.values["volume"])
        .sum()
}

/// No loss; dilution roof 0.05 m, floor 0.1 m, edge 0.02 m, other 1 %. OB gives A its roof dilution 250 m³, edge
/// 0.02 × 100 m × 2 m = 4 m³ and other 100 m³; A-B gives A its floor dilution 500 m³ off its top and B its roof 250 m³,
/// edge 6 m³ and other 150 m³ off its bottom; the waste below gives B its floor 500 m³. ROM A 10,854 m³, ROM B 15,906 m³.
#[test]
fn rom_dilution_cuts_each_waste_interval_for_the_coal_either_side() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], ""));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let last = views.last().unwrap();
    assert!(close(rom_volume(last, "2 coal A"), 10_854.0), "{}", rom_volume(last, "2 coal A"));
    assert!(close(rom_volume(last, "4 coal B"), 15_906.0), "{}", rom_volume(last, "4 coal B"));
    let pieces: Vec<(String, String, f64)> =
        last.units.iter().filter(|u| u.became.as_deref().is_some_and(|b| b.starts_with("dilution"))).map(|u| (u.horizon.clone(), u.became.clone().unwrap(), u.values["volume"])).collect();
    let want = [
        ("OB", "dilution (roof)", 250.0),
        ("OB", "dilution (edge)", 4.0),
        ("OB", "dilution (other)", 100.0),
        ("A-B", "dilution (floor)", 500.0),
        ("A-B", "dilution (roof)", 250.0),
        ("A-B", "dilution (edge)", 6.0),
        ("A-B", "dilution (other)", 150.0),
        ("below B", "dilution (floor)", 500.0),
    ];
    let mut got = pieces.clone();
    got.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut want: Vec<(String, String, f64)> = want.iter().map(|(h, b, v)| (h.to_string(), b.to_string(), *v)).collect();
    want.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    assert_eq!(got.len(), want.len(), "{pieces:?}");
    for (g, w) in got.iter().zip(&want) {
        assert_eq!((&g.0, &g.1), (&w.0, &w.1));
        assert!(close(g.2, w.2), "{g:?} against {w:?}");
    }

    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    assert!(record.messages[0].contains("every column balanced to 0.0e0"), "{}", record.messages[0]);
    let t = table(&s, created(&record)[0]);
    // Each dilution on the leaf of the waste it was taken from: OB on A's, A-B on B's.
    let d = |seam: &str, kind: &str| value(&t, leaf(&t, "B1", seam, None), &format!("Dilution\\{kind}\\Waste\\volume")).or_else(|| value(&t, leaf(&t, "B1", seam, None), &format!("Dilution\\{kind}\\volume"))).unwrap_or(0.0);
    assert!(close(d("A", "Roof"), 250.0) && close(d("A", "Edge"), 4.0) && close(d("A", "Other"), 100.0));
    assert!(close(d("B", "Floor"), 500.0) && close(d("B", "Roof"), 250.0) && close(d("B", "Edge"), 6.0) && close(d("B", "Other"), 150.0));
}

/// A-B (25,000 m³) asked for A's floor dilution 3 m (15,000 m³) and B's roof 2.5 m (12,500 m³): 27,500 m³. Each is
/// shared out by 25,000 ÷ 27,500, and the losses sent into A-B (A's floor loss and B's roof loss, 500 m³ each) go back to
/// their coal; said.
#[test]
fn a_waste_too_thin_for_its_dilution_shares_it_out_and_returns_its_losses() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.1, 0.1, 0.0, 0.0], [2.5, 3.0, 0.0, 0.0], ""));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let last = views.last().unwrap();
    let into = |section: &str, kind: &str| last.units.iter().filter(|u| u.rom.as_deref() == Some(section) && u.became.as_deref() == Some(kind)).map(|u| u.values["volume"]).sum::<f64>();
    assert!(close(into("2 coal A", "dilution (floor)"), 15_000.0 * 25_000.0 / 27_500.0));
    assert!(close(into("4 coal B", "dilution (roof)"), 12_500.0 * 25_000.0 / 27_500.0));
    assert!(close(into("3 waste B", "loss (floor)") + into("3 waste B", "loss (roof)"), 0.0), "the losses sent into A-B are back");
    assert!(close(into("1 waste A", "loss (roof)"), 500.0), "A's roof loss into OB stays");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    assert_eq!(notes_of(&record, Code::ReservesOutOfRange), ["1 short of their dilution (dilution shared out, losses sent there returned): 1_B1_A-B (25000.0 of 27500.0 m3 asked)"]);
    assert!(notes_of(&record, Code::ReservesDefaulted).iter().any(|n| n == "1 unit loss sent into waste short of its dilution, returned to its coal, 1000.0 m3"), "{:?}", notes_of(&record, Code::ReservesDefaulted));
}

/// B's ROM ash with its dilution (906 m³ of waste at RD 2.3 and 80 % ash with 15,000 m³ of coal at 1.4 and 10 %) is
/// 376,704 ÷ 23,083.8 = 16.32 %, over its 15 % cut-off: B is rejected, its coal high ROM ash loss and its dilution back
/// to waste. A, with no cut-off, keeps its ROM.
#[test]
fn the_rom_ash_cut_off_rejects_a_section_once() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], "15"));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let last = views.last().unwrap();
    assert!(close(rom_volume(last, "4 coal B"), 0.0), "B rejected");
    assert!(last.units.iter().all(|u| u.rom.as_deref() != Some("4 coal B")), "its dilution back to waste");
    assert!(close(rom_volume(last, "2 coal A"), 10_854.0), "A as it was");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    assert_eq!(notes_of(&record, Code::ReservesOutOfRange), ["1 ROM section over its ash cut-off, rejected: 1_B1_B section B (16.32 over 15.00)"]);
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "B", None), "Loss\\High ROM ash\\volume").unwrap(), 15_000.0));
}

// Decision 0082, workstream 1: a built-in step's workings, the arithmetic the notebook shows, line by line.

/// A unit's id in a cell by its horizon and material, the first not yet set aside or cut.
fn unit_in(cell: &zd_commands::builtin::StepView, horizon: &str) -> u32 {
    cell.units.iter().find(|u| u.horizon == horizon && u.became.is_none()).unwrap_or_else(|| panic!("no {horizon} in {}", cell.step)).id
}

/// The workings a step wrote for a unit, by what was worked out.
fn worked<'a>(cell: &'a zd_commands::builtin::StepView, unit: u32, what: &str) -> Vec<&'a zd_commands::builtin::Working> {
    cell.workings.iter().filter(|w| w.unit == unit && w.what == what).collect()
}

/// `rom_dilution` in the notebook on the dilution column (roof 0.05 m, floor 0.1 m, edge 0.02 m, other 1 %): A-B's
/// workings give A its floor dilution 0.1 × 5,000 m² = 500 m³ off its top and B its roof 0.05 × 5,000 = 250 m³, edge
/// 0.02 × 100 m × 3 m = 6 m³ and other 1 % of 15,000 m³ = 150 m³, each the volume of the piece it cut; B's coal carries
/// the lines for its own dilution. Each value against the piece: to 1e-9 relative.
#[test]
fn rom_dilution_writes_its_arithmetic_for_the_notebook() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let opened = table_column(s.project(), s.log(), created(&record)[0], &["Pit".to_string(), "1".to_string(), "B1".to_string()], None).unwrap();
    let at = opened.steps.iter().position(|c| c.step == "rom_dilution").unwrap();
    let (before, cell) = (&opened.steps[at - 1], &opened.steps[at]);
    let (parting, b) = (unit_in(before, "A-B"), unit_in(before, "B"));
    let piece = |kind: &str| cell.units.iter().find(|u| u.horizon == "A-B" && u.became.as_deref() == Some(kind)).unwrap().values["volume"];
    let one = |unit: u32, what: &str| {
        let lines = worked(cell, unit, what);
        assert_eq!(lines.len(), 1, "{what} on {unit}: {:?}", cell.workings);
        lines[0].clone()
    };
    let floor = one(parting, "floor dilution");
    assert_eq!(floor.sum, "floor_dilution 0.1 m × area 5,000 m²");
    assert_eq!(floor.then.as_deref(), Some("off the top of section 3 waste B, into section 2 coal A"));
    let roof = one(parting, "roof dilution");
    assert_eq!((roof.sum.as_str(), roof.measure), ("roof_dilution 0.05 m × area 5,000 m²", "m³"));
    assert_eq!(roof.then.as_deref(), Some("off the bottom of section 3 waste B, into section 4 coal B"));
    let edge = one(parting, "edge dilution");
    assert_eq!(edge.sum, "edge_dilution 0.02 m × length 100 m × thickness 3 m");
    let other = one(parting, "other dilution");
    assert_eq!(other.sum, "other_dilution 1 % × coal 15,000 m³");
    for (line, kind, want) in [(&floor, "dilution (floor)", 500.0), (&roof, "dilution (roof)", 250.0), (&edge, "dilution (edge)", 6.0), (&other, "dilution (other)", 150.0)] {
        let value = line.value.unwrap();
        assert!(close(value, want) && close(value, piece(kind)), "{kind}: {value} against {want} and the piece {}", piece(kind));
    }
    // B's coal has its own dilution's lines: its roof from A-B, its floor from the waste below, not A's floor dilution.
    assert_eq!(one(b, "roof dilution"), zd_commands::builtin::Working { unit: b, ..roof.clone() });
    assert_eq!(one(b, "floor dilution").then.as_deref(), Some("off the top of section 5 waste, into section 4 coal B"));
    // A formula step and the column as input write none.
    assert!(opened.steps.iter().filter(|c| c.kind != StepKind::BuiltIn).all(|c| c.workings.is_empty()));
}

/// A-B (25,000 m³) short of the 27,500 m³ asked: its workings say so, and each dilution's share, A's floor 15,000 ×
/// 25,000 ÷ 27,500 = 13,636.4 m³.
#[test]
fn a_waste_short_of_its_dilution_says_its_shares() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.1, 0.1, 0.0, 0.0], [2.5, 3.0, 0.0, 0.0], ""));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let at = views.iter().position(|c| c.step == "rom_dilution").unwrap();
    let parting = unit_in(&views[at - 1], "A-B");
    let holds = worked(&views[at], parting, "waste holds");
    assert_eq!(holds.len(), 1);
    assert_eq!(holds[0].sum, "25,000 m³ < dilution asked 27,500 m³");
    let floor = worked(&views[at], parting, "floor dilution")[0];
    assert!(close(floor.value.unwrap(), 15_000.0), "the floor dilution asked");
    assert_eq!(floor.then.as_deref(), Some("13,636.4 m³ of it, shared out, off the top of section 3 waste B, into section 2 coal A"));
}

/// `working_sections` on the hand-worked section column: each test the step made, with its numbers and what followed.
#[test]
fn working_sections_write_each_test_with_its_numbers() {
    let mut s = session();
    let ids = section_column(&mut s);
    let flow = flow(&mut s, section_steps(), SECTION_CSV);
    let views = preview_of(&s, flow, &ids, &SECTION_STACK);
    let at = views.iter().position(|c| c.step == "working_sections").unwrap();
    let (before, cell) = (&views[at - 1], &views[at]);
    let line = |horizon: &str, what: &str| {
        let lines = worked(cell, unit_in(before, horizon), what);
        assert_eq!(lines.len(), 1, "{what} on {horizon}: {:?}", cell.workings);
        (lines[0].sum.clone(), lines[0].then.clone().unwrap_or_default(), lines[0].value)
    };
    // A-B: 0.3 m, under B's and A's 1 m, then B's 7,500 m³ × 0.5 = 3,750 covers its 1,500 m³.
    let (sum, then, _) = line("A-B", "parting");
    assert_eq!((sum.as_str(), then.as_str()), ("thickness 0.3 m ≤ min_parting 1 m (the larger of the plies' either side)", "may join the plies either side, by the strip ratio"));
    let (sum, then, value) = line("B", "strip ratio");
    assert_eq!(sum, "B coal 7,500 m³ × ws_strip_ratio 0.5 = 3,750 m³ ≥ parting 1,500 m³");
    assert_eq!((then.as_str(), value), ("B coal joins the section, with the parting", Some(3_750.0)));
    assert_eq!(line("A-B", "strip ratio").0, sum, "the parting carries the same line");
    // B-C: 5 m, a break. E-F: F's 1,250 doesn't cover 4,500.
    assert_eq!(line("B-C", "parting").1, "a break: the plies either side start new groups");
    let (sum, then, _) = line("E-F", "strip ratio");
    assert_eq!(sum, "F coal 2,500 m³ × ws_strip_ratio 0.5 = 1,250 m³ < parting 4,500 m³");
    assert_eq!(then, "the section closes and the next starts at F coal; the parting stays waste");
    // Sections by thickness: A-B 3.8 m kept, C 0.4 m thin.
    assert_eq!(line("A", "section thickness"), ("3.8 m, partings in, > min_coal_thickness 0.45 m".into(), "a working section".into(), None));
    assert_eq!(line("C", "section thickness"), ("0.4 m, partings in, ≤ min_coal_thickness 0.45 m".into(), "thin loss: its coal set aside, its partings waste".into(), None));
    // D discarded for its in situ ash, and the waste round it a break.
    assert!(line("D", "discarded").1.starts_with("high in situ ash"));
    assert_eq!(line("D-E", "parting").0, "holds discarded coal");
    // The parting in section A-B is its dilution.
    assert_eq!(line("A-B", "working section"), ("number 2, coal, A-B".into(), "a parting in a coal section: parting dilution".into(), None));
}

/// `rom_loss` on the loss column (roof 0.1 m, floor 0.2 m, edge 0.05 m, other 2 %): A's coal says its roof loss 0.1 ×
/// 5,000 m² = 500 m³ into OB, floor 1,000 m³ into A-B, edge 0.05 × 100 m × 2 m = 10 m³ and other 2 % of 10,000 m³ = 200
/// m³ off its top into OB.
#[test]
fn rom_loss_writes_its_arithmetic_for_the_notebook() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, loss_steps(), &loss_csv(0.1, 0.2));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let at = views.iter().position(|c| c.step == "rom_loss").unwrap();
    let a = unit_in(&views[at - 1], "A");
    let cell = &views[at];
    let one = |what: &str| {
        let lines = worked(cell, a, what);
        assert_eq!(lines.len(), 1, "{what}: {:?}", cell.workings);
        (lines[0].sum.clone(), lines[0].value, lines[0].then.clone().unwrap_or_default())
    };
    assert_eq!(one("roof loss"), ("roof_loss 0.1 m × area 5,000 m²".into(), Some(500.0), "off the top of section 2 coal A, into section 1 waste A".into()));
    let (sum, value, then) = one("floor loss");
    assert_eq!((sum.as_str(), then.as_str()), ("floor_loss 0.2 m × area 5,000 m²", "off the bottom of section 2 coal A, into section 3 waste B"));
    assert!(close(value.unwrap(), 1_000.0));
    let (sum, value, then) = one("edge loss");
    assert_eq!((sum.as_str(), then.as_str()), ("edge_loss 0.05 m × length 100 m × thickness 2 m", "off its top, into section 1 waste A"));
    assert!(close(value.unwrap(), 10.0));
    let (sum, value, _) = one("other loss");
    assert_eq!(sum, "other_loss 2 % × volume 10,000 m³");
    assert!(close(value.unwrap(), 200.0));
    assert_eq!(one("section holds").2, "it holds them");
}

/// `rom_ash_cutoff`: B's ROM ash 16.32 % over its 15 % cut-off, rejected, said on its coal; A, with no cut-off, isn't
/// tested.
#[test]
fn the_rom_ash_cut_off_writes_its_test() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, dilution_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], "15"));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let at = views.iter().position(|c| c.step == "rom_ash_cutoff").unwrap();
    let cell = &views[at];
    let b = worked(cell, unit_in(&views[at - 1], "B"), "ROM ash");
    assert_eq!(b.len(), 1, "{:?}", cell.workings);
    assert_eq!(b[0].sum, "ash 16.32 %, over its coal, partings and dilution, > rom_ash_cutoff 15 %");
    assert_eq!(b[0].then.as_deref(), Some("section 4 coal B rejected: its coal high ROM ash loss, its dilution back to its waste"));
    let a = worked(cell, unit_in(&views[at - 1], "A"), "ROM ash");
    assert_eq!((a[0].sum.as_str(), a[0].then.as_deref()), ("no rom_ash_cutoff for its coal", Some("not tested")));
}

/// A number as a working shows it: thousands marked, no trailing zeros.
#[test]
fn workings_mark_thousands_and_drop_trailing_zeros() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, loss_steps(), &loss_csv(0.1, 0.2));
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    // Every figure in every line reads plainly: no long decimals from float sums, thousands marked.
    for w in views.iter().flat_map(|c| &c.workings) {
        assert!(!w.sum.contains("000000") && !w.sum.contains("99999"), "{}", w.sum);
        assert!(!w.sum.contains(".0 ") && !w.sum.contains("10000"), "{}", w.sum);
    }
}

// Decision 0074: block geometry measured onto the blocks; the dataflow reads it.

/// A 100 m by 50 m block from RL 38 to 40 at mine coordinates: 10,000 m³ over 5,000 m², 2 m thick, 100 m long and 50 m
/// wide, its centroid in its middle.
#[test]
fn solid_measure_writes_each_block_s_geometry_onto_it() {
    let mut s = session();
    let id = box_solid(&mut s, "block", 0.0, (38.0, 40.0), json!({ "strip": 1.0 }));
    let record = s.run("solid.measure", json!({ "solids": [id] })).unwrap();
    assert!(record.messages[0].starts_with("measured 1 block: 10000.0 m3 over 5000.0 m2 in plan"), "{}", record.messages[0]);
    let a = &s.project().object(id).unwrap().attributes;
    let n = |k: &str| match a.get(k) {
        Some(zd_core::AttrValue::Number(v)) => *v,
        other => panic!("{k}: {other:?}"),
    };
    assert!(close(n("volume"), 10_000.0) && close(n("plan_area"), 5_000.0) && close(n("thickness"), 2.0));
    assert!(close(n("length"), 100.0) && close(n("width"), 50.0));
    assert!(close(n("centroid_x"), E + 50.0) && close(n("centroid_y"), N + 25.0) && close(n("centroid_z"), 39.0));
    assert!(close(n("dip"), 0.0) && close(n("true_thickness"), 2.0), "flat: true thickness is the vertical");
    assert!(a.get("strip").is_some(), "the block keeps what it had");
}

/// A block 100 m by 50 m in plan, 2 m thick vertically, its floor and roof dipping 10° to the east: 10,000 m³ over
/// 5,000 m², dip 10°, true thickness 2 × cos 10° = 1.96962 m, the vertical kept beside it.
#[test]
fn solid_measure_gives_dip_and_true_thickness() {
    let mut s = session();
    let slope = 10f64.to_radians().tan();
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| {
            let x = if i & 1 == 1 { 100.0 } else { 0.0 };
            let y = if i >> 1 & 1 == 1 { 50.0 } else { 0.0 };
            let floor = 30.0 - x * slope;
            [E + x, N + y, if i >> 2 & 1 == 1 { floor + 2.0 } else { floor }]
        })
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let id = created(&s.run("solid.from_mesh", json!({ "name": "dipping", "layer": "blocks", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    s.run("solid.measure", json!({ "solids": [id] })).unwrap();
    let a = &s.project().object(id).unwrap().attributes;
    let n = |k: &str| match a.get(k) {
        Some(zd_core::AttrValue::Number(v)) => *v,
        other => panic!("{k}: {other:?}"),
    };
    assert!(close(n("volume"), 10_000.0) && close(n("plan_area"), 5_000.0) && close(n("thickness"), 2.0));
    assert!(close(n("dip"), 10.0), "{}", n("dip"));
    assert!(close(n("true_thickness"), 2.0 * 10f64.to_radians().cos()) && (n("true_thickness") - 1.96962).abs() < 1e-5);
}

/// A bench between two levels, flat top and bottom, takes its seam's dip from the block's other benches of that seam (the
/// same `benched_from` and `block`): beside the 10° bench above, the flat 2 m box measures 10° and 2 × cos 10° true. On
/// its own, with only level faces, it dips 0.
#[test]
fn a_bench_between_two_levels_dips_as_its_seam() {
    let mut s = session();
    let slope = 10f64.to_radians().tan();
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| {
            let x = if i & 1 == 1 { 100.0 } else { 0.0 };
            let y = if i >> 1 & 1 == 1 { 50.0 } else { 0.0 };
            let floor = 30.0 - x * slope;
            [E + x, N + y, if i >> 2 & 1 == 1 { floor + 2.0 } else { floor }]
        })
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let dipping = created(&s.run("solid.from_mesh", json!({ "name": "bench 30", "layer": "blocks", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    s.run("test.tag", json!({ "object": dipping, "attributes": { "benched_from": "seam U", "block": "B1" } })).unwrap();
    let flat = box_solid(&mut s, "bench 20", 0.0, (20.0, 22.0), json!({ "benched_from": "seam U", "block": "B1" }));
    let alone = box_solid(&mut s, "alone", 200.0, (20.0, 22.0), json!({ "block": "B2" }));
    s.run("solid.measure", json!({ "solids": [dipping, flat, alone] })).unwrap();
    let n = |id: ObjectId, k: &str| match s.project().object(id).unwrap().attributes.get(k) {
        Some(zd_core::AttrValue::Number(v)) => *v,
        other => panic!("{k}: {other:?}"),
    };
    assert!(close(n(flat, "dip"), 10.0) && close(n(flat, "true_thickness"), 2.0 * 10f64.to_radians().cos()), "{}", n(flat, "dip"));
    assert!(close(n(dipping, "dip"), 10.0));
    assert!(close(n(alone, "dip"), 0.0) && close(n(alone, "true_thickness"), 2.0));
}

/// The loss column with edge loss but no measure: edge loss has no length to run along, so the block stops, said.
#[test]
fn edge_loss_on_an_unmeasured_block_stops_it() {
    let mut s = session();
    let rows: [(&str, f64, f64, &str, &str); 5] =
        [("OB", 40.0, 50.0, "waste", "A"), ("A", 38.0, 40.0, "coal", "A"), ("A-B", 33.0, 38.0, "waste", "B"), ("B", 30.0, 33.0, "coal", "B"), ("below B", 26.0, 30.0, "waste", "")];
    let ids: Vec<ObjectId> = rows
        .iter()
        .map(|&(h, z0, z1, material, seam)| {
            let mut a = json!({ "strip": 1.0, "block": "B1", "horizon": h, "material": material, "seam": seam });
            if material == "coal" {
                a["rd"] = json!(1.4);
            }
            box_solid(&mut s, &format!("1_B1_{h}"), 0.0, (z0, z1), a)
        })
        .collect();
    let flow = flow(&mut s, loss_steps(), &loss_csv(0.1, 0.2));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let stopped = record.notes.iter().find(|n| n.code == Some(Code::ReservesBlockStopped)).unwrap();
    assert!(stopped.text.contains("1_B1_A has no length: measure the blocks (solid.measure) or give one in a parameter row"), "{}", stopped.text);
}

/// A length that looks wrong, from an odd solid, corrected in the parameters: a minimum equal to the maximum forces the
/// default, so A's edge loss runs along 120 m, not its measured 100: 0.05 × 120 × 2 = 12 m³.
#[test]
fn a_parameter_row_corrects_a_measured_length() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let csv = loss_csv(0.1, 0.2).replace("other_loss (%)\n", "other_loss (%),length_min (m),length_max (m),length_default (m)\n").replace(",2\n", ",2,120,120,120\n").replace("waste,2.3,,,,,,,\n", "waste,2.3,,,,,,,,,,\n");
    let flow = flow(&mut s, loss_steps(), &csv);
    let views = preview_of(&s, flow, &ids, &LOSS_STACK);
    let edge: f64 = views.last().unwrap().units.iter().filter(|u| u.horizon == "A" && u.became.as_deref() == Some("loss (edge)")).map(|u| u.values["volume"]).sum();
    assert!(close(edge, 12.0), "{edge}");
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let said = notes_of(&record, Code::ReservesFormulaSaid);
    assert!(said.iter().any(|n| n == "2 units printed by limits: length: coal forced to its default, 25000.0 m3"), "{said:?}");
}

/// A block whose volume attribute isn't its solid's (measured, then cut again) is named; the run reads what it carries.
#[test]
fn a_block_whose_measured_volume_isn_t_its_solid_s_is_named() {
    let mut s = session();
    let ids = loss_column(&mut s);
    s.run("test.tag", json!({ "object": ids[1], "attributes": { "volume": 9_000.0 } })).unwrap();
    let table_id = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": "when material,rd (t/m3),underburden_thickness (m)\nwaste,2.3,\n,,0\n" })).unwrap())[0];
    let content = json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "attribute", "attribute": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "attribute", "attribute": "thickness" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
        ],
        "steps": [{ "step": "initialise" }, { "step": "in_situ" }],
        "parameters": [table_id],
    });
    let flow = created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": starter(content) })).unwrap())[0];
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    assert_eq!(notes_of(&record, Code::ReservesOutOfRange), ["1 block whose volume isn't its solid's (measured before it was cut again?): 1_B1_A"]);
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "A", None), "Insitu\\Coal\\volume").unwrap(), 9_000.0), "the data table's volume");
}

// Decision 0074, workstream 4d: ROM calculations, final, Scheduled, clean coal and out-of-model volume.

fn final_steps() -> serde_json::Value {
    json!([
        { "step": "initialise" }, { "step": "join_benches" }, { "step": "working_sections" }, { "step": "rom_loss" },
        { "step": "rom_dilution" }, { "step": "rom_ash_cutoff" }, { "step": "rom" }, { "step": "final" }
    ])
}

/// The reserves table's value on a seam's leaf of the loss column (B1, unbenched): a folder path, else 0.
fn at_seam(t: &Table, seam: Option<&str>, path: &str) -> f64 {
    let seams = texts(t, "seam");
    (0..t.rows()).find(|&i| seams[i].as_deref() == seam).and_then(|i| value(t, i, path)).unwrap_or(0.0)
}

/// Dilution only, as the 4c column: ROM coal A 10,854 m³ on A's leaf (its coal, OB's 250 + 4 + 100 m³ and A-B's 500 m³),
/// B 15,906 m³ on B's; ROM waste what's left of each waste on its own leaf, OB 49,646 m³ on A's, A-B 24,094 m³ on B's. The
/// waste below B, the last coal, isn't mined (decision 0077, point 7, Spry's way): B's 500 m³ of floor dilution comes from
/// it and the 19,500 m³ left is underburden, netted off `Summary\Out of model`. ROM coal and waste are the blocks'
/// 120,000 m³ less it. Scheduled the same; clean coal the
/// coal alone, 10,000 and 15,000 m³. ROM tonnes on the density's basis here, no ROM density in this dataflow: A's
/// 10,000 × 1.4 + 854 × 2.3 = 15,964.2 t.
#[test]
fn final_writes_rom_coal_and_waste_where_they_end_and_scheduled_beside_them() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, final_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    assert!(record.messages[0].contains("every column balanced to 0.0e0"), "{}", record.messages[0]);
    let t = table(&s, created(&record)[0]);
    let rom = |seam: Option<&str>, m: &str| at_seam(&t, seam, &format!("ROM\\{m}\\volume"));
    assert!(close(rom(Some("A"), "Coal"), 10_854.0) && close(rom(Some("B"), "Coal"), 15_906.0), "{} {}", rom(Some("A"), "Coal"), rom(Some("B"), "Coal"));
    assert!(close(rom(Some("A"), "Waste"), 49_646.0) && close(rom(Some("B"), "Waste"), 24_094.0) && close(rom(None, "Waste"), 0.0));
    assert!(close(total(&t, "Summary\\Out of model\\volume"), -19_500.0), "the waste below the last coal, netted");
    assert!(close(total(&t, "ROM\\Coal\\volume") + total(&t, "ROM\\Waste\\volume"), 120_000.0 - 19_500.0), "ROM is the blocks but the waste below the last coal");
    assert_eq!(notes_of(&record, Code::ReservesUnderburden).iter().filter(|n| n.contains("below the last coal")).count(), 1, "said as underburden");
    assert!(close(total(&t, "Scheduled\\Coal\\volume"), total(&t, "ROM\\Coal\\volume")) && close(total(&t, "Scheduled\\Waste\\volume"), total(&t, "ROM\\Waste\\volume")));
    assert!(close(at_seam(&t, Some("A"), "Clean coal\\Coal\\volume"), 10_000.0) && close(at_seam(&t, Some("B"), "Clean coal\\Coal\\volume"), 15_000.0));
    assert!(close(at_seam(&t, Some("A"), "ROM\\Coal\\tonnes"), 10_000.0 * 1.4 + 854.0 * 2.3));
    assert_eq!(texts(&t, "ROM\\section")[(0..t.rows()).find(|&i| texts(&t, "seam")[i].as_deref() == Some("B")).unwrap()].as_deref(), Some("B"));
}

/// What each step wrote (decision 0084), as the Dataflow tab shows under each step: the 4c column with dilution only, run
/// to `final`. The column as input writes Imported, every block, 120,000 m³; `final` writes ROM coal A and B, 10,854 +
/// 15,906 m³. Every folder's writes, added up over the steps, are the reserves table's total for it, and every folder the
/// table holds a volume for was written by a step: nothing written that the table hasn't, nothing in it from nowhere.
#[test]
fn each_step_s_writes_add_up_to_the_reserves_table_folder_by_folder() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, final_steps(), &dilution_csv([0.0; 4], [0.05, 0.1, 0.02, 1.0], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let id = created(&record)[0];
    let t = table(&s, id);
    let opened = table_column(s.project(), s.log(), id, &["Pit".to_string(), "1".to_string(), "B1".to_string()], None).unwrap();

    // The column as input: Imported, coal (A, B) and waste (OB, A-B, below B), each block as read.
    let input = &opened.steps[0].writes;
    let folders: Vec<(&str, usize)> = input.iter().map(|w| (w.folder.as_str(), w.units.len())).collect();
    assert_eq!(folders.len(), 2, "{folders:?}");
    assert!(folders.contains(&("Imported\\Waste", 3)) && folders.contains(&("Imported\\Coal", 2)), "{folders:?}");
    assert!(close(input.iter().map(|w| w.volume).sum(), 120_000.0));

    // rom_dilution, as the 4c column: roof 250 m³ to A off OB and 250 m³ to B off A-B; floor 500 m³ to A off A-B and
    // 500 m³ to B off the waste below; edge 4 + 6 m³; other 100 + 150 m³. Each folder two units, the coal it went to.
    let dilution = opened.steps.iter().find(|c| c.step == "rom_dilution").unwrap();
    for (folder, v) in [("Dilution\\Roof", 500.0), ("Dilution\\Floor", 1_000.0), ("Dilution\\Edge", 10.0), ("Dilution\\Other", 250.0)] {
        let w = dilution.writes.iter().find(|w| w.folder == folder).unwrap_or_else(|| panic!("{folder}: {:?}", dilution.writes));
        assert!(close(w.volume, v) && w.units.len() == 2, "{folder}: {} m³, {} units", w.volume, w.units.len());
    }
    assert!(opened.steps.iter().filter(|c| ["initialise", "join_benches", "rom_loss", "rom"].contains(&c.step.as_str())).all(|c| c.writes.is_empty()), "steps that write nothing here");
    // What each built-in step reads of the units (decision 0084): dilution its thickness, volume, area and length (edge
    // dilution runs along the block); final the volume and the quality; the column as input nothing.
    let read = |step: &str| opened.steps.iter().find(|c| c.step == step).unwrap().fields_read.clone();
    assert_eq!(read("rom_dilution"), ["thickness", "volume", "area", "length"]);
    assert_eq!(read("rom_loss"), read("rom_dilution"));
    assert_eq!(read("final"), ["volume", "ash"]);
    assert_eq!(read("initialise"), ["thickness", "area"]);
    assert!(opened.steps[0].fields_read.is_empty() && read("rom").is_empty());

    // final: ROM coal, A's and B's.
    let last = opened.steps.iter().find(|c| c.step == "final").unwrap();
    let rom_coal = last.writes.iter().find(|w| w.folder == "ROM\\Coal").unwrap_or_else(|| panic!("{:?}", last.writes));
    assert!(close(rom_coal.volume, 10_854.0 + 15_906.0), "{}", rom_coal.volume);

    // Every folder's writes over the steps are the table's total for it (one block, so the table is this column's).
    let mut written: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for cell in &opened.steps {
        for w in &cell.writes {
            *written.entry(w.folder.clone()).or_default() += w.volume;
        }
    }
    for (folder, v) in &written {
        let path = format!("{folder}\\volume");
        assert!(close(total(&t, &path), *v), "{path}: table {} written {v}", total(&t, &path));
    }
    // And no volume in the table that no step wrote (the summary is the table's own, worked from the folders).
    let unwritten: Vec<&str> = t
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .filter(|n| n.ends_with("\\volume") && !n.starts_with("Summary\\"))
        .filter(|n| !written.contains_key(n.trim_end_matches("\\volume")) && total(&t, n) != 0.0)
        .collect();
    assert!(unwritten.is_empty(), "{unwritten:?}");
}

/// Floor loss 0.2 m alone: A's 1,000 m³ goes into A-B, written in ROM waste on A-B's leaf (B's), its first unit; B's goes
/// into the waste below, and is ROM waste on its leaf, coal lost. ROM coal A 9,000 m³ and B 14,000 m³; ROM waste on B's
/// leaf A-B's 25,000 + A's 1,000 m³. The 20,000 m³ of waste below B, the last coal, is underburden, not mined.
#[test]
fn a_loss_is_rom_waste_where_it_went() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, final_steps(), &dilution_csv([0.0, 0.2, 0.0, 0.0], [0.0; 4], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    assert!(close(at_seam(&t, Some("A"), "ROM\\Coal\\volume"), 9_000.0) && close(at_seam(&t, Some("B"), "ROM\\Coal\\volume"), 14_000.0));
    assert!(close(at_seam(&t, Some("B"), "ROM\\Waste\\volume"), 26_000.0) && close(at_seam(&t, None, "ROM\\Waste\\volume"), 1_000.0));
    assert!(close(total(&t, "ROM\\Coal\\volume") + total(&t, "ROM\\Waste\\volume"), 120_000.0 - 20_000.0));
}

/// No waste below B: 10 m of underburden goes under it, and B's floor dilution of 0.1 m (500 m³) comes from it. ROM coal B
/// takes it, 15,500 m³, and it's reported as out-of-model volume on B's leaf; the rest of the underburden is never
/// written.
#[test]
fn underburden_taken_into_rom_is_out_of_model_volume() {
    let mut s = session();
    let ids: Vec<ObjectId> = loss_column(&mut s).into_iter().take(4).collect();
    let flow = flow(&mut s, final_steps(), &dilution_csv([0.0; 4], [0.0, 0.1, 0.0, 0.0], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    assert!(close(at_seam(&t, Some("B"), "ROM\\Coal\\volume"), 15_500.0));
    assert!(close(at_seam(&t, Some("B"), "Summary\\Out of model\\volume"), 500.0));
    assert!(close(total(&t, "ROM\\Coal\\volume") + total(&t, "ROM\\Waste\\volume"), 100_000.0 + 500.0), "the blocks and the underburden taken");
    assert!(notes_of(&record, Code::ReservesUnderburden).iter().any(|n| n == "1 unit underburden taken into ROM coal, out of model, 500.0 m3"), "{:?}", notes_of(&record, Code::ReservesUnderburden));
}

/// Section A-B (A ash 10 %, B ash 12 %, the parting between): with aggregate_sections yes, A's and B's ROM coal carry the
/// section's ash; without, their own.
#[test]
fn aggregate_sections_puts_the_section_s_quality_on_its_coal() {
    let mut s = session();
    let ids = section_column(&mut s);
    let csv = |yes: &str| format!("when material,rd (t/m3),ash_default (%),min_parting (m),ws_strip_ratio,min_coal_thickness (m),insitu_ash_cutoff (%),roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%),roof_dilution (m),floor_dilution (m),edge_dilution (m),other_dilution (%),aggregate_sections\nwaste,2.3,80,,,,,,,,,,,,,\ncoal,,,1,0.5,0.45,30,0,0,0,0,0,0,0,0,{yes}\n");
    let flow_own = flow(&mut s, final_steps(), &csv(""));
    let record = s.run("reserves.run", json!({ "dataflow": flow_own, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let (a, b) = (at_seam(&t, Some("A"), "ROM\\Coal\\ash"), at_seam(&t, Some("B"), "ROM\\Coal\\ash"));
    assert!(close(a, 10.0), "{a}");
    let flow_agg = flow(&mut s, final_steps(), &csv("yes"));
    let record = s.run("reserves.run", json!({ "dataflow": flow_agg, "blocks": ids, "stack": SECTION_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let (a2, b2) = (at_seam(&t, Some("A"), "ROM\\Coal\\ash"), at_seam(&t, Some("B"), "ROM\\Coal\\ash"));
    assert!(close(a2, b2) && a2 > 10.0 && a2 < 80.0, "{a2} {b2}, own {a} {b}");
    assert!(close(at_seam(&t, Some("A"), "Clean coal\\Coal\\ash"), at_seam(&t, Some("B"), "Clean coal\\Coal\\ash")) && at_seam(&t, Some("A"), "Clean coal\\Coal\\ash") < 12.0, "clean coal its coal's own average");
}

/// Clean coal at its section's average, by tonnes (decision 0077, 7c part 2; Tim: qualities are weighed by tonnes). One
/// section of A (2 m, 10,000 m³, RD 1.3, ash 10 %) and B (1.5 m, 7,500 m³, RD 1.6, ash 20 %) joined over a 0.3 m parting:
/// with `clean_coal_average` yes, each ply's clean coal carries (10 × 13,000 + 20 × 12,000) ÷ 25,000 = 14.8 %, not the
/// 14.29 % a volume average would give; without, its own.
#[test]
fn clean_coal_takes_its_section_s_average_by_tonnes() {
    let mut s = session();
    let rows: [(&str, f64, f64, &str, &str, f64, f64); 5] =
        [("OB", 40.0, 50.0, "waste", "A", 0.0, 0.0), ("A", 38.0, 40.0, "coal", "A", 1.3, 10.0), ("A-B", 37.7, 38.0, "waste", "B", 0.0, 0.0), ("B", 36.2, 37.7, "coal", "B", 1.6, 20.0), ("below B", 30.0, 36.2, "waste", "", 0.0, 0.0)];
    let ids: Vec<ObjectId> = rows
        .iter()
        .map(|&(h, z0, z1, material, seam, rd, ash)| {
            let mut a = json!({ "strip": 1.0, "block": "B1", "horizon": h, "material": material, "seam": seam });
            if material == "coal" {
                a["rd"] = json!(rd);
                a["ash"] = json!(ash);
            }
            box_solid(&mut s, &format!("1_B1_{h}"), 0.0, (z0, z1), a)
        })
        .collect();
    s.run("solid.measure", json!({ "solids": ids })).unwrap();
    let stack = ["OB", "A", "A-B", "B", "below B"];
    let csv = |yes: &str| format!("when material,rd (t/m3),ash_default (%),min_parting (m),ws_strip_ratio,min_coal_thickness (m),roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%),roof_dilution (m),floor_dilution (m),edge_dilution (m),other_dilution (%),clean_coal_average\nwaste,2.3,80,,,,,,,,,,,,\ncoal,,,1,0.5,0.45,0,0,0,0,0,0,0,0,{yes}\n");
    let clean = |s: &mut Session, yes: &str| {
        let flow = flow(s, final_steps(), &csv(yes));
        let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": stack, "layer": "reserves" })).unwrap();
        let t = table(s, created(&record)[0]);
        (at_seam(&t, Some("A"), "Clean coal\\Coal\\ash"), at_seam(&t, Some("B"), "Clean coal\\Coal\\ash"))
    };
    let (a, b) = clean(&mut s, "");
    assert!(close(a, 10.0) && close(b, 20.0), "each its own: {a} {b}");
    let (a, b) = clean(&mut s, "yes");
    let by_tonnes = (10.0 * 13_000.0 + 20.0 * 12_000.0) / 25_000.0;
    assert!(close(a, by_tonnes) && close(b, by_tonnes), "{a} {b} against {by_tonnes}");
}

/// Coal at Mad 3 %, Mis 8 %, Mrom 10 %; A-B gives A 500 m³ of floor dilution. The dilution, with no moisture of its own,
/// takes A's (said), so its ROM density is 2.3 × 0.92 ÷ 0.90; A's ROM tonnes are its coal's 10,000 m³ at its in situ
/// density × 0.92 ÷ 0.90 and the dilution's 500 m³ at that, and A's ash on the ROM basis is its air-dried ash
/// × 0.90 ÷ 0.97.
#[test]
fn rom_gives_dilution_its_coal_s_moisture_and_works_rom_tonnes() {
    let mut s = session();
    let ids = loss_column(&mut s);
    let csv = "when material,rd (t/m3),ash_default (%),mad (%),mis (%),mrom (%),min_parting (m),ws_strip_ratio,min_coal_thickness (m),roof_loss (m),floor_loss (m),edge_loss (m),other_loss (%),roof_dilution (m),floor_dilution (m),edge_dilution (m),other_dilution (%)\nwaste,2.3,80,,,,,,,,,,,,,,\ncoal,,,3,8,10,1,0.5,0.5,0,0,0,0,0,0.1,0,0\n";
    let table_id = created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": csv })).unwrap())[0];
    let content = json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "area", "kind": "area", "add_up": { "by": "max" }, "from": "plan_area" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
            { "name": "mad", "kind": "moisture", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "mad" },
            { "name": "rd_is", "kind": "density", "add_up": { "by": "volume" }, "from": "worked" },
            { "name": "rd_rom", "kind": "density", "add_up": { "by": "volume" }, "from": "worked" },
            { "name": "mis", "kind": "moisture", "add_up": { "by": "mass", "density": "rd_is" }, "from": "attribute", "attribute": "mis" },
            { "name": "mrom", "kind": "moisture", "add_up": { "by": "mass", "density": "rd_rom" }, "from": "parameter", "column": "mrom" },
        ],
        "steps": [
            { "step": "initialise" }, { "step": "in_situ" }, { "step": "join_benches" }, { "step": "working_sections" }, { "step": "rom_loss" },
            { "step": "rom_dilution" }, { "step": "rom_ash_cutoff" }, { "step": "rom" }, { "step": "final" }
        ],
        "parameters": [table_id],
    });
    let flow = created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": starter(content) })).unwrap())[0];
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    let dis = 1.4 * 0.97 / (1.0 + 1.4 * 0.05 - 0.08);
    let tonnes = 10_000.0 * dis * 0.92 / 0.90 + 500.0 * 2.3 * 0.92 / 0.90;
    let got = at_seam(&t, Some("A"), "ROM\\Coal\\tonnes");
    assert!(close(got, tonnes), "{got} against {tonnes}");
    // Said by the starter's rom_qualities formula (decision 0077, 7c part 2).
    let said = notes_of(&record, Code::ReservesFormulaSaid);
    for m in ["mad", "mis", "mrom"] {
        assert!(said.iter().any(|n| n.ends_with(&format!("dilution and partings given their coal's {m}, 1000.0 m3")) || n.contains(&format!("given their coal's {m}"))), "{m}: {said:?}");
    }
    assert!(at_seam(&t, Some("A"), "ROM\\Coal\\ash_rom").is_finite() && at_seam(&t, Some("A"), "ROM\\Coal\\ash_rom") > 0.0);
}

/// A box in plan `u0..u1` along and `v0..v1` across, turned `deg` at mine coordinates, from RL 30 to 32.
fn turned_box(s: &mut Session, name: &str, deg: f64, (u0, u1): (f64, f64), (v0, v1): (f64, f64), attributes: serde_json::Value) -> ObjectId {
    let (c, sn) = (deg.to_radians().cos(), deg.to_radians().sin());
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| {
            let u = if i & 1 == 1 { u1 } else { u0 };
            let v = if i >> 1 & 1 == 1 { v1 } else { v0 };
            [E + u * c - v * sn, N + u * sn + v * c, if i >> 2 & 1 == 1 { 32.0 } else { 30.0 }]
        })
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let id = created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "blocks", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    s.run("test.tag", json!({ "object": id, "attributes": attributes })).unwrap();
    id
}

/// Length is along the strip, width across it (Tim, 15 November 2026). Strip 1 is three blocks 50 m along and 80 m across,
/// turned 30°: each is 50 m long and 80 m wide, though 80 m is its longer side. A fourth block in another pit's strip 1,
/// 200 m away, isn't part of that strip: on its own, as is a block with no strip, its longer side is its length.
#[test]
fn length_is_along_the_strip_and_width_across_it() {
    let mut s = session();
    let strip: Vec<ObjectId> = (0..3)
        .map(|k| turned_box(&mut s, &format!("B{k}"), 30.0, (50.0 * k as f64, 50.0 * (k + 1) as f64), (0.0, 80.0), json!({ "strip": 1.0, "block": format!("B{k}") })))
        .collect();
    let other_pit = turned_box(&mut s, "other", 30.0, (0.0, 50.0), (200.0, 280.0), json!({ "pit": "North", "strip": 1.0 }));
    let loose = turned_box(&mut s, "loose", 30.0, (0.0, 50.0), (400.0, 480.0), json!({}));
    let mut ids = strip.clone();
    ids.extend([other_pit, loose]);
    let record = s.run("solid.measure", json!({ "solids": ids })).unwrap();
    assert!(record.messages[0].contains("length along each of 2 strips and width across; 1 block with no strip measured on its own"), "{}", record.messages[0]);
    let n = |id: ObjectId, k: &str| match s.project().object(id).unwrap().attributes.get(k) {
        Some(zd_core::AttrValue::Number(v)) => *v,
        other => panic!("{k}: {other:?}"),
    };
    for id in strip {
        assert!((n(id, "length") - 50.0).abs() < 1e-9 && (n(id, "width") - 80.0).abs() < 1e-9, "{} {}", n(id, "length"), n(id, "width"));
    }
    for id in [other_pit, loose] {
        assert!((n(id, "length") - 80.0).abs() < 1e-9 && (n(id, "width") - 50.0).abs() < 1e-9, "{} {}", n(id, "length"), n(id, "width"));
    }
}

/// No waste below B: B's 0.2 m floor loss (1,000 m³) goes into the underburden, which has no leaf, so it's ROM waste on
/// B's own leaf; no leaf without a seam is written.
#[test]
fn a_loss_into_the_underburden_is_rom_waste_on_its_own_leaf() {
    let mut s = session();
    let ids: Vec<ObjectId> = loss_column(&mut s).into_iter().take(4).collect();
    let flow = flow(&mut s, final_steps(), &dilution_csv([0.0, 0.2, 0.0, 0.0], [0.0; 4], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let t = table(&s, created(&record)[0]);
    assert!(texts(&t, "seam").iter().all(Option::is_some), "{:?}", texts(&t, "seam"));
    assert!(close(at_seam(&t, Some("B"), "ROM\\Coal\\volume"), 14_000.0));
    assert!(close(at_seam(&t, Some("B"), "ROM\\Waste\\volume"), 25_000.0 + 1_000.0 + 1_000.0), "A-B's own, A's floor loss into it and B's own floor loss: {}", at_seam(&t, Some("B"), "ROM\\Waste\\volume"));
    assert!(close(total(&t, "ROM\\Coal\\volume") + total(&t, "ROM\\Waste\\volume"), 100_000.0));
}

/// A reserves table carries its levels and their defaults, so each block's solid is matched to the leaf the run put it
/// on (decision 0075): every block of the loss column, which has no `pit`, lands on a leaf the table holds, under Pit.
#[test]
fn a_table_s_leaves_are_matched_to_their_solids() {
    use zd_commands::reserves_view::{breakdown, case_of, leaf_of, level_defaults, levels};
    let mut s = session();
    let ids = loss_column(&mut s);
    let flow = flow(&mut s, final_steps(), &dilution_csv([0.0; 4], [0.0; 4], ""));
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": LOSS_STACK, "layer": "reserves" })).unwrap();
    let id = created(&record)[0];
    let object = s.project().object(id).unwrap().clone();
    assert_eq!(object.attributes.get("levels"), Some(&zd_core::AttrValue::Text("pit=Pit;strip;block;seam;bench".into())));
    let t = table(&s, id);
    let (levels, defaults) = (levels(&t), level_defaults(&object.attributes));
    assert_eq!(levels, ["pit", "strip", "block", "seam", "bench"]);
    for &b in &ids {
        let solid = s.project().object(b).unwrap();
        let leaf = leaf_of(&levels, &defaults, &solid.attributes);
        assert_eq!(leaf[0], "Pit", "the default where a block has none");
        assert_eq!(case_of(&solid.attributes), "Base");
        assert!(breakdown(&t, &leaf, &[]).is_ok(), "{leaf:?} is a leaf of the table");
    }
    // The block's breakdown: its seams across, its ROM coal and waste the blocks' 120,000 m³ less the 20,000 m³ of waste
    // below the last coal, which is netted off the out-of-model volume.
    let b = breakdown(&t, &["Pit".into(), "1".into(), "B1".into()], &[]).unwrap();
    let total = |f: &str| b.fields.iter().find(|x| x.column == f).and_then(|x| x.values[0]).unwrap_or(0.0);
    assert!(close(total("ROM\\Coal\\volume") + total("ROM\\Waste\\volume"), 100_000.0));
    assert!(close(total("Summary\\Out of model\\volume"), -20_000.0));
}
