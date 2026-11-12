//! Blocks cut from solids by polygons (decision 0062): each solid is cut vertically where each polygon sits, by one
//! Manifold boolean with the polygon's prism run past the solid's top and bottom. What lies outside every polygon is the
//! remainder. The pieces and the remainder add up to the solid.
//!
//! Both are worked about a whole-metre origin near the solid, so mine grid coordinates keep their precision.

use std::time::Instant;

use manifold_csg::{CrossSection, Manifold};
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
