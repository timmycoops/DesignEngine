//! Snapping on a section (Phase 2): the point on the cut lines nearest the cursor, exact in mine grid
//! coordinates, so a string drawn on a section sits on what was cut and not on a screen's rounding.
//!
//! The order is a crossing, then a vertex, then the nearest point on a line. Crossings come first
//! because they are what a planner is after, where a seam meets a wall or a toe meets a floor, while
//! a cut line's vertices fall wherever a triangle's edge did.

use crate::slice::SectionPlane;

/// Two ends of a cut line's segment, in mine grid coordinates.
pub type CutSegment = [[f64; 3]; 2];

/// How close to a point a dragged vertex's own segments are taken to touch it (m).
const TOUCHING: f64 = 1e-6;

/// The segments nearest the cursor that are tried against each other for crossings.
const CROSSING_SEGMENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionSnapKind {
    Crossing,
    Vertex,
    Segment,
}

/// A point on the cut lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionSnap {
    pub kind: SectionSnapKind,
    pub point: [f64; 3],
    /// The group of lines it is on, and for a crossing the other one.
    pub group: usize,
    pub other: Option<usize>,
}

/// A segment flat on the section, near the cursor.
struct Near {
    ends: [[f64; 3]; 2],
    flat: [[f64; 2]; 2],
    group: usize,
    distance: f64,
}

/// The snap for a cursor at `at` (distance along the section, RL). Vertices and crossings count
/// within `vertex_reach` metres of it and lines within `segment_reach`. `ignoring` is a point on the
/// section whose own segments are left out, so a vertex being dragged doesn't snap to itself.
pub fn snap_to_section(groups: &[&[CutSegment]], plane: &SectionPlane, at: [f64; 2], vertex_reach: f64, segment_reach: f64, ignoring: Option<[f64; 2]>) -> Option<SectionSnap> {
    let flat = |p: [f64; 3]| [plane.along(p), p[2]];
    let reach = vertex_reach.max(segment_reach);
    let mut vertex: Option<(f64, [f64; 3], usize)> = None;
    let mut near: Vec<Near> = Vec::new();
    for (group, segments) in groups.iter().enumerate() {
        for &ends in segments.iter() {
            let line = ends.map(flat);
            if ignoring.is_some_and(|point| line.iter().any(|end| distance(*end, point) < TOUCHING)) {
                continue;
            }
            let distance_to_line = distance_to_segment(at, line);
            if distance_to_line > reach {
                continue;
            }
            for (end, point) in line.iter().zip(ends) {
                let d = distance(*end, at);
                if d <= vertex_reach && vertex.is_none_or(|best| d < best.0) {
                    vertex = Some((d, point, group));
                }
            }
            near.push(Near { ends, flat: line, group, distance: distance_to_line });
        }
    }
    near.sort_by(|p, q| p.distance.total_cmp(&q.distance));
    near.truncate(CROSSING_SEGMENTS);
    if let Some(crossing) = crossing_near(&near, at, vertex_reach) {
        return Some(crossing);
    }
    if let Some((_, point, group)) = vertex {
        return Some(SectionSnap { kind: SectionSnapKind::Vertex, point, group, other: None });
    }
    let line = near.first().filter(|n| n.distance <= segment_reach)?;
    let point = point_at(line, along_segment(at, line.flat));
    Some(SectionSnap { kind: SectionSnapKind::Segment, point, group: line.group, other: None })
}

/// The nearest place within `reach` of the cursor where two different groups' lines cross.
fn crossing_near(near: &[Near], at: [f64; 2], reach: f64) -> Option<SectionSnap> {
    let mut best: Option<(f64, SectionSnap)> = None;
    for (i, p) in near.iter().enumerate() {
        for q in near[i + 1..].iter().filter(|q| q.group != p.group) {
            let Some(t) = crossing(p.flat, q.flat) else { continue };
            let point = point_at(p, t);
            let d = distance([plane_along(p, t), point[2]], at);
            if d <= reach && best.is_none_or(|b| d < b.0) {
                best = Some((d, SectionSnap { kind: SectionSnapKind::Crossing, point, group: p.group, other: Some(q.group) }));
            }
        }
    }
    best.map(|(_, snap)| snap)
}

/// Where along `p`, from 0 to 1, it crosses `q`. Lines that run together don't cross.
pub(crate) fn crossing(p: [[f64; 2]; 2], q: [[f64; 2]; 2]) -> Option<f64> {
    let r = [p[1][0] - p[0][0], p[1][1] - p[0][1]];
    let s = [q[1][0] - q[0][0], q[1][1] - q[0][1]];
    let denominator = r[0] * s[1] - r[1] * s[0];
    if denominator.abs() < 1e-12 {
        return None;
    }
    let w = [q[0][0] - p[0][0], q[0][1] - p[0][1]];
    let t = (w[0] * s[1] - w[1] * s[0]) / denominator;
    let u = (w[0] * r[1] - w[1] * r[0]) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

fn plane_along(n: &Near, t: f64) -> f64 {
    n.flat[0][0] + t * (n.flat[1][0] - n.flat[0][0])
}

fn point_at(n: &Near, t: f64) -> [f64; 3] {
    let [p, q] = n.ends;
    [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])]
}

