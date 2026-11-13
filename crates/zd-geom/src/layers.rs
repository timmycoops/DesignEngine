//! Solids cut by a seam model's horizons in one run (decision 0062, workstream 3).
//!
//! The working section's roofs and floors, top first, split the ground into layers: overburden above the first roof,
//! each seam between its roof and floor, each interburden between a floor and the next roof down, and what lies below
//! the last floor. Each layer is made as a slab between its two grids where it's thicker than a minimum, clipped exactly
//! where its thickness passes that minimum inside each grid triangle, with a wall of the minimum's height there; a seam
//! with no thickness, at a subcrop or a washout, then makes no slab at all, and nothing coincides. A solid's piece of a
//! layer is one Manifold intersection with its slab. What the slabs leave out, where a layer is thinner than the
//! minimum, is said by its volume; what lies off the grids is kept whole.
//!
//! Everything is worked about a whole-metre origin near the solid, so mine grid coordinates keep their precision.

use std::collections::HashMap;
use std::time::Instant;

use manifold_csg::{CrossSection, Manifold};
use zd_core::{Lattice, TriMesh};

use crate::cut::{from_manifold, to_manifold};
use crate::{GeomError, TinyEdges, join_tiny_edges};

/// A piece with less volume than this (m³) is nothing: a slab only touching the solid.
const NOTHING: f64 = 1e-6;

/// The surfaces between the layers of a seam stack on one lattice, top first: each working horizon's roof then its
/// floor. Every node holds a height in every surface, in order down the stack.
#[derive(Debug, Clone)]
pub struct LayerStack {
    pub lattice: Lattice,
    /// Two a horizon, roof then floor, top first; node `row * cols + col`. NaN only where no horizon has a height,
    /// which the cut takes as below everything.
    pub surfaces: Vec<Vec<f64>>,
    /// Horizons at nodes where they hold no height (null, or marked absent), laid on the floor of the live horizon
    /// above or, with none above, on the roof of the one below: counted once a horizon a node.
    pub absent: usize,
    /// Heights lowered to the surface above where the two crossed, and the most any was lowered (m).
    pub lowered: usize,
    pub most_lowered: f64,
}

impl LayerStack {
    /// Layers: overburden, then each horizon and the layer below it.
    pub fn layers(&self) -> usize {
        self.surfaces.len() + 1
    }
}

/// The surfaces of a stack from its working horizons' roofs and floors, top first, each `row * cols + col` on
/// `lattice`. Where a horizon has no height, it's laid flat on the floor of the live horizon above, so the waste
/// between belongs with the seam below it; with no live horizon above, on the roof of the live one below, so the
/// overburden reaches down to it. Then each surface is kept from rising above the one over it: the stack's own check
/// blocks crossings beyond its tolerance, so only overlaps within it are lowered, and they're counted.
pub fn layer_stack(lattice: Lattice, horizons: &[(&[f64], &[f64])]) -> Result<LayerStack, GeomError> {
    let nodes = lattice.cols * lattice.rows;
    if lattice.cols < 2 || lattice.rows < 2 {
        return Err(GeomError::InvalidInput("the grids need two rows and two columns of nodes or more".into()));
    }
    if horizons.is_empty() {
        return Err(GeomError::InvalidInput("the stack has no working horizons".into()));
    }
    if horizons.iter().any(|(r, f)| r.len() != nodes || f.len() != nodes) {
        return Err(GeomError::InvalidInput("every roof and floor must be on the stack's lattice".into()));
    }
    let mut surfaces: Vec<Vec<f64>> = horizons.iter().flat_map(|(r, f)| [r.to_vec(), f.to_vec()]).collect();
    let (mut absent, mut lowered, mut most_lowered) = (0, 0, 0.0_f64);
    for node in 0..nodes {
        let live = |k: usize, s: &[Vec<f64>]| !s[2 * k][node].is_nan() && !s[2 * k + 1][node].is_nan();
        let first = (0..horizons.len()).find(|&k| live(k, &surfaces));
        let mut above: Option<f64> = None;
        for k in 0..horizons.len() {
            if live(k, &surfaces) {
                above = Some(surfaces[2 * k + 1][node]);
                continue;
            }
            absent += 1;
            let laid = above.or_else(|| first.map(|f| surfaces[2 * f][node])).unwrap_or(f64::NAN);
            surfaces[2 * k][node] = laid;
            surfaces[2 * k + 1][node] = laid;
        }
        for s in 1..surfaces.len() {
            let (over, z) = (surfaces[s - 1][node], surfaces[s][node]);
            if z > over {
                lowered += 1;
                most_lowered = most_lowered.max(z - over);
                surfaces[s][node] = over;
            }
        }
    }
    Ok(LayerStack { lattice, surfaces, absent, lowered, most_lowered })
}

