//! `solid.cut_by_benches` through the registry, as the app calls it (decision 0071): solids cut at bench levels by
//! bench rules, the first rule that fits a solid winning, each piece taking its bench; then cut into blocks, the bench in
//! each block's name. Made-up boxes at mine coordinates carrying the attributes the seam cut writes. Volumes to 1 part
//! in a million.

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::{Code, Command, CommandError, CommandSpec, Inputs, Outcome, Registry, RunRecord, Session, Severity};
use zd_core::{AttrValue, Attributes, Body, Change, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// Sets attributes on an object, as the seam cut leaves them on its pieces.
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

/// A box at mine coordinates, `x` east and `y` north from the origin, RL `z0` to `z1`, carrying `attributes`.
fn box_solid(s: &mut Session, name: &str, x: (f64, f64), y: (f64, f64), z: (f64, f64), attributes: serde_json::Value) -> ObjectId {
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| [E + if i & 1 == 1 { x.1 } else { x.0 }, N + if i >> 1 & 1 == 1 { y.1 } else { y.0 }, if i >> 2 & 1 == 1 { z.1 } else { z.0 }])
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let id = created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "seam pieces", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    s.run("test.tag", json!({ "object": id, "attributes": attributes })).unwrap();
    id
}

fn volume(s: &Session, id: ObjectId) -> f64 {
    let Body::Solid(solid) = &s.project().object(id).unwrap().body else { panic!("a solid") };
    solid.mesh().signed_volume()
}

fn bench(s: &Session, id: ObjectId) -> Option<AttrValue> {
    s.project().object(id).unwrap().attributes.get("bench").cloned()
}

fn names(s: &Session, ids: &[ObjectId]) -> Vec<String> {
    ids.iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect()
}

/// A strip's overburden from RL 0 to 30, 1,500 m by 60 m, cut by the default rule (everything, RL 0 to 500 every 10 m,
/// by the floor): three benches of 900,000 m³, named and labelled 0, 10 and 20, keeping the strip's attributes, in one
/// undo.
#[test]
fn the_default_rule_benches_everything_every_10_m() {
    let mut s = session();
    let ob = box_solid(&mut s, "Strip 3 OB", (0.0, 1500.0), (0.0, 60.0), (0.0, 30.0), json!({ "strip": 3.0, "horizon": "OB", "material": "waste" }));
    let before = s.project().objects().count();
    let record = s.run("solid.cut_by_benches", json!({ "solids": [ob], "layer": "benches" })).unwrap();
    println!("{}", record.messages.join("\n"));
    let made = created(&record);
    assert_eq!(names(&s, &made), ["Strip 3 OB 0", "Strip 3 OB 10", "Strip 3 OB 20"]);
    for (&id, floor) in made.iter().zip([0.0, 10.0, 20.0]) {
        assert_eq!(bench(&s, id), Some(AttrValue::Number(floor)));
        assert!((volume(&s, id) - 900_000.0).abs() < 0.9, "{}", volume(&s, id));
        assert_eq!(s.project().object(id).unwrap().attributes.get("horizon"), Some(&AttrValue::Text("OB".into())));
    }
    assert!(record.notes.iter().all(|n| n.severity < Severity::Warning), "{:?}", record.notes);
    assert!(record.messages.iter().any(|m| m.contains("Benches add back to every solid exactly") || m.contains("within")));
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), before);
}

