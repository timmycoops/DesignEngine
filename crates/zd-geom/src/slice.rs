//! Sections (Phase 2): a vertical plane through two plan points cuts a mesh into exact lines.
//!
//! Each triangle that crosses the plane gives one segment, between the two edges it crosses. An edge
//! is crossed once, whichever triangles share it, so neighbours meet at the same point to the bit and
//! the segments chain into whole lines with no gaps. A vertex on the plane counts as on its positive
//! side: a mesh is never cut through a corner, so a closed solid always gives closed loops. Lines
//! run the way the triangles are wound, so on a closed solid a hole runs against its outer edge.

use std::collections::HashMap;

use zd_core::TriMesh;

use crate::{GeomError, worker_threads};

/// A vertical plane through two plan points, in mine grid coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionPlane {
    /// The first point: distance along the section is measured from it.
    pub origin: [f64; 2],
    /// Unit plan direction from the first point towards the second.
    pub direction: [f64; 2],
}

impl SectionPlane {
    /// The plane through `a` and `b`, refused if they are the same point in plan.
    pub fn through(a: [f64; 2], b: [f64; 2]) -> Result<Self, GeomError> {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx.hypot(dy);
        if length == 0.0 || !length.is_finite() {
            return Err(GeomError::InvalidInput("a section needs two different points in plan".into()));
        }
        Ok(SectionPlane { origin: a, direction: [dx / length, dy / length] })
    }

    /// How far `p` lies off the plane (m), positive to the left looking along the section.
    pub fn offset(&self, p: [f64; 3]) -> f64 {
        self.direction[0] * (p[1] - self.origin[1]) - self.direction[1] * (p[0] - self.origin[0])
    }

    /// How far along the section `p` lies (m), from the first point.
    pub fn along(&self, p: [f64; 3]) -> f64 {
        self.direction[0] * (p[0] - self.origin[0]) + self.direction[1] * (p[1] - self.origin[1])
    }

    /// The parallel plane `distance` to the left (right if negative).
    pub fn stepped(&self, distance: f64) -> Self {
        let [dx, dy] = self.direction;
        SectionPlane { origin: [self.origin[0] - dy * distance, self.origin[1] + dx * distance], direction: self.direction }
    }
}

/// One line of a section.
#[derive(Debug, Clone, PartialEq)]
pub struct SliceLine {
    pub points: Vec<[f64; 3]>,
    /// The line closes on itself; the first point is not repeated at the end.
    pub closed: bool,
}

/// The lines where `plane` cuts `mesh`.
pub fn slice_mesh(mesh: &TriMesh, plane: &SectionPlane) -> Vec<SliceLine> {
    let offsets = offsets_from(mesh, plane);
    let crossing = crossing_triangles(mesh, &offsets);
    let mut nodes: HashMap<(u32, u32), u32> = HashMap::with_capacity(crossing.len() * 2);
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(crossing.len() * 2);
    let mut links: Vec<(u32, u32)> = Vec::with_capacity(crossing.len());
    let mut node_on = |u: u32, v: u32| {
        // The same way round from either triangle, so an edge gives one point, to the bit.
        let key = (u.min(v), u.max(v));
        *nodes.entry(key).or_insert_with(|| {
            let (a, b) = (key.0 as usize, key.1 as usize);
            let t = offsets[a] / (offsets[a] - offsets[b]);
            let (p, q) = (mesh.vertices[a], mesh.vertices[b]);
            points.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])]);
            points.len() as u32 - 1
        })
    };
    for &triangle in &crossing {
        let corners = mesh.triangles[triangle as usize];
        // Round the triangle's winding, its edges cross one each way. Neighbours wind a shared edge
        // opposite ways, so the end of one segment is the start of the next.
        let (mut entering, mut leaving) = (0, 0);
        for k in 0..3 {
            let (u, v) = (corners[k], corners[(k + 1) % 3]);
            match (offsets[u as usize] >= 0.0, offsets[v as usize] >= 0.0) {
                (true, false) => leaving = node_on(u, v),
                (false, true) => entering = node_on(u, v),
                _ => {}
            }
        }
        links.push((entering, leaving));
    }
    chain(&links, &points)
}

