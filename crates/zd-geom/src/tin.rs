//! Triangulate (Surface tools plan, workstream 4; decision 0096): one surface through the strings picked, every vertex
//! of them a vertex of it at its own RL and every segment kept as an edge (a breakline), so crests and toes stay sharp.
//! spade's constrained Delaunay does the work, in plan, about a whole-metre origin.
//!
//! What bounds it: the outermost closed strings picked, each filling what's inside it (one closed string alone is a pad
//! at its vertices' RLs); with none closed, the hull, with every edge longer than a limit along it peeled back. Strings
//! that cross or touch in plan at different heights can't be one surface: they're refused, each place listed. Where they
//! cross at one height, the crossing becomes a vertex at that height.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use zd_core::{Polyline, TriMesh};

/// Strings this close in height where they cross or touch in plan are at one height there (m).
pub const SAME_HEIGHT: f64 = 1e-3;

/// A vertex this close to a bounding string's edge in plan is on it, not outside it (m).
const ON_THE_BOUND: f64 = 1e-3;

/// The longest edge the hull keeps, unless set (m).
pub const LONGEST_HULL_EDGE: f64 = 200.0;

type Cdt = ConstrainedDelaunayTriangulation<Point2<f64>>;

/// Where two strings cross or touch in plan at different heights: which strings (by their index in the input), where, and
/// each one's height there.
#[derive(Debug, Clone, PartialEq)]
pub struct TinCrossing {
    pub strings: [usize; 2],
    pub at: [f64; 2],
    pub heights: [f64; 2],
}

/// Why the strings can't be made one surface.
#[derive(Debug, Clone, PartialEq)]
pub enum TinRefusal {
    /// They cross or touch in plan at different heights, at these places (the first so many) of `count`.
    Crossings { places: Vec<TinCrossing>, count: usize },
    /// These strings (by index) have vertices outside every bounding string.
    Outside(Vec<usize>),
    /// Anything else, in words.
    Invalid(String),
}

/// What bounds a surface triangulated.
#[derive(Debug, Clone, PartialEq)]
pub enum TinBound {
    /// The outermost closed strings, by their index in the input.
    Strings(Vec<usize>),
    /// The hull, with every edge longer than this peeled back along it (m), and how many triangles that took off.
    Hull { longest: f64, peeled: usize },
}

/// A surface triangulated through strings.
#[derive(Debug, Clone)]
pub struct Triangulated {
    pub mesh: TriMesh,
    pub bound: TinBound,
    /// Its area in plan (m²).
    pub area: f64,
    /// Vertices where strings crossed at one height, added.
    pub crossings_joined: usize,
    pub ms: f64,
}

/// The most crossings a refusal lists.
const LISTED: usize = 10;

