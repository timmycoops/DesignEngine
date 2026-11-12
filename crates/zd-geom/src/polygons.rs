//! Polygons from lines (decision 0062): every area a set of strings closes in plan, as a closed polygon. It's how
//! blocks are laid out: a line across the strips offset every 100 m, lines drawn to close them, and a polygon made of
//! each closed area, as Deswik and Spry do it.
//!
//! The lines are joined where they cross, in plan. An end that stops short of another line by no more than the
//! tolerance is joined to it, and each join is listed. A stretch that closes nothing (an end running past the last line
//! it crosses, or a line that crosses nothing) is left out; where such a stretch lies inside a polygon, it was meant to
//! split it and doesn't, and its free end is listed with the gap to the nearest line. Nothing is moved: a join adds the
//! short gap as a stretch of its own.

use std::collections::HashMap;

use zd_core::Polyline;

use crate::GeomError;

/// Points closer than this in plan (m) are one point: well under any tolerance a planner sets, well over the
/// rounding of mine grid coordinates.
const SAME_POINT: f64 = 1e-6;

/// A closed area of the lines.
#[derive(Debug, Clone)]
pub struct LinePolygon {
    /// Anticlockwise in plan, the first vertex not repeated at the end. Each vertex has the height of the line it lies on.
    pub vertices: Vec<[f64; 3]>,
    /// Plan area (m²).
    pub area: f64,
    /// Plan centroid.
    pub centroid: [f64; 2],
}

/// An end of a line joined to another line across a gap no wider than the tolerance.
#[derive(Debug, Clone)]
pub struct JoinedEnd {
    /// Which line, by its index in the input.
    pub line: usize,
    pub at: [f64; 3],
    /// The gap closed (m).
    pub gap: f64,
}

/// A free end inside a polygon: its line was meant to split the polygon and stops short.
#[derive(Debug, Clone)]
pub struct ShortLineEnd {
    pub line: usize,
    pub at: [f64; 3],
    /// How far it is from the nearest other line (m).
    pub gap: f64,
    /// The polygon it lies inside, by index in [`LinePolygons::polygons`].
    pub inside: usize,
}

/// What [`polygons_from_lines`] made, with what it joined and what it left out.
#[derive(Debug, Clone, Default)]
pub struct LinePolygons {
    /// Sorted west to east by centroid, then south to north.
    pub polygons: Vec<LinePolygon>,
    pub joined: Vec<JoinedEnd>,
    pub short: Vec<ShortLineEnd>,
    /// Lines that close no area at all, by index.
    pub unused: Vec<usize>,
    /// Free ends outside every polygon, where a line runs on past the last line it crosses, and the length of line left
    /// out outside the polygons (m).
    pub run_past: usize,
    pub run_past_length: f64,
    /// Rings of lines lying inside a polygon without touching its sides, and the polygon each is in: the polygon isn't
    /// split by them, and the ring's own polygons overlap it.
    pub islands: Vec<([f64; 3], usize)>,
    /// Segments in and crossings found.
    pub segments: usize,
    pub crossings: usize,
}

/// One segment of a line, in plan, with its heights.
#[derive(Debug, Clone, Copy)]
struct Segment {
    a: [f64; 3],
    b: [f64; 3],
    line: usize,
}

impl Segment {
    fn min_x(&self) -> f64 {
        self.a[0].min(self.b[0])
    }

    fn max_x(&self) -> f64 {
        self.a[0].max(self.b[0])
    }

    fn at(&self, t: f64) -> [f64; 3] {
        [self.a[0] + t * (self.b[0] - self.a[0]), self.a[1] + t * (self.b[1] - self.a[1]), self.a[2] + t * (self.b[2] - self.a[2])]
    }

    fn length(&self) -> f64 {
        (self.b[0] - self.a[0]).hypot(self.b[1] - self.a[1])
    }

    /// The nearest point to `p` on the segment, as its parameter, and the distance in plan.
    fn nearest(&self, p: [f64; 3]) -> (f64, f64) {
        let (dx, dy) = (self.b[0] - self.a[0], self.b[1] - self.a[1]);
        let length2 = dx * dx + dy * dy;
        let t = if length2 == 0.0 { 0.0 } else { (((p[0] - self.a[0]) * dx + (p[1] - self.a[1]) * dy) / length2).clamp(0.0, 1.0) };
        let q = self.at(t);
        (t, (p[0] - q[0]).hypot(p[1] - q[1]))
    }
}