/// The rules of the plan's example, read top down: the interburden left whole, coal on strips 1 to 5 left whole, other
/// coal every 5 m, everything else every 10 m by its top. Each solid takes the first rule it fits; those no rule benches
/// are carried over whole onto the same layer.
#[test]
fn rules_read_top_down_decide_whats_benched_and_how() {
    let mut s = session();
    let ob = box_solid(&mut s, "S7 OB", (0.0, 100.0), (0.0, 60.0), (20.0, 40.0), json!({ "strip": 7.0, "horizon": "OB", "material": "waste" }));
    let um = box_solid(&mut s, "S7 U-M", (0.0, 100.0), (0.0, 60.0), (10.0, 17.0), json!({ "strip": 7.0, "horizon": "U-M", "material": "waste" }));
    let coal_2 = box_solid(&mut s, "S2 U", (0.0, 100.0), (100.0, 160.0), (17.0, 20.0), json!({ "strip": 2.0, "horizon": "U", "material": "coal", "seam": "U" }));
    let coal_7 = box_solid(&mut s, "S7 U", (0.0, 100.0), (200.0, 260.0), (12.0, 20.0), json!({ "strip": 7.0, "horizon": "U", "material": "coal", "seam": "U" }));
    let rules = json!([
        { "when": { "horizon": "U-M, M-L" }, "benched": false },
        { "when": { "material": "coal", "strip": "1-5" }, "benched": false },
        { "when": { "material": "coal" }, "from": 0.0, "to": 500.0, "every": 5.0 },
        { "from": 0.0, "to": 500.0, "every": 10.0, "named_by": "top" },
    ]);
    let record = s.run("solid.cut_by_benches", json!({ "solids": [ob, um, coal_2, coal_7], "layer": "benches", "rules": rules })).unwrap();
    println!("{}", record.messages.join("\n"));
    let made = created(&record);
    let by_name = |name: &str| *made.iter().find(|&&id| s.project().object(id).unwrap().name == name).unwrap_or_else(|| panic!("{name} in {:?}", names(&s, &made)));
    // The overburden, RL 20 to 40 every 10 m by the top: 30 and 40, 120,000 m³ each.
    for top in [30.0, 40.0] {
        let id = by_name(&format!("S7 OB {top}"));
        assert_eq!(bench(&s, id), Some(AttrValue::Number(top)));
        assert!((volume(&s, id) - 60_000.0).abs() < 0.06, "{}", volume(&s, id));
    }
    // The interburden and strip 2's coal carried over whole, no bench.
    for name in ["S7 U-M", "S2 U"] {
        let id = by_name(name);
        assert_eq!(bench(&s, id), None);
    }
    assert!((volume(&s, by_name("S7 U-M")) - 42_000.0).abs() < 0.042);
    // Strip 7's coal, RL 12 to 20 every 5 m: 10 (12 to 15, 18,000 m³) and 15 (15 to 20, 30,000 m³).
    assert!((volume(&s, by_name("S7 U 10")) - 18_000.0).abs() < 0.018);
    assert!((volume(&s, by_name("S7 U 15")) - 30_000.0).abs() < 0.03);
    assert_eq!(made.len(), 6);
    let whole = record.notes.iter().find(|n| n.text.contains("no rule benches")).expect("those left whole are said");
    assert!(whole.text.contains("'S7 U-M'") && whole.text.contains("'S2 U'"), "{}", whole.text);
    assert!(record.messages.iter().any(|m| m.contains("2 benched into 4 pieces, 2 carried over whole")), "{:?}", record.messages);
}

/// Each rule can send what it takes to a layer of its own: coal benches to one, waste benches to another, the
/// interburden it leaves whole to a third; a solid whose rule names no layer, or that no rule takes, goes to the run's.
#[test]
fn each_rule_can_name_its_own_output_layer() {
    let mut s = session();
    let ob = box_solid(&mut s, "OB", (0.0, 100.0), (0.0, 60.0), (20.0, 40.0), json!({ "horizon": "OB", "material": "waste" }));
    let um = box_solid(&mut s, "U-M", (0.0, 100.0), (0.0, 60.0), (10.0, 17.0), json!({ "horizon": "U-M", "material": "waste" }));
    let u = box_solid(&mut s, "U", (0.0, 100.0), (0.0, 60.0), (17.0, 20.0), json!({ "horizon": "U", "material": "coal" }));
    let below = box_solid(&mut s, "below L", (0.0, 100.0), (0.0, 60.0), (-10.0, 0.0), json!({ "horizon": "below L", "material": "waste" }));
    let rules = json!([
        { "when": { "horizon": "U-M" }, "benched": false, "layer": "9 kept whole" },
        { "when": { "material": "coal" }, "layer": "9 coal benches" },
        { "when": { "horizon": "OB" }, "layer": "9 waste benches" },
        { "when": { "horizon": "below L" }, "layer": "  " },
    ]);
    let record = s.run("solid.cut_by_benches", json!({ "solids": [ob, um, u, below], "layer": "9 benches", "rules": rules })).unwrap();
    println!("{}", record.messages.join("\n"));
    let layer_of = |s: &Session, id: ObjectId| s.project().layer(s.project().object(id).unwrap().layer).unwrap().name.clone();
    let mut placed: Vec<(String, String)> = created(&record).iter().map(|&id| (s.project().object(id).unwrap().name.clone(), layer_of(&s, id))).collect();
    placed.sort();
    let expected = [("OB 20", "9 waste benches"), ("OB 30", "9 waste benches"), ("U 10", "9 coal benches"), ("U-M", "9 kept whole"), ("below L below 0", "9 benches")];
    assert_eq!(placed, expected.map(|(a, b)| (a.to_string(), b.to_string())));
    assert!(record.messages.iter().any(|m| m.contains("Onto layers: '9 benches' 1, '9 coal benches' 1, '9 kept whole' 1, '9 waste benches' 2")), "{:?}", record.messages);
}

