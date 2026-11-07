//! A solid between two surfaces inside a closed string (decision 0049): the large solid a strip's faces slice, such
//! as the topography down to the lowest seam's floor inside the pit's limit, made in the app.

use std::time::Instant;

use manifold_csg::{CrossSection, Manifold};
use zd_core::{Aabb, MeshValidity, TriMesh};

use crate::cut::{design_near, from_manifold, to_manifold};
use crate::{GeomError, Side, close_surface_to_halfspace};

/// A solid between two surfaces.
#[derive(Debug, Clone)]
pub struct Between {
    pub mesh: TriMesh,
    pub volume: f64,
    pub validity: MeshValidity,
    /// Separate parts it's in.
    pub parts: usize,
    /// The string's area in plan (m²).
    pub area: f64,
    /// Of the string's area, how much the top doesn't cover in plan, and the bottom (m²): no solid is made there.
    pub top_uncovered: f64,
    pub bottom_uncovered: f64,
    pub ms: f64,
}

/// The solid inside the closed string `outline` (in plan), below `top` and above `bottom`, with vertical sides on
/// the string. Where the bottom is above the top there's nothing, and where either doesn't cover the string in plan
/// there's nothing either, and the area is said.
pub fn solid_between(top: &TriMesh, bottom: &TriMesh, outline: &[[f64; 2]]) -> Result<Between, GeomError> {
    let start = Instant::now();
    if outline.len() < 3 {
        return Err(GeomError::InvalidInput("the string needs three points or more to close round an area".into()));
    }
    let (lo, hi) = outline.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
    let plan = Aabb { min: [lo[0], lo[1], 0.0], max: [hi[0], hi[1], 0.0] };
    let reach = 1.0 + 0.01 * (hi[0] - lo[0]).hypot(hi[1] - lo[1]);
    // Only the surfaces over the string matter: a topography of millions of triangles costs what its few over it cost.
    let (top, bottom) = (design_near(top, &plan, reach), design_near(bottom, &plan, reach));
    let (Some(tb), Some(bb)) = (top.bounds(), bottom.bounds()) else {
        return Err(GeomError::Refused("the top or the bottom has nothing over the string".into()));
    };
    let margin = 1.0 + 0.01 * (tb.max[2].max(bb.max[2]) - tb.min[2].min(bb.min[2]));
    let (z_low, z_high) = (tb.min[2].min(bb.min[2]) - margin, tb.max[2].max(bb.max[2]) + margin);

    let origin = [((lo[0] + hi[0]) / 2.0).round(), ((lo[1] + hi[1]) / 2.0).round(), 0.0];
    let to_local = [-origin[0], -origin[1], 0.0];
    let mut ring: Vec<[f64; 2]> = outline.iter().map(|p| [p[0] - origin[0], p[1] - origin[1]]).collect();
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    let area = CrossSection::from_polygons(std::slice::from_ref(&ring));
    if area.is_empty() {
        return Err(GeomError::Refused("the string closes round no area".into()));
    }
    let prism = Manifold::extrude(&area, z_high - z_low).translate(0.0, 0.0, z_low);
    let below_top = close_surface_to_halfspace(&top.translated(to_local), Side::Below, z_low)?;
    let above_bottom = close_surface_to_halfspace(&bottom.translated(to_local), Side::Above, z_high)?;
    let below_top = to_manifold(&below_top.mesh, [0.0; 3], "the half-space below the top")?;
    let above_bottom = to_manifold(&above_bottom.mesh, [0.0; 3], "the half-space above the bottom")?;
    let solid = prism.intersection(&below_top).intersection(&above_bottom);
    solid.status().map_err(|e| GeomError::Manifold(e.to_string()))?;

    let uncovered = |half: &Manifold| area.difference(&CrossSection::from_polygons(&half.project())).area();
    let mesh = from_manifold(&solid, origin);
    Ok(Between {
        volume: mesh.signed_volume(),
        validity: mesh.validity(),
        parts: solid.decompose().len(),
        area: area.area(),
        top_uncovered: uncovered(&below_top),
        bottom_uncovered: uncovered(&above_bottom),
        mesh,
        ms: start.elapsed().as_secs_f64() * 1e3,
    })
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let mut twice = 0.0;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        twice += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * twice
}
