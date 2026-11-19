//! Strings from surfaces (String tools plan, workstream 5; decision 0083): where two surfaces meet, contours, a
//! surface's or solid's outline, and what a section cuts.
//!
//! The lines are exact, as sections are (`slice.rs`): each point lies on one edge of a triangle, worked out from that
//! edge the same way whichever triangle asks, so the pieces of a line meet to the bit and chain into whole strings, open
//! or closed as the line is. Nothing here repairs a surface; what can't be followed is counted and said.

use std::collections::HashMap;

use zd_core::TriMesh;

use crate::GeomError;
use crate::slice::SliceLine;

/// More contour levels than this in one run is refused: a spacing typed in centimetres for metres.
pub const MAX_CONTOUR_LEVELS: usize = 10_000;

/// One contour: its RL and the lines a surface has there.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub rl: f64,
    pub lines: Vec<SliceLine>,
}

/// The RLs of contours every `every` m from `from` to `to`, both included where they fall on the spacing.
pub fn contour_levels(from: f64, to: f64, every: f64) -> Result<Vec<f64>, GeomError> {
    if !(every.is_finite() && every > 0.0) {
        return Err(GeomError::InvalidInput(format!("contours every {every} m isn't a spacing")));
    }
    if !(from.is_finite() && to.is_finite()) || to < from {
        return Err(GeomError::InvalidInput(format!("contours from RL {from} to RL {to}: the second RL has to be the higher")));
    }
    // A hair over, so a top RL on the spacing isn't lost to rounding.
    let count = ((to - from) / every * (1.0 + 1e-12) + 1e-9).floor() as usize + 1;
    if count > MAX_CONTOUR_LEVELS {
        return Err(GeomError::InvalidInput(format!("{count} contours from RL {from} to RL {to} every {every} m is more than {MAX_CONTOUR_LEVELS}: space them further apart")));
    }
    Ok((0..count).map(|k| level(from, every, k)).collect())
}

fn level(from: f64, every: f64, k: usize) -> f64 {
    from + k as f64 * every
}

/// The lines a surface has at each RL every `every` m from `from` to `to`, lowest first; an RL the surface doesn't
/// reach has none. A point is exactly at its RL. Each triangle is looked at once, for the RLs it spans.
pub fn contours(mesh: &TriMesh, from: f64, to: f64, every: f64) -> Result<Vec<Contour>, GeomError> {
    let levels = contour_levels(from, to, every)?;
    let mut nodes: Vec<HashMap<(u32, u32), u32>> = vec![HashMap::new(); levels.len()];
    let mut points: Vec<Vec<[f64; 3]>> = vec![Vec::new(); levels.len()];
    let mut links: Vec<Vec<(u32, u32)>> = vec![Vec::new(); levels.len()];
    let z = |v: u32| mesh.vertices[v as usize][2];
    for corners in &mesh.triangles {
        let heights = corners.map(z);
        let (low, high) = (heights.iter().copied().fold(f64::INFINITY, f64::min), heights.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        if !(low.is_finite() && high.is_finite()) || high < from || low >= to + every {
            continue;
        }
        // The RLs it crosses: a corner at an RL counts as above it, as a corner on a section plane does.
        let first = (((low - from) / every).floor().max(0.0)) as usize;
        let mut k = first;
        while k < levels.len() && levels[k] <= high {
            let rl = levels[k];
            k += 1;
            let sides = heights.map(|h| h >= rl);
            if sides[0] == sides[1] && sides[0] == sides[2] {
                continue;
            }
            let at = k - 1;
            let mut node_on = |u: u32, v: u32| {
                let key = (u.min(v), u.max(v));
                *nodes[at].entry(key).or_insert_with(|| {
                    let (p, q) = (mesh.vertices[key.0 as usize], mesh.vertices[key.1 as usize]);
                    let t = (rl - p[2]) / (q[2] - p[2]);
                    points[at].push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), rl]);
                    points[at].len() as u32 - 1
                })
            };
            let (mut entering, mut leaving) = (0, 0);
            for e in 0..3 {
                let (u, v) = (corners[e], corners[(e + 1) % 3]);
                match (sides[e], sides[(e + 1) % 3]) {
                    (true, false) => leaving = node_on(u, v),
                    (false, true) => entering = node_on(u, v),
                    _ => {}
                }
            }
            links[at].push((entering, leaving));
        }
    }
    Ok(levels
        .iter()
        .enumerate()
        .map(|(k, &rl)| Contour { rl, lines: chain_undirected(&links[k], &points[k]) })
        .collect())
}

