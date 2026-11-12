//! `solid.interrogate` (decision 0061, workstream 2): each block's seam grids averaged over it by exact partial cells and
//! written onto it. A made-up seam model of two seams on a 25 m grid, and 60 × 100 m blocks naming their seam by an
//! attribute. Values to 1 part in a billion.

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::{Code, CommandError, Session};
use zd_core::{AttrValue, Attributes, Body, Change, ObjectId, Project, RunId, Solid, TriMesh};
use zd_io::ProjectStore;

const E: f64 = 500_000.0;
const N: f64 = 7_000_000.0;
/// The grids' 25 m lattice: 13 by 9 nodes, 300 by 200 m.
const STEP: f64 = 25.0;
const COLS: usize = 13;
const ROWS: usize = 9;

/// A fresh, empty folder under the temp directory; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-interrogate-{tag}-{}-{}", std::process::id(), RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A grid in Vulcan's plain layout on the 25 m lattice; `f` takes x and y in metres from the lattice's origin.
fn vulcan_grid(f: impl Fn(f64, f64) -> f32) -> Vec<u8> {
    vulcan_grid_of(COLS, ROWS, f)
}

/// The same, `cols` by `rows` nodes.
fn vulcan_grid_of(cols: usize, rows: usize, f: impl Fn(f64, f64) -> f32) -> Vec<u8> {
    let mut out = b"SYNTHETIC SEAM GRID             ".to_vec();
    for v in [E, E + (cols - 1) as f64 * STEP, N, N + (rows - 1) as f64 * STEP] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(cols * rows) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [STEP as f32, STEP as f32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [cols as i32, rows as i32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for row in 0..rows {
        for col in 0..cols {
            out.extend_from_slice(&f(col as f64 * STEP, row as f64 * STEP).to_be_bytes());
            out.extend_from_slice(&1i32.to_be_bytes());
        }
    }
    out
}

fn flat(z: f32) -> Vec<u8> {
    vulcan_grid(|_, _| z)
}

/// Ash in seam `cu` rising 0.1 % a metre east from 10 % at the lattice's west edge.
fn ash_cu(x: f64) -> f64 {
    10.0 + 0.1 * x
}

/// Two seams: `cu` from RL 96 to 100 (RD 1.40, ash rising east), and below it `cl` from 90 to 94 (RD 1.50, ash 20 %).
fn write_model(dir: &Path) {
    let files: [(&str, Vec<u8>); 8] = [
        ("cu.srg", flat(100.0)),
        ("cu.sfg", flat(96.0)),
        ("cu.rdg", flat(1.4)),
        ("cu.ashg", vulcan_grid(|x, _| ash_cu(x) as f32)),
        ("cl.srg", flat(94.0)),
        ("cl.sfg", flat(90.0)),
        ("cl.rdg", flat(1.5)),
        ("cl.ashg", flat(20.0)),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    std::fs::write(
        dir.join("stack.csv"),
        "horizon,parent,material,working,roof,floor,rd,ash\ncu,,coal,yes,cu.srg,cu.sfg,cu.rdg,cu.ashg\ncl,,coal,yes,cl.srg,cl.sfg,cl.rdg,cl.ashg\n",
    )
    .unwrap();
}

/// A box `wide` east and `long` north from `(x, y)` metres off the lattice's origin, from `z0` up to `z1`.
fn cuboid(x: f64, y: f64, wide: f64, long: f64, z0: f64, z1: f64) -> TriMesh {
    let vertices = (0..8)
        .map(|i| [E + x + (i & 1) as f64 * wide, N + y + (i >> 1 & 1) as f64 * long, if i >> 2 & 1 == 1 { z1 } else { z0 }])
        .collect();
    let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    TriMesh::new(vertices, triangles)
}

/// A block of the project: its name, its attributes and its mesh.
type Block = (&'static str, Vec<(&'static str, AttrValue)>, TriMesh);

/// A project of these blocks with the seam model built beside them, opened as a session.
fn project(t: &TempDir, blocks: Vec<Block>) -> (Session, ObjectId, Vec<ObjectId>) {
    write_model(&t.0);
    let mut project = Project::new();
    let changes = blocks
        .into_iter()
        .map(|(name, attributes, mesh)| Change::Create {
            name: name.into(),
            layer: "blocks".into(),
            attributes: attributes.into_iter().collect::<Attributes>(),
            body: Body::Solid(Solid::new(mesh).unwrap()),
        })
        .collect();
    project.apply(RunId::new(), changes).unwrap();
    let path = t.0.join("pit.zdproj");
    ProjectStore::create(&path).unwrap().save(&project, "[]").unwrap();
    let mut s = Session::open(&path).unwrap();
    let ids = s.project().objects().map(|o| o.id).collect();
    let model = s.run("seam.build_model", json!({ "definition": t.0.join("stack.csv").to_string_lossy(), "layer": "geology" })).unwrap().outputs[0].id;
    (s, model, ids)
}

fn seam(code: &str) -> (&'static str, AttrValue) {
    ("seam", AttrValue::Text(code.into()))
}

fn number(s: &Session, id: ObjectId, name: &str) -> Option<f64> {
    match s.project().object(id).unwrap().attributes.get(name) {
        Some(AttrValue::Number(n)) => Some(*n),
        _ => None,
    }
}

fn interrogate(s: &mut Session, model: ObjectId, solids: &[ObjectId], more: serde_json::Value) -> Result<zd_commands::RunRecord, CommandError> {
    // The seam attribute is `seam` unless a run says otherwise, as the surface stack writes it.
    let mut params = json!({ "solids": solids, "model": model });
    params.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    s.run("solid.interrogate", params)
}

fn near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9 * b.abs().max(1.0), "{what}: {a} against {b}");
}

