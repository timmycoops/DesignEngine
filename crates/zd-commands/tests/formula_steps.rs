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
    registry.register(zd_commands::Module::Design, Tag);
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

/// The formula panel's check (decision 0077, 7d): a formula as edited, never saved, said problem by problem with its
/// line, and each known answer held or not; the names it may use with their units.
#[test]
fn a_formula_as_edited_is_checked_problem_by_problem() {
    use zd_commands::builtin::{STEPS, check_formula};
    let mut s = session();
    let table = table_id(&mut s);
    let tests = json!([
        { "name": "U at 10 %", "unit": { "ash": 10.0 }, "parameters": { "ash_cutoff": 35.0 }, "expect": { "ash2": 55.0 } },
        { "name": "wrong on purpose", "unit": { "ash": 10.0 }, "parameters": { "ash_cutoff": 35.0 }, "expect": { "ash2": 56.0 } },
    ]);
    let content = content(table, json!([{ "step": "initialise" }, { "step": "x", "formula": { "script": DOUBLE_ASH, "tests": tests } }]));
    let d: zd_core::Dataflow = serde_json::from_value(content.clone()).unwrap();
    let Body::ParameterTable(t) = &s.project().object(table).unwrap().body else { panic!("a table") };
    let built_in: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
    let report = check_formula(&d, &[&**t], 1, &built_in).unwrap();
    assert_eq!(report.stage.as_deref(), Some("initialise"));
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.tests.iter().map(|t| (t.name.as_str(), t.ok)).collect::<Vec<_>>(), [("U at 10 %", true), ("wrong on purpose", false)]);
    assert_eq!(report.tests[1].why.as_deref(), Some("ash2 is 55, not 56"));
    // A worked example can expect no value, or a stop.
    let mut d2 = d.clone();
    let f2 = d2.steps[1].formula.as_mut().unwrap();
    f2.script = "def formula(unit, p):\n    if unit.ash == None:\n        fail(\"no ash on this coal\")\n    unit.ash2 = None if unit.ash > 50 else unit.ash * 2\n".into();
    f2.tests = serde_json::from_value(json!([
        { "name": "dirty: none", "unit": { "ash": 60.0 }, "expect": { "ash2": null } },
        { "name": "no ash: stops", "unit": {}, "stops": "no ash" },
        { "name": "stops, said wrong", "unit": {}, "stops": "no rd" },
        { "name": "runs on, said to stop", "unit": { "ash": 10.0 }, "stops": "no ash" },
    ]))
    .unwrap();
    let report = check_formula(&d2, &[&**t], 1, &built_in).unwrap();
    let held: Vec<(bool, Option<&str>)> = report.tests.iter().map(|t| (t.ok, t.why.as_deref())).collect();
    assert_eq!(held[..2], [(true, None), (true, None)]);
    assert_eq!(held[2], (false, Some("it stopped with \"no ash on this coal\", not \"no rd\"")));
    assert_eq!(held[3], (false, Some("it ran on; it should stop with \"no ash\"")));
    assert!(report.names.fields.contains(&("rd".to_string(), "t/m3".to_string())) && report.names.parameters.iter().any(|p| p.0 == "ash_cutoff" && p.1 == "%"));
    // Two problems at once, each with its line.
    let mut broken = d.clone();
    broken.steps[1].formula.as_mut().unwrap().script = "def formula(unit, p):\n    unit.ash2 = unit.ahs\n    unit.seam = p.ash_cutof\n".into();
    let report = check_formula(&broken, &[&**t], 1, &built_in).unwrap();
    let lines: Vec<Option<usize>> = report.problems.iter().map(|p| p.line).collect();
    assert_eq!(lines, [Some(2), Some(3), Some(3)], "{:?}", report.problems);
    assert!(report.tests.is_empty(), "a formula that doesn't compile runs no tests");
}

/// The formula panel's trace on a picked solid: B1's U column run up to the formula as edited, the formula traced on
/// its coal: ash2 = 10 × 2 + 35, line 2, its reads and its set. L, outside the filter, is said so.
#[test]
fn a_formula_is_traced_on_a_picked_solid_s_unit() {
    use zd_commands::builtin::formula_trace;
    let mut s = session();
    let ids = blocks(&mut s);
    let table = table_id(&mut s);
    let steps = json!([{ "step": "initialise" }, { "step": "double_ash", "formula": { "when": { "seam": "U" }, "script": DOUBLE_ASH } }, { "step": "in_situ" }]);
    let d: std::sync::Arc<zd_core::Dataflow> = std::sync::Arc::new(serde_json::from_value(content(table, steps)).unwrap());
    let p: RunReservesParams = serde_json::from_value(json!({ "dataflow": ObjectId::new(), "blocks": ids, "stack": STACK, "layer": "trace" })).unwrap();
    let project = s.project();
    let get = |id| project.object(id).cloned().ok_or(CommandError::MissingObject(id));
    let u = ids.iter().copied().find(|&id| project.object(id).unwrap().name == "3_B1_U").unwrap();
    let tr = formula_trace(&get, &d, &p, "double_ash", u).unwrap();
    assert_eq!(tr.block, "3_B1_U");
    assert!(tr.fits && tr.stopped.is_none());
    assert_eq!(tr.lines.iter().map(|l| l.line).collect::<Vec<_>>(), [2]);
    assert_eq!(tr.sets, [("ash2".to_string(), "55".to_string())]);
    assert!(tr.reads.contains(&("p.ash_cutoff".to_string(), "35".to_string())) && tr.reads.contains(&("unit.ash".to_string(), "10".to_string())));
    assert!(tr.before.iter().any(|(k, v)| k == "ash" && *v == Some(10.0)));
    let l = ids.iter().copied().find(|&id| project.object(id).unwrap().name == "3_B1_L").unwrap();
    assert!(!formula_trace(&get, &d, &p, "double_ash", l).unwrap().fits, "L is outside the filter");
}