/// Where two surfaces meet, and what couldn't be followed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meeting {
    pub lines: Vec<SliceLine>,
    /// Pairs of triangles lying in one plane where they overlap: no line can be drawn there.
    pub coplanar: usize,
    /// Triangle pairs looked at closely, for the timings.
    pub pairs: usize,
}

/// A point where an edge of one surface goes through a triangle of the other: the edge (corners in order) and which
/// surface it's on, and the triangle. Worked out from these alone, so both triangles on the edge give it to the bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Crossing {
    edge_of_b: bool,
    edge: (u32, u32),
    triangle: u32,
}

/// The lines where two surfaces meet, such as a design's crest on the topography: every place a triangle of one goes
/// through a triangle of the other, chained into strings at their own RLs, open or closed as the line is.
pub fn where_surfaces_meet(a: &TriMesh, b: &TriMesh) -> Meeting {
    let index = PlanBins::new(b);
    let mut nodes: HashMap<Crossing, u32> = HashMap::new();
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut links: Vec<(u32, u32)> = Vec::new();
    let mut meeting = Meeting::default();
    let mut seen = vec![u32::MAX; b.triangles.len()];
    for (s, corners_a) in a.triangles.iter().enumerate() {
        let ta = corners_a.map(|v| a.vertices[v as usize]);
        let (lo, hi) = bounds_of(&ta);
        for t in index.near(lo, hi) {
            if seen[t as usize] == s as u32 {
                continue;
            }
            seen[t as usize] = s as u32;
            let corners_b = b.triangles[t as usize];
            let tb = corners_b.map(|v| b.vertices[v as usize]);
            let (lo_b, hi_b) = bounds_of(&tb);
            if (0..3).any(|k| hi_b[k] < lo[k] || lo_b[k] > hi[k]) {
                continue;
            }
            meeting.pairs += 1;
            let (na, nb) = (normal(&ta), normal(&tb));
            // How far each corner lies off the other's plane, the side taken as on or above as the slicing does.
            let da = ta.map(|p| dot(nb, sub(p, tb[0])));
            let db = tb.map(|p| dot(na, sub(p, ta[0])));
            if da.iter().all(|&d| d == 0.0) {
                meeting.coplanar += 1;
                continue;
            }
            if same_side(&da) || same_side(&db) {
                continue;
            }
            let mut found: Vec<u32> = Vec::with_capacity(4);
            let mut add = |key: Crossing, p: Option<[f64; 3]>| {
                if let Some(p) = p {
                    let id = *nodes.entry(key).or_insert_with(|| {
                        points.push(p);
                        points.len() as u32 - 1
                    });
                    if !found.contains(&id) {
                        found.push(id);
                    }
                }
            };
            for e in 0..3 {
                let (u, v) = (corners_a[e], corners_a[(e + 1) % 3]);
                if (da[e] >= 0.0) != (da[(e + 1) % 3] >= 0.0) {
                    let key = Crossing { edge_of_b: false, edge: (u.min(v), u.max(v)), triangle: t };
                    add(key, edge_through(a, key.edge, &tb, nb));
                }
                let (u, v) = (corners_b[e], corners_b[(e + 1) % 3]);
                if (db[e] >= 0.0) != (db[(e + 1) % 3] >= 0.0) {
                    let key = Crossing { edge_of_b: true, edge: (u.min(v), u.max(v)), triangle: s as u32 };
                    add(key, edge_through(b, key.edge, &ta, na));
                }
            }
            // Two in general; more where the line runs through a corner: the two farthest apart are its ends.
            if found.len() >= 2 {
                let (mut i, mut j, mut far) = (0, 1, -1.0);
                for x in 0..found.len() {
                    for y in x + 1..found.len() {
                        let d = dist2(points[found[x] as usize], points[found[y] as usize]);
                        if d > far {
                            (i, j, far) = (x, y, d);
                        }
                    }
                }
                if far > 0.0 {
                    links.push((found[i], found[j]));
                }
            }
        }
    }
    // Where the line runs exactly where an edge of each surface crosses, as it does on two grids on one lattice, the
    // same place comes from two edges: points within 10 nm are one before the line is followed.
    let same = weld(&points, 1e-8);
    let links: Vec<(u32, u32)> = links.into_iter().map(|(p, q)| (same[p as usize], same[q as usize])).filter(|(p, q)| p != q).collect();
    meeting.lines = chain_undirected(&links, &points);
    meeting
}

