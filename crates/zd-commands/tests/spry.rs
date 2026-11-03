//! Spry projects in through the registry (decision 0039): block solids named by their node paths with
//! Spry's values and volume, design-layer strings and solids with their attributes, each on a layer
//! named after where it was in Spry.

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Attributes, Body, ChangeKind, Object, ObjectId, Polyline, TriMesh};
use zd_io::spry_fixtures::{FixtureLayer, FixtureNode, FixtureSolid, FixtureTable, SpryFixture, write_spry_fixture};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zd-spry-{tag}-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 100 x 100 x 5 m box from a corner, its triangles wound out: 50,000 m³.
fn box_mesh(at: [f64; 3]) -> TriMesh {
    let [x, y, z] = at;
    let vertices = vec![
        [x, y, z],
        [x + 100.0, y, z],
        [x + 100.0, y + 100.0, z],
        [x, y + 100.0, z],
        [x, y, z + 5.0],
        [x + 100.0, y, z + 5.0],
        [x + 100.0, y + 100.0, z + 5.0],
        [x, y + 100.0, z + 5.0],
    ];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    TriMesh::new(vertices, quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect())
}

/// A box as a node's solid, with Spry's four-cornered faces beside its triangles.
fn node_box(at: [f64; 3]) -> FixtureSolid {
    let mesh = box_mesh(at);
    let faces = vec![vec![0, 3, 2, 1], vec![4, 5, 6, 7], vec![0, 1, 5, 4], vec![1, 2, 6, 5], vec![2, 3, 7, 6], vec![3, 0, 4, 7]];
    FixtureSolid { vertices: mesh.vertices, faces, triangles: mesh.triangles.iter().map(|t| t.map(|i| i as i32)).collect() }
}

/// A box with a cavity: a tetrahedron of 3 m legs inside it, wound inward, 4.5 m³ taken away.
fn box_with_cavity() -> TriMesh {
    let mut mesh = box_mesh([E + 400.0, N, 95.0]);
    let (x, y, z) = (E + 440.0, N + 40.0, 96.0);
    mesh.vertices.extend([[x, y, z], [x + 3.0, y, z], [x, y + 3.0, z], [x, y, z + 3.0]]);
    // Outward would be [8,10,9], [8,9,11], [8,11,10], [9,10,11]; each is reversed to face in.
    mesh.triangles.extend([[8, 9, 10], [8, 11, 9], [8, 10, 11], [9, 11, 10]]);
    mesh
}

fn text(s: &str) -> AttrValue {
    AttrValue::Text(s.into())
}

fn project(dir: &Path) -> PathBuf {
    let path = dir.join("LOM v2.spry");
    let mut toe_attributes = Attributes::new();
    toe_attributes.insert("Name".into(), text("Strip 12 toe"));
    toe_attributes.insert("strip".into(), AttrValue::Number(12.0));
    let toe = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0, N + 100.0, 100.0], [E, N + 100.0, 100.0]], closed: true };
    let topo = TriMesh::new(vec![[E, N, 110.0], [E + 100.0, N, 110.0], [E + 100.0, N + 100.0, 110.0], [E, N + 100.0, 110.0]], vec![[0, 1, 2], [0, 2, 3]]);
    let fixture = SpryFixture {
        tables: vec![FixtureTable {
            name: "Deposit".into(),
            levels: vec!["Pit".into(), "Strip".into(), "Block".into(), "Seam".into()],
            folder: "Coal".into(),
            solid_field: "Solid".into(),
            nodes: vec![
                FixtureNode {
                    path: vec!["PIT A".into(), "12".into(), "3".into(), "4U".into()],
                    solid: Some(node_box([E, N, 90.0])),
                    values: vec![("Volume".into(), AttrValue::Number(50_000.0)), ("Product".into(), text("Coking"))],
                },
                // Spry's volume of this one is a thousandth short of the solid's.
                FixtureNode {
                    path: vec!["PIT A".into(), "12".into(), "4".into(), "4U".into()],
                    solid: Some(node_box([E + 100.0, N, 90.0])),
                    values: vec![("Volume".into(), AttrValue::Number(49_950.0))],
                },
            ],
        }],
        layers: vec![
            FixtureLayer { path: vec!["DESIGN".into(), "TOES".into()], strings: vec![(toe, toe_attributes)], solids: vec![] },
            FixtureLayer { path: vec!["DESIGN".into(), "SOLIDS".into()], strings: vec![], solids: vec![(box_with_cavity(), Attributes::new()), (topo, Attributes::new())] },
        ],
        split_solids_at: None,
    };
    write_spry_fixture(&path, &fixture).unwrap();
    path
}

fn object(s: &Session, id: ObjectId) -> (&Object, String) {
    let object = s.project().object(id).unwrap();
    let layer = s.project().layer(object.layer).unwrap().name.clone();
    (object, layer)
}

