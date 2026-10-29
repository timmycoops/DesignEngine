//! The import repair step (engine rule 8, decision 0010): weld by tolerance, drop the triangles
//! the weld collapses and any that are degenerate or duplicated, then orient by flood fill. Every
//! change is counted; crossings are found separately ([`find_crossings`](crate::find_crossings))
//! and never fixed.

use std::collections::HashMap;
use std::time::Instant;

use zd_core::{TriMesh, is_zero_area};

use crate::{OrientLog, orient_patches};

/// What [`repair_mesh`] changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepairLog {
    /// The weld tolerance, m.
    pub tolerance: f64,
    /// Vertices merged into an earlier vertex within the tolerance.
    pub vertices_welded: usize,
    /// The farthest a welded vertex moved, m.
    pub largest_move: f64,
    /// Triangles the weld left with a repeated corner.
    pub collapsed_dropped: usize,
    /// Triangles of zero area: all three corners in a line.
    pub degenerate_dropped: usize,
    /// Triangles on the same three vertices as an earlier one, either way round.
    pub duplicates_dropped: usize,
    /// Vertices no remaining triangle uses.
    pub unused_dropped: usize,
    pub orient: OrientLog,
    pub ms: f64,
}

/// Welds, drops and orients, in that order. Each vertex merges into the first vertex already kept
/// within `tolerance` (3D distance), in the mesh's own order, so every kept coordinate is exactly
/// as read and merges never chain; a tolerance of 0 merges only identical coordinates.
pub fn repair_mesh(mesh: &TriMesh, tolerance: f64) -> (TriMesh, RepairLog) {
    let start = Instant::now();
    let mut log = RepairLog { tolerance, ..RepairLog::default() };
    let (kept, map) = weld(&mesh.vertices, tolerance, &mut log);

    let mut triangles = Vec::with_capacity(mesh.triangles.len());
    for tri in &mesh.triangles {
        let t = tri.map(|i| map[i as usize]);
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            log.collapsed_dropped += 1;
        } else if is_zero_area(t.map(|i| kept[i as usize])) {
            log.degenerate_dropped += 1;
        } else {
            triangles.push(t);
        }
    }
    // Duplicates by sorting on each triangle's vertex set; the first in the mesh's order stays.
    let mut keys: Vec<([u32; 3], u32)> = triangles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut key = *t;
            key.sort_unstable();
            (key, i as u32)
        })
        .collect();
    keys.sort_unstable();
    let mut duplicate = vec![false; triangles.len()];
    for pair in keys.windows(2) {
        if pair[0].0 == pair[1].0 {
            duplicate[pair[1].1 as usize] = true;
        }
    }
    log.duplicates_dropped = duplicate.iter().filter(|d| **d).count();
    let mut index = 0;
    triangles.retain(|_| {
        index += 1;
        !duplicate[index - 1]
    });

    // Keep the vertices the triangles still use, in their own order.
    let mut used = vec![false; kept.len()];
    triangles.iter().flatten().for_each(|&i| used[i as usize] = true);
    let mut renumber = vec![u32::MAX; kept.len()];
    let mut vertices = Vec::with_capacity(kept.len());
    for (i, p) in kept.iter().enumerate().filter(|(i, _)| used[*i]) {
        renumber[i] = vertices.len() as u32;
        vertices.push(*p);
    }
    triangles.iter_mut().flatten().for_each(|i| *i = renumber[*i as usize]);
    log.unused_dropped = kept.len() - vertices.len();

    let (oriented, orient) = orient_patches(&TriMesh::new(vertices, triangles));
    log.orient = orient;
    log.ms = start.elapsed().as_secs_f64() * 1e3;
    (oriented, log)
}

