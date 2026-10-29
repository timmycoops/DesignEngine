//! Strings and meshes out to DXF and CSV and back in through the registry (workstream 4, decision
//! 0009): the Phase 1 gate's toe string leaves as DXF R12 and comes back bit for bit.

use std::path::PathBuf;

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zd-exchange-{tag}-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Name, layer, closed flag and every coordinate's bits, for comparing strings exactly.
fn fingerprint(s: &Session, id: ObjectId) -> (String, String, bool, Vec<u64>) {
    let object = s.project().object(id).expect("live object");
    let layer = s.project().layer(object.layer).unwrap().name.clone();
    match &object.body {
        Body::String(line) => (object.name.clone(), layer, line.closed, line.vertices.iter().flatten().map(|v| v.to_bits()).collect()),
        other => panic!("expected a string, found {:?}", other.kind()),
    }
}

/// A graded toe and its crest 10 m out, as the gate builds them.
fn toe_and_crest(s: &mut Session) -> (ObjectId, ObjectId) {
    let points = [[E + 0.1, N + 0.2, 50.0], [E + 100.3, N + 0.2, 50.0], [E + 100.3, N + 100.7, 50.0]];
    let toe = created(&s.run("string.create", json!({ "name": "toe", "layer": "design", "points": points })).unwrap())[0];
    s.run("string.set_grade", json!({ "string": toe, "from": 0, "grade": 7.0 })).unwrap();
    let crest = created(&s.run("string.offset", json!({ "string": toe, "distance": 10.0, "side": "right", "layer": "crests" })).unwrap())[0];
    s.run("string.set_closed", json!({ "string": crest, "closed": true })).unwrap();
    (toe, crest)
}

