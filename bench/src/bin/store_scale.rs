//! The Store plan's made-up project at Phase 4's scale: block solids of about a hundred triangles, each with
//! 250 attributes (mostly zeros, as real block attributes are), written as `.zdp` tables and read back.
//!
//! `cargo run --release -p bench --bin store_scale -- [solids] [attributes]`
//! Defaults: 200,000 solids, 250 attributes each. Tables go to `bench/out/store-scale/`.

use std::collections::HashMap;

use bench::{count, out_dir, timed};
use zd_core::{AttrValue, Attributes, Body, Change, Project, RunId, Solid, TriMesh};
use zd_io::{FolderContainer, Snapshot, ZdpStore, project_differences, read_project, write_project};

/// A block `size` metres a side from `min`, each face cut into `k` by `k` squares: 12 k² triangles.
fn block(min: [f64; 3], size: [f64; 3], k: usize) -> TriMesh {
    let mut index: HashMap<[usize; 3], u32> = HashMap::new();
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut at = |p: [usize; 3], vertices: &mut Vec<[f64; 3]>| {
        *index.entry(p).or_insert_with(|| {
            vertices.push([0, 1, 2].map(|a| min[a] + size[a] * p[a] as f64 / k as f64));
            (vertices.len() - 1) as u32
        })
    };
    for a in 0..3 {
        let (u, v) = ((a + 1) % 3, (a + 2) % 3);
        for side in [0, k] {
            for i in 0..k {
                for j in 0..k {
                    let corner = |di: usize, dj: usize| {
                        let mut p = [0; 3];
                        (p[a], p[u], p[v]) = (side, i + di, j + dj);
                        p
                    };
                    let [p00, p10, p11, p01] = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)].map(|p| at(p, &mut vertices));
                    // Counter-clockwise about the outward normal: +a on the far side, -a on the near one.
                    if side == k {
                        triangles.extend([[p00, p10, p11], [p00, p11, p01]]);
                    } else {
                        triangles.extend([[p00, p11, p10], [p00, p01, p11]]);
                    }
                }
            }
        }
    }
    TriMesh::new(vertices, triangles)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let solids: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200_000);
    let attributes: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(250);

    let (project, ms) = timed(|| {
        let mut project = Project::new();
        let changes: Vec<Change> = (0..solids)
            .map(|i| {
                let (x, y, z) = (650_000.0 + (i % 400) as f64 * 50.0, 7_450_000.0 + ((i / 400) % 500) as f64 * 40.0, 100.0 - (i / 200_000) as f64 * 10.0);
                let mut values = Attributes::new();
                for a in 0..attributes {
                    let value = match a {
                        0 => AttrValue::Text(format!("STRIP {:03}", i % 400)),
                        1 => AttrValue::Text(["A1", "A2", "B", "C", "OB"][i % 5].into()),
                        2 => AttrValue::Text(format!("BLK_{i:07}")),
                        a if a < 40 => AttrValue::Number((i as f64 * 0.37 + a as f64).sin() * 50.0 + 60.0),
                        _ => AttrValue::Number(0.0),
                    };
                    values.insert(format!("Q{a:03}"), value);
                }
                let mesh = block([x, y, z], [50.0, 40.0, 10.0], 3);
                Change::Create { name: format!("block {i}"), layer: "blocks".into(), attributes: values, body: Body::Solid(Solid::new(mesh).expect("a closed block")) }
            })
            .collect();
        project.apply(RunId::new(), changes).expect("blocks made");
        project
    });
    let triangles: usize = project.objects().map(|o| if let Body::Solid(s) = &o.body { s.mesh().triangle_count() } else { 0 }).sum();
    println!("made {} solids, {} triangles, {} attribute values in {ms:.0} ms", count(solids), count(triangles), count(solids * attributes));

    let dir = out_dir().join("store-scale");
    let _ = std::fs::remove_dir_all(&dir);
    let mut folder = FolderContainer::new(&dir);
    let written = write_project(&project, "[]", &mut folder).expect("written");
    println!("tables: {} entries, {:.1} MB, encoded in {:.0} ms, written in {:.0} ms", written.entries, written.bytes as f64 / 1e6, written.encode_ms, written.write_ms);
    let read = read_project(&folder).expect("read");
    println!("read back in {:.0} ms", read.ms);
    let (differences, ms) = timed(|| project_differences(&project, &read.project));
    println!("{} differences, compared in {ms:.0} ms", differences.len());
    assert!(differences.is_empty(), "{:?}", &differences[..differences.len().min(5)]);
    let volume: f64 = read.project.objects().map(|o| if let Body::Solid(s) = &o.body { s.volume() } else { 0.0 }).sum();
    assert!((volume - solids as f64 * 20_000.0).abs() < 1e-6 * volume, "every block 50 x 40 x 10 m: {volume}");

    // As one .zdp file: saved, opened, one block renamed and saved, then saved with nothing changed (decision 0058).
    let path = out_dir().join("store-scale.zdp");
    let first = ZdpStore::create(&path).save(&Snapshot::of(&project), "[]", true).expect("saved");
    println!("file: {:.1} MB, first save {:.0} ms", first.bytes as f64 / 1e6, first.ms);
    drop(project);
    let ((mut store, read), ms) = timed(|| ZdpStore::open(&path).expect("opened"));
    println!("opened in {ms:.0} ms");
    let mut project = read.project;
    let block = project.objects().next().expect("a block").clone();
    project
        .apply(RunId::new(), vec![Change::Revise { id: block.id, base: block.revision, name: Some("block 0 moved".into()), layer: None, attributes: None, body: None }])
        .expect("renamed");
    let changed = store.save(&Snapshot::of(&project), "[]", true).expect("saved");
    println!("save after one change: {:.0} ms, {} of {} entries encoded", changed.ms, changed.encoded, changed.entries);
    let unchanged = store.save(&Snapshot::of(&project), "[]", false).expect("saved");
    assert!(unchanged.unchanged);
    println!("save with nothing changed: {:.0} ms, nothing written", unchanged.ms);
}
