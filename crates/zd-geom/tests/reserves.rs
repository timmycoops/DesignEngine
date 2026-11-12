//! Interrogating a block against a seam model (decision 0061, workstream 1): known answers worked by hand on made-up
//! blocks at mine-grid coordinates, each the size a coal mine usually cuts a block to: 60 m across its strip and 100 m
//! along it. Volumes to 0.001 m³, RD to 1e-9, qualities to 1 part in a million.

use zd_core::{Lattice, TriMesh};
use zd_geom::{Sample, Variable, columns_in_solid, composite, horizon_samples, in_situ_rd, solid_samples};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;
/// A block's width across its strip (m), east, and its length along it, north.
const WIDE: f64 = 60.0;
const LONG: f64 = 100.0;

/// A block from `(x0, y0)` 60 m east and 100 m north, its floor at `z0` and its top at `top(x)` across it: wound outward.
fn block(x0: f64, y0: f64, z0: f64, top: impl Fn(f64) -> f64) -> TriMesh {
    let vertices = (0..8)
        .map(|i| {
            let x = x0 + (i & 1) as f64 * WIDE;
            let y = y0 + (i >> 1 & 1) as f64 * LONG;
            [x, y, if i >> 2 & 1 == 1 { top(x) } else { z0 }]
        })
        .collect();
    let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    TriMesh::new(vertices, triangles)
}

/// The block 3 m thick, floor at RL 100: 6,000 m² and 18,000 m³.
fn flat() -> TriMesh {
    block(E, N, 100.0, |_| 103.0)
}

/// A wedge 3 m thick at its west side and 8 m at its east: 5.5 m on average, 33,000 m³. Its west half is 4.25 m thick on
/// average and its east half 6.75 m.
fn wedge() -> TriMesh {
    block(E, N, 100.0, |x| 103.0 + 5.0 / 60.0 * (x - E))
}

/// 5 m cells from 20 m south-west of the block, so grid lines run along its sides.
fn on_its_sides() -> Lattice {
    Lattice { origin: [E - 20.0, N - 20.0], cell_size: [5.0, 5.0], cols: 30, rows: 30 }
}

/// 5 m cells with their nodes in the middle of the block's 5 m squares: 12 by 20 of them, none on a side.
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

/// West of the block's middle, or east.
fn west(x: f64) -> bool {
    x < E + WIDE / 2.0
}

fn near(a: f64, b: f64, tolerance: f64, what: &str) {
    assert!((a - b).abs() <= tolerance, "{what}: {a} against {b}");
}

/// A block's own samples: its whole thickness at each node.
fn whole(mesh: &TriMesh, lattice: &Lattice) -> Vec<Sample> {
    solid_samples(&columns_in_solid(mesh, lattice))
}

/// A variable averaged by volume times the variables at `weights`.
fn by<'a>(grid: &'a [f64], weights: &'a [usize]) -> Variable<'a> {
    Variable { grid, weight_by: weights }
}

#[test]
fn a_flat_block_takes_its_plan_area_in_nodes_and_its_thickness_at_each() {
    let lattice = on_its_sides();
    let columns = columns_in_solid(&flat(), &lattice);
    // 60 x 100 m at 5 m: 12 x 20 nodes, those on the west and north sides taken and not the east and south.
    assert_eq!(columns.len(), 240);
    for c in &columns {
        assert_eq!(c.spans, vec![[100.0, 103.0]], "node {}", c.node);
        let (col, row) = (c.node % lattice.cols, c.node / lattice.cols);
        let (x, y) = (lattice.origin[0] + col as f64 * 5.0 - E, lattice.origin[1] + row as f64 * 5.0 - N);
        assert!((0.0..WIDE).contains(&x) && (0.0..=LONG).contains(&y) && y > 0.0, "node at {x}, {y} m in the block");
    }
    let c = composite(&whole(&flat(), &lattice), 25.0, &[]);
    near(c.area, 6000.0, 1e-9, "plan area");
    near(c.sampled_volume, 18_000.0, 1e-6, "sampled volume");
    near(flat().signed_volume(), 18_000.0, 1e-6, "the block's own volume");
}

