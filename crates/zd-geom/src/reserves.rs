//! Interrogating a block against a seam model (decision 0061, workstream 1): where a block solid lies at each node of the
//! model's lattice, how much of that is in each horizon, and each of the model's variables averaged over it.
//!
//! The interrogation is the geology at the block and nothing more. Tonnes on a moisture basis, mining factors and seam
//! rules are steps after it, as a dataflow is in Spry or a workflow in Deswik.
//!
//! The block's volume of record is its mesh's enclosed volume. What's here sets the weights: each lattice node stands for
//! its cell, and the block's volume there is its thickness at the node times the cell's area. Each variable says how it's
//! averaged: by that volume alone, or by that volume times other variables at the node. Density is by volume and
//! qualities by coal tonnes (volume × density) as a rule, and clean-coal qualities by tonnes × yield, but the choice is
//! the definition's, not the engine's.

use zd_core::{Lattice, TriMesh};

/// Where a vertical line through one lattice node lies inside a solid: each stretch from where it enters to where it
/// leaves, bottom first, as `[bottom, top]` RL.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeColumn {
    /// The node's index on the lattice: `row * cols + col`.
    pub node: usize,
    pub spans: Vec<[f64; 2]>,
}

impl NodeColumn {
    /// The solid's thickness at the node (m).
    pub fn thickness(&self) -> f64 {
        self.spans.iter().map(|[bottom, top]| top - bottom).sum()
    }
}

/// Every lattice node a solid lies over, with where it's inside the solid there. A node on the edge between two of the
/// solid's faces in plan is counted once, by the top-left rule, so a block whose sides stand on grid lines takes the
/// nodes on its west and north sides and not those on its east and south: its nodes times the cell's area is its plan
/// area. The solid must be closed and wound outward; faces standing straight up have no plan and are passed over.
pub fn columns_in_solid(mesh: &TriMesh, lattice: &Lattice) -> Vec<NodeColumn> {
    let [ox, oy] = lattice.origin;
    let [dx, dy] = lattice.cell_size;
    // Hits per node: (RL, +1 entering going up, -1 leaving).
    let mut hits: std::collections::BTreeMap<usize, Vec<(f64, i32)>> = std::collections::BTreeMap::new();
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
        // Plan coordinates in lattice cells from the lattice's origin, so the tests are exact on grid lines.
        let plan = |p: [f64; 3]| [(p[0] - ox) / dx, (p[1] - oy) / dy];
        let (pa, mut pb, mut pc) = (plan(a), plan(b), plan(c));
        let (za, mut zb, mut zc) = (a[2], b[2], c[2]);
        let area = cross(pa, pb, pc);
        if area == 0.0 {
            continue;
        }
        // Outward normal up (area > 0): a top, left going up. Down: a bottom, entered. Wound counter-clockwise in plan
        // for the fill rule.
        let way = if area > 0.0 { -1 } else { 1 };
        if area < 0.0 {
            std::mem::swap(&mut pb, &mut pc);
            std::mem::swap(&mut zb, &mut zc);
        }
        let area = area.abs();
        let (lo_c, hi_c) = (pa[0].min(pb[0]).min(pc[0]).ceil().max(0.0), pa[0].max(pb[0]).max(pc[0]).floor().min(lattice.cols as f64 - 1.0));
        let (lo_r, hi_r) = (pa[1].min(pb[1]).min(pc[1]).ceil().max(0.0), pa[1].max(pb[1]).max(pc[1]).floor().min(lattice.rows as f64 - 1.0));
        if lo_c > hi_c || lo_r > hi_r {
            continue;
        }
        for row in lo_r as usize..=hi_r as usize {
            for col in lo_c as usize..=hi_c as usize {
                let p = [col as f64, row as f64];
                let (wa, wb, wc) = (cross(pb, pc, p), cross(pc, pa, p), cross(pa, pb, p));
                if !(covers(wa, pb, pc) && covers(wb, pc, pa) && covers(wc, pa, pb)) {
                    continue;
                }
                let z = (wa * za + wb * zb + wc * zc) / area;
                hits.entry(row * lattice.cols + col).or_default().push((z, way));
            }
        }
    }
    hits.into_iter()
        .filter_map(|(node, mut along)| {
            along.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
            let mut spans = Vec::new();
            let (mut inside, mut from) = (0, 0.0);
            for (z, way) in along {
                let was = inside;
                inside += way;
                if was <= 0 && inside > 0 {
                    from = z;
                } else if was > 0 && inside <= 0 && z > from {
                    spans.push([from, z]);
                }
            }
            (!spans.is_empty()).then_some(NodeColumn { node, spans })
        })
        .collect()
}

