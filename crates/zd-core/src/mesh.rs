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
    fn box_volume_and_area_at_mine_grid_coordinates() {
        // 10 x 20 x 5 m box at MGA-scale eastings/northings: volume 1000 m3, area 700 m2.
        let b = box_mesh([650_000.0, 7_450_000.0, 100.0], [650_010.0, 7_450_020.0, 105.0]);
        assert!((b.signed_volume() - 1000.0).abs() < 1e-9);
        assert!((b.area() - 700.0).abs() < 1e-9);
        assert!((b.flipped().signed_volume() + 1000.0).abs() < 1e-9);
    }
}
