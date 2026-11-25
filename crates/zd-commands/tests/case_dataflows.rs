//! A case's dataflows (the Build Plan's "Reserves case plan", workstream 1, decisions 1 to 5 and 14): a reserves table's
//! setup lists its row dataflows after its reserves dataflow, each run on the table's rows in its range, with its own
//! settings tables, writing only its own columns.
//!
//! A made-up pit: strips 1 to 8, one block each, 100 m by 50 m, OB from RL 20 to 30 (waste, 50,000 m³) over the L seam
//! from RL 12 to 15 (coal, 15,000 m³ at RD 1.5, so 22,500 t). Haulage, a row dataflow, gives each row a haul distance by
//! strip from its settings table (2 km on strips 1 to 3, 3 km on 4 to 6, 5 km on the rest) and the tonne-kilometres of its
//! coal, over strips 1 to 6 only: 45,000 t.km a block on strips 1 to 3, 67,500 on 4 to 6, 337,500 in all.

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::builtin::rerun_params;
use zd_commands::{Code, Command, CommandError, CommandSpec, Inputs, Outcome, Registry, RunRecord, Session};
use zd_core::{Attributes, Body, Change, ChangeKind, ColumnValues, ObjectId, Table};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// Volumes from boxes add exactly; products of them to this, relative.
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

const STACK: [&str; 2] = ["OB", "L"];

/// Strips 1 to 8, a block each: OB over L.
fn pit(s: &mut Session) -> Vec<ObjectId> {
    let mut ids = Vec::new();
    for strip in 1..=8 {
        let x = (strip as f64 - 1.0) * 100.0;
        let tag = |mut a: serde_json::Value| {
            a["strip"] = json!(strip as f64);
            a["block"] = json!("B1");
            a
        };
        ids.push(box_solid(s, &format!("{strip}_B1_OB"), x, (20.0, 30.0), tag(json!({ "horizon": "OB", "material": "waste", "seam": "L" }))));
        ids.push(box_solid(s, &format!("{strip}_B1_L"), x, (12.0, 15.0), tag(json!({ "horizon": "L", "material": "coal", "seam": "L", "rd": 1.5, "ash": 12.0 }))));
    }
    ids
}

/// The reserves dataflow: volume, thickness, RD and ash, `initialise` then `final`.
fn reserves(s: &mut Session) -> ObjectId {
    let name = if s.project().objects().any(|o| o.name == "assumptions") { "assumptions 2" } else { "assumptions" };
    let table = created(&s.run("parameters.save", json!({ "name": name, "csv": "when material,rd (t/m3)\nwaste,2.3\n" })).unwrap())[0];
    let content = json!({
        "fields": [
            { "name": "volume", "kind": "volume", "add_up": { "by": "sum" }, "from": "volume" },
            { "name": "thickness", "kind": "thickness", "add_up": { "by": "sum" }, "from": "thickness" },
            { "name": "rd", "kind": "density", "add_up": { "by": "volume" }, "from": "attribute", "attribute": "rd" },
            { "name": "ash", "kind": "quality", "add_up": { "by": "mass", "density": "rd" }, "from": "attribute", "attribute": "ash" },
        ],
        "steps": [{ "step": "initialise" }, { "step": "final" }],
        "parameters": [table],
    });
    created(&s.run("dataflow.save", json!({ "name": "Reserves", "content": content })).unwrap())[0]
}

/// The column of coal tonnes Haulage reads: with no working sections in this dataflow, the coal as it came in.
const COAL_TONNES: &str = "Imported\\Coal\\tonnes";

/// Haulage: a row dataflow, a distance by strip from its table and the tonne-kilometres of the row's ROM coal.
fn haulage(s: &mut Session, script: &str) -> (ObjectId, ObjectId) {
    let distances = created(&s.run("parameters.save", json!({ "name": "haul distances", "csv": "when strip,distance (km)\n1-3,2\n4-6,3\n,5\n" })).unwrap())[0];
    let content = json!({
        "rows": true,
        "fields": [
            { "name": "distance", "kind": "other", "add_up": { "by": "max" }, "from": "worked" },
            { "name": "tkm", "kind": "other", "add_up": { "by": "sum" }, "from": "worked" },
        ],
        "units": { "distance": "km", "tkm": "t.km" },
        "steps": [{ "step": "haul", "formula": { "script": script } }],
        "parameters": [distances],
    });
    (created(&s.run("dataflow.save", json!({ "name": "Haulage", "content": content })).unwrap())[0], distances)
}

