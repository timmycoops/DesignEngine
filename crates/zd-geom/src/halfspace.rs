use manifold_csg::triangulate_polygons;
use zd_core::TriMesh;

use crate::{GeomError, boundary_loops, orient_upward};

/// Which side of the surface the closed solid fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
}

/// How the flat cap of a closed half-space was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapMethod {
    /// Constrained Delaunay triangulation of the boundary loops (few triangles).
    Triangulated,
    /// The surface's own triangles flattened onto the cap plane (fallback when the boundary
    /// cannot be triangulated as a polygon).
    Projected,
}

#[derive(Debug, Clone)]
pub struct ClosedHalfSpace {
    pub mesh: TriMesh,
    pub cap_method: CapMethod,
    /// Triangles whose winding was changed to orient the surface consistently.
    pub surface_flips: usize,
}

/// Closes an open surface into a solid: the region between the surface and a flat cap at
/// `cap_z`, below it (`Side::Below`) or above it (`Side::Above`), with vertical walls under the
/// surface's boundary. The surface must be single-valued in plan.
pub fn close_surface_to_halfspace(surface: &TriMesh, side: Side, cap_z: f64) -> Result<ClosedHalfSpace, GeomError> {
    let bounds = surface
        .bounds()
        .ok_or_else(|| GeomError::InvalidInput("cannot close an empty surface".into()))?;
    let cap_clear = match side {
        Side::Below => cap_z < bounds.min[2],
        Side::Above => cap_z > bounds.max[2],
    };
    if !cap_clear {
        return Err(GeomError::InvalidInput(format!(
            "cap at z = {cap_z} must lie beyond the surface (z {} to {})",
            bounds.min[2], bounds.max[2]
        )));
    }

    // Wind the surface outward from the solid: up for the region below, down for above.
    let (up, surface_flips) = orient_upward(surface)?;
    let sheet = match side {
        Side::Below => up,
        Side::Above => up.flipped(),
    };
    let loops = boundary_loops(&sheet)?;

    let mut mesh = sheet.clone();
    let n = sheet.vertices.len() as u32;
    // Cap vertex for surface vertex v is n + v; only boundary ones are used by walls and a
    // triangulated cap, all of them by a projected cap.
    mesh.vertices.extend(sheet.vertices.iter().map(|p| [p[0], p[1], cap_z]));
    for ring in &loops {
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            mesh.triangles.push([n + a, n + b, b]);
            mesh.triangles.push([n + a, b, a]);
        }
    }

    let walls_end = mesh.triangles.len();
    let projected_cap = |mesh: &mut TriMesh| {
        mesh.triangles.extend(sheet.triangles.iter().map(|&[a, b, c]| [n + a, n + c, n + b]));
    };
    let mut cap_method = match triangulated_cap(&sheet, &loops, side) {
        Some(cap) => {
            mesh.triangles.extend(cap.iter().map(|t| t.map(|v| n + v)));
            CapMethod::Triangulated
        }
        None => {
            projected_cap(&mut mesh);
            CapMethod::Projected
        }
    };
    let mut validity = mesh.validity();
    if cap_method == CapMethod::Triangulated && !validity.is_watertight() {
        mesh.triangles.truncate(walls_end);
        projected_cap(&mut mesh);
        cap_method = CapMethod::Projected;
        validity = mesh.validity();
    }
    drop_unused_vertices(&mut mesh);

    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "closed half-space", validity });
    }
    Ok(ClosedHalfSpace { mesh, cap_method, surface_flips })
}

/// Cap triangles (surface vertex indices) wound opposite to the boundary loops, so they close
/// the walls. `None` if the loops do not triangulate as polygons in plan.
fn triangulated_cap(sheet: &TriMesh, loops: &[Vec<u32>], side: Side) -> Option<Vec<[u32; 3]>> {
    // Triangulate in plan about a local origin; outer rings counter-clockwise, holes clockwise.
    let origin = sheet.bounds()?.min;
    let plan = |v: u32| {
        let p = sheet.vertices[v as usize];
        [p[0] - origin[0], p[1] - origin[1]]
    };
    let rings: Vec<Vec<u32>> = loops
        .iter()
        .map(|ring| match side {
            Side::Below => ring.clone(),
            Side::Above => ring.iter().rev().copied().collect(),
        })
        .collect();
    let areas: Vec<f64> = rings.iter().map(|r| signed_plan_area(&r.iter().map(|&v| plan(v)).collect::<Vec<_>>())).collect();

    let mut cap = Vec::new();
    for (outer, ring) in rings.iter().enumerate().filter(|(i, _)| areas[*i] > 0.0) {
        let outer_plan: Vec<[f64; 2]> = ring.iter().map(|&v| plan(v)).collect();
        let mut polygon = vec![outer_plan.clone()];
        let mut index_map: Vec<u32> = ring.clone();
        for (hole, hole_ring) in rings.iter().enumerate().filter(|(i, _)| areas[*i] < 0.0) {
            if containing_outer(&rings, &areas, hole, &plan) == Some(outer) {
                polygon.push(hole_ring.iter().map(|&v| plan(v)).collect());
                index_map.extend_from_slice(hole_ring);
            }
        }
        for tri in triangulate_polygons(&polygon, 1e-9)? {
            let [a, b, c] = tri.map(|i| index_map[i as usize]);
            // Flat cap: wind each triangle so it runs opposite to the loops.
            let ccw = signed_plan_area(&[plan(a), plan(b), plan(c)]) > 0.0;
            let want_ccw = side == Side::Above;
            cap.push(if ccw == want_ccw { [a, b, c] } else { [a, c, b] });
        }
    }
    Some(cap)
}