#[test]
fn a_project_comes_in_with_block_names_values_strings_solids_and_surfaces() {
    let dir = scratch("all");
    let path = project(&dir);
    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path.to_string_lossy() })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 5, "{:#?}", record.messages);

    let (toe, layer) = object(&s, made[0]);
    assert_eq!((toe.name.as_str(), layer.as_str()), ("TOES 1", r"DESIGN\TOES"));
    assert_eq!(toe.attributes.get("Name"), Some(&text("Strip 12 toe")));
    assert_eq!(toe.attributes.get("strip"), Some(&AttrValue::Number(12.0)));
    let Body::String(line) = &toe.body else { panic!("a string") };
    assert!(line.closed);
    assert_eq!(line.vertices[2], [E + 100.0, N + 100.0, 100.0], "bit for bit");

    // The block: named by its node path, its position on each level and the values beside its Solid
    // field kept, and Spry's volume agreeing with its own to a millionth of a cubic metre.
    let (block, layer) = object(&s, made[1]);
    assert_eq!((block.name.as_str(), layer.as_str()), ("PIT A/12/3/4U", r"Deposit\Coal\Solid"));
    let Body::Solid(b) = &block.body else { panic!("a solid") };
    assert!((b.volume() - 50_000.0).abs() < 1e-6, "{} m3", b.volume());
    assert_eq!(block.attributes.get("spry_volume_m3"), Some(&AttrValue::Number(50_000.0)));
    for (level, position) in [("Pit", "PIT A"), ("Strip", "12"), ("Block", "3"), ("Seam", "4U")] {
        assert_eq!(block.attributes.get(level), Some(&text(position)), "{level}");
    }
    assert_eq!(block.attributes.get("Product"), Some(&text("Coking")));

    let (short, _) = object(&s, made[2]);
    assert_eq!(short.name, "PIT A/12/4/4U");

    // A closed piece inside another is a cavity, faced in: 50,000 - 4.5 m³.
    let (cavity, layer) = object(&s, made[3]);
    assert_eq!((cavity.name.as_str(), layer.as_str()), ("SOLIDS 1", r"DESIGN\SOLIDS"));
    let Body::Solid(c) = &cavity.body else { panic!("a solid") };
    assert!((c.volume() - 49_995.5).abs() < 1e-6, "{} m3", c.volume());
    let (topo, _) = object(&s, made[4]);
    assert!(matches!(&topo.body, Body::Surface(_)), "open, so a surface");

    let lines = record.messages.join("\n");
    assert!(lines.contains("Spry (database version 175): 1 solid fields read, 2 block solids; 2 layers read, 2 solids and 1 strings; 42 triangles"), "{lines}");
    assert!(lines.contains("3 solids imported"), "{lines}");
    assert!(lines.contains("1 surfaces imported"), "{lines}");
    assert!(lines.contains("1 closed patches inside others wound to face in, as cavities"), "{lines}");
    assert!(lines.contains("1 solids whose volume differs from Spry's own (Volume) by more than a millionth, +50.000 m3 in all"), "{lines}");
    assert!(lines.contains(r"PIT A/12/4/4U (Deposit\Coal\Solid): 50000.000 m3 here, 49950.000 m3 in Spry (+0.1001 %)"), "{lines}");

    s.undo();
    assert_eq!(s.project().objects().count(), 0, "one step to undo");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn only_what_is_named_comes_in_and_one_layer_or_one_kind_can_be_asked_for() {
    let dir = scratch("some");
    let path = project(&dir);
    let path = path.to_string_lossy();

    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path, "solid_fields": [r"Deposit\Coal\Solid"], "layer": "from spry" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 2, "the blocks alone");
    for id in &made {
        let (o, layer) = object(&s, *id);
        assert_eq!(layer, "from spry");
        assert_eq!(o.attributes.get("spry_layer"), Some(&text(r"Deposit\Coal\Solid")));
    }

    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path, "layers": [r"DESIGN\TOES", r"DESIGN\SOLIDS"], "meshes": "none" })).unwrap();
    assert_eq!(created(&record).len(), 1, "the string alone");
    assert!(record.messages.iter().any(|m| m == "2 meshes left out, as asked"), "{:?}", record.messages);

    // With another volume field named, there is none to set beside the blocks, and it says so.
    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path, "solid_fields": [r"Deposit\Coal\Solid"], "volume_field": "Tonnes" })).unwrap();
    assert!(record.messages[0].contains(r"no volume field beside Deposit\Coal\Solid"), "{:?}", record.messages);
    assert!(!record.messages.iter().any(|m| m.contains("differs from Spry's own")));

    let mut s = Session::with_builtins();
    let refused = s.run("spry.import", json!({ "path": path, "layers": ["PIT B"] })).unwrap_err().to_string();
    assert!(refused.contains("no solid field or layer named PIT B"), "{refused}");
    let refused = s.run("spry.import", json!({ "path": path, "layers": [r"DESIGN\SOLIDS"], "meshes": "solids" })).unwrap();
    assert!(refused.messages.iter().any(|m| m == "1 meshes refused, nothing made of them:"), "the open topography refused as a solid: {:?}", refused.messages);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Opt-in timing on a real project: `ZD_SPRY_FILE` names a `.spry`, `ZD_SPRY_FIELDS` and
/// `ZD_SPRY_LAYERS` what to read (`;` between them; everything if neither is set). Run in release.
#[test]
#[ignore = "needs ZD_SPRY_FILE; run with --release"]
fn timing_a_real_project_import() {
    let Ok(path) = std::env::var("ZD_SPRY_FILE") else { panic!("set ZD_SPRY_FILE") };
    let list = |name: &str| -> Vec<String> { std::env::var(name).map(|l| l.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default() };
    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path, "solid_fields": list("ZD_SPRY_FIELDS"), "layers": list("ZD_SPRY_LAYERS") })).unwrap();
    println!("{:.0} ms in all, {} objects", record.duration_ms, created(&record).len());
    for line in record.messages.iter().take(80) {
        println!("{line}");
    }
}