const HAUL: &str = r#"def formula(row, p):
    row.distance = p.distance
    row.tkm = row["Imported\\Coal\\tonnes"] * p.distance
"#;

fn table(s: &Session, id: ObjectId) -> Table {
    let Body::Table(t) = &s.project().object(id).unwrap().body else { panic!("a table") };
    (**t).clone()
}

fn numbers(t: &Table, name: &str) -> Vec<Option<f64>> {
    let Some(c) = t.column(name) else { panic!("no column {name}: {:?}", t.columns.iter().map(|c| &c.name).collect::<Vec<_>>()) };
    let ColumnValues::Number(v) = &c.values else { panic!("numbers") };
    v.clone()
}

fn texts(t: &Table, name: &str) -> Vec<Option<String>> {
    let ColumnValues::Text(v) = &t.column(name).unwrap().values else { panic!("words") };
    v.clone()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOLERANCE * b.abs().max(1.0)
}

/// The pit run through the reserves dataflow into a table: its id, and the dataflow's.
fn case(s: &mut Session) -> (ObjectId, ObjectId) {
    let ids = pit(s);
    let flow = reserves(s);
    let id = created(&s.run("reserves.run", json!({ "dataflow": flow, "blocks": ids, "stack": STACK, "layer": "reserves" })).unwrap())[0];
    (id, flow)
}

/// The table's setup with these row dataflows listed after its reserves dataflow, saved.
fn with_rows(s: &mut Session, id: ObjectId, rows: serde_json::Value) -> Result<RunRecord, String> {
    let mut setup = serde_json::to_value(table(s, id).setup.unwrap()).unwrap();
    setup["row_dataflows"] = rows;
    s.run("table.setup", json!({ "table": id, "setup": setup })).map_err(|e| e.to_string())
}

/// The coal leaf of each strip, by strip: its row.
fn coal_rows(t: &Table) -> Vec<(u32, usize)> {
    let strips = texts(t, "strip");
    let tonnes = numbers(t, COAL_TONNES);
    let mut out: Vec<(u32, usize)> = (0..t.rows()).filter(|&r| tonnes[r].is_some_and(|x| x > 0.0)).map(|r| (strips[r].as_deref().unwrap().parse().unwrap(), r)).collect();
    out.sort();
    out
}

/// A setup saved before row dataflows reads as a list of one, and Run again from it gives the same table, every column
/// (decision 2): exact.
#[test]
fn a_setup_saved_before_reads_as_a_list_of_one_and_runs_the_same() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let before = table(&s, id);
    let setup = before.setup.clone().unwrap();
    assert!(setup.row_dataflows.is_empty(), "none listed: a list of one");
    let json = serde_json::to_string(&setup).unwrap();
    assert!(!json.contains("row_dataflows"), "nothing written where there are none: {json}");
    let p = rerun_params(s.project(), s.log(), id, &[]).unwrap();
    s.run("reserves.run", serde_json::to_value(&p).unwrap()).unwrap();
    let after = table(&s, id);
    // Every column but the audit's time of the run, which says "run again".
    let but_time = |t: &Table| t.columns.iter().filter(|c| c.name != "Summary\\Audit\\run").cloned().collect::<Vec<_>>();
    assert_eq!(but_time(&after), but_time(&before), "the same table, every column");
    assert_eq!(after.columns.len(), before.columns.len());
}