/// A value as a grid holds it, in single precision.
fn held(v: f64) -> f64 {
    v as f32 as f64
}

/// The ash of a block from `x0` to `x1` east in `cu`, each node's value over the 25 m cell centred on it, weighted by the
/// block's length in the cell: worked from the cells' edges alone.
fn ash_by_hand(x0: f64, x1: f64) -> f64 {
    let (mut weight, mut sum) = (0.0, 0.0);
    for col in 0..COLS {
        let x = col as f64 * STEP;
        let w = ((x + STEP / 2.0).min(x1) - (x - STEP / 2.0).max(x0)).max(0.0);
        weight += w;
        sum += w * held(ash_cu(x));
    }
    sum / weight
}

#[test]
fn each_block_takes_its_own_seam_s_grids_by_its_attribute() {
    let t = TempDir::new("seams");
    // Two blocks on the same plan, sitting anyhow on the 25 m cells: one in `cu`, named in capitals, one in `cl`.
    let (mut s, model, ids) = project(
        &t,
        vec![("upper", vec![seam("CU")], cuboid(70.0, 40.0, 60.0, 100.0, 96.0, 100.0)), ("lower", vec![seam("cl")], cuboid(70.0, 40.0, 60.0, 100.0, 90.0, 94.0))],
    );
    let record = interrogate(&mut s, model, &ids, json!({})).unwrap();
    assert!(record.messages[0].starts_with("2 of 2 blocks interrogated against"), "{:?}", record.messages);
    assert!(record.notes.iter().all(|n| n.code.is_none()), "nothing to warn of: {:?}", record.messages);
    near(number(&s, ids[0], "rd").unwrap(), held(1.4), "cu's RD");
    near(number(&s, ids[0], "ash").unwrap(), ash_by_hand(70.0, 130.0), "cu's ash, rising east, by the cells' shares");
    near(number(&s, ids[1], "rd").unwrap(), held(1.5), "cl's RD");
    near(number(&s, ids[1], "ash").unwrap(), 20.0, "cl's ash");
    assert_eq!(s.project().object(ids[0]).unwrap().attributes.get("interrogated"), Some(&AttrValue::Text("stack".into())));
    // One edit of the blocks, undone in one step.
    assert_eq!(s.project().object(ids[0]).unwrap().revision, 2);
    s.undo();
    assert_eq!(number(&s, ids[0], "ash"), None);
}

