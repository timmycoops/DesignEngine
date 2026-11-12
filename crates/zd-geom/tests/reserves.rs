//! Reserves inside a solid (decision 0061, workstream 1): known answers worked by hand on made-up blocks at mine-grid
//! coordinates. Volumes to 0.001 m³, RD to 1e-9, qualities to 1 part in a million.

use zd_core::{Lattice, TriMesh};
use zd_geom::{HorizonGrids, columns_in_solid, composite, horizon_samples, in_situ_rd};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// A block from `(x0, y0)` 50 m east and 40 m north, its floor at `z0` and its top at `top(x)` along it: wound outward.
fn block(x0: f64, y0: f64, z0: f64, top: impl Fn(f64) -> f64) -> TriMesh {
    let vertices = (0..8)
        .map(|i| {
            let x = x0 + (i & 1) as f64 * 50.0;
            let y = y0 + (i >> 1 & 1) as f64 * 40.0;
            [x, y, if i >> 2 & 1 == 1 { top(x) } else { z0 }]
        })
        .collect();
    let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    TriMesh::new(vertices, triangles)
}

/// The block 3 m thick, floor at RL 100.
fn flat() -> TriMesh {
    block(E, N, 100.0, |_| 103.0)
}

/// 5 m cells from 20 m south-west of the block, so grid lines run along its sides.
fn on_its_sides() -> Lattice {
    Lattice { origin: [E - 20.0, N - 20.0], cell_size: [5.0, 5.0], cols: 30, rows: 30 }
}

/// 5 m cells with their nodes in the middle of the block's 5 m squares: 10 by 8 of them, none on a side.
fn in_its_squares() -> Lattice {
    Lattice { origin: [E - 17.5, N - 17.5], cell_size: [5.0, 5.0], cols: 30, rows: 30 }
}

/// A grid on `lattice` with `value(x, y)` at each node.
fn grid(lattice: &Lattice, value: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    (0..lattice.rows)
        .flat_map(|r| (0..lattice.cols).map(move |c| (c, r)))
        .map(|(c, r)| value(lattice.origin[0] + c as f64 * lattice.cell_size[0], lattice.origin[1] + r as f64 * lattice.cell_size[1]))
        .collect()
}

fn near(a: f64, b: f64, tolerance: f64, what: &str) {
    assert!((a - b).abs() <= tolerance, "{what}: {a} against {b}");
}

#[test]
fn a_flat_block_takes_its_plan_area_in_nodes_and_its_thickness_at_each() {
    let lattice = on_its_sides();
    let columns = columns_in_solid(&flat(), &lattice);
    // 50 x 40 m at 5 m: 10 x 8 nodes, those on the west and north sides taken and not the east and south.
    assert_eq!(columns.len(), 80);
    for c in &columns {
        assert_eq!(c.spans, vec![[100.0, 103.0]], "node {}", c.node);
        let (col, row) = (c.node % lattice.cols, c.node / lattice.cols);
        let (x, y) = (lattice.origin[0] + col as f64 * 5.0 - E, lattice.origin[1] + row as f64 * 5.0 - N);
        assert!((0.0..50.0).contains(&x) && (0.0..=40.0).contains(&y) && y > 0.0, "node at {x}, {y} m in the block");
    }
    let samples = horizon_samples(&columns, &vec![90.0; 900], &vec![110.0; 900]);
    let c = composite(&samples, 25.0, &HorizonGrids { rd: &vec![1.4; 900], qualities: &[], moisture: None });
    near(c.area, 2000.0, 1e-9, "plan area");
    near(c.sampled_volume, 6000.0, 1e-6, "sampled volume");
    near(flat().signed_volume(), 6000.0, 1e-6, "the block's own volume");
}

