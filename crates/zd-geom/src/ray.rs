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
        let ray = self.local_ray(from, direction);
        let toi = self.bvh.cast_local_ray(&ray, f64::MAX, false)?;
        Some(self.world_point(&ray, toi))
    }

    /// Nearest hit on a triangle that `keep` takes, with that triangle's number in the surface. A
    /// surface joined from many pieces can then pass over some of them without a new BVH.
    pub fn cast_where(&self, from: [f64; 3], direction: [f64; 3], keep: impl Fn(u32) -> bool) -> Option<(u32, [f64; 3])> {
        let ray = self.local_ray(from, direction);
        let (triangle, toi) = self.bvh.bvh().cast_ray(&ray, f64::MAX, |t, best| if keep(t) { self.bvh.triangle(t).cast_local_ray(&ray, best, false) } else { None })?;
        Some((triangle, self.world_point(&ray, toi)))
    }

    fn local_ray(&self, from: [f64; 3], direction: [f64; 3]) -> Ray {
        let o = self.origin;
        Ray::new(Vector::new(from[0] - o[0], from[1] - o[1], from[2] - o[2]), Vector::new(direction[0], direction[1], direction[2]))
    }

    fn world_point(&self, ray: &Ray, toi: f64) -> [f64; 3] {
        let (hit, o) = (ray.point_at(toi), self.origin);
        [hit.x + o[0], hit.y + o[1], hit.z + o[2]]
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

    /// Two flat sheets, one 10 m over the other, joined as one surface: the upper is triangles 0 and 1.
    #[test]
    fn a_cast_can_pass_over_some_triangles_and_names_what_it_hits() {
        let sheet = |z: f64, at: u32| {
            let vertices = vec![[0.0, 0.0, z], [10.0, 0.0, z], [10.0, 10.0, z], [0.0, 10.0, z]];
            (vertices, vec![[at, at + 1, at + 2], [at, at + 2, at + 3]])
        };
        let (mut vertices, mut triangles) = sheet(20.0, 0);
        let lower = sheet(10.0, 4);
        vertices.extend(lower.0);
        triangles.extend(lower.1);
        let caster = SurfaceRayCaster::new(&TriMesh::new(vertices, triangles)).unwrap();
        let down = ([3.0, 1.0, 50.0], [0.0, 0.0, -1.0]);

        let (triangle, hit) = caster.cast_where(down.0, down.1, |_| true).unwrap();
        assert_eq!((triangle, hit), (0, [3.0, 1.0, 20.0]));
        assert_eq!(caster.cast(down.0, down.1), Some([3.0, 1.0, 20.0]), "the same as a plain cast");

        let (triangle, hit) = caster.cast_where(down.0, down.1, |t| t >= 2).unwrap();
        assert_eq!((triangle, hit), (2, [3.0, 1.0, 10.0]));
        assert!(caster.cast_where(down.0, down.1, |_| false).is_none());
    }

    #[test]
    fn oblique_ray_hits_where_the_line_meets_the_surface() {
        let g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 11, 11, |_, _| 5.0);
        let caster = SurfaceRayCaster::new(&triangulate_grid(&g)).unwrap();
        let hit = caster.cast([0.0, 0.0, 15.0], [1.0, 1.0, -2.0]).unwrap();
        assert!((hit[0] - 5.0).abs() < 1e-9 && (hit[1] - 5.0).abs() < 1e-9 && (hit[2] - 5.0).abs() < 1e-9);
    }
}
