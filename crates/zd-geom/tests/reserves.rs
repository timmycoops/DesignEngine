//! Interrogating a block against a seam model (decision 0061, workstream 1): known answers worked by hand on made-up
//! blocks at mine-grid coordinates, each the size a coal mine usually cuts a block to: 60 m across its strip and 100 m
//! along it. Each grid's value holds over the cell centred on its node, weighted by the block's volume in the cell (exact
//! partial cells, as Vulcan does). Volumes to 0.001 m³, RD to 1e-9, qualities to 1 part in a million.

use zd_core::{Lattice, TriMesh};
use zd_geom::{Sample, Variable, cells_in_solid, column_volume, columns_in_solid, composite, horizon_samples, in_situ_rd};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;
/// A block's width across its strip (m), east, and its length along it, north.
const WIDE: f64 = 60.0;
const LONG: f64 = 100.0;

/// A box `wide` east and `long` north from `(x0, y0)`, its floor at `z0` and its top at `top(x)` across it: wound outward.
fn cuboid(x0: f64, y0: f64, wide: f64, long: f64, z0: f64, top: impl Fn(f64) -> f64) -> TriMesh {
    let vertices = (0..8)
        .map(|i| {
            let x = x0 + (i & 1) as f64 * wide;
            let y = y0 + (i >> 1 & 1) as f64 * long;
            [x, y, if i >> 2 & 1 == 1 { top(x) } else { z0 }]
        })
        .collect();
    let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    TriMesh::new(vertices, triangles)
}

/// A 60 × 100 m block from `(x0, y0)`.
fn block(x0: f64, y0: f64, z0: f64, top: impl Fn(f64) -> f64) -> TriMesh {
    cuboid(x0, y0, WIDE, LONG, z0, top)
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

/// 5 m cells whose nodes stand on the block's sides: the cells there are half in it.
fn on_its_sides() -> Lattice {
    Lattice { origin: [E - 20.0, N - 20.0], cell_size: [5.0, 5.0], cols: 30, rows: 30 }
}

/// 5 m cells whose edges run along the block's sides: 12 by 20 of them, each wholly in it.
fn in_its_squares() -> Lattice {
    Lattice { origin: [E - 17.5, N - 17.5], cell_size: [5.0, 5.0], cols: 30, rows: 30 }
}

/// 25 m cells, as most geological models have, sitting anyhow against the block.
fn at_25_m() -> Lattice {
    Lattice { origin: [E - 113.7, N - 107.3], cell_size: [25.0, 25.0], cols: 20, rows: 20 }
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

/// A variable averaged by volume times the variables at `weights`.
fn by<'a>(grid: &'a [f64], weights: &'a [usize]) -> Variable<'a> {
    Variable { grid, weight_by: weights }
}

/// How much of `[lo, hi]` lies in `[a, b]`.
fn overlap(lo: f64, hi: f64, a: f64, b: f64) -> f64 {
    (hi.min(b) - lo.max(a)).max(0.0)
}

#[test]
fn a_block_s_cells_add_up_to_its_volume_and_plan_area() {
    // Cells half in the block along its sides: 13 x 21 nodes' cells, the side ones halved and the corners quartered.
    let samples = cells_in_solid(&flat(), &on_its_sides());
    assert_eq!(samples.len(), 13 * 21);
    let c = composite(&samples, &[]);
    near(c.volume, 18_000.0, 1e-6, "volume");
    near(c.area, 6000.0, 1e-6, "plan area");
    near(flat().signed_volume(), 18_000.0, 1e-6, "the block's own volume");
    // Cells whose edges run along its sides: 240 whole cells of 75 m³.
    let whole = cells_in_solid(&flat(), &in_its_squares());
    assert_eq!(whole.len(), 240);
    assert!(whole.iter().all(|s| (s.volume - 75.0).abs() < 1e-9 && (s.area - 25.0).abs() < 1e-9));
}

#[test]
fn on_25_m_cells_each_node_weighs_the_block_s_volume_in_its_cell() {
    // The block sits anyhow on 25 m cells: up to 4 by 5 of them, each by its share.
    let lattice = at_25_m();
    let samples = cells_in_solid(&flat(), &lattice);
    let c = composite(&samples, &[]);
    near(c.volume, 18_000.0, 1e-6, "volume, whatever the grid");
    near(c.area, 6000.0, 1e-6, "plan area");
    // Ash rising 0.1 % a metre east, taken at each node over its cell: each node's share is its cell's overlap with the
    // block, worked here from the cell's edges alone.
    let ash = grid(&lattice, |x, _| 10.0 + 0.1 * (x - E));
    let rd = vec![1.4; 400];
    let (mut weight, mut sum) = (0.0, 0.0);
    for r in 0..lattice.rows {
        for col in 0..lattice.cols {
            let (x, y) = (lattice.origin[0] + col as f64 * 25.0, lattice.origin[1] + r as f64 * 25.0);
            let share = overlap(x - 12.5, x + 12.5, E, E + WIDE) * overlap(y - 12.5, y + 12.5, N, N + LONG) * 3.0;
            weight += share;
            sum += share * (10.0 + 0.1 * (x - E));
        }
    }
    near(weight, 18_000.0, 1e-6, "the shares worked by hand");
    let got = composite(&samples, &[by(&rd, &[]), by(&ash, &[0])]);
    near(got.values[1].unwrap(), sum / weight, 1e-9, "ash by the cells' shares");
    assert_eq!(got.nodes, samples.len());
    // A metre east, the volume is the same.
    near(composite(&cells_in_solid(&block(E + 1.0, N, 100.0, |_| 103.0), &lattice), &[]).volume, 18_000.0, 1e-6, "moved");
}

#[test]
fn a_block_smaller_than_a_cell_takes_its_node_s_value() {
    // A 10 m square 3 m thick within one 25 m cell: one node, 300 m³, and that node's ash.
    let lattice = at_25_m();
    let node = [lattice.origin[0] + 5.0 * 25.0, lattice.origin[1] + 5.0 * 25.0];
    let small = cuboid(node[0] - 4.0, node[1] - 6.0, 10.0, 10.0, 100.0, |_| 103.0);
    let samples = cells_in_solid(&small, &lattice);
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].node, 5 * lattice.cols + 5);
    let ash = grid(&lattice, |x, y| x - lattice.origin[0] + (y - lattice.origin[1]) / 1000.0);
    let c = composite(&samples, &[by(&ash, &[])]);
    near(c.volume, 300.0, 1e-9, "volume");
    near(c.values[0].unwrap(), ash[5 * lattice.cols + 5], 1e-12, "the node's ash");
}