/// One surface through `strings`, bounded by the outermost closed ones or, with none closed, the hull with no edge longer
/// than `longest` along it. Every vertex of the strings is a vertex of it at its own RL; every segment is an edge.
pub fn triangulate_strings(strings: &[Polyline], longest: f64) -> Result<Triangulated, TinRefusal> {
    let started = Instant::now();
    if longest.is_nan() || longest <= 0.0 {
        return Err(TinRefusal::Invalid(format!("the longest hull edge must be more than 0 m; {longest} was given")));
    }
    let lines: Vec<Vec<[f64; 3]>> = strings.iter().map(vertices_once).collect();
    if lines.iter().map(Vec::len).sum::<usize>() < 3 {
        return Err(TinRefusal::Invalid("too few vertices to make a surface: three or more are needed".into()));
    }
    let (lo, hi) = lines.iter().flatten().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])]));
    let origin = [((lo[0] + hi[0]) / 2.0).round(), ((lo[1] + hi[1]) / 2.0).round()];
    let local: Vec<Vec<[f64; 3]>> = lines.iter().map(|l| l.iter().map(|p| [p[0] - origin[0], p[1] - origin[1], p[2]]).collect()).collect();
    let closed: Vec<bool> = strings.iter().zip(&local).map(|(s, l)| s.closed && l.len() >= 3).collect();

    let segments = segments_of(&local, &closed);
    let crossings = crossings_at_different_heights(&segments, &local);
    if !crossings.is_empty() {
        let count = crossings.len();
        let places = crossings.into_iter().take(LISTED).map(|c| TinCrossing { at: [c.at[0] + origin[0], c.at[1] + origin[1]], ..c }).collect();
        return Err(TinRefusal::Crossings { places, count });
    }

    // The bounding strings: the closed ones not inside another, larger one.
    let mut rings: Vec<(usize, f64)> = (0..local.len()).filter(|&i| closed[i]).map(|i| (i, ring_area(&local[i]).abs())).filter(|&(_, a)| a > 1e-9).collect();
    rings.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut outer: Vec<usize> = Vec::new();
    for &(i, _) in &rings {
        if !outer.iter().any(|&o| inside_ring(&local[o], plan(local[i][0]))) {
            outer.push(i);
        }
    }
    if !outer.is_empty() {
        let outside: Vec<usize> = (0..local.len())
            .filter(|&i| local[i].iter().any(|p| !outer.iter().any(|&o| inside_ring(&local[o], plan(*p)) || near_ring(&local[o], plan(*p)))))
            .collect();
        if !outside.is_empty() {
            return Err(TinRefusal::Outside(outside));
        }
    }

    let (cdt, heights, crossings_joined) = std::panic::catch_unwind(|| build(&local, &closed))
        .map_err(|_| TinRefusal::Invalid("the triangulation failed on strings that nearly overlap in plan; look for strings drawn twice or doubling back".into()))??;

    let mut kept = vec![false; cdt.num_all_faces()];
    let bound = if outer.is_empty() {
        for f in cdt.inner_faces() {
            kept[f.fix().index()] = true;
        }
        let peeled = peel_hull(&cdt, &mut kept, longest);
        TinBound::Hull { longest, peeled }
    } else {
        for f in cdt.inner_faces() {
            let [a, b, c] = f.vertices().map(|v| v.position());
            let middle = [(a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0];
            kept[f.fix().index()] = outer.iter().any(|&o| inside_ring(&local[o], middle));
        }
        TinBound::Strings(outer)
    };

    let mut index: HashMap<usize, u32> = HashMap::new();
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut area = 0.0;
    for f in cdt.inner_faces().filter(|f| kept[f.fix().index()]) {
        let corners = f.vertices();
        let [a, b, c] = corners.map(|v| v.position());
        area += ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) / 2.0;
        triangles.push(corners.map(|v| {
            let k = v.fix().index();
            *index.entry(k).or_insert_with(|| {
                let p = v.position();
                vertices.push([p.x + origin[0], p.y + origin[1], heights[k]]);
                vertices.len() as u32 - 1
            })
        }));
    }
    if triangles.is_empty() {
        return Err(TinRefusal::Invalid("no triangles: the strings lie on one line in plan, or the hull's edges are all longer than the limit".into()));
    }
    Ok(Triangulated { mesh: TriMesh { vertices, triangles }, bound, area, crossings_joined, ms: started.elapsed().as_secs_f64() * 1e3 })
}

/// A string's vertices, the closing one not repeated, and none twice in a row.
fn vertices_once(line: &Polyline) -> Vec<[f64; 3]> {
    let mut v: Vec<[f64; 3]> = Vec::with_capacity(line.vertices.len());
    for &p in &line.vertices {
        if v.last() != Some(&p) {
            v.push(p);
        }
    }
    if line.closed && v.len() > 1 && v.first() == v.last() {
        v.pop();
    }
    v
}

fn plan(p: [f64; 3]) -> [f64; 2] {
    [p[0], p[1]]
}

/// One segment: its string, its first vertex's index on it, and its ends.
#[derive(Debug, Clone, Copy)]
struct Segment {
    string: usize,
    k: usize,
    a: [f64; 3],
    b: [f64; 3],
}

fn segments_of(lines: &[Vec<[f64; 3]>], closed: &[bool]) -> Vec<Segment> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let n = l.len();
        let count = if closed[i] { n } else { n.saturating_sub(1) };
        for k in 0..count {
            out.push(Segment { string: i, k, a: l[k], b: l[(k + 1) % n] });
        }
    }
    out
}

