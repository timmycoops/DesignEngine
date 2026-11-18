//! A layer's filter at a pit's scale (decision 0088, workstream 3): 42,000 block solids on one layer, as PIT B holds,
//! filtered by attributes and by volume, and what a run costs with a volume filter on, since every run works out again
//! what the filters leave out. Run with `cargo test --release -p zd-core --test layer_filter_timing -- --ignored --nocapture`.

use std::collections::BTreeMap;
use std::time::Instant;

use zd_core::{AttrValue, Attributes, Body, Change, Project, RunId, Solid, TriMesh};

const SOLIDS: usize = 42_000;

/// A box `w` across, `h` high, at mine coordinates by its index, wound outward.
fn block(k: usize, h: f64) -> TriMesh {
    let (x0, y0, z0) = (650_000.0 + (k % 200) as f64 * 25.0, 7_450_000.0 + (k / 200) as f64 * 25.0, 100.0);
    let (x1, y1, z1) = (x0 + 20.0, y0 + 20.0, z0 + h);
    let vertices = vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
    let triangles = vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]];
    TriMesh::new(vertices, triangles)
}

#[test]
#[ignore = "a timing at a pit's scale: run with --release --ignored"]
fn a_layers_filter_at_a_pits_scale() {
    let mut project = Project::new();
    let seams = ["U", "M", "L"];
    let changes: Vec<Change> = (0..SOLIDS)
        .map(|k| {
            let mut attributes = Attributes::new();
            attributes.insert("strip".into(), AttrValue::Number((k % 15 + 1) as f64));
            attributes.insert("seam".into(), AttrValue::Text(seams[k % 3].into()));
            // Heights 0.5 m to 10 m: volumes 200 m³ to 4,000 m³.
            let h = 0.5 + (k % 20) as f64 * 0.5;
            Change::Create { name: format!("block {k}"), layer: "blocks".into(), attributes, body: Body::Solid(Solid::new(block(k, h)).unwrap()) }
        })
        .collect();
    project.apply(RunId::new(), changes).unwrap();
    let layer = project.layer_named("blocks").unwrap().id;
    let when = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>();
    let timed = |project: &mut Project, what: &str, when: BTreeMap<String, String>| {
        let start = Instant::now();
        project.set_layer_filter(layer, when);
        println!("{SOLIDS} solids, {what}: {:.1} ms, {} left out", start.elapsed().as_secs_f64() * 1e3, project.filtered_out().len());
    };
    // Seam U of strips 1 to 5: a third of a third kept.
    timed(&mut project, "seam: U; strip: 1-5", when(&[("seam", "U"), ("strip", "1-5")]));
    assert_eq!(project.filtered_out().len(), SOLIDS - (0..SOLIDS).filter(|k| k % 3 == 0 && k % 15 < 5).count());
    // Under 500 m³: heights 0.5 and 1.0 m (200 and 400 m³), 2 in 20 kept.
    timed(&mut project, "volume: <500", when(&[("volume", "<500")]));
    assert_eq!(project.filtered_out().len(), SOLIDS - SOLIDS / 10);
    // A run with the volume filter on: one solid renamed, and every filter read again.
    let one = project.objects().next().unwrap().id;
    let start = Instant::now();
    project.apply(RunId::new(), vec![Change::Revise { id: one, base: 1, name: Some("renamed".into()), layer: None, attributes: None, body: None }]).unwrap();
    println!("{SOLIDS} solids, a run with volume: <500 on: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    timed(&mut project, "cleared", when(&[]));
    assert!(project.filtered_out().is_empty());
}