#[test]
fn a_quality_rising_across_the_block_averages_to_its_value_at_the_middle() {
    // Ash from 10 % at the west side, rising 0.1 % a metre east, by coal tonnes: 13 % in the middle, 30 m across, whether
    // the cells sit wholly in the block or half over its sides.
    for lattice in [in_its_squares(), on_its_sides()] {
        let (rd, ash) = (vec![1.4; 900], grid(&lattice, |x, _| 10.0 + 0.1 * (x - E)));
        let c = composite(&cells_in_solid(&flat(), &lattice), &[by(&rd, &[]), by(&ash, &[0])]);
        near(c.values[0].unwrap(), 1.4, 1e-9, "RD");
        near(c.values[1].unwrap(), 13.0, 13e-6, "ash");
    }
}

#[test]
fn density_is_by_volume_and_a_quality_by_coal_tonnes() {
    let lattice = in_its_squares();
    // On the wedge: RD 1.40 west, 1.50 east; ash 10 % west, 20 % east.
    let rd = grid(&lattice, |x, _| if west(x) { 1.40 } else { 1.50 });
    let ash = grid(&lattice, |x, _| if west(x) { 10.0 } else { 20.0 });
    let samples = cells_in_solid(&wedge(), &lattice);
    near(composite(&samples, &[]).volume, 33_000.0, 1e-6, "the wedge's volume in its cells");
    let c = composite(&samples, &[by(&rd, &[]), by(&ash, &[0])]);
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
    let samples = cells_in_solid(&flat(), &lattice);
    // By coal tonnes: (1.4 × 25 + 1.6 × 21) / 3.0 = 68.6 / 3. By volume alone, as someone may want: the plain mean, 23.
    near(composite(&samples, &[by(&rd, &[]), by(&cv, &[0])]).values[1].unwrap(), 68.6 / 3.0, 1e-9, "by tonnes");
    near(composite(&samples, &[by(&cv, &[])]).values[0].unwrap(), 23.0, 1e-9, "by volume");
    // By as many variables as it names: the RD and a weight grid of 2 west and 1 east, (2.8 × 25 + 1.6 × 21) / 4.4.
    let weight = grid(&lattice, |x, _| if west(x) { 2.0 } else { 1.0 });
    near(composite(&samples, &[by(&rd, &[]), by(&weight, &[]), by(&cv, &[0, 1])]).values[2].unwrap(), 103.6 / 4.4, 1e-9, "by both");
}