#[test]
fn a_quality_rising_across_the_block_composites_to_its_value_at_the_middle() {
    let lattice = in_its_squares();
    let columns = columns_in_solid(&flat(), &lattice);
    assert_eq!(columns.len(), 80);
    // Ash from 10 % at the west side, rising 0.1 % a metre east: 12.5 % in the middle.
    let ash = grid(&lattice, |x, _| 10.0 + 0.1 * (x - E));
    let samples = horizon_samples(&columns, &vec![90.0; 900], &vec![110.0; 900]);
    let c = composite(&samples, 25.0, &HorizonGrids { rd: &vec![1.4; 900], qualities: &[&ash], moisture: None });
    near(c.qualities[0].unwrap(), 12.5, 12.5e-6, "ash");
    near(c.rd_ad.unwrap(), 1.4, 1e-9, "RD");
}

#[test]
fn rd_is_composited_by_thickness_and_qualities_by_mass() {
    let lattice = in_its_squares();
    let columns = columns_in_solid(&flat(), &lattice);
    // RD 1.40 on the west half, 1.50 on the east; ash 10 % west, 20 % east; equal thickness throughout.
    let rd = grid(&lattice, |x, _| if x < E + 25.0 { 1.40 } else { 1.50 });
    let ash = grid(&lattice, |x, _| if x < E + 25.0 { 10.0 } else { 20.0 });
    let samples = horizon_samples(&columns, &vec![90.0; 900], &vec![110.0; 900]);
    let c = composite(&samples, 25.0, &HorizonGrids { rd: &rd, qualities: &[&ash], moisture: None });
    near(c.rd_ad.unwrap(), 1.45, 1e-9, "RD by thickness");
    // By mass: (1.40 × 10 + 1.50 × 20) / (1.40 + 1.50) = 44 / 2.9.
    near(c.qualities[0].unwrap(), 44.0 / 2.9, 1e-6 * 15.0, "ash by mass");
}

#[test]
fn in_situ_rd_is_preston_and_sanders() {
    // RD 1.45 air-dried, 3 % air-dried moisture, 8 % in situ: 1.45 × 97 / (100 + 1.45 × 5 − 8) = 140.65 / 99.25.
    near(in_situ_rd(1.45, 3.0, 8.0), 1.417_128_463_5, 1e-9, "in-situ RD");
    near(in_situ_rd(1.45, 5.0, 5.0), 1.45, 1e-12, "no more moisture in situ than air-dried: the same RD");
    let lattice = in_its_squares();
    let samples = horizon_samples(&columns_in_solid(&flat(), &lattice), &vec![90.0; 900], &vec![110.0; 900]);
    let (rd, is, ad) = (vec![1.45; 900], vec![8.0; 900], vec![3.0; 900]);
    let c = composite(&samples, 25.0, &HorizonGrids { rd: &rd, qualities: &[], moisture: Some((&is, &ad)) });
    near(c.rd_is.unwrap(), 140.65 / 99.25, 1e-9, "the block's in-situ RD");
    // In situ is less dense, so the same volume weighs less: 6,000 m³ at 1.4171 is 8,502.8 t, not 8,700 t.
    near(c.sampled_volume * c.rd_is.unwrap(), 6000.0 * 140.65 / 99.25, 1e-6, "tonnes in situ");
}

#[test]
fn a_block_across_two_horizons_splits_by_their_roofs_and_floors() {
    // A block from RL 100 to 110; horizon A from 107 up, horizon B from 95 to 107: 30 : 70.
    let mesh = block(E, N, 100.0, |_| 110.0);
    let lattice = in_its_squares();
    let columns = columns_in_solid(&mesh, &lattice);
    let a = horizon_samples(&columns, &vec![107.0; 900], &vec![115.0; 900]);
    let b = horizon_samples(&columns, &vec![95.0; 900], &vec![107.0; 900]);
    let rd = vec![1.4; 900];
    let grids = HorizonGrids { rd: &rd, qualities: &[], moisture: None };
    let (va, vb) = (composite(&a, 25.0, &grids).sampled_volume, composite(&b, 25.0, &grids).sampled_volume);
    near(va, 6000.0, 1e-6, "in A");
    near(vb, 14_000.0, 1e-6, "in B");
    near((va + vb) / mesh.signed_volume(), 1.0, 1e-12, "together, the block");
    // Where B's roof is null (its east half), the block holds none of B there.
    let roof = grid(&lattice, |x, _| if x < E + 25.0 { 107.0 } else { f64::NAN });
    near(composite(&horizon_samples(&columns, &vec![95.0; 900], &roof), 25.0, &grids).sampled_volume, 7000.0, 1e-6, "B's west half");
}

