//! Interrogating a block against a seam model (decision 0061, workstream 1): a block is a coal solid of one seam, and each
//! of that seam's grids is averaged over it by exact partial cells, as Vulcan does: a node's value holds over the cell
//! centred on it, weighted by the block's volume inside the cell. How much of the solid lies between its seam's roof and
//! floor is a check that it's in the seam.
//!
//! The interrogation is the geology at the block and nothing more: whatever the grids hold, density in situ or air-dried
//! included, with nothing converted. Tonnes on a moisture basis, mining factors and seam
//! rules are steps after it, as a dataflow is in Spry or a workflow in Deswik.
//!
//! The block's volume of record is its mesh's enclosed volume, which the cells' shares add up to. Each variable says how
//! it's averaged: by those volumes alone, or by them times other variables at the node. Density is by volume and
//! qualities by coal tonnes (volume × density) as a rule, but the choice is the definition's, not the engine's. The solid
//! is seam coal: yield, dilution and coal mixed with waste come later, in the steps that make run-of-mine coal.

use std::collections::BTreeMap;

use manifold_csg::Manifold;
use zd_core::{Grid, Lattice, TriMesh};

use crate::cut::{from_manifold, to_manifold};
use crate::{GeomError, Side, close_surface_to_halfspace, triangulate_grid};

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

/// One node's share of a block: the node, the block's volume inside the node's cell (m³), and the plan area of the
/// block's top inside it (m²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub node: usize,
    pub volume: f64,
    pub area: f64,
}

/// The block's volume in each node's cell, by exact partial cells, as Vulcan does it: a node's value holds over the cell
/// centred on it, and its share of the block is the block's volume inside that cell, its faces clipped to the cell. The
/// shares add up to the block's volume whatever the node spacing and wherever the block sits on the grid, and a block
/// smaller than a cell still takes its node's value.
///
/// The volume inside a cell is the integral of the height of each face over its plan inside the cell, a face that looks
/// up counted and one that looks down taken away, as a closed solid's volume is; the cell's walls stand straight up and
/// add nothing. The solid must be closed and wound outward.
pub fn cells_in_solid(mesh: &TriMesh, lattice: &Lattice) -> Vec<Sample> {
    shares_in_cells(mesh, lattice).into_iter().filter(|(_, (volume, _))| *volume > 0.0).map(|(node, (volume, area))| Sample { node, volume, area }).collect()
}

/// For each node's cell a solid reaches into: its volume there, and the plan area of its faces looking up there.
fn shares_in_cells(mesh: &TriMesh, lattice: &Lattice) -> BTreeMap<usize, (f64, f64)> {
    let [ox, oy] = lattice.origin;
    let [dx, dy] = lattice.cell_size;
    let cell_area = dx * dy;
    // Heights from the solid's lowest point, so the sums aren't of large RLs; the result is the same for any datum.
    let datum = mesh.vertices.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
    let mut shares: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    for t in &mesh.triangles {
        // In cells from the lattice's origin: node (c, r) at (c, r), its cell from c - ½ to c + ½.
        let [a, b, c] = t.map(|i| {
            let v = mesh.vertices[i as usize];
            [(v[0] - ox) / dx, (v[1] - oy) / dy, v[2] - datum]
        });
        let up = cross([a[0], a[1]], [b[0], b[1]], [c[0], c[1]]);
        if up == 0.0 {
            continue;
        }
        let sign = up.signum();
        let span = |k: usize| -> (i64, i64) {
            let lo = a[k].min(b[k]).min(c[k]);
            let hi = a[k].max(b[k]).max(c[k]);
            ((lo + 0.5).floor() as i64, (hi + 0.5).floor() as i64)
        };
        let ((c0, c1), (r0, r1)) = (span(0), span(1));
        for row in r0.max(0)..=r1.min(lattice.rows as i64 - 1) {
            for col in c0.max(0)..=c1.min(lattice.cols as i64 - 1) {
                let (x, y) = (col as f64, row as f64);
                let piece = clip_to_cell(&[a, b, c], [x - 0.5, x + 0.5], [y - 0.5, y + 0.5]);
                let (area, height) = area_and_height(&piece);
                if area <= 0.0 {
                    continue;
                }
                let share = shares.entry(row as usize * lattice.cols + col as usize).or_default();
                share.0 += sign * area * height * cell_area;
                if sign > 0.0 {
                    share.1 += area * cell_area;
                }
            }
        }
    }
    shares
}