#[test]
fn in_situ_rd_is_preston_and_sanders() {
    // For a step after the interrogation. RD 1.45 air-dried, 3 % air-dried moisture, 8 % in situ:
    // 1.45 × 97 / (100 + 1.45 × 5 − 8) = 140.65 / 99.25.
    near(in_situ_rd(1.45, 3.0, 8.0), 1.417_128_463_5, 1e-9, "in-situ RD");
    near(in_situ_rd(1.45, 5.0, 5.0), 1.45, 1e-12, "no more moisture in situ than air-dried: the same RD");
}

#[test]
fn columns_at_nodes_on_its_sides_are_counted_once() {
    let lattice = on_its_sides();
    let columns = columns_in_solid(&flat(), &lattice);
    // 12 x 20 nodes, those on the west and north sides taken and not the east and south.
    assert_eq!(columns.len(), 240);
    for c in &columns {
        assert_eq!(c.spans, vec![[100.0, 103.0]], "node {}", c.node);
        let (col, row) = (c.node % lattice.cols, c.node / lattice.cols);
        let (x, y) = (lattice.origin[0] + col as f64 * 5.0 - E, lattice.origin[1] + row as f64 * 5.0 - N);
        assert!((0.0..WIDE).contains(&x) && (0.0..=LONG).contains(&y) && y > 0.0, "node at {x}, {y} m in the block");
    }
}

#[test]
fn how_much_of_a_block_lies_in_a_seam_is_found_from_its_roof_and_floor() {
    // A check that a block is in its seam. A block from RL 100 to 110; seam A from 107 up, seam B from 95 to 107: 30 : 70.
    let mesh = block(E, N, 100.0, |_| 110.0);
    let lattice = in_its_squares();
    let columns = columns_in_solid(&mesh, &lattice);
    let a = horizon_samples(&columns, &vec![107.0; 900], &vec![115.0; 900], 25.0);
    let b = horizon_samples(&columns, &vec![95.0; 900], &vec![107.0; 900], 25.0);
    let (va, vb) = (composite(&a, &[]).volume, composite(&b, &[]).volume);
    near(va, 18_000.0, 1e-6, "in A");
    near(vb, 42_000.0, 1e-6, "in B");
    near((va + vb) / mesh.signed_volume(), 1.0, 1e-12, "together, the block");
    // Where B's roof is null (its east half), the block holds none of B there.
    let roof = grid(&lattice, |x, _| if west(x) { 107.0 } else { f64::NAN });
    near(composite(&horizon_samples(&columns, &vec![95.0; 900], &roof, 25.0), &[]).volume, 21_000.0, 1e-6, "B's west half");
}

#[test]
fn a_solid_in_two_pieces_is_sampled_as_it_lies() {
    // Two 3 m blocks one over the other with 2 m between, as one mesh: two stretches at every node, 36,000 m³ in cells.
    let lattice = in_its_squares();
    let mut two = block(E, N, 100.0, |_| 103.0);
    let upper = block(E, N, 105.0, |_| 108.0);
    let offset = two.vertices.len() as u32;
    two.vertices.extend(upper.vertices);
    two.triangles.extend(upper.triangles.iter().map(|t| t.map(|i| i + offset)));
    assert!(columns_in_solid(&two, &lattice).iter().all(|c| c.spans == vec![[100.0, 103.0], [105.0, 108.0]]));
    near(composite(&cells_in_solid(&two, &lattice), &[]).volume, 36_000.0, 1e-6, "both pieces");
}

#[test]
fn nodes_with_no_value_or_no_weight_are_left_out_and_counted() {
    let lattice = in_its_squares();
    // No RD in the block's south row of cells (12 of 240), and no ash in its west column (20, one of them in that row).
    let rd = grid(&lattice, |_, y| if y < N + 5.0 { f64::NAN } else { 1.4 });
    let ash = grid(&lattice, |x, _| if x < E + 5.0 { f64::NAN } else { 12.0 });
    let c = composite(&cells_in_solid(&flat(), &lattice), &[by(&rd, &[]), by(&ash, &[0])]);
    assert_eq!(c.nodes, 240);
    assert_eq!(c.nodes_without, vec![12, 12 + 19], "ash by tonnes wants the RD too");
    near(c.values[1].unwrap(), 12.0, 1e-12, "ash where it's known");
    near(c.volume, 18_000.0, 1e-6, "every cell still counts in the volume");
}