/// The parts of a string within `width` of the plane, as segments laid flat onto it: a section is a
/// thin slice, and a string runs through it rather than lying in it.
pub fn string_near_plane(points: &[[f64; 3]], closed: bool, plane: &SectionPlane, width: f64) -> Vec<[[f64; 3]; 2]> {
    let n = points.len();
    let flat = |p: [f64; 3]| {
        let along = plane.along(p);
        [plane.origin[0] + along * plane.direction[0], plane.origin[1] + along * plane.direction[1], p[2]]
    };
    let at = |p: [f64; 3], q: [f64; 3], t: f64| [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])];
    let mut out = Vec::new();
    for k in 0..if closed { n } else { n.saturating_sub(1) } {
        let (p, q) = (points[k], points[(k + 1) % n]);
        let (dp, dq) = (plane.offset(p), plane.offset(q));
        // The stretch of the segment, from 0 to 1, that stays inside the width.
        let (mut from, mut to) = (0.0f64, 1.0f64);
        if dp != dq {
            let (enter, leave) = ((-width - dp) / (dq - dp), (width - dp) / (dq - dp));
            from = from.max(enter.min(leave));
            to = to.min(enter.max(leave));
        } else if dp.abs() > width {
            continue;
        }
        if from < to {
            out.push([flat(at(p, q, from)), flat(at(p, q, to))]);
        }
    }
    out
}

/// Every vertex's offset from the plane, across the cores for a large mesh.
fn offsets_from(mesh: &TriMesh, plane: &SectionPlane) -> Vec<f64> {
    let chunk = mesh.vertices.len().div_ceil(worker_threads(mesh.vertices.len())).max(1);
    let mut offsets = vec![0.0; mesh.vertices.len()];
    std::thread::scope(|scope| {
        for (part, vertices) in offsets.chunks_mut(chunk).zip(mesh.vertices.chunks(chunk)) {
            scope.spawn(move || {
                for (offset, &p) in part.iter_mut().zip(vertices) {
                    *offset = plane.offset(p);
                }
            });
        }
    });
    offsets
}

