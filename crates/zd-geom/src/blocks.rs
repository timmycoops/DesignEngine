//! Blocks cut from solids by polygons (decision 0062): each solid is cut vertically where each polygon sits, by one
//! Manifold boolean with the polygon's prism run past the solid's top and bottom. What lies outside every polygon is the
//! remainder. The pieces and the remainder add up to the solid. The polygons must cover the solid in plan, which
//! [`polygons_uncovered`] checks before a cut: a gap is the polygons to fix, not ground to leave out.
//!
//! Both are worked about a whole-metre origin near the solid, so mine grid coordinates keep their precision.

use std::time::Instant;

use manifold_csg::{CrossSection, JoinType, Manifold};
use zd_core::TriMesh;

use crate::cut::{from_manifold, to_manifold};
use crate::{GeomError, TinyEdges, join_tiny_edges};

/// A piece with less volume than this (m³) is nothing: a polygon only touching the solid.
const NOTHING: f64 = 1e-6;

/// A solid's piece inside one polygon.
#[derive(Debug, Clone)]
pub struct PolygonPiece {
    /// Which polygon, by its index in the input.
    pub polygon: usize,
    pub mesh: TriMesh,
    pub volume: f64,
    /// Separate parts in it: more than one where the polygon takes the solid in two places.
    pub parts: usize,
    pub tiny_edges: TinyEdges,
}

/// One solid cut by polygons.
#[derive(Debug, Clone)]
pub struct PolygonCut {
    /// A piece for each polygon the solid reaches into, in the polygons' order.
    pub pieces: Vec<PolygonPiece>,
    /// What lies outside every polygon, if anything.
    pub remainder: Option<PolygonPiece>,
    pub input_volume: f64,
    /// Polygons whose plan box came near the solid's, and so were cut with.
    pub polygons_near: usize,
    pub ms: f64,
}

impl PolygonCut {
    /// The pieces and remainder together, less the solid (m³): what the cut lost or gained.
    pub fn difference(&self) -> f64 {
        self.pieces.iter().chain(&self.remainder).map(|p| p.volume).sum::<f64>() - self.input_volume
    }
}

/// Cuts a closed solid by each polygon's vertical prism. Polygons are plan rings, either way round, the first point not
/// repeated at the end.
pub fn cut_solid_by_polygons(solid: &TriMesh, polygons: &[Vec<[f64; 2]>]) -> Result<PolygonCut, GeomError> {
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "solid", validity });
    }
    let started = Instant::now();
    let bounds = solid.bounds().ok_or_else(|| GeomError::InvalidInput("empty solid".into()))?;
    let centre = bounds.centre();
    let origin = [centre[0].round(), centre[1].round(), 0.0];
    let margin = 1.0 + 0.01 * bounds.size()[2];
    let (bottom, top) = (bounds.min[2] - margin, bounds.max[2] + margin);
    let solid_m = to_manifold(solid, [-origin[0], -origin[1], 0.0], "solid")?;
    let input_volume = solid.signed_volume();

    let mut prisms = Vec::new();
    let mut pieces = Vec::new();
    for (k, polygon) in polygons.iter().enumerate() {
        if polygon.len() < 3 || !near(polygon, [bounds.min[0], bounds.min[1]], [bounds.max[0], bounds.max[1]]) {
            continue;
        }
        let prism = prism(polygon, [origin[0], origin[1]], bottom, top);
        let inside = solid_m.intersection(&prism);
        prisms.push(prism);
        if let Some(piece) = piece_of(&inside, k, origin)? {
            pieces.push(piece);
        }
    }
    let polygons_near = prisms.len();
    let outside = if prisms.is_empty() { solid_m } else { solid_m.difference(&Manifold::batch_union(&prisms)) };
    let remainder = piece_of(&outside, usize::MAX, origin)?;
    Ok(PolygonCut { pieces, remainder, input_volume, polygons_near, ms: started.elapsed().as_secs_f64() * 1000.0 })
}

