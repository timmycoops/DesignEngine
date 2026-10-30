//! The general cut (decision 0016): solids cut by a surface, a grid or a level, keeping the parts
//! below, above or both, one solid or many in a run; each part keeps its solid's attributes.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Attributes, Body, Change, ChangeKind, Grid, ObjectId, Project, RunId, Solid, TriMesh};
use zd_io::ProjectStore;

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

/// A box wound counter-clockwise about its outward normals.
fn box_mesh(min: [f64; 3], max: [f64; 3]) -> TriMesh {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    TriMesh::new(
        vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
    )
}

/// A session on a saved project holding `count` 10 m cubes in a row (RL 100 to 110), each with a
/// `block` attribute, and a flat 5 m grid at RL 104 under all of them.
fn pit(count: usize) -> (Session, Vec<ObjectId>, ObjectId, PathBuf) {
    let dir = std::env::temp_dir().join(format!("zd-cut-{count}-{}-{}", std::process::id(), RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut project = Project::new();
    let mut changes: Vec<Change> = (0..count)
        .map(|k| {
            let x = E + 20.0 * k as f64;
            let mut attributes = Attributes::new();
            attributes.insert("block".into(), AttrValue::Text(format!("B{k}")));
            Change::Create {
                name: format!("cube {k}"),
                layer: "blocks".into(),
                attributes,
                body: Body::Solid(Solid::new(box_mesh([x, N, 100.0], [x + 10.0, N + 10.0, 110.0])).unwrap()),
            }
        })
        .collect();
    let grid = Grid::from_fn([E - 10.0, N - 10.0], [5.0, 5.0], 8 + 4 * count, 7, |_, _| 104.0);
    changes.push(Change::Create { name: "bow".into(), layer: "grids".into(), attributes: Attributes::new(), body: Body::Grid(Arc::new(grid)) });
    project.apply(RunId::new(), changes).unwrap();
    let cubes: Vec<ObjectId> = (0..count).map(|k| project.objects().find(|o| o.name == format!("cube {k}")).unwrap().id).collect();
    let bow = project.objects().find(|o| o.name == "bow").unwrap().id;
    ProjectStore::create(dir.join("pit.zdproj")).unwrap().save(&project, "[]").unwrap();
    (Session::open(dir.join("pit.zdproj")).unwrap(), cubes, bow, dir)
}

fn solid_volume(s: &Session, id: ObjectId) -> f64 {
    match &s.project().object(id).unwrap().body {
        Body::Solid(solid) => solid.volume(),
        other => panic!("expected a solid, found {:?}", other.kind()),
    }
}

#[test]
fn a_cube_cut_at_a_level_keeps_its_attributes_and_the_side_each_part_lies_on() {
    let (mut s, cubes, _, dir) = pit(1);
    let record = s.run("solid.cut_by_surface", json!({ "solid": cubes[0], "rl": 105.0, "layer": "cut" })).unwrap();
    let parts = created(&record);
    assert_eq!(parts.len(), 2, "{:?}", record.messages);
    // Half the 1,000 m³ cube on each side of RL 105, to Manifold's precision.
    for (id, side) in parts.iter().zip(["below", "above"]) {
        let object = s.project().object(*id).unwrap();
        assert_eq!(object.name, format!("cube 0 {side} RL 105.000"));
        assert_eq!(object.attributes["block"], AttrValue::Text("B0".into()), "the solid's own attributes carry over");
        assert_eq!(object.attributes["cut_side"], AttrValue::Text(side.into()));
        assert_eq!(object.attributes["cut_by"], AttrValue::Text("RL 105.000".into()));
        assert!((solid_volume(&s, *id) - 500.0).abs() < 1e-6, "{side}: {}", solid_volume(&s, *id));
    }
    assert!(record.messages.iter().any(|m| m.starts_with("volume kept by every cut: below + above within 0.0000")), "{:?}", record.messages);

    // Keeping one side makes one part; a level above the cube leaves nothing above it to keep.
    let below = s.run("solid.cut_by_surface", json!({ "solid": cubes[0], "rl": 102.5, "keep": "below", "layer": "cut" })).unwrap();
    let kept = created(&below);
    assert_eq!(kept.len(), 1);
    assert!((solid_volume(&s, kept[0]) - 250.0).abs() < 1e-6);
    let above = s.run("solid.cut_by_surface", json!({ "solid": cubes[0], "rl": 120.0, "keep": "above", "layer": "cut" })).unwrap();
    assert!(created(&above).is_empty());
    assert!(above.messages.iter().any(|m| m == "nothing of the solid lies above RL 120.000"), "{:?}", above.messages);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn many_solids_are_cut_by_a_grid_in_one_run_and_summed_up() {
    // Twelve cubes, more than the run record gives a line each, cut by a grid at RL 104.
    let (mut s, cubes, bow, dir) = pit(12);
    let record = s.run("solid.cut_by_surface", json!({ "solids": cubes, "surface": bow, "layer": "cut" })).unwrap();
    let parts = created(&record);
    assert_eq!(parts.len(), 24);
    let summary = record.messages.iter().find(|m| m.starts_with("12 solids cut by bow in ")).unwrap_or_else(|| panic!("{:?}", record.messages));
    assert!(summary.contains("keeping both sides: 12 parts below (4800.000 m3) and 12 above (7200.000 m3) kept; 0 solids wholly below and 0 wholly above"), "{summary}");
    assert!(record.messages.iter().any(|m| m == "24 parts checked by the solid rules before saving: none crosses itself"), "{:?}", record.messages);
    let total: f64 = parts.iter().map(|&id| solid_volume(&s, id)).sum();
    assert!((total - 12_000.0).abs() < 1e-6, "{total}");
    let first = s.project().object(parts[0]).unwrap();
    assert_eq!((first.name.as_str(), &first.attributes["block"]), ("cube 0 below bow", &AttrValue::Text("B0".into())));
    // The inputs are left as they were.
    assert!(cubes.iter().all(|&id| (solid_volume(&s, id) - 1000.0).abs() < 1e-9));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cut_needs_one_cutter_and_names_only_one_solids_parts() {
    let (mut s, cubes, bow, dir) = pit(2);
    let refuse = |s: &mut Session, params: serde_json::Value| s.run("solid.cut_by_surface", params).unwrap_err().to_string();
    assert!(refuse(&mut s, json!({ "rl": 105.0, "layer": "cut" })).contains("give the solid (or solids) to cut"));
    assert!(refuse(&mut s, json!({ "solid": cubes[0], "layer": "cut" })).contains("or an rl, not both"));
    assert!(refuse(&mut s, json!({ "solid": cubes[0], "surface": bow, "rl": 105.0, "layer": "cut" })).contains("or an rl, not both"));
    assert!(refuse(&mut s, json!({ "solid": cubes[0], "solids": [cubes[0]], "rl": 105.0, "layer": "cut" })).contains("is given twice"));
    assert!(refuse(&mut s, json!({ "solids": cubes, "rl": 105.0, "below_name": "lower", "layer": "cut" })).contains("name one solid's parts"));
    assert!(refuse(&mut s, json!({ "solid": cubes[0], "rl": 105.0, "keep": "middle", "layer": "cut" })).contains("unknown variant"));
    assert_eq!(s.project().objects().count(), 3, "nothing made by a refused cut");
    std::fs::remove_dir_all(&dir).unwrap();
}
