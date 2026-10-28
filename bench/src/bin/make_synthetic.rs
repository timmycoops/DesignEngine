//! Writes the synthetic pit the app's bench mode loads when there is no real data:
//! `bench/out/synthetic/{topo,roof,floor}.xyz` and `design.dxf`.
//!
//! `cargo run --release -p bench --bin make_synthetic -- [cells] [cell_size_m]`
//! Defaults: 2000 cells at 5 m (a 10 km x 10 km pit, 8 million triangles per surface).

use bench::synth::{synth_design, synth_pit};
use bench::{count, out_dir, timed};
use zd_geom::triangulate_grid;
use zd_io::{write_dxf_3dfaces, write_xyz_grid};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cells: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let cell_size: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let dir = out_dir().join("synthetic");
    std::fs::create_dir_all(&dir).expect("create bench/out/synthetic");

    let pit = synth_pit(cells, cell_size);
    for (name, grid) in [("topo", &pit.topo), ("roof", &pit.roof), ("floor", &pit.floor)] {
        let path = dir.join(format!("{name}.xyz"));
        let (result, ms) = timed(|| write_xyz_grid(&path, grid));
        result.expect("write grid");
        println!("{}: {} nodes in {ms:.0} ms", path.display(), count(grid.z.len()));
    }

    // The design is a DXF so the app exercises both readers; a quarter of the resolution keeps
    // the file a sensible size.
    let design = triangulate_grid(&synth_design(cells / 4, cell_size * 4.0));
    let path = dir.join("design.dxf");
    let (result, ms) = timed(|| write_dxf_3dfaces(&path, &design, "DESIGN"));
    result.expect("write design");
    println!("{}: {} triangles in {ms:.0} ms", path.display(), count(design.triangle_count()));
}