/// A solid's piece of one layer.
#[derive(Debug, Clone)]
pub struct LayerPiece {
    /// Which layer: 0 the overburden, `2k + 1` horizon `k`, `2k + 2` the layer below it.
    pub layer: usize,
    pub mesh: TriMesh,
    pub volume: f64,
    /// Separate parts in it.
    pub parts: usize,
    pub tiny_edges: TinyEdges,
}

/// One solid cut by the layers of a stack.
#[derive(Debug, Clone)]
pub struct LayerCut {
    /// A piece for each layer the solid reaches into, top down.
    pub pieces: Vec<LayerPiece>,
    /// What lies off the grids in plan, kept whole.
    pub outside: Option<LayerPiece>,
    /// Volume on the grids in no slab, where a layer is thinner than the minimum (m³).
    pub thin: f64,
    pub input_volume: f64,
    /// Triangles in the slabs it was cut by.
    pub slab_triangles: usize,
    pub ms: f64,
}

impl LayerCut {
    /// The pieces, what's off the grids and what's too thin together, less the solid (m³): what the cut lost or gained.
    pub fn difference(&self) -> f64 {
        self.pieces.iter().chain(&self.outside).map(|p| p.volume).sum::<f64>() + self.thin - self.input_volume
    }
}

/// Cuts a closed solid by every layer of `stack` thicker than `min_thickness`: one piece a layer it reaches into.
pub fn cut_solid_by_layers(solid: &TriMesh, stack: &LayerStack, min_thickness: f64) -> Result<LayerCut, GeomError> {
    if !(min_thickness > 0.0 && min_thickness.is_finite()) {
        return Err(GeomError::InvalidInput("the minimum thickness must be more than nothing".into()));
    }
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "solid", validity });
    }
    let started = Instant::now();
    let bounds = solid.bounds().ok_or_else(|| GeomError::InvalidInput("empty solid".into()))?;
    let centre = bounds.centre();
    let origin = [centre[0].round(), centre[1].round(), 0.0];
    let solid_m = to_manifold(solid, [-origin[0], -origin[1], 0.0], "solid")?;
    let input_volume = solid.signed_volume();
    let lattice = stack.lattice;
    let [x0, y0] = lattice.origin;
    let [dx, dy] = lattice.cell_size;
    let (x1, y1) = (x0 + (lattice.cols - 1) as f64 * dx, y0 + (lattice.rows - 1) as f64 * dy);

    // What lies off the grids, kept whole.
    let margin = 1.0 + 0.01 * bounds.size()[2];
    let off_grids = bounds.min[0] < x0 || bounds.min[1] < y0 || bounds.max[0] > x1 || bounds.max[1] > y1;
    let (on, outside) = if off_grids {
        let rect = vec![[x0 - origin[0], y0 - origin[1]], [x1 - origin[0], y0 - origin[1]], [x1 - origin[0], y1 - origin[1]], [x0 - origin[0], y1 - origin[1]]];
        let prism = CrossSection::from_polygons(&[rect]).extrude(bounds.size()[2] + 2.0 * margin).translate(0.0, 0.0, bounds.min[2] - margin);
        (solid_m.intersection(&prism), piece_of(&solid_m.difference(&prism), usize::MAX, origin)?)
    } else {
        (solid_m.clone(), None)
    };

    // The cells under the solid, one more each way.
    let span = |lo: f64, hi: f64, start: f64, step: f64, count: usize| {
        let first = (((lo - start) / step).floor() - 1.0).max(0.0) as usize;
        let last = ((((hi - start) / step).ceil() + 1.0).max(0.0) as usize).min(count - 1);
        (first, last)
    };
    let mut pieces = Vec::new();
    let mut slabs = Vec::new();
    let mut slab_triangles = 0;
    if bounds.max[0] >= x0 && bounds.max[1] >= y0 && bounds.min[0] <= x1 && bounds.min[1] <= y1 {
        let (c0, c1) = span(bounds.min[0], bounds.max[0], x0, dx, lattice.cols);
        let (r0, r1) = span(bounds.min[1], bounds.max[1], y0, dy, lattice.rows);
        let window = Window { lattice, c0, r0, cols: c1 - c0 + 1, rows: r1 - r0 + 1, origin };
        if window.cols >= 2 && window.rows >= 2 {
            // Above and below everything there: the top of the overburden and the bottom of the last layer.
            let heights = || stack.surfaces.iter().flat_map(|s| window.nodes().map(move |i| s[i])).filter(|z| !z.is_nan());
            let sky = heights().fold(bounds.max[2], f64::max) + margin;
            let basement = heights().fold(bounds.min[2], f64::min) - margin;
            let surface = |s: usize| -> Vec<f64> {
                window.nodes().map(|i| if s == 0 { sky } else if s > stack.surfaces.len() { basement } else { stack.surfaces[s - 1][i] }).map(|z| if z.is_nan() { basement } else { z }).collect()
            };
            for layer in 0..stack.layers() {
                let slab = slab_between(&window, &surface(layer), &surface(layer + 1), min_thickness);
                if slab.triangles.is_empty() {
                    continue;
                }
                slab_triangles += slab.triangle_count();
                let slab = to_manifold(&slab, [0.0; 3], "a layer's slab")?;
                if let Some(piece) = piece_of(&on.intersection(&slab), layer, origin)? {
                    pieces.push(piece);
                }
                slabs.push(slab);
            }
        }
    }
    let rest = if slabs.is_empty() { on } else { on.difference(&Manifold::batch_union(&slabs)) };
    rest.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    Ok(LayerCut { pieces, outside, thin: rest.volume(), input_volume, slab_triangles, ms: started.elapsed().as_secs_f64() * 1000.0 })
}