#[test]
fn a_quality_rising_across_the_block_averages_to_its_value_at_the_middle() {
    let lattice = in_its_squares();
    // Ash from 10 % at the west side, rising 0.1 % a metre east, by coal tonnes: 13 % in the middle, 30 m across.
    let (rd, ash) = (vec![1.4; 900], grid(&lattice, |x, _| 10.0 + 0.1 * (x - E)));
    let c = composite(&whole(&flat(), &lattice), 25.0, &[by(&rd, &[]), by(&ash, &[0])]);
    near(c.values[0].unwrap(), 1.4, 1e-9, "RD");
    near(c.values[1].unwrap(), 13.0, 13e-6, "ash");
}

#[test]
fn density_is_by_volume_and_a_quality_by_coal_tonnes() {
    let lattice = in_its_squares();
    // On the wedge: RD 1.40 west, 1.50 east; ash 10 % west, 20 % east.
    let rd = grid(&lattice, |x, _| if west(x) { 1.40 } else { 1.50 });
    let ash = grid(&lattice, |x, _| if west(x) { 10.0 } else { 20.0 });
    let c = composite(&whole(&wedge(), &lattice), 25.0, &[by(&rd, &[]), by(&ash, &[0])]);
    // By volume: (1.40 × 4.25 + 1.50 × 6.75) / 11 = 16.075 / 11.
    near(c.values[0].unwrap(), 16.075 / 11.0, 1e-9, "RD by volume");
    // By coal tonnes: (1.40 × 4.25 × 10 + 1.50 × 6.75 × 20) / (1.40 × 4.25 + 1.50 × 6.75) = 262.0 / 16.075.
    near(c.values[1].unwrap(), 262.0 / 16.075, 1e-6 * 17.0, "ash by tonnes");
}

#[test]
fn a_variable_is_averaged_by_whatever_it_names_as_its_weights() {
    // RD 1.4 west, 1.6 east; CV 25 MJ/kg west, 21 east; thickness the same throughout.
    let lattice = in_its_squares();
    let rd = grid(&lattice, |x, _| if west(x) { 1.4 } else { 1.6 });
    let cv = grid(&lattice, |x, _| if west(x) { 25.0 } else { 21.0 });
    let samples = whole(&flat(), &lattice);
    // By coal tonnes: (1.4 × 25 + 1.6 × 21) / 3.0 = 68.6 / 3. By volume alone, as someone may want: the plain mean, 23.
    near(composite(&samples, 25.0, &[by(&rd, &[]), by(&cv, &[0])]).values[1].unwrap(), 68.6 / 3.0, 1e-9, "by tonnes");
    near(composite(&samples, 25.0, &[by(&cv, &[])]).values[0].unwrap(), 23.0, 1e-9, "by volume");
    // By as many variables as it names: the RD and a weight grid of 2 west and 1 east, (2.8 × 25 + 1.6 × 21) / 4.4.
    let weight = grid(&lattice, |x, _| if west(x) { 2.0 } else { 1.0 });
    near(composite(&samples, 25.0, &[by(&rd, &[]), by(&weight, &[]), by(&cv, &[0, 1])]).values[2].unwrap(), 103.6 / 4.4, 1e-9, "by both");
}

#[test]
fn in_situ_rd_is_preston_and_sanders() {
    // For a step after the interrogation. RD 1.45 air-dried, 3 % air-dried moisture, 8 % in situ:
    // 1.45 × 97 / (100 + 1.45 × 5 − 8) = 140.65 / 99.25.
    near(in_situ_rd(1.45, 3.0, 8.0), 1.417_128_463_5, 1e-9, "in-situ RD");
    near(in_situ_rd(1.45, 5.0, 5.0), 1.45, 1e-12, "no more moisture in situ than air-dried: the same RD");
}