/// Every place two segments cross or touch in plan at heights more than [`SAME_HEIGHT`] apart. Segments are bucketed on
/// a grid in plan so only those near each other are tried.
fn crossings_at_different_heights(segments: &[Segment], lines: &[Vec<[f64; 3]>]) -> Vec<TinCrossing> {
    if segments.is_empty() {
        return Vec::new();
    }
    let (lo, hi) = segments.iter().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), s| {
        ([lo[0].min(s.a[0]).min(s.b[0]), lo[1].min(s.a[1]).min(s.b[1])], [hi[0].max(s.a[0]).max(s.b[0]), hi[1].max(s.a[1]).max(s.b[1])])
    });
    let mean = segments.iter().map(|s| (s.b[0] - s.a[0]).hypot(s.b[1] - s.a[1])).sum::<f64>() / segments.len() as f64;
    let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6);
    let cell = mean.max(extent / 2048.0).max(1e-6);
    let at = |v: f64, o: f64| ((v - o) / cell).floor() as i64;
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (n, s) in segments.iter().enumerate() {
        for cx in at(s.a[0].min(s.b[0]) - SAME_HEIGHT, lo[0])..=at(s.a[0].max(s.b[0]) + SAME_HEIGHT, lo[0]) {
            for cy in at(s.a[1].min(s.b[1]) - SAME_HEIGHT, lo[1])..=at(s.a[1].max(s.b[1]) + SAME_HEIGHT, lo[1]) {
                buckets.entry((cx, cy)).or_default().push(n);
            }
        }
    }
    let mut tried: HashSet<(usize, usize)> = HashSet::new();
    let mut found = Vec::new();
    let mut seen_places: HashSet<(i64, i64, usize, usize)> = HashSet::new();
    for members in buckets.values() {
        for (x, &p) in members.iter().enumerate() {
            for &q in &members[x + 1..] {
                let pair = (p.min(q), p.max(q));
                if !tried.insert(pair) {
                    continue;
                }
                let (s, t) = (segments[pair.0], segments[pair.1]);
                if s.string == t.string && neighbours(s.k, t.k, lines[s.string].len()) {
                    continue;
                }
                for (point, zs, zt) in meetings(s, t) {
                    if (zs - zt).abs() > SAME_HEIGHT {
                        let key = ((point[0] * 1e3).round() as i64, (point[1] * 1e3).round() as i64, s.string.min(t.string), s.string.max(t.string));
                        if seen_places.insert(key) {
                            found.push(TinCrossing { strings: [s.string, t.string], at: point, heights: [zs, zt] });
                        }
                    }
                }
            }
        }
    }
    found.sort_by(|a, b| a.at[0].total_cmp(&b.at[0]).then(a.at[1].total_cmp(&b.at[1])));
    found
}

/// Two segments of one string sharing a vertex.
fn neighbours(j: usize, k: usize, n: usize) -> bool {
    j.abs_diff(k) == 1 || (n > 2 && j.abs_diff(k) == n - 1)
}