/// Twice the signed area of the plan triangle `a b c`: positive when counter-clockwise.
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether a point on the inner side of edge `from -> to` of a counter-clockwise triangle is in it: strictly inside, or
/// on a top or left edge (the top-left rule), so a point on an edge shared by two faces is in one of them.
fn covers(w: f64, from: [f64; 2], to: [f64; 2]) -> bool {
    if w != 0.0 {
        return w > 0.0;
    }
    let (ex, ey) = (to[0] - from[0], to[1] - from[1]);
    // Counter-clockwise with y up: a top edge runs west along a row; a left edge runs south.
    (ey == 0.0 && ex < 0.0) || ey < 0.0
}

/// How much of a column lies between a horizon's floor and roof at its node (m). A null roof or floor holds none.
pub fn thickness_between(column: &NodeColumn, floor: f64, roof: f64) -> f64 {
    if !(floor.is_finite() && roof.is_finite()) || roof <= floor {
        return 0.0;
    }
    column.spans.iter().map(|[bottom, top]| (top.min(roof) - bottom.max(floor)).max(0.0)).sum()
}

/// One node's share of a block in one horizon: the node, and the block's thickness there in the horizon (m).
pub type HorizonSample = (usize, f64);

/// The block's columns split by a horizon's roof and floor grids, node by node, on the same lattice: the nodes where it
/// holds some of the horizon, with how much.
pub fn horizon_samples(columns: &[NodeColumn], floor: &[f64], roof: &[f64]) -> Vec<HorizonSample> {
    columns
        .iter()
        .filter_map(|c| {
            let t = thickness_between(c, floor[c.node], roof[c.node]);
            (t > 0.0).then_some((c.node, t))
        })
        .collect()
}

/// In-situ relative density from air-dried RD and the two moistures (%), by Preston and Sanders (1993):
/// `RD_is = RD_ad × (100 − M_ad) / (100 + RD_ad × (M_is − M_ad) − M_is)`. Not part of the interrogation: for a step after
/// it that puts tonnes on an in-situ basis.
pub fn in_situ_rd(rd_ad: f64, m_ad: f64, m_is: f64) -> f64 {
    rd_ad * (100.0 - m_ad) / (100.0 + rd_ad * (m_is - m_ad) - m_is)
}

/// One of the model's variables, and how it's averaged over a block: by the block's volume at each node times each of
/// the variables named in `weight_by` (by their places in the list) at that node. Density takes none; a raw quality the
/// density, so it's by coal tonnes; a clean-coal quality the density and the yield.
pub struct Variable<'a> {
    /// The variable's grid on the model's lattice; NaN is no value.
    pub grid: &'a [f64],
    pub weight_by: &'a [usize],
}

/// A block in one horizon: how much of it there is, and each variable averaged over it.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    /// Nodes holding some of the block in the horizon.
    pub nodes: usize,
    /// Plan area those nodes stand for (m²): nodes × the cell's area.
    pub area: f64,
    /// The block's sampled volume in the horizon (m³): thickness × the cell's area, summed. Its share of the block's own
    /// volume is set by this against the block's whole sampled volume.
    pub sampled_volume: f64,
    /// Each variable averaged by its weighting, in the order given; none where no node has it and its weights.
    pub values: Vec<Option<f64>>,
    /// For each variable, nodes left out for want of its value or one of its weights. They still count in the volume.
    pub nodes_without: Vec<usize>,
}

/// Averages each variable over a block's samples in one horizon by its weighting. A node is left out of a variable where
/// it has no value for it, or none for a variable it's weighted by, and counted.
pub fn composite(samples: &[HorizonSample], cell_area: f64, variables: &[Variable<'_>]) -> Composite {
    let thickness: f64 = samples.iter().map(|s| s.1).sum();
    let n = variables.len();
    let (mut weights, mut weighted, mut nodes_without) = (vec![0.0; n], vec![0.0; n], vec![0; n]);
    for &(node, t) in samples {
        let volume = t * cell_area;
        for (k, v) in variables.iter().enumerate() {
            let value = v.grid[node];
            let weight = v.weight_by.iter().fold(volume, |w, &by| w * variables[by].grid[node]);
            if value.is_finite() && weight.is_finite() {
                weights[k] += weight;
                weighted[k] += weight * value;
            } else {
                nodes_without[k] += 1;
            }
        }
    }
    Composite {
        nodes: samples.len(),
        area: samples.len() as f64 * cell_area,
        sampled_volume: thickness * cell_area,
        values: (0..n).map(|k| (weights[k] > 0.0).then(|| weighted[k] / weights[k])).collect(),
        nodes_without,
    }
}
