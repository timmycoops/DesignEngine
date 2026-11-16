//! Formula steps in a dataflow (decision 0077, workstream 7b), through the registry, on the made-up strip of
//! `reserves_run.rs`: two blocks, B1 and B2, of OB, U (B2's in two benches), U-L and L, 100 m by 50 m each.
//!
//! | horizon | material | B1 m³  | B2 m³            | RD       | ash |
//! |---------|----------|--------|------------------|----------|-----|
//! | OB      | waste    | 50,000 | 50,000           | 2.3      |     |
//! | U       | coal     | 20,000 | 10,000 + 10,000  | 1.4; 1.6 | 10  |
//! | U-L     | waste    | 5,000  | 5,000            | 2.3      |     |
//! | L       | coal     | 15,000 | 15,000           | 1.5      | 20  |
//!
//! The parameter table gives waste an RD of 2.3 and everything an ash cut-off of 35 %.

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::builtin::{RunReservesParams, reserves_preview};
use zd_commands::{Code, Command, CommandError, CommandSpec, Inputs, Outcome, Registry, RunRecord, Session};
use zd_core::{Attributes, Body, Change, ChangeKind, ColumnValues, ObjectId, Table};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;
const TOLERANCE: f64 = 1e-9;
const STACK: [&str; 5] = ["OB", "U", "U-L", "L", "below L"];

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
    registry.register(Tag);
    Session::new(registry)
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

fn blocks(s: &mut Session) -> Vec<ObjectId> {
    let mut ids = Vec::new();
    for (b, x) in [("B1", 0.0), ("B2", 100.0)] {
        let tag = |mut a: serde_json::Value| {
            a["strip"] = json!(3.0);
            a["block"] = json!(b);
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

fn table_id(s: &mut Session) -> ObjectId {
    created(&s.run("parameters.save", json!({ "name": "assumptions", "csv": "when material,when seam,rd (t/m3),ash_cutoff (%)\nwaste,,2.3,\n,,,35\n" })).unwrap())[0]
}

/// The usual fields and one a formula works, `ash2`.
fn content(table: ObjectId, steps: serde_json::Value) -> serde_json::Value {
    json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "area", "kind": "area", "add_up": { "by": "max" }, "from": "plan_area" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
            { "name": "ash_cutoff", "kind": "setting", "add_up": { "by": "min" }, "from": "parameter", "column": "ash_cutoff" },
            { "name": "ash2", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "worked" },
        ],
        "steps": steps,
        "parameters": [table],
    })
}

fn save(s: &mut Session, steps: serde_json::Value) -> Result<RunRecord, CommandError> {
    let table = table_id(s);
    s.run("dataflow.save", json!({ "name": "Reserves", "content": content(table, steps) }))
}

fn run(s: &mut Session, flow: ObjectId, ids: &[ObjectId]) -> RunRecord {
    s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap()
}

fn table(s: &Session, id: ObjectId) -> Table {
    let Body::Table(t) = &s.project().object(id).unwrap().body else { panic!("a table") };
    (**t).clone()
}

fn texts(t: &Table, name: &str) -> Vec<Option<String>> {
    let ColumnValues::Text(v) = &t.column(name).unwrap().values else { panic!("words") };
    v.clone()
}

fn leaf(t: &Table, block: &str, seam: &str, bench: Option<&str>) -> usize {
    let (blocks, seams, benches) = (texts(t, "block"), texts(t, "seam"), texts(t, "bench"));
    (0..t.rows()).find(|&i| blocks[i].as_deref() == Some(block) && seams[i].as_deref() == Some(seam) && benches[i].as_deref() == bench).expect("a leaf")
}

fn value(t: &Table, at: usize, path: &str) -> Option<f64> {
    t.column(path).and_then(|c| match &c.values {
        ColumnValues::Number(v) => v[at],
        _ => None,
    })
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE * b.abs().max(1.0)
}