/// A prism over a plan polygon (counter-clockwise), from `z0` up to `top(x)` at each corner: wound outward.
fn prism(corners: &[[f64; 2]], z0: f64, top: impl Fn(f64) -> f64) -> TriMesh {
    let n = corners.len() as u32;
    let mut vertices: Vec<[f64; 3]> = corners.iter().map(|c| [c[0], c[1], z0]).collect();
    vertices.extend(corners.iter().map(|c| [c[0], c[1], top(c[0])]));
    let mut triangles = Vec::new();
    for i in 1..n - 1 {
        triangles.push([0, i + 1, i]);
        triangles.push([n, n + i, n + i + 1]);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        triangles.push([i, j, n + j]);
        triangles.push([i, n + j, n + i]);
    }
    TriMesh::new(vertices, triangles)
}

/// A 60 × 100 m block turned `degrees` anticlockwise about its middle, as its corners.
fn turned(degrees: f64) -> Vec<[f64; 2]> {
    let (s, c) = degrees.to_radians().sin_cos();
    let middle = [E + 130.0, N + 120.0];
    [[-30.0, -50.0], [30.0, -50.0], [30.0, 50.0], [-30.0, 50.0]].iter().map(|[x, y]| [middle[0] + x * c - y * s, middle[1] + x * s + y * c]).collect()
}

#[test]
fn the_column_volume_of_a_whole_seam_block_is_its_own() {
    // A block turned 30° across 25 m cells, the full 4 m of a flat seam: its plan in the cells is exact, 6,000 m², and its
    // column 24,000 m³, its own volume.
    let lattice = at_25_m();
    let block = prism(&turned(30.0), 100.0, |_| 104.0);
    near(block.signed_volume(), 24_000.0, 1e-6, "the block");
    let samples = cells_in_solid(&block, &lattice);
    near(samples.iter().map(|s| s.area).sum(), 6000.0, 1e-6, "its plan in the cells");
    near(column_volume(&samples, &vec![100.0; 400], &vec![104.0; 400]), 24_000.0, 1e-6, "its column");
    // Where the roof is null over the cells west of its middle, the column holds none there.
    let roof = grid(&lattice, |x, _| if x < E + 130.0 { f64::NAN } else { 104.0 });
    assert!(column_volume(&samples, &vec![100.0; 400], &roof) < 15_000.0);
}

#[test]
fn a_seam_thickening_across_the_block_gives_its_column_cell_by_cell() {
    // The seam's roof rises 0.05 m a metre east from 103 m at the block's west side, the block's top on it: on cells wholly
    // in the block each node's thickness holds over its cell, as it does across a linear seam, so the column is the block.
    let lattice = in_its_squares();
    let block = block(E, N, 100.0, |x| 103.0 + 0.05 * (x - E));
    let samples = cells_in_solid(&block, &lattice);
    let roof = grid(&lattice, |x, _| 103.0 + 0.05 * (x - E));
    // 60 × 100 × (3 + 1.5) = 27,000 m³.
    near(block.signed_volume(), 27_000.0, 1e-6, "the block");
    near(column_volume(&samples, &vec![100.0; 900], &roof), 27_000.0, 1e-6, "its column");
    // A block cut 1 m short of the roof, as a bench would be, holds 6,000 m³ less than its seam's column there.
    let short = self::block(E, N, 100.0, |x| 102.0 + 0.05 * (x - E));
    let short_cells = cells_in_solid(&short, &lattice);
    near(column_volume(&short_cells, &vec![100.0; 900], &roof) - short.signed_volume(), 6000.0, 1e-6, "the difference");
}

/// The cells of 43,000 made-up blocks, 60 × 100 m and 3 m thick, 400 across, on 25 m and 10 m lattices: what workstream
/// 2's run costs in sampling. Prints the time; run with `--ignored`.
#[test]
#[ignore]
fn cells_of_43000_blocks() {
    let blocks: Vec<TriMesh> = (0..43_000).map(|i| block(E + (i % 400) as f64 * WIDE, N + (i / 400) as f64 * LONG, 100.0, |_| 103.0)).collect();
    for spacing in [25.0, 10.0] {
        let lattice = Lattice { origin: [E - 7.3, N - 3.9], cell_size: [spacing, spacing], cols: (24_100.0 / spacing) as usize, rows: (10_900.0 / spacing) as usize };
        let start = std::time::Instant::now();
        let samples: Vec<Vec<Sample>> = blocks.iter().map(|b| cells_in_solid(b, &lattice)).collect();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        let volume: f64 = samples.iter().flatten().map(|s| s.volume).sum();
        println!("43,000 blocks on {spacing} m cells: {} cell shares in {ms:.0} ms on one thread", samples.iter().map(Vec::len).sum::<usize>());
        near(volume, 43_000.0 * 18_000.0, 1e-3, "every block's volume");
    }
}
