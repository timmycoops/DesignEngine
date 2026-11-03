//! Closed meshes an import couldn't make solids (decision 0040): flagged `not_a_solid` with why, and
//! made solids by `solid.repair`, each fixed one replacing its surface in one step to undo.

use std::path::PathBuf;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId, TriMesh};
use zd_io::spry_fixtures::{FixtureLayer, FixtureNode, FixtureSolid, FixtureTable, SpryFixture, write_spry_fixture};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zd-repair-{tag}-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A box from a corner, its triangles wound out.
fn cuboid(at: [f64; 3], size: [f64; 3]) -> TriMesh {
    let ([x, y, z], [a, b, c]) = (at, size);
    let vertices = vec![[x, y, z], [x + a, y, z], [x + a, y + b, z], [x, y + b, z], [x, y, z + c], [x + a, y, z + c], [x + a, y + b, z + c], [x, y + b, z + c]];
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    TriMesh::new(vertices, quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect())
}

/// Two 10 m cubes overlapping by 5 m each way, as one mesh crossing itself: 2,000 m³ as read,
/// 1,000 + 1,000 - 125 = 1,875 m³ enclosed.
fn overlapping_cubes() -> TriMesh {
    let mut mesh = cuboid([E, N, 100.0], [10.0; 3]);
    let other = cuboid([E + 5.0, N + 5.0, 105.0], [10.0; 3]);
    mesh.triangles.extend(other.triangles.iter().map(|t| t.map(|i| i + 8)));
    mesh.vertices.extend(other.vertices);
    mesh
}

/// A flat 100 m square, open: no repair closes it within the 10 % fill limit.
fn sheet() -> TriMesh {
    TriMesh::new(vec![[E, N, 110.0], [E + 100.0, N, 110.0], [E + 100.0, N + 100.0, 110.0], [E, N + 100.0, 110.0]], vec![[0, 1, 2], [0, 2, 3]])
}

fn node_solid(mesh: TriMesh) -> FixtureSolid {
    FixtureSolid { vertices: mesh.vertices, faces: vec![], triangles: mesh.triangles.iter().map(|t| t.map(|i| i as i32)).collect() }
}

fn text(s: &str) -> AttrValue {
    AttrValue::Text(s.into())
}

fn volume(s: &Session, id: ObjectId) -> f64 {
    match &s.project().object(id).unwrap().body {
        Body::Solid(solid) => solid.volume(),
        other => panic!("not a solid: {:?}", other.kind()),
    }
}

