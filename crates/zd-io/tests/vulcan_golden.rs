//! Opt-in checks against real Vulcan grids. They are client data and are never committed. Point
//! the environment at them and run
//! `cargo test --release -p zd-io --test vulcan_golden -- --ignored --nocapture`:
//!
//! - `ZD_VULCAN_GRIDS`: the folder of grids
//! - `ZD_VULCAN_PREFIX`: the project prefix on the file names
//! - `ZD_VULCAN_SEAM`: a seam with `sr`, `sf`, `tk` and `st` grids from one model (for the first test)

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use zd_io::{read_vulcan_seam, vulcan_grid_name};

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

#[test]
#[ignore]
fn one_seam_reads_and_agrees_with_itself() {
    let (Some(dir), Some(prefix), Some(seam_code)) = (env("ZD_VULCAN_GRIDS"), env("ZD_VULCAN_PREFIX"), env("ZD_VULCAN_SEAM")) else {
        return;
    };
    let start = Instant::now();
    let seam = read_vulcan_seam(&PathBuf::from(dir), &prefix, &seam_code).unwrap();
    println!("{} grids read in {:.0} ms; skipped {:?}", seam.grids.len(), start.elapsed().as_secs_f64() * 1e3, seam.skipped);

    let roof = &seam.roof().grid;
    let floor = &seam.floor().grid;
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
        crossed += usize::from(set.roof().grid.z.iter().zip(&set.floor().grid.z).any(|(r, f)| f > r));
    }
    println!(
        "{} seams, {grids} grids read in {:.1} s; {skipped} skipped; seams with floor above roof: {crossed}",
        seams.len(),
        start.elapsed().as_secs_f64()
    );
    assert!(!seams.is_empty());
}