/// Where two segments meet in plan, with each one's height there: where they cross or touch, or, lying along each other,
/// each end of one on the other.
fn meetings(s: Segment, t: Segment) -> Vec<([f64; 2], f64, f64)> {
    let (p, r) = ([s.a[0], s.a[1]], [s.b[0] - s.a[0], s.b[1] - s.a[1]]);
    let (q, u) = ([t.a[0], t.a[1]], [t.b[0] - t.a[0], t.b[1] - t.a[1]]);
    let cross = r[0] * u[1] - r[1] * u[0];
    let qp = [q[0] - p[0], q[1] - p[1]];
    let lengths = (r[0].hypot(r[1]) * u[0].hypot(u[1])).max(1e-300);
    let lerp = |a: [f64; 3], b: [f64; 3], f: f64| a[2] + f * (b[2] - a[2]);
    if cross.abs() > 1e-12 * lengths {
        let a = (qp[0] * u[1] - qp[1] * u[0]) / cross;
        let b = (qp[0] * r[1] - qp[1] * r[0]) / cross;
        let slack_a = ON_THE_BOUND / r[0].hypot(r[1]).max(1e-12) * 1e-3;
        let slack_b = ON_THE_BOUND / u[0].hypot(u[1]).max(1e-12) * 1e-3;
        if a >= -slack_a && a <= 1.0 + slack_a && b >= -slack_b && b <= 1.0 + slack_b {
            let (a, b) = (a.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
            return vec![([p[0] + a * r[0], p[1] + a * r[1]], lerp(s.a, s.b, a), lerp(t.a, t.b, b))];
        }
        return Vec::new();
    }
    // Parallel: only if they lie along each other, each end of one on the other.
    let mut out = Vec::new();
    for (end, on, end_is_s) in [(s.a, t, true), (s.b, t, true), (t.a, s, false), (t.b, s, false)] {
        if let Some(f) = on_segment([end[0], end[1]], on) {
            let other = lerp(on.a, on.b, f);
            out.push(if end_is_s { ([end[0], end[1]], end[2], other) } else { ([end[0], end[1]], other, end[2]) });
        }
    }
    out
}

/// How far along `s` a point lies, if it lies on it in plan within a micrometre.
fn on_segment(p: [f64; 2], s: Segment) -> Option<f64> {
    let (dx, dy) = (s.b[0] - s.a[0], s.b[1] - s.a[1]);
    let length = dx * dx + dy * dy;
    if length == 0.0 {
        return None;
    }
    let f = ((p[0] - s.a[0]) * dx + (p[1] - s.a[1]) * dy) / length;
    if !(-1e-12..=1.0 + 1e-12).contains(&f) {
        return None;
    }
    let gap = (s.a[0] + f * dx - p[0]).hypot(s.a[1] + f * dy - p[1]);
    (gap <= 1e-6).then_some(f.clamp(0.0, 1.0))
}

/// The triangulation, each vertex's height by spade's index, and how many crossings at one height became vertices.
#[allow(clippy::type_complexity)]
fn build(lines: &[Vec<[f64; 3]>], closed: &[bool]) -> Result<(Cdt, Vec<f64>, usize), TinRefusal> {
    let mut cdt = Cdt::new();
    let mut heights: Vec<f64> = Vec::new();
    let height_at = |heights: &mut Vec<f64>, index: usize, z: f64| {
        if heights.len() <= index {
            heights.resize(index + 1, f64::NAN);
        }
        if heights[index].is_nan() {
            heights[index] = z;
        }
    };
    let mut handles: Vec<Vec<_>> = Vec::with_capacity(lines.len());
    for line in lines {
        let mut hs = Vec::with_capacity(line.len());
        for p in line {
            let h = cdt.insert(Point2::new(p[0], p[1])).map_err(|e| TinRefusal::Invalid(format!("a vertex can't be taken: {e:?}")))?;
            height_at(&mut heights, h.index(), p[2]);
            hs.push(h);
        }
        handles.push(hs);
    }
    let mut joined = 0;
    for (i, line) in lines.iter().enumerate() {
        let n = line.len();
        let count = if closed[i] { n } else { n.saturating_sub(1) };
        for k in 0..count {
            let (from, to) = (handles[i][k], handles[i][(k + 1) % n]);
            if from == to {
                continue;
            }
            let before = cdt.num_vertices();
            cdt.add_constraint_and_split(from, to, |p| p);
            // A crossing at one height: the new vertex takes the height along this segment there.
            let (a, b) = (line[k], line[(k + 1) % n]);
            let length = (b[0] - a[0]).hypot(b[1] - a[1]);
            for index in before..cdt.num_vertices() {
                let p = cdt.vertex(FixedVertexHandle::from_index(index)).position();
                let f = if length > 0.0 { ((p.x - a[0]).hypot(p.y - a[1]) / length).clamp(0.0, 1.0) } else { 0.0 };
                height_at(&mut heights, index, a[2] + f * (b[2] - a[2]));
                joined += 1;
            }
        }
    }
    Ok((cdt, heights, joined))
}

/// Peels back from the hull every triangle whose edge on the outside is longer than `longest` in plan and isn't a string's
/// segment, and on inwards as each one goes. Returns how many went.
fn peel_hull(cdt: &Cdt, kept: &mut [bool], longest: f64) -> usize {
    let long = |e: spade::handles::DirectedEdgeHandle<'_, Point2<f64>, (), spade::CdtEdge<()>, ()>| {
        let [a, b] = e.positions();
        (b.x - a.x).hypot(b.y - a.y) > longest && !cdt.is_constraint_edge(e.as_undirected().fix())
    };
    let mut queue: Vec<_> = cdt
        .directed_edges()
        .filter(|e| e.face().is_outer())
        .filter_map(|e| e.rev().face().as_inner().filter(|_| long(e.rev())).map(|f| f.fix()))
        .collect();
    let mut peeled = 0;
    while let Some(f) = queue.pop() {
        if !kept[f.index()] {
            continue;
        }
        kept[f.index()] = false;
        peeled += 1;
        for e in cdt.face(f).adjacent_edges() {
            if let Some(g) = e.rev().face().as_inner()
                && kept[g.fix().index()]
                && long(e)
            {
                queue.push(g.fix());
            }
        }
    }
    peeled
}