/// Haulage on strips 1 to 6 only (decisions 2 to 4): a distance by strip from its own table times each row's ROM coal,
/// worked by hand; rows on strips 7 and 8 empty, not 0; totals over strips 1 to 6 alone. 1e-9.
#[test]
fn a_row_dataflow_runs_on_the_rows_in_its_range_from_its_own_settings() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let (haul, _) = haulage(&mut s, HAUL);
    with_rows(&mut s, id, json!([{ "dataflow": haul, "range": { "strip": "1-6" } }])).unwrap();
    let record = s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    assert!(record.messages[0].starts_with("Reserves Base: Haulage ran on 6 of 8 rows (strip 1-6), wrote Haulage"), "{:?}", record.messages);
    assert!(record.messages[0].contains("2 rows outside its range left empty"), "{:?}", record.messages);
    let t = table(&s, id);
    let tonnes = numbers(&t, COAL_TONNES);
    let (distance, tkm) = (numbers(&t, "Haulage\\distance"), numbers(&t, "Haulage\\tkm"));
    assert_eq!(t.column("Haulage\\distance").unwrap().unit, "km");
    assert_eq!(t.column("Haulage\\tkm").unwrap().unit, "t.km");
    let coal = coal_rows(&t);
    assert_eq!(coal.len(), 8, "a coal leaf a strip");
    for &(strip, r) in &coal {
        assert!(close(tonnes[r].unwrap(), 22_500.0), "strip {strip}: {:?}", tonnes[r]);
        let km = match strip {
            1..=3 => Some(2.0),
            4..=6 => Some(3.0),
            _ => None,
        };
        assert_eq!(distance[r], km, "strip {strip}");
        match km {
            Some(km) => assert!(close(tkm[r].unwrap(), 22_500.0 * km), "strip {strip}: {:?}", tkm[r]),
            None => assert_eq!(tkm[r], None, "strip {strip} is outside the range: empty, not 0"),
        }
    }
    let total: f64 = tkm.iter().flatten().sum();
    assert!(close(total, 337_500.0), "{total}");
    // Its columns add up by their fields' rules.
    assert_eq!(t.column("Haulage\\tkm").unwrap().add_up, Some(zd_reserves::AddUp::Sum));
}

/// Two dataflows writing one column, a dataflow reading a column of one below it, a column the reserves run writes, and
/// a reserves dataflow listed as a row dataflow are each refused when saved, naming what (decision 5): exact.
#[test]
fn one_writer_a_column_and_reads_only_from_above() {
    let mut s = session();
    let (id, flow) = case(&mut s);
    let (haul, _) = haulage(&mut s, HAUL);
    // Costs, writing Haulage's own column by its full name.
    let clash = json!({ "rows": true, "fields": [{ "name": "Haulage\\tkm", "kind": "other", "add_up": { "by": "sum" }, "from": "worked" }], "steps": [{ "step": "c", "formula": { "script": "def formula(row):\n    row[\"Haulage\\\\tkm\"] = 1\n" } }] });
    let costs = created(&s.run("dataflow.save", json!({ "name": "Costs", "content": clash })).unwrap())[0];
    let said = with_rows(&mut s, id, json!([{ "dataflow": haul }, { "dataflow": costs }])).unwrap_err();
    assert!(said.contains("Haulage and Costs both write Haulage\\tkm"), "{said}");
    // Rates, reading Haulage's column while listed above it.
    let reads = json!({ "rows": true, "fields": [{ "name": "rate", "kind": "other", "add_up": { "by": "max" }, "from": "worked" }], "steps": [{ "step": "r", "formula": { "script": "def formula(row):\n    row.rate = row[\"Haulage\\\\tkm\"] * 2\n" } }] });
    let rates = created(&s.run("dataflow.save", json!({ "name": "Rates", "content": reads })).unwrap())[0];
    let said = with_rows(&mut s, id, json!([{ "dataflow": rates }, { "dataflow": haul }])).unwrap_err();
    assert!(said.contains("Rates reads Haulage\\tkm, which Haulage writes after it"), "{said}");
    // Below it, it may.
    with_rows(&mut s, id, json!([{ "dataflow": haul }, { "dataflow": rates }])).unwrap();
    // A field named as a column the reserves run writes.
    let over = json!({ "rows": true, "fields": [{ "name": COAL_TONNES, "kind": "other", "add_up": { "by": "sum" }, "from": "worked" }], "steps": [{ "step": "o", "formula": { "script": "def formula(row):\n    pass\n" } }] });
    let over = created(&s.run("dataflow.save", json!({ "name": "Over", "content": over })).unwrap())[0];
    let said = with_rows(&mut s, id, json!([{ "dataflow": over }])).unwrap_err();
    assert!(said.contains("Over writes Imported\\Coal\\tonnes, which the reserves run writes"), "{said}");
    // The reserves dataflow as a row dataflow, and a main dataflow that isn't one.
    let said = with_rows(&mut s, id, json!([{ "dataflow": flow }])).unwrap_err();
    assert!(said.contains("reserves dataflow is listed again"), "{said}");
    let other = reserves(&mut s);
    let said = with_rows(&mut s, id, json!([{ "dataflow": other }])).unwrap_err();
    assert!(said.contains("isn't a row dataflow"), "{said}");
}

