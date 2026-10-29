//! Closing a nearly closed triangulation into a solid (decision 0013): fins taken off, cracks
//! joined, holes filled. It runs only when asked for, and every change is counted; how much filling
//! is acceptable is the caller's call (`solid.import` refuses fills over a share of the surface).
//!
//! - A **fin** is a triangle with an open edge that also sits on an edge of three or more faces: a
//!   flap with no volume, left hanging off the solid. Fins come off until none is left.
//! - A **crack** is a vertex lying on an open edge, within the tolerance, without being joined to it
//!   (a T-junction). The triangle on that edge is split at the vertex, so both sides share the
//!   edges; no coordinate moves.
//! - A **hole** is a loop of open edges that is left. It is filled with triangles across the loop's
//!   own vertices (no new vertices), by Manifold's constrained Delaunay triangulation of the loop
//!   projected onto its best-fit plane. A loop that folds over itself in that plane is left open.
//!
//! Last, each patch is oriented again: one that was open (and so turned to face up by the repair
//! step) and is now closed faces out.

use std::collections::HashMap;

use manifold_csg::triangulate_polygons;
use zd_core::TriMesh;

use crate::orient_patches;

/// What [`close_solid`] changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CloseLog {
    /// Triangles taken off as fins.
    pub fin_triangles: usize,
    /// Vertices joined into open edges they lay on, each splitting one triangle in two.
    pub cracks_joined: usize,
    /// Each hole filled: a point in its middle and the area added (m²).
    pub filled: Vec<([f64; 3], f64)>,
    /// A point on each hole that could not be filled (its loop folds over itself).
    pub unfillable: Vec<[f64; 3]>,
    /// Triangles turned when the closed patches were oriented to face out.
    pub triangles_turned: usize,
    /// The surface area after closing (m²).
    pub surface_area: f64,
}

impl CloseLog {
    pub fn filled_area(&self) -> f64 {
        self.filled.iter().map(|f| f.1).sum()
    }
}

/// The most cracks joined in one mesh, a guard against a mesh that keeps producing them.
const MAX_CRACKS: usize = 100_000;

/// Fins off, cracks joined, holes filled, in that order (see the module notes).
pub fn close_solid(mesh: &TriMesh, tolerance: f64) -> (TriMesh, CloseLog) {
    let mut log = CloseLog::default();
    let mut triangles = mesh.triangles.clone();
    let mut vertices = mesh.vertices.clone();

    // Fins.
    loop {
        let counts = edge_counts(&triangles);
        let before = triangles.len();
        triangles.retain(|t| {
            let c = edges(t).map(|e| counts[&e]);
            !(c.contains(&1) && c.iter().any(|&n| n > 2))
        });
        log.fin_triangles += before - triangles.len();
        if triangles.len() == before {
            break;
        }
    }

    // Cracks: split the triangle on an open edge at a vertex lying on it.
    while log.cracks_joined < MAX_CRACKS {
        let counts = edge_counts(&triangles);
        let open: Vec<(usize, usize)> = (0..triangles.len())
            .flat_map(|k| (0..3).map(move |slot| (k, slot)))
            .filter(|&(k, slot)| counts[&undirected(triangles[k][slot], triangles[k][(slot + 1) % 3])] == 1)
            .collect();
        let mut on_open: Vec<u32> = open.iter().flat_map(|&(k, slot)| [triangles[k][slot], triangles[k][(slot + 1) % 3]]).collect();
        on_open.sort_unstable();
        on_open.dedup();
        let found = open.iter().find_map(|&(k, slot)| {
            let (a, b) = (triangles[k][slot], triangles[k][(slot + 1) % 3]);
            on_open.iter().find(|&&v| v != a && v != b && on_segment(vertices[v as usize], vertices[a as usize], vertices[b as usize], tolerance)).map(|&v| (k, slot, v))
        });
        let Some((k, slot, v)) = found else { break };
        let [a, b, c] = [triangles[k][slot], triangles[k][(slot + 1) % 3], triangles[k][(slot + 2) % 3]];
        triangles[k] = [a, v, c];
        triangles.push([v, b, c]);
        log.cracks_joined += 1;
    }

    // Holes: each loop of open edges, walked the way its triangles run, filled the other way round.
    for ring in open_loops(&triangles) {
        let fill: Vec<u32> = ring.iter().rev().copied().collect();
        let points: Vec<[f64; 3]> = fill.iter().map(|&v| vertices[v as usize]).collect();
        let centre = points.iter().fold([0.0; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]).map(|c| c / points.len() as f64);
        match fill_ring(&points, centre) {
            Some(faces) => {
                let mut area = 0.0;
                for f in faces {
                    let t = f.map(|i| fill[i as usize]);
                    area += triangle_area(t.map(|i| vertices[i as usize]));
                    triangles.push(t);
                }
                log.filled.push((centre, area));
            }
            None => log.unfillable.push(centre),
        }
    }

    // Vertices no triangle uses any more (a fin's tip) go, in order.
    let mut used = vec![false; vertices.len()];
    triangles.iter().flatten().for_each(|&i| used[i as usize] = true);
    let mut renumber = vec![u32::MAX; vertices.len()];
    let mut kept = Vec::with_capacity(vertices.len());
    for (i, p) in vertices.drain(..).enumerate() {
        if used[i] {
            renumber[i] = kept.len() as u32;
            kept.push(p);
        }
    }
    triangles.iter_mut().flatten().for_each(|i| *i = renumber[*i as usize]);
    let (closed, orient) = orient_patches(&TriMesh::new(kept, triangles));
    log.triangles_turned = orient.triangles_flipped;
    log.surface_area = closed.area();
    (closed, log)
}