/// Every closed area the lines make in plan, joining ends that stop short by up to `tolerance` (m).
pub fn polygons_from_lines(lines: &[Polyline], tolerance: f64) -> Result<LinePolygons, GeomError> {
    if !(tolerance.is_finite() && tolerance >= 0.0) {
        return Err(GeomError::InvalidInput(format!("a tolerance of {tolerance} m isn't a distance")));
    }
    let mut out = LinePolygons::default();
    let mut segments = line_segments(lines);
    out.segments = segments.len();
    // Ends that stop short of another line within the tolerance: the gap becomes a stretch of its own.
    for (line, polyline) in lines.iter().enumerate() {
        if polyline.closed || polyline.vertices.len() < 2 {
            continue;
        }
        let n = polyline.vertices.len();
        for (end, next) in [(0, 1), (n - 1, n - 2)] {
            let p = polyline.vertices[end];
            let Some((s, t, gap)) = nearest_other(&segments, line, p, polyline.vertices[next]) else { continue };
            if gap > SAME_POINT && gap <= tolerance {
                let q = segments[s].at(t);
                segments.push(Segment { a: p, b: [q[0], q[1], q[2]], line });
                out.joined.push(JoinedEnd { line, at: p, gap });
            }
        }
    }
    let (points, edges, crossings) = node(&segments);
    out.crossings = crossings;
    // Stretches with a free end close nothing: taken off, end by end, until every point is on a ring.
    let mut live = vec![true; edges.len()];
    let mut degree = vec![0usize; points.len()];
    let mut at_point: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
    for (e, &(u, v, _)) in edges.iter().enumerate() {
        degree[u] += 1;
        degree[v] += 1;
        at_point[u].push(e);
        at_point[v].push(e);
    }
    let mut free: Vec<usize> = (0..points.len()).filter(|&p| degree[p] == 1).collect();
    while let Some(p) = free.pop() {
        if degree[p] != 1 {
            continue;
        }
        let Some(&e) = at_point[p].iter().find(|&&e| live[e]) else { continue };
        live[e] = false;
        let (u, v, _) = edges[e];
        let other = if u == p { v } else { u };
        degree[p] -= 1;
        degree[other] -= 1;
        if degree[other] == 1 {
            free.push(other);
        }
    }
    let rings = walk_faces(&points, &edges, &live);
    let mut outers = Vec::new();
    for ring in rings {
        let area = signed_area(&points, &ring);
        if area > SAME_POINT {
            let vertices: Vec<[f64; 3]> = ring.iter().map(|&k| points[k]).collect();
            let centroid = centroid(&vertices, area);
            out.polygons.push(LinePolygon { vertices, area, centroid });
        } else if area < -SAME_POINT {
            outers.push(ring);
        }
    }
    out.polygons.sort_by(|a, b| a.centroid[0].total_cmp(&b.centroid[0]).then(a.centroid[1].total_cmp(&b.centroid[1])));
    // A ring's outside boundary lying inside another polygon is an island in it.
    for ring in &outers {
        let first = points[ring[0]];
        if let Some(k) = out.polygons.iter().position(|polygon| !ring.iter().any(|&p| polygon.vertices.contains(&points[p])) && inside(&polygon.vertices, first)) {
            out.islands.push((first, k));
        }
    }
    // What was taken off: inside a polygon it's a line stopping short; outside, a stretch running past.
    let within = |p: [f64; 3]| out.polygons.iter().position(|polygon| inside(&polygon.vertices, p) && distance_to_ring(&polygon.vertices, p) > SAME_POINT);
    let mut used = vec![false; lines.len()];
    let mut run_past_length = 0.0;
    for (e, &(u, v, line)) in edges.iter().enumerate() {
        if live[e] {
            used[line] = true;
        } else if within([(points[u][0] + points[v][0]) / 2.0, (points[u][1] + points[v][1]) / 2.0, 0.0]).is_none() {
            run_past_length += (points[v][0] - points[u][0]).hypot(points[v][1] - points[u][1]);
        }
    }
    let ends: Vec<usize> = (0..points.len()).filter(|&p| at_point[p].len() == 1).collect();
    let mut short = Vec::new();
    let mut run_past = 0;
    for p in ends {
        let (_, _, line) = edges[at_point[p][0]];
        match within(points[p]) {
            Some(k) => {
                let gap = segments.iter().filter(|s| s.line != line).map(|s| s.nearest(points[p]).1).fold(f64::INFINITY, f64::min);
                short.push(ShortLineEnd { line, at: points[p], gap, inside: k });
            }
            None => run_past += 1,
        }
    }
    out.short = short;
    out.run_past = run_past;
    out.run_past_length = run_past_length;
    out.unused = (0..lines.len()).filter(|&k| !used[k] && lines[k].vertices.len() >= 2).collect();
    Ok(out)
}