/// For each point, the first point within `tolerance` of it (itself if none), found through cells `tolerance` wide.
fn weld(points: &[[f64; 3]], tolerance: f64) -> Vec<u32> {
    let cell = |p: [f64; 3]| p.map(|x| (x / tolerance).floor() as i64);
    let mut cells: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut same = Vec::with_capacity(points.len());
    for (k, &p) in points.iter().enumerate() {
        let c = cell(p);
        let mut found = None;
        'look: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for &q in cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]).into_iter().flatten() {
                        if dist2(p, points[q as usize]) <= tolerance * tolerance {
                            found = Some(q);
                            break 'look;
                        }
                    }
                }
            }
        }
        same.push(found.unwrap_or_else(|| {
            cells.entry(c).or_default().push(k as u32);
            k as u32
        }));
    }
    same
}

/// Where the edge `(u, v)` of `mesh` goes through the plane of `triangle` (normal `n`), if that point is in the
/// triangle. Worked out from the edge as given, the lower corner first, so it's the same from either side.
fn edge_through(mesh: &TriMesh, (u, v): (u32, u32), triangle: &[[f64; 3]; 3], n: [f64; 3]) -> Option<[f64; 3]> {
    let (p, q) = (mesh.vertices[u as usize], mesh.vertices[v as usize]);
    let (dp, dq) = (dot(n, sub(p, triangle[0])), dot(n, sub(q, triangle[0])));
    if dp == dq {
        return None;
    }
    let t = dp / (dp - dq);
    let x = [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])];
    inside(triangle, n, x).then_some(x)
}

/// Whether `x`, in the triangle's plane, lies in it, its edges included.
fn inside(t: &[[f64; 3]; 3], n: [f64; 3], x: [f64; 3]) -> bool {
    let scale = dot(n, n);
    (0..3).all(|k| dot(n, cross(sub(t[(k + 1) % 3], t[k]), sub(x, t[k]))) >= -1e-9 * scale)
}

fn same_side(d: &[f64; 3]) -> bool {
    let up = d.map(|x| x >= 0.0);
    up[0] == up[1] && up[0] == up[2]
}

/// A surface's edge: every edge only one triangle has, chained into strings at their own RLs, each hole its own. A
/// surface with no edge (closed all round) gives none.
pub fn surface_outline(mesh: &TriMesh) -> Vec<SliceLine> {
    let mut uses: HashMap<(u32, u32), u32> = HashMap::new();
    for t in &mesh.triangles {
        for e in 0..3 {
            let (u, v) = (t[e], t[(e + 1) % 3]);
            if u != v {
                *uses.entry((u.min(v), u.max(v))).or_default() += 1;
            }
        }
    }
    let mut edges: Vec<(u32, u32)> = uses.into_iter().filter(|&(_, n)| n == 1).map(|(e, _)| e).collect();
    edges.sort_unstable();
    chain_undirected(&edges, &mesh.vertices)
}