/// The part of a lattice under a solid, worked about a local origin.
struct Window {
    lattice: Lattice,
    c0: usize,
    r0: usize,
    cols: usize,
    rows: usize,
    origin: [f64; 3],
}

impl Window {
    /// The lattice's node indices in the window, row by row.
    fn nodes(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.rows).flat_map(move |r| (0..self.cols).map(move |c| (self.r0 + r) * self.lattice.cols + self.c0 + c))
    }

    /// A window node's place in plan, about the local origin.
    fn xy(&self, node: usize) -> [f64; 2] {
        let (c, r) = ((self.c0 + node % self.cols) as f64, (self.r0 + node / self.cols) as f64);
        [self.lattice.origin[0] + c * self.lattice.cell_size[0] - self.origin[0], self.lattice.origin[1] + r * self.lattice.cell_size[1] - self.origin[1]]
    }
}

/// A corner of a slab: at a node, or where its thickness passes the minimum along a grid edge (lower node first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Corner {
    Node(usize),
    Edge(usize, usize),
}

/// The slab between `upper` and `lower` (heights at the window's nodes) where it's thicker than `min`: each grid triangle
/// (the cell split south-west to north-east, as a grid is triangulated) clipped to where it is, with the upper surface on
/// top, the lower beneath, and a vertical wall round it, `min` high where the thickness passes it and full height at the
/// window's edge. Watertight: a corner along a grid edge is worked from the edge alone, so the triangles either side
/// share it.
fn slab_between(window: &Window, upper: &[f64], lower: &[f64], min: f64) -> TriMesh {
    let thickness: Vec<f64> = upper.iter().zip(lower).map(|(u, l)| u - l).collect();
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut index: HashMap<Corner, u32> = HashMap::new();
    let mut corner = |c: Corner, vertices: &mut Vec<[f64; 3]>| -> u32 {
        *index.entry(c).or_insert_with(|| {
            let ([x, y], top, bottom) = match c {
                Corner::Node(n) => (window.xy(n), upper[n], lower[n]),
                Corner::Edge(a, b) => {
                    let s = (min - thickness[a]) / (thickness[b] - thickness[a]);
                    let ([xa, ya], [xb, yb]) = (window.xy(a), window.xy(b));
                    ([xa + s * (xb - xa), ya + s * (yb - ya)], upper[a] + s * (upper[b] - upper[a]), lower[a] + s * (lower[b] - lower[a]))
                }
            };
            vertices.push([x, y, top]);
            vertices.push([x, y, bottom]);
            (vertices.len() - 2) as u32
        })
    };
    // Inside where thicker than the minimum. Where an edge leaves at a node exactly the minimum thick, the corner is
    // that node, so the triangles round it agree.
    let inside = |n: usize| thickness[n] > min;
    let mut tops: Vec<[u32; 3]> = Vec::new();
    let mut ring: Vec<u32> = Vec::with_capacity(4);
    for row in 0..window.rows - 1 {
        for col in 0..window.cols - 1 {
            let at = |c: usize, r: usize| (row + r) * window.cols + col + c;
            let (sw, se, ne, nw) = (at(0, 0), at(1, 0), at(1, 1), at(0, 1));
            for tri in [[sw, se, ne], [sw, ne, nw]] {
                if !tri.iter().any(|&n| inside(n)) {
                    continue;
                }
                ring.clear();
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    if inside(a) {
                        ring.push(corner(Corner::Node(a), &mut vertices));
                    }
                    if inside(a) != inside(b) {
                        let out = if inside(a) { b } else { a };
                        let c = if thickness[out] == min { Corner::Node(out) } else { Corner::Edge(a.min(b), a.max(b)) };
                        ring.push(corner(c, &mut vertices));
                    }
                }
                ring.dedup();
                if ring.len() > 1 && ring.first() == ring.last() {
                    ring.pop();
                }
                for k in 1..ring.len().saturating_sub(1) {
                    tops.push([ring[0], ring[k], ring[k + 1]]);
                }
            }
        }
    }
    // Walls on the top's edges with no twin: its rim.
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    for t in &tops {
        for k in 0..3 {
            *edges.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    let mut triangles = Vec::with_capacity(2 * tops.len() + edges.len());
    for t in &tops {
        triangles.push(*t);
        triangles.push([t[0] + 1, t[2] + 1, t[1] + 1]);
    }
    for t in &tops {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if !edges.contains_key(&(b, a)) {
                triangles.push([a + 1, b + 1, b]);
                triangles.push([a + 1, b, a]);
            }
        }
    }
    TriMesh::new(vertices, triangles)
}

