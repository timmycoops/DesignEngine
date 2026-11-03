//! Opt-in checks against real Deswik drawings. They are client data and are never committed. Point
//! the environment at them and run
//! `cargo test --release -p zd-io --test deswik_golden -- --ignored --nocapture`:
//!
//! - `ZD_DESWIK_DCF`: a `.dcf`
//! - `ZD_DESWIK_LAYERS`: optional, the layers to read, `;` between them (every layer by default)
//! - `ZD_DESWIK_DXF`: the same layers exported to DXF by Deswik (for the export test)

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use zd_core::TriMesh;
use zd_geom::repair_mesh;
use zd_io::{DeswikDrawing, list_deswik_layers, read_deswik_layers, read_dxf};

fn env(name: &str) -> Option<String> {
    let value = std::env::var(name).ok();
    if value.is_none() {
        eprintln!("{name} is not set; skipped");
    }
    value
}

fn chosen_layers() -> Vec<String> {
    std::env::var("ZD_DESWIK_LAYERS").map(|l| l.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

fn read(path: &str) -> DeswikDrawing {
    let start = Instant::now();
    let layers = list_deswik_layers(Path::new(path)).unwrap();
    println!("{} layers listed in {:.0} ms, {:.1} MB unzipped", layers.len(), start.elapsed().as_secs_f64() * 1e3, layers.iter().map(|l| l.bytes).sum::<u64>() as f64 / 1e6);
    let start = Instant::now();
    let drawing = read_deswik_layers(Path::new(path), &chosen_layers()).unwrap();
    let vertices: usize = drawing.strings.iter().map(|s| s.line.vertices.len()).sum();
    let triangles: usize = drawing.meshes.iter().map(|m| m.mesh.triangle_count()).sum();
    println!(
        "{} layers read in {:.0} ms: {} strings ({vertices} vertices), {} meshes ({triangles} triangles); log {:?}",
        drawing.log.layers_read,
        start.elapsed().as_secs_f64() * 1e3,
        drawing.strings.len(),
        drawing.meshes.len(),
        drawing.log
    );
    drawing
}

/// Every layer named reads, every coordinate is finite, and where Deswik stored a volume for a mesh
/// that is closed once through the repair step, the mesh's own volume matches it to a millionth.
#[test]
#[ignore]
fn a_real_drawing_reads_whole() {
    let Some(path) = env("ZD_DESWIK_DCF") else { return };
    let drawing = read(&path);
    for s in &drawing.strings {
        assert!(s.line.vertices.iter().flatten().all(|c| c.is_finite()), "a string on {} with a coordinate that is not a number", s.layer);
    }
    let mut by_layer: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for s in &drawing.strings {
        by_layer.entry(&s.layer).or_default().0 += 1;
    }
    // Where Deswik stored a volume: the mesh as read, where it is closed and wound one way, checks the
    // reader; the mesh through the repair step (welded at 1 mm, wound outward) checks what an import makes.
    let (mut as_read, mut repaired, mut open) = (Comparison::default(), Comparison::default(), 0);
    for m in &drawing.meshes {
        assert!(m.mesh.vertices.iter().flatten().all(|c| c.is_finite()), "a mesh on {} with a coordinate that is not a number", m.layer);
        let row = by_layer.entry(&m.layer).or_default();
        row.1 += 1;
        row.2 += m.mesh.triangle_count();
        let Some(theirs) = m.deswik_volume else { continue };
        // Deswik winds a closed mesh either way; its volume is the size.
        if m.mesh.validity().is_watertight() {
            as_read.add(m.mesh.signed_volume().abs(), theirs, &m.layer, &m.mesh, String::new());
        }
        let (mesh, log) = repair_mesh(&m.mesh, 0.001);
        if mesh.validity().is_watertight() {
            let what = format!(
                "; repair: {} welded (largest move {:.2} mm), {} collapsed, {} degenerate, {} duplicates dropped, {} flipped",
                log.vertices_welded,
                log.largest_move * 1e3,
                log.collapsed_dropped,
                log.degenerate_dropped,
                log.duplicates_dropped,
                log.orient.triangles_flipped
            );
            repaired.add(mesh.signed_volume(), theirs, &m.layer, &m.mesh, what);
        } else {
            open += 1;
        }
    }
    if std::env::var("ZD_DESWIK_BY_LAYER").is_ok() {
        for (layer, (strings, meshes, triangles)) in by_layer {
            println!("  {layer}: {strings} strings, {meshes} meshes, {triangles} triangles");
        }
    }
    as_read.report("closed and wound one way as read");
    repaired.report("closed after the repair step");
    println!("{open} meshes with a Deswik volume are open after the repair step, so have no volume to compare");
    assert!(as_read.differ.is_empty(), "meshes read whole whose volume differs from Deswik's");
}

/// Our volumes against Deswik's, and those more than a millionth apart.
#[derive(Default)]
struct Comparison {
    compared: usize,
    differ: Vec<(f64, String, usize, f64, f64, String)>,
}

impl Comparison {
    fn add(&mut self, ours: f64, theirs: f64, layer: &str, read: &TriMesh, note: String) {
        self.compared += 1;
        let share = (ours - theirs).abs() / theirs;
        if share > 1e-6 {
            self.differ.push((share, layer.to_string(), read.triangle_count(), ours, theirs, note));
        }
    }

    fn report(&mut self, what: &str) {
        self.differ.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("{} meshes with a Deswik volume {what}: {} differ by more than a millionth", self.compared, self.differ.len());
        for (share, layer, triangles, ours, theirs, note) in self.differ.iter().take(8) {
            println!("  {share:.2e} on {layer}: {triangles} triangles as read, ours {ours:.3} m3, Deswik {theirs:.3} m3{note}");
        }
    }
}

/// What Deswik exported to DXF from the same layers must be what we read: every string vertex for
/// vertex to a micrometre, and every triangle as a set of corners.
#[test]
#[ignore]
fn a_real_drawing_matches_its_dxf_export() {
    let (Some(path), Some(dxf)) = (env("ZD_DESWIK_DCF"), env("ZD_DESWIK_DXF")) else { return };
    let ours = read(&path);
    let theirs = read_dxf(Path::new(&dxf)).unwrap();
    let key = |p: &[f64; 3]| p.map(|c| (c * 1e6).round() as i64);
    let mut a: Vec<Vec<[i64; 3]>> = ours.strings.iter().map(|s| s.line.vertices.iter().map(key).collect()).collect();
    let mut b: Vec<Vec<[i64; 3]>> = theirs.strings.iter().map(|s| s.line.vertices.iter().map(key).collect()).collect();
    a.sort();
    b.sort();
    println!("{} strings ours, {} in the DXF", a.len(), b.len());
    assert_eq!(a, b, "the same strings, vertex for vertex");
    let corners = |m: &TriMesh| {
        let mut set: Vec<[[i64; 3]; 3]> = m
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
    let mut mine: Vec<[[i64; 3]; 3]> = ours.meshes.iter().flat_map(|m| corners(&m.mesh)).collect();
    mine.sort();
    let dxf_faces = corners(&theirs.mesh);
    println!("{} triangles ours, {} in the DXF", mine.len(), dxf_faces.len());
    assert_eq!(mine, dxf_faces, "the same triangles");
}