/// A solid's outline in plan: its shadow looking down, each hole through it its own string, laid at the solid's lowest
/// RL. Through Manifold's projection, the solid moved near the origin for it so mine coordinates keep their digits.
pub fn solid_outline(mesh: &TriMesh) -> Result<Vec<SliceLine>, GeomError> {
    let bounds = mesh.bounds().ok_or_else(|| GeomError::InvalidInput("the solid has no vertices".into()))?;
    let offset = [-bounds.min[0], -bounds.min[1], -bounds.min[2]];
    let solid = crate::cut::to_manifold(mesh, offset, "the solid")?;
    let rl = bounds.min[2];
    Ok(solid
        .project()
        .into_iter()
        .filter(|ring| ring.len() >= 3)
        .map(|ring| SliceLine { points: ring.iter().map(|p| [p[0] - offset[0], p[1] - offset[1], rl]).collect(), closed: true })
        .collect())
}

/// Chains links between points into lines, whichever way each runs: open lines from their ends first, then the loops
/// left. Where more than two links meet at a point, the walk takes them in turn, so every link is in one line.
pub(crate) fn chain_undirected(links: &[(u32, u32)], points: &[[f64; 3]]) -> Vec<SliceLine> {
    let mut at: HashMap<u32, Vec<u32>> = HashMap::new();
    for (k, &(p, q)) in links.iter().enumerate() {
        if p != q {
            at.entry(p).or_default().push(k as u32);
            at.entry(q).or_default().push(k as u32);
        }
    }
    let mut used = vec![false; links.len()];
    let mut ends: Vec<u32> = at.iter().filter(|(_, ks)| ks.len() % 2 == 1).map(|(&p, _)| p).collect();
    ends.sort_unstable();
    let mut all: Vec<u32> = at.keys().copied().collect();
    all.sort_unstable();
    let mut lines = Vec::new();
    // Two points within a nanometre are one: a line through a corner, or along an edge both surfaces have, gives the
    // same place from two edges.
    let same = |p: [f64; 3], q: [f64; 3]| dist2(p, q) <= 1e-18;
    let walk_from = |start: u32, used: &mut Vec<bool>| -> Option<SliceLine> {
        let mut line = vec![points[start as usize]];
        let mut here = start;
        loop {
            let next = at.get(&here).and_then(|ks| ks.iter().copied().find(|&k| !used[k as usize]));
            let Some(k) = next else { break };
            used[k as usize] = true;
            let (p, q) = links[k as usize];
            here = if p == here { q } else { p };
            if here == start {
                if line.len() >= 2 && same(line[line.len() - 1], line[0]) {
                    line.pop();
                }
                return (line.len() >= 3).then_some(SliceLine { points: line, closed: true });
            }
            let point = points[here as usize];
            if !same(line[line.len() - 1], point) {
                line.push(point);
            }
        }
        (line.len() >= 2).then_some(SliceLine { points: line, closed: false })
    };
    for &start in ends.iter().chain(&all) {
        while at.get(&start).is_some_and(|ks| ks.iter().any(|&k| !used[k as usize])) {
            lines.extend(walk_from(start, &mut used));
        }
    }
    lines
}

/// Triangles binned in plan, to find those near a box quickly.
struct PlanBins {
    lo: [f64; 2],
    size: [f64; 2],
    side: usize,
    starts: Vec<u32>,
    triangles: Vec<u32>,
}