/// Gaps in the cover narrower than this (m), between two polygons or between a polygon and the solid's edge, are lines
/// drawn on the same place parting by rounding, not ground left out.
pub const COVER_SLIVER: f64 = 0.01;

/// A gap in the cover smaller than this in plan (m²) is nothing.
const COVER_AREA: f64 = 0.01;

/// A part of a solid's footprint in plan outside every polygon.
#[derive(Debug, Clone, PartialEq)]
pub struct Uncovered {
    /// Its area in plan (m²).
    pub area: f64,
    /// About its middle, in mine-grid coordinates.
    pub at: [f64; 2],
}

/// Where the polygons leave a closed solid uncovered in plan: each part of its footprint outside every polygon, largest
/// first. Empty when they cover it. Slivers narrower than [`COVER_SLIVER`] are left out. Polygons are plan rings either way
/// round, the first point not repeated at the end.
pub fn polygons_uncovered(solid: &TriMesh, polygons: &[Vec<[f64; 2]>]) -> Result<Vec<Uncovered>, GeomError> {
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "solid", validity });
    }
    let bounds = solid.bounds().ok_or_else(|| GeomError::InvalidInput("empty solid".into()))?;
    let centre = bounds.centre();
    let origin = [centre[0].round(), centre[1].round()];
    let solid_m = to_manifold(solid, [-origin[0], -origin[1], 0.0], "solid")?;
    let footprint = CrossSection::from_polygons(&solid_m.project());
    let near: Vec<CrossSection> = polygons
        .iter()
        .filter(|p| p.len() >= 3 && near(p, [bounds.min[0], bounds.min[1]], [bounds.max[0], bounds.max[1]]))
        .map(|p| section(p, origin))
        .collect();
    let gap = if near.is_empty() { footprint } else { footprint.difference(&CrossSection::batch_union(&near)) };
    Ok(uncovered_parts(&gap, origin))
}

/// Each part of a gap in plan worked about `origin`, largest first, with slivers narrower than [`COVER_SLIVER`] and
/// specks under a hundredth of a square metre left out.
pub(crate) fn uncovered_parts(gap: &CrossSection, origin: [f64; 2]) -> Vec<Uncovered> {
    // Opened by half the sliver width: what's thinner than a sliver closes up and is gone.
    let half = COVER_SLIVER / 2.0;
    let gap = gap.offset(-half, JoinType::Miter, 2.0, 0).offset(half, JoinType::Miter, 2.0, 0);
    let mut found: Vec<Uncovered> = gap
        .decompose()
        .into_iter()
        .filter_map(|part| {
            let area = part.area();
            if area < COVER_AREA {
                return None;
            }
            let [x, y] = middle(&part);
            Some(Uncovered { area, at: [origin[0] + x, origin[1] + y] })
        })
        .collect();
    found.sort_by(|a, b| b.area.total_cmp(&a.area));
    found
}

/// About the middle of a part in plan: its outer ring's centroid, or the middle of its box where that falls outside it.
fn middle(part: &CrossSection) -> [f64; 2] {
    let rings = part.to_polygons();
    let Some(outer) = rings.iter().max_by(|a, b| ring_area(a).abs().total_cmp(&ring_area(b).abs())) else { return [0.0, 0.0] };
    let (twice, mut cx, mut cy) = (2.0 * ring_area(outer), 0.0, 0.0);
    for k in 0..outer.len() {
        let (p, q) = (outer[k], outer[(k + 1) % outer.len()]);
        let cross = p[0] * q[1] - q[0] * p[1];
        cx += (p[0] + q[0]) * cross;
        cy += (p[1] + q[1]) * cross;
    }
    if twice.abs() > 0.0 {
        let c = [cx / (3.0 * twice), cy / (3.0 * twice)];
        if inside(outer, c) {
            return c;
        }
    }
    let (lo, hi) = plan_box(outer);
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
}