/// A formula setting a column not its own is refused at the run, naming it; a row a formula stops on keeps its columns
/// empty, said with the first, and the rest are worked. Exact.
#[test]
fn a_row_dataflow_writes_only_its_own_and_a_row_it_stops_on_is_said() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let sets_other = "def formula(row, p):\n    row.distance = p.distance\n    row[\"Imported\\\\Coal\\\\tonnes\"] = 0\n    row.tkm = 0\n";
    let (haul, _) = haulage(&mut s, sets_other);
    with_rows(&mut s, id, json!([{ "dataflow": haul }])).unwrap();
    let said = s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap_err().to_string();
    assert!(said.contains("sets Imported\\Coal\\tonnes, which Haulage doesn't write"), "{said}");

    let mut s = session();
    let (id, _) = case(&mut s);
    // Stops past 4 km (strips 7 and 8, 5 km): those rows keep their columns empty, both of them, the distance set
    // before the stop too; the other six are worked.
    let stops = r#"def formula(row, p):
    row.distance = p.distance
    if p.distance > 4:
        fail("too far to haul")
    row.tkm = row["Imported\\Coal\\tonnes"] * p.distance
"#;
    let (haul, _) = haulage(&mut s, stops);
    with_rows(&mut s, id, json!([{ "dataflow": haul }])).unwrap();
    let record = s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    let t = table(&s, id);
    let (distance, tkm) = (numbers(&t, "Haulage\\distance"), numbers(&t, "Haulage\\tkm"));
    for (strip, r) in coal_rows(&t) {
        match strip {
            1..=6 => assert!(close(tkm[r].unwrap(), 22_500.0 * if strip <= 3 { 2.0 } else { 3.0 }), "strip {strip}"),
            _ => assert_eq!((distance[r], tkm[r]), (None, None), "strip {strip} stopped: empty"),
        }
    }
    let stopped: Vec<&zd_commands::Note> = record.notes.iter().filter(|n| n.code == Some(Code::RowsStopped)).collect();
    assert_eq!(stopped.len(), 1, "{:?}", record.notes);
    assert!(stopped[0].text.starts_with("Haulage: haul stopped on 2 rows, its columns left empty there: Base Pit 7 B1 L (line 4: "), "{}", stopped[0].text);
    assert!(stopped[0].text.contains("too far to haul"), "{}", stopped[0].text);
}

/// Run again of the reserves dataflow keeps a row dataflow's columns by each leaf's key until it runs again, and never
/// runs it (decision 6): the same values after. Running a dataflow not in the case, or the reserves one with
/// `table.run_dataflow`, is refused.
#[test]
fn run_again_keeps_a_row_dataflow_s_columns_and_never_runs_it() {
    let mut s = session();
    let (id, flow) = case(&mut s);
    let (haul, _) = haulage(&mut s, HAUL);
    with_rows(&mut s, id, json!([{ "dataflow": haul, "range": { "strip": "1-6" } }])).unwrap();
    s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    let ran = table(&s, id);
    let p = rerun_params(s.project(), s.log(), id, &[]).unwrap();
    let again = s.run("reserves.run", serde_json::to_value(&p).unwrap()).unwrap();
    assert!(!again.messages.iter().any(|m| m.contains("Haulage")), "the reserves run doesn't run Haulage: {:?}", again.messages);
    let after = table(&s, id);
    assert_eq!(after.setup, ran.setup, "the case's list kept");
    let key = |t: &Table| -> Vec<(u32, Option<f64>)> {
        let tkm = numbers(t, "Haulage\\tkm");
        coal_rows(t).into_iter().map(|(strip, r)| (strip, tkm[r])).collect()
    };
    assert_eq!(key(&after), key(&ran), "kept by each leaf's key");
    assert!(s.run("table.run_dataflow", json!({ "table": id, "dataflow": flow })).unwrap_err().to_string().contains("reserves dataflow"));
    let content = json!({ "rows": true, "fields": [{ "name": "x", "kind": "other", "add_up": { "by": "sum" }, "from": "worked" }], "steps": [{ "step": "x", "formula": { "script": "def formula(row):\n    row.x = 1\n" } }] });
    let other = created(&s.run("dataflow.save", json!({ "name": "Other", "content": content })).unwrap())[0];
    assert!(s.run("table.run_dataflow", json!({ "table": id, "dataflow": other })).unwrap_err().to_string().contains("isn't one of"));
}