/// Levels from RL 0 to 30 against a box from RL -5 to 37: the ground below and above them a piece each, `below 0` and
/// `above 30`, and the three benches between.
#[test]
fn ground_past_the_levels_is_below_or_above() {
    let mut s = session();
    let ob = box_solid(&mut s, "OB", (0.0, 100.0), (0.0, 60.0), (-5.0, 37.0), json!({}));
    let record = s.run("solid.cut_by_benches", json!({ "solids": [ob], "layer": "benches", "rules": [{ "from": 0.0, "to": 30.0, "every": 10.0 }] })).unwrap();
    let made = created(&record);
    assert_eq!(names(&s, &made), ["OB below 0", "OB 0", "OB 10", "OB 20", "OB above 30"]);
    assert_eq!(bench(&s, made[0]), Some(AttrValue::Text("below 0".into())));
    assert!((volume(&s, made[0]) - 30_000.0).abs() < 0.03);
    assert!((volume(&s, made[4]) - 42_000.0).abs() < 0.042);
}

/// Benched first, then cut into blocks: each block named `{strip}_{block}_{horizon}_{bench}` by default, and a block of
/// a solid never benched keeps the name it had.
#[test]
fn blocks_cut_from_benches_carry_the_bench_in_their_name() {
    let mut s = session();
    let ob = box_solid(&mut s, "S3 OB", (0.0, 200.0), (0.0, 60.0), (0.0, 20.0), json!({ "strip": 3.0, "horizon": "OB" }));
    let u = box_solid(&mut s, "S3 U", (0.0, 200.0), (0.0, 60.0), (-3.0, 0.0), json!({ "strip": 3.0, "horizon": "U" }));
    let benches = created(&s.run("solid.cut_by_benches", json!({ "solids": [ob, u], "layer": "benches", "rules": [{ "when": { "horizon": "U" }, "benched": false }, {}] })).unwrap());
    assert_eq!(benches.len(), 3);
    let line = |s: &mut Session, name: &str, points: [(f64, f64); 2]| {
        let points: Vec<[f64; 3]> = points.iter().map(|&(x, y)| [E + x, N + y, 0.0]).collect();
        created(&s.run("string.create", json!({ "name": name, "layer": "block lines", "points": points })).unwrap())[0]
    };
    let mut lines = vec![line(&mut s, "south", [(-20.0, -20.0), (220.0, -20.0)]), line(&mut s, "north", [(-20.0, 80.0), (220.0, 80.0)])];
    for k in 0..3 {
        lines.push(line(&mut s, &format!("across {k}"), [(100.0 * k as f64, -40.0), (100.0 * k as f64, 100.0)]));
    }
    let polygons = created(&s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "block polygons", "name": "B#" })).unwrap());
    let blocks = created(&s.run("solid.cut_by_polygons", json!({ "solids": benches, "polygons": polygons, "layer": "blocks" })).unwrap());
    let mut got = names(&s, &blocks);
    got.sort();
    assert_eq!(got, ["3_B1_OB_0", "3_B1_OB_10", "3_B1_U", "3_B2_OB_0", "3_B2_OB_10", "3_B2_U"]);
}

/// A rule asking for an attribute no solid carries is warned of (likely mistyped), one taking no solid is said, and a
/// rule whose levels run downward is refused, naming it.
#[test]
fn rules_that_take_nothing_are_said_and_bad_levels_refused() {
    let mut s = session();
    let ob = box_solid(&mut s, "OB", (0.0, 100.0), (0.0, 60.0), (0.0, 20.0), json!({ "horizon": "OB" }));
    let rules = json!([{ "when": { "horizn": "OB" }, "benched": false }, { "when": { "horizon": "U" }, "benched": false }, {}]);
    let record = s.run("solid.cut_by_benches", json!({ "solids": [ob], "layer": "benches", "rules": rules })).unwrap();
    let mistyped = record.notes.iter().find(|n| n.code == Some(Code::BenchRuleUnused) && n.severity == Severity::Warning).expect("the mistyped attribute warned of");
    assert!(mistyped.text.contains("bench rule 1") && mistyped.text.contains("horizn"), "{}", mistyped.text);
    let unused = record.notes.iter().find(|n| n.code == Some(Code::BenchRuleUnused) && n.severity == Severity::Info).expect("the rule taking nothing said");
    assert!(unused.text.contains("bench rule 2"), "{}", unused.text);
    assert_eq!(created(&record).len(), 2);

    let refused = s.run("solid.cut_by_benches", json!({ "solids": [ob], "layer": "benches", "rules": [{}, { "from": 100.0, "to": 0.0 }] })).unwrap_err();
    assert!(refused.to_string().contains("bench rule 2"), "{refused}");
    assert!(s.run("solid.cut_by_benches", json!({ "solids": [ob], "layer": "benches", "rules": [] })).is_err());
}
