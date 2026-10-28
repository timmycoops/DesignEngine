use crate::TriMesh;
use crate::mesh::{cross, norm, sub};

/// Computed validity flags (Seequent Evo vocabulary). Nothing reaches Manifold or a volume
/// report without them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshValidity {
    /// No boundary: every edge has at least two faces.
    pub closed: bool,
    /// Every edge has exactly two faces.
    pub manifold: bool,
    /// The two faces on every two-face edge traverse it in opposite directions.
    pub consistent_winding: bool,
    /// No triangle repeats a vertex or has zero area.
    pub non_degenerate: bool,
    /// Not computed in Phase 0: it needs a triangle-triangle intersection pass.
    pub non_self_intersecting: Option<bool>,
    pub boundary_edges: usize,
    pub non_manifold_edges: usize,
    pub inconsistent_edges: usize,
    pub degenerate_triangles: usize,
}

impl MeshValidity {
    pub fn of(mesh: &TriMesh) -> Self {
        // One entry per half-edge: undirected key plus whether it runs low -> high index.
        let mut half_edges: Vec<(u64, bool)> = Vec::with_capacity(mesh.triangles.len() * 3);
        let mut degenerate_triangles = 0;
        for &tri in &mesh.triangles {
            let [a, b, c] = tri;
            if a == b || b == c || a == c || is_zero_area(tri.map(|i| mesh.vertices[i as usize])) {
                degenerate_triangles += 1;
            }
            for (u, v) in [(a, b), (b, c), (c, a)] {
                half_edges.push((edge_key(u, v), u < v));
            }
        }
        half_edges.sort_unstable_by_key(|&(key, _)| key);

        let mut boundary_edges = 0;
        let mut non_manifold_edges = 0;
        let mut inconsistent_edges = 0;
        for run in half_edges.chunk_by(|x, y| x.0 == y.0) {
            match run.len() {
                1 => boundary_edges += 1,
                2 if run[0].1 == run[1].1 => inconsistent_edges += 1,
                2 => {}
                _ => non_manifold_edges += 1,
            }
        }

        MeshValidity {
            closed: boundary_edges == 0,
            manifold: boundary_edges == 0 && non_manifold_edges == 0,
            consistent_winding: inconsistent_edges == 0,
            non_degenerate: degenerate_triangles == 0,
            non_self_intersecting: None,
            boundary_edges,
            non_manifold_edges,
            inconsistent_edges,
            degenerate_triangles,
        }
    }

    /// Closed, manifold and consistently wound: safe to hand to Manifold and to report a volume.
    pub fn is_watertight(&self) -> bool {
        self.closed && self.manifold && self.consistent_winding
    }
}

pub(crate) fn edge_key(u: u32, v: u32) -> u64 {
    let (lo, hi) = if u < v { (u, v) } else { (v, u) };
    (u64::from(lo) << 32) | u64::from(hi)
}

/// Zero area relative to the triangle's own size, so the test is scale-free.
pub fn is_zero_area([a, b, c]: [[f64; 3]; 3]) -> bool {
    let twice_area = norm(cross(sub(b, a), sub(c, a)));
    let longest_sq = [(a, b), (b, c), (c, a)]
        .iter()
        .map(|(p, q)| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2))
        .fold(0.0, f64::max);
    twice_area <= 1e-12 * longest_sq
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::box_mesh;

    #[test]
    fn closed_box_is_watertight() {
        let v = box_mesh([0.0; 3], [1.0; 3]).validity();
        assert!(v.is_watertight());
        assert!(v.non_degenerate);
        assert_eq!(v.non_self_intersecting, None);
    }

    #[test]
    fn open_box_reports_boundary() {
        let mut m = box_mesh([0.0; 3], [1.0; 3]);
        m.triangles.truncate(10); // drop one side: 4 boundary edges
        let v = m.validity();
        assert!(!v.closed && !v.manifold);
        assert_eq!(v.boundary_edges, 4);
        assert!(v.consistent_winding);
    }

    #[test]
    fn flipped_face_reports_inconsistent_winding() {
        let mut m = box_mesh([0.0; 3], [1.0; 3]);
        let [a, b, c] = m.triangles[0];
        m.triangles[0] = [a, c, b];
        let v = m.validity();
        assert!(v.closed);
        assert!(!v.consistent_winding);
        assert_eq!(v.inconsistent_edges, 3);
    }

    #[test]
    fn degenerate_triangle_is_flagged() {
        let m = TriMesh::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], vec![[0, 1, 2]]);
        assert_eq!(m.validity().degenerate_triangles, 1);
    }
}