fn notes_of(record: &RunRecord, code: Code) -> Vec<String> {
    record.notes.iter().filter(|n| n.code == Some(code)).map(|n| n.text.clone()).collect()
}

const DOUBLE_ASH: &str = "def formula(unit, p):\n    unit.ash2 = unit.ash * 2 + p.ash_cutoff\n";

/// After initialise, U's coal gets ash2 = 10 × 2 + 35 = 55; L's coal and the waste, outside the filter, get none. The
/// run names the formula step among the steps it ran.
#[test]
fn a_formula_step_sets_a_field_on_the_units_its_filter_fits() {
    let mut s = session();
    let ids = blocks(&mut s);
    let steps = json!([
        { "step": "initialise" },
        { "step": "double_ash", "formula": { "when": { "material": "coal", "seam": "U" }, "script": DOUBLE_ASH } },
        { "step": "in_situ" },
        { "step": "join_benches" },
    ]);
    let saved = save(&mut s, steps).unwrap();
    assert!(saved.messages[0].contains("(1 formula checked)"), "{}", saved.messages[0]);
    let record = run(&mut s, created(&saved)[0], &ids);
    assert!(record.messages[0].starts_with("2 of 2 blocks ran through initialise, double_ash, in_situ, join_benches"), "{}", record.messages[0]);
    let t = table(&s, created(&record)[0]);
    for (block, bench) in [("B1", None), ("B2", Some("18")), ("B2", Some("16"))] {
        let u = leaf(&t, block, "U", bench);
        assert!(close(value(&t, u, "Insitu\\Coal\\ash2").expect("U's coal has ash2"), 55.0), "{block} {bench:?}");
    }
    assert_eq!(value(&t, leaf(&t, "B1", "L", None), "Insitu\\Coal\\ash2"), None, "L is outside the filter");
    assert!(notes_of(&record, Code::ReservesUnbalanced).is_empty(), "every column kept its volume");
}

/// A formula reads its block's attributes and the unit above, and what it prints is said with its step's name.
#[test]
fn a_formula_reads_its_block_and_the_unit_above_and_its_prints_are_said() {
    let mut s = session();
    let ids = blocks(&mut s);
    let script = "def formula(unit):\n    under_waste = unit.above != None and unit.above.material == \"waste\"\n    unit.ash2 = unit.block.strip * 10 + (1 if under_waste else 0)\n    if under_waste:\n        print(\"coal under waste\")\n";
    let steps = json!([{ "step": "initialise" }, { "step": "under", "formula": { "when": { "material": "coal" }, "script": script } }, { "step": "in_situ" }]);
    let flow = created(&save(&mut s, steps).unwrap())[0];
    let record = run(&mut s, flow, &ids);
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "U", None), "Insitu\\Coal\\ash2").unwrap(), 31.0), "strip 3 × 10, under OB");
    assert!(close(value(&t, leaf(&t, "B2", "U", Some("16")), "Insitu\\Coal\\ash2").unwrap(), 30.0), "B2's lower bench is under its upper, coal");
    assert!(close(value(&t, leaf(&t, "B1", "L", None), "Insitu\\Coal\\ash2").unwrap(), 31.0), "L under U-L");
    // B1's U, B2's upper bench and both L's: 20,000 + 10,000 + 15,000 + 15,000 m³.
    assert_eq!(notes_of(&record, Code::ReservesFormulaSaid), ["4 units printed by under: coal under waste, 60000.0 m3"]);
}

/// The preview shows the column after a formula step as after any other.
#[test]
fn the_preview_shows_a_formula_step_like_any_other() {
    let mut s = session();
    let ids = blocks(&mut s);
    let steps = json!([{ "step": "initialise" }, { "step": "double_ash", "formula": { "when": { "material": "coal" }, "script": DOUBLE_ASH } }]);
    let flow = created(&save(&mut s, steps).unwrap())[0];
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap();
    let project = s.project();
    let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
    let views = reserves_preview(&get, &p, "Base", &["Pit".to_string(), "3".to_string(), "B1".to_string()]).unwrap();
    let steps: Vec<&str> = views.iter().map(|v| v.step.as_str()).collect();
    assert_eq!(steps, ["as input", "initialise", "double_ash"]);
    let l = views[2].units.iter().find(|u| u.horizon == "L").expect("L");
    assert!(close(l.values["ash2"], 75.0), "20 × 2 + 35");
}