/// A row dataflow's table script step (decision 2): each coal leaf's share of the whole table's coal, 1/8, kept on the
/// rows in its range (strips 1 to 4) and empty on the rest. One giving back a column it doesn't list is refused. Exact.
#[test]
fn a_row_dataflow_s_table_script_writes_its_own_column_on_its_range() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let share = r#"def run(t):
    v = t["Imported\\Coal\\tonnes"]
    whole = t.total("Imported\\Coal\\tonnes")
    return t.with_column("Shares\\share", [x / whole if x != None else None for x in v], adds="sum")
"#;
    let content = json!({ "rows": true, "fields": [{ "name": "share", "kind": "other", "add_up": { "by": "sum" }, "from": "worked" }], "steps": [{ "step": "share", "table": { "script": share } }] });
    let shares = created(&s.run("dataflow.save", json!({ "name": "Shares", "content": content })).unwrap())[0];
    with_rows(&mut s, id, json!([{ "dataflow": shares, "range": { "strip": "1-4" } }])).unwrap();
    s.run("table.run_dataflow", json!({ "table": id, "dataflow": shares })).unwrap();
    let t = table(&s, id);
    let share = numbers(&t, "Shares\\share");
    for (strip, r) in coal_rows(&t) {
        assert_eq!(share[r], if strip <= 4 { Some(0.125) } else { None }, "strip {strip}");
    }
    let stray = r#"def run(t):
    return t.with_column("stray", [1] * len(t))
"#;
    let content = json!({ "rows": true, "fields": [{ "name": "share", "kind": "other", "add_up": { "by": "sum" }, "from": "worked" }], "steps": [{ "step": "share", "table": { "script": stray } }] });
    s.run("dataflow.save", json!({ "dataflow": shares, "name": "Shares", "content": content })).unwrap();
    let said = s.run("table.run_dataflow", json!({ "table": id, "dataflow": shares })).unwrap_err().to_string();
    assert!(said.contains("gave back stray, which Shares doesn't list as a field"), "{said}");
}

/// Each dataflow's state and why (decision 6), read from the log, through a case's life: Haulage never run; both up to
/// date; Haulage's distances changed; the reserves run again after it; a block changed, so Reserves is out of date and
/// Haulage with it; Haulage's range changed in the setup; a run undone. Exact words.
#[test]
fn each_dataflow_says_whether_it_s_up_to_date_and_why() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let (haul, distances) = haulage(&mut s, HAUL);
    with_rows(&mut s, id, json!([{ "dataflow": haul, "range": { "strip": "1-6" } }])).unwrap();
    let state = |s: &Session| -> Vec<(String, bool, Vec<String>)> {
        zd_commands::builtin::case_state(s.project(), s.log(), id).unwrap().dataflows.into_iter().map(|d| (d.name, d.up_to_date, d.why)).collect()
    };
    let ok = |name: &str| (name.to_string(), true, Vec::<String>::new());
    let out = |name: &str, why: &[&str]| (name.to_string(), false, why.iter().map(|w| w.to_string()).collect::<Vec<_>>());
    assert_eq!(state(&s), [ok("Reserves"), out("Haulage", &["it hasn't run on this table"])]);

    s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), ok("Haulage")]);
    let c = zd_commands::builtin::case_state(s.project(), s.log(), id).unwrap();
    assert!(c.dataflows[1].ran.is_some() && c.dataflows[1].range.get("strip").map(String::as_str) == Some("1-6"));
    // What each reads: its own settings tables, by name (workstream 6's Open another dataflow).
    assert_eq!((c.dataflows[0].tables.clone(), c.dataflows[1].tables.clone()), (vec!["assumptions".to_string()], vec!["haul distances".to_string()]));

    // Its distances revised: Haulage alone is out of date.
    s.run("parameters.save", json!({ "table": distances, "name": "haul distances", "csv": "when strip,distance (km)\n1-3,2.5\n4-6,3\n,5\n" })).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), out("Haulage", &["haul distances changed"])]);
    s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), ok("Haulage")]);

    // The reserves dataflow run again: Haulage's numbers are from before it.
    let p = rerun_params(s.project(), s.log(), id, &[]).unwrap();
    s.run("reserves.run", serde_json::to_value(&p).unwrap()).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), out("Haulage", &["Reserves ran after it"])]);
    s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), ok("Haulage")]);

    // A block changed: Reserves out of date, and Haulage with it.
    let block = s.project().objects().find(|o| o.name == "1_B1_L").unwrap().id;
    s.run("test.tag", json!({ "object": block, "attributes": { "ash": 14.0 } })).unwrap();
    assert_eq!(state(&s), [out("Reserves", &["1 block changed"]), out("Haulage", &["Reserves is out of date"])]);
    s.undo();
    assert_eq!(state(&s), [ok("Reserves"), ok("Haulage")], "the change undone: up to date again");

    // Its range changed in the setup.
    with_rows(&mut s, id, json!([{ "dataflow": haul, "range": { "strip": "1-4" } }])).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), out("Haulage", &["its range changed since it ran"])]);
    s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    assert_eq!(state(&s), [ok("Reserves"), ok("Haulage")]);
    // That run undone: back to the run before it, against the range as it is now.
    s.undo();
    assert_eq!(state(&s), [ok("Reserves"), out("Haulage", &["its range changed since it ran"])]);
    // A block added to the layer the setup picks: Reserves hasn't run on it.
    box_solid(&mut s, "9_B1_OB", 800.0, (20.0, 30.0), json!({ "strip": 9.0, "block": "B1", "horizon": "OB", "material": "waste", "seam": "L" }));
    assert_eq!(state(&s)[0], out("Reserves", &["1 block added to what the setup picks"]));
    assert_eq!(state(&s)[1].2, ["its range changed since it ran", "Reserves is out of date"]);
}