#[test]
fn the_toe_string_goes_out_as_dxf_and_comes_back_bit_for_bit() {
    let dir = scratch("dxf");
    let path = dir.join("toe.dxf");
    let mut s = Session::with_builtins();
    let (toe, crest) = toe_and_crest(&mut s);

    let export = s.run("export.dxf", json!({ "objects": [toe, crest, toe], "path": path.to_string_lossy() })).unwrap();
    assert!(export.outputs.is_empty(), "an export changes nothing");
    assert_eq!(export.inputs.len(), 2, "the record lists each revision written once");
    assert_eq!(export.messages, [format!("wrote 2 strings (6 vertices) to {} as DXF R12", path.display())]);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("$ACADVER\n1\nAC1009\n") && text.contains("0\nPOLYLINE\n8\ncrests\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n9\n"));

    // Exporting adds no undo step: undo takes off the close before it.
    let closed = s.project().object(crest).unwrap().revision;
    s.undo();
    assert_eq!(s.project().object(crest).unwrap().revision, closed - 1);
    s.redo();

    let refused = s.run("export.dxf", json!({ "objects": [toe], "path": path.to_string_lossy() })).unwrap_err();
    assert!(refused.to_string().contains("exists; set overwrite"), "{refused}");
    s.run("export.dxf", json!({ "objects": [toe], "path": path.to_string_lossy(), "overwrite": true })).unwrap();
    s.run("export.dxf", json!({ "objects": [toe, crest], "path": path.to_string_lossy(), "overwrite": true })).unwrap();

    let mut fresh = Session::with_builtins();
    let record = fresh.run("string.import", json!({ "path": path.to_string_lossy() })).unwrap();
    assert_eq!(record.messages, ["DXF: 2 strings read on 2 layers"]);
    let back = created(&record);
    let (a, b) = (fingerprint(&fresh, back[0]), fingerprint(&fresh, back[1]));
    let (toe_print, crest_print) = (fingerprint(&s, toe), fingerprint(&s, crest));
    assert_eq!((a.0.as_str(), a.1.as_str()), ("design 1", "design"));
    assert_eq!((b.0.as_str(), b.1.as_str()), ("crests 1", "crests"));
    assert_eq!((a.2, a.3), (toe_print.2, toe_print.3), "the toe, every coordinate exact");
    assert_eq!((b.2, b.3), (crest_print.2, crest_print.3), "the closed crest, every coordinate exact");

    // One layer for all, if asked.
    let mut one = Session::with_builtins();
    let record = one.run("string.import", json!({ "path": path.to_string_lossy(), "layer": "imported" })).unwrap();
    assert!(created(&record).iter().all(|&id| fingerprint(&one, id).1 == "imported"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn strings_go_out_as_csv_and_come_back_with_their_names_and_layers() {
    let dir = scratch("csv");
    let path = dir.join("strings.csv");
    let mut s = Session::with_builtins();
    let (toe, crest) = toe_and_crest(&mut s);
    let record = s.run("export.csv", json!({ "objects": [toe, crest], "path": path.to_string_lossy() })).unwrap();
    assert_eq!(record.messages, [format!("wrote 2 strings (6 vertices) to {} as CSV", path.display())]);

    let mut fresh = Session::with_builtins();
    let record = fresh.run("string.import", json!({ "path": path.to_string_lossy() })).unwrap();
    assert_eq!(record.messages, ["CSV: 2 strings read from 6 rows"]);
    let back = created(&record);
    assert_eq!(fingerprint(&fresh, back[0]), fingerprint(&s, toe));
    assert_eq!(fingerprint(&fresh, back[1]), fingerprint(&s, crest));

    // Only strings go to CSV.
    let surface = created(
        &s.run(
            "surface.from_mesh",
            json!({ "name": "pad", "layer": "surfaces", "vertices": [[E, N, 0.0], [E + 1.0, N, 0.0], [E, N + 1.0, 0.0]], "triangles": [[0, 1, 2]] }),
        )
        .unwrap(),
    )[0];
    let refused = s.run("export.csv", json!({ "objects": [toe, surface], "path": dir.join("x.csv").to_string_lossy() })).unwrap_err();
    assert_eq!(refused.to_string(), "only strings are exported to CSV, not pad");
    assert!(!dir.join("x.csv").exists());
    assert!(s.run("export.csv", json!({ "objects": [], "path": dir.join("x.csv").to_string_lossy() })).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_solid_goes_out_as_3dfaces_and_reads_back_as_the_same_mesh() {
    let dir = scratch("mesh");
    let path = dir.join("block.dxf");
    let mut s = Session::with_builtins();
    // A 10 x 20 x 5 m box at mine grid coordinates: 1,000 m³.
    let (x, y) = (E + 0.25, N + 0.75);
    let vertices: Vec<[f64; 3]> = (0..8).map(|i| [x + (i & 1) as f64 * 10.0, y + (i >> 1 & 1) as f64 * 20.0, 30.0 + (i >> 2) as f64 * 5.0]).collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let solid = created(&s.run("solid.from_mesh", json!({ "name": "block", "layer": "solids", "vertices": vertices, "triangles": triangles })).unwrap())[0];
    let record = s.run("export.dxf", json!({ "objects": [solid], "path": path.to_string_lossy() })).unwrap();
    assert_eq!(record.messages, [format!("wrote 1 surface or solid (12 triangles) to {} as DXF R12", path.display())]);

    let record = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "back" })).unwrap();
    assert!(record.messages[0].starts_with("DXF: 12 faces read (0 polyface meshes)"), "{:?}", record.messages);
    let back = created(&record)[0];
    match &s.project().object(back).unwrap().body {
        Body::Surface(surface) => {
            // Every corner exact and welded back to eight (in the order the faces first meet them).
            let mut corners = surface.mesh().vertices.clone();
            let mut expected = vertices.clone();
            for list in [&mut corners, &mut expected] {
                list.sort_by(|a, b| a.iter().zip(b).map(|(p, q)| p.total_cmp(q)).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal));
            }
            assert_eq!(corners, expected);
            assert!(surface.validity().is_watertight());
            assert!((surface.mesh().signed_volume() - 1000.0).abs() < 1e-6, "volume to 1e-6 m³");
        }
        other => panic!("expected a surface, found {:?}", other.kind()),
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn what_a_dxf_import_converts_and_passes_over_is_in_the_run_record() {
    let dir = scratch("log");
    let path = dir.join("mixed.dxf");
    // A ramp centreline with a 90-degree arc of radius 20 m (bulge tan 22.5°), a one-vertex string,
    // a tilted LINE-free LWPOLYLINE, text, and one 3DFACE.
    let bulge = (std::f64::consts::PI / 8.0).tan();
    let body = format!(
        "0\nLWPOLYLINE\n8\nRAMP\n90\n3\n38\n120.0\n10\n{e0}\n20\n{n0}\n42\n{bulge}\n10\n{e1}\n20\n{n1}\n10\n{e1}\n20\n{n2}\n\
         0\nLWPOLYLINE\n8\nRAMP\n90\n1\n10\n{e0}\n20\n{n0}\n\
         0\nLWPOLYLINE\n8\nTILTED\n90\n2\n10\n1.0\n20\n2.0\n10\n3.0\n20\n4.0\n210\n0.0\n220\n0.0\n230\n-1.0\n\
         0\nTEXT\n8\nRAMP\n1\nRamp 1\n0\nTEXT\n8\nRAMP\n1\n10%\n0\nPOINT\n8\nRAMP\n\
         0\n3DFACE\n8\nTOPO\n10\n0\n20\n0\n30\n0\n11\n1\n21\n0\n31\n0\n12\n0\n22\n1\n32\n0\n13\n0\n23\n1\n33\n0\n",
        e0 = E,
        n0 = N,
        e1 = E + 20.0,
        n1 = N + 20.0,
        n2 = N + 120.0,
    );
    std::fs::write(&path, format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")).unwrap();

    let mut s = Session::with_builtins();
    let record = s.run("string.import", json!({ "path": path.to_string_lossy() })).unwrap();
    assert_eq!(
        record.messages,
        ["DXF: 2 strings read on 2 layers; 1 arc replaced by chords within 0.01 m; 1 string drawn in a tilted plane moved to world coordinates; \
          1 string with one vertex passed over; 1 triangle not read (import the file as a surface); passed over 2 TEXT, 1 POINT"]
    );
    let ramp = created(&record)[0];
    let (name, layer, _, _) = fingerprint(&s, ramp);
    assert_eq!((name.as_str(), layer.as_str()), ("RAMP 1", "RAMP"));
    // The arc runs counter-clockwise from (E, N) to (E + 20, N + 20) about (E, N + 20): every chord
    // vertex 20 m from the centre, at the polyline's elevation.
    let Body::String(line) = &s.project().object(ramp).unwrap().body else { unreachable!() };
    let arc = &line.vertices[..line.vertices.len() - 1];
    assert!(arc.len() > 3);
    assert!(arc.iter().all(|p| ((p[0] - E).hypot(p[1] - (N + 20.0)) - 20.0).abs() < 1e-9 && p[2] == 120.0));
    assert_eq!(line.vertices.last().unwrap(), &[E + 20.0, N + 120.0, 120.0]);

    let surface = s.run("surface.import", json!({ "path": path.to_string_lossy(), "layer": "topo" })).unwrap();
    assert_eq!(
        surface.messages[0],
        "DXF: 1 faces read (0 polyface meshes), 0 quads split, 0 degenerate and 0 duplicate faces dropped; 2 strings not read (import the file as strings); passed over 2 TEXT, 1 POINT"
    );

    // A file with nothing to import as strings says what it does hold.
    let faces = dir.join("faces.dxf");
    std::fs::write(&faces, "0\nSECTION\n2\nENTITIES\n0\n3DFACE\n8\nT\n10\n0\n20\n0\n30\n0\n11\n1\n21\n0\n31\n0\n12\n0\n22\n1\n32\n0\n0\nENDSEC\n0\nEOF\n").unwrap();
    let error = s.run("string.import", json!({ "path": faces.to_string_lossy() })).unwrap_err();
    assert!(error.to_string().contains("holds no strings (DXF: 0 strings read on 0 layers; 1 triangle not read"), "{error}");
    assert!(s.run("string.import", json!({ "path": dir.join("x.str").to_string_lossy() })).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A flat 2 x 2 Vulcan grid at 10 m (plain layout) at elevation `z`.
fn vulcan_grid(z: f32) -> Vec<u8> {
    let mut out = b"SYNTHETIC GRID                  ".to_vec();
    for v in [E, E + 10.0, N, N + 10.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [4i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [10.0f32, 10.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [2i32, 2] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for _ in 0..4 {
        out.extend_from_slice(&z.to_be_bytes());
        out.extend_from_slice(&1i32.to_be_bytes());
    }
    out
}

#[test]
fn surfaces_and_strings_share_a_file_and_grids_are_refused() {
    let dir = scratch("grid");
    std::fs::write(dir.join("topo.xyz"), "0 0 1
10 0 1
0 10 1
10 10 1
").unwrap();
    std::fs::write(dir.join("pitu.srg"), vulcan_grid(100.0)).unwrap();
    std::fs::write(dir.join("pitu.sfg"), vulcan_grid(98.0)).unwrap();
    let mut s = Session::with_builtins();
    let surface = created(&s.run("surface.import", json!({ "path": dir.join("topo.xyz").to_string_lossy(), "layer": "s" })).unwrap())[0];
    let grids = created(&s.run("grid.import_vulcan_seam", json!({ "dir": dir.to_string_lossy(), "prefix": "pit", "seam": "u", "layer": "g" })).unwrap());
    let (toe, _) = toe_and_crest(&mut s);

    // A surface and a string together: one file, one layer each.
    let path = dir.join("both.dxf");
    let record = s.run("export.dxf", json!({ "objects": [surface, toe], "path": path.to_string_lossy() })).unwrap();
    assert!(record.messages[0].starts_with("wrote 1 string (3 vertices) and 1 surface or solid (2 triangles)"), "{:?}", record.messages);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("0
TABLE
2
LAYER
70
2
0
LAYER
2
s
"));

    let refused = s.run("export.dxf", json!({ "objects": [toe, grids[0]], "path": dir.join("g.dxf").to_string_lossy() })).unwrap_err();
    assert_eq!(refused.to_string(), "grids and seam models are not exported to DXF: U sf");
    assert!(!dir.join("g.dxf").exists());

    // A Vulcan elevation grid also imports as a surface: two triangles per cell, uncrossable.
    let record = s.run("surface.import", json!({ "path": dir.join("pitu.srg").to_string_lossy(), "layer": "roofs" })).unwrap();
    assert_eq!(record.messages[0], "Vulcan grid 2 x 2 nodes at 10 x 10 m: 4 live, 0 null");
    let Body::Surface(roof) = &s.project().object(created(&record)[0]).unwrap().body else { panic!("not a surface") };
    assert_eq!((roof.mesh().triangle_count(), roof.validity().non_self_intersecting), (2, Some(true)));
    assert!(s.run("solid.import", json!({ "path": dir.join("pitu.srg").to_string_lossy(), "layer": "x" })).unwrap_err().to_string().contains("is a grid"));
    std::fs::remove_dir_all(&dir).unwrap();
}
