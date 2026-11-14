//! Solids cut by a seam model's horizons in one run (decision 0062, workstream 3).
//!
//! The working section's roofs and floors, top first, split the ground into layers: overburden above the first roof,
//! each seam between its roof and floor, each interburden between a floor and the next roof down, and what lies below
//! the last floor. Each layer is made as a slab between its two grids where it's thicker than a minimum, clipped exactly
//! where its thickness passes that minimum inside each grid triangle, with a wall of the minimum's height there; a seam
//! with no thickness, at a subcrop or a washout, then makes no slab at all, and nothing coincides. A solid's piece of a
//! layer is one Manifold intersection with its slab. What the slabs leave out, where a layer is thinner than the
//! minimum, is said by its volume. The grids must reach over the whole solid with no hole under it, which
//! [`solid_off_the_grids`] finds before the cut (decision 0063); what lies off them is kept apart, so the cut adds up.
//!
//! Everything is worked about a whole-metre origin near the solid, so mine grid coordinates keep their precision.

use std::collections::HashMap;
use std::time::Instant;

use manifold_csg::{CrossSection, JoinType, Manifold};
use zd_core::{Lattice, TriMesh};

use crate::blocks::{Uncovered, uncovered_parts};
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
    /// Flaps taken off the pieces, where a piece is thinner than the flap thickness between faces nearly flat (its top
    /// and bottom, as where a strip's berm lies along a seam's roof): how many pieces had them, and their volume (m³).
    pub flapped: usize,
    pub flaps: f64,
    pub input_volume: f64,
    /// Triangles in the slabs it was cut by.
    pub slab_triangles: usize,
    pub ms: f64,
}

impl LayerCut {
    /// The pieces, what's off the grids and what's too thin together, less the solid (m³): what the cut lost or gained.
    pub fn difference(&self) -> f64 {
        self.pieces.iter().chain(&self.outside).map(|p| p.volume).sum::<f64>() + self.thin + self.flaps - self.input_volume
    }
}

/// Cuts a closed solid by every layer of `stack` thicker than `min_thickness`: one piece a layer it reaches into, with
/// its flaps thinner than `flap_thickness` taken off (none with 0).
pub fn cut_solid_by_layers(solid: &TriMesh, stack: &LayerStack, min_thickness: f64, flap_thickness: f64) -> Result<LayerCut, GeomError> {
    if !(min_thickness > 0.0 && min_thickness.is_finite()) {
        return Err(GeomError::InvalidInput("the minimum thickness must be more than nothing".into()));
    }
    if !(flap_thickness >= 0.0 && flap_thickness.is_finite()) {
        return Err(GeomError::InvalidInput("the flap thickness can't be less than nothing".into()));
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
    let (mut flapped, mut flaps) = (0, 0.0);
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
                let (inside, flap) = without_flaps(on.intersection(&slab), flap_thickness)?;
                if flap > 0.0 {
                    flapped += 1;
                    flaps += flap;
                }
                if let Some(piece) = piece_of(&inside, layer, origin)? {
                    pieces.push(piece);
                }
                slabs.push(slab);
            }
        }
    }
    let rest = if slabs.is_empty() { on } else { on.difference(&Manifold::batch_union(&slabs)) };
    rest.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    Ok(LayerCut { pieces, outside, thin: rest.volume(), flapped, flaps, input_volume, slab_triangles, ms: started.elapsed().as_secs_f64() * 1000.0 })
}

/// Faces steeper than this (the up component of their normal less than this) aren't the top or bottom of a flap: a
/// flap lies along a seam, and seams dip under 30°.
const FLAP_FACE: f64 = 0.5;

/// Flaps with less area than this in plan (m²) are left: rounding, not a flap.
const FLAP_AREA: f64 = 1e-6;

/// How far past where it's the flap thickness a flap is cut out, in plan (m). Where a berm lies on a seam's roof, the
/// piece is the flap thickness thick just where the layer is the minimum thickness thick, so a cut there would stand on
/// the layer's own wall at its edge, two walls a hair apart.
const FLAP_REACH: f64 = 0.005;

