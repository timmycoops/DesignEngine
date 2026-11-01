//! Opt-in checks against real Vulcan grids. They are client data and are never committed. Point
//! the environment at them and run
//! `cargo test --release -p zd-io --test vulcan_golden -- --ignored --nocapture`:
//!
//! - `ZD_VULCAN_GRIDS`: the folder of grids
//! - `ZD_VULCAN_PREFIX`: the project prefix on the file names (not needed by the last test)
//! - `ZD_VULCAN_SEAM`: a seam with `sr`, `sf`, `tk` and `st` grids from one model (for the first test)
//! - `ZD_VULCAN_EXPORT` and `ZD_VULCAN_EXPORT_GRID`: a grid exported from Vulcan as XYZ or CSV text, and
//!   the grid file it came from (for the node-for-node test; nothing else needed)

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use zd_io::{VulcanGrid, XyzOptions, read_vulcan_grid, read_vulcan_seam, read_xyz_grid, vulcan_grid_name};

fn env(name: &str) -> Option<String> {
    let value = std::env::var(name).ok();
    if value.is_none() {
        eprintln!("{name} is not set; skipped");
    }
    value
}

/// Live values of a grid's nodes.
fn live(z: &[f64]) -> impl Iterator<Item = f64> + '_ {
    z.iter().copied().filter(|v| !v.is_nan())
}

/// The independent check of the format: Vulcan's own text export of a grid must match what we read
/// from the binary file at every node, value for value (the export writes single-precision values in
/// full, so equality is exact), with nulls in the same places.
/// What every roof, floor and topography in `ZD_VULCAN_GRIDS` holds at nodes flagged off, and what becomes
/// of it on a surface.
#[test]
#[ignore]
fn held_heights_in_a_folder() {
    let Some(dir) = env("ZD_VULCAN_GRIDS") else { return };
    let (mut grids, mut held, mut kept, mut rough) = (0, 0, 0, 0);
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if !["srg", "sfg", "tpg"].contains(&ext.as_str()) {
            continue;
        }
        let Ok(read) = read_vulcan_grid(&path) else { continue };
        grids += 1;
        if read.log.held == 0 {
            continue;
        }
        held += 1;
        let surface = read.surface();
        assert_eq!(surface.grid.null_count(), read.log.nulls - read.log.held);
        kept += surface.kept;
        if let Some([edge, live]) = surface.rough {
            rough += 1;
            println!("large steps: {} of {} nodes, {edge:.2} m against {live:.2} m, {:.1} times", read.log.held, read.log.nodes, edge / live);
        }
    }
    println!("{grids} surfaces read, {held} hold heights at nodes flagged off, {kept} nodes kept, {rough} with large steps");
}

#[test]
#[ignore]
fn grid_matches_vulcan_export_node_for_node() {
    let (Some(export), Some(file)) = (env("ZD_VULCAN_EXPORT"), env("ZD_VULCAN_EXPORT_GRID")) else { return };
    let exported = read_xyz_grid(&PathBuf::from(export), &XyzOptions::default()).unwrap();
    let read = read_vulcan_grid(&PathBuf::from(file)).unwrap();
    let (e, g) = (&exported.grid, &read.grid);
    println!(
        "export: {} x {} nodes from ({}, {}) at {} m, {} points; binary: {} x {} from ({}, {}), {} blocks unpacked",
        e.cols, e.rows, e.origin[0], e.origin[1], e.cell_size[0], exported.log.points_read, g.cols, g.rows, g.origin[0], g.origin[1], read.log.compressed_blocks
    );
    assert_eq!(e.cell_size, g.cell_size, "cell size");

    // The export may cover only part of the lattice (it skips null rows and columns at the edges).
    let offset = |a: f64, b: f64, cell: f64| {
        let steps = (a - b) / cell;
        assert_eq!(steps, steps.round(), "export nodes sit on the binary grid's lattice");
        steps as usize
    };
    let (dc, dr) = (offset(e.origin[0], g.origin[0], g.cell_size[0]), offset(e.origin[1], g.origin[1], g.cell_size[1]));
    assert!(dc + e.cols <= g.cols && dr + e.rows <= g.rows, "export inside the binary grid");
    let (mut equal, mut differ, mut only_export, mut only_binary, mut worst) = (0, 0, 0, 0, 0.0f64);
    for row in 0..g.rows {
        for col in 0..g.cols {
            let inside = col >= dc && col < dc + e.cols && row >= dr && row < dr + e.rows;
            let x = if inside { e.z_at(col - dc, row - dr) } else { f64::NAN };
            let b = g.z_at(col, row);
            match (x.is_nan(), b.is_nan()) {
                (true, true) => {}
                (false, true) => only_export += 1,
                (true, false) => only_binary += 1,
                (false, false) if x == b => equal += 1,
                (false, false) => {
                    differ += 1;
                    worst = worst.max((x - b).abs());
                }
            }
        }
    }
    println!("equal {equal}, different {differ} (worst {worst}), live only in export {only_export}, live only in binary {only_binary}");
    assert_eq!((differ, only_export, only_binary), (0, 0, 0));
    assert!(equal > 0);
}

