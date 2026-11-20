//! Merge surfaces (Surface tools plan, workstream 2; decision 0094): `surface.merge` through the session. Every answer is
//! known; mine grid coordinates throughout.

use serde_json::json;
use zd_commands::{RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn mesh(s: &mut Session, name: &str, vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> ObjectId {
    created(&s.run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": vertices, "triangles": triangles })).unwrap())[0]
}

fn level(s: &mut Session, name: &str, size: f64, rl: f64) -> ObjectId {
    mesh(s, name, vec![[E - 100.0, N - 100.0, rl], [E - 100.0 + size, N - 100.0, rl], [E - 100.0 + size, N - 100.0 + size, rl], [E - 100.0, N - 100.0 + size, rl]], vec![[0, 1, 2], [0, 2, 3]])
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

/// The bottom of a level topography 400 m square and the pit, exact: one new surface "Merged bottom" on 'merged'; compared
/// against the topography, the pit's 121,333.3 m³ of cut over the topography's 160,000 m². The two read are as they were.
#[test]
fn the_pit_dug_out_exactly() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 400.0, 100.0);
    let design = pit(&mut s);
    let revisions = [topo, design].map(|id| s.project().object(id).unwrap().revision);
    let record = s.run("surface.merge", json!({ "objects": [topo, design], "output": "surface" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 1);
    let object = s.project().object(made[0]).unwrap();
    assert_eq!(object.name, "Merged bottom");
    assert_eq!(s.project().layer(object.layer).unwrap().name, "merged");
    assert!(matches!(object.body, Body::Surface(_)));
    assert!(record.messages[0].starts_with("the bottom of Topography, Pit design, as an exact surface"), "{:?}", record.messages);
    assert_eq!([topo, design].map(|id| s.project().object(id).unwrap().revision), revisions);
    let compared = s.run("surface.compare", json!({ "surface": topo, "with": made[0], "cell": 1.0 })).unwrap();
    assert!(compared.messages[0].contains("cut 121,333.3 m³, fill 0.0 m³, net cut 121,333.3 m³, over 160,000.0 m² both cover"), "{:?}", compared.messages);
}

/// As a 5 m grid, by default: "Merged bottom" a grid 81 by 81, its floor node at RL 90, past the design the topography.
#[test]
fn as_a_grid_by_default() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 400.0, 100.0);
    let design = pit(&mut s);
    let record = s.run("surface.merge", json!({ "objects": [topo, design] })).unwrap();
    let object = s.project().object(created(&record)[0]).unwrap();
    let Body::Grid(g) = &object.body else { panic!("a grid") };
    assert_eq!((g.cols, g.rows, g.cell_size), (81, 81, [5.0, 5.0]));
    let at = |x: f64, y: f64| g.z[g.index(((E + x - g.origin[0]) / 5.0).round() as usize, ((N + y - g.origin[1]) / 5.0).round() as usize)];
    assert_eq!((at(100.0, 100.0), at(250.0, 250.0)), (90.0, 100.0));
    assert!(record.messages[0].contains("as a grid sampled at 5 m: 81 by 81 nodes"), "{:?}", record.messages);
}

/// A dump laid on: the top of the topography and a 20 m block from RL 100 to 112, named, on a layer of its own; against
/// the topography, 4,800 m³ of fill.
#[test]
fn a_dump_laid_on_as_the_top() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 400.0, 100.0);
    let v: Vec<[f64; 3]> = (0..8).map(|i| [E + 100.0 + (i & 1) as f64 * 20.0, N + 100.0 + (i >> 1 & 1) as f64 * 20.0, if i >> 2 == 0 { 100.0 } else { 112.0 }]).collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    let dump = created(&s.run("solid.from_mesh", json!({ "name": "Dump", "layer": "solids", "vertices": v, "triangles": triangles })).unwrap())[0];
    let record = s.run("surface.merge", json!({ "objects": [topo, dump], "keep": "top", "output": "surface", "name": "Final landform", "layer": "landforms" })).unwrap();
    let made = created(&record)[0];
    assert_eq!(s.project().object(made).unwrap().name, "Final landform");
    let compared = s.run("surface.compare", json!({ "surface": topo, "with": made, "cell": 1.0 })).unwrap();
    assert!(compared.messages[0].contains("cut 0.0 m³, fill 4,800.0 m³"), "{:?}", compared.messages);
}

/// What can't be merged: one thing alone, the same thing twice, a string.
#[test]
fn what_cant_be_merged_is_refused() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 400.0, 100.0);
    let line = created(&s.run("string.create", json!({ "name": "Line", "layer": "lines", "points": [[E, N, 0.0], [E + 10.0, N, 0.0]], "closed": false })).unwrap())[0];
    let refused = |s: &mut Session, params: serde_json::Value| s.run("surface.merge", params).unwrap_err().to_string();
    assert!(refused(&mut s, json!({ "objects": [topo] })).contains("two things or more"));
    assert!(refused(&mut s, json!({ "objects": [topo, topo] })).contains("twice"));
    assert!(refused(&mut s, json!({ "objects": [topo, line] })).to_lowercase().contains("string"));
}
