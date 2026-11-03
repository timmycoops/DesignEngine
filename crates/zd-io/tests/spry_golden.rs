//! Opt-in checks against real Spry projects. They are client data and are never committed. Point the
//! environment at them and run
//! `cargo test --release -p zd-io --test spry_golden -- --ignored --nocapture`:
//!
//! - `ZD_SPRY_FILE`: a `.spry`
//! - `ZD_SPRY_FIELDS`, `ZD_SPRY_LAYERS`: optional, the solid fields and layers to read, `;` between
//!   them (everything by default)
//! - `ZD_SPRY_VOLUME`: optional, the volume field beside each Solid field (`Volume` by default)

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use zd_core::TriMesh;
use zd_geom::repair_mesh;
use zd_io::{SpryRead, list_spry, read_spry};

fn env(name: &str) -> Option<String> {
    let value = std::env::var(name).ok();
    if value.is_none() {
        eprintln!("{name} is not set; skipped");
    }
    value
}

fn list(name: &str) -> Vec<String> {
    std::env::var(name).map(|l| l.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

/// The volume a mesh encloses, from its first vertex rather than the grid's origin: products of
/// coordinates in the millions lose a thousandth of a solid's volume to rounding.
fn signed_volume(mesh: &TriMesh) -> f64 {
    let Some(&o) = mesh.vertices.first() else { return 0.0 };
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| {
                let v = mesh.vertices[i as usize];
                [v[0] - o[0], v[1] - o[1], v[2] - o[2]]
            });
            a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
        })
        .sum::<f64>()
        / 6.0
}

/// Shares of difference from Spry's volume, and how many solids fall within each.
const BANDS: [f64; 5] = [1e-9, 1e-6, 1e-3, 1e-2, 1e-1];

/// What a solid field's solids came to beside Spry's volumes.
#[derive(Default)]
struct Compared {
    with_volume: usize,
    /// By band, those closed, manifold and wound one way as read.
    as_read: [usize; BANDS.len() + 1],
    /// By band, those closed, manifold and wound one way after the repair step.
    repaired: [usize; BANDS.len() + 1],
    not_solid: usize,
    ours: f64,
    theirs: f64,
}

/// Everything named reads, every coordinate is finite, and each node's solid is set beside Spry's own
/// volume of it, as read and once closed through the repair step, by bands of difference.
#[test]
#[ignore]
fn a_real_project_reads_whole() {
    let Some(path) = env("ZD_SPRY_FILE") else { return };
    let start = Instant::now();
    let contents = list_spry(Path::new(&path)).unwrap();
    println!("database version {}, listed in {:.0} ms", contents.database_version, start.elapsed().as_secs_f64() * 1e3);
    for f in &contents.solid_fields {
        println!("  solid field {}: {} solids, {:.1} MB", f.name, f.solids, f.bytes as f64 / 1e6);
    }
    println!("  {} layers, {:.1} MB", contents.layers.len(), contents.layers.iter().map(|l| l.bytes).sum::<u64>() as f64 / 1e6);

    let read = SpryRead { solid_fields: list("ZD_SPRY_FIELDS"), layers: list("ZD_SPRY_LAYERS"), volume_field: std::env::var("ZD_SPRY_VOLUME").unwrap_or_else(|_| "Volume".into()) };
    let start = Instant::now();
    let project = read_spry(Path::new(&path), &read).unwrap();
    let vertices: usize = project.strings.iter().map(|s| s.line.vertices.len()).sum();
    println!(
        "read in {:.0} ms: {} strings ({vertices} vertices), {} meshes; log {:#?}",
        start.elapsed().as_secs_f64() * 1e3,
        project.strings.len(),
        project.meshes.len(),
        project.log
    );
    for s in &project.strings {
        assert!(s.line.vertices.iter().flatten().all(|c| c.is_finite()), "a string on {} with a coordinate that is not a number", s.layer);
    }
    for m in &project.meshes {
        assert!(m.mesh.vertices.iter().flatten().all(|c| c.is_finite()), "a mesh on {} with a coordinate that is not a number", m.layer);
    }

    let start = Instant::now();
    let mut by_field: BTreeMap<&str, Compared> = BTreeMap::new();
    let mut largest: Vec<(f64, f64, f64, String)> = Vec::new();
    for m in &project.meshes {
        let Some(theirs) = m.spry_volume else { continue };
        let row = by_field.entry(&m.layer).or_default();
        let band = |ours: f64| BANDS.iter().position(|&b| (ours - theirs).abs() <= b * theirs.abs().max(1e-9)).unwrap_or(BANDS.len());
        row.with_volume += 1;
        let read = m.mesh.validity();
        if read.closed && read.manifold && read.consistent_winding {
            row.as_read[band(signed_volume(&m.mesh).abs())] += 1;
        }
        let (welded, _) = repair_mesh(&m.mesh, 1e-3);
        let v = welded.validity();
        if !(v.closed && v.manifold && v.consistent_winding) {
            row.not_solid += 1;
            continue;
        }
        let ours = signed_volume(&welded);
        row.repaired[band(ours)] += 1;
        row.ours += ours;
        row.theirs += theirs;
        largest.push(((ours - theirs).abs(), ours, theirs, m.node.clone().unwrap_or_default()));
    }
    println!("compared in {:.0} ms; bands of |ours - Spry's| / Spry's: {BANDS:?} and over", start.elapsed().as_secs_f64() * 1e3);
    for (field, c) in &by_field {
        println!(
            "  {field}: {} with a volume; closed as read {:?}; through the repair step {:?}, {} not closed, manifold and wound one way after it; sum of those {:.1} m3 here, {:.1} m3 in Spry",
            c.with_volume, c.as_read, c.repaired, c.not_solid, c.ours, c.theirs
        );
    }
    largest.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (d, ours, theirs, node) in largest.iter().take(10) {
        println!("  largest: {node}: {ours:.3} m3 here, {theirs:.3} m3 in Spry ({d:.3} m3)");
    }
}