fn distance(p: [f64; 2], q: [f64; 2]) -> f64 {
    (p[0] - q[0]).hypot(p[1] - q[1])
}

/// How far along a segment, from 0 to 1, the point nearest `p` lies.
fn along_segment(p: [f64; 2], [a, b]: [[f64; 2]; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    if length == 0.0 { 0.0 } else { (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length).clamp(0.0, 1.0) }
}

fn distance_to_segment(p: [f64; 2], [a, b]: [[f64; 2]; 2]) -> f64 {
    let t = along_segment(p, [a, b]);
    distance(p, [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])])
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A section due east from (E, N), so a point's distance along it is plain.
    fn plane() -> SectionPlane {
        SectionPlane::through([E, N], [E + 100.0, N]).unwrap()
    }

    /// A segment on the section from distance and RL to distance and RL.
    fn segment(from: [f64; 2], to: [f64; 2]) -> CutSegment {
        [[E + from[0], N, from[1]], [E + to[0], N, to[1]]]
    }

    #[test]
    fn a_vertex_comes_back_exact_to_the_bit() {
        // A coordinate no screen could hold, on a line either side of it.
        let corner = [E + 12.345_678_901, N, 98.765_432_109];
        let lines = [[[E, N, 90.0], corner], [corner, [E + 30.0, N, 80.0]]];
        let snap = snap_to_section(&[&lines], &plane(), [12.4, 98.7], 1.0, 0.5, None).unwrap();
        assert_eq!(snap.kind, SectionSnapKind::Vertex);
        assert_eq!(snap.point, corner);
    }

    #[test]
    fn away_from_a_vertex_it_takes_the_nearest_point_on_the_line() {
        // A 1 in 2 line through (0, 100): the nearest point to (20, 105) is at distance 18, RL 91 + 18.
        let line = [segment([0.0, 100.0], [100.0, 150.0])];
        let snap = snap_to_section(&[&line], &plane(), [20.0, 105.0], 1.0, 10.0, None).unwrap();
        assert_eq!(snap.kind, SectionSnapKind::Segment);
        assert!((snap.point[0] - (E + 18.0)).abs() < 1e-9 && (snap.point[2] - 109.0).abs() < 1e-9, "{:?}", snap.point);
        assert!((plane().offset(snap.point)).abs() < 1e-9, "on the plane");
    }

    #[test]
    fn two_groups_crossing_win_over_a_vertex_beside_them() {
        // RL = distance + 100 and RL = 110 - distance cross at distance 5, RL 105, between two vertices.
        let rising = [segment([0.0, 100.0], [5.5, 105.5]), segment([5.5, 105.5], [10.0, 110.0])];
        let falling = [segment([0.0, 110.0], [10.0, 100.0])];
        let snap = snap_to_section(&[&rising, &falling], &plane(), [5.2, 105.2], 1.0, 0.5, None).unwrap();
        assert_eq!((snap.kind, snap.group, snap.other), (SectionSnapKind::Crossing, 0, Some(1)));
        assert!((snap.point[0] - (E + 5.0)).abs() < 1e-9 && (snap.point[2] - 105.0).abs() < 1e-9, "{:?}", snap.point);
    }

    #[test]
    fn a_line_does_not_cross_itself_at_its_own_vertices() {
        let chain = [segment([0.0, 100.0], [5.0, 105.0]), segment([5.0, 105.0], [10.0, 100.0])];
        let snap = snap_to_section(&[&chain], &plane(), [5.1, 105.0], 1.0, 0.5, None).unwrap();
        assert_eq!(snap.kind, SectionSnapKind::Vertex);
    }

    #[test]
    fn lines_laid_along_each_other_do_not_cross() {
        // Two solids sharing a wall cut to the same line.
        let (left, right) = ([segment([0.0, 100.0], [10.0, 100.0])], [segment([0.0, 100.0], [10.0, 100.0])]);
        let snap = snap_to_section(&[&left, &right], &plane(), [5.0, 100.2], 1.0, 0.5, None).unwrap();
        assert_eq!(snap.kind, SectionSnapKind::Segment);
    }

    #[test]
    fn nothing_in_reach_is_no_snap() {
        let line = [segment([0.0, 100.0], [10.0, 100.0])];
        assert_eq!(snap_to_section(&[&line], &plane(), [5.0, 103.0], 1.0, 2.0, None), None);
        assert_eq!(snap_to_section(&[], &plane(), [5.0, 100.0], 1.0, 2.0, None), None);
    }

    #[test]
    fn a_dragged_vertex_does_not_snap_to_its_own_segments() {
        let string = [segment([0.0, 100.0], [5.0, 105.0]), segment([5.0, 105.0], [10.0, 100.0])];
        let other = [segment([0.0, 90.0], [10.0, 90.0])];
        let groups: [&[CutSegment]; 2] = [&string, &other];
        assert_eq!(snap_to_section(&groups, &plane(), [5.0, 105.0], 1.0, 1.0, Some([5.0, 105.0])), None, "only its own lines are near");
        let snap = snap_to_section(&groups, &plane(), [5.0, 90.5], 1.0, 1.0, Some([5.0, 105.0])).unwrap();
        assert_eq!(snap.group, 1, "other lines still snap");
    }
}