/// A ring's signed area (anticlockwise positive).
fn ring_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n).map(|k| ring[k][0] * ring[(k + 1) % n][1] - ring[(k + 1) % n][0] * ring[k][1]).sum::<f64>() / 2.0
}

/// Whether a point lies inside a ring (crossing count).
fn inside(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = ring.len();
    let mut within = false;
    for k in 0..n {
        let (a, b) = (ring[k], ring[(k + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            within = !within;
        }
    }
    within
}

/// Whether two plan rings overlap by more than `least` (m²): each overlap as the two polygons, its area and its centre.
pub fn polygons_overlapping(polygons: &[Vec<[f64; 2]>], least: f64) -> Vec<(usize, usize, f64, [f64; 2])> {
    let Some(first) = polygons.iter().find_map(|p| p.first()) else { return Vec::new() };
    let origin = [first[0].round(), first[1].round()];
    let boxes: Vec<([f64; 2], [f64; 2])> = polygons.iter().map(|p| plan_box(p)).collect();
    let sections: Vec<CrossSection> = polygons.iter().map(|p| section(p, origin)).collect();
    let mut found = Vec::new();
    for i in 0..polygons.len() {
        for j in i + 1..polygons.len() {
            let ((lo_i, hi_i), (lo_j, hi_j)) = (boxes[i], boxes[j]);
            if lo_i[0] > hi_j[0] || lo_j[0] > hi_i[0] || lo_i[1] > hi_j[1] || lo_j[1] > hi_i[1] {
                continue;
            }
            let both = sections[i].intersection(&sections[j]);
            let area = both.area();
            if area > least {
                let points: Vec<[f64; 2]> = both.to_polygons().into_iter().flatten().collect();
                let n = points.len().max(1) as f64;
                let centre = points.iter().fold([0.0, 0.0], |c, p| [c[0] + p[0] / n, c[1] + p[1] / n]);
                found.push((i, j, area, [origin[0] + centre[0], origin[1] + centre[1]]));
            }
        }
    }
    found
}

/// A boolean's result as a piece, or nothing if it has no volume.
fn piece_of(m: &Manifold, polygon: usize, origin: [f64; 3]) -> Result<Option<PolygonPiece>, GeomError> {
    m.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    if m.is_empty() || m.volume() < NOTHING {
        return Ok(None);
    }
    let parts = m.decompose().len();
    let (mesh, tiny_edges) = join_tiny_edges(from_manifold(m, origin));
    let volume = mesh.signed_volume();
    Ok(Some(PolygonPiece { polygon, mesh, volume, parts, tiny_edges }))
}

/// The polygon's prism from `bottom` to `top`, about the local origin.
fn prism(polygon: &[[f64; 2]], origin: [f64; 2], bottom: f64, top: f64) -> Manifold {
    section(polygon, origin).extrude(top - bottom).translate(0.0, 0.0, bottom)
}

/// The polygon in plan about the local origin, anticlockwise.
fn section(polygon: &[[f64; 2]], origin: [f64; 2]) -> CrossSection {
    let mut ring: Vec<[f64; 2]> = polygon.iter().map(|p| [p[0] - origin[0], p[1] - origin[1]]).collect();
    let n = ring.len();
    let twice: f64 = (0..n).map(|k| ring[k][0] * ring[(k + 1) % n][1] - ring[(k + 1) % n][0] * ring[k][1]).sum();
    if twice < 0.0 {
        ring.reverse();
    }
    CrossSection::from_polygons(&[ring])
}

fn plan_box(polygon: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    polygon.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]))
}

/// Whether the polygon's plan box meets the solid's.
fn near(polygon: &[[f64; 2]], lo: [f64; 2], hi: [f64; 2]) -> bool {
    let (plo, phi) = plan_box(polygon);
    plo[0] <= hi[0] && phi[0] >= lo[0] && plo[1] <= hi[1] && phi[1] >= lo[1]
}
