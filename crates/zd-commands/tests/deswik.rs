//! Deswik drawings in through the registry (decision 0036): strings with their projection details,
//! closed meshes as solids and open ones as surfaces, each on a layer named after its Deswik layer.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{AttrValue, Body, ChangeKind, Object, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zd-deswik-{tag}-{}-{}", std::process::id(), zd_core::RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn listed(items: &[[f64; 3]], kind: &str, extra: &str) -> String {
    items.iter().map(|p| format!(r#"<P F="_Item" T="{kind}" V="{},{},{}{extra}" />"#, p[0], p[1], p[2])).collect()
}

fn layer_xml(entities: &[String]) -> String {
    format!(r#"<?xml version="1.0" encoding="utf-8"?><Begin><Activator><P F="version" V="10.1004.0.3" /></Activator><O T="vdEntities">{}</O></Begin>"#, entities.concat())
}

/// A closed 100 m toe at RL 100, carrying the projection Deswik made from it.
fn toe() -> String {
    let corners = [[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0, N + 100.0, 100.0], [E, N + 100.0, 100.0]];
    format!(
        r#"<O F="_Item" T="dwPolyline"><O F="XProperties" T="vdXProperties"><O F="_Item" T="vdXProperty"><P F="Name" V="Projection Details" /><P F="PropValue" V="Inside/Right at 45.Up to level 140" /></O><O F="_Item" T="vdXProperty"><P F="Name" V="Projection Surface" /><P F="PropValue" V="3.0 PIT\TOPO" /></O></O><P F="Flag" T="Enum" V="1" /><O F="VertexList" T="Vertexes">{}</O></O>"#,
        listed(&corners, "Vertex", " ,0")
    )
}

/// A polyface of corners and faces (1-based, four to a face then a colour), with Deswik's volume.
fn polyface(points: &[[f64; 3]], faces: &[[i32; 4]], volume: f64, attributes: &str) -> String {
    let faces: String = faces.iter().flat_map(|f| f.iter().copied().chain([-1])).map(|i| format!(r#"<P F="_Item" T="Int32" V="{i}" />"#)).collect();
    format!(
        r#"<O F="_Item" T="dwPolyface"><O F="XProperties" T="vdXProperties">{attributes}</O><O F="VertexList" T="gPoints">{}</O><O F="FaceList" T="Int32Array">{faces}</O><P F="VolumeInternal" T="Double" V="{volume}" /></O>"#,
        listed(points, "gPoint", "")
    )
}

/// A 100 x 100 x 5 m box of four-cornered faces, wound as Deswik left one: some in, some out.
fn box_corners(at: [f64; 3]) -> Vec<[f64; 3]> {
    let [x, y, z] = at;
    vec![[x, y, z], [x + 100.0, y, z], [x + 100.0, y + 100.0, z], [x, y + 100.0, z], [x, y, z + 5.0], [x + 100.0, y, z + 5.0], [x + 100.0, y + 100.0, z + 5.0], [x, y + 100.0, z + 5.0]]
}

const BOX_FACES: [[i32; 4]; 6] = [[1, 2, 3, 4], [5, 6, 7, 8], [1, 2, 6, 5], [2, 3, 7, 6], [3, 4, 8, 7], [4, 1, 5, 8]];

fn mixed_box() -> String {
    polyface(&box_corners([E, N, 95.0]), &BOX_FACES, 50_000.0, r#"<O F="_Item" T="vdXProperty"><P F="Name" V="BENCH" /><P F="PropValue" V="95" /></O>"#)
}

/// The box with a cavity inside it: a tetrahedron wound inward, as a boolean leaves one, so
/// Deswik's volume is the box less the cavity.
fn box_with_cavity() -> String {
    let mut points = box_corners([E + 200.0, N, 95.0]);
    let (x, y, z) = (E + 240.0, N + 40.0, 96.0);
    points.extend([[x, y, z], [x + 3.0, y, z], [x, y + 3.0, z], [x, y, z + 3.0]]);
    let mut faces = BOX_FACES.to_vec();
    // Outward winding of the box faces, so only the cavity is in question.
    faces[0] = [1, 4, 3, 2];
    faces[2] = [1, 2, 6, 5];
    faces[3] = [2, 3, 7, 6];
    faces[4] = [3, 4, 8, 7];
    faces[5] = [4, 1, 5, 8];
    // The tetrahedron's faces wound to face into it.
    faces.extend([[9, 10, 11, 9], [9, 12, 10, 9], [9, 11, 12, 9], [10, 12, 11, 10]]);
    polyface(&points, &faces, 50_000.0 - 4.5, "")
}

/// A flat 100 m square of topography at RL 110, open.
fn topo() -> String {
    let points = [[E, N, 110.0], [E + 100.0, N, 110.0], [E + 100.0, N + 100.0, 110.0], [E, N + 100.0, 110.0]];
    polyface(&points, &[[1, 2, 3, 1], [1, 3, 4, 1]], -1.0, "")
}

fn write_dcf(path: &Path, layers: &[(&str, String)]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("Pit v3.dcf", options).unwrap();
    zip.write_all(layer_xml(&[]).as_bytes()).unwrap();
    for (layer, xml) in layers {
        zip.start_file(format!("Pit v3.dcf_{layer}"), options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn drawing(dir: &Path) -> PathBuf {
    let path = dir.join("Pit v3.dcf");
    write_dcf(
        &path,
        &[
            ("3@@0 PIT^^DESIGN", layer_xml(&[toe()])),
            ("3@@0 PIT^^SOLIDS", layer_xml(&[mixed_box()])),
            ("3@@0 PIT^^TOPO", layer_xml(&[topo()])),
            ("3@@0 PIT^^CAVITY", layer_xml(&[box_with_cavity()])),
        ],
    );
    path
}

fn object(s: &Session, id: ObjectId) -> (&Object, String) {
    let object = s.project().object(id).unwrap();
    let layer = s.project().layer(object.layer).unwrap().name.clone();
    (object, layer)
}

#[test]
fn a_drawing_comes_in_with_its_layers_strings_solids_and_surfaces() {
    let dir = scratch("all");
    let path = drawing(&dir);
    let mut s = Session::with_builtins();
    let record = s.run("deswik.import", json!({ "path": path.to_string_lossy() })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 4, "{:#?}", record.messages);

    let (string, layer) = object(&s, made[0]);
    assert_eq!((string.name.as_str(), layer.as_str()), ("DESIGN 1", r"3.0 PIT\DESIGN"));
    assert_eq!(string.attributes.get("Projection Details"), Some(&AttrValue::Text("Inside/Right at 45.Up to level 140".into())));
    assert_eq!(string.attributes.get("Projection Surface"), Some(&AttrValue::Text(r"3.0 PIT\TOPO".into())));
    let Body::String(line) = &string.body else { panic!("a string") };
    assert!(line.closed);
    assert_eq!(line.vertices[2], [E + 100.0, N + 100.0, 100.0]);

    // The box's faces were wound either way; the repair step turns them out, and its volume is Deswik's.
    let (solid, layer) = object(&s, made[1]);
    assert_eq!((solid.name.as_str(), layer.as_str()), ("SOLIDS 1", r"3.0 PIT\SOLIDS"));
    let Body::Solid(b) = &solid.body else { panic!("a solid, as it is closed") };
    assert!((b.volume() - 50_000.0).abs() < 1e-6, "{} m3", b.volume());
    assert_eq!(solid.attributes.get("deswik_volume_m3"), Some(&AttrValue::Number(50_000.0)));
    assert_eq!(solid.attributes.get("BENCH"), Some(&AttrValue::Text("95".into())));

    let (surface, layer) = object(&s, made[2]);
    assert_eq!((surface.name.as_str(), layer.as_str()), ("TOPO 1", r"3.0 PIT\TOPO"));
    assert!(matches!(&surface.body, Body::Surface(_)), "open, so a surface");
    assert!(!surface.attributes.contains_key("deswik_volume_m3"), "Deswik writes -1 where it worked out no volume");

    // A closed piece inside another is a cavity: the repair step keeps it facing in, so its 4.5 m3 is
    // taken away, as Deswik takes it, and no volume differs from Deswik's.
    let (cavity, _) = object(&s, made[3]);
    let Body::Solid(c) = &cavity.body else { panic!("a solid") };
    assert!((c.volume() - (50_000.0 - 4.5)).abs() < 1e-6, "{} m3", c.volume());
    let lines = record.messages.join("\n");
    assert!(lines.contains("CAVITY 1: corners as written; 0 triangles flipped to agree with their neighbours and face up, or out where closed (2 patches, 2 closed); 1 closed patches inside others wound to face in, as cavities"), "{lines}");
    assert!(lines.contains("1 closed patches inside others wound to face in, as cavities"), "{lines}");
    assert!(!lines.contains("differs from Deswik's own"), "{lines}");
    assert!(lines.contains("Deswik: 4 layers read (VectorDraw 10.1004.0.3), 1 strings and 3 meshes of 18 faces; 12 four-cornered faces split in two"), "{lines}");
    assert!(lines.contains("2 solids imported"), "{lines}");
    assert!(lines.contains("1 surfaces imported"), "{lines}");

    // One step to undo.
    s.undo();
    assert_eq!(s.project().objects().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn only_the_layers_named_come_in_and_one_layer_or_one_kind_can_be_asked_for() {
    let dir = scratch("some");
    let path = drawing(&dir);
    let path = path.to_string_lossy();

    let mut s = Session::with_builtins();
    let record = s.run("deswik.import", json!({ "path": path, "layers": [r"3.0 PIT\SOLIDS", r"3.0 PIT\TOPO"], "layer": "from deswik" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 2);
    for id in &made {
        let (o, layer) = object(&s, *id);
        assert_eq!(layer, "from deswik");
        assert!(matches!(o.attributes.get("deswik_layer"), Some(AttrValue::Text(l)) if l.starts_with(r"3.0 PIT\")), "the Deswik layer kept on it");
    }

    let mut s = Session::with_builtins();
    let record = s.run("deswik.import", json!({ "path": path, "layers": [r"3.0 PIT\SOLIDS"], "meshes": "surfaces" })).unwrap();
    assert!(matches!(&object(&s, created(&record)[0]).0.body, Body::Surface(_)));

    let mut s = Session::with_builtins();
    let record = s.run("deswik.import", json!({ "path": path, "meshes": "none" })).unwrap();
    assert_eq!(created(&record).len(), 1, "the string alone");
    assert!(record.messages.iter().any(|m| m == "3 meshes left out, as asked"), "{:?}", record.messages);

    let mut s = Session::with_builtins();
    let refused = s.run("deswik.import", json!({ "path": path, "layers": [r"3.0 PIT\SOLIDS", "PIT B"] })).unwrap_err().to_string();
    assert!(refused.contains("no layer named PIT B"), "{refused}");
    let refused = s.run("deswik.import", json!({ "path": path, "layers": [r"3.0 PIT\TOPO"], "meshes": "solids" })).unwrap_err().to_string();
    assert!(refused.contains("1 meshes refused"), "{refused}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Opt-in timing on a real drawing: `ZD_DESWIK_DCF` names a `.dcf` and `ZD_DESWIK_LAYERS` the layers
/// (`;` between them; every layer if unset). Run in release.
#[test]
#[ignore = "needs ZD_DESWIK_DCF; run with --release"]
fn timing_a_real_drawing_import() {
    let Ok(path) = std::env::var("ZD_DESWIK_DCF") else { panic!("set ZD_DESWIK_DCF") };
    let layers: Vec<String> = std::env::var("ZD_DESWIK_LAYERS").map(|l| l.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    let mut s = Session::with_builtins();
    let record = s.run("deswik.import", json!({ "path": path, "layers": layers })).unwrap();
    println!("{:.0} ms in all, {} objects", record.duration_ms, created(&record).len());
    for line in record.messages.iter().take(60) {
        println!("{line}");
    }
}