/// A block's plan at mid-seam in each node's cell (decision 0062, workstream 4): where the surface halfway between its
/// seam's roof and floor lies inside it, by exact partial cells. It's the polygon Minex or Vulcan would take for the block
/// at the seam, so a battered wall, which widens the block's whole plan, drops out of the column check. Each sample's
/// `area` is that plan in the cell (m²) and its `volume` the area times the seam's thickness at the node: the cell's part
/// of the 2.5D column volume.
///
/// The block is cut at the mid-seam surface into the part below it and the part above. A face of the block the cut
/// doesn't touch looks up from one part or the other, and the section is the top of the part below, so in each cell the
/// section's plan is the two parts' upward plan less the block's. Where the roof or floor is null the mid-seam surface is
/// taken below the block: no section there, as no seam.
pub fn mid_seam_cells(mesh: &TriMesh, lattice: &Lattice, floor: &[f64], roof: &[f64]) -> Result<Vec<Sample>, GeomError> {
    let bounds = mesh.bounds().ok_or_else(|| GeomError::InvalidInput("empty block".into()))?;
    let [ox, oy] = lattice.origin;
    let [dx, dy] = lattice.cell_size;
    let (cols, rows) = (lattice.cols as i64, lattice.rows as i64);
    // Nodes from one past the block each way, held at the nearest edge node past the lattice's, so the surface reaches
    // over every cell the block is in.
    let (c0, c1) = (((bounds.min[0] - ox) / dx).floor() as i64 - 1, ((bounds.max[0] - ox) / dx).ceil() as i64 + 1);
    let (r0, r1) = (((bounds.min[1] - oy) / dy).floor() as i64 - 1, ((bounds.max[1] - oy) / dy).ceil() as i64 + 1);
    let below = bounds.min[2] - 1.0;
    let mid = |c: i64, r: i64| {
        let i = (r.clamp(0, rows - 1) * cols + c.clamp(0, cols - 1)) as usize;
        let (f, t) = (floor[i], roof[i]);
        if f.is_finite() && t.is_finite() { 0.5 * (f + t) } else { below }
    };
    let centre = bounds.centre();
    let origin = [centre[0].round(), centre[1].round(), 0.0];
    let (wc, wr) = ((c1 - c0 + 1) as usize, (r1 - r0 + 1) as usize);
    let z: Vec<f64> = (0..wr).flat_map(|r| (0..wc).map(move |c| (c, r))).map(|(c, r)| mid(c0 + c as i64, r0 + r as i64)).collect();
    let (lowest, highest) = z.iter().fold((bounds.min[2], bounds.max[2]), |(lo, hi), &z| (lo.min(z), hi.max(z)));
    let window = Grid::new([ox + c0 as f64 * dx - origin[0], oy + r0 as f64 * dy - origin[1]], [dx, dy], wc, wr, z);
    let surface = triangulate_grid(&window);
    let below_mid = to_manifold(&close_surface_to_halfspace(&surface, Side::Below, lowest - 1.0)?.mesh, [0.0; 3], "the half-space below mid-seam")?;
    let above_mid = to_manifold(&close_surface_to_halfspace(&surface, Side::Above, highest + 1.0)?.mesh, [0.0; 3], "the half-space above mid-seam")?;
    let block = to_manifold(mesh, [-origin[0], -origin[1], 0.0], "the block")?;
    let up = |m: &Manifold| -> Result<BTreeMap<usize, (f64, f64)>, GeomError> {
        m.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
        Ok(shares_in_cells(&from_manifold(m, origin), lattice))
    };
    let (lower, upper, whole) = (up(&block.intersection(&below_mid))?, up(&block.intersection(&above_mid))?, shares_in_cells(mesh, lattice));
    let area_of = |shares: &BTreeMap<usize, (f64, f64)>, node: usize| shares.get(&node).map_or(0.0, |s| s.1);
    let least = 1e-9 * dx * dy;
    Ok(lower
        .keys()
        .filter_map(|&node| {
            let area = area_of(&lower, node) + area_of(&upper, node) - area_of(&whole, node);
            (area > least).then(|| {
                let (f, t) = (floor[node], roof[node]);
                let thickness = if f.is_finite() && t.is_finite() { (t - f).max(0.0) } else { 0.0 };
                Sample { node, volume: area * thickness, area }
            })
        })
        .collect())
}

/// A plan polygon clipped to a cell, Sutherland and Hodgman's way, its heights carried along its edges: the face is flat,
/// so a height between two corners is between theirs.
fn clip_to_cell(polygon: &[[f64; 3]], x: [f64; 2], y: [f64; 2]) -> Vec<[f64; 3]> {
    let mut out = polygon.to_vec();
    for (axis, bound, keep_below) in [(0, x[0], false), (0, x[1], true), (1, y[0], false), (1, y[1], true)] {
        if out.is_empty() {
            break;
        }
        let inside = |p: &[f64; 3]| if keep_below { p[axis] <= bound } else { p[axis] >= bound };
        let input = std::mem::take(&mut out);
        for k in 0..input.len() {
            let (p, q) = (input[k], input[(k + 1) % input.len()]);
            let (pin, qin) = (inside(&p), inside(&q));
            if pin {
                out.push(p);
            }
            if pin != qin {
                let t = (bound - p[axis]) / (q[axis] - p[axis]);
                out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])]);
            }
        }
    }
    out
}