fn ring_area(ring: &[[f64; 3]]) -> f64 {
    let o = ring[0];
    let mut twice = 0.0;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        twice += (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1]);
    }
    twice / 2.0
}

fn inside_ring(ring: &[[f64; 3]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            inside = !inside;
        }
    }
    inside
}

fn near_ring(ring: &[[f64; 3]], p: [f64; 2]) -> bool {
    let n = ring.len();
    (0..n).any(|i| {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        on_segment_within(p, a, b, ON_THE_BOUND)
    })
}

fn on_segment_within(p: [f64; 2], a: [f64; 3], b: [f64; 3], within: f64) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let f = if length > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    (a[0] + f * dx - p[0]).hypot(a[1] + f * dy - p[1]) <= within
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare_surfaces;
    use crate::compare_surfaces::tests::{E, N, level};

    /// A closed square `half` m from (E + 100, N + 100) at `rl`.
    fn square(half: f64, rl: f64) -> Polyline {
        Polyline { vertices: [[-half, -half], [half, -half], [half, half], [-half, half]].map(|[x, y]| [E + 100.0 + x, N + 100.0 + y, rl]).to_vec(), closed: true }
    }

    #[test]
    fn a_toe_and_crest_make_the_batter_and_the_floor() {
        // A 100 m toe at RL 90 and a 120 m crest at RL 100, 45° between: the crest bounds it; every triangle is on the
        // floor or the batter; against a level topography at RL 100, the frustum, 121,333.33 m³, over the crest's 14,400 m².
        let t = triangulate_strings(&[square(50.0, 90.0), square(60.0, 100.0)], LONGEST_HULL_EDGE).unwrap();
        assert_eq!(t.bound, TinBound::Strings(vec![1]));
        assert!((t.area - 14_400.0).abs() < 1e-6, "{}", t.area);
        assert_eq!(t.mesh.vertices.len(), 8);
        for tri in &t.mesh.triangles {
            let [a, b, c] = tri.map(|i| t.mesh.vertices[i as usize]);
            let middle = [(a[0] + b[0] + c[0]) / 3.0 - E - 100.0, (a[1] + b[1] + c[1]) / 3.0 - N - 100.0, (a[2] + b[2] + c[2]) / 3.0];
            let off = middle[0].abs().max(middle[1].abs());
            let expected = if off <= 50.0 { 90.0 } else { 90.0 + (off - 50.0) };
            assert!((middle[2] - expected).abs() < 1e-9, "on the floor or the batter: {middle:?} against {expected}");
        }
        let c = compare_surfaces(&level(400.0, 8, 100.0), &t.mesh, None, 1.0, false).unwrap();
        let frustum = 10.0 / 3.0 * (10_000.0 + 14_400.0 + 12_000.0);
        assert!((c.cut - frustum).abs() / frustum < 1e-6 && c.fill.abs() < 1e-6, "{} {}", c.cut, c.fill);
    }

    #[test]
    fn a_closed_string_alone_is_a_pad() {
        // A polygon drawn at RL 100 alone: a flat pad of its own area, every vertex at RL 100.
        let pad = Polyline { vertices: vec![[E, N, 100.0], [E + 80.0, N, 100.0], [E + 80.0, N + 30.0, 100.0], [E + 40.0, N + 60.0, 100.0], [E, N + 30.0, 100.0]], closed: true };
        let t = triangulate_strings(&[pad], LONGEST_HULL_EDGE).unwrap();
        assert!((t.area - (80.0 * 30.0 + 0.5 * 80.0 * 30.0)).abs() < 1e-6, "{}", t.area);
        assert!(t.mesh.vertices.iter().all(|v| v[2] == 100.0));
        assert_eq!(t.mesh.triangles.len(), 3);
    }

    #[test]
    fn open_strings_make_the_hull_and_long_edges_are_peeled() {
        // Three open lines 100 m long, 20 m apart, then a fourth 300 m off: within 200 m the three make one surface 100
        // by 40 m; the far line is cut off by the long edges along the hull.
        let line = |y: f64, rl: f64| Polyline { vertices: vec![[E, N + y, rl], [E + 100.0, N + y, rl]], closed: false };
        let t = triangulate_strings(&[line(0.0, 100.0), line(20.0, 110.0), line(40.0, 120.0)], LONGEST_HULL_EDGE).unwrap();
        assert!(matches!(t.bound, TinBound::Hull { peeled: 0, .. }));
        assert!((t.area - 4_000.0).abs() < 1e-6, "{}", t.area);
        let far = Polyline { vertices: vec![[E, N + 340.0, 100.0], [E + 100.0, N + 340.0, 100.0]], closed: false };
        let t = triangulate_strings(&[line(0.0, 100.0), line(20.0, 110.0), line(40.0, 120.0), far], LONGEST_HULL_EDGE).unwrap();
        let TinBound::Hull { peeled, .. } = t.bound else { panic!("the hull") };
        assert!(peeled >= 2 && (t.area - 4_000.0).abs() < 1e-6, "{peeled} {}", t.area);
    }

    #[test]
    fn strings_crossing_at_different_heights_are_refused_and_at_one_height_joined() {
        let a = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N + 100.0, 100.0]], closed: false };
        let b = Polyline { vertices: vec![[E, N + 100.0, 80.0], [E + 100.0, N, 100.0]], closed: false };
        let refused = triangulate_strings(&[a.clone(), b], LONGEST_HULL_EDGE).unwrap_err();
        let TinRefusal::Crossings { places, count } = refused else { panic!("crossings: {refused:?}") };
        assert_eq!(count, 1);
        assert_eq!(places[0].strings, [0, 1]);
        assert!((places[0].at[0] - (E + 50.0)).abs() < 1e-9 && (places[0].at[1] - (N + 50.0)).abs() < 1e-9);
        assert!((places[0].heights[0] - 100.0).abs() < 1e-9 && (places[0].heights[1] - 90.0).abs() < 1e-9, "{:?}", places[0].heights);
        // At one height (RL 90 to 110 both ways, crossing at RL 100): joined, the crossing a vertex at RL 100.
        let c = Polyline { vertices: vec![[E, N + 100.0, 90.0], [E + 100.0, N, 110.0]], closed: false };
        let a = Polyline { vertices: vec![[E, N, 90.0], [E + 100.0, N + 100.0, 110.0]], closed: false };
        let t = triangulate_strings(&[a, c], LONGEST_HULL_EDGE).unwrap();
        assert_eq!(t.crossings_joined, 1);
        assert!(t.mesh.vertices.iter().any(|v| (v[0] - E - 50.0).abs() < 1e-9 && (v[1] - N - 50.0).abs() < 1e-9 && (v[2] - 100.0).abs() < 1e-9));
    }

    #[test]
    fn a_string_outside_the_bound_is_refused() {
        let outside = Polyline { vertices: vec![[E + 300.0, N, 100.0], [E + 320.0, N, 100.0]], closed: false };
        assert_eq!(triangulate_strings(&[square(60.0, 100.0), outside], LONGEST_HULL_EDGE).unwrap_err(), TinRefusal::Outside(vec![1]));
    }

    /// Timing, release, made-up data: 150 toes and crests, 75 nested rings of 200 vertices each, 5 m apart in plan and
    /// 2 m in height.
    #[test]
    #[ignore]
    fn timing_triangulate() {
        let rings: Vec<Polyline> = (0..150)
            .map(|k| {
                let r = 100.0 + 5.0 * k as f64;
                Polyline { vertices: (0..200).map(|j| { let a = j as f64 / 200.0 * std::f64::consts::TAU; [E + r * a.cos(), N + r * a.sin(), 50.0 + 2.0 * k as f64] }).collect(), closed: true }
            })
            .collect();
        let t = triangulate_strings(&rings, LONGEST_HULL_EDGE).unwrap();
        println!("150 strings, 30,000 vertices: {} triangles in {:.0} ms", t.mesh.triangles.len(), t.ms);
    }
}