#[test]
#[ignore]
fn one_seam_reads_and_agrees_with_itself() {
    let (Some(dir), Some(prefix), Some(seam_code)) = (env("ZD_VULCAN_GRIDS"), env("ZD_VULCAN_PREFIX"), env("ZD_VULCAN_SEAM")) else {
        return;
    };
    let start = Instant::now();
    let seam = read_vulcan_seam(&PathBuf::from(dir), &prefix, &seam_code).unwrap();
    println!("{} grids read in {:.0} ms; skipped {:?}", seam.grids.len(), start.elapsed().as_secs_f64() * 1e3, seam.skipped);

    let roof = &seam.roof().unwrap().grid;
    let floor = &seam.floor().unwrap().grid;
    println!("lattice {} x {} at {} m", roof.cols, roof.rows, roof.cell_size[0]);
    assert_eq!(roof.z.iter().zip(&floor.z).filter(|(r, f)| f > r).count(), 0, "floor never above roof");

    // Thickness and structural thickness from one model run are written separately; they must agree
    // node for node.
    let tk = &seam.grids["tk"].grid;
    let st = &seam.grids["st"].grid;
    let pairs: Vec<(f64, f64)> = tk.z.iter().zip(&st.z).filter(|(a, b)| !a.is_nan() && !b.is_nan()).map(|(a, b)| (*a, *b)).collect();
    let equal = pairs.iter().filter(|(a, b)| (a - b).abs() < 1e-3).count();
    println!("tk vs st: {equal} of {} shared live nodes equal within 1 mm", pairs.len());
    assert!(!pairs.is_empty() && equal == pairs.len());

    // Physical bounds for qualities present: raw ash %, relative density, total sulphur %, volatiles %.
    for (code, low, high) in [("ra", 0.0, 100.0), ("rd", 1.2, 2.5), ("ts", 0.0, 5.0), ("rv", 0.0, 60.0)] {
        let Some(grid) = seam.grids.get(code) else { continue };
        let (min, max) = live(&grid.grid.z).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
        println!("{code}: {min:.3} .. {max:.3}");
        assert!(min >= low && max <= high, "{code} outside {low}..{high}");
    }
}

#[test]
#[ignore]
fn every_seam_with_roof_and_floor_reads() {
    let (Some(dir), Some(prefix)) = (env("ZD_VULCAN_GRIDS"), env("ZD_VULCAN_PREFIX")) else { return };
    let dir = PathBuf::from(dir);
    let prefix = prefix.to_ascii_lowercase();
    let mut variables: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if let Some((stem, variable)) = vulcan_grid_name(&name)
            && let Some(seam) = stem.strip_prefix(&prefix)
        {
            variables.entry(seam.to_string()).or_default().insert(variable);
        }
    }
    let seams: Vec<&String> = variables.iter().filter(|(_, v)| v.contains("sr") && v.contains("sf")).map(|(s, _)| s).collect();
    let start = Instant::now();
    let (mut grids, mut skipped, mut crossed) = (0, 0, 0);
    for seam in &seams {
        let set = read_vulcan_seam(&dir, &prefix, seam).unwrap_or_else(|e| panic!("{seam}: {e}"));
        grids += set.grids.len();
        skipped += set.skipped.len();
        crossed += usize::from(set.roof().unwrap().grid.z.iter().zip(&set.floor().unwrap().grid.z).any(|(r, f)| f > r));
    }
    println!(
        "{} seams, {grids} grids read in {:.1} s; {skipped} skipped; seams with floor above roof: {crossed}",
        seams.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(!seams.is_empty());
}

/// Every grid file in the folder reads, compressed or plain. Where a seam's roof, floor and structural
/// thickness (`st`) include a compressed grid, prints how far roof minus floor is from the thickness:
/// grids written from one model run agree exactly, which a wrong decoder could not manage.
#[test]
#[ignore]
fn every_grid_reads_including_compressed() {
    let Some(dir) = env("ZD_VULCAN_GRIDS") else { return };
    // Grids are a few MB each and a folder can hold thousands, so the first pass keeps only file names.
    let mut by_stem: BTreeMap<String, BTreeMap<String, (PathBuf, bool)>> = BTreeMap::new();
    let (mut plain, mut compressed, mut compressed_ms, mut failed) = (0, 0, 0.0, Vec::new());
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let Some((stem, variable)) = path.file_name().and_then(|n| vulcan_grid_name(&n.to_string_lossy())) else { continue };
        let start = Instant::now();
        match read_vulcan_grid(&path) {
            Ok(grid) => {
                let is_compressed = grid.log.compressed_blocks > 0;
                if is_compressed {
                    compressed += 1;
                    compressed_ms += start.elapsed().as_secs_f64() * 1e3;
                } else {
                    plain += 1;
                }
                by_stem.entry(stem).or_default().insert(variable, (path, is_compressed));
            }
            Err(e) => failed.push(e.to_string()),
        }
    }
    println!("{plain} plain and {compressed} compressed grids read; compressed ones took {:.1} ms each", compressed_ms / f64::from(compressed.max(1)));
    for (stem, files) in &by_stem {
        let (Some(roof), Some(floor), Some(st)) = (files.get("sr"), files.get("sf"), files.get("st")) else { continue };
        if ![roof, floor, st].iter().any(|(_, c)| *c) {
            continue;
        }
        let read = |(path, _): &(PathBuf, bool)| -> VulcanGrid { read_vulcan_grid(path).unwrap() };
        let (roof, floor, st) = (read(roof), read(floor), read(st));
        let gaps: Vec<f64> = (0..roof.grid.z.len())
            .map(|i| (roof.grid.z[i] - floor.grid.z[i] - st.grid.z[i]).abs())
            .filter(|gap| !gap.is_nan())
            .collect();
        let exact = gaps.iter().filter(|&&gap| gap == 0.0).count();
        let worst = gaps.iter().copied().fold(0.0, f64::max);
        let which: Vec<&str> = ["sr", "sf", "st"].into_iter().filter(|v| files[*v].1).collect();
        println!("{stem}: sr - sf = st exactly at {exact} of {} live nodes, worst {worst:.3} m (compressed: {})", gaps.len(), which.join(" "));
    }
    assert!(failed.is_empty(), "{} grids failed to read: {failed:#?}", failed.len());
}
