//! Fills for a section (Phase 2): the closed loops a solid cuts into, triangulated on the plane.
//!
//! Loops run the way a solid is wound: looking at the section, an outer edge runs anticlockwise and a
//! hole the other way. A hole goes with the smallest outer edge round it, and each outer edge is
//! triangulated on its own, so solids that touch or overlap on a layer each fill as themselves. An
//! edge kept in a constrained Delaunay triangulation holds the loops; a flood out from the outside
//! tells what is inside them. The area filled has to match the area the loops enclose: a loop that
//! fails that, such as one that crosses itself, stays unfilled and is counted, so its lines show as
//! they were cut and nothing is coloured in that isn't there.
//!
//! A seam has no loops: its roof and floor are surfaces, and they cut into open lines. Each line is a
//! height along the section, so the fill walks both together, from breakpoint to breakpoint, and
//! takes in what lies between them wherever both exist and the roof is above the floor.

use spade::handles::{FixedFaceHandle, InnerTag};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use crate::slice::{SectionPlane, SliceLine};
use crate::worker_threads;

type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

/// How far the area filled may stray from the area enclosed, as a share of it.
const AREA_TOLERANCE: f64 = 1e-6;
/// How far a surface's line may run backwards along the section (m): rounding, not a real fold.
const ALONG_TOLERANCE: f64 = 1e-6;

/// What a section's closed loops fill.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SectionFill {
    /// Triangles on the plane, in mine grid coordinates.
    pub triangles: Vec<[[f64; 3]; 3]>,
    /// Loops that could not be filled.
    pub unfilled: usize,
}

/// A closed loop flat on the section: distance along it, and RL.
struct Outline {
    points: Vec<[f64; 2]>,
    /// Signed: positive runs anticlockwise.
    area: f64,
    low: [f64; 2],
    high: [f64; 2],
}

impl Outline {
    fn on(line: &SliceLine, plane: &SectionPlane) -> Self {
        let points: Vec<[f64; 2]> = line.points.iter().map(|&p| [plane.along(p), p[2]]).collect();
        let n = points.len();
        let area = (0..n).map(|k| points[k][0] * points[(k + 1) % n][1] - points[(k + 1) % n][0] * points[k][1]).sum::<f64>() / 2.0;
        let fold = |pick: fn(f64, f64) -> f64, from: f64| [0, 1].map(|axis| points.iter().fold(from, |at, p| pick(at, p[axis])));
        let (low, high) = (fold(f64::min, f64::INFINITY), fold(f64::max, f64::NEG_INFINITY));
        Outline { points, area, low, high }
    }