#[test]
fn a_block_off_the_grid_and_one_smaller_than_a_cell() {
    let t = TempDir::new("small");
    // A 10 m square within the cell of the node 4 east and 3 north: that node's ash.
    let (mut s, model, ids) = project(&t, vec![("small", vec![seam("cu")], cuboid(4.0 * STEP - 3.0, 3.0 * STEP - 6.0, 10.0, 10.0, 96.0, 100.0))]);
    interrogate(&mut s, model, &ids, json!({})).unwrap();
    near(number(&s, ids[0], "ash").unwrap(), held(ash_cu(4.0 * STEP)), "the node's ash");
}

#[test]
fn blocks_with_no_seam_or_an_unknown_one_are_left_as_they_were_and_said() {
    let t = TempDir::new("unknown");
    let (mut s, model, ids) = project(
        &t,
        vec![
            ("known", vec![seam("cu")], cuboid(70.0, 40.0, 60.0, 100.0, 96.0, 100.0)),
            ("none", vec![], cuboid(140.0, 40.0, 60.0, 100.0, 96.0, 100.0)),
            ("other", vec![seam("xx")], cuboid(210.0, 40.0, 60.0, 100.0, 96.0, 100.0)),
        ],
    );
    let record = interrogate(&mut s, model, &ids, json!({})).unwrap();
    assert!(record.messages[0].starts_with("1 of 3 blocks interrogated"), "{:?}", record.messages);
    let codes: Vec<Code> = record.notes.iter().filter_map(|n| n.code).collect();
    assert_eq!(codes, vec![Code::InterrogateNoSeam, Code::InterrogateUnknownSeam]);
    assert_eq!(s.project().object(ids[1]).unwrap().revision, 1, "left as it was");
    assert_eq!(s.project().object(ids[2]).unwrap().revision, 1);
}

#[test]
fn a_block_whose_seam_attribute_looks_wrong_is_warned_of_and_still_written() {
    let t = TempDir::new("outside");
    // Says `cu`, but lies in `cl`: the values are cu's, as asked, and the block is pointed out.
    let (mut s, model, ids) = project(&t, vec![("misnamed", vec![seam("cu")], cuboid(70.0, 40.0, 60.0, 100.0, 90.0, 94.0))]);
    let record = interrogate(&mut s, model, &ids, json!({})).unwrap();
    let warned = record.notes.iter().find(|n| n.code == Some(Code::InterrogateOutsideSeam)).expect("warned");
    assert_eq!(warned.object, Some(ids[0]));
    assert!(warned.place.is_some(), "placed at the block");
    near(number(&s, ids[0], "rd").unwrap(), held(1.4), "cu's RD, as its attribute says");
}

#[test]
fn an_attribute_from_elsewhere_isn_t_written_over_without_a_prefix() {
    let t = TempDir::new("prefix");
    // Spry's own ash on the block.
    let (mut s, model, ids) = project(&t, vec![("spry", vec![seam("cu"), ("ash", AttrValue::Number(11.0))], cuboid(70.0, 40.0, 60.0, 100.0, 96.0, 100.0))]);
    let refused = interrogate(&mut s, model, &ids, json!({})).unwrap_err().to_string();
    assert!(refused.contains("already has an attribute 'ash'"), "{refused}");
    interrogate(&mut s, model, &ids, json!({ "prefix": "g_" })).unwrap();
    near(number(&s, ids[0], "g_ash").unwrap(), ash_by_hand(70.0, 130.0), "ours beside it");
    near(number(&s, ids[0], "ash").unwrap(), 11.0, "Spry's kept");
    // Run again, it writes its own afresh.
    interrogate(&mut s, model, &ids, json!({ "prefix": "g_" })).unwrap();
    assert_eq!(s.project().object(ids[0]).unwrap().revision, 3);
}