/// The lines as segments, leaving out any with no length in plan.
fn line_segments(lines: &[Polyline]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (line, polyline) in lines.iter().enumerate() {
        let v = &polyline.vertices;
        let n = v.len();
        let count = if polyline.closed && n > 2 { n } else { n.saturating_sub(1) };
        for k in 0..count {
            let segment = Segment { a: v[k], b: v[(k + 1) % n], line };
            if segment.length() > SAME_POINT {
                segments.push(segment);
            }
        }
    }
    segments
}

/// The nearest point on any segment to the end `p`, leaving out the end's own segment (the one towards `next`).
fn nearest_other(segments: &[Segment], line: usize, p: [f64; 3], next: [f64; 3]) -> Option<(usize, f64, f64)> {
    let own = |s: &Segment| s.line == line && ((s.a == p && s.b == next) || (s.b == p && s.a == next));
    segments.iter().enumerate().filter(|(_, s)| !own(s)).map(|(k, s)| {
        let (t, d) = s.nearest(p);
        (k, t, d)
    }).min_by(|a, b| a.2.total_cmp(&b.2))
}

/// An edge between two points, with the line it came from.
type Edge = (usize, usize, usize);

/// The lines cut where they cross or touch: the points, the edges between them and how many crossings there were.
fn node(segments: &[Segment]) -> (Vec<[f64; 3]>, Vec<Edge>, usize) {
    let mut cuts: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; segments.len()];
    let mut order: Vec<usize> = (0..segments.len()).collect();
    order.sort_by(|&a, &b| segments[a].min_x().total_cmp(&segments[b].min_x()));
    let mut crossings = 0;
    for (k, &i) in order.iter().enumerate() {
        let si = segments[i];
        let reach = si.max_x() + SAME_POINT;
        for &j in &order[k + 1..] {
            let sj = segments[j];
            if sj.min_x() > reach {
                break;
            }
            // Neighbours along one line meet at their shared vertex, which is cut there already.
            if si.line == sj.line && (si.b == sj.a || si.a == sj.b) {
                continue;
            }
            if si.a[1].max(si.b[1]) + SAME_POINT < sj.a[1].min(sj.b[1]) || sj.a[1].max(sj.b[1]) + SAME_POINT < si.a[1].min(si.b[1]) {
                continue;
            }
            for (ti, tj) in meet(&si, &sj) {
                cuts[i].push(ti);
                cuts[j].push(tj);
                crossings += 1;
            }
        }
    }
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut point_of = |p: [f64; 3]| -> usize {
        let key = ((p[0] / SAME_POINT).round() as i64, (p[1] / SAME_POINT).round() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(found) = grid.get(&(key.0 + dx, key.1 + dy)) {
                    for &k in found {
                        if (points[k][0] - p[0]).hypot(points[k][1] - p[1]) <= SAME_POINT {
                            return k;
                        }
                    }
                }
            }
        }
        points.push(p);
        grid.entry(key).or_default().push(points.len() - 1);
        points.len() - 1
    };
    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, segment) in segments.iter().enumerate() {
        let mut ts = std::mem::take(&mut cuts[i]);
        ts.sort_by(f64::total_cmp);
        let ids: Vec<usize> = ts.iter().map(|&t| point_of(segment.at(t))).collect();
        for w in ids.windows(2) {
            let (u, v) = (w[0], w[1]);
            if u != v && seen.insert((u.min(v), u.max(v))) {
                edges.push((u, v, segment.line));
            }
        }
    }
    (points, edges, crossings)
}

