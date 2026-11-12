//! Reserves inside a solid (decision 0061, workstream 1): where a block solid lies at each node of a seam model's lattice,
//! how much of that is in each horizon, and its density and qualities composited as CLAUDE.md's reserves conventions say.
//!
//! The block's volume of record is its mesh's enclosed volume. What's here sets the weights: each lattice node stands for
//! its cell, and its weight is the block's thickness at the node times the cell's area. Air-dried RD is composited by
//! thickness, raw qualities by mass (thickness × air-dried RD), and in-situ RD by Preston–Sanders, node by node, then by
//! thickness.

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
/// `RD_is = RD_ad × (100 − M_ad) / (100 + RD_ad × (M_is − M_ad) − M_is)`.
pub fn in_situ_rd(rd_ad: f64, m_ad: f64, m_is: f64) -> f64 {
    rd_ad * (100.0 - m_ad) / (100.0 + rd_ad * (m_is - m_ad) - m_is)
}

/// A block's density and qualities in one horizon, composited from its samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    /// Nodes holding some of the block in the horizon.
    pub nodes: usize,
    /// Plan area those nodes stand for (m²): nodes × the cell's area.
    pub area: f64,
    /// The block's sampled volume in the horizon (m³): thickness × the cell's area, summed. Its share of the block's own
    /// volume is set by this against the block's whole sampled volume.
    pub sampled_volume: f64,
    /// Air-dried RD by thickness, where any node has one.
    pub rd_ad: Option<f64>,
    /// In-situ RD by thickness, node by node by Preston–Sanders, where the moistures are given and any node has all
    /// three.
    pub rd_is: Option<f64>,
    /// Each quality by mass (thickness × air-dried RD), in the order given; none where no node has both.
    pub qualities: Vec<Option<f64>>,
    /// Nodes left out of the RD composite for want of an RD: they still count in the volume.
    pub nodes_without_rd: usize,
    /// For each quality, nodes left out for want of a value or an RD.
    pub nodes_without: Vec<usize>,
}

/// The grids a composite reads, node by node on the model's lattice; NaN is no value.
pub struct HorizonGrids<'a> {
    /// Air-dried RD.
    pub rd: &'a [f64],
    /// Each quality's grid.
    pub qualities: &'a [&'a [f64]],
    /// In-situ and air-dried moisture (%), for in-situ RD.
    pub moisture: Option<(&'a [f64], &'a [f64])>,
}

/// Composites a block's samples in one horizon: RD by thickness, raw qualities by mass, in-situ RD by thickness after
/// Preston–Sanders at each node.
pub fn composite(samples: &[HorizonSample], cell_area: f64, grids: &HorizonGrids<'_>) -> Composite {
    let thickness: f64 = samples.iter().map(|s| s.1).sum();
    let (mut t_rd, mut rd_sum, mut t_is, mut is_sum) = (0.0, 0.0, 0.0, 0.0);
    let mut nodes_without_rd = 0;
    let n = grids.qualities.len();
    let (mut mass, mut weighted, mut nodes_without) = (vec![0.0; n], vec![0.0; n], vec![0; n]);
    for &(node, t) in samples {
        let rd = grids.rd[node];
        if !rd.is_finite() {
            nodes_without_rd += 1;
            nodes_without.iter_mut().for_each(|w| *w += 1);
            continue;
        }
        t_rd += t;
        rd_sum += t * rd;
        if let Some((is, ad)) = grids.moisture
            && is[node].is_finite()
            && ad[node].is_finite()
        {
            t_is += t;
            is_sum += t * in_situ_rd(rd, ad[node], is[node]);
        }
        for (k, grid) in grids.qualities.iter().enumerate() {
            let q = grid[node];
            if q.is_finite() {
                mass[k] += t * rd;
                weighted[k] += t * rd * q;
            } else {
                nodes_without[k] += 1;
            }
        }
    }
    Composite {
        nodes: samples.len(),
        area: samples.len() as f64 * cell_area,
        sampled_volume: thickness * cell_area,
        rd_ad: (t_rd > 0.0).then(|| rd_sum / t_rd),
        rd_is: (t_is > 0.0).then(|| is_sum / t_is),
        qualities: (0..n).map(|k| (mass[k] > 0.0).then(|| weighted[k] / mass[k])).collect(),
        nodes_without_rd,
        nodes_without,
    }
}
