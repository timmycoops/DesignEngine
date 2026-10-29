//! Vulcan triangulations (`.00t`) through the registry (decision 0011): surfaces and solids, each
//! through the repair step and the crossing check, with opt-in checks against real files and their
//! Deswik exports.

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId, RunId, TriMesh};
use zd_io::{plain_triangulation_bytes, read_dxf, read_vulcan_triangulation};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zd-00t-{tag}-{}-{}", std::process::id(), RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, mesh: &TriMesh) {
    std::fs::write(path, plain_triangulation_bytes("Created External", mesh)).unwrap();
}

/// A box wound outward.
fn block(min: [f64; 3], size: [f64; 3]) -> TriMesh {
    let v = (0..8).map(|i| [min[0] + (i & 1) as f64 * size[0], min[1] + (i >> 1 & 1) as f64 * size[1], min[2] + (i >> 2) as f64 * size[2]]).collect();
    let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    TriMesh::new(v, t)
}

#[test]
fn a_vulcan_surface_imports_through_the_repair_step() {
    let dir = scratch("surface");
    // A 3 x 3 node sheet at 10 m, every other triangle wound backwards.
    let v: Vec<[f64; 3]> = (0..9).map(|i| [E + (i % 3) as f64 * 10.0, N + (i / 3) as f64 * 10.0, 200.0 + (i % 3) as f64]).collect();
    let mut t = Vec::new();
    for r in 0..2u32 {
        for c in 0..2u32 {
            let a = r * 3 + c;
            t.push([a, a + 1, a + 4]);
            t.push([a, a + 3, a + 4]); // clockwise: to be flipped
        }
    }
    write(&dir.join("topo.00t"), &TriMesh::new(v.clone(), t));
    let mut s = Session::with_builtins();
    let record = s.run("surface.import", json!({ "path": dir.join("topo.00t").to_string_lossy(), "layer": "topo" })).unwrap();
    assert_eq!(record.messages[0], "Vulcan triangulation: 9 vertices, 8 triangles");
    assert!(record.messages[1].contains("4 triangles flipped"), "{}", record.messages[1]);
    assert!(record.messages[2].starts_with("crossings: none"), "{}", record.messages[2]);
    let Body::Surface(topo) = &s.project().object(created(&record)[0]).unwrap().body else { panic!("not a surface") };
    assert_eq!(topo.mesh().vertices, v, "every coordinate exactly as stored");
    assert!(topo.mesh().triangles.iter().all(|&t| topo.mesh().triangle_cross(t)[2] > 0.0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_closed_vulcan_triangulation_imports_as_a_solid() {
    let dir = scratch("solid");
    write(&dir.join("coal.00t"), &block([E + 0.25, N + 0.75, 250.0], [10.0, 20.0, 5.0]));
    let mut s = Session::with_builtins();
    let record = s.run("solid.import", json!({ "path": dir.join("coal.00t").to_string_lossy(), "layer": "coal" })).unwrap();
    assert_eq!(record.messages.last().unwrap(), "volume 1000.000 m3");
    let object = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(object.name, "coal");
    let Body::Solid(solid) = &object.body else { panic!("not a solid") };
    assert_eq!(solid.validity().non_self_intersecting, Some(true));

    // Open: one face missing.
    let mut open = block([E, N, 250.0], [10.0, 10.0, 10.0]);
    open.triangles.truncate(10);
    write(&dir.join("open.00t"), &open);
    let error = s.run("solid.import", json!({ "path": dir.join("open.00t").to_string_lossy(), "layer": "coal" })).unwrap_err();
    assert!(error.to_string().starts_with("'open' is not a closed solid after repair: 4 boundary"), "{error}");
    assert!(error.to_string().contains("the first open edge is at 650010.000 7450005.000 RL 250.000"), "{error}");
    assert!(error.to_string().contains("if it is a surface, import it with surface.import, and if it is a solid, it has gaps to fix at source"), "{error}");

    // Two blocks run into each other: closed, but crossing.
    let (a, b) = (block([E, N, 250.0], [10.0, 10.0, 10.0]), block([E + 5.0, N + 5.0, 255.0], [10.0, 10.0, 10.0]));
    let mut both = a.clone();
    both.vertices.extend(&b.vertices);
    both.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + 8)));
    write(&dir.join("both.00t"), &both);
    let error = s.run("solid.import", json!({ "path": dir.join("both.00t").to_string_lossy(), "layer": "coal" })).unwrap_err();
    assert!(error.to_string().starts_with("the solid 'both' crosses itself"), "{error}");
    assert!(s.run("solid.import", json!({ "path": dir.join("missing.00t").to_string_lossy(), "layer": "coal" })).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn closed_pieces_meeting_along_an_edge_import_as_a_solid_each() {
    let dir = scratch("pieces");
    // Two 10 m cubes meeting along one vertical edge, written with their own vertices: the weld
    // joins the edge's two corners, which leaves it with four faces.
    let (a, b) = (block([E, N, 250.0], [10.0, 10.0, 10.0]), block([E + 10.0, N + 10.0, 250.0], [10.0, 10.0, 10.0]));
    let mut pair = a.clone();
    pair.vertices.extend(&b.vertices);
    pair.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + 8)));
    write(&dir.join("pair.00t"), &pair);
    let mut s = Session::with_builtins();
    let record = s.run("solid.import", json!({ "path": dir.join("pair.00t").to_string_lossy(), "layer": "coal" })).unwrap();
    assert!(
        record.messages.iter().any(|m| m == "1 edges where closed pieces touch taken apart (2 vertices copied, no coordinate moved): 2 closed pieces, each a solid of its own"),
        "{:?}",
        record.messages
    );
    assert_eq!(record.messages.last().unwrap(), "volumes: pair part 1 1000.000 m3, pair part 2 1000.000 m3");
    let names: Vec<String> = created(&record).iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    assert_eq!(names, ["pair part 1", "pair part 2"]);

    // With one piece open, nothing is made.
    let mut open = pair.clone();
    open.triangles.truncate(22);
    write(&dir.join("half.00t"), &open);
    let error = s.run("solid.import", json!({ "path": dir.join("half.00t").to_string_lossy(), "layer": "coal" })).unwrap_err();
    assert!(error.to_string().starts_with("'half' is not a closed solid after repair"), "{error}");
    assert_eq!(s.project().objects().count(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn repair_closes_fins_cracks_and_small_holes_and_notes_them_on_the_solid() {
    let dir = scratch("repair");
    // A 10 m cube with a fin on its bottom front edge, a crack where the front face meets the top
    // front edge at its midpoint, and one triangle missing from the back face.
    let mut m = block([E, N, 250.0], [10.0, 10.0, 10.0]);
    m.vertices.push([E + 5.0, N - 8.0, 245.0]); // 8: the fin's tip
    m.triangles.push([0, 1, 8]);
    m.vertices.push([E + 5.0, N, 260.0]); // 9: on edge 4-5
    let front = m.triangles.iter().position(|t| *t == [0, 5, 4]).unwrap();
    m.triangles[front] = [0, 5, 9];
    m.triangles.push([0, 9, 4]);
    let back = m.triangles.iter().position(|t| *t == [2, 7, 3]).unwrap();
    m.triangles.remove(back);
    write(&dir.join("worn.00t"), &m);

    let mut s = Session::with_builtins();
    let refused = s.run("solid.import", json!({ "path": dir.join("worn.00t").to_string_lossy(), "layer": "coal" })).unwrap_err();
    assert!(refused.to_string().contains("is not a closed solid"), "without repair it is refused: {refused}");
    let record = s.run("solid.import", json!({ "path": dir.join("worn.00t").to_string_lossy(), "layer": "coal", "repair": true })).unwrap();
    assert_eq!(record.messages[2], "closing: 1 fin triangles taken off; 1 cracks joined; 1 holes filled (50.0 m², 8.3 % of the surface)");
    assert_eq!(record.messages.last().unwrap(), "volume 1000.000 m3");
    let object = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(object.attributes["repair"], zd_core::AttrValue::Text("1 fin triangles taken off; 1 cracks joined; 1 holes filled (50.0 m², 8.3 % of the surface)".into()));
    assert_eq!(object.attributes["repair_filled_m2"], zd_core::AttrValue::Number(50.0));

    // The whole top missing: 100 m² of 600, over the 10 % limit, so refused; at 20 % it is filled.
    let mut lid = block([E, N, 250.0], [10.0, 10.0, 10.0]);
    lid.triangles.drain(2..4);
    write(&dir.join("lidless.00t"), &lid);
    let error = s.run("solid.import", json!({ "path": dir.join("lidless.00t").to_string_lossy(), "layer": "coal", "repair": true })).unwrap_err();
    assert!(error.to_string().starts_with("'lidless' has holes of 100.0 m² in all, 16.7 % of its surface, over the 10 % limit; the largest is 100.0 m² at 650005.000 7450005.000 RL 260.000"), "{error}");
    let record = s.run("solid.import", json!({ "path": dir.join("lidless.00t").to_string_lossy(), "layer": "coal", "repair": true, "fill_limit": 0.2 })).unwrap();
    assert_eq!(record.messages.last().unwrap(), "volume 1000.000 m3");
    // A clean solid has nothing to repair and carries no note.
    write(&dir.join("clean.00t"), &block([E, N, 250.0], [10.0, 10.0, 10.0]));
    let record = s.run("solid.import", json!({ "path": dir.join("clean.00t").to_string_lossy(), "layer": "coal", "repair": true })).unwrap();
    assert_eq!(record.messages[2], "closing: nothing to do");
    assert!(s.project().object(created(&record)[0]).unwrap().attributes.is_empty());
    assert!(s.run("solid.import", json!({ "path": dir.join("clean.00t").to_string_lossy(), "layer": "coal", "repair": true, "fill_limit": 2.0 })).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn repair_resolves_crossings_into_the_solid_enclosed() {
    let dir = scratch("union");
    // Two 10 m cubes overlapping by 5 m on each axis, as one file: 2,000 m³ as read, 1,875 enclosed.
    let (a, b) = (block([E, N, 250.0], [10.0, 10.0, 10.0]), block([E + 5.0, N + 5.0, 255.0], [10.0, 10.0, 10.0]));
    let mut both = a.clone();
    both.vertices.extend(&b.vertices);
    both.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + 8)));
    write(&dir.join("overlap.00t"), &both);
    let mut s = Session::with_builtins();
    assert!(s.run("solid.import", json!({ "path": dir.join("overlap.00t").to_string_lossy(), "layer": "coal" })).unwrap_err().to_string().contains("crosses itself"));
    let record = s.run("solid.import", json!({ "path": dir.join("overlap.00t").to_string_lossy(), "layer": "coal", "repair": true })).unwrap();
    assert!(record.messages[2].starts_with("closing: ") && record.messages[2].contains("crossings resolved, cut along every crossing") && record.messages[2].contains("volume 2000.000 m3 as read, 1875.000 m3 enclosed, -125.000 m3)"), "{}", record.messages[2]);
    assert!(record.messages[3].starts_with("crossings: none"), "{}", record.messages[3]);
    assert_eq!(record.messages.last().unwrap(), "volume 1875.000 m3");
    let object = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(object.attributes["repair_volume_change_m3"], zd_core::AttrValue::Number(-125.0));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A lens 10 m square rising from knife edges on two sides to a ridge 0.5 mm high down the middle:
/// 0.025 m³. No two vertices lie within the 1 mm weld.
fn knife_edges() -> TriMesh {
    let v = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [5.0, 0.0, 0.0005], [0.0, 10.0, 0.0], [10.0, 10.0, 0.0], [5.0, 10.0, 0.0005]];
    TriMesh::new(v.iter().map(|p| [E + p[0], N + p[1], 250.0 + p[2]]).collect(), vec![[0, 3, 1], [1, 3, 4], [0, 2, 3], [2, 5, 3], [2, 1, 5], [1, 4, 5], [0, 1, 2], [3, 5, 4]])
}

#[test]
fn a_knife_edge_solid_imports_with_its_thin_pairs_reported() {
    // Its top and bottom stay within 0.5 mm of each other: by the solid rules (decision 0014) they
    // are thin, not crossing, so the solid imports as read, with or without repair.
    let dir = scratch("knife");
    let path = dir.join("sliver.00t");
    write(&path, &knife_edges());
    let mut s = Session::with_builtins();
    for repair in [false, true] {
        let record = s.run("solid.import", json!({ "path": path.to_string_lossy(), "layer": "coal", "repair": repair })).unwrap();
        let checked = record.messages.iter().find(|m| m.starts_with("crossings: ")).unwrap();
        assert!(checked.starts_with("crossings: none by more than 1 mm; ") && checked.contains("thin pairs, faces within 1 mm of each other facing opposite ways (knife edges or hairline gaps), the first at 650"), "{checked}");
        assert!(checked.contains(": kept as they are (8 triangles"), "{checked}");
        assert_eq!(record.messages.last().unwrap(), "volume 0.025 m3");
        let Body::Solid(solid) = &s.project().object(created(&record)[0]).unwrap().body else { panic!("not a solid") };
        let crossings = solid.crossings().unwrap();
        assert_eq!((crossings.check, crossings.pairs), (zd_core::CrossingCheck::SolidSearch { tolerance: 0.001 }, 0));
        assert!(crossings.thin > 0 && crossings.thin_at.len() == crossings.thin);
        assert_eq!(solid.validity().non_self_intersecting, Some(true));
    }
    // As a surface the same faces fold back onto each other: flagged by the surface rules.
    let record = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "topo" })).unwrap();
    assert!(record.messages[2].contains("pairs of triangles cross by more than 1 mm"), "{}", record.messages[2]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Opt-in, against real files kept out of the repository: `ZD_VULCAN_TRI` names a `.00t` and
/// `ZD_VULCAN_TRI_DXF` the same triangulation exported to DXF by Deswik. Every vertex must match,
/// and so must the triangles as sets of corner coordinates; for a solid, `ZD_VULCAN_TRI_VOLUME`
/// (m³, Deswik's figure) is checked to 0.001 m³.
#[test]
#[ignore = "needs ZD_VULCAN_TRI and ZD_VULCAN_TRI_DXF"]
fn a_real_triangulation_matches_its_deswik_export() {
    let (Ok(tri), Ok(dxf)) = (std::env::var("ZD_VULCAN_TRI"), std::env::var("ZD_VULCAN_TRI_DXF")) else {
        panic!("set ZD_VULCAN_TRI and ZD_VULCAN_TRI_DXF");
    };
    let ours = read_vulcan_triangulation(Path::new(&tri)).unwrap().mesh;
    let theirs = read_dxf(Path::new(&dxf)).unwrap().mesh;
    let key = |p: &[f64; 3]| p.map(|c| c.to_bits());
    let corners = |m: &TriMesh| {
        let mut set: Vec<[[u64; 3]; 3]> = m
            .triangles
            .iter()
            .map(|t| {
                let mut c = t.map(|i| key(&m.vertices[i as usize]));
                c.sort();
                c
            })
            .collect();
        set.sort();
        set
    };
    let (mut a, mut b): (Vec<[u64; 3]>, Vec<[u64; 3]>) = (ours.vertices.iter().map(key).collect(), theirs.vertices.iter().map(key).collect());
    a.sort();
    a.dedup();
    b.sort();
    let largest = ours
        .vertices
        .iter()
        .zip(&theirs.vertices)
        .map(|(p, q)| (0..3).map(|k| (p[k] - q[k]).abs()).fold(0.0, f64::max))
        .fold(0.0, f64::max);
    println!("{} vertices, {} triangles; in file order the largest difference is {largest} m", ours.vertices.len(), ours.triangles.len());
    assert_eq!(a, b, "the same vertices, bit for bit");
    assert_eq!(corners(&ours), corners(&theirs), "the same triangles");
    if let Ok(volume) = std::env::var("ZD_VULCAN_TRI_VOLUME") {
        let volume: f64 = volume.parse().unwrap();
        assert!((ours.signed_volume() - volume).abs() < 0.001, "ours {} m3, Deswik {volume} m3", ours.signed_volume());
    }
}

/// Opt-in timing on a real surface: `ZD_VULCAN_TRI_TIMING` names a large `.00t`. Run in release.
#[test]
#[ignore = "needs ZD_VULCAN_TRI_TIMING; run with --release"]
fn timing_a_real_surface_import() {
    let Ok(path) = std::env::var("ZD_VULCAN_TRI_TIMING") else { panic!("set ZD_VULCAN_TRI_TIMING") };
    let mut s = Session::with_builtins();
    let record = s.run("surface.import", json!({ "path": path, "layer": "topo" })).unwrap();
    println!("{:.0} ms in all", record.duration_ms);
    for line in &record.messages {
        println!("{line}");
    }
}

/// Opt-in: every `.00t` in the folder `ZD_VULCAN_TRI_DIR` through `solid.import`, one line each, a
/// count of those refused with why, and the solids made.
#[test]
#[ignore = "needs ZD_VULCAN_TRI_DIR"]
fn every_solid_in_a_folder() {
    let Ok(dir) = std::env::var("ZD_VULCAN_TRI_DIR") else { panic!("set ZD_VULCAN_TRI_DIR") };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("00t"))).collect();
    paths.sort();
    let mut s = Session::with_builtins();
    let (mut made, mut refused) = (0, 0);
    for (k, path) in paths.iter().enumerate() {
        match s.run("solid.import", json!({ "path": path.to_string_lossy(), "layer": "solids" })) {
            Ok(record) => {
                made += 1;
                println!("{k:>3} ok: {} | {} | {}", record.messages[1], record.messages[2], record.messages.last().unwrap());
            }
            Err(e) => {
                refused += 1;
                println!("{k:>3} refused: {e}");
            }
        }
    }
    println!("{made} files imported, {refused} refused; {} solids in the project", s.project().objects().count());
}
