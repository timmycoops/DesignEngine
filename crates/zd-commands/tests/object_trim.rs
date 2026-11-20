//! Trim in plan (Surface tools plan, workstream 3; decision 0095): `object.trim` through the session. Every answer is
//! known; mine grid coordinates throughout.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn made(record: &RunRecord, change: ChangeKind) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == change).map(|o| o.id).collect()
}

fn mesh(s: &mut Session, name: &str, vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> ObjectId {
    made(&s.run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": vertices, "triangles": triangles })).unwrap(), ChangeKind::Created)[0]
}

/// A level topography 400 m square from (E - 100, N - 100) at RL 100.
fn topography(s: &mut Session) -> ObjectId {
    mesh(s, "Topography", vec![[E - 100.0, N - 100.0, 100.0], [E + 300.0, N - 100.0, 100.0], [E + 300.0, N + 300.0, 100.0], [E - 100.0, N + 300.0, 100.0]], vec![[0, 1, 2], [0, 2, 3]])
}

/// The pit design of the compare tests: a 100 m floor at RL 90, 45° walls to a 120 m crest, in a 200 m square at RL 100.
fn pit(s: &mut Session) -> ObjectId {
    let square = |half: f64, rl: f64| [[-half, -half], [half, -half], [half, half], [-half, half]].map(|[x, y]| [E + 100.0 + x, N + 100.0 + y, rl]);
    let mut vertices = Vec::new();
    vertices.extend(square(50.0, 90.0));
    vertices.extend(square(60.0, 100.0));
    vertices.extend(square(100.0, 100.0));
    let mut triangles = vec![[0, 1, 2], [0, 2, 3]];
    for (inner, outer) in [(0u32, 4u32), (4, 8)] {
        for k in 0..4u32 {
            let (a, b) = (inner + k, inner + (k + 1) % 4);
            let (c, d) = (outer + (k + 1) % 4, outer + k);
            triangles.push([a, d, c]);
            triangles.push([a, c, b]);
        }
    }
    mesh(s, "Pit design", vertices, triangles)
}

/// A box 20 m square from (E + x, N + y), RL 90 to 110.
fn block(s: &mut Session, name: &str, x: f64, y: f64) -> ObjectId {
    let v: Vec<[f64; 3]> = (0..8).map(|i| [E + x + (i & 1) as f64 * 20.0, N + y + (i >> 1 & 1) as f64 * 20.0, if i >> 2 == 0 { 90.0 } else { 110.0 }]).collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    made(&s.run("solid.from_mesh", json!({ "name": name, "layer": "solids", "vertices": v, "triangles": triangles })).unwrap(), ChangeKind::Created)[0]
}

fn string(s: &mut Session, name: &str, points: Vec<[f64; 3]>, closed: bool) -> ObjectId {
    made(&s.run("string.create", json!({ "name": name, "layer": "lines", "points": points, "closed": closed })).unwrap(), ChangeKind::Created)[0]
}

/// The pit's crest, 120 m square, as the string trimmed by.
fn crest(s: &mut Session) -> ObjectId {
    string(s, "Pit limit", vec![[E + 40.0, N + 40.0, 0.0], [E + 160.0, N + 40.0, 0.0], [E + 160.0, N + 160.0, 0.0], [E + 40.0, N + 160.0, 0.0]], true)
}

struct Pit {
    s: Session,
    topo: ObjectId,
    design: ObjectId,
    grid: ObjectId,
    solid: ObjectId,
    line: ObjectId,
    limit: ObjectId,
}

/// The topography, the pit design, their bottom as a 5 m grid, a block half over the crest's east edge, a line across
/// the pit at RL 100 to 120, and the crest as the string to trim by.
fn a_pit() -> Pit {
    let mut s = Session::with_builtins();
    let topo = topography(&mut s);
    let design = pit(&mut s);
    let grid = made(&s.run("surface.merge", json!({ "objects": [topo, design], "name": "Pit grid" })).unwrap(), ChangeKind::Created)[0];
    let solid = block(&mut s, "Block", 150.0, 90.0);
    let line = string(&mut s, "Section line", vec![[E, N + 100.0, 100.0], [E + 200.0, N + 100.0, 120.0]], false);
    let limit = crest(&mut s);
    Pit { s, topo, design, grid, solid, line, limit }
}

/// Every kind, inside the crest, in one run: the design keeps the pit (compared against the topography, the frustum,
/// 121,333.3 m³, over the crest's 14,400 m²), the topography its 14,400 m², the grid its 25 by 25 nodes on the crest and
/// inside, the block its half inside (4,000 m³ of 8,000), the line its 120 m inside at its own heights. One undo puts
/// every one back.
#[test]
fn every_kind_trimmed_inside_in_one_run_and_one_undo() {
    let Pit { mut s, topo, design, grid, solid, line, limit } = a_pit();
    let ids = [topo, design, grid, solid, line];
    let revisions = ids.map(|id| s.project().object(id).unwrap().revision);
    let record = s.run("object.trim", json!({ "objects": ids, "by": limit })).unwrap();
    assert_eq!(made(&record, ChangeKind::Revised).len(), 5, "{:?}", record.messages);
    assert!(made(&record, ChangeKind::Created).is_empty());
    let said = &record.messages[0];
    assert!(said.starts_with("trimmed 5 of 5 to inside 'Pit limit'"), "{said}");
    assert!(said.contains("'Block' 4,000.0 m³ of 8,000.0 m³ kept"), "{said}");
    assert!(said.contains("'Pit grid' 625 of 6,561 nodes with a height kept, 25 by 25"), "{said}");
    assert!(said.contains("'Topography' 14,400.0 m² of 160,000.0 m² kept in plan"), "{said}");
    assert!(said.contains("'Section line' cut in 2 places, 2 parts dropped"), "{said}");

    let compared = s.run("surface.compare", json!({ "surface": topo, "with": design, "cell": 1.0 })).unwrap();
    assert!(compared.messages[0].contains("cut 121,333.3 m³, fill 0.0 m³, net cut 121,333.3 m³, over 14,400.0 m² both cover"), "{:?}", compared.messages);
    let Body::Grid(g) = &s.project().object(grid).unwrap().body else { panic!("a grid") };
    assert_eq!((g.cols, g.rows, g.origin), (25, 25, [E + 40.0, N + 40.0]));
    let Body::String(l) = &s.project().object(line).unwrap().body else { panic!("a string") };
    assert_eq!(l.vertices.len(), 2);
    assert!((l.vertices[0][0] - (E + 40.0)).abs() < 1e-9 && (l.vertices[0][2] - 104.0).abs() < 1e-9 && (l.vertices[1][2] - 116.0).abs() < 1e-9, "{:?}", l.vertices);

    s.undo().unwrap();
    assert_eq!(ids.map(|id| s.project().object(id).unwrap().revision), revisions);
}

/// Outside, as new on a layer (decision 0091): each kept as a copy named as it was on 'trimmed', the originals as they
/// were. The topography keeps 145,600 m² round the crest; the design compared against it nothing, being level there.
#[test]
fn outside_as_new_leaves_the_originals() {
    let Pit { mut s, topo, design, solid, limit, .. } = a_pit();
    let revisions = [topo, design, solid].map(|id| s.project().object(id).unwrap().revision);
    let record = s.run("object.trim", json!({ "objects": [topo, design, solid], "by": limit, "keep": "outside", "result": { "as_new": true, "layer": "trimmed" } })).unwrap();
    let copies = made(&record, ChangeKind::Created);
    assert_eq!(copies.len(), 3);
    assert_eq!([topo, design, solid].map(|id| s.project().object(id).unwrap().revision), revisions);
    let names: Vec<String> = copies.iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    assert_eq!(names, ["Topography", "Pit design", "Block"]);
    assert!(copies.iter().all(|&id| s.project().layer(s.project().object(id).unwrap().layer).unwrap().name == "trimmed"));
    assert!(record.messages[0].contains("'Topography' 145,600.0 m² of 160,000.0 m² kept in plan"), "{:?}", record.messages);
    assert!(record.messages[0].contains("'Block' 4,000.0 m³ of 8,000.0 m³ kept"), "{:?}", record.messages);
    let compared = s.run("surface.compare", json!({ "surface": copies[0], "with": copies[1], "cell": 1.0 })).unwrap();
    assert!(compared.messages[0].contains("cut 0.0 m³, fill 0.0 m³") && compared.messages[0].contains("over 25,600.0 m² both cover"), "{:?}", compared.messages);
}

/// A thing wholly on the side kept is left as it was and said; one wholly on the side dropped refuses the run, named, with
/// nothing changed; and what isn't a closed string can't be trimmed by.
#[test]
fn left_as_it_was_and_what_is_refused() {
    let Pit { mut s, topo, solid, line, limit, .. } = a_pit();
    let inside = block(&mut s, "Inside block", 60.0, 60.0);
    let record = s.run("object.trim", json!({ "objects": [topo, inside], "by": limit })).unwrap();
    assert_eq!(made(&record, ChangeKind::Revised).len(), 1);
    assert!(record.messages[0].ends_with("1 wholly inside already and left as it was: 'Inside block'"), "{:?}", record.messages);

    let refused = |s: &mut Session, params: serde_json::Value| s.run("object.trim", params).unwrap_err().to_string();
    let away = block(&mut s, "Far block", 1_000.0, 1_000.0);
    let before = s.project().object(solid).unwrap().revision;
    let words = refused(&mut s, json!({ "objects": [solid, away], "by": limit }));
    assert!(words.contains("'Far block' lies wholly outside 'Pit limit', so nothing of it would be left"), "{words}");
    assert_eq!(s.project().object(solid).unwrap().revision, before);
    assert!(refused(&mut s, json!({ "objects": [inside], "by": limit })).contains("'Inside block' lies wholly inside 'Pit limit' already"));
    assert!(refused(&mut s, json!({ "objects": [solid], "by": line })).contains("isn't a closed string"));
    assert!(refused(&mut s, json!({ "objects": [solid, limit], "by": limit })).contains("can't be trimmed by itself"));
    assert!(refused(&mut s, json!({ "objects": [], "by": limit })).contains("nothing to trim"));
}