/// A dataflow whose formula doesn't compile, sits in the wrong place, takes a built-in step's name or fails its known
/// answers isn't saved; one that passes them is.
#[test]
fn a_formula_step_is_checked_when_the_dataflow_is_saved() {
    let mut s = session();
    let refused = |s: &mut Session, steps: serde_json::Value| save(s, steps).unwrap_err().to_string();
    let err = refused(&mut s, json!([{ "step": "initialise" }, { "step": "x", "formula": { "script": "def formula(unit):\n    unit.ash2 = unit.ahs\n" } }]));
    assert!(err.contains("the formula step x: line 2: `unit.ahs` isn't a field of this dataflow (did you mean `ash`?)"), "{err}");
    let err = refused(&mut s, json!([{ "step": "x", "formula": { "script": DOUBLE_ASH } }, { "step": "initialise" }]));
    assert!(err.contains("sits before any built-in step"), "{err}");
    let err = refused(&mut s, json!([{ "step": "initialise" }, { "step": "in_situ" }, { "step": "x", "formula": { "script": DOUBLE_ASH } }]));
    assert!(err.contains("sits after in_situ"), "{err}");
    let err = refused(&mut s, json!([{ "step": "initialise" }, { "step": "rom", "formula": { "script": DOUBLE_ASH } }]));
    assert!(err.contains("has a built-in step's name"), "{err}");
    let test = |expect: f64| json!([{ "step": "initialise" }, { "step": "x", "formula": { "script": DOUBLE_ASH, "tests": [{ "name": "U at 10 %", "unit": { "ash": 10.0 }, "parameters": { "ash_cutoff": 35.0 }, "expect": { "ash2": expect } }] } }]);
    let err = refused(&mut s, test(56.0));
    assert!(err.contains("the formula step x, test U at 10 %: ash2 is 55, not 56"), "{err}");
    save(&mut s, test(55.0)).expect("its known answer holds");
}

/// A formula that stops on a unit stops that block, said with the step, the line and the solid; the rest go on. Setting
/// the volume, the volume of record, stops it too.
#[test]
fn a_formula_that_stops_on_a_unit_stops_its_block_and_the_rest_go_on() {
    let mut s = session();
    let ids = blocks(&mut s);
    // Coal has no RD row: reading p.rd stops B1, the only block the filter takes.
    let steps = json!([{ "step": "initialise" }, { "step": "rd_check", "formula": { "when": { "material": "coal", "block": "B1", "seam": "U" }, "script": "def formula(unit, p):\n    unit.ash2 = p.rd\n" } }, { "step": "in_situ" }]);
    let flow = created(&save(&mut s, steps).unwrap())[0];
    let record = run(&mut s, flow, &ids);
    assert!(record.messages[0].starts_with("1 of 2 blocks ran"), "{}", record.messages[0]);
    let stopped = notes_of(&record, Code::ReservesBlockStopped).join(" | ");
    assert!(stopped.contains("at rd_check: line 2: no parameter row gives `rd` for this unit") && stopped.contains("on 3_B1_U"), "{stopped}");
    let steps = json!([{ "step": "initialise" }, { "step": "grow", "formula": { "when": { "seam": "L" }, "script": "def formula(unit):\n    unit.volume = unit.volume * 2\n" } }]);
    let flow = created(&save(&mut s, steps).unwrap())[0];
    let record = s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" }));
    let said = match record {
        Ok(r) => notes_of(&r, Code::ReservesBlockStopped).join(" | "),
        Err(e) => e.to_string(),
    };
    assert!(said.contains("the volume is of record"), "{said}");
}