fn undirected(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

fn edges(t: &[u32; 3]) -> [(u32, u32); 3] {
    [undirected(t[0], t[1]), undirected(t[1], t[2]), undirected(t[2], t[0])]
}

fn edge_counts(triangles: &[[u32; 3]]) -> HashMap<(u32, u32), usize> {
    let mut counts = HashMap::with_capacity(triangles.len() * 3 / 2);
    for t in triangles {
        for e in edges(t) {
            *counts.entry(e).or_default() += 1;
        }
    }
    counts
}

/// Within `tolerance` of segment `a`-`b` and strictly between its ends (more than the tolerance from
/// each), so a vertex at an end is never taken for a crack.
fn on_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3], tolerance: f64) -> bool {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let length2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if length2 == 0.0 {
        return false;
    }
    let t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / length2;
    let length = length2.sqrt();
    if t * length <= tolerance || (1.0 - t) * length <= tolerance {
        return false;
    }
    let off = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
    (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt() <= tolerance
}

/// Loops of open edges, each walked the way the triangles run its edges, with any loop that passes
/// through a vertex twice cut there into simple loops.
fn open_loops(triangles: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let counts = edge_counts(triangles);
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut order = Vec::new();
    for t in triangles {
        for slot in 0..3 {
            let (a, b) = (t[slot], t[(slot + 1) % 3]);
            if counts[&undirected(a, b)] == 1 {
                next.entry(a).or_default().push(b);
                order.push(a);
            }
        }
    }
    let mut loops = Vec::new();
    for start in order {
        while next.get(&start).is_some_and(|n| !n.is_empty()) {
            // Walk until back at the start or stuck, cutting off a simple loop at each repeat.
            let mut path = vec![start];
            let mut at = start;
            while let Some(step) = next.get_mut(&at).and_then(|n| n.pop()) {
                if let Some(j) = path.iter().position(|&v| v == step) {
                    let ring: Vec<u32> = path.drain(j..).collect();
                    if ring.len() >= 3 {
                        loops.push(ring);
                    }
                    if path.is_empty() {
                        break;
                    }
                }
                path.push(step);
                at = step;
            }
        }
    }
    loops
}

/// Triangles across a ring of points (counter-clockwise about the ring's own normal), as indices into
/// it, or None if the ring folds over itself in its best-fit plane.
fn fill_ring(points: &[[f64; 3]], centre: [f64; 3]) -> Option<Vec<[u32; 3]>> {
    let local: Vec<[f64; 3]> = points.iter().map(|p| [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]]).collect();
    // Newell's normal: the ring's area vector, pointing so the ring runs counter-clockwise about it.
    let mut n = [0.0; 3];
    for i in 0..local.len() {
        let (p, q) = (local[i], local[(i + 1) % local.len()]);
        n[0] += (p[1] - q[1]) * (p[2] + q[2]);
        n[1] += (p[2] - q[2]) * (p[0] + q[0]);
        n[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length == 0.0 {
        return None;
    }
    let n = n.map(|c| c / length);
    let axis = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = unit(cross(axis, n))?;
    let w = cross(n, u);
    let ring: Vec<[f64; 2]> = local.iter().map(|p| [dot(*p, u), dot(*p, w)]).collect();
    let signed = |t: &[u32; 3]| {
        let [a, b, c] = t.map(|i| ring[i as usize]);
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
    };
    let ring_area: f64 = (0..ring.len()).map(|i| {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        0.5 * (p[0] * q[1] - q[0] * p[1])
    })
    .sum();
    let mut faces = triangulate_polygons(std::slice::from_ref(&ring), 1e-12)?;
    if faces.len() != ring.len() - 2 {
        return None;
    }
    for f in &mut faces {
        if signed(f) < 0.0 {
            f.swap(1, 2);
        }
    }
    // A simple ring's triangles cover exactly its area; a folded one's do not.
    let covered: f64 = faces.iter().map(signed).sum();
    ((covered - ring_area).abs() <= 1e-9 * ring_area.abs().max(1.0)).then_some(faces)
}

fn triangle_area([a, b, c]: [[f64; 3]; 3]) -> f64 {
    let n = cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
    0.5 * dot(n, n).sqrt()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = dot(v, v).sqrt();
    (length > 0.0).then(|| v.map(|c| c / length))
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A 10 m cube at mine grid coordinates, wound outward: vertex i has x, y, z bits 0, 1, 2.
    fn cube() -> TriMesh {
        let v = (0..8).map(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect();
        let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh::new(v, t)
    }

    #[test]
    fn a_fin_comes_off_and_the_cube_is_whole() {
        let mut m = cube();
        // A flap on the top front edge (4-5), reaching out to a point of its own.
        m.vertices.push([E + 5.0, N - 8.0, 115.0]);
        m.triangles.push([4, 5, 8]);
        assert!(!m.validity().is_watertight());
        let (closed, log) = close_solid(&m, 0.001);
        assert_eq!((log.fin_triangles, log.cracks_joined, log.filled.len()), (1, 0, 0));
        assert!(closed.validity().is_watertight());
        assert_eq!(closed.vertices.len(), 8, "the fin's tip goes with it");
        assert!((closed.signed_volume() - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn a_crack_is_joined_without_moving_anything() {
        let mut m = cube();
        // The front face meets the top edge 4-5 at its midpoint m; the top face does not.
        m.vertices.push([E + 5.0, N, 110.0]);
        let mid = 8;
        let front = m.triangles.iter().position(|t| *t == [0, 5, 4]).unwrap();
        m.triangles[front] = [0, 5, mid];
        m.triangles.push([0, mid, 4]);
        assert_eq!(m.validity().boundary_edges, 3);
        let (closed, log) = close_solid(&m, 0.001);
        assert_eq!((log.fin_triangles, log.cracks_joined, log.filled.len()), (0, 1, 0));
        assert!(closed.validity().is_watertight());
        assert!(closed.vertices[mid as usize] == [E + 5.0, N, 110.0]);
        assert!((closed.signed_volume() - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn holes_are_filled_across_their_own_vertices() {
        // One triangle of the top missing: filled with one triangle of 50 m².
        let mut m = cube();
        m.triangles.remove(3);
        let (closed, log) = close_solid(&m, 0.001);
        assert_eq!(log.filled.len(), 1);
        assert!((log.filled_area() - 50.0).abs() < 1e-9 && (log.surface_area - 600.0).abs() < 1e-9);
        assert!(closed.validity().is_watertight());
        assert!((closed.signed_volume() - 1000.0).abs() < 1e-9);

        // The whole top missing: a square hole of 100 m², filled with two triangles, facing up. Turned
        // inward first (as the repair step leaves an open box, facing up), it comes out facing out.
        let mut m = cube();
        m.triangles.drain(2..4);
        let m = m.flipped();
        let (closed, log) = close_solid(&m, 0.001);
        assert_eq!(log.triangles_turned, 12);
        assert_eq!(log.filled.len(), 1);
        assert!((log.filled_area() - 100.0).abs() < 1e-9);
        assert!(closed.validity().is_watertight());
        assert!((closed.signed_volume() - 1000.0).abs() < 1e-9);
        assert!((log.filled[0].0[2] - 110.0).abs() < 1e-9, "the hole's middle is on the top");
    }

    #[test]
    fn a_hole_whose_edge_is_not_flat_is_filled_and_its_area_counted() {
        // The top missing and one top corner raised 2 m: the four sides still meet it; the fill is a
        // fold across the square, more than 100 m², and the volume is that of the solid it closes.
        let mut m = cube();
        m.triangles.drain(2..4);
        m.vertices[7][2] += 2.0;
        let (closed, log) = close_solid(&m, 0.001);
        assert!(closed.validity().is_watertight());
        assert!(log.filled_area() > 100.0 && log.filled_area() < 103.0, "{}", log.filled_area());
        // The two fill triangles meet along a diagonal; either way the volume lies between the cube's and
        // the cube's plus the prism under the raised corner.
        assert!(closed.signed_volume() > 1000.0 && closed.signed_volume() < 1000.0 + 100.0 * 2.0 / 2.0 + 1e-9, "{}", closed.signed_volume());
    }
}