#[test]
fn a_sloping_top_and_a_solid_in_two_pieces_are_sampled_as_they_lie() {
    // A wedge 3 m thick at the west side, 8 m at the east: 50 × 40 × 5.5 = 11,000 m³, which nodes in the middle of
    // each 5 m square give exactly.
    let wedge = block(E, N, 100.0, |x| 103.0 + 0.1 * (x - E));
    let lattice = in_its_squares();
    let rd = vec![1.4; 900];
    let grids = HorizonGrids { rd: &rd, qualities: &[], moisture: None };
    let all = |m: &TriMesh| horizon_samples(&columns_in_solid(m, &lattice), &vec![0.0; 900], &vec![500.0; 900]);
    near(wedge.signed_volume(), 11_000.0, 1e-6, "the wedge");
    near(composite(&all(&wedge), 25.0, &grids).sampled_volume, 11_000.0, 1e-6, "the wedge sampled");
    // Two 3 m blocks one over the other with 2 m between, as one mesh: two stretches at every node.
    let mut two = block(E, N, 100.0, |_| 103.0);
    let upper = block(E, N, 105.0, |_| 108.0);
    let offset = two.vertices.len() as u32;
    two.vertices.extend(upper.vertices);
    two.triangles.extend(upper.triangles.iter().map(|t| t.map(|i| i + offset)));
    let columns = columns_in_solid(&two, &lattice);
    assert!(columns.iter().all(|c| c.spans == vec![[100.0, 103.0], [105.0, 108.0]]));
    near(composite(&all(&two), 25.0, &grids).sampled_volume, 12_000.0, 1e-6, "both pieces");
}

#[test]
fn nodes_with_no_rd_or_no_value_are_left_out_and_counted() {
    let lattice = in_its_squares();
    let samples = horizon_samples(&columns_in_solid(&flat(), &lattice), &vec![90.0; 900], &vec![110.0; 900]);
    // No RD in the block's south row of nodes (10 of 80), and no ash in its west column (8, one of them in that row).
    let rd = grid(&lattice, |_, y| if y < N + 5.0 { f64::NAN } else { 1.4 });
    let ash = grid(&lattice, |x, _| if x < E + 5.0 { f64::NAN } else { 12.0 });
    let c = composite(&samples, 25.0, &HorizonGrids { rd: &rd, qualities: &[&ash], moisture: None });
    assert_eq!(c.nodes, 80);
    assert_eq!(c.nodes_without_rd, 10);
    assert_eq!(c.nodes_without, vec![10 + 7]);
    near(c.qualities[0].unwrap(), 12.0, 1e-12, "ash where it's known");
    near(c.sampled_volume, 6000.0, 1e-6, "every node still counts in the volume");
}

/// The columns of 43,000 made-up blocks, laid out as the store bench lays them (400 across, 50 × 40 m, 3 m thick), on a
/// 10 m lattice over them all: what workstream 2's run costs in sampling. Prints the time; run with `--ignored`.
#[test]
#[ignore]
fn columns_of_43000_blocks() {
    let blocks: Vec<TriMesh> = (0..43_000).map(|i| block(E + (i % 400) as f64 * 50.0, N + (i / 400) as f64 * 40.0, 100.0, |_| 103.0)).collect();
    let lattice = Lattice { origin: [E - 5.0, N - 5.0], cell_size: [10.0, 10.0], cols: 2_002, rows: 434 };
    let start = std::time::Instant::now();
    let nodes: usize = blocks.iter().map(|b| columns_in_solid(b, &lattice).len()).sum();
    let ms = start.elapsed().as_secs_f64() * 1e3;
    println!("43,000 blocks: {nodes} node columns in {ms:.0} ms on one thread");
    assert_eq!(nodes, 43_000 * 20, "5 x 4 nodes a block");
}