/// A formula step that loads a project script (Tables and scripts plan, decision 9): `doubled_plus` written once in the
/// script `ash`, the step's ash2 = 10 × 2 + 35 = 55. Saving the dataflow binds the script by name; the run reads it by
/// id and records its revision. The script revised to add 1 more, the dataflow saved again as it was gives 56.
#[test]
fn a_formula_step_loads_a_project_script_and_the_run_records_its_revision() {
    let mut s = session();
    let ids = blocks(&mut s);
    let made = s.run("script.save", json!({ "name": "ash", "source": "def doubled_plus(x, k):\n    return x * 2 + k\n" })).unwrap();
    assert_eq!(made.messages, ["the script ash saved"]);
    let script = created(&made)[0];
    let loading = "load(\"ash\", \"doubled_plus\")\ndef formula(unit, p):\n    unit.ash2 = doubled_plus(unit.ash, p.ash_cutoff)\n";
    let steps = json!([{ "step": "initialise" }, { "step": "double_ash", "formula": { "when": { "material": "coal", "seam": "U" }, "script": loading } }, { "step": "in_situ" }]);
    let saved = save(&mut s, steps.clone()).unwrap();
    let flow = created(&saved)[0];
    let Body::Dataflow(d) = &s.project().object(flow).unwrap().body else { panic!("a dataflow") };
    assert_eq!(d.scripts, [script], "bound by name, kept by id");
    let record = run(&mut s, flow, &ids);
    assert!(record.inputs.iter().any(|i| i.id == script && i.revision == 1), "the run names the script's revision");
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "U", None), "Insitu\\Coal\\ash2").unwrap(), 55.0));

    // The script revised: a run reads it as it is now.
    s.run("script.save", json!({ "script": script, "name": "ash", "source": "def doubled_plus(x, k):\n    return x * 2 + k + 1\n" })).unwrap();
    let record = run(&mut s, flow, &ids);
    assert!(record.inputs.iter().any(|i| i.id == script && i.revision == 2));
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "U", None), "Insitu\\Coal\\ash2").unwrap(), 56.0));
}

/// What a script can't be, said with why (decision 9): a load of a name the project hasn't refuses the dataflow; a
/// script whose Starlark doesn't run is refused on its line; two of one name are refused; a calculated column binds the
/// scripts it loads too.
#[test]
fn a_script_and_what_loads_it_are_checked() {
    let mut s = session();
    let _ = blocks(&mut s);
    let steps = json!([{ "step": "initialise" }, { "step": "double_ash", "formula": { "script": "load(\"nowhere\", \"f\")\ndef formula(unit):\n    unit.ash2 = f(unit.ash)\n" } }, { "step": "in_situ" }]);
    let err = save(&mut s, steps).unwrap_err();
    assert!(err.to_string().contains("can't load nowhere: the project has no script of that name"), "{err}");
    let bad = s.run("script.save", json!({ "name": "maths", "source": "def f(x):\n    return x\ny = nothing_here\n" })).unwrap_err();
    assert!(bad.to_string().contains("line 3"), "{bad}");
    s.run("script.save", json!({ "name": "maths", "source": "def f(x):\n    return x\n" })).unwrap();
    let twice = s.run("script.save", json!({ "name": "maths", "source": "x = 1\n" })).unwrap_err();
    assert!(twice.to_string().contains("a script named maths already"), "{twice}");
}

