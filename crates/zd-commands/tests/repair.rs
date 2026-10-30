//! The import repair step and the crossing check through the registry (workstream 4, decision
//! 0010): every change in the run record, crossings flagged and saved, and cuts refusing them.

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Attributes, Body, Change, ChangeKind, CrossingCheck, ObjectId, Project, RunId, Solid, Surface, TriMesh};
use zd_geom::triangulate_grid;
use zd_io::ProjectStore;

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zd-repair-{tag}-{}-{}", std::process::id(), RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 3DFACE triangles (local x, y, z about the mine grid origin) as an ASCII DXF.
fn write_faces(path: &Path, faces: &[[[f64; 3]; 3]]) {
    let mut text = String::from("0\nSECTION\n2\nENTITIES\n");
    for face in faces {
        text += "0\n3DFACE\n8\nTOPO\n";
        for (i, p) in [face[0], face[1], face[2], face[2]].iter().enumerate() {
            text += &format!("1{i}\n{}\n2{i}\n{}\n3{i}\n{}\n", E + p[0], N + p[1], p[2]);
        }
    }
    text += "0\nENDSEC\n0\nEOF\n";
    std::fs::write(path, text).unwrap();
}

fn surface(s: &Session, id: ObjectId) -> Surface {
    match &s.project().object(id).unwrap().body {
        Body::Surface(surface) => surface.clone(),
        other => panic!("expected a surface, found {:?}", other.kind()),
    }
}

/// A 20 x 20 m sheet of four 10 m squares at RL 100, written the way a package might: the second
/// square's corners 0.4 mm off, one triangle wound backwards, one written twice (0.3 mm off the
/// first time), and a sliver whose two near corners are 0.2 mm apart.
fn dirty_sheet() -> Vec<[[f64; 3]; 3]> {
    let mut faces = Vec::new();
    for (square, (sx, sy)) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0)].into_iter().enumerate() {
        let jitter = if square == 1 { 0.0004 } else { 0.0 };
        let c = |x: f64, y: f64| [sx + x + jitter, sy + y, 100.0];
        faces.push([c(0.0, 0.0), c(10.0, 0.0), c(10.0, 10.0)]);
        if square == 2 {
            faces.push([c(0.0, 0.0), c(0.0, 10.0), c(10.0, 10.0)]);
        } else {
            faces.push([c(0.0, 0.0), c(10.0, 10.0), c(0.0, 10.0)]);
        }
    }
    faces.push([[0.0, 0.0, 100.0], [10.0003, 0.0, 100.0], [10.0, 10.0, 100.0]]);
    faces.push([[5.0, 5.0, 100.0], [5.0002, 5.0, 100.0], [5.0, 6.0, 100.0]]);
    faces
}