/// A boolean's result as a piece, or nothing if it has no volume.
fn piece_of(m: &Manifold, layer: usize, origin: [f64; 3]) -> Result<Option<LayerPiece>, GeomError> {
    m.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    if m.is_empty() || m.volume() < NOTHING {
        return Ok(None);
    }
    let parts = m.decompose().len();
    let (mesh, tiny_edges) = join_tiny_edges(from_manifold(m, origin));
    let volume = mesh.signed_volume();
    Ok(Some(LayerPiece { layer, mesh, volume, parts, tiny_edges }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(cols: usize, rows: usize) -> Window {
        let lattice = Lattice { origin: [650_000.0, 7_450_000.0], cell_size: [10.0, 10.0], cols, rows };
        Window { lattice, c0: 0, r0: 0, cols, rows, origin: [650_000.0, 7_450_000.0, 0.0] }
    }

    #[test]
    fn a_slab_of_even_thickness_is_a_box() {
        let w = window(4, 3);
        let slab = slab_between(&w, &[110.0; 12], &[100.0; 12], 0.01);
        let v = slab.validity();
        assert!(v.is_watertight(), "{v:?}");
        assert!((slab.signed_volume() - 30.0 * 20.0 * 10.0).abs() < 1e-9);
    }

    #[test]
    fn a_slab_stops_where_it_thins_past_the_minimum() {
        // Thickness 2 m at x = 0 down to 0 at x = 20 m, straight: thicker than 0.5 m to x = 15 m. The slab is the
        // trapezoid from 2 m to 0.5 m thick over 15 m, 10 m wide: (2 + 0.5) / 2 * 15 * 10 = 187.5 m³.
        let w = window(3, 2);
        let upper = [102.0, 101.0, 100.0, 102.0, 101.0, 100.0];
        let slab = slab_between(&w, &upper, &[100.0; 6], 0.5);
        let v = slab.validity();
        assert!(v.is_watertight(), "{v:?}");
        assert!((slab.signed_volume() - 187.5).abs() < 1e-9, "{}", slab.signed_volume());
    }

    #[test]
    fn nodes_exactly_the_minimum_thick_close_cleanly() {
        // A ridge 1 m thick along the middle column, 0.5 m at the sides. At a minimum of 1 m nothing is thicker, so no
        // slab; at 0.5 m every node but the sides' is, and the sides are exactly the minimum: the slab is all of it.
        let w = window(3, 3);
        let upper = [100.5, 101.0, 100.5, 100.5, 101.0, 100.5, 100.5, 101.0, 100.5];
        assert!(slab_between(&w, &upper, &[100.0; 9], 1.0).triangles.is_empty());
        let slab = slab_between(&w, &upper, &[100.0; 9], 0.5);
        assert!(slab.validity().is_watertight());
        // Thickness 0.5 + 0.05 * (10 - |x - 10|) over 20 x 20 m: 20 * (0.5 * 20 + 0.05 * 100) = 300 m³.
        assert!((slab.signed_volume() - 300.0).abs() < 1e-9, "{}", slab.signed_volume());
    }
}
