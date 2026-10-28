use parry3d_f64::math::Vector;
use parry3d_f64::query::{Ray, RayCast};
use parry3d_f64::shape::TriMesh as ParryTriMesh;
use zd_core::TriMesh;

use crate::GeomError;

/// BVH ray caster over one surface, for snapping. The BVH is built about a local origin; rays
/// and hits are in world coordinates.
pub struct SurfaceRayCaster {
    origin: [f64; 3],
    bvh: ParryTriMesh,
}

impl SurfaceRayCaster {
    pub fn new(surface: &TriMesh) -> Result<Self, GeomError> {
        let bounds = surface
            .bounds()
            .ok_or_else(|| GeomError::InvalidInput("cannot ray cast an empty surface".into()))?;
        let c = bounds.centre();
        let origin = [c[0].round(), c[1].round(), c[2].round()];
        let vertices = surface
            .vertices
            .iter()
            .map(|p| Vector::new(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]))
            .collect();
        let bvh = ParryTriMesh::new(vertices, surface.triangles.clone())
            .map_err(|e| GeomError::InvalidInput(format!("ray caster: {e:?}")))?;
        Ok(SurfaceRayCaster { origin, bvh })
    }

    /// Nearest hit along the ray (either side of a triangle), in world coordinates.
    pub fn cast(&self, from: [f64; 3], direction: [f64; 3]) -> Option<[f64; 3]> {
        let o = self.origin;
        let ray = Ray::new(
            Vector::new(from[0] - o[0], from[1] - o[1], from[2] - o[2]),
            Vector::new(direction[0], direction[1], direction[2]),
        );
        let toi = self.bvh.cast_local_ray(&ray, f64::MAX, false)?;
        let hit = ray.point_at(toi);
        Some([hit.x + o[0], hit.y + o[1], hit.z + o[2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    #[test]
    fn vertical_rays_hit_a_dipping_plane_from_both_sides() {
        let plane = |x: f64, y: f64| 100.0 + 0.05 * (x - 650_000.0) + 0.02 * (y - 7_450_000.0);
        let g = Grid::from_fn([650_000.0, 7_450_000.0], [10.0, 10.0], 21, 21, plane);
        let caster = SurfaceRayCaster::new(&triangulate_grid(&g)).unwrap();
        let (x, y) = (650_123.4, 7_450_077.7);
        let down = caster.cast([x, y, 500.0], [0.0, 0.0, -1.0]).unwrap();
        let up = caster.cast([x, y, -500.0], [0.0, 0.0, 1.0]).unwrap();
        for hit in [down, up] {
            assert!((hit[0] - x).abs() < 1e-9 && (hit[1] - y).abs() < 1e-9);
            assert!((hit[2] - plane(x, y)).abs() < 1e-9);
        }
        assert!(caster.cast([640_000.0, y, 500.0], [0.0, 0.0, -1.0]).is_none());
    }

    #[test]
    fn oblique_ray_hits_where_the_line_meets_the_surface() {
        let g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 11, 11, |_, _| 5.0);
        let caster = SurfaceRayCaster::new(&triangulate_grid(&g)).unwrap();
        let hit = caster.cast([0.0, 0.0, 15.0], [1.0, 1.0, -2.0]).unwrap();
        assert!((hit[0] - 5.0).abs() < 1e-9 && (hit[1] - 5.0).abs() < 1e-9 && (hit[2] - 5.0).abs() < 1e-9);
    }
}