impl PlanBins {
    fn new(mesh: &TriMesh) -> Self {
        let Some(bounds) = mesh.bounds() else { return PlanBins { lo: [0.0; 2], size: [1.0; 2], side: 1, starts: vec![0, 0], triangles: Vec::new() } };
        let side = ((mesh.triangles.len() as f64 / 8.0).sqrt().ceil() as usize).clamp(1, 4096);
        let lo = [bounds.min[0], bounds.min[1]];
        let size = [((bounds.max[0] - lo[0]) / side as f64).max(1e-9), ((bounds.max[1] - lo[1]) / side as f64).max(1e-9)];
        let mut bins = PlanBins { lo, size, side, starts: vec![0; side * side + 1], triangles: Vec::new() };
        let cells_of = |t: &[u32; 3], bins: &PlanBins| {
            let (l, h) = bounds_of(&t.map(|v| mesh.vertices[v as usize]));
            bins.cells(l, h)
        };
        for t in &mesh.triangles {
            for c in cells_of(t, &bins) {
                bins.starts[c + 1] += 1;
            }
        }
        for c in 0..side * side {
            bins.starts[c + 1] += bins.starts[c];
        }
        let mut next = bins.starts.clone();
        bins.triangles = vec![0; bins.starts[side * side] as usize];
        for (k, t) in mesh.triangles.iter().enumerate() {
            for c in cells_of(t, &bins) {
                bins.triangles[next[c] as usize] = k as u32;
                next[c] += 1;
            }
        }
        bins
    }

    fn cells(&self, lo: [f64; 3], hi: [f64; 3]) -> impl Iterator<Item = usize> + use<> {
        let cell = |x: f64, axis: usize| (((x - self.lo[axis]) / self.size[axis]).floor().max(0.0) as usize).min(self.side - 1);
        let (x0, x1, y0, y1) = (cell(lo[0], 0), cell(hi[0], 0), cell(lo[1], 1), cell(hi[1], 1));
        let side = self.side;
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| y * side + x))
    }

    fn near(&self, lo: [f64; 3], hi: [f64; 3]) -> impl Iterator<Item = u32> + '_ {
        self.cells(lo, hi).flat_map(|c| self.triangles[self.starts[c] as usize..self.starts[c + 1] as usize].iter().copied())
    }
}

