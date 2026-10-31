//! Vulcan triangulations (`.00t`) through the registry (decision 0011): surfaces and solids, each
//! through the repair step and the crossing check, with opt-in checks against real files and their
//! Deswik exports.

use std::io::Write;
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
fn closed_pieces_meeting_along_an_edge_import_as_one_solid() {
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
        record.messages.iter().any(|m| m == "1 edges where closed pieces touch taken apart (2 vertices copied, no coordinate moved): 2 closed pieces, kept as one solid"),
        "{:?}",
        record.messages
    );
    // One file, one solid, under the file's name, with the volume of both.
    assert_eq!(record.messages.last().unwrap(), "volume 2000.000 m3");
    let names: Vec<String> = created(&record).iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    assert_eq!(names, ["pair"]);

    // It cuts as one: at half height each cube gives 500 m3 to a side (to 1e-6 m3).
    let solid = created(&record)[0];
    let cut = s.run("solid.cut_by_surface", json!({ "solid": solid, "rl": 255.0, "layer": "cut" })).unwrap();
    let parts = created(&cut);
    assert_eq!(parts.len(), 2);
    for part in parts {
        let Some(Body::Solid(solid)) = s.project().object(part).map(|o| &o.body) else { panic!("a part is a solid") };
        assert!((solid.volume() - 1000.0).abs() < 1e-6, "{}", solid.volume());
        assert!(solid.validity().is_watertight());
    }

    // With one piece open, nothing is made.
    let mut open = pair.clone();
    open.triangles.truncate(22);
    write(&dir.join("half.00t"), &open);
    let error = s.run("solid.import", json!({ "path": dir.join("half.00t").to_string_lossy(), "layer": "coal" })).unwrap_err();
    assert!(error.to_string().starts_with("'half' is not a closed solid after repair"), "{error}");
    assert_eq!(s.project().objects().count(), 3);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_zip_and_a_folder_import_in_one_run_and_a_refused_file_is_named() {
    let dir = scratch("many");
    let cube = |k: f64| plain_triangulation_bytes("Created External", &block([E + 20.0 * k, N, 250.0], [10.0, 10.0, 10.0]));
    let mut open = block([E, N + 50.0, 250.0], [10.0, 10.0, 10.0]);
    open.triangles.truncate(10);

    // Three cubes in a zip, one in a subfolder of it, with an open mesh and a file to leave out.
    let zipped = dir.join("relimits.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&zipped).unwrap());
    let entries: [(&str, Vec<u8>); 5] = [
        ("a.00t", cube(0.0)),
        ("b.00t", cube(1.0)),
        ("strip 4/c.00t", cube(2.0)),
        ("gap.00t", plain_triangulation_bytes("Created External", &open)),
        ("readme.txt", b"not a mesh".to_vec()),
    ];
    for (name, bytes) in &entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    // And one more in a folder beside it.
    let folder = dir.join("loose");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("d.00t"), cube(3.0)).unwrap();

    let mut s = Session::with_builtins();
    let record = s.run("solid.import_many", json!({ "paths": [zipped.to_string_lossy(), folder.to_string_lossy()], "layer": "coal" })).unwrap();
    let made = created(&record);
    let mut names: Vec<String> = made.iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    names.sort();
    assert_eq!(names, ["a", "b", "c", "d"]);
    // Four 10 m cubes: 4,000 m3 (to 1e-6 m3), each closed.
    let mut volume = 0.0;
    for &id in &made {
        let Some(Body::Solid(solid)) = s.project().object(id).map(|o| &o.body) else { panic!("a solid") };
        assert!(solid.validity().is_watertight());
        volume += solid.volume();
    }
    assert!((volume - 4000.0).abs() < 1e-6, "{volume}");
    assert!(record.messages[0].starts_with("5 .00t and DXF files from 1 zip and 1 folder; 1 other files left out"), "{:?}", record.messages);
    let refused = record.messages.iter().position(|m| m == "1 files refused, nothing saved for them:").expect("the refusal is reported");
    assert!(record.messages[refused + 1].contains("relimits.zip!gap.00t"), "{:?}", record.messages);

    // One run, so one undo takes them all off.
    assert!(s.undo().is_some());
    assert_eq!(s.project().objects().count(), 0);

    // Nothing to import is refused, and says where it looked.
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let error = s.run("solid.import_many", json!({ "paths": [empty.to_string_lossy()], "layer": "coal" })).unwrap_err();
    assert!(error.to_string().contains("no .00t or .dxf files in"), "{error}");
    let none = s.run("solid.import_many", json!({ "paths": [], "layer": "coal" })).unwrap_err();
    assert!(none.to_string().contains("no files, folders or zips given"), "{none}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pieces_lying_on_each_other_are_searched_as_one_solid() {
    let dir = scratch("lying");
    // Two tetrahedra hinged on one edge, the first above z = 250 and the second below, with a face
    // each folded flat on the other. Sharing the edge, those faces are thin; taken apart they share
    // nothing, and are one piece lying on another.
    let (p, q) = ([E, N, 250.0], [E, N + 10.0, 250.0]);
    let (r, s_top) = ([E + 10.0, N + 5.0, 250.0], [E, N + 5.0, 260.0]);
    let (t, u) = ([E + 6.0, N + 5.0, 250.0], [E, N + 5.0, 240.0]);
    let mesh = TriMesh::new(vec![p, q, r, s_top, t, u], vec![[0, 1, 3], [0, 2, 1], [0, 3, 2], [1, 2, 3], [0, 1, 4], [0, 5, 1], [0, 4, 5], [1, 5, 4]]);
    write(&dir.join("hinged.00t"), &mesh);
    let mut s = Session::with_builtins();

    let as_read = s.run("solid.import", json!({ "path": dir.join("hinged.00t").to_string_lossy(), "layer": "coal" })).unwrap_err();
    assert!(as_read.to_string().starts_with("the solid 'hinged' crosses itself"), "{as_read}");

    // With repair the union makes one solid of them: 500/3 + 100 m3 (to 1e-6 m3), touching only.
    let record = s.run("solid.import", json!({ "path": dir.join("hinged.00t").to_string_lossy(), "layer": "coal", "repair": true })).unwrap();
    assert!(record.messages.iter().any(|m| m.contains("crossings between its pieces resolved")), "{:?}", record.messages);
    let made = created(&record);
    assert_eq!(made.len(), 1);
    let Some(Body::Solid(solid)) = s.project().object(made[0]).map(|o| &o.body) else { panic!("a solid") };
    assert!((solid.volume() - (500.0 / 3.0 + 100.0)).abs() < 1e-6, "{}", solid.volume());
    assert_eq!(solid.crossings().map(|c| c.pairs), Some(0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn repair_closes_fins_cracks_and_small_holes_and_notes_them_on_the_solid() {
    let dir = scratch("repair");
    // A 10 m cube with a fin on its bottom front edge (a hanging sheet of one triangle), a crack
    // where the front face meets the top front edge at its midpoint, and one triangle missing from
    // the back face.
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
    assert_eq!(record.messages[2], "closing: 1 hanging sheets taken off (47.2 m², no volume; the largest 47.2 m² at 650005.000 7449997.333 RL 248.333); 1 cracks joined; 1 holes filled (50.0 m², 8.3 % of the surface)");
    assert_eq!(record.messages.last().unwrap(), "volume 1000.000 m3");
    let object = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(object.attributes["repair"], zd_core::AttrValue::Text("1 hanging sheets taken off (47.2 m², no volume; the largest 47.2 m² at 650005.000 7449997.333 RL 248.333); 1 cracks joined; 1 holes filled (50.0 m², 8.3 % of the surface)".into()));
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
        assert!(checked.starts_with("crossings: none by more than 1 mm; ") && checked.contains("thin pairs, faces within 1 mm of each other that do not cross (knife edges, hairline gaps or thin lips), the first at 650"), "{checked}");
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

/// Opt-in, on real data: sections through a surface and a zip of solids, timed and checked.
/// `ZD_SLICE_SURFACE=<.00t> ZD_SLICE_SOLIDS=<zip> cargo test --release -p zd-commands --test vulcan_tri -- --ignored --nocapture a_real_pit`
#[test]
#[ignore]
fn a_real_pit_slices_exactly_and_fast() {
    let (Ok(surface), Ok(solids)) = (std::env::var("ZD_SLICE_SURFACE"), std::env::var("ZD_SLICE_SOLIDS")) else {
        eprintln!("ZD_SLICE_SURFACE and ZD_SLICE_SOLIDS are not set; skipped");
        return;
    };
    let mut s = Session::with_builtins();
    let topo = created(&s.run("surface.import", json!({ "path": surface, "layer": "topo" })).unwrap())[0];
    let Some(Body::Surface(topo)) = s.project().object(topo).map(|o| o.body.clone()) else { panic!("a surface") };
    let made = created(&s.run("solid.import_many", json!({ "paths": [solids], "layer": "solids", "repair": true })).unwrap());
    let mut joined = TriMesh::default();
    for id in made {
        let Some(Body::Solid(solid)) = s.project().object(id).map(|o| &o.body) else { panic!("a solid") };
        let offset = joined.vertices.len() as u32;
        joined.vertices.extend_from_slice(&solid.mesh().vertices);
        joined.triangles.extend(solid.mesh().triangles.iter().map(|t| t.map(|i| i + offset)));
    }
    let bounds = joined.bounds().unwrap();
    let centre = bounds.centre();
    let span = bounds.max[1] - bounds.min[1];
    for (name, mesh) in [("topography", topo.mesh()), ("solids", &joined)] {
        let mut worst = 0.0f64;
        let mut times = Vec::new();
        let mut totals = (0, 0, 0);
        for k in 0..41 {
            let east = centre[0] + (k as f64 - 20.0) * 25.0;
            let plane = zd_geom::SectionPlane::through([east, centre[1] - span], [east + 30.0, centre[1] + span]).unwrap();
            let start = std::time::Instant::now();
            let lines = zd_geom::slice_mesh(mesh, &plane);
            times.push(start.elapsed().as_secs_f64() * 1e3);
            totals.0 += lines.len();
            totals.1 += lines.iter().filter(|l| l.closed).count();
            totals.2 += lines.iter().map(|l| l.points.len()).sum::<usize>();
            worst = lines.iter().flat_map(|l| &l.points).fold(worst, |w, &p| w.max(plane.offset(p).abs()));
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{name}: {} triangles; 41 sections: {:.1} ms p50, {:.1} ms worst; {} lines per section ({} closed), {} points; furthest point off the plane {worst:.2e} m",
            mesh.triangle_count(),
            times[20],
            times[40],
            totals.0 / 41,
            totals.1 / 41,
            totals.2 / 41
        );
        assert!(worst < 1e-6, "{worst}");
    }
}