/// The kept vertices and, for every input vertex, the kept vertex it became.
fn weld(points: &[[f64; 3]], tolerance: f64, log: &mut RepairLog) -> (Vec<[f64; 3]>, Vec<u32>) {
    let mut kept: Vec<[f64; 3]> = Vec::with_capacity(points.len());
    let mut map = Vec::with_capacity(points.len());
    if tolerance <= 0.0 {
        // Identical coordinates only (-0.0 and 0.0 alike).
        let mut exact: HashMap<[u64; 3], u32> = HashMap::with_capacity(points.len());
        for p in points {
            let key = p.map(|v| if v == 0.0 { 0 } else { v.to_bits() });
            let next = kept.len() as u32;
            let index = *exact.entry(key).or_insert(next);
            if index == next {
                kept.push(*p);
            } else {
                log.vertices_welded += 1;
            }
            map.push(index);
        }
        return (kept, map);
    }

    // Cells two tolerances wide, about the lowest corner: the sphere of the tolerance about a vertex
    // reaches only its own cell and the next one on each axis, towards the half it sits in, so eight
    // cells hold every vertex within the tolerance. Each cell keeps its vertices as a linked list.
    let low = points.iter().fold([f64::INFINITY; 3], |m, p| [m[0].min(p[0]), m[1].min(p[1]), m[2].min(p[2])]);
    let width = 2.0 * tolerance;
    let mut heads: HashMap<[i64; 3], u32> = HashMap::with_capacity(points.len());
    let mut next: Vec<u32> = Vec::with_capacity(points.len());
    let limit = tolerance * tolerance;
    for p in points {
        let u = [0, 1, 2].map(|i| (p[i] - low[i]) / width);
        let c = u.map(|v| v.floor() as i64);
        let towards = [0, 1, 2].map(|i| if u[i] - (c[i] as f64) < 0.5 { -1 } else { 1 });
        let mut best: Option<(u32, f64)> = None;
        for corner in 0..8 {
            let key = [0, 1, 2].map(|i| c[i] + if corner >> i & 1 == 1 { towards[i] } else { 0 });
            let mut k = heads.get(&key).copied().unwrap_or(u32::MAX);
            while k != u32::MAX {
                let q = kept[k as usize];
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 <= limit && best.is_none_or(|(b, _)| k < b) {
                    best = Some((k, d2));
                }
                k = next[k as usize];
            }
        }
        match best {
            Some((k, d2)) => {
                log.vertices_welded += 1;
                log.largest_move = log.largest_move.max(d2.sqrt());
                map.push(k);
            }
            None => {
                let k = kept.len() as u32;
                kept.push(*p);
                next.push(heads.insert(c, k).unwrap_or(u32::MAX));
                map.push(k);
            }
        }
    }
    (kept, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A 2 x 2 grid of 10 m squares at mine grid coordinates, as separate triangles that do not
    /// share vertices, with the corners of the second square written 0.4 mm off.
    fn loose_squares() -> TriMesh {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for (sx, sy, jitter) in [(0.0, 0.0, 0.0), (10.0, 0.0, 0.0004), (0.0, 10.0, 0.0), (10.0, 10.0, 0.0)] {
            let corner = |x: f64, y: f64| [E + sx + x + jitter, N + sy + y, 100.0];
            for tri in [[corner(0.0, 0.0), corner(10.0, 0.0), corner(10.0, 10.0)], [corner(0.0, 0.0), corner(10.0, 10.0), corner(0.0, 10.0)]] {
                let base = vertices.len() as u32;
                vertices.extend(tri);
                triangles.push([base, base + 1, base + 2]);
            }
        }
        TriMesh::new(vertices, triangles)
    }

    #[test]
    fn a_weld_merges_within_the_tolerance_and_keeps_the_first_coordinates() {
        let mesh = loose_squares();
        let (welded, log) = repair_mesh(&mesh, 0.001);
        // 24 corners written; the 3 x 3 grid has 9 points.
        assert_eq!(welded.vertices.len(), 9);
        assert_eq!(log.vertices_welded, 15);
        assert!((log.largest_move - 0.0004).abs() < 1e-9, "{}", log.largest_move);
        assert_eq!(welded.vertices[0], [E, N, 100.0], "coordinates kept exactly as read");
        assert_eq!(welded.validity().boundary_edges, 8, "one sheet: only the outer edge is open");
        // The offset square's welded corners moved 0.4 mm: 10.0004 x 10 plus a 10 x 10.0002 trapezoid.
        assert!((welded.area() - 400.006).abs() < 1e-6, "{}", welded.area());

        // Below the jitter the offset square keeps its own four corners: 8 + 4 vertices.
        let (apart, log) = repair_mesh(&mesh, 0.0001);
        assert_eq!(apart.vertices.len(), 12);
        assert_eq!(log.largest_move, 0.0);
        let (exact, _) = repair_mesh(&mesh, 0.0);
        assert_eq!(exact.vertices.len(), 12);
    }

    #[test]
    fn merges_never_chain_along_a_line_of_close_points() {
        // Points 0.6 mm apart: each within 1 mm of its neighbour, the third 1.2 mm from the first.
        let points: Vec<[f64; 3]> = (0..3).map(|i| [E + 0.0006 * i as f64, N, 0.0]).collect();
        let mut log = RepairLog::default();
        let (kept, map) = weld(&points, 0.001, &mut log);
        assert_eq!(kept.len(), 2, "the third point stays: it is farther than 1 mm from the first kept one");
        assert_eq!(map, [0, 0, 1]);
    }

    #[test]
    fn collapsed_degenerate_duplicate_and_unused_are_dropped_and_counted() {
        let v = vec![
            [E, N, 0.0],
            [E + 10.0, N, 0.0],
            [E, N + 10.0, 0.0],
            [E + 0.0005, N, 0.0], // welds onto 0
            [E + 20.0, N, 0.0],   // in line with 0 and 1
            [E + 50.0, N + 50.0, 0.0], // used by nothing
        ];
        let t = vec![[0, 1, 2], [3, 1, 0], [0, 1, 4], [2, 1, 0], [1, 2, 3]];
        let (mesh, log) = repair_mesh(&TriMesh::new(v, t), 0.001);
        assert_eq!(mesh.triangles.len(), 1);
        assert_eq!((log.collapsed_dropped, log.degenerate_dropped, log.duplicates_dropped), (1, 1, 2));
        assert_eq!((log.vertices_welded, log.unused_dropped), (1, 2));
    }

    #[test]
    fn patches_are_wound_consistently_and_face_up_or_out() {
        // A scrambled box (every other face reversed, then the whole lot) comes out facing out with
        // its volume positive.
        let size = [10.0, 20.0, 5.0];
        let v: Vec<[f64; 3]> = (0..8).map(|i| [E + (i & 1) as f64 * size[0], N + (i >> 1 & 1) as f64 * size[1], (i >> 2) as f64 * size[2]]).collect();
        let outward = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        let scrambled: Vec<[u32; 3]> = outward.iter().enumerate().map(|(i, &[a, b, c])| if i % 2 == 0 { [a, b, c] } else { [a, c, b] }).map(|[a, b, c]| [a, c, b]).collect();
        let (boxed, log) = repair_mesh(&TriMesh::new(v, scrambled), 0.001);
        assert_eq!((log.orient.patches, log.orient.closed_patches), (1, 1));
        assert!(boxed.validity().is_watertight());
        assert!((boxed.signed_volume() - 1000.0).abs() < 1e-6, "{}", boxed.signed_volume());

        // A sheet facing down is turned up.
        let (sheet, log) = repair_mesh(&loose_squares().flipped(), 0.001);
        assert_eq!(log.orient.triangles_flipped, 8);
        assert!(sheet.triangles.iter().all(|&t| sheet.triangle_cross(t)[2] > 0.0));
    }

    #[test]
    fn edges_with_three_faces_are_listed_and_stop_the_fill() {
        // Three fins on one edge.
        let v = vec![[E, N, 0.0], [E, N, 10.0], [E + 10.0, N, 5.0], [E, N + 10.0, 5.0], [E - 10.0, N, 5.0]];
        let t = vec![[0, 1, 2], [0, 1, 3], [1, 0, 4]];
        let (_, log) = repair_mesh(&TriMesh::new(v, t), 0.001);
        assert_eq!(log.orient.non_manifold_edges, 1);
        assert_eq!(log.orient.non_manifold_at, [[E, N, 5.0]]);
        assert_eq!(log.orient.patches, 3);
    }

    #[test]
    fn a_one_sided_strip_is_left_as_read_and_counted() {
        // A Möbius strip of 5 quads: the last joins the first edge reversed.
        let n = 5;
        let mut v = Vec::new();
        for i in 0..n {
            let a = std::f64::consts::PI * 2.0 * i as f64 / n as f64;
            let half = a / 2.0;
            let (r, w) = (50.0, 5.0 * half.cos());
            let lift = 5.0 * half.sin();
            v.push([E + (r + w) * a.cos(), N + (r + w) * a.sin(), lift]);
            v.push([E + (r - w) * a.cos(), N + (r - w) * a.sin(), -lift]);
        }
        let mut t = Vec::new();
        for i in 0..n as u32 {
            let (a, b) = (2 * i, 2 * i + 1);
            let (c, d) = if i + 1 < n as u32 { (2 * i + 2, 2 * i + 3) } else { (1, 0) };
            t.push([a, b, d]);
            t.push([a, d, c]);
        }
        let mesh = TriMesh::new(v, t);
        let (read, log) = repair_mesh(&mesh, 0.001);
        assert_eq!(log.orient.non_orientable_patches, 1);
        assert_eq!(log.orient.triangles_flipped, 0);
        assert_eq!(read.triangles, mesh.triangles);
    }
}
