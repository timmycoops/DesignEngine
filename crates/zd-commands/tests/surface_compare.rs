//! Compare surfaces (Surface tools plan, workstream 1; decision 0093): `surface.compare` through the session as the app
//! and Python run it. Every answer is known; mine grid coordinates throughout.

use serde_json::json;
use zd_commands::{Code, RunRecord, Session};
use zd_core::{Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn mesh(s: &mut Session, name: &str, vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> ObjectId {
    created(&s.run("surface.from_mesh", json!({ "name": name, "layer": "surfaces", "vertices": vertices, "triangles": triangles })).unwrap())[0]
}

/// A level surface 200 m square from (E, N) at `rl`.
fn level(s: &mut Session, name: &str, rl: f64) -> ObjectId {
    mesh(s, name, vec![[E, N, rl], [E + 200.0, N, rl], [E + 200.0, N + 200.0, rl], [E, N + 200.0, rl]], vec![[0, 1, 2], [0, 2, 3]])
}

/// A pit in a 200 m square design at RL 100: its floor 100 m square at RL 90, walls at 45° to a crest 120 m square.
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

/// The pit against a level topography at RL 100, wherever both are: 121,333.3 m³ of cut, none of fill, over the 200 m
/// square; the column check said and within 0.1 %, so no warning; nothing made, the surfaces at their revisions.
#[test]
fn a_pit_against_the_topography_is_its_cut() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 100.0);
    let design = pit(&mut s);
    let revisions = [topo, design].map(|id| s.project().object(id).unwrap().revision);
    let record = s.run("surface.compare", json!({ "surface": topo, "with": design, "cell": 1.0 })).unwrap();
    assert!(created(&record).is_empty(), "no thickness grid unless asked for");
    assert!(
        record.messages[0].starts_with("'Pit design' against 'Topography' wherever both are: cut 121,333.3 m³, fill 0.0 m³, net cut 121,333.3 m³, over 40,000.0 m² both cover"),
        "{:?}",
        record.messages
    );
    assert!(record.messages[1].starts_with("by columns at 1 m"), "{:?}", record.messages);
    assert!(!record.notes.iter().any(|n| n.code == Some(Code::SurfaceColumnsDiffer)));
    assert_eq!([topo, design].map(|id| s.project().object(id).unwrap().revision), revisions);
}

/// Inside the crest, against a topography at RL 95: cut 55,166.7 m³ below RL 95, fill 5,833.3 m³ above it, net cut
/// 49,333.3 m³; with the thickness grid asked for, "Pit design less Topography" on 'isopach', its floor nodes 5 m down.
#[test]
fn inside_a_string_with_the_thickness_grid() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 95.0);
    let design = pit(&mut s);
    let crest = [[-60.0, -60.0], [60.0, -60.0], [60.0, 60.0], [-60.0, 60.0]].map(|[x, y]| [E + 100.0 + x, N + 100.0 + y, 0.0]);
    let limit = created(&s.run("string.create", json!({ "name": "Crest", "layer": "limits", "points": crest, "closed": true })).unwrap())[0];
    let record = s.run("surface.compare", json!({ "surface": topo, "with": design, "string": limit, "cell": 1.0, "thickness": true, "layer": "isopach" })).unwrap();
    assert!(record.messages[0].contains("inside 'Crest': cut 55,166.7 m³, fill 5,833.3 m³, net cut 49,333.3 m³, over 14,400.0 m² both cover"), "{:?}", record.messages);
    let grid = created(&record);
    assert_eq!(grid.len(), 1);
    let object = s.project().object(grid[0]).unwrap();
    assert_eq!(object.name, "Pit design less Topography");
    assert_eq!(s.project().layer(object.layer).unwrap().name, "isopach");
    let Body::Grid(g) = &object.body else { panic!("a grid") };
    let at = |x: f64, y: f64| g.z[g.index(((E + x - g.origin[0]) / g.cell_size[0]).round() as usize, ((N + y - g.origin[1]) / g.cell_size[1]).round() as usize)];
    assert!((at(100.5, 100.5) + 5.0).abs() < 1e-9, "the floor 5 m below RL 95: {}", at(100.5, 100.5));
    // The grid spans the crest alone: 120 columns of 1 m from 40 m east, a node in the middle of each cell.
    assert_eq!((g.cols, g.rows), (120, 120));
    assert!((g.origin[0] - (E + 40.5)).abs() < 1e-9 && (g.origin[1] - (N + 40.5)).abs() < 1e-9, "{:?}", g.origin);
}

/// What can't be compared: a surface with itself, surfaces that don't overlap, a cell of nothing.
#[test]
fn what_cant_be_compared_is_refused() {
    let mut s = Session::with_builtins();
    let topo = level(&mut s, "Topography", 100.0);
    let far = mesh(&mut s, "Far", vec![[E + 1000.0, N, 0.0], [E + 1100.0, N, 0.0], [E + 1100.0, N + 100.0, 0.0]], vec![[0, 1, 2]]);
    let refused = |s: &mut Session, params: serde_json::Value| s.run("surface.compare", params).unwrap_err().to_string();
    assert!(refused(&mut s, json!({ "surface": topo, "with": topo })).contains("itself"));
    assert!(refused(&mut s, json!({ "surface": topo, "with": far })).contains("don't overlap"));
    assert!(refused(&mut s, json!({ "surface": topo, "with": far, "cell": 0.0 })).contains("cell"));
}