#[test]
fn how_much_of_a_block_lies_in_a_seam_is_found_from_its_roof_and_floor() {
    // A check that a block is in its seam. A block from RL 100 to 110; seam A from 107 up, seam B from 95 to 107: 30 : 70.
    let mesh = block(E, N, 100.0, |_| 110.0);
    let lattice = in_its_squares();
    let columns = columns_in_solid(&mesh, &lattice);
    let a = horizon_samples(&columns, &vec![107.0; 900], &vec![115.0; 900]);
    let b = horizon_samples(&columns, &vec![95.0; 900], &vec![107.0; 900]);
    let (va, vb) = (composite(&a, 25.0, &[]).sampled_volume, composite(&b, 25.0, &[]).sampled_volume);
    near(va, 18_000.0, 1e-6, "in A");
    near(vb, 42_000.0, 1e-6, "in B");
    near((va + vb) / mesh.signed_volume(), 1.0, 1e-12, "together, the block");
    // Where B's roof is null (its east half), the block holds none of B there.
    let roof = grid(&lattice, |x, _| if west(x) { 107.0 } else { f64::NAN });
    near(composite(&horizon_samples(&columns, &vec![95.0; 900], &roof), 25.0, &[]).sampled_volume, 21_000.0, 1e-6, "B's west half");
}

#[test]
fn a_sloping_top_and_a_solid_in_two_pieces_are_sampled_as_they_lie() {
    // The wedge's 33,000 m³, which nodes in the middle of each 5 m square give exactly.
    let lattice = in_its_squares();
    near(wedge().signed_volume(), 33_000.0, 1e-6, "the wedge");
    near(composite(&whole(&wedge(), &lattice), 25.0, &[]).sampled_volume, 33_000.0, 1e-6, "the wedge sampled");
    // Two 3 m blocks one over the other with 2 m between, as one mesh: two stretches at every node.
    let mut two = block(E, N, 100.0, |_| 103.0);
    let upper = block(E, N, 105.0, |_| 108.0);
    let offset = two.vertices.len() as u32;
    two.vertices.extend(upper.vertices);
    two.triangles.extend(upper.triangles.iter().map(|t| t.map(|i| i + offset)));
    let columns = columns_in_solid(&two, &lattice);
    assert!(columns.iter().all(|c| c.spans == vec![[100.0, 103.0], [105.0, 108.0]]));
    near(composite(&whole(&two, &lattice), 25.0, &[]).sampled_volume, 36_000.0, 1e-6, "both pieces");
}

#[test]
fn nodes_with_no_value_or_no_weight_are_left_out_and_counted() {
    let lattice = in_its_squares();
    // No RD in the block's south row of nodes (12 of 240), and no ash in its west column (20, one of them in that row).
    let rd = grid(&lattice, |_, y| if y < N + 5.0 { f64::NAN } else { 1.4 });
    let ash = grid(&lattice, |x, _| if x < E + 5.0 { f64::NAN } else { 12.0 });
    let c = composite(&whole(&flat(), &lattice), 25.0, &[by(&rd, &[]), by(&ash, &[0])]);
    assert_eq!(c.nodes, 240);
    assert_eq!(c.nodes_without, vec![12, 12 + 19], "ash by tonnes wants the RD too");
    near(c.values[1].unwrap(), 12.0, 1e-12, "ash where it's known");
    near(c.sampled_volume, 18_000.0, 1e-6, "every node still counts in the volume");
}

/// The columns of 43,000 made-up blocks, 60 × 100 m and 3 m thick, 400 across, on a 10 m lattice over them all: what
/// workstream 2's run costs in sampling. Prints the time; run with `--ignored`.
#[test]
#[ignore]
fn columns_of_43000_blocks() {
    let blocks: Vec<TriMesh> = (0..43_000).map(|i| block(E + (i % 400) as f64 * WIDE, N + (i / 400) as f64 * LONG, 100.0, |_| 103.0)).collect();
    let lattice = Lattice { origin: [E - 5.0, N - 5.0], cell_size: [10.0, 10.0], cols: 2_402, rows: 1_084 };
    let start = std::time::Instant::now();
    let nodes: usize = blocks.iter().map(|b| columns_in_solid(b, &lattice).len()).sum();
    let ms = start.elapsed().as_secs_f64() * 1e3;
    println!("43,000 blocks: {nodes} node columns in {ms:.0} ms on one thread");
    assert_eq!(nodes, 43_000 * 60, "6 x 10 nodes a block");
}