/// The triangles with corners on both sides of the plane, in order.
fn crossing_triangles(mesh: &TriMesh, offsets: &[f64]) -> Vec<u32> {
    let chunk = mesh.triangles.len().div_ceil(worker_threads(mesh.triangles.len())).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = mesh
            .triangles
            .chunks(chunk)
            .enumerate()
            .map(|(part, triangles)| {
                scope.spawn(move || {
                    let side = |v: u32| offsets[v as usize] >= 0.0;
                    let crosses = |t: &[u32; 3]| side(t[0]) != side(t[1]) || side(t[0]) != side(t[2]);
                    triangles.iter().enumerate().filter(|(_, t)| crosses(t)).map(|(k, _)| (part * chunk + k) as u32).collect::<Vec<u32>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a section thread panicked")).collect()
    })
}

/// Chains directed links between points into lines: open lines from the points they start at, then
/// the loops that are left.
fn chain(links: &[(u32, u32)], points: &[[f64; 3]]) -> Vec<SliceLine> {
    let mut outgoing: Vec<Vec<u32>> = vec![Vec::new(); points.len()];
    let mut incoming = vec![0u32; points.len()];
    for (k, &(from, to)) in links.iter().enumerate() {
        outgoing[from as usize].push(k as u32);
        incoming[to as usize] += 1;
    }
    let mut lines = Vec::new();
    let starts = (0..points.len() as u32).filter(|&p| outgoing[p as usize].len() > incoming[p as usize] as usize).collect::<Vec<_>>();
    for start in starts {
        while !outgoing[start as usize].is_empty() {
            lines.extend(walk(start, links, &mut outgoing, points));
        }
    }
    for start in 0..points.len() as u32 {
        while !outgoing[start as usize].is_empty() {
            lines.extend(walk(start, links, &mut outgoing, points));
        }
    }
    lines
}

/// Follows unused links from `start` until they run out or the line comes back to it.
fn walk(start: u32, links: &[(u32, u32)], outgoing: &mut [Vec<u32>], points: &[[f64; 3]]) -> Option<SliceLine> {
    let mut line = vec![points[start as usize]];
    let mut at = start;
    let mut closed = false;
    while let Some(link) = outgoing[at as usize].pop() {
        at = links[link as usize].1;
        let p = points[at as usize];
        // A vertex on the plane gives two points in one place.
        if line.last() != Some(&p) {
            line.push(p);
        }
        if at == start {
            closed = true;
            break;
        }
    }
    if closed && line.first() == line.last() {
        line.pop();
    }
    (line.len() >= if closed { 3 } else { 2 }).then_some(SliceLine { points: line, closed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A box wound outward.
    fn block(min: [f64; 3], size: [f64; 3]) -> TriMesh {
        let v = (0..8).map(|i| [min[0] + (i & 1) as f64 * size[0], min[1] + (i >> 1 & 1) as f64 * size[1], min[2] + (i >> 2) as f64 * size[2]]).collect();
        let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh::new(v, t)
    }

    /// The loop's signed area on the section, along it and up it.
    fn area(line: &SliceLine, plane: &SectionPlane) -> f64 {
        let at = |k: usize| {
            let p = line.points[k % line.points.len()];
            (plane.along(p), p[2])
        };
        (0..line.points.len()).map(|k| at(k).0 * at(k + 1).1 - at(k + 1).0 * at(k).1).sum::<f64>() / 2.0
    }

    fn length(line: &SliceLine) -> f64 {
        let n = line.points.len();
        let segments = if line.closed { n } else { n - 1 };
        (0..segments)
            .map(|k| {
                let (p, q) = (line.points[k], line.points[(k + 1) % n]);
                ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()
            })
            .sum()
    }

    #[test]
    fn a_cube_cut_through_the_middle_is_one_closed_square() {
        let cube = block([E, N, 100.0], [10.0, 10.0, 10.0]);
        let plane = SectionPlane::through([E + 5.0, N - 3.0], [E + 5.0, N + 20.0]).unwrap();
        let lines = slice_mesh(&cube, &plane);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].closed);
        // 10 m by 10 m, to 1e-9 m2 and 1e-9 m.
        assert!((area(&lines[0], &plane).abs() - 100.0).abs() < 1e-9, "{}", area(&lines[0], &plane));
        assert!((length(&lines[0]) - 40.0).abs() < 1e-9, "{}", length(&lines[0]));
        assert!(lines[0].points.iter().all(|&p| plane.offset(p).abs() < 1e-9));
    }

    #[test]
    fn a_plane_through_corners_cuts_as_cleanly_as_one_that_misses_them() {
        // The diagonal runs through two vertical edges: 10 m by 10 x root 2 m.
        let cube = block([E, N, 100.0], [10.0, 10.0, 10.0]);
        let plane = SectionPlane::through([E, N], [E + 10.0, N + 10.0]).unwrap();
        let lines = slice_mesh(&cube, &plane);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].closed);
        let expected = 100.0 * 2f64.sqrt();
        assert!((area(&lines[0], &plane).abs() - expected).abs() < 1e-9, "{}", area(&lines[0], &plane));
    }

    #[test]
    fn a_plane_along_a_face_gives_that_outline_and_one_that_misses_gives_nothing() {
        let cube = block([E, N, 100.0], [10.0, 10.0, 10.0]);
        let on_face = SectionPlane::through([E, N], [E, N + 10.0]).unwrap();
        let lines = slice_mesh(&cube, &on_face);
        assert_eq!(lines.len(), 1);
        assert!((area(&lines[0], &on_face).abs() - 100.0).abs() < 1e-9);
        assert!(slice_mesh(&cube, &SectionPlane::through([E - 5.0, N], [E - 5.0, N + 10.0]).unwrap()).is_empty());
    }

    #[test]
    fn separate_pieces_give_a_loop_each_and_a_hole_runs_the_other_way() {
        let mut both = block([E, N, 100.0], [10.0, 10.0, 10.0]);
        let inner = block([E + 2.0, N + 2.0, 102.0], [6.0, 6.0, 6.0]);
        let offset = both.vertices.len() as u32;
        both.vertices.extend(&inner.vertices);
        // Wound inward, as the wall of a hollow is.
        both.triangles.extend(inner.triangles.iter().map(|t| [t[0] + offset, t[2] + offset, t[1] + offset]));
        let plane = SectionPlane::through([E + 5.0, N - 1.0], [E + 5.0, N + 11.0]).unwrap();
        let lines = slice_mesh(&both, &plane);
        assert_eq!(lines.len(), 2);
        let areas: Vec<f64> = lines.iter().map(|l| area(l, &plane)).collect();
        assert!(areas[0] * areas[1] < 0.0, "{areas:?}");
        assert!((areas.iter().sum::<f64>().abs() - 64.0).abs() < 1e-9, "100 less 36: {areas:?}");
    }

    #[test]
    fn a_surface_gives_one_open_line_on_the_surface() {
        // A plane grid, z = 100 + 0.1 x + 0.05 y, so every point on the line is known.
        let height = |x: f64, y: f64| 100.0 + 0.1 * (x - E) + 0.05 * (y - N);
        let grid = Grid::from_fn([E, N], [10.0, 10.0], 21, 11, height);
        let surface = triangulate_grid(&grid);
        let plane = SectionPlane::through([E + 3.0, N - 50.0], [E + 187.0, N + 150.0]).unwrap();
        let lines = slice_mesh(&surface, &plane);
        assert_eq!(lines.len(), 1, "chained, not one piece per triangle");
        assert!(!lines[0].closed);
        assert!(lines[0].points.len() > 10);
        for &p in &lines[0].points {
            assert!(plane.offset(p).abs() < 1e-9);
            assert!((p[2] - height(p[0], p[1])).abs() < 1e-9, "{p:?}");
        }
    }

    #[test]
    fn a_string_shows_only_where_it_runs_through_the_width() {
        let plane = SectionPlane::through([E, N], [E + 100.0, N]).unwrap();
        // Crossing at 45 degrees: 2 m of width takes 2 m along the section, flat on the plane.
        let crossing = [[E + 40.0, N - 10.0, 100.0], [E + 60.0, N + 10.0, 120.0]];
        let segments = string_near_plane(&crossing, false, &plane, 1.0);
        assert_eq!(segments.len(), 1);
        let [p, q] = segments[0];
        assert!((plane.along(q) - plane.along(p) - 2.0).abs() < 1e-9, "{segments:?}");
        assert!(plane.offset(p).abs() < 1e-9 && plane.offset(q).abs() < 1e-9);
        assert!((p[2] - 109.0).abs() < 1e-9 && (q[2] - 111.0).abs() < 1e-9, "RLs stay as they are");
        // Lying in the width, it all shows; beyond it, none.
        assert_eq!(string_near_plane(&[[E, N + 0.5, 1.0], [E + 9.0, N + 0.5, 1.0]], false, &plane, 1.0).len(), 1);
        assert!(string_near_plane(&[[E, N + 5.0, 1.0], [E + 9.0, N + 5.0, 1.0]], false, &plane, 1.0).is_empty());
        // A closed string crosses twice.
        let ring = [[E, N - 5.0, 1.0], [E + 10.0, N - 5.0, 1.0], [E + 10.0, N + 5.0, 1.0], [E, N + 5.0, 1.0]];
        assert_eq!(string_near_plane(&ring, true, &plane, 1.0).len(), 2);
    }

    #[test]
    fn stepping_moves_the_plane_square_to_itself() {
        let plane = SectionPlane::through([E, N], [E + 30.0, N + 40.0]).unwrap();
        let stepped = plane.stepped(25.0);
        assert!((plane.offset([stepped.origin[0], stepped.origin[1], 0.0]) - 25.0).abs() < 1e-9);
        assert_eq!(stepped.direction, plane.direction);
        assert_eq!(plane.along([E + 30.0, N + 40.0, 7.0]), 50.0);
        assert!(SectionPlane::through([E, N], [E, N]).is_err());
    }
}