/// Where two segments meet in plan, as the parameter along each: one point where they cross or touch, or the ends of
/// the stretch they share where they lie along each other.
fn meet(p: &Segment, q: &Segment) -> Vec<(f64, f64)> {
    let r = [p.b[0] - p.a[0], p.b[1] - p.a[1]];
    let s = [q.b[0] - q.a[0], q.b[1] - q.a[1]];
    let w = [q.a[0] - p.a[0], q.a[1] - p.a[1]];
    let (lp, lq) = (p.length(), q.length());
    let cross = r[0] * s[1] - r[1] * s[0];
    // Along each other: within a point's width of one line all the way.
    let off_p = |x: [f64; 3]| ((x[0] - p.a[0]) * r[1] - (x[1] - p.a[1]) * r[0]).abs() / lp;
    if off_p(q.a) <= SAME_POINT && off_p(q.b) <= SAME_POINT {
        let mut out = Vec::new();
        for (x, on_q) in [(q.a, 0.0), (q.b, 1.0)] {
            let (t, d) = p.nearest(x);
            if d <= SAME_POINT {
                out.push((t, on_q));
            }
        }
        for (x, on_p) in [(p.a, 0.0), (p.b, 1.0)] {
            let (u, d) = q.nearest(x);
            if d <= SAME_POINT {
                out.push((on_p, u));
            }
        }
        return out;
    }
    if cross.abs() > 1e-12 * lp * lq {
        let t = (w[0] * s[1] - w[1] * s[0]) / cross;
        let u = (w[0] * r[1] - w[1] * r[0]) / cross;
        let (et, eu) = (SAME_POINT / lp, SAME_POINT / lq);
        if t >= -et && t <= 1.0 + et && u >= -eu && u <= 1.0 + eu {
            return vec![(t.clamp(0.0, 1.0), u.clamp(0.0, 1.0))];
        }
    }
    // Nearly parallel, or just short: an end within a point's width of the other segment still touches it.
    let mut out = Vec::new();
    for (x, on_q) in [(q.a, 0.0), (q.b, 1.0)] {
        let (t, d) = p.nearest(x);
        if d <= SAME_POINT {
            out.push((t, on_q));
        }
    }
    for (x, on_p) in [(p.a, 0.0), (p.b, 1.0)] {
        let (u, d) = q.nearest(x);
        if d <= SAME_POINT {
            out.push((on_p, u));
        }
    }
    out
}

/// Every face of the live edges, each as a ring of points with the face on its left: the closed areas anticlockwise,
/// each ring's outside clockwise.
fn walk_faces(points: &[[f64; 3]], edges: &[Edge], live: &[bool]) -> Vec<Vec<usize>> {
    // Half-edges out of each point, anticlockwise by direction.
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
    let mut half = Vec::new();
    for (e, &(u, v, _)) in edges.iter().enumerate() {
        if live[e] {
            out[u].push(half.len());
            half.push((u, v));
            out[v].push(half.len());
            half.push((v, u));
        }
    }
    let angle = |h: usize| {
        let (u, v) = half[h];
        (points[v][1] - points[u][1]).atan2(points[v][0] - points[u][0])
    };
    for list in &mut out {
        list.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
    }
    let mut position = vec![0usize; half.len()];
    for list in &out {
        for (k, &h) in list.iter().enumerate() {
            position[h] = k;
        }
    }
    let mut done = vec![false; half.len()];
    let mut rings = Vec::new();
    for start in 0..half.len() {
        if done[start] {
            continue;
        }
        let mut ring = Vec::new();
        let mut h = start;
        while !done[h] {
            done[h] = true;
            let (u, v) = half[h];
            ring.push(u);
            // The next edge keeps the face on the left: clockwise round v from the way back.
            let twin = h ^ 1;
            let list = &out[v];
            h = list[(position[twin] + list.len() - 1) % list.len()];
        }
        rings.push(ring);
    }
    rings
}

fn signed_area(points: &[[f64; 3]], ring: &[usize]) -> f64 {
    let n = ring.len();
    let o = points[ring[0]];
    (0..n).map(|k| {
        let (a, b) = (points[ring[k]], points[ring[(k + 1) % n]]);
        (a[0] - o[0]) * (b[1] - o[1]) - (b[0] - o[0]) * (a[1] - o[1])
    }).sum::<f64>() / 2.0
}

fn centroid(vertices: &[[f64; 3]], area: f64) -> [f64; 2] {
    let n = vertices.len();
    let o = vertices[0];
    let (mut cx, mut cy) = (0.0, 0.0);
    for k in 0..n {
        let (a, b) = (vertices[k], vertices[(k + 1) % n]);
        let (ax, ay, bx, by) = (a[0] - o[0], a[1] - o[1], b[0] - o[0], b[1] - o[1]);
        let c = ax * by - bx * ay;
        cx += (ax + bx) * c;
        cy += (ay + by) * c;
    }
    [o[0] + cx / (6.0 * area), o[1] + cy / (6.0 * area)]
}

/// How far `p` is from the ring's sides in plan.
fn distance_to_ring(ring: &[[f64; 3]], p: [f64; 3]) -> f64 {
    let n = ring.len();
    (0..n).map(|k| Segment { a: ring[k], b: ring[(k + 1) % n], line: 0 }.nearest(p).1).fold(f64::INFINITY, f64::min)
}

/// Whether `p` lies inside the ring in plan (crossing count).
fn inside(ring: &[[f64; 3]], p: [f64; 3]) -> bool {
    let n = ring.len();
    let mut within = false;
    for k in 0..n {
        let (a, b) = (ring[k], ring[(k + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            within = !within;
        }
    }
    within
}