const HAUL_IN_TWO: &str = "distance";

const TKM: &str = r#"def formula(row, p):
    row.tkm = row["Imported\\Coal\\tonnes"] * row.distance
"#;

/// Haulage in two steps: the distance, then the tonne-kilometres from it.
fn haulage_in_two(s: &mut Session) -> (ObjectId, ObjectId) {
    let (haul, distances) = haulage(&mut *s, HAUL);
    let Body::Dataflow(d) = &s.project().object(haul).unwrap().body else { panic!("a dataflow") };
    let mut content = serde_json::to_value(&**d).unwrap();
    content["steps"] = json!([
        { "step": HAUL_IN_TWO, "formula": { "script": "def formula(row, p):\n    row.distance = p.distance\n" } },
        { "step": "tkm", "formula": { "script": TKM } },
    ]);
    s.run("dataflow.save", json!({ "dataflow": haul, "name": "Haulage", "content": content })).unwrap();
    (haul, distances)
}

/// A row dataflow at a step (workstream 6, the Reserves case tab): Haulage on strips 1 to 6, the distance then the
/// tonne-kilometres. Before its first step its columns are empty; after it the distances are there (3 km the largest)
/// and no tonne-kilometres; after the second, 337,500 t.km over the range. Nothing is written to the table until it
/// runs, and then the last run holds the same. Strips 1 to 3 edited to 4 km and not saved: 472,500 t.km. 1e-9.
#[test]
fn a_row_dataflow_steps_through_its_rows_before_and_after() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let (haul, distances) = haulage_in_two(&mut s);
    with_rows(&mut s, id, json!([{ "dataflow": haul, "range": { "strip": "1-6" } }])).unwrap();
    let none = zd_commands::builtin::Unsaved::default();
    let at = |s: &Session, step: usize, unsaved: &zd_commands::builtin::Unsaved| zd_commands::builtin::row_dataflow_at_step(s.project(), id, haul, step, unsaved).unwrap();
    let total = |v: &zd_commands::builtin::RowDataflowAtStep, field: &str| v.columns.iter().find(|c| c.field == field).unwrap().clone();

    let first = at(&s, 1, &none);
    assert_eq!(first.name, "Haulage");
    assert_eq!(first.steps.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), [HAUL_IN_TWO, "tkm"]);
    assert_eq!(first.rows_in_range + first.outside, table(&s, id).rows());
    assert!(first.outside > 0 && first.rows.len() == first.rows_in_range);
    assert_eq!(first.tables.iter().map(|(t, n)| (*t, n.as_str())).collect::<Vec<_>>(), [(distances, "haul distances")]);
    let d = total(&first, "distance");
    assert_eq!((d.before, d.after, d.last_run), (None, Some(3.0), None), "{d:?}");
    assert_eq!(total(&first, "tkm").after, None);
    assert_eq!(first.steps[0].sets, ["distance"]);
    assert_eq!(first.steps[1].sets, ["tkm"]);
    assert!(first.steps[1].reads.iter().any(|r| r == COAL_TONNES) && first.steps[1].reads.iter().any(|r| r == "distance"), "{:?}", first.steps[1].reads);
    // A row's before and after, in the columns' order: empty, then its strip's distance.
    let strip_one = first.rows.iter().find(|r| r.label.contains(" 1 ")).unwrap_or(&first.rows[0]);
    assert_eq!(strip_one.before[0], serde_json::Value::Null);
    assert!(strip_one.after[0].is_number(), "{strip_one:?}");

    let second = at(&s, 2, &none);
    let t = total(&second, "tkm");
    assert_eq!(t.before, None);
    assert!(close(t.after.unwrap(), 337_500.0), "{t:?}");
    assert_eq!(total(&second, "distance").before, Some(3.0));
    assert!(table(&s, id).column("Haulage\tkm").is_none(), "nothing written");

    s.run("table.run_dataflow", json!({ "table": id, "dataflow": haul })).unwrap();
    let ran = at(&s, 2, &none);
    assert!(close(total(&ran, "tkm").last_run.unwrap(), 337_500.0));

    // Strips 1 to 3 at 4 km, not saved.
    let edited = zd_commands::builtin::Unsaved { dataflow: None, tables: vec![(distances, std::sync::Arc::new(zd_core::ParameterTable::from_csv("when strip,distance (km)\n1-3,4\n4-6,3\n,5\n").unwrap()))] };
    let e = at(&s, 2, &edited);
    assert!(close(total(&e, "tkm").after.unwrap(), 22_500.0 * (3.0 * 4.0 + 3.0 * 3.0)), "{:?}", total(&e, "tkm"));
    assert!(close(total(&e, "tkm").last_run.unwrap(), 337_500.0), "the last run as it was");
    assert!(zd_commands::builtin::row_dataflow_at_step(s.project(), id, haul, 3, &none).is_err(), "no third step");
}