/// A formula step looks up a parameter table by name with the unit's own worked values as keys (Schedule quantities
/// plan, decision 1): an ash factor by seam thickness, 0 to 3.5 m at 0.9 and 3.5 to 10 m at 1.1. B1's U, 4 m thick, takes
/// 1.1: ash 10 gives 11; its L, 3 m, takes 0.9: ash 20 gives 18. The dataflow keeps the table's id, bound by name; the
/// run names its revision among its inputs and in the table's audit; revised, the next run reads it as it is.
#[test]
fn a_formula_step_looks_up_a_table_by_the_unit_s_own_values() {
    let mut s = session();
    let ids = blocks(&mut s);
    let factors = created(&s.run("parameters.save", json!({ "name": "Factors", "csv": "when material,when thickness,factor\ncoal,0-3.5,0.9\ncoal,3.5-10,1.1\n" })).unwrap())[0];
    let script = "def formula(unit):\n    unit.ash2 = unit.ash * lookup(\"Factors\", \"factor\", material=unit.material, thickness=unit.thickness)\n";
    let steps = json!([{ "step": "initialise" }, { "step": "by_thickness", "formula": { "when": { "material": "coal" }, "script": script } }, { "step": "in_situ" }]);
    let saved = save(&mut s, steps).unwrap();
    let flow = created(&saved)[0];
    let Body::Dataflow(d) = &s.project().object(flow).unwrap().body else { panic!("a dataflow") };
    assert_eq!(d.lookups, [factors], "bound by name, kept by id");
    let record = run(&mut s, flow, &ids);
    assert!(record.inputs.iter().any(|i| i.id == factors && i.revision == 1), "the run names the table's revision");
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "U", None), "Insitu\\Coal\\ash2").unwrap(), 11.0));
    assert!(close(value(&t, leaf(&t, "B1", "L", None), "Insitu\\Coal\\ash2").unwrap(), 18.0));
    let audit = texts(&t, "Summary\\Audit\\parameters");
    assert!(audit.iter().flatten().any(|a| a.contains("Factors r1 (looked up)")), "{audit:?}");

    s.run("parameters.save", json!({ "table": factors, "name": "Factors", "csv": "when material,when thickness,factor\ncoal,0-3.5,0.5\ncoal,3.5-10,1.5\n" })).unwrap();
    let record = run(&mut s, flow, &ids);
    assert!(record.inputs.iter().any(|i| i.id == factors && i.revision == 2));
    let t = table(&s, created(&record)[0]);
    assert!(close(value(&t, leaf(&t, "B1", "U", None), "Insitu\\Coal\\ash2").unwrap(), 15.0));
}

/// What a lookup can't do, said: a table the project hasn't refuses the dataflow when it's saved; a key no row fits stops
/// the block and says the table and keys; a calculated column and a table script bind and look up the same way.
#[test]
fn a_lookup_is_bound_by_name_or_refused() {
    let mut s = session();
    let ids = blocks(&mut s);
    let missing = "def formula(unit):\n    unit.ash2 = lookup(\"Haulage\", \"km\", seam=unit.seam)\n";
    let steps = json!([{ "step": "initialise" }, { "step": "haul", "formula": { "script": missing } }, { "step": "in_situ" }]);
    let e = save(&mut s, steps).unwrap_err().to_string();
    assert!(e.contains("can't look up Haulage: the project has no parameter table of that name"), "{e}");

    s.run("parameters.save", json!({ "name": "Haulage", "csv": "when seam,km\nU,2.5\n" })).unwrap();
    let steps = json!([{ "step": "initialise" }, { "step": "haul", "formula": { "when": { "material": "coal" }, "script": missing } }, { "step": "in_situ" }]);
    let flow = created(&save(&mut s, steps).unwrap())[0];
    let record = run(&mut s, flow, &ids);
    // L has no row, so a column with L stops at the step, said with the table and the key: here, every column.
    let stopped: Vec<&str> = record.notes.iter().map(|n| n.text.as_str()).filter(|t| t.contains("no row of Haulage gives km for seam L")).collect();
    assert!(!stopped.is_empty(), "{:?}", record.notes);
    assert!(created(&record).is_empty(), "no column ran through");
    // Given a row for L, every column runs.
    let Body::Dataflow(d) = &s.project().object(flow).unwrap().body else { panic!("a dataflow") };
    let haulage = d.lookups[0];
    s.run("parameters.save", json!({ "table": haulage, "name": "Haulage", "csv": "when seam,km
U,2.5
L,4
" })).unwrap();
    let record = run(&mut s, flow, &ids);

    // A calculated column looks it up by its row's words; a table script by a group's.
    let tid = created(&record)[0];
    let calc = "def formula(row):\n    row.km = lookup(\"Haulage\", \"km\", seam=row.seam, default=0)\n";
    s.run("table.calculate", json!({ "table": tid, "column": { "name": "km", "script": calc, "adds": { "by": "max" } } })).unwrap();
    let t = table(&s, tid);
    assert_eq!(t.calculated[0].lookups.len(), 1, "the column keeps the table's id");
    let script = "def run(t):\n    s = t.group(\"seam\")\n    return s.with_column(\"km\", [lookup(\"Haulage\", \"km\", seam=w, default=0) for w in s[\"seam\"]]).select([\"seam\", \"km\"])\n";
    let ran = s.run("table.script", json!({ "table": tid, "source": script })).unwrap();
    let made = table(&s, created(&ran)[0]);
    let km = match &made.column("km").unwrap().values {
        ColumnValues::Number(v) => v.clone(),
        _ => panic!("numbers"),
    };
    let seams = match &made.column("seam").unwrap().values {
        ColumnValues::Text(v) => v.clone(),
        _ => panic!("words"),
    };
    let at = |w: &str| seams.iter().position(|s| s.as_deref() == Some(w)).map(|i| km[i]);
    assert_eq!((at("U"), at("L")), (Some(Some(2.5)), Some(Some(4.0))), "{seams:?} {km:?}");
}
