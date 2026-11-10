use crate::{Aabb, MeshValidity};

/// Triangulated mesh in the OMF2 layout: f64 vertices, u32 triangles, wound counter-clockwise
/// about the outward normal (upward for an open surface).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriMesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl TriMesh {
    pub fn new(vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> Self {
        TriMesh { vertices, triangles }
    }

    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub fn bounds(&self) -> Option<Aabb> {
        Aabb::from_points(&self.vertices)
    }

    pub fn validity(&self) -> MeshValidity {
        MeshValidity::of(self)
    }

    /// Enclosed volume by the divergence theorem; positive for an outward-wound closed mesh.
    /// Summed about the bounding-box centre so mine-grid coordinates do not cancel away precision.
    pub fn signed_volume(&self) -> f64 {
        let Some(bounds) = self.bounds() else { return 0.0 };
        let c = bounds.centre();
        let local = |i: u32| {
            let p = self.vertices[i as usize];
            [p[0] - c[0], p[1] - c[1], p[2] - c[2]]
        };
        let mut six_v = 0.0;
        for &[a, b, d] in &self.triangles {
            six_v += dot(local(a), cross(local(b), local(d)));
        }
        six_v / 6.0
    }

    /// The signed volume of each shell: the triangles joined through shared corners, in the order of their first
    /// triangles. Positive for a shell wound outward, negative for one wound inward (a cavity). Summed about the
    /// bounding-box centre, as [`signed_volume`](Self::signed_volume) is, so the shells add up to it.
    pub fn shell_volumes(&self) -> Vec<f64> {
        let Some(bounds) = self.bounds() else { return Vec::new() };
        let c = bounds.centre();
        let mut to: Vec<u32> = (0..self.vertices.len() as u32).collect();
        fn root(to: &mut [u32], mut v: u32) -> u32 {
            while to[v as usize] != v {
                to[v as usize] = to[to[v as usize] as usize];
                v = to[v as usize];
            }
            v
        }
        for &[a, b, d] in &self.triangles {
            let ra = root(&mut to, a);
            for other in [b, d] {
                let ro = root(&mut to, other);
                to[ro as usize] = ra;
            }
        }
        let mut shell_of = vec![usize::MAX; self.vertices.len()];
        let mut six_v: Vec<f64> = Vec::new();
        let local = |i: u32| {
            let p = self.vertices[i as usize];
            [p[0] - c[0], p[1] - c[1], p[2] - c[2]]
        };
        for &[a, b, d] in &self.triangles {
            let r = root(&mut to, a) as usize;
            if shell_of[r] == usize::MAX {
                shell_of[r] = six_v.len();
                six_v.push(0.0);
            }
            six_v[shell_of[r]] += dot(local(a), cross(local(b), local(d)));
        }
        six_v.into_iter().map(|v| v / 6.0).collect()
    }

    /// Total triangle area.
    pub fn area(&self) -> f64 {
        self.triangles
            .iter()
            .map(|&t| 0.5 * norm(self.triangle_cross(t)))
            .sum()
    }

    /// `(b - a) x (c - a)` for one triangle: twice its area along its normal.
    pub fn triangle_cross(&self, [a, b, c]: [u32; 3]) -> [f64; 3] {
        let pa = self.vertices[a as usize];
        let pb = self.vertices[b as usize];
        let pc = self.vertices[c as usize];
        cross(sub(pb, pa), sub(pc, pa))
    }

    /// The same mesh with every triangle wound the other way.
    pub fn flipped(&self) -> TriMesh {
        TriMesh {
            vertices: self.vertices.clone(),
            triangles: self.triangles.iter().map(|&[a, b, c]| [a, c, b]).collect(),
        }
    }

    pub fn translated(&self, offset: [f64; 3]) -> TriMesh {
        TriMesh {
            vertices: self
                .vertices
                .iter()
                .map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
                .collect(),
            triangles: self.triangles.clone(),
        }
    }
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Axis-aligned box with outward winding.
    pub fn box_mesh(min: [f64; 3], max: [f64; 3]) -> TriMesh {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let vertices = vec![
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        TriMesh::new(vertices, triangles)
    }

    #[test]
    fn shells_are_told_apart_with_their_volumes() {
        // A 10 x 20 x 5 m box (1,000 m3), a 2 m cube beside it (8 m3) and a 1 m cube inside the box wound inward (a
        // cavity, -1 m3), at mine grid coordinates: three shells, in order, adding up to 1,007 m3. Tolerance 1e-9 m3.
        let big = box_mesh([650_000.0, 7_450_000.0, 100.0], [650_010.0, 7_450_020.0, 105.0]);
        let small = box_mesh([650_030.0, 7_450_000.0, 100.0], [650_032.0, 7_450_002.0, 102.0]);
        let cavity = box_mesh([650_004.0, 7_450_004.0, 102.0], [650_005.0, 7_450_005.0, 103.0]).flipped();
        let mut all = TriMesh::default();
        for part in [&big, &small, &cavity] {
            let base = all.vertices.len() as u32;
            all.vertices.extend_from_slice(&part.vertices);
            all.triangles.extend(part.triangles.iter().map(|t| t.map(|v| v + base)));
        }
        let shells = all.shell_volumes();
        assert_eq!(shells.len(), 3);
        for (found, expected) in shells.iter().zip([1000.0, 8.0, -1.0]) {
            assert!((found - expected).abs() < 1e-9, "{found} vs {expected}");
        }
        assert!((shells.iter().sum::<f64>() - all.signed_volume()).abs() < 1e-9);
        assert!(TriMesh::default().shell_volumes().is_empty());
    }

    #[test]
    fn box_volume_and_area_at_mine_grid_coordinates() {
        // 10 x 20 x 5 m box at MGA-scale eastings/northings: volume 1000 m3, area 700 m2.
        let b = box_mesh([650_000.0, 7_450_000.0, 100.0], [650_010.0, 7_450_020.0, 105.0]);
        assert!((b.signed_volume() - 1000.0).abs() < 1e-9);
        assert!((b.area() - 700.0).abs() < 1e-9);
        assert!((b.flipped().signed_volume() + 1000.0).abs() < 1e-9);
    }
}