#[test]
fn an_import_flags_a_block_that_crosses_itself_and_repair_replaces_it_with_the_solid_it_encloses() {
    let dir = scratch("spry");
    let path = dir.join("plan.spry");
    let fixture = SpryFixture {
        tables: vec![FixtureTable {
            name: "Deposit".into(),
            levels: vec!["Pit".into(), "Strip".into(), "Block".into()],
            folder: "Coal".into(),
            solid_field: "Solid".into(),
            nodes: vec![
                FixtureNode { path: vec!["PIT A".into(), "12".into(), "3".into()], solid: Some(node_solid(cuboid([E + 50.0, N, 100.0], [10.0; 3]))), values: vec![("Volume".into(), AttrValue::Number(1_000.0))] },
                FixtureNode { path: vec!["PIT A".into(), "12".into(), "4".into()], solid: Some(node_solid(overlapping_cubes())), values: vec![("Volume".into(), AttrValue::Number(1_875.0))] },
            ],
        }],
        layers: vec![FixtureLayer { path: vec!["TOPO".into()], strings: vec![], solids: vec![(sheet(), Default::default())] }],
        split_solids_at: None,
    };
    write_spry_fixture(&path, &fixture).unwrap();

    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path.to_string_lossy() })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 3, "{:#?}", record.messages);
    let [clean, crossed, topo] = [made[0], made[1], made[2]];
    assert!((volume(&s, clean) - 1_000.0).abs() < 1e-6);
    assert!(s.project().object(clean).unwrap().attributes.get("not_a_solid").is_none(), "a solid isn't flagged");

    // Closed but crossing itself: kept as a surface, flagged with why.
    let o = s.project().object(crossed).unwrap();
    assert!(matches!(o.body, Body::Surface(_)));
    let Some(AttrValue::Text(why)) = o.attributes.get("not_a_solid") else { panic!("flagged: {:?}", o.attributes) };
    assert!(why.contains("crosses itself"), "{why}");
    // Open, so a surface as asked for, and not flagged.
    assert!(s.project().object(topo).unwrap().attributes.get("not_a_solid").is_none(), "an open surface isn't a failed solid");

    let record = s.run("solid.repair", json!({ "surfaces": [crossed] })).unwrap();
    let fixed = created(&record);
    assert_eq!(fixed.len(), 1, "{:#?}", record.messages);
    assert!(s.project().object(crossed).is_none(), "the surface is replaced");
    let solid = s.project().object(fixed[0]).unwrap();
    assert_eq!(solid.name, "PIT A/12/4");
    assert_eq!(s.project().layer(solid.layer).unwrap().name, r"Deposit\Coal\Solid");
    for (key, value) in [("Pit", "PIT A"), ("Strip", "12"), ("Block", "4")] {
        assert_eq!(solid.attributes.get(key), Some(&text(value)), "{key} kept");
    }
    assert_eq!(solid.attributes.get("spry_volume_m3"), Some(&AttrValue::Number(1_875.0)));
    assert!(solid.attributes.get("not_a_solid").is_none(), "no longer flagged");
    let Some(AttrValue::Number(change)) = solid.attributes.get("repair_volume_change_m3") else { panic!("{:?}", solid.attributes) };
    assert!((change + 125.0).abs() < 1e-6, "{change} m3: the overlap counted once");
    assert!((volume(&s, fixed[0]) - 1_875.0).abs() < 1e-6, "{} m3", volume(&s, fixed[0]));
    let lines = record.messages.join("\n");
    assert!(lines.contains("1 of 1 surfaces made solids by repair, each replacing its surface; -125.000 m3 changed by the union where it ran"), "{lines}");
    assert!(!lines.contains("differ from the source's own volume"), "1,875 m3 agrees with Spry's: {lines}");

    s.undo();
    let back = s.project().object(crossed).expect("undo brings the surface back");
    assert!(matches!(back.body, Body::Surface(_)) && back.attributes.contains_key("not_a_solid"));
    assert!(s.project().object(fixed[0]).is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Opt-in on a real project: `ZD_SPRY_FILE` names a `.spry`, read whole; the closed meshes it flags
/// are then repaired, with timings and what was left. Run in release.
#[test]
#[ignore = "needs ZD_SPRY_FILE; run with --release"]
fn repairing_what_a_real_project_flags() {
    let Ok(path) = std::env::var("ZD_SPRY_FILE") else { panic!("set ZD_SPRY_FILE") };
    let mut s = Session::with_builtins();
    let record = s.run("spry.import", json!({ "path": path })).unwrap();
    let flagged: Vec<ObjectId> = created(&record).into_iter().filter(|&id| s.project().object(id).unwrap().attributes.contains_key("not_a_solid")).collect();
    println!("import: {:.0} ms, {} objects, {} flagged not_a_solid", record.duration_ms, created(&record).len(), flagged.len());
    let triangles: usize = flagged.iter().map(|&id| match &s.project().object(id).unwrap().body {
        Body::Surface(surface) => surface.mesh().triangle_count(),
        _ => 0,
    }).sum();
    let record = s.run("solid.repair", json!({ "surfaces": flagged })).unwrap();
    println!("repair: {:.0} ms, {triangles} triangles in, {} solids out", record.duration_ms, created(&record).len());
    for line in &record.messages {
        println!("{line}");
    }
}

#[test]
fn what_repair_cannot_fix_stays_a_surface_saying_why_now() {
    let mut s = Session::with_builtins();
    let mesh = |name: &str, m: TriMesh| json!({ "name": name, "layer": "blocks", "vertices": m.vertices, "triangles": m.triangles });
    let crossed = created(&s.run("surface.from_mesh", mesh("B4", overlapping_cubes())).unwrap())[0];
    let open = created(&s.run("surface.from_mesh", mesh("sheet", sheet())).unwrap())[0];

    let record = s.run("solid.repair", json!({ "surfaces": [crossed, open, crossed] })).unwrap();
    let fixed = created(&record);
    assert_eq!(fixed.len(), 1, "a surface named twice is taken once: {:#?}", record.messages);
    assert!((volume(&s, fixed[0]) - 1_875.0).abs() < 1e-6);
    let sheet = s.project().object(open).expect("still there");
    assert!(matches!(sheet.body, Body::Surface(_)));
    let Some(AttrValue::Text(why)) = sheet.attributes.get("not_a_solid") else { panic!("{:?}", sheet.attributes) };
    assert!(why.contains("over the 10 % limit"), "{why}");
    let lines = record.messages.join("\n");
    assert!(lines.contains("1 still not solids, kept as surfaces with why on each:"), "{lines}");
    assert!(lines.contains("sheet: "), "{lines}");

    // Nothing to fix refuses the run; a solid isn't a surface to repair.
    let refused = s.run("solid.repair", json!({ "surfaces": [open] })).unwrap_err().to_string();
    assert!(refused.contains("none of the 1 surfaces could be made a solid by repair"), "{refused}");
    let refused = s.run("solid.repair", json!({ "surfaces": [fixed[0]] })).unwrap_err().to_string();
    assert!(refused.contains("is a Solid, not a Surface"), "{refused}");
    assert!(s.run("solid.repair", json!({ "surfaces": [] })).unwrap_err().to_string().contains("no surfaces given"));
}
