use parry3d_f64::math::Vector;
use parry3d_f64::query::{Ray, RayCast};
use parry3d_f64::shape::TriMesh as ParryTriMesh;
use zd_core::TriMesh;

use crate::GeomError;

/// Triangles in one piece of a snap index at most. Parry builds a BVH on one thread, and a 1 m lidar's 85 M triangles
/// take 27 s that way; split by plan position into pieces of this many, they're indexed side by side on every core.
pub const PIECE_TRIANGLES: usize = 4_000_000;

/// BVH ray caster over one surface, for snapping. The BVH is built about a local origin; rays
/// and hits are in world coordinates. A big surface is indexed in pieces by plan position (`PIECE_TRIANGLES`), each
/// knowing the surface's own number for its triangles, so a cast names the same triangle either way.
pub struct SurfaceRayCaster {
    origin: [f64; 3],
    pieces: Vec<Piece>,
}

struct Piece {
    bvh: ParryTriMesh,
    /// The surface's number for each of the piece's triangles; none when the piece is the whole surface.
    triangles: Option<Vec<u32>>,
}

impl SurfaceRayCaster {
    pub fn new(surface: &TriMesh) -> Result<Self, GeomError> {
        Self::in_pieces(surface, PIECE_TRIANGLES)
    }

    /// As `new`, with pieces of at most about `most` triangles (a piece can hold more where the triangles crowd).
    pub fn in_pieces(surface: &TriMesh, most: usize) -> Result<Self, GeomError> {
        let bounds = surface
            .bounds()
            .ok_or_else(|| GeomError::InvalidInput("cannot ray cast an empty surface".into()))?;
        let c = bounds.centre();
        let origin = [c[0].round(), c[1].round(), c[2].round()];
        if surface.triangles.len() <= most.max(1) {
            let vertices = surface.vertices.iter().map(|p| local(*p, origin)).collect();
            let bvh = ParryTriMesh::new(vertices, surface.triangles.clone()).map_err(|e| GeomError::InvalidInput(format!("ray caster: {e:?}")))?;
            return Ok(SurfaceRayCaster { origin, pieces: vec![Piece { bvh, triangles: None }] });
        }
        let groups = split_in_plan(surface, most.max(1));
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        let pieces = crate::at_most(groups.len(), cores, |i| piece_of(surface, origin, &groups[i])).into_iter().collect::<Result<_, _>>()?;
        Ok(SurfaceRayCaster { origin, pieces })
    }

    /// Nearest hit along the ray (either side of a triangle), in world coordinates.
    pub fn cast(&self, from: [f64; 3], direction: [f64; 3]) -> Option<[f64; 3]> {
        self.cast_where(from, direction, |_| true).map(|(_, hit)| hit)
    }

    /// Nearest hit on a triangle that `keep` takes, with that triangle's number in the surface. A
    /// surface joined from many pieces can then pass over some of them without a new BVH.
    pub fn cast_where(&self, from: [f64; 3], direction: [f64; 3], keep: impl Fn(u32) -> bool) -> Option<(u32, [f64; 3])> {
        let ray = self.local_ray(from, direction);
        // Pieces in the order the ray reaches their boxes; once the next box is beyond the best hit, none further is nearer.
        let mut reached: Vec<(f64, &Piece)> = self.pieces.iter().filter_map(|p| p.bvh.local_aabb().cast_local_ray(&ray, f64::MAX, true).map(|toi| (toi, p))).collect();
        reached.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut best: Option<(u32, f64)> = None;
        for (entry, piece) in reached {
            let limit = best.map_or(f64::MAX, |(_, toi)| toi);
            if entry > limit {
                break;
            }
            let number = |t: u32| piece.triangles.as_ref().map_or(t, |all| all[t as usize]);
            let hit = piece.bvh.bvh().cast_ray(&ray, limit, |t, best| if keep(number(t)) { piece.bvh.triangle(t).cast_local_ray(&ray, best, false) } else { None });
            if let Some((t, toi)) = hit.filter(|(_, toi)| *toi < limit) {
                best = Some((number(t), toi));
            }
        }
        best.map(|(triangle, toi)| (triangle, self.world_point(&ray, toi)))
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

fn local(p: [f64; 3], origin: [f64; 3]) -> Vector {
    Vector::new(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2])
}

/// The surface's triangles in squares of plan by their centres, about `most` a square where they're spread evenly.
/// Empty squares are left out.
fn split_in_plan(surface: &TriMesh, most: usize) -> Vec<Vec<u32>> {
    let across = (surface.triangles.len().div_ceil(most) as f64).sqrt().ceil() as usize;
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &surface.vertices {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let cell = |v: f64, k: usize| (((v - lo[k]) / (hi[k] - lo[k]).max(f64::MIN_POSITIVE)) * across as f64).floor().clamp(0.0, (across - 1) as f64) as usize;
    let mut groups = vec![Vec::new(); across * across];
    for (i, t) in surface.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| surface.vertices[v as usize]);
        let (x, y) = ((a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0);
        groups[cell(y, 1) * across + cell(x, 0)].push(i as u32);
    }
    groups.retain(|g| !g.is_empty());
    groups
}

/// One piece's index: its triangles with only the vertices they use.
fn piece_of(surface: &TriMesh, origin: [f64; 3], triangles: &[u32]) -> Result<Piece, GeomError> {
    let mut renumbered = vec![u32::MAX; surface.vertices.len()];
    let mut vertices = Vec::new();
    let indices = triangles
        .iter()
        .map(|&t| {
            surface.triangles[t as usize].map(|v| {
                let slot = &mut renumbered[v as usize];
                if *slot == u32::MAX {
                    *slot = vertices.len() as u32;
                    vertices.push(local(surface.vertices[v as usize], origin));
                }
                *slot
            })
        })
        .collect();
    let bvh = ParryTriMesh::new(vertices, indices).map_err(|e| GeomError::InvalidInput(format!("ray caster: {e:?}")))?;
    Ok(Piece { bvh, triangles: Some(triangles.to_vec()) })
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