/// A flat polygon's plan area (in cells, unsigned) and its mean height over that area.
fn area_and_height(polygon: &[[f64; 3]]) -> (f64, f64) {
    if polygon.len() < 3 {
        return (0.0, 0.0);
    }
    let (mut area, mut moment) = (0.0, 0.0);
    let p0 = polygon[0];
    for k in 1..polygon.len() - 1 {
        let (p, q) = (polygon[k], polygon[k + 1]);
        let a = cross([p0[0], p0[1]], [p[0], p[1]], [q[0], q[1]]).abs() / 2.0;
        area += a;
        moment += a * (p0[2] + p[2] + q[2]) / 3.0;
    }
    if area > 0.0 { (area, moment / area) } else { (0.0, 0.0) }
}

/// The block's columns between a horizon's floor and roof grids at the nodes it lies over, each standing for its whole
/// cell: a check of how much of a block lies in its seam, set against the block's volume. Coarse where the grid is coarse
/// against the block, as it counts whole cells.
pub fn horizon_samples(columns: &[NodeColumn], floor: &[f64], roof: &[f64], cell_area: f64) -> Vec<Sample> {
    columns
        .iter()
        .filter_map(|c| {
            let t = thickness_between(c, floor[c.node], roof[c.node]);
            (t > 0.0).then_some(Sample { node: c.node, volume: t * cell_area, area: cell_area })
        })
        .collect()
}

/// A block's 2.5D column volume (decision 0061, workstream 3): its seam's thickness at each node, roof less floor, times
/// the block's plan in the node's cell, summed. It's how Vulcan and Minex work out a seam's volume inside a polygon, with
/// exact partial cells, and it's set against the block's own volume as a check, never in its place. A null roof or floor,
/// or a floor above its roof, holds none.
pub fn column_volume(samples: &[Sample], floor: &[f64], roof: &[f64]) -> f64 {
    samples
        .iter()
        .map(|s| {
            let (f, r) = (floor[s.node], roof[s.node]);
            if f.is_finite() && r.is_finite() { s.area * (r - f).max(0.0) } else { 0.0 }
        })
        .sum()
}

/// In-situ relative density from air-dried RD and the two moistures (%), by Preston and Sanders (1993):
/// `RD_is = RD_ad × (100 − M_ad) / (100 + RD_ad × (M_is − M_ad) − M_is)`. Not part of the interrogation: for a step after
/// it that puts tonnes on an in-situ basis.
pub fn in_situ_rd(rd_ad: f64, m_ad: f64, m_is: f64) -> f64 {
    rd_ad * (100.0 - m_ad) / (100.0 + rd_ad * (m_is - m_ad) - m_is)
}

/// One of the model's variables, and how it's averaged over a block: by the block's volume in each node's cell times
/// each of the variables named in `weight_by` (by their places in the list) at that node. Density takes none; a raw quality the
/// density, so it's by coal tonnes.
pub struct Variable<'a> {
    /// The variable's grid on the model's lattice; NaN is no value.
    pub grid: &'a [f64],
    pub weight_by: &'a [usize],
}

/// A block's samples: how much of it there is, and each variable averaged over it.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    /// Nodes whose cells hold some of the block.
    pub nodes: usize,
    /// Plan area of the block's top in those cells (m²).
    pub area: f64,
    /// The block's volume in those cells (m³). With every cell it touches, the block's own volume.
    pub volume: f64,
    /// Each variable averaged by its weighting, in the order given; none where no node has it and its weights.
    pub values: Vec<Option<f64>>,
    /// For each variable, nodes left out for want of its value or one of its weights. They still count in the volume.
    pub nodes_without: Vec<usize>,
}

/// Averages each variable over a block's samples by its weighting. A node is left out of a variable where it has no
/// value for it, or none for a variable it's weighted by, and counted.
pub fn composite(samples: &[Sample], variables: &[Variable<'_>]) -> Composite {
    let n = variables.len();
    let (mut weights, mut weighted, mut nodes_without) = (vec![0.0; n], vec![0.0; n], vec![0; n]);
    for sample in samples {
        for (k, v) in variables.iter().enumerate() {
            let value = v.grid[sample.node];
            let weight = v.weight_by.iter().fold(sample.volume, |w, &by| w * variables[by].grid[sample.node]);
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
        area: samples.iter().map(|s| s.area).sum(),
        volume: samples.iter().map(|s| s.volume).sum(),
        values: (0..n).map(|k| (weights[k] > 0.0).then(|| weighted[k] / weights[k])).collect(),
        nodes_without,
    }
}