    /// Whether `p` lies inside, counting the edges a line from it crosses.
    fn holds(&self, p: [f64; 2]) -> bool {
        if p[0] < self.low[0] || p[0] > self.high[0] || p[1] < self.low[1] || p[1] > self.high[1] {
            return false;
        }
        let n = self.points.len();
        let crossings = (0..n).filter(|&k| {
            let (a, b) = (self.points[k], self.points[(k + 1) % n]);
            (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        });
        crossings.count() % 2 == 1
    }
}

/// Triangulates the closed loops in `lines`, cut from a solid by `plane`. Open lines fill nothing.
pub fn fill_section_loops(lines: &[SliceLine], plane: &SectionPlane) -> SectionFill {
    let outlines = lines.iter().filter(|l| l.closed).map(|l| Outline::on(l, plane));
    let (mut outers, holes): (Vec<Outline>, Vec<Outline>) = outlines.filter(|o| o.area != 0.0).partition(|o| o.area > 0.0);
    outers.sort_by(|p, q| p.area.total_cmp(&q.area));
    let mut fill = SectionFill::default();
    let mut held: Vec<Vec<&Outline>> = vec![Vec::new(); outers.len()];
    for hole in &holes {
        match outers.iter().position(|o| o.area > -hole.area && o.holds(hole.points[0])) {
            Some(outer) => held[outer].push(hole),
            None => fill.unfilled += 1,
        }
    }
    let on_plane = |[along, rl]: [f64; 2]| [plane.origin[0] + along * plane.direction[0], plane.origin[1] + along * plane.direction[1], rl];
    let jobs: Vec<(&Outline, &Vec<&Outline>)> = outers.iter().zip(&held).collect();
    let threads = worker_threads(outers.iter().map(|o| o.points.len()).sum());
    let filled: Vec<Vec<FlatFill>> = std::thread::scope(|scope| {
        let chunks = jobs.chunks(jobs.len().div_ceil(threads).max(1));
        let workers: Vec<_> = chunks.map(|chunk| scope.spawn(move || chunk.iter().map(|(outer, holes)| fill_outline(outer, holes)).collect())).collect();
        workers.into_iter().map(|worker| worker.join().expect("fill worker")).collect()
    });
    for (result, (_, holes)) in filled.into_iter().flatten().zip(&jobs) {
        match result {
            Some(triangles) => fill.triangles.extend(triangles.into_iter().map(|t| t.map(on_plane))),
            None => fill.unfilled += 1 + holes.len(),
        }
    }
    fill
}

/// One outer edge's triangles flat on the section, if it filled.
type FlatFill = Option<Vec<[[f64; 2]; 3]>>;

/// What lies between a seam's roof and floor on a section: wherever both exist and the roof is above
/// the floor. Where one passes through the other the fill stops at the crossing.
pub fn fill_between_lines(roof: &[SliceLine], floor: &[SliceLine], plane: &SectionPlane) -> SectionFill {
    let mut fill = SectionFill::default();
    let mut profiles_of = |lines: &[SliceLine]| {
        let profiles: Vec<Option<Vec<[f64; 2]>>> = lines.iter().filter(|l| !l.closed).map(|l| profile(l, plane)).collect();
        fill.unfilled += profiles.iter().filter(|p| p.is_none()).count();
        profiles.into_iter().flatten().collect::<Vec<_>>()
    };
    let (roofs, floors) = (profiles_of(roof), profiles_of(floor));
    let mut flat = Vec::new();
    for top in &roofs {
        for base in &floors {
            bands_between(top, base, &mut flat);
        }
    }
    let on_plane = |[along, rl]: [f64; 2]| [plane.origin[0] + along * plane.direction[0], plane.origin[1] + along * plane.direction[1], rl];
    fill.triangles.extend(flat.into_iter().map(|t: [[f64; 2]; 3]| t.map(on_plane)));
    fill
}

/// A surface's line as RL by distance along the section, running forward. A line that truly runs
/// back on itself is not a height along the section, and has no profile.
fn profile(line: &SliceLine, plane: &SectionPlane) -> Option<Vec<[f64; 2]>> {
    let mut points: Vec<[f64; 2]> = line.points.iter().map(|&p| [plane.along(p), p[2]]).collect();
    if points.first()?[0] > points.last()?[0] {
        points.reverse();
    }
    if points.windows(2).any(|w| w[1][0] < w[0][0] - ALONG_TOLERANCE) {
        return None;
    }
    points.sort_by(|p, q| p[0].total_cmp(&q[0]));
    Some(points)
}

/// Walks a roof and floor together over the stretch they share, band by band.
fn bands_between(roof: &[[f64; 2]], floor: &[[f64; 2]], out: &mut Vec<[[f64; 2]; 3]>) {
    let to = roof[roof.len() - 1][0].min(floor[floor.len() - 1][0]);
    let mut along = roof[0][0].max(floor[0][0]);
    let (mut r, mut f) = (0, 0);
    while along < to {
        while r + 2 < roof.len() && roof[r + 1][0] <= along {
            r += 1;
        }
        while f + 2 < floor.len() && floor[f + 1][0] <= along {
            f += 1;
        }
        let next = roof[r + 1][0].min(floor[f + 1][0]).min(to);
        band([along, next], [height(roof, r, along), height(roof, r, next)], [height(floor, f, along), height(floor, f, next)], out);
        along = next;
    }
}

/// The RL at `along` on the segment of `line` from point `k`.
fn height(line: &[[f64; 2]], k: usize, along: f64) -> f64 {
    let (p, q) = (line[k], line[k + 1]);
    if q[0] == p[0] { p[1] } else { p[1] + (q[1] - p[1]) * (along - p[0]) / (q[0] - p[0]) }
}

/// The part of one band, straight at both edges, where the roof is above the floor.
fn band([s0, s1]: [f64; 2], [r0, r1]: [f64; 2], [f0, f1]: [f64; 2], out: &mut Vec<[[f64; 2]; 3]>) {
    let (t0, t1) = (r0 - f0, r1 - f1);
    // Where the roof meets the floor, as a share of the way across the band.
    let at = t0 / (t0 - t1);
    let crossing = [s0 + (s1 - s0) * at, f0 + (f1 - f0) * at];
    match (t0 > 0.0, t1 > 0.0) {
        (true, true) => out.extend([[[s0, r0], [s0, f0], [s1, f1]], [[s0, r0], [s1, f1], [s1, r1]]]),
        (true, false) => out.push([[s0, r0], [s0, f0], crossing]),
        (false, true) => out.push([crossing, [s1, f1], [s1, r1]]),
        (false, false) => {}
    }
}

/// The triangles inside one outer edge and its holes, if they add up to the area enclosed. spade
/// asserts on some near-degenerate constraints; a panic there leaves this loop unfilled, not the
/// section gone.
fn fill_outline(outer: &Outline, holes: &[&Outline]) -> FlatFill {
    let triangles = std::panic::catch_unwind(|| triangles_inside(outer, holes)).ok()??;
    let filled: f64 = triangles.iter().map(|&[a, b, c]| ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0).sum();
    let enclosed = outer.area + holes.iter().map(|h| h.area).sum::<f64>();
    ((filled - enclosed).abs() <= AREA_TOLERANCE * enclosed).then_some(triangles)
}

fn triangles_inside(outer: &Outline, holes: &[&Outline]) -> FlatFill {
    let mut cdt = Cdt::new();
    for ring in std::iter::once(outer).chain(holes.iter().copied()) {
        let mut handles = Vec::with_capacity(ring.points.len());
        for &[x, y] in &ring.points {
            handles.push(cdt.insert(Point2::new(x, y)).ok()?);
        }
        for k in 0..handles.len() {
            let (from, to) = (handles[k], handles[(k + 1) % handles.len()]);
            if from != to {
                cdt.add_constraint_and_split(from, to, |p| p);
            }
        }
    }
    let inside = faces_inside(&cdt);
    let corners = |face: spade::handles::FaceHandle<InnerTag, _, _, _, _>| face.vertices().map(|v| [v.position().x, v.position().y]);
    Some(cdt.inner_faces().filter(|f| inside[f.fix().index()]).map(corners).collect())
}

/// Which faces lie inside the kept edges, numbered as spade numbers them. The flood takes in all it
/// can reach before crossing a kept edge, so a kept edge with outside on both sides, a spike doubling
/// back on itself, changes nothing.
fn faces_inside(cdt: &Cdt) -> Vec<bool> {
    let mut inside = vec![false; cdt.num_all_faces()];
    let mut seen = vec![false; cdt.num_all_faces()];
    let touching_outside = cdt.directed_edges().filter(|e| e.face().is_outer()).filter_map(|e| e.rev().face().as_inner().map(|f| (f.fix(), e.as_undirected().fix())));
    let mut across: Vec<FixedFaceHandle<InnerTag>> = Vec::new();
    let mut region: Vec<FixedFaceHandle<InnerTag>> = Vec::new();
    for (face, edge) in touching_outside {
        if cdt.is_constraint_edge(edge) {
            across.push(face);
        } else if !seen[face.index()] {
            seen[face.index()] = true;
            region.push(face);
        }
    }
    let mut filling = false;
    loop {
        while let Some(face) = region.pop() {
            inside[face.index()] = filling;
            for edge in cdt.face(face).adjacent_edges() {
                let Some(next) = edge.rev().face().as_inner() else { continue };
                let next = next.fix();
                if seen[next.index()] {
                    continue;
                }
                if cdt.is_constraint_edge(edge.as_undirected().fix()) {
                    across.push(next);
                } else {
                    seen[next.index()] = true;
                    region.push(next);
                }
            }
        }
        filling = !filling;
        region.extend(std::mem::take(&mut across).into_iter().filter(|f| !seen[f.index()]));
        region.sort_by_key(|f| f.index());
        region.dedup();
        for face in &region {
            seen[face.index()] = true;
        }
        if region.is_empty() {
            return inside;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slice::slice_mesh;
    use crate::triangulate_grid;
    use zd_core::{Grid, TriMesh};

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;
    /// To 1e-7 m2: a coordinate this size holds to about 1e-9 m, over sides of 10 to 20 m.
    const AREA_NEAR: f64 = 1e-7;

    /// An open line on the plane, given as distance along it and RL.
    fn line(plane: &SectionPlane, points: &[[f64; 2]]) -> SliceLine {
        SliceLine { closed: false, ..ring(plane, points) }
    }

    fn plane() -> SectionPlane {
        SectionPlane::through([E, N], [E + 300.0, N + 400.0]).unwrap()
    }

    /// A closed loop through points on the plane, given as distance along it and RL.
    fn ring(plane: &SectionPlane, points: &[[f64; 2]]) -> SliceLine {
        let point = |&[along, rl]: &[f64; 2]| [plane.origin[0] + along * plane.direction[0], plane.origin[1] + along * plane.direction[1], rl];
        SliceLine { points: points.iter().map(point).collect(), closed: true }
    }

    /// A rectangle, anticlockwise for an outer edge and the other way for a hole.
    fn rectangle(plane: &SectionPlane, low: [f64; 2], high: [f64; 2], outer: bool) -> SliceLine {
        let mut corners = vec![low, [high[0], low[1]], high, [low[0], high[1]]];
        if !outer {
            corners.reverse();
        }
        ring(plane, &corners)
    }

    fn area(fill: &SectionFill) -> f64 {
        let area = |t: &[[f64; 3]; 3]| {
            let [u, v] = [1, 2].map(|k| [t[k][0] - t[0][0], t[k][1] - t[0][1], t[k][2] - t[0][2]]);
            let cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() / 2.0
        };
        fill.triangles.iter().map(area).sum()
    }

    /// A box wound outward.
    fn block(min: [f64; 3], size: [f64; 3]) -> TriMesh {
        let v = (0..8).map(|i| [min[0] + (i & 1) as f64 * size[0], min[1] + (i >> 1 & 1) as f64 * size[1], min[2] + (i >> 2) as f64 * size[2]]).collect();
        let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh::new(v, t)
    }

    #[test]
    fn a_cube_cut_through_the_middle_fills_a_square() {
        let cube = block([E, N, 100.0], [10.0, 10.0, 10.0]);
        let plane = SectionPlane::through([E + 5.0, N - 3.0], [E + 5.0, N + 20.0]).unwrap();
        let fill = fill_section_loops(&slice_mesh(&cube, &plane), &plane);
        // 10 m by 10 m: this is also what says an outer edge runs anticlockwise.
        assert!((area(&fill) - 100.0).abs() < AREA_NEAR, "{}", area(&fill));
        assert_eq!(fill.unfilled, 0);
        assert!(fill.triangles.iter().flatten().all(|&p| plane.offset(p).abs() < 1e-9 && (100.0..=110.0).contains(&p[2])), "on the plane, within the cube");
    }

    #[test]
    fn a_hollow_cut_from_a_mesh_leaves_its_hole_out() {
        let mut both = block([E, N, 100.0], [10.0, 10.0, 10.0]);
        let inner = block([E + 2.0, N + 2.0, 102.0], [6.0, 6.0, 6.0]);
        let offset = both.vertices.len() as u32;
        both.vertices.extend(&inner.vertices);
        // Wound inward, as the wall of a hollow is.
        both.triangles.extend(inner.triangles.iter().map(|t| [t[0] + offset, t[2] + offset, t[1] + offset]));
        let plane = SectionPlane::through([E + 5.0, N - 1.0], [E + 5.0, N + 11.0]).unwrap();
        let fill = fill_section_loops(&slice_mesh(&both, &plane), &plane);
        assert!((area(&fill) - 64.0).abs() < AREA_NEAR, "100 less 36: {}", area(&fill));
        assert_eq!(fill.unfilled, 0);
    }

    #[test]
    fn a_concave_loop_fills_only_what_it_encloses() {
        let plane = plane();
        // An L: 10 by 10 less a 5 by 5 notch at the top right.
        let l = ring(&plane, &[[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [5.0, 5.0], [5.0, 10.0], [0.0, 10.0]]);
        let fill = fill_section_loops(&[l], &plane);
        assert!((area(&fill) - 75.0).abs() < AREA_NEAR, "{}", area(&fill));
        let centres = fill.triangles.iter().map(|t| ((plane.along(t[0]) + plane.along(t[1]) + plane.along(t[2])) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0));
        assert!(centres.into_iter().all(|(along, rl)| along < 5.0 || rl < 5.0), "nothing in the notch");
    }

    #[test]
    fn an_island_in_a_hole_fills_again() {
        let plane = plane();
        let lines = [rectangle(&plane, [0.0, 0.0], [10.0, 10.0], true), rectangle(&plane, [2.0, 2.0], [8.0, 8.0], false), rectangle(&plane, [4.0, 4.0], [6.0, 6.0], true)];
        let fill = fill_section_loops(&lines, &plane);
        assert!((area(&fill) - 68.0).abs() < AREA_NEAR, "100 less 36 plus 4: {}", area(&fill));
    }

    #[test]
    fn a_hole_goes_with_the_smallest_edge_round_it() {
        // A solid inside a larger one's outline, each whole: the hole is the smaller one's.
        let plane = plane();
        let lines = [rectangle(&plane, [0.0, 0.0], [20.0, 20.0], true), rectangle(&plane, [5.0, 5.0], [15.0, 15.0], true), rectangle(&plane, [9.0, 9.0], [11.0, 11.0], false)];
        let fill = fill_section_loops(&lines, &plane);
        assert!((area(&fill) - 496.0).abs() < AREA_NEAR, "400 plus 100 less 4: {}", area(&fill));
    }

    #[test]
    fn solids_that_touch_or_overlap_each_fill_as_themselves() {
        let plane = plane();
        let touching = [rectangle(&plane, [0.0, 0.0], [10.0, 10.0], true), rectangle(&plane, [10.0, 0.0], [20.0, 10.0], true)];
        assert!((area(&fill_section_loops(&touching, &plane)) - 200.0).abs() < AREA_NEAR);
        let overlapping = [rectangle(&plane, [0.0, 0.0], [10.0, 10.0], true), rectangle(&plane, [5.0, 0.0], [15.0, 10.0], true)];
        assert!((area(&fill_section_loops(&overlapping, &plane)) - 200.0).abs() < AREA_NEAR);
    }

    #[test]
    fn a_long_winding_loop_fills_to_its_area() {
        // A 4,000-point outline with a wavy roof: the fill must match the area it encloses.
        let plane = plane();
        let top: Vec<[f64; 2]> = (0..=4_000).map(|k| [4_000.0 - k as f64, 50.0 + 20.0 * (k as f64 / 60.0).sin()]).collect();
        let points: Vec<[f64; 2]> = [[0.0, 0.0], [4_000.0, 0.0]].into_iter().chain(top).collect();
        let n = points.len();
        let enclosed = (0..n).map(|k| points[k][0] * points[(k + 1) % n][1] - points[(k + 1) % n][0] * points[k][1]).sum::<f64>() / 2.0;
        let fill = fill_section_loops(&[ring(&plane, &points)], &plane);
        assert_eq!(fill.unfilled, 0);
        assert!((area(&fill) - enclosed).abs() < 1e-6 * enclosed, "{} against {enclosed}", area(&fill));
    }

    #[test]
    fn what_cannot_be_filled_is_counted_and_nothing_is_coloured_in() {
        let plane = plane();
        // A bow tie crosses itself, so it encloses nothing it can be held to.
        let bow_tie = ring(&plane, &[[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]]);
        let fill = fill_section_loops(&[bow_tie], &plane);
        assert!(fill.triangles.is_empty());
        // A hole with no edge round it.
        let fill = fill_section_loops(&[rectangle(&plane, [0.0, 0.0], [10.0, 10.0], false)], &plane);
        assert!(fill.triangles.is_empty());
        assert_eq!(fill.unfilled, 1);
    }

    #[test]
    fn a_flat_seam_fills_its_thickness_by_its_length() {
        let plane = plane();
        let (roof, floor) = (line(&plane, &[[0.0, 110.0], [100.0, 110.0]]), line(&plane, &[[0.0, 100.0], [100.0, 100.0]]));
        let fill = fill_between_lines(&[roof], &[floor], &plane);
        assert!((area(&fill) - 1_000.0).abs() < AREA_NEAR, "{}", area(&fill));
        assert_eq!(fill.unfilled, 0);
        assert!(fill.triangles.iter().flatten().all(|&p| plane.offset(p).abs() < 1e-9 && (100.0..=110.0).contains(&p[2])), "on the plane, between the two");
    }

    #[test]
    fn a_roof_that_dips_through_its_floor_stops_at_the_crossing() {
        // 20 m of roof fall over 100 m through a flat floor at the halfway mark: a 50 m by 10 m triangle.
        let plane = plane();
        let (roof, floor) = (line(&plane, &[[0.0, 110.0], [100.0, 90.0]]), line(&plane, &[[0.0, 100.0], [100.0, 100.0]]));
        let fill = fill_between_lines(&[roof.clone()], &[floor.clone()], &plane);
        assert!((area(&fill) - 250.0).abs() < AREA_NEAR, "{}", area(&fill));
        let furthest = fill.triangles.iter().flatten().map(|&p| plane.along(p)).fold(0.0, f64::max);
        assert!((furthest - 50.0).abs() < 1e-6, "{furthest}");
        // And coming up through it the other way.
        let rising = line(&plane, &[[0.0, 90.0], [100.0, 110.0]]);
        assert!((area(&fill_between_lines(&[rising], &[floor], &plane)) - 250.0).abs() < AREA_NEAR);
    }

    #[test]
    fn breakpoints_need_not_match_and_lines_can_run_either_way() {
        let plane = plane();
        let roof = line(&plane, &[[0.0, 112.0], [40.0, 108.0], [100.0, 114.0]]);
        // Against the roof's direction, as a surface's winding can make it.
        let floor = line(&plane, &[[100.0, 100.0], [75.0, 102.0], [50.0, 98.0], [25.0, 100.0], [0.0, 100.0]]);
        // By the trapezium rule over every breakpoint of either.
        let heights = [(0.0, 112.0, 100.0), (25.0, 109.5, 100.0), (40.0, 108.0, 98.8), (50.0, 109.0, 98.0), (75.0, 111.5, 102.0), (100.0, 114.0, 100.0)];
        let expected: f64 = heights.windows(2).map(|w| (w[1].0 - w[0].0) * ((w[0].1 - w[0].2) + (w[1].1 - w[1].2)) / 2.0).sum();
        let fill = fill_between_lines(&[roof], &[floor], &plane);
        assert!((area(&fill) - expected).abs() < AREA_NEAR, "{} against {expected}", area(&fill));
    }

    #[test]
    fn a_seam_fills_only_where_both_exist() {
        // The roof has a gap, as a grid with nulls does: 20 m of 100 m is not seam.
        let plane = plane();
        let roof = [line(&plane, &[[0.0, 110.0], [40.0, 110.0]]), line(&plane, &[[60.0, 110.0], [100.0, 110.0]])];
        let floor = line(&plane, &[[-50.0, 100.0], [150.0, 100.0]]);
        assert!((area(&fill_between_lines(&roof, &[floor], &plane)) - 800.0).abs() < AREA_NEAR);
    }

    #[test]
    fn a_floor_above_its_roof_fills_nothing() {
        let plane = plane();
        let (roof, floor) = (line(&plane, &[[0.0, 100.0], [100.0, 100.0]]), line(&plane, &[[0.0, 110.0], [100.0, 110.0]]));
        assert_eq!(fill_between_lines(&[roof.clone()], &[floor], &plane), SectionFill::default());
        assert_eq!(fill_between_lines(&[roof.clone()], &[], &plane), SectionFill::default());
        // Touching along its length is no seam either.
        assert_eq!(fill_between_lines(&[roof.clone()], &[roof], &plane), SectionFill::default());
    }

    #[test]
    fn a_line_that_runs_back_on_itself_is_counted_and_left_out() {
        let plane = plane();
        let fold = line(&plane, &[[0.0, 110.0], [60.0, 110.0], [40.0, 111.0], [100.0, 110.0]]);
        let fill = fill_between_lines(&[fold], &[line(&plane, &[[0.0, 100.0], [100.0, 100.0]])], &plane);
        assert!(fill.triangles.is_empty());
        assert_eq!(fill.unfilled, 1);
    }

    #[test]
    fn a_seam_cut_from_grids_fills_its_exact_thickness() {
        // Thickness 5 m at the west edge, growing 1 cm a metre east, along a section due east for 200 m:
        // 200 by 5, plus half of 200 by 2. Both surfaces slope, so only the thickness is simple.
        let tilt = |x: f64, y: f64| 100.0 + 0.03 * (x - E) - 0.02 * (y - N);
        let thickness = |x: f64| 5.0 + 0.01 * (x - E);
        let roof = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 21, 11, |x, y| tilt(x, y) + thickness(x)));
        let floor = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 21, 11, tilt));
        let plane = SectionPlane::through([E - 30.0, N + 33.0], [E + 300.0, N + 33.0]).unwrap();
        let fill = fill_between_lines(&slice_mesh(&roof, &plane), &slice_mesh(&floor, &plane), &plane);
        assert!((area(&fill) - 1_200.0).abs() < 1e-6, "{}", area(&fill));
    }

    #[test]
    fn open_lines_fill_nothing() {
        let plane = plane();
        let mut open = rectangle(&plane, [0.0, 0.0], [10.0, 10.0], true);
        open.closed = false;
        assert_eq!(fill_section_loops(&[open], &plane), SectionFill::default());
    }
}