fn bounds_of(t: &[[f64; 3]; 3]) -> ([f64; 3], [f64; 3]) {
    let mut lo = t[0];
    let mut hi = t[0];
    for p in &t[1..] {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normal(t: &[[f64; 3]; 3]) -> [f64; 3] {
    cross(sub(t[1], t[0]), sub(t[2], t[0]))
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

/// A line's length in plan.
pub fn plan_length(line: &SliceLine) -> f64 {
    let n = line.points.len();
    let segments = if line.closed { n } else { n.saturating_sub(1) };
    (0..segments).map(|k| {
        let (p, q) = (line.points[k], line.points[(k + 1) % n]);
        (q[0] - p[0]).hypot(q[1] - p[1])
    }).sum()
}

/// A closed line's area in plan, by the shoelace, taken from its first point so mine coordinates keep their digits.
pub fn plan_area(line: &SliceLine) -> f64 {
    let o = line.points[0];
    let n = line.points.len();
    (0..n).map(|k| {
        let (p, q) = (line.points[k], line.points[(k + 1) % n]);
        (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1])
    }).sum::<f64>().abs() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A square plane `size` m a side from (E, N), its RL at each corner from `rl`, in a grid of `cells` × `cells`.
    fn plane(size: f64, cells: usize, rl: impl Fn(f64, f64) -> f64) -> TriMesh {
        let mut vertices = Vec::new();
        for j in 0..=cells {
            for i in 0..=cells {
                let (x, y) = (i as f64 * size / cells as f64, j as f64 * size / cells as f64);
                vertices.push([E + x, N + y, rl(x, y)]);
            }
        }
        let at = |i: usize, j: usize| (j * (cells + 1) + i) as u32;
        let mut triangles = Vec::new();
        for j in 0..cells {
            for i in 0..cells {
                triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        TriMesh { vertices, triangles }
    }

    /// Contours every 5 m on a plane rising 10 % east, from RL 105 to 145: nine straight strings 50 m apart in plan,
    /// each at its RL exactly, running the plane's full 200 m north.
    #[test]
    fn contours_every_5_m_on_a_plane_rising_10_percent_east_are_50_m_apart() {
        let mesh = plane(500.0, 25, |x, _| 100.0 + 0.1 * x);
        let lines = contours(&mesh, 105.0, 145.0, 5.0).unwrap();
        assert_eq!(lines.len(), 9);
        for (k, c) in lines.iter().enumerate() {
            assert_eq!(c.rl, 105.0 + 5.0 * k as f64);
            assert_eq!(c.lines.len(), 1, "one string at RL {}", c.rl);
            let line = &c.lines[0];
            assert!(!line.closed);
            let x = 50.0 * (k + 1) as f64;
            for p in &line.points {
                assert_eq!(p[2], c.rl, "exactly at its RL");
                assert!((p[0] - (E + x)).abs() < 1e-9, "{} m east, not {}", x, p[0] - E);
            }
            assert!((plan_length(line) - 500.0).abs() < 1e-9);
        }
    }

    /// An RL the surface doesn't reach has none; a spacing that gives too many is refused.
    #[test]
    fn contours_outside_the_surface_are_empty_and_too_many_are_refused() {
        let mesh = plane(100.0, 4, |x, _| 100.0 + 0.1 * x);
        let lines = contours(&mesh, 50.0, 60.0, 5.0).unwrap();
        assert!(lines.iter().all(|c| c.lines.is_empty()));
        assert!(contours(&mesh, 0.0, 1000.0, 0.01).is_err());
        assert!(contours(&mesh, 110.0, 100.0, 1.0).is_err());
        assert_eq!(contour_levels(100.0, 110.0, 5.0).unwrap(), [100.0, 105.0, 110.0], "the top RL on the spacing kept");
    }

    /// A level plane at RL 100 and a plane at 45° rising east from RL 70 at x = 0: they meet in one straight string
    /// 30 m east, at RL 100, the full 200 m north.
    #[test]
    fn a_level_plane_meeting_a_plane_at_45_degrees_is_one_straight_string() {
        let level = plane(200.0, 7, |_, _| 100.0);
        let slope = plane(200.0, 9, |x, _| 70.0 + x);
        let met = where_surfaces_meet(&level, &slope);
        assert_eq!(met.lines.len(), 1, "{:?}", met.lines.iter().map(|l| l.points.len()).collect::<Vec<_>>());
        let line = &met.lines[0];
        assert!(!line.closed);
        for p in &line.points {
            assert!((p[0] - (E + 30.0)).abs() < 1e-9 && (p[2] - 100.0).abs() < 1e-9, "{:?}", [p[0] - E, p[1] - N, p[2]]);
        }
        assert!((plan_length(line) - 200.0).abs() < 1e-9, "{}", plan_length(line));
        assert_eq!(met.coplanar, 0);
        // Either way round, the same line.
        let back = where_surfaces_meet(&slope, &level);
        assert_eq!(back.lines.len(), 1);
        assert!((plan_length(&back.lines[0]) - 200.0).abs() < 1e-9);
    }

    /// The same two planes on one lattice, as a design and a topography gridded alike: every place the line crosses an
    /// edge, both surfaces have that edge. Still one straight string, 200 m long, at RL 100.
    #[test]
    fn planes_on_one_lattice_still_meet_in_one_string() {
        for cells in [8, 40] {
            let level = plane(200.0, cells, |_, _| 100.0);
            let slope = plane(200.0, cells, |x, _| 70.0 + x);
            let met = where_surfaces_meet(&level, &slope);
            assert_eq!(met.lines.len(), 1, "{cells} cells: {} pieces", met.lines.len());
            assert!((plan_length(&met.lines[0]) - 200.0).abs() < 1e-9);
        }
    }

    /// A rolling topography and a wavy design gridded alike, at two sizes: every string where they meet is closed or runs
    /// to the surfaces' edge, none ends inside, and the finer grid finds the same 9 (2 closed) as the coarser.
    #[test]
    fn rolling_surfaces_on_one_lattice_meet_in_whole_strings() {
        let on_edge = |p: [f64; 3]| [p[0] - E, p[1] - N].iter().any(|&c| c.abs() < 1e-6 || (c - 2000.0).abs() < 1e-6);
        for cells in [20, 80] {
            let topo = plane(2000.0, cells, |x, y| 100.0 + 20.0 * (x / 150.0).sin() * (y / 210.0).cos() + 0.01 * x);
            let design = plane(2000.0, cells, |x, y| 95.0 + 0.012 * x + 8.0 * (y / 300.0).sin());
            let met = where_surfaces_meet(&design, &topo);
            assert!(met.lines.iter().all(|l| l.closed || (on_edge(l.points[0]) && on_edge(l.points[l.points.len() - 1]))), "{cells} cells: a string ends inside");
            assert_eq!((met.lines.len(), met.lines.iter().filter(|l| l.closed).count()), (9, 2), "{cells} cells");
        }
    }

    /// A 72-sided cone at 45°, its point at RL 150, meeting a level plane at RL 100: one closed string, every vertex on
    /// the 72-sided ring 50 m out (those on the cone's edges exactly 50 m from the axis), its length the ring's.
    #[test]
    fn a_cone_meeting_a_plane_is_one_closed_ring() {
        let (cx, cy, sides) = (E + 100.0, N + 100.0, 72usize);
        let mut vertices = vec![[cx, cy, 150.0]];
        for k in 0..sides {
            let a = std::f64::consts::TAU * k as f64 / sides as f64;
            vertices.push([cx + 80.0 * a.cos(), cy + 80.0 * a.sin(), 70.0]);
        }
        let triangles = (0..sides as u32).map(|k| [0, 1 + k, 1 + (k + 1) % sides as u32]).collect();
        let cone = TriMesh { vertices, triangles };
        let level = plane(200.0, 3, |_, _| 100.0);
        let met = where_surfaces_meet(&cone, &level);
        assert_eq!(met.lines.len(), 1);
        let ring = &met.lines[0];
        assert!(ring.closed);
        let chord = 2.0 * 50.0 * (std::f64::consts::PI / sides as f64).sin();
        let apothem = 50.0 * (std::f64::consts::PI / sides as f64).cos();
        let mut on_edges = 0;
        for p in &ring.points {
            assert!((p[2] - 100.0).abs() < 1e-9);
            let r = (p[0] - cx).hypot(p[1] - cy);
            assert!(r <= 50.0 + 1e-9 && r >= apothem - 1e-9, "on the ring: {r}");
            if (r - 50.0).abs() < 1e-9 {
                on_edges += 1;
            }
        }
        assert_eq!(on_edges, sides, "a vertex on each of the cone's edges, 50 m out");
        assert!((plan_length(ring) - sides as f64 * chord).abs() < 1e-6, "{} against {}", plan_length(ring), sides as f64 * chord);
    }

    /// Timings on made-up data, as the String tools plan asks (decision 0083): contours every metre on a rolling
    /// topography, and where a design meets it, at 0.1 M and 1.0 M triangles a surface. Counts and timings only. Run
    /// with `cargo test --release -p zd-geom timing_strings_from_surfaces -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_strings_from_surfaces() {
        let sizes: Vec<usize> = std::env::var("ZD_TIMING_CELLS").ok().map_or(vec![224, 707], |v| v.split(",").map(|c| c.parse().unwrap()).collect());
        for cells in sizes {
            let topo = plane(2000.0, cells, |x, y| 100.0 + 20.0 * (x / 150.0).sin() * (y / 210.0).cos() + 0.01 * x);
            let design = plane(2000.0, cells, |x, y| 95.0 + 0.012 * x + 8.0 * (y / 300.0).sin());
            let started = std::time::Instant::now();
            let made = contours(&topo, 80.0, 140.0, 1.0).unwrap();
            let lines: usize = made.iter().map(|c| c.lines.len()).sum();
            println!("contours every 1 m: {} triangles in, {:.0} ms, {lines} strings out", topo.triangles.len(), started.elapsed().as_secs_f64() * 1000.0);
            let started = std::time::Instant::now();
            let met = where_surfaces_meet(&design, &topo);
            let on_edge = |p: [f64; 3]| [p[0] - E, p[1] - N].iter().any(|&c| c.abs() < 1e-6 || (c - 2000.0).abs() < 1e-6);
            let loose = met.lines.iter().filter(|l| !l.closed && !(on_edge(l.points[0]) && on_edge(l.points[l.points.len() - 1]))).count();
            println!("  closed {}, open to the edge {}, open ending inside {loose}", met.lines.iter().filter(|l| l.closed).count(), met.lines.len() - loose - met.lines.iter().filter(|l| l.closed).count());
            println!(
                "where surfaces meet: {} + {} triangles in, {:.0} ms, {} strings out ({} pairs looked at)",
                design.triangles.len(),
                topo.triangles.len(),
                started.elapsed().as_secs_f64() * 1000.0,
                met.lines.len(),
                met.pairs
            );
        }
    }

    /// A 40 m block with a 10 m square shaft through it, at mine coordinates from RL 60: its outline in plan is two
    /// closed strings, 1,600 m² and 100 m², at RL 60.
    #[test]
    fn a_solid_with_a_hole_through_it_outlines_in_plan() {
        use manifold_csg::Manifold;
        let block = Manifold::cube(40.0, 40.0, 30.0, false).difference(&Manifold::cube(10.0, 10.0, 50.0, false).translate(15.0, 15.0, -10.0));
        let (vertices, props, triangles) = block.to_mesh_f64();
        let mesh = TriMesh {
            vertices: vertices.chunks(props).map(|v| [v[0] + E, v[1] + N, v[2] + 60.0]).collect(),
            triangles: triangles.chunks(3).map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect(),
        };
        let mut lines = solid_outline(&mesh).unwrap();
        lines.sort_by(|a, b| plan_area(b).total_cmp(&plan_area(a)));
        assert_eq!(lines.len(), 2);
        assert!((plan_area(&lines[0]) - 1_600.0).abs() < 1e-6 && (plan_area(&lines[1]) - 100.0).abs() < 1e-6, "{} {}", plan_area(&lines[0]), plan_area(&lines[1]));
        assert!(lines.iter().flat_map(|l| &l.points).all(|p| p[2] == 60.0 && p[0] >= E - 1e-9 && p[1] >= N - 1e-9));
    }

    /// The outline of a 100 m square surface with a 20 m square hole in its middle: two closed strings, 10,000 m² and
    /// 400 m² in plan, at the surface's RLs.
    #[test]
    fn a_square_with_a_hole_outlines_as_two_closed_strings() {
        let full = plane(100.0, 5, |x, y| 100.0 + 0.01 * x + 0.02 * y);
        // The 20 m cell in the middle (cells of 20 m, the third along and up) taken out.
        let middle = [2 * 5 + 2, 2 * 5 + 2].map(|c| c * 2);
        let triangles = full.triangles.iter().enumerate().filter(|(k, _)| *k != middle[0] && *k != middle[0] + 1).map(|(_, t)| *t).collect();
        let holed = TriMesh { vertices: full.vertices.clone(), triangles };
        let mut lines = surface_outline(&holed);
        lines.sort_by(|a, b| plan_area(b).total_cmp(&plan_area(a)));
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.closed));
        assert!((plan_area(&lines[0]) - 10_000.0).abs() < 1e-6 && (plan_area(&lines[1]) - 400.0).abs() < 1e-6, "{} {}", plan_area(&lines[0]), plan_area(&lines[1]));
        for p in lines.iter().flat_map(|l| &l.points) {
            assert_eq!(p[2], 100.0 + 0.01 * (p[0] - E) + 0.02 * (p[1] - N), "the surface's own RL");
        }
    }
}