/// A layer's piece with its flaps taken off (decision 0064), and their volume (m³). A flap is where the piece's top
/// and bottom, nearly flat faces, one facing up over one facing down, come within `thickness` of each other: a knife
/// edge where a strip's surface runs into a seam's roof or floor at a low angle, or a sheet with no thickness where the
/// two lie on each other, as a berm built on the seam's roof does. It's cut out straight down where the two faces are
/// that close, as Deswik removes flaps before cutting. A steep wall meeting a seam is no flap.
fn without_flaps(piece: Manifold, thickness: f64) -> Result<(Manifold, f64), GeomError> {
    piece.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    if thickness <= 0.0 || piece.is_empty() {
        return Ok((piece, 0.0));
    }
    let mesh = from_manifold(&piece, [0.0; 3]);
    let (outline, [low, high]) = flap_outline(&mesh, thickness);
    if outline.is_empty() {
        return Ok((piece, 0.0));
    }
    let sections: Vec<CrossSection> = outline.iter().map(|p| CrossSection::from_polygons(std::slice::from_ref(p))).collect();
    let area = CrossSection::batch_union(&sections).offset(FLAP_REACH, JoinType::Square, 2.0, 0);
    if area.is_empty() || area.area() < FLAP_AREA {
        return Ok((piece, 0.0));
    }
    let prism = area.extrude(high - low + 2.0).translate(0.0, 0.0, low - 1.0);
    let kept = piece.difference(&prism);
    kept.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    let removed = piece.volume() - kept.volume();
    Ok((kept, removed.max(0.0)))
}

type Tri = [[f64; 3]; 3];

/// The up component of a triangle's unit normal.
fn facing_up(a: &Tri) -> f64 {
    let (u, v) = ([a[1][0] - a[0][0], a[1][1] - a[0][1], a[1][2] - a[0][2]], [a[2][0] - a[0][0], a[2][1] - a[0][1], a[2][2] - a[0][2]]);
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-15 { 0.0 } else { n[2] / len }
}

/// A triangle's lowest and highest corner.
fn tri_box(a: &Tri) -> ([f64; 3], [f64; 3]) {
    let lo = [0, 1, 2].map(|k| a[0][k].min(a[1][k]).min(a[2][k]));
    let hi = [0, 1, 2].map(|k| a[0][k].max(a[1][k]).max(a[2][k]));
    (lo, hi)
}

/// The height of a (not vertical) triangle's plane over a plan point.
fn plane_height(a: &Tri, p: [f64; 2]) -> f64 {
    let d = (a[1][0] - a[0][0]) * (a[2][1] - a[0][1]) - (a[2][0] - a[0][0]) * (a[1][1] - a[0][1]);
    let u = ((p[0] - a[0][0]) * (a[2][1] - a[0][1]) - (a[2][0] - a[0][0]) * (p[1] - a[0][1])) / d;
    let w = ((a[1][0] - a[0][0]) * (p[1] - a[0][1]) - (p[0] - a[0][0]) * (a[1][1] - a[0][1])) / d;
    a[0][2] + u * (a[1][2] - a[0][2]) + w * (a[2][2] - a[0][2])
}

/// Where a closed mesh's nearly flat faces, one facing up over one facing down, are within `thickness` of each other (or
/// pass through each other by up to it), as convex polygons in plan wound anticlockwise, and the lowest and highest
/// they reach.
fn flap_outline(mesh: &TriMesh, thickness: f64) -> (Vec<Vec<[f64; 2]>>, [f64; 2]) {
    let (mut tops, mut bottoms): (Vec<Tri>, Vec<Tri>) = (Vec::new(), Vec::new());
    for t in &mesh.triangles {
        let a: Tri = t.map(|i| mesh.vertices[i as usize]);
        let up = facing_up(&a);
        if up > FLAP_FACE {
            tops.push(a);
        } else if up < -FLAP_FACE {
            bottoms.push(a);
        }
    }
    let mut out = Vec::new();
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    if tops.is_empty() || bottoms.is_empty() {
        return (out, [low, high]);
    }
    // The bottoms by plan cell.
    let (lo, hi) = bottoms.iter().map(tri_box).fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), (a, b)| ([lo[0].min(a[0]), lo[1].min(a[1])], [hi[0].max(b[0]), hi[1].max(b[1])]));
    let cell = (((hi[0] - lo[0]).max(1e-3) * (hi[1] - lo[1]).max(1e-3) / bottoms.len() as f64).sqrt() * 2.0).max(1e-3);
    let (cols, rows) = (((hi[0] - lo[0]) / cell) as usize + 1, ((hi[1] - lo[1]) / cell) as usize + 1);
    let at = |v: f64, o: f64, n: usize| (((v - o) / cell).floor().max(0.0) as usize).min(n - 1);
    let mut cells: Vec<Vec<u32>> = vec![Vec::new(); cols * rows];
    for (k, b) in bottoms.iter().enumerate() {
        let (a, z) = tri_box(b);
        for r in at(a[1], lo[1], rows)..=at(z[1], lo[1], rows) {
            for c in at(a[0], lo[0], cols)..=at(z[0], lo[0], cols) {
                cells[r * cols + c].push(k as u32);
            }
        }
    }
    let mut seen: Vec<u32> = Vec::new();
    for top in &tops {
        let (a, z) = tri_box(top);
        if z[0] < lo[0] || a[0] > hi[0] || z[1] < lo[1] || a[1] > hi[1] {
            continue;
        }
        seen.clear();
        for r in at(a[1], lo[1], rows)..=at(z[1], lo[1], rows) {
            for c in at(a[0], lo[0], cols)..=at(z[0], lo[0], cols) {
                seen.extend(&cells[r * cols + c]);
            }
        }
        seen.sort_unstable();
        seen.dedup();
        for &k in &seen {
            let bottom = &bottoms[k as usize];
            let (ba, bz) = tri_box(bottom);
            if bz[0] < a[0] || ba[0] > z[0] || bz[1] < a[1] || ba[1] > z[1] || ba[2] > z[2] + thickness || bz[2] < a[2] - thickness {
                continue;
            }
            // Where the two overlap in plan, and there where they're within the thickness of each other.
            let gap = |p: [f64; 2]| plane_height(top, p) - plane_height(bottom, p);
            let mut poly = plan_overlap(top, bottom);
            if poly.len() >= 3 {
                poly = clip_where(&poly, |p| thickness - gap(p));
            }
            if poly.len() >= 3 {
                poly = clip_where(&poly, |p| gap(p) + thickness);
            }
            if poly.len() < 3 || polygon_area(&poly).abs() < FLAP_AREA {
                continue;
            }
            for p in &poly {
                for z in [plane_height(top, *p), plane_height(bottom, *p)] {
                    low = low.min(z);
                    high = high.max(z);
                }
            }
            if polygon_area(&poly) < 0.0 {
                poly.reverse();
            }
            out.push(poly);
        }
    }
    (out, [low, high])
}

