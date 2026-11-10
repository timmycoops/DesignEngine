//! The general cut (decision 0016): solids cut by a surface, a grid or a level, keeping the parts
//! below, above or both, one solid or many in a run; each part keeps its solid's attributes.

mod common;

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
    assert!(record.messages.iter().any(|m| m.starts_with("volume kept by every cut: below + above ")), "{:?}", record.messages);

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
fn the_solids_a_cut_makes_read_back_from_dxf_as_they_were() {
    // Twelve cubes cut by a grid: the cubes and every part.
    let (mut s, cubes, bow, dir) = pit(12);
    s.run("solid.cut_by_surface", json!({ "solids": cubes, "surface": bow, "layer": "cut" })).unwrap();
    assert!(common::assert_solids_round_trip(&s) >= 24);
    std::fs::remove_dir_all(&dir).ok();
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

/// Two 100 m squares of level surface at RL 105 that meet only at one corner, which a cut can't close
/// round, a cube over that corner and a cube well clear of it.
fn cubes_and_a_bow_tie() -> (Session, ObjectId, ObjectId, ObjectId) {
    let at = |x: f64, y: f64| [E + x, N + y, 105.0];
    let vertices = vec![at(0.0, 0.0), at(100.0, 0.0), at(100.0, 100.0), at(0.0, 100.0), at(200.0, 100.0), at(200.0, 200.0), at(100.0, 200.0)];
    let tie = TriMesh::new(vertices, vec![[0, 1, 2], [0, 2, 3], [2, 4, 5], [2, 5, 6]]);
    let mut project = Project::new();
    let solid = |name: &str, x: f64, y: f64| Change::Create {
        name: name.into(),
        layer: "blocks".into(),
        attributes: Attributes::new(),
        body: Body::Solid(Solid::new(box_mesh([x, y, 100.0], [x + 10.0, y + 10.0, 110.0])).unwrap()),
    };
    let surface = Change::Create { name: "tie".into(), layer: "design".into(), attributes: Attributes::new(), body: Body::Surface(zd_core::Surface::new(tie)) };
    project.apply(RunId::new(), vec![solid("at the corner", E + 95.0, N + 95.0), solid("clear", E + 20.0, N + 20.0), surface]).unwrap();
    let id = |name: &str| project.objects().find(|o| o.name == name).unwrap().id;
    let (corner, clear, tie) = (id("at the corner"), id("clear"), id("tie"));
    let dir = std::env::temp_dir().join(format!("zd-cut-tie-{}-{}", std::process::id(), RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    ProjectStore::create(dir.join("pit.zdproj")).unwrap().save(&project, "[]").unwrap();
    (Session::open(dir.join("pit.zdproj")).unwrap(), corner, clear, tie)
}

#[test]
fn a_solid_that_cant_be_cut_is_named_and_the_rest_go_on() {
    let (mut s, corner, clear, tie) = cubes_and_a_bow_tie();
    let record = s.run("solid.cut_by_surface", json!({ "solids": [corner, clear], "surface": tie, "layer": "cut" })).unwrap();
    let parts = created(&record);
    assert_eq!(parts.len(), 2, "both parts of the clear cube: {:?}", record.messages);
    let volumes: Vec<f64> = parts.iter().map(|&id| solid_volume(&s, id)).collect();
    assert!((volumes[0] - 500.0).abs() < 1e-6 && (volumes[1] - 500.0).abs() < 1e-6, "{volumes:?}");
    let uncut = record.messages.iter().find(|m| m.starts_with("1 solids left uncut")).unwrap_or_else(|| panic!("{:?}", record.messages));
    assert!(uncut.contains("'at the corner'") && uncut.contains("touches itself"), "{uncut}");

    // On its own a solid that can't be cut is still refused: there's nothing else to go on with.
    let refused = s.run("solid.cut_by_surface", json!({ "solid": corner, "surface": tie, "layer": "cut" })).unwrap_err();
    assert!(refused.to_string().contains("touches itself"), "{refused}");
}

/// A 5 m Vulcan grid (plain layout) over the cubes of `pit(1)`, level at RL 104, flagged off over the
/// eastern half where it holds the same level.
fn held_grid() -> Vec<u8> {
    let (cols, rows) = (9usize, 7usize);
    let (x0, y0) = (E - 10.0, N - 10.0);
    let mut out = b"SYNTHETIC HELD GRID             ".to_vec();
    for v in [x0, x0 + (cols - 1) as f64 * 5.0, y0, y0 + (rows - 1) as f64 * 5.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(cols * rows) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&5.0f32.to_be_bytes());
    out.extend_from_slice(&5.0f32.to_be_bytes());
    out.extend_from_slice(&(cols as i32).to_be_bytes());
    out.extend_from_slice(&(rows as i32).to_be_bytes());
    for node in 0..cols * rows {
        out.extend_from_slice(&104.0f32.to_be_bytes());
        out.extend_from_slice(&i32::from(node % cols < 4).to_be_bytes());
    }
    out
}

/// The grid is flagged off across the cube, which would leave it uncovered; held, it cuts it exactly.
#[test]
fn a_grid_cuts_across_nodes_flagged_off() {
    let (mut s, cubes, _, dir) = pit(1);
    let path = dir.join("pitlower.sfg");
    std::fs::write(&path, held_grid()).unwrap();
    let grid = created(&s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "design" })).unwrap())[0];
    let record = s.run("solid.cut_by_surface", json!({ "solid": cubes[0], "surface": grid, "layer": "cut" })).unwrap();
    assert!(!record.messages.iter().any(|m| m.contains("does not cover")), "{:?}", record.messages);
    let parts = created(&record);
    assert_eq!(parts.len(), 2);
    // A 10 m cube from RL 100 to 110, cut level at RL 104.
    assert!((solid_volume(&s, parts[0]) - 400.0).abs() < 1e-6, "{}", solid_volume(&s, parts[0]));
    assert!((solid_volume(&s, parts[1]) - 600.0).abs() < 1e-6, "{}", solid_volume(&s, parts[1]));
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

/// A 10 m cube under a design of eight triangles on a 3 x 3 lattice, flat at RL 105 but for its middle corner, 0.5 m
/// higher and `off` m east of the cube's east wall.
fn cube_and_a_design_by_its_wall(off: f64) -> (Session, ObjectId, ObjectId) {
    let mut s = Session::with_builtins();
    let cube = box_mesh([E, N, 100.0], [E + 10.0, N + 10.0, 110.0]);
    let cube = created(&s.run("solid.from_mesh", json!({ "name": "cube", "layer": "solids", "vertices": cube.vertices, "triangles": cube.triangles })).unwrap())[0];
    let (xs, ys) = ([E - 2.0, E + 10.0 + off, E + 14.0], [N - 2.0, N + 5.0, N + 12.0]);
    let vertices: Vec<[f64; 3]> = (0..9).map(|k| [xs[k % 3], ys[k / 3], if k == 4 { 105.5 } else { 105.0 }]).collect();
    let triangles: Vec<[u32; 3]> = [0u32, 1, 3, 4].iter().flat_map(|&a| [[a, a + 1, a + 4], [a, a + 4, a + 3]]).collect();
    let design = created(&s.run("surface.from_mesh", json!({ "name": "design", "layer": "design", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    (s, cube, design)
}

#[test]
fn a_cut_that_makes_corners_a_nanometre_apart_joins_them_and_says_so() {
    // The design's middle corner sits half a nanometre inside the cube's east wall, so the wall cuts each of the
    // design's edges into that corner within a nanometre of it: the boolean leaves corners a nanometre apart. With
    // the corner on the wall exactly it leaves none, and the two cuts agree.
    let (mut on, cube, design) = cube_and_a_design_by_its_wall(0.0);
    let record = on.run("solid.cut_by_surface", json!({ "solid": cube, "surface": design, "layer": "cut" })).unwrap();
    assert!(record.messages.iter().all(|m| !m.contains("corners under")), "{:?}", record.messages);
    let exact: Vec<f64> = created(&record).iter().map(|&id| solid_volume(&on, id)).collect();

    let (mut s, cube, design) = cube_and_a_design_by_its_wall(-5e-10);
    let record = s.run("solid.cut_by_surface", json!({ "solid": cube, "surface": design, "layer": "cut" })).unwrap();
    let said = record.messages.iter().find(|m| m.contains("corners under 0.001 mm apart joined")).unwrap_or_else(|| panic!("no corners joined: {:?}", record.messages));
    assert!(said.ends_with("leftovers of the cut, no coordinate kept moved"), "{said}");
    let parts = created(&record);
    assert_eq!(parts.len(), 2);
    for &id in &parts {
        let Body::Solid(part) = &s.project().object(id).unwrap().body else { panic!("a solid") };
        let mesh = part.mesh();
        let shortest = mesh.triangles.iter().flat_map(|&[a, b, c]| [(a, b), (b, c), (c, a)]).map(|(u, v)| {
            let (p, q) = (mesh.vertices[u as usize], mesh.vertices[v as usize]);
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        });
        let shortest = shortest.fold(f64::INFINITY, f64::min);
        assert!(shortest >= 1e-6, "an edge of {shortest} m is left");
    }
    // Volume tolerance: 1e-6 m3 on 1,000 m3, for below + above against the cube and for each part against the cut with
    // the corner on the wall. Joining corners half a nanometre apart over a few tens of square metres of face moves
    // about 1e-8 m3, as does moving the design's corner that far.
    let volumes: Vec<f64> = parts.iter().map(|&id| solid_volume(&s, id)).collect();
    assert!((volumes[0] + volumes[1] - 1000.0).abs() < 1e-6, "{volumes:?}");
    for (found, exact) in volumes.iter().zip(&exact) {
        assert!((found - exact).abs() < 1e-6, "{found} against {exact}");
    }
    // The cube and its two parts go out as DXF and come back the same solids.
    assert_eq!(common::assert_solids_round_trip(&s), 3);
}