#[test]
fn each_variable_is_weighted_as_asked() {
    let t = TempDir::new("weights");
    // RD 1.4 west of 100 m east and 1.6 east of it; ash by tonnes leans east, by volume it doesn't.
    let (mut s, model, ids) = project(&t, vec![("block", vec![seam("cu")], cuboid(70.0, 40.0, 60.0, 100.0, 96.0, 100.0))]);
    std::fs::write(t.0.join("cu.rdg"), vulcan_grid(|x, _| if x < 100.0 { 1.4 } else { 1.6 })).unwrap();
    let record = interrogate(&mut s, model, &ids, json!({ "weights": { "ash": [] }, "variables": ["ash"] })).unwrap();
    assert!(record.notes.iter().all(|n| n.code.is_none()), "by volume alone the RD grid isn't read: {:?}", record.messages);
    near(number(&s, ids[0], "ash").unwrap(), ash_by_hand(70.0, 130.0), "ash by volume alone");
    assert_eq!(number(&s, ids[0], "rd"), None, "only what was asked is written");
    let record = interrogate(&mut s, model, &ids, json!({ "variables": ["ash"] })).unwrap();
    assert!(record.notes.iter().any(|n| n.code == Some(Code::LinkChanged)), "the changed RD grid is said: {:?}", record.messages);
    // By tonnes: the cells east of 100 m weigh 1.6 against 1.4, so the ash leans east.
    assert!(number(&s, ids[0], "ash").unwrap() > ash_by_hand(70.0, 130.0) + 0.01);
}

/// 43,000 made-up 60 × 100 m blocks, 400 across, all in seam `cu`, against a 25 m seam model over them all: what a run
/// costs, on every core. Prints counts and times; run with `--ignored`.
#[test]
#[ignore]
fn interrogate_43000_blocks() {
    let t = TempDir::new("scale");
    let (cols, rows) = (24_100 / 25 + 2, 10_900 / 25 + 2);
    let grid = |f: &dyn Fn(f64, f64) -> f32| vulcan_grid_of(cols, rows, f);
    for (name, bytes) in [("cu.srg", grid(&|_, _| 104.0)), ("cu.sfg", grid(&|_, _| 96.0)), ("cu.rdg", grid(&|_, _| 1.4)), ("cu.ashg", grid(&|x, _| ash_cu(x) as f32))] {
        std::fs::write(t.0.join(name), bytes).unwrap();
    }
    std::fs::write(t.0.join("stack.csv"), "horizon,parent,material,working,roof,floor,rd,ash
cu,,coal,yes,cu.srg,cu.sfg,cu.rdg,cu.ashg
").unwrap();
    let mut project = Project::new();
    let changes = (0..43_000)
        .map(|i| Change::Create {
            name: format!("block {i}"),
            layer: "blocks".into(),
            attributes: [seam("cu")].into_iter().collect::<Attributes>(),
            body: Body::Solid(Solid::new(cuboid(10.0 + (i % 400) as f64 * 60.0, 10.0 + (i / 400) as f64 * 100.0, 60.0, 100.0, 97.0, 103.0)).unwrap()),
        })
        .collect();
    project.apply(RunId::new(), changes).unwrap();
    let path = t.0.join("blocks.zdp");
    zd_io::ZdpStore::create(&path).save(&zd_io::Snapshot::of(&project), "[]", true).unwrap();
    let mut s = Session::open(&path).unwrap();
    let ids: Vec<ObjectId> = s.project().objects().map(|o| o.id).collect();
    let model = s.run("seam.build_model", json!({ "definition": t.0.join("stack.csv").to_string_lossy(), "layer": "geology" })).unwrap().outputs[0].id;
    let start = std::time::Instant::now();
    let record = interrogate(&mut s, model, &ids, json!({})).unwrap();
    println!("{} ({:.0} ms all told, {} revisions)", record.messages[0], start.elapsed().as_secs_f64() * 1e3, record.outputs.len());
    assert_eq!(record.outputs.len(), 43_000);
}

#[test]
fn the_seam_can_come_from_another_attribute() {
    let t = TempDir::new("named");
    // As a Spry block names its seam by its level.
    let (mut s, model, ids) = project(&t, vec![("block", vec![("Seam", AttrValue::Text("cl".into()))], cuboid(70.0, 40.0, 60.0, 100.0, 90.0, 94.0))]);
    let record = interrogate(&mut s, model, &ids, json!({})).unwrap();
    assert!(record.notes.iter().any(|n| n.code == Some(Code::InterrogateNoSeam)), "no `seam` on it");
    interrogate(&mut s, model, &ids, json!({ "seam_attribute": "Seam" })).unwrap();
    near(number(&s, ids[0], "ash").unwrap(), 20.0, "cl's ash");
}