/// Two triangles' overlap in plan, a convex polygon.
fn plan_overlap(a: &Tri, b: &Tri) -> Vec<[f64; 2]> {
    let turn = (b[1][0] - b[0][0]) * (b[2][1] - b[0][1]) - (b[2][0] - b[0][0]) * (b[1][1] - b[0][1]) > 0.0;
    let mut poly: Vec<[f64; 2]> = a.iter().map(|p| [p[0], p[1]]).collect();
    for k in 0..3 {
        let (p, q) = (b[k], b[(k + 1) % 3]);
        poly = clip_where(&poly, |r| {
            let s = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            if turn { s } else { -s }
        });
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

/// A convex polygon clipped to where `inside` is not negative (a linear function in plan).
fn clip_where(poly: &[[f64; 2]], inside: impl Fn([f64; 2]) -> f64) -> Vec<[f64; 2]> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (r, s) = (poly[i], poly[(i + 1) % poly.len()]);
        let (dr, ds) = (inside(r), inside(s));
        if dr >= 0.0 {
            out.push(r);
        }
        if (dr >= 0.0) != (ds >= 0.0) {
            let t = dr / (dr - ds);
            out.push([r[0] + t * (s[0] - r[0]), r[1] + t * (s[1] - r[1])]);
        }
    }
    out
}

/// A polygon's area in plan, positive anticlockwise.
fn polygon_area(poly: &[[f64; 2]]) -> f64 {
    0.5 * (0..poly.len()).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        a[0] * b[1] - b[0] * a[1]
    }).sum::<f64>()
}

/// Where a solid can't be cut by a seam model's grids, found before the cut (decision 0063).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GridCover {
    /// Each part of its footprint in plan past the grids' edge, largest first.
    pub past_edge: Vec<Uncovered>,
    /// Each part of its footprint over a grid cell with a hole at a corner, largest first.
    pub over_holes: Vec<Uncovered>,
}

impl GridCover {
    /// The grids reach over the whole solid, with no hole under it.
    pub fn covered(&self) -> bool {
        self.past_edge.is_empty() && self.over_holes.is_empty()
    }
}