#[test]
fn every_repair_is_in_the_import_log_and_the_check_survives_a_save() {
    let dir = scratch("dirty");
    let path = dir.join("topo.dxf");
    write_faces(&path, &dirty_sheet());
    let mut s = Session::with_builtins();
    let record = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "topo" })).unwrap();
    let repair = &record.messages[1];
    for part in [
        "repair at 1 mm; ",
        "5 vertices welded (largest move 0.4 mm)",
        "triangles dropped: 1 collapsed by the weld, 1 duplicated",
        "2 vertices no triangle uses dropped",
        "1 triangles flipped to agree with their neighbours and face up, or out where closed (1 patches, 0 closed)",
        "8 triangles out",
    ] {
        assert!(repair.contains(part), "'{part}' missing from: {repair}");
    }
    assert!(record.messages[2].starts_with("crossings: none by more than 1 mm (8 triangles"), "{}", record.messages[2]);

    let id = created(&record)[0];
    let topo = surface(&s, id);
    let v = topo.validity();
    assert_eq!((v.boundary_edges, v.consistent_winding, v.non_self_intersecting), (8, true, Some(true)));
    assert!(topo.mesh().triangles.iter().all(|&t| topo.mesh().triangle_cross(t)[2] > 0.0), "every triangle faces up");
    assert_eq!(topo.mesh().vertices[0], [E, N, 100.0], "kept coordinates are exactly as read");

    // The check is saved with the surface.
    s.save(dir.join("pit.zdproj")).unwrap();
    let reopened = Session::open(dir.join("pit.zdproj")).unwrap();
    let back = surface(&reopened, id);
    assert_eq!(back.crossings(), topo.crossings());
    assert_eq!(back.validity().non_self_intersecting, Some(true));

    // A tighter weld leaves the offset square apart: its edges open and nothing is welded there.
    let record = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "topo", "weld_tolerance": 0.0001 })).unwrap();
    assert!(!record.messages[1].contains("largest move 0.4 mm"), "{}", record.messages[1]);
    assert!(surface(&s, created(&record)[0]).validity().boundary_edges > 8);
    assert!(s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "topo", "weld_tolerance": -1.0 })).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_crossing_is_flagged_on_import_and_refused_by_cuts() {
    let dir = scratch("crossing");
    let path = dir.join("design.dxf");
    // A flat triangle with a vertical one passing through it.
    write_faces(&path, &[[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]], [[1.0, 2.0, -5.0], [4.0, 2.0, -5.0], [2.5, 2.0, 5.0]]]);
    let mut s = Session::with_builtins();
    let record = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "design" })).unwrap();
    assert!(
        record.messages[2].starts_with("crossings: 1 pairs of triangles cross by more than 1 mm, the first at 650002.500 7450002.000 RL 0.000; cuts refuse"),
        "{}",
        record.messages[2]
    );
    let design = created(&record)[0];
    let flagged = surface(&s, design);
    assert_eq!(flagged.validity().non_self_intersecting, Some(false));
    assert_eq!(flagged.crossings().unwrap().check, CrossingCheck::Search { tolerance: 0.001 });

    let block = box_mesh([E + 1.0, N + 1.0, -2.0], [E + 3.0, N + 3.0, 2.0]);
    let solid = created(&s.run("solid.from_mesh", json!({ "name": "block", "layer": "solids", "vertices": block.vertices, "triangles": block.triangles })).unwrap())[0];
    let refused = s.run("solid.cut_by_surface", json!({ "solid": solid, "surface": design, "layer": "cut" })).unwrap_err();
    assert_eq!(
        refused.to_string(),
        "the surface 'design' crosses itself: 1 pairs of triangles by more than 1 mm, the first at 650002.500 7450002.000 RL 0.000; fix it at source"
    );

    // Two boxes run into each other: closed and manifold, but crossing.
    let a = box_mesh([E, N, 0.0], [E + 10.0, N + 10.0, 10.0]);
    let b = box_mesh([E + 5.0, N + 5.0, 5.0], [E + 15.0, N + 15.0, 15.0]);
    let mut both = a.clone();
    both.vertices.extend(&b.vertices);
    both.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + 8)));
    let error = s.run("solid.from_mesh", json!({ "name": "two boxes", "layer": "solids", "vertices": both.vertices, "triangles": both.triangles })).unwrap_err();
    assert!(error.to_string().starts_with("the solid 'two boxes' crosses itself: "), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_grid_cannot_cross_itself_and_is_recorded_so() {
    let dir = scratch("grid");
    std::fs::write(dir.join("topo.xyz"), "0 0 1\n10 0 2\n0 10 3\n10 10 4\n").unwrap();
    let mut s = Session::with_builtins();
    let record = s.run("surface.import", json!({ "path": dir.join("topo.xyz").to_string_lossy(), "layer": "topo" })).unwrap();
    assert_eq!(record.messages[1], "a grid has nothing to weld or orient, and cannot cross itself: one elevation per node on a regular plan lattice");
    let topo = surface(&s, created(&record)[0]);
    assert_eq!(topo.crossings().unwrap().check, CrossingCheck::Lattice);
    assert_eq!(topo.validity().non_self_intersecting, Some(true));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cut_checks_inputs_that_were_never_checked() {
    // A project saved before crossing checks existed: its surface and solid carry none.
    let dir = scratch("unchecked");
    let mut project = Project::new();
    let cover = triangulate_grid(&zd_core::Grid::from_fn([E - 10.0, N - 10.0], [10.0, 10.0], 4, 4, |_, _| 5.0));
    let create = |name: &str, body| Change::Create { name: name.into(), layer: "old".into(), attributes: Attributes::new(), body };
    project
        .apply(
            RunId::new(),
            vec![
                create("cover", Body::Surface(Surface::new(cover))),
                create("block", Body::Solid(Solid::new(box_mesh([E, N, 0.0], [E + 10.0, N + 10.0, 10.0])).unwrap())),
            ],
        )
        .unwrap();
    let id = |name: &str| project.objects().find(|o| o.name == name).unwrap().id;
    let (cover, block) = (id("cover"), id("block"));
    ProjectStore::create(dir.join("old.zdproj")).unwrap().save(&project, "[]").unwrap();

    let mut s = Session::open(dir.join("old.zdproj")).unwrap();
    let record = s.run("solid.cut_by_surface", json!({ "solid": block, "surface": cover, "layer": "cut" })).unwrap();
    assert!(record.messages[0].starts_with("solid 'block' crossings: none by more than 1 mm (12 triangles"), "{:?}", record.messages);
    assert!(record.messages[1].starts_with("surface 'cover' crossings: none by more than 1 mm (18 triangles"), "{:?}", record.messages);
    // Half the 1,000 mÂ³ box lies below RL 5.
    assert!(record.messages[3].contains("below 500.000 m3"), "{:?}", record.messages);
    // Both parts are checked by the solid rules before they are saved, and keep the check.
    assert!(record.messages.iter().any(|m| m.starts_with("2 solids made, checked by the solid rules: none crosses itself")), "{:?}", record.messages);
    for id in created(&record) {
        let Body::Solid(part) = &s.project().object(id).unwrap().body else { panic!("a cut makes solids") };
        assert!(matches!(part.crossings().map(|c| c.check), Some(CrossingCheck::SolidSearch { .. })));
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cut_takes_a_thin_solid_and_searches_an_old_check_again() {
    // A wedge rising from a knife edge to 0.5 mm, saved with a check by the surface rules (as before
    // decision 0014), which calls its top and bottom a fold. The cut searches it again by the
    // solid rules, finds it thin and cuts it at RL 0.25 mm: 0.01875 m³ below, 0.00625 m³ above.
    let dir = scratch("thin");
    let v = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 0.0005], [0.0, 10.0, 0.0], [10.0, 10.0, 0.0], [10.0, 10.0, 0.0005]];
    let wedge = TriMesh::new(v.iter().map(|p| [E + p[0], N + p[1], p[2]]).collect(), vec![[0, 3, 1], [1, 3, 4], [0, 2, 3], [2, 5, 3], [1, 4, 5], [1, 5, 2], [0, 1, 2], [3, 5, 4]]);
    let old_check = zd_geom::find_crossings(&wedge, 0.001);
    assert!(old_check.pairs > 0);
    let cover = triangulate_grid(&zd_core::Grid::from_fn([E - 10.0, N - 10.0], [10.0, 10.0], 4, 4, |_, _| 0.00025));
    let mut project = Project::new();
    let create = |name: &str, body| Change::Create { name: name.into(), layer: "old".into(), attributes: Attributes::new(), body };
    project
        .apply(
            RunId::new(),
            vec![create("cover", Body::Surface(Surface::new(cover))), create("wedge", Body::Solid(Solid::new(wedge).unwrap().with_crossings(old_check)))],
        )
        .unwrap();
    let id = |name: &str| project.objects().find(|o| o.name == name).unwrap().id;
    let (cover, wedge) = (id("cover"), id("wedge"));
    ProjectStore::create(dir.join("old.zdproj")).unwrap().save(&project, "[]").unwrap();

    let mut s = Session::open(dir.join("old.zdproj")).unwrap();
    let record = s.run("solid.cut_by_surface", json!({ "solid": wedge, "surface": cover, "layer": "cut" })).unwrap();
    assert!(record.messages[0].starts_with("solid 'wedge' crossings: none by more than 1 mm; ") && record.messages[0].contains("thin pairs"), "{:?}", record.messages);
    let volumes: Vec<f64> = created(&record)
        .into_iter()
        .map(|id| match &s.project().object(id).unwrap().body {
            Body::Solid(solid) => solid.volume(),
            other => panic!("expected a solid, found {:?}", other.kind()),
        })
        .collect();
    assert_eq!(volumes.len(), 2, "{:?}", record.messages);
    assert!((volumes[0] - 0.01875).abs() < 1e-9 && (volumes[1] - 0.00625).abs() < 1e-9, "{volumes:?}");
    std::fs::remove_dir_all(&dir).unwrap();
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