/// Smallest counter-clockwise ring containing the first vertex of ring `hole`.
fn containing_outer(rings: &[Vec<u32>], areas: &[f64], hole: usize, plan: &impl Fn(u32) -> [f64; 2]) -> Option<usize> {
    let probe = plan(rings[hole][0]);
    rings
        .iter()
        .enumerate()
        .filter(|(i, _)| areas[*i] > 0.0)
        .filter(|(_, ring)| point_in_ring(probe, &ring.iter().map(|&v| plan(v)).collect::<Vec<_>>()))
        .min_by(|(i, _), (j, _)| areas[*i].total_cmp(&areas[*j]))
        .map(|(i, _)| i)
}

fn signed_plan_area(ring: &[[f64; 2]]) -> f64 {
    let mut twice = 0.0;
    for i in 0..ring.len() {
        let p = ring[i];
        let q = ring[(i + 1) % ring.len()];
        twice += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * twice
}

fn point_in_ring(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn drop_unused_vertices(mesh: &mut TriMesh) {
    let mut remap = vec![u32::MAX; mesh.vertices.len()];
    let mut vertices = Vec::new();
    for tri in &mut mesh.triangles {
        for v in tri.iter_mut() {
            if remap[*v as usize] == u32::MAX {
                remap[*v as usize] = vertices.len() as u32;
                vertices.push(mesh.vertices[*v as usize]);
            }
            *v = remap[*v as usize];
        }
    }
    mesh.vertices = vertices;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    fn flat_design(z: f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([650_000.0, 7_450_000.0], [1.0, 1.0], 11, 11, move |_, _| z))
    }

    #[test]
    fn region_below_a_flat_surface_is_a_box() {
        let closed = close_surface_to_halfspace(&flat_design(105.0), Side::Below, 99.0).unwrap();
        assert_eq!(closed.cap_method, CapMethod::Triangulated);
        assert!(closed.mesh.validity().is_watertight());
        assert!((closed.mesh.signed_volume() - 600.0).abs() < 1e-6, "10 x 10 x 6 m");
    }

    #[test]
    fn region_above_a_downward_wound_surface() {
        let closed = close_surface_to_halfspace(&flat_design(105.0).flipped(), Side::Above, 120.0).unwrap();
        assert!(closed.mesh.validity().is_watertight());
        assert!((closed.mesh.signed_volume() - 1500.0).abs() < 1e-6, "10 x 10 x 15 m");
        assert_eq!(closed.surface_flips, 200, "the flipped surface is turned back up first");
    }

    #[test]
    fn surface_with_a_hole_closes_around_it() {
        let mut g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 5, 5, |x, y| 10.0 + 0.1 * x + 0.2 * y);
        let centre = g.index(2, 2);
        g.z[centre] = f64::NAN;
        let surface = triangulate_grid(&g);
        let closed = close_surface_to_halfspace(&surface, Side::Below, 0.0).unwrap();
        assert!(closed.mesh.validity().is_watertight());
        // Volume under a plane over a region = area x height at the region's centroid; the
        // hole (diamond of area 2 centred at (2,2)) is removed from the 4 x 4 square, both
        // centred at (2,2): (16 - 2) x (10 + 0.2 + 0.4).
        assert!((closed.mesh.signed_volume() - 14.0 * 10.6).abs() < 1e-9);
    }

    #[test]
    fn cap_must_clear_the_surface() {
        assert!(close_surface_to_halfspace(&flat_design(105.0), Side::Below, 105.0).is_err());
    }
}