/// Where a closed solid reaches past a seam model's grids in plan, or over a hole in them: `holes` flags each node of
/// `lattice` (`row * cols + col`) where a working horizon holds no height. A cell with a hole at any corner can't place
/// the layers, so the whole cell counts. Slivers narrower than [`COVER_SLIVER`](crate::COVER_SLIVER) are left out.
/// Nothing is filled: a hole is the geology model's to fix.
pub fn solid_off_the_grids(solid: &TriMesh, lattice: Lattice, holes: &[bool]) -> Result<GridCover, GeomError> {
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "solid", validity });
    }
    let (cols, rows) = (lattice.cols, lattice.rows);
    if cols < 2 || rows < 2 || holes.len() != cols * rows {
        return Err(GeomError::InvalidInput("the holes must flag every node of a lattice two nodes or more each way".into()));
    }
    let bounds = solid.bounds().ok_or_else(|| GeomError::InvalidInput("empty solid".into()))?;
    let centre = bounds.centre();
    let origin = [centre[0].round(), centre[1].round()];
    let solid_m = to_manifold(solid, [-origin[0], -origin[1], 0.0], "solid")?;
    let footprint = CrossSection::from_polygons(&solid_m.project());
    let [x0, y0] = lattice.origin;
    let [dx, dy] = lattice.cell_size;
    let node = |c: usize, r: usize| [x0 + c as f64 * dx - origin[0], y0 + r as f64 * dy - origin[1]];
    let rect = |a: [f64; 2], b: [f64; 2]| CrossSection::from_polygons(&[vec![a, [b[0], a[1]], b, [a[0], b[1]]]]);
    let past_edge = uncovered_parts(&footprint.difference(&rect(node(0, 0), node(cols - 1, rows - 1))), origin);

    // The cells under the solid with a hole at a corner, a row's run of them at a time.
    let first = |lo: f64, start: f64, step: f64, count: usize| (((lo - start) / step).floor().max(0.0) as usize).min(count - 2);
    let last = |hi: f64, start: f64, step: f64, count: usize| (((hi - start) / step).ceil().max(0.0) as usize).min(count - 1);
    let (c0, c1) = (first(bounds.min[0], x0, dx, cols), last(bounds.max[0], x0, dx, cols));
    let (r0, r1) = (first(bounds.min[1], y0, dy, rows), last(bounds.max[1], y0, dy, rows));
    let hole = |c: usize, r: usize| holes[r * cols + c];
    let mut runs = Vec::new();
    for r in r0..r1 {
        let mut run = None;
        for c in c0..=c1 {
            let bad = c < c1 && (hole(c, r) || hole(c + 1, r) || hole(c, r + 1) || hole(c + 1, r + 1));
            match (bad, run) {
                (true, None) => run = Some(c),
                (false, Some(start)) => {
                    runs.push(rect(node(start, r), node(c, r + 1)));
                    run = None;
                }
                _ => {}
            }
        }
    }
    let over_holes = if runs.is_empty() { Vec::new() } else { uncovered_parts(&footprint.intersection(&CrossSection::batch_union(&runs)), origin) };
    Ok(GridCover { past_edge, over_holes })
}

/// One hole in a grid: nodes holding no height, joined side to side or corner to corner.
#[derive(Debug, Clone, PartialEq)]
pub struct GridHole {
    pub nodes: usize,
    /// Its node nearest the middle of its nodes, in mine-grid coordinates.
    pub at: [f64; 2],
}

/// The holes among a lattice's flagged nodes (`row * cols + col`), largest first.
pub fn grid_hole_patches(lattice: Lattice, holes: &[bool]) -> Vec<GridHole> {
    let (cols, rows) = (lattice.cols, lattice.rows);
    let xy = |i: usize| [lattice.origin[0] + (i % cols) as f64 * lattice.cell_size[0], lattice.origin[1] + (i / cols) as f64 * lattice.cell_size[1]];
    let mut seen = vec![false; holes.len().min(cols * rows)];
    let mut found = Vec::new();
    for start in 0..seen.len() {
        if seen[start] || !holes[start] {
            continue;
        }
        seen[start] = true;
        let (mut patch, mut next) = (Vec::new(), vec![start]);
        while let Some(i) = next.pop() {
            patch.push(i);
            let (c, r) = ((i % cols) as isize, (i / cols) as isize);
            for (dc, dr) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nc, nr) = (c + dc, r + dr);
                if nc < 0 || nr < 0 || nc >= cols as isize || nr >= rows as isize {
                    continue;
                }
                let j = nr as usize * cols + nc as usize;
                if holes[j] && !seen[j] {
                    seen[j] = true;
                    next.push(j);
                }
            }
        }
        let n = patch.len() as f64;
        let mean = patch.iter().map(|&i| xy(i)).fold([0.0, 0.0], |[a, b], [x, y]| [a + x / n, b + y / n]);
        let nearest = patch.iter().map(|&i| xy(i)).min_by(|p, q| ((p[0] - mean[0]).hypot(p[1] - mean[1])).total_cmp(&(q[0] - mean[0]).hypot(q[1] - mean[1])));
        found.push(GridHole { nodes: patch.len(), at: nearest.unwrap_or(mean) });
    }
    found.sort_by_key(|h| std::cmp::Reverse(h.nodes));
    found
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