/// A row dataflow's formula step checked as the formula panel checks it (workstream 7): against the case table's
/// columns, its own fields and its settings, with no stage; the tonne-kilometres step holds, a misspelt field is said on
/// its line, and a setting it hasn't is said too.
#[test]
fn a_row_dataflow_s_formula_is_checked_against_the_case_s_columns() {
    let mut s = session();
    let (id, _) = case(&mut s);
    let (haul, distances) = haulage_in_two(&mut s);
    let t = table(&s, id);
    let numbers: Vec<(String, String)> = t.columns.iter().filter(|c| matches!(c.values, ColumnValues::Number(_))).map(|c| (c.name.clone(), c.unit.clone())).collect();
    let words: Vec<String> = t.columns.iter().filter(|c| matches!(c.values, ColumnValues::Text(_))).map(|c| c.name.clone()).collect();
    let Body::Dataflow(d) = &s.project().object(haul).unwrap().body else { panic!("a dataflow") };
    let Body::ParameterTable(p) = &s.project().object(distances).unwrap().body else { panic!("a table") };
    let check = |d: &zd_core::Dataflow, i: usize| zd_commands::builtin::check_row_formula(d, &[&**p], i, &numbers, &words, Default::default(), Vec::new()).unwrap();
    for i in 0..2 {
        let r = check(d, i);
        assert!(r.problems.is_empty() && r.stage.is_none(), "step {i}: {:?}", r.problems);
    }
    let mut bad = (**d).clone();
    bad.steps[1].formula.as_mut().unwrap().script = TKM.replace("row.distance", "row.distanse");
    let r = check(&bad, 1);
    assert!(r.problems.iter().any(|p| p.message.contains("distanse") && p.line == Some(2)), "{:?}", r.problems);
    bad.steps[0].formula.as_mut().unwrap().script = "def formula(row, p):\n    row.distance = p.distanse\n".into();
    let r = check(&bad, 0);
    assert!(r.problems.iter().any(|p| p.message.contains("distanse")), "{:?}", r.problems);
}
