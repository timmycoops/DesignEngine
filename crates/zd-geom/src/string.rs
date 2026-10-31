//! String operations (decision 0008): set a grade, offset in plan, drape onto a surface.
//!
//! Offsets use Manifold's 2D booleans (Clipper2 underneath): each segment sweeps a band of the
//! offset width on the chosen side, each outside corner adds a mitre or round piece, and their union
//! is taken. The edge of that union lying at the offset distance from the string is the new string.
//! Tight inside corners and strings that curl back on themselves are resolved by the union, never by
//! trimming crossed lines afterwards; open and closed strings go the same way.

use manifold_csg::{CrossSection, FillRule};
use zd_core::{Polyline, TriMesh};

use crate::GeomError;

/// Which side of the string, looking along its direction from above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetSide {
    Left,
    Right,
}

/// How outside corners are joined.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Corner {
    /// Sharp corners, bevelled where the mitre would reach farther than `limit` times the distance.
    Mitre { limit: f64 },
    Round,
}

/// Points on a full circle for round corners.
const ROUND_SEGMENTS: f64 = 64.0;

/// Elevations along a string at a grade: vertex `from` takes `start_rl`, and each vertex from there
/// to `to` (either direction along the string) rises `grade_percent` per 100 m of plan length.
/// Vertices outside that range keep their elevations.
pub fn grade_string(line: &Polyline, from: usize, to: usize, start_rl: f64, grade_percent: f64) -> Result<Polyline, GeomError> {
    let n = line.vertices.len();
    if from >= n || to >= n {
        return Err(GeomError::InvalidInput(format!("vertices {from} to {to} are not all in a {n}-vertex string")));
    }
    if !start_rl.is_finite() || !grade_percent.is_finite() {
        return Err(GeomError::InvalidInput("the start RL and grade must be finite".into()));
    }
    let mut out = line.clone();
    out.vertices[from][2] = start_rl;
    let path: Vec<usize> = if to >= from { (from..=to).collect() } else { (to..=from).rev().collect() };
    for pair in path.windows(2) {
        let (a, b) = (out.vertices[pair[0]], out.vertices[pair[1]]);
        out.vertices[pair[1]][2] = a[2] + grade_percent / 100.0 * (b[0] - a[0]).hypot(b[1] - a[1]);
    }
    Ok(out)
}

/// The string offset `distance` metres in plan to one side. Each new vertex takes the elevation of
/// the nearest point on the source string, or `rl` when given. An offset can come back in several
/// pieces (an inside offset that pinches off), and is empty-handed only if nothing is left, which is
/// refused.
pub fn offset_string(line: &Polyline, distance: f64, side: OffsetSide, corner: Corner, rl: Option<f64>) -> Result<Vec<Polyline>, GeomError> {
    if !(distance.is_finite() && distance > 0.0) {
        return Err(GeomError::InvalidInput("the offset distance must be more than zero".into()));
    }
    if let Corner::Mitre { limit } = corner
        && !(limit.is_finite() && limit >= 1.0)
    {
        return Err(GeomError::InvalidInput("the mitre limit must be at least 1".into()));
    }
    let Some(&first) = line.vertices.first() else { return Err(GeomError::InvalidInput("the string has no vertices".into())) };
    // Plan coordinates about a whole-metre origin, so Clipper's fixed precision (1e-8) holds at mine
    // grid eastings and northings.
    let origin = [first[0].round(), first[1].round()];
    let (mut plan, mut zs): (Vec<[f64; 2]>, Vec<f64>) = (Vec::with_capacity(line.vertices.len()), Vec::with_capacity(line.vertices.len()));
    for v in &line.vertices {
        let p = [v[0] - origin[0], v[1] - origin[1]];
        if plan.last() != Some(&p) {
            plan.push(p);
            zs.push(v[2]);
        }
    }
    if line.closed && plan.len() > 1 && plan.first() == plan.last() {
        plan.pop();
        zs.pop();
    }
    let n = plan.len();
    if n < if line.closed { 3 } else { 2 } {
        return Err(GeomError::InvalidInput("the string needs at least two distinct vertices (three if closed)".into()));
    }
    let segments = if line.closed { n } else { n - 1 };
    let sign = if side == OffsetSide::Left { 1.0 } else { -1.0 };
    let dir = |i: usize| unit(sub(plan[(i + 1) % n], plan[i]));
    let normal = |i: usize| {
        let [dx, dy] = dir(i);
        [-dy * sign, dx * sign]
    };

    let mut pieces: Vec<Vec<[f64; 2]>> = Vec::new();
    for i in 0..segments {
        let (a, b, m) = (plan[i], plan[(i + 1) % n], scale(normal(i), distance));
        pieces.push(ccw(vec![a, b, add(b, m), add(a, m)]));
    }
    let corners: Vec<usize> = if line.closed { (0..n).collect() } else { (1..n - 1).collect() };
    for i in corners {
        let prev = (i + segments - 1) % segments;
        // Only corners that turn away from the offset side need filling.
        if sign * cross(dir(prev), dir(i)) >= 0.0 {
            continue;
        }
        let (p, n0, n1) = (plan[i], normal(prev), normal(i));
        let (a, b) = (add(p, scale(n0, distance)), add(p, scale(n1, distance)));
        let piece = match corner {
            Corner::Mitre { limit } => {
                let c = dot(n0, n1);
                let reach = (2.0 / (1.0 + c)).sqrt();
                if reach <= limit {
                    vec![p, a, add(p, scale(add(n0, n1), distance / (1.0 + c))), b]
                } else {
                    vec![p, a, b]
                }
            }
            Corner::Round => {
                let angle = dot(n0, n1).clamp(-1.0, 1.0).acos();
                let turn = if cross(n0, n1) >= 0.0 { 1.0 } else { -1.0 };
                let steps = (angle / (std::f64::consts::TAU / ROUND_SEGMENTS)).ceil().max(1.0) as usize;
                std::iter::once(p)
                    .chain((0..=steps).map(|k| add(p, scale(rotate(n0, turn * angle * k as f64 / steps as f64), distance))))
                    .collect()
            }
        };
        pieces.push(ccw(piece));
    }
    let union = CrossSection::from_polygons_with_fill_rule(&pieces, FillRule::Positive);

    // Keep the stretches of the union's boundary that lie at the offset distance.
    let near = |p: [f64; 2]| nearest_on(&plan, line.closed, p);
    let tolerance = 1e-6 + distance * 1e-9;
    let at_distance = |p: [f64; 2]| near(p).0 >= distance - tolerance;
    let source_ccw = signed_area(&plan) > 0.0;
    let mut out = Vec::new();
    for ring in union.to_polygons() {
        let kept: Vec<bool> = ring.iter().map(|&p| at_distance(p)).collect();
        let mut runs: Vec<(Vec<[f64; 2]>, bool)> = Vec::new();
        match kept.iter().position(|k| !k) {
            None => runs.push((ring.clone(), true)),
            Some(start) => {
                let mut run = Vec::new();
                for k in 1..=ring.len() {
                    let i = (start + k) % ring.len();
                    if kept[i] {
                        run.push(ring[i]);
                    } else if !run.is_empty() {
                        runs.push((std::mem::take(&mut run), false));
                    }
                }
            }
        }
        for (points, closed) in runs {
            // Where a band meets a corner piece the union keeps a vertex in the middle of a straight
            // run; those carry nothing.
            let mut points = drop_collinear(points, closed);
            if points.len() < 2 || (closed && points.len() < 3) {
                continue;
            }
            // Run the new string the same way as the source.
            let backwards = if closed {
                line.closed && (signed_area(&points) > 0.0) != source_ccw
            } else {
                near(points[0]).1 > near(*points.last().expect("two or more points")).1
            };
            if backwards {
                points.reverse();
            }
            let vertices = points
                .iter()
                .map(|&p| {
                    let z = rl.unwrap_or_else(|| {
                        let along = near(p).1;
                        let i = (along.floor() as usize).min(segments - 1);
                        let t = along - i as f64;
                        zs[i] + t * (zs[(i + 1) % n] - zs[i])
                    });
                    [p[0] + origin[0], p[1] + origin[1], z]
                })
                .collect();
            out.push(Polyline { vertices, closed });
        }
    }
    if out.is_empty() {
        return Err(GeomError::InvalidInput(format!("nothing is left {distance} m to that side of the string")));
    }
    Ok(out)
}

/// A string split at vertex `at`. An open string splits into two that share that vertex (`at`
/// must be an inner vertex); a closed string opens there instead, starting and ending on it.
pub fn split_string(line: &Polyline, at: usize) -> Result<(Polyline, Option<Polyline>), GeomError> {
    let n = line.vertices.len();
    if line.closed {
        if at >= n {
            return Err(GeomError::InvalidInput(format!("vertex {at} is not in a {n}-vertex string")));
        }
        let mut vertices: Vec<[f64; 3]> = line.vertices[at..].iter().chain(&line.vertices[..at]).copied().collect();
        vertices.push(line.vertices[at]);
        return Ok((Polyline { vertices, closed: false }, None));
    }
    if at == 0 || at + 1 >= n {
        return Err(GeomError::InvalidInput("an open string splits at an inner vertex, not at an end".into()));
    }
    let first = Polyline { vertices: line.vertices[..=at].to_vec(), closed: false };
    let second = Polyline { vertices: line.vertices[at..].to_vec(), closed: false };
    Ok((first, Some(second)))
}

/// Two open strings joined at their nearest ends (in plan), keeping `a`'s direction: `b` is added
/// after `a`'s end or before its start, reversed if need be. Ends that meet exactly share a vertex;
/// otherwise a segment bridges the gap, whose 3D length comes back with the joined string.
pub fn join_strings(a: &Polyline, b: &Polyline) -> Result<(Polyline, f64), GeomError> {
    if a.closed || b.closed {
        return Err(GeomError::InvalidInput("only open strings can be joined".into()));
    }
    let (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) = (a.vertices.first(), a.vertices.last(), b.vertices.first(), b.vertices.last()) else {
        return Err(GeomError::InvalidInput("a string to join has no vertices".into()));
    };
    let plan = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).hypot(p[1] - q[1]);
    // (a's end used, b's end used): after a's end take b forwards from its start, and so on.
    let options = [(true, true, plan(a1, b0)), (true, false, plan(a1, b1)), (false, false, plan(a0, b1)), (false, true, plan(a0, b0))];
    let &(after, from_start, _) = options.iter().min_by(|x, y| x.2.total_cmp(&y.2)).expect("four options");
    let mut tail: Vec<[f64; 3]> = b.vertices.clone();
    let mut vertices = a.vertices.clone();
    let (meet, gap) = if after {
        if !from_start {
            tail.reverse();
        }
        let gap = distance3(a1, tail[0]);
        (gap == 0.0, gap)
    } else {
        if from_start {
            tail.reverse();
        }
        let gap = distance3(*tail.last().expect("vertices"), a0);
        (gap == 0.0, gap)
    };
    if after {
        vertices.extend(tail.into_iter().skip(usize::from(meet)));
    } else {
        let keep = tail.len() - usize::from(meet);
        tail.truncate(keep);
        tail.extend(vertices);
        vertices = tail;
    }
    Ok((Polyline { vertices, closed: false }, gap))
}

/// A smooth curve through every vertex (a centripetal Catmull-Rom spline, which neither loops nor
/// cusps), with vertices added so no step is longer than `spacing` in plan. The original vertices
/// stay exactly where they are; elevations follow the same curve.
pub fn smooth_string(line: &Polyline, spacing: f64) -> Result<Polyline, GeomError> {
    if !(spacing.is_finite() && spacing > 0.0) {
        return Err(GeomError::InvalidInput("the smoothing spacing must be more than zero".into()));
    }
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(line.vertices.len());
    for &v in &line.vertices {
        if points.last() != Some(&v) {
            points.push(v);
        }
    }
    if line.closed && points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let n = points.len();
    if n < if line.closed { 3 } else { 2 } {
        return Err(GeomError::InvalidInput("the string needs at least two distinct vertices (three if closed)".into()));
    }
    // Neighbours beyond the ends of an open string are reflections, so the curve leaves each end
    // heading along its end segment.
    let at = |i: isize| -> [f64; 3] {
        if line.closed {
            points[i.rem_euclid(n as isize) as usize]
        } else if i < 0 {
            let (p, q) = (points[0], points[1]);
            [2.0 * p[0] - q[0], 2.0 * p[1] - q[1], 2.0 * p[2] - q[2]]
        } else if i as usize >= n {
            let (p, q) = (points[n - 1], points[n - 2]);
            [2.0 * p[0] - q[0], 2.0 * p[1] - q[1], 2.0 * p[2] - q[2]]
        } else {
            points[i as usize]
        }
    };
    let spans = if line.closed { n } else { n - 1 };
    let mut vertices = Vec::new();
    for i in 0..spans {
        let i = i as isize;
        let [p0, p1, p2, p3] = [at(i - 1), at(i), at(i + 1), at(i + 2)];
        let steps = ((p2[0] - p1[0]).hypot(p2[1] - p1[1]) / spacing).ceil().max(1.0) as usize;
        vertices.push(p1);
        for k in 1..steps {
            vertices.push(catmull_rom(p0, p1, p2, p3, k as f64 / steps as f64));
        }
    }
    if !line.closed {
        vertices.push(points[n - 1]);
    }
    Ok(Polyline { vertices, closed: line.closed })
}

/// The centripetal Catmull-Rom curve between `p1` and `p2` at fraction `u`, by the Barry-Goldman
/// construction with knots spaced by the square root of each chord length.
fn catmull_rom(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3], p3: [f64; 3], u: f64) -> [f64; 3] {
    let knot = |a: [f64; 3], b: [f64; 3]| distance3(a, b).sqrt().max(1e-12);
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let t = t1 + u * (t2 - t1);
    let mix = |a: [f64; 3], b: [f64; 3], ta: f64, tb: f64| -> [f64; 3] {
        let (wa, wb) = ((tb - t) / (tb - ta), (t - ta) / (tb - ta));
        [wa * a[0] + wb * b[0], wa * a[1] + wb * b[1], wa * a[2] + wb * b[2]]
    };
    let (a1, a2, a3) = (mix(p0, p1, t0, t1), mix(p1, p2, t1, t2), mix(p2, p3, t2, t3));
    let (b1, b2) = (mix(a1, a2, t0, t2), mix(a2, a3, t1, t3));
    mix(b1, b2, t1, t2)
}

/// Vertices that lie within `tolerance` metres (in 3D, so grades keep) of the line through the
/// vertices kept either side are removed (Douglas-Peucker). The ends of an open string stay; a closed
/// string keeps its first vertex and the one farthest from it.
pub fn filter_string(line: &Polyline, tolerance: f64) -> Result<Polyline, GeomError> {
    if !(tolerance.is_finite() && tolerance >= 0.0) {
        return Err(GeomError::InvalidInput("the filter tolerance must be zero or more".into()));
    }
    let v = &line.vertices;
    let n = v.len();
    if n < 3 {
        return Ok(line.clone());
    }
    let mut keep = vec![false; n];
    if line.closed {
        let far = (1..n).max_by(|&i, &j| distance3(v[0], v[i]).total_cmp(&distance3(v[0], v[j]))).expect("three or more vertices");
        keep[0] = true;
        keep[far] = true;
        douglas_peucker(v, 0, far, tolerance, &mut keep);
        // The second half runs from the far vertex back round to the first.
        let ring: Vec<[f64; 3]> = v[far..].iter().chain(std::iter::once(&v[0])).copied().collect();
        let mut ring_keep = vec![false; ring.len()];
        douglas_peucker(&ring, 0, ring.len() - 1, tolerance, &mut ring_keep);
        for (k, kept) in ring_keep.iter().enumerate().take(ring.len() - 1) {
            keep[far + k] |= *kept;
        }
    } else {
        keep[0] = true;
        keep[n - 1] = true;
        douglas_peucker(v, 0, n - 1, tolerance, &mut keep);
    }
    let vertices: Vec<[f64; 3]> = v.iter().zip(&keep).filter(|(_, k)| **k).map(|(p, _)| *p).collect();
    if line.closed && vertices.len() < 3 {
        return Ok(line.clone());
    }
    Ok(Polyline { vertices, closed: line.closed })
}

/// Marks the vertex between `first` and `last` farthest from their chord if it is beyond
/// `tolerance`, and recurses either side of it. Iterative, so long strings cannot overflow the stack.
fn douglas_peucker(v: &[[f64; 3]], first: usize, last: usize, tolerance: f64, keep: &mut [bool]) {
    let mut stack = vec![(first, last)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (worst, d) = (a + 1..b).map(|i| (i, distance_to_segment3(v[i], v[a], v[b]))).max_by(|x, y| x.1.total_cmp(&y.1)).expect("inner vertices");
        if d > tolerance {
            keep[worst] = true;
            stack.push((a, worst));
            stack.push((worst, b));
        }
    }
}

fn distance3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn distance_to_segment3(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let len = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = if len > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len).clamp(0.0, 1.0) } else { 0.0 };
    distance3(p, [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]])
}

/// What draping did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrapeLog {
    /// Vertices with no surface under them; they keep their elevations.
    pub off_surface: usize,
    /// Vertices added where the string crosses the surface's triangle edges.
    pub crossings_added: usize,
}

/// The string laid on a surface: each vertex takes the surface's elevation under it (the highest,
/// where the surface overhangs). With `add_crossings`, a vertex is also added wherever a segment
/// crosses a triangle edge in plan, so the string follows the surface exactly between its vertices.
pub fn drape_string(line: &Polyline, surface: &TriMesh, add_crossings: bool) -> Result<(Polyline, DrapeLog), GeomError> {
    let Some(&first) = line.vertices.first() else { return Err(GeomError::InvalidInput("the string has no vertices".into())) };
    let mut lo = [first[0], first[1]];
    let mut hi = lo;
    for v in &line.vertices {
        lo = [lo[0].min(v[0]), lo[1].min(v[1])];
        hi = [hi[0].max(v[0]), hi[1].max(v[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let mut log = DrapeLog::default();
    let mut vertices = Vec::with_capacity(line.vertices.len());
    let segments = if line.closed { line.vertices.len() } else { line.vertices.len().saturating_sub(1) };
    let drape = |v: [f64; 3], log: &mut DrapeLog| match bins.height(surface, [v[0], v[1]]) {
        Some(z) => [v[0], v[1], z],
        None => {
            log.off_surface += 1;
            v
        }
    };
    for (i, &v) in line.vertices.iter().enumerate() {
        vertices.push(drape(v, &mut log));
        if add_crossings && i < segments {
            let w = line.vertices[(i + 1) % line.vertices.len()];
            for p in bins.crossings(surface, [v[0], v[1]], [w[0], w[1]]) {
                vertices.push(p);
                log.crossings_added += 1;
            }
        }
    }
    Ok((Polyline { vertices, closed: line.closed }, log))
}

/// Triangles of a surface binned in plan over the area a string covers.
pub(crate) struct TriangleBins {
    lo: [f64; 2],
    cell: [f64; 2],
    dims: [usize; 2],
    bins: Vec<Vec<u32>>,
}

impl TriangleBins {
    pub(crate) fn new(surface: &TriMesh, lo: [f64; 2], hi: [f64; 2]) -> Self {
        let plan = |t: &[u32; 3]| t.map(|i| surface.vertices[i as usize]);
        let overlaps = |t: &[[f64; 3]; 3]| {
            t.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min) <= hi[0]
                && t.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max) >= lo[0]
                && t.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min) <= hi[1]
                && t.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max) >= lo[1]
        };
        let candidates: Vec<u32> = (0..surface.triangles.len() as u32).filter(|&t| overlaps(&plan(&surface.triangles[t as usize]))).collect();
        let side = ((candidates.len() as f64).sqrt().ceil() as usize).clamp(1, 1024);
        let span = [(hi[0] - lo[0]).max(1e-9), (hi[1] - lo[1]).max(1e-9)];
        let cell = [span[0] / side as f64, span[1] / side as f64];
        let mut out = TriangleBins { lo, cell, dims: [side, side], bins: vec![Vec::new(); side * side] };
        for t in candidates {
            let p = plan(&surface.triangles[t as usize]);
            let (c0, r0) = out.bin_of([p.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min), p.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min)]);
            let (c1, r1) = out.bin_of([p.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max), p.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max)]);
            for r in r0..=r1 {
                for c in c0..=c1 {
                    out.bins[r * side + c].push(t);
                }
            }
        }
        out
    }

    fn bin_of(&self, p: [f64; 2]) -> (usize, usize) {
        let along = |a: usize| (((p[a] - self.lo[a]) / self.cell[a]).floor().max(0.0) as usize).min(self.dims[a] - 1);
        (along(0), along(1))
    }

    /// The triangles that may hold the plan point `x`, `y`.
    pub(crate) fn at(&self, x: f64, y: f64) -> &[u32] {
        let (c, r) = self.bin_of([x, y]);
        &self.bins[r * self.dims[0] + c]
    }

    /// The highest elevation of the surface at `p`, if any triangle covers it.
    pub(crate) fn height(&self, surface: &TriMesh, p: [f64; 2]) -> Option<f64> {
        let (c, r) = self.bin_of(p);
        self.bins[r * self.dims[0] + c]
            .iter()
            .filter_map(|&t| {
                let [a, b, d] = surface.triangles[t as usize].map(|i| surface.vertices[i as usize]);
                let area = (b[0] - a[0]) * (d[1] - a[1]) - (d[0] - a[0]) * (b[1] - a[1]);
                if area.abs() < 1e-12 {
                    return None;
                }
                let u = ((b[0] - p[0]) * (d[1] - p[1]) - (d[0] - p[0]) * (b[1] - p[1])) / area;
                let v = ((d[0] - p[0]) * (a[1] - p[1]) - (a[0] - p[0]) * (d[1] - p[1])) / area;
                let w = 1.0 - u - v;
                let eps = -1e-9;
                (u >= eps && v >= eps && w >= eps).then(|| u * a[2] + v * b[2] + w * d[2])
            })
            .reduce(f64::max)
    }

    /// Where the plan segment `a`-`b` crosses triangle edges, strictly between its ends, in order from
    /// `a`, each at the edge's elevation there.
    pub(crate) fn crossings(&self, surface: &TriMesh, a: [f64; 2], b: [f64; 2]) -> Vec<[f64; 3]> {
        let (c0, r0) = self.bin_of([a[0].min(b[0]), a[1].min(b[1])]);
        let (c1, r1) = self.bin_of([a[0].max(b[0]), a[1].max(b[1])]);
        let mut seen = std::collections::HashSet::new();
        let mut hits: Vec<(f64, f64)> = Vec::new();
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &t in &self.bins[r * self.dims[0] + c] {
                    if !seen.insert(t) {
                        continue;
                    }
                    let tri = surface.triangles[t as usize];
                    for k in 0..3 {
                        let (p, q) = (surface.vertices[tri[k] as usize], surface.vertices[tri[(k + 1) % 3] as usize]);
                        if let Some((s, e)) = segment_crossing(a, b, [p[0], p[1]], [q[0], q[1]]) {
                            hits.push((s, p[2] + e * (q[2] - p[2])));
                        }
                    }
                }
            }
        }
        hits.sort_by(|x, y| x.0.total_cmp(&y.0));
        hits.dedup_by(|x, y| (x.0 - y.0).abs() < 1e-9);
        hits.into_iter().map(|(s, z)| [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1]), z]).collect()
    }
}

/// Where segment `a`-`b` crosses segment `p`-`q`: the fraction along each, strictly inside `a`-`b`.
pub(crate) fn segment_crossing(a: [f64; 2], b: [f64; 2], p: [f64; 2], q: [f64; 2]) -> Option<(f64, f64)> {
    let (r, s) = (sub(b, a), sub(q, p));
    let denom = cross(r, s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let ap = sub(p, a);
    let t = cross(ap, s) / denom;
    let u = cross(ap, r) / denom;
    let eps = 1e-12;
    (t > eps && t < 1.0 - eps && (-eps..=1.0 + eps).contains(&u)).then_some((t, u.clamp(0.0, 1.0)))
}

/// Distance in plan from `p` to the nearest point of the string, and where that point is: the
/// segment index plus the fraction along it.
fn nearest_on(plan: &[[f64; 2]], closed: bool, p: [f64; 2]) -> (f64, f64) {
    let n = plan.len();
    let segments = if closed { n } else { n - 1 };
    (0..segments)
        .map(|i| {
            let (a, b) = (plan[i], plan[(i + 1) % n]);
            let ab = sub(b, a);
            let t = (dot(sub(p, a), ab) / dot(ab, ab)).clamp(0.0, 1.0);
            let d = sub(p, add(a, scale(ab, t)));
            (dot(d, d).sqrt(), i as f64 + t)
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .expect("a string has a segment")
}

/// Drops vertices that sit on the straight line between their neighbours (within a micrometre),
/// heading on in the same direction. Ends of an open run stay.
fn drop_collinear(points: Vec<[f64; 2]>, closed: bool) -> Vec<[f64; 2]> {
    let straight = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        let ac = sub(c, a);
        let span = dot(ac, ac).sqrt();
        span > 0.0 && cross(ac, sub(b, a)).abs() / span <= 1e-6 && dot(sub(b, a), sub(c, b)) > 0.0
    };
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for &p in &points {
        while out.len() >= 2 && straight(out[out.len() - 2], out[out.len() - 1], p) {
            out.pop();
        }
        out.push(p);
    }
    if closed {
        while out.len() > 3 && straight(out[out.len() - 2], out[out.len() - 1], out[0]) {
            out.pop();
        }
        while out.len() > 3 && straight(out[out.len() - 1], out[0], out[1]) {
            out.remove(0);
        }
    }
    out
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: [f64; 2], s: f64) -> [f64; 2] {
    [a[0] * s, a[1] * s]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub(crate) fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn unit(a: [f64; 2]) -> [f64; 2] {
    scale(a, 1.0 / dot(a, a).sqrt())
}

fn rotate(a: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    [a[0] * c - a[1] * s, a[0] * s + a[1] * c]
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| cross(ring[i], ring[(i + 1) % n])).sum::<f64>() / 2.0
}

fn ccw(mut ring: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    ring
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    const E: f64 = 600_000.0;
    const N: f64 = 7_500_000.0;

    fn line(points: &[[f64; 3]], closed: bool) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [E + p[0], N + p[1], p[2]]).collect(), closed }
    }

    /// Plan area of a closed string, about the mine grid origin used here.
    fn area(l: &Polyline) -> f64 {
        let ring: Vec<[f64; 2]> = l.vertices.iter().map(|v| [v[0] - E, v[1] - N]).collect();
        signed_area(&ring).abs()
    }

    /// Plan coordinates about the origin, to the micrometre.
    fn local(l: &Polyline) -> Vec<[f64; 2]> {
        l.vertices.iter().map(|v| [((v[0] - E) * 1e6).round() / 1e6, ((v[1] - N) * 1e6).round() / 1e6]).collect()
    }

    const MITRE: Corner = Corner::Mitre { limit: 4.0 };

    /// A 100 m square, anticlockwise from above.
    fn square() -> Polyline {
        line(&[[0.0, 0.0, 50.0], [100.0, 0.0, 50.0], [100.0, 100.0, 50.0], [0.0, 100.0, 50.0]], true)
    }

    #[test]
    fn a_grade_rises_by_plan_length() {
        // Plan lengths 50 m (a 3-4-5 triangle) and 60 m.
        let l = line(&[[0.0, 0.0, 0.0], [30.0, 40.0, 0.0], [30.0, 100.0, 0.0]], false);
        let down = grade_string(&l, 0, 2, 100.0, -5.0).unwrap();
        assert_eq!(down.vertices.iter().map(|v| v[2]).collect::<Vec<_>>(), [100.0, 97.5, 94.5]);
        // Backwards from the far end, a 10 % climb: 60 m gives 6 m, 50 m gives 5 m more.
        let up = grade_string(&l, 2, 0, 200.0, 10.0).unwrap();
        assert_eq!(up.vertices.iter().map(|v| v[2]).collect::<Vec<_>>(), [211.0, 206.0, 200.0]);
        // A part range leaves the rest alone.
        let part = grade_string(&l, 1, 2, 10.0, 10.0).unwrap();
        assert_eq!(part.vertices.iter().map(|v| v[2]).collect::<Vec<_>>(), [0.0, 10.0, 16.0]);
        assert!(grade_string(&l, 0, 3, 0.0, 1.0).is_err());
    }

    #[test]
    fn a_square_offset_out_and_in_has_the_exact_area() {
        let out = offset_string(&square(), 10.0, OffsetSide::Right, MITRE, None).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].closed);
        // Right of an anticlockwise square is outside: a 120 m square, to Clipper's 1e-8 m.
        assert!((area(&out[0]) - 14_400.0).abs() < 1e-4, "{}", area(&out[0]));
        let inside = offset_string(&square(), 10.0, OffsetSide::Left, MITRE, None).unwrap();
        assert!((area(&inside[0]) - 6_400.0).abs() < 1e-4);
        assert!(inside[0].vertices.iter().all(|v| v[2] == 50.0), "elevations from the source");
        // Round corners add four quarter circles: 64 chords to the circle, so within 1 m2 of pi * 100.
        let round = offset_string(&square(), 10.0, OffsetSide::Right, Corner::Round, None).unwrap();
        let expected = 10_000.0 + 4.0 * 100.0 * 10.0 + std::f64::consts::PI * 100.0;
        assert!((area(&round[0]) - expected).abs() < 1.0, "{} vs {expected}", area(&round[0]));
    }

    #[test]
    fn sides_follow_the_strings_direction() {
        // The same square clockwise: now left is outside.
        let mut cw = square();
        cw.vertices.reverse();
        let out = offset_string(&cw, 10.0, OffsetSide::Left, MITRE, None).unwrap();
        assert!((area(&out[0]) - 14_400.0).abs() < 1e-4);
        let ring: Vec<[f64; 2]> = out[0].vertices.iter().map(|v| [v[0] - E, v[1] - N]).collect();
        assert!(signed_area(&ring) < 0.0, "runs clockwise like its source");
    }

    #[test]
    fn an_open_l_offsets_to_either_side_with_square_ends() {
        // East 100 m then north 100 m: a left turn.
        let l = line(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]], false);
        let inside = offset_string(&l, 10.0, OffsetSide::Left, MITRE, None).unwrap();
        assert_eq!(inside.len(), 1);
        assert!(!inside[0].closed);
        assert_eq!(local(&inside[0]), [[0.0, 10.0], [90.0, 10.0], [90.0, 100.0]]);
        let outside = offset_string(&l, 10.0, OffsetSide::Right, MITRE, None).unwrap();
        assert_eq!(local(&outside[0]), [[0.0, -10.0], [110.0, -10.0], [110.0, 100.0]]);
        // Rounded outside: starts and ends square to the string, and every vertex is 10 m off it.
        let round = offset_string(&l, 10.0, OffsetSide::Right, Corner::Round, None).unwrap();
        let pts = local(&round[0]);
        assert_eq!((pts[0], *pts.last().unwrap()), ([0.0, -10.0], [110.0, 100.0]));
        let plan: Vec<[f64; 2]> = l.vertices.iter().map(|v| [v[0] - E, v[1] - N]).collect();
        assert!(pts.iter().all(|&p| (nearest_on(&plan, false, p).0 - 10.0).abs() < 1e-6));
    }

    #[test]
    fn a_sharp_corner_beyond_the_mitre_limit_is_bevelled() {
        // East 100 m, then back at 170 degrees: the mitre would reach 1 / sin(5 deg), about 11.5 x.
        let back = [100.0 + 100.0 * 170f64.to_radians().cos(), 100.0 * 170f64.to_radians().sin()];
        let l = line(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [back[0], back[1], 0.0]], false);
        // How far the offset reaches from the corner, among its points within 20 m of it.
        let reach = |o: &Polyline| local(o).iter().map(|p| (p[0] - 100.0).hypot(p[1])).filter(|&d| d < 20.0).fold(0.0, f64::max);
        let bevelled = offset_string(&l, 1.0, OffsetSide::Right, MITRE, None).unwrap();
        assert!(reach(&bevelled[0]) < 1.0 + 1e-6, "no spike: {}", reach(&bevelled[0]));
        let sharp = offset_string(&l, 1.0, OffsetSide::Right, Corner::Mitre { limit: 20.0 }, None).unwrap();
        assert!((reach(&sharp[0]) - 1.0 / 5f64.to_radians().sin()).abs() < 1e-4, "{}", reach(&sharp[0]));
    }

    #[test]
    fn a_u_narrower_than_twice_the_distance_has_no_inside_offset() {
        // Down, east 15 m, up: the inside is on the left.
        let narrow = line(&[[0.0, 100.0, 0.0], [0.0, 0.0, 0.0], [15.0, 0.0, 0.0], [15.0, 100.0, 0.0]], false);
        assert!(offset_string(&narrow, 10.0, OffsetSide::Left, MITRE, None).is_err());
        let wide = line(&[[0.0, 100.0, 0.0], [0.0, 0.0, 0.0], [50.0, 0.0, 0.0], [50.0, 100.0, 0.0]], false);
        let inside = offset_string(&wide, 10.0, OffsetSide::Left, MITRE, None).unwrap();
        assert_eq!(local(&inside[0]), [[10.0, 100.0], [10.0, 10.0], [40.0, 10.0], [40.0, 100.0]]);
    }

    #[test]
    fn an_offset_of_a_graded_string_keeps_its_grade() {
        let flat = line(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]], false);
        let l = grade_string(&flat, 0, 2, 100.0, 10.0).unwrap();
        let out = offset_string(&l, 10.0, OffsetSide::Left, MITRE, None).unwrap();
        // (0, 10) sits beside the start (RL 100), (90, 10) beside (90, 0) on the first leg (RL 109),
        // (90, 100) beside the end (RL 120).
        let z: Vec<f64> = out[0].vertices.iter().map(|v| (v[2] * 1e6).round() / 1e6).collect();
        assert_eq!(z, [100.0, 109.0, 120.0]);
        let fixed = offset_string(&l, 10.0, OffsetSide::Left, MITRE, Some(85.0)).unwrap();
        assert!(fixed[0].vertices.iter().all(|v| v[2] == 85.0));
    }

    #[test]
    fn bad_offsets_are_refused() {
        assert!(offset_string(&square(), 0.0, OffsetSide::Left, MITRE, None).is_err());
        assert!(offset_string(&square(), 1.0, OffsetSide::Left, Corner::Mitre { limit: 0.5 }, None).is_err());
        let point = line(&[[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]], false);
        assert!(offset_string(&point, 1.0, OffsetSide::Left, MITRE, None).is_err());
    }

    /// The plane z = 5 + 0.1x + 0.2y on a 10 m grid over 0..100 m, triangulated.
    fn plane() -> TriMesh {
        triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 11, 11, |x, y| 5.0 + 0.1 * (x - E) + 0.2 * (y - N)))
    }

    #[test]
    fn a_draped_string_lies_on_the_surface() {
        let surface = plane();
        let z = |v: &[f64; 3]| 5.0 + 0.1 * (v[0] - E) + 0.2 * (v[1] - N);
        let l = line(&[[5.0, 5.0, 0.0], [95.0, 55.0, 0.0], [150.0, 5.0, 7.0]], false);
        let (plain, log) = drape_string(&l, &surface, false).unwrap();
        assert_eq!(log, DrapeLog { off_surface: 1, crossings_added: 0 });
        assert!(plain.vertices[..2].iter().all(|v| (v[2] - z(v)).abs() < 1e-9));
        assert_eq!(plain.vertices[2][2], 7.0, "off the surface: left as it was");

        let (dense, log) = drape_string(&l, &surface, true).unwrap();
        // From (5, 5) to (95, 55) the string crosses 9 vertical and 5 horizontal grid lines plus the
        // cell diagonals it passes; every vertex on the surface lies on the plane, in order.
        assert!(log.crossings_added > 14, "{log:?}");
        let on_surface: Vec<&[f64; 3]> = dense.vertices.iter().filter(|v| v[0] - E <= 100.0 && v[1] - N <= 100.0).collect();
        assert!(on_surface.iter().all(|v| (v[2] - z(v)).abs() < 1e-9));
        assert!(on_surface.windows(2).all(|w| w[1][0] >= w[0][0]), "in order from the start");
    }

    #[test]
    fn an_open_string_splits_into_two_sharing_the_vertex() {
        let l = line(&[[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [20.0, 0.0, 3.0], [30.0, 0.0, 4.0]], false);
        let (first, second) = split_string(&l, 2).unwrap();
        assert_eq!(first.vertices, l.vertices[..3]);
        assert_eq!(second.unwrap().vertices, l.vertices[2..]);
        assert!(split_string(&l, 0).is_err() && split_string(&l, 3).is_err(), "not at an end");
    }

    #[test]
    fn a_closed_string_opens_where_it_is_split() {
        let (open, rest) = split_string(&square(), 2).unwrap();
        assert!(rest.is_none() && !open.closed);
        assert_eq!(local(&open), [[100.0, 100.0], [0.0, 100.0], [0.0, 0.0], [100.0, 0.0], [100.0, 100.0]]);
    }

    #[test]
    fn strings_join_at_their_nearest_ends_in_the_first_ones_direction() {
        let a = line(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0]], false);
        // b runs back towards a's end: it is reversed and added after it, sharing the meeting vertex.
        let b = line(&[[30.0, 0.0, 0.0], [20.0, 0.0, 0.0], [10.0, 0.0, 0.0]], false);
        let (joined, gap) = join_strings(&a, &b).unwrap();
        assert_eq!(gap, 0.0);
        assert_eq!(local(&joined), [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]]);
        // c lies 3-4-5 before a's start: added in front, with the gap bridged and reported.
        let c = line(&[[-20.0, 0.0, 0.0], [-3.0, -4.0, 0.0]], false);
        let (joined, gap) = join_strings(&a, &c).unwrap();
        assert_eq!(gap, 5.0);
        assert_eq!(local(&joined), [[-20.0, 0.0], [-3.0, -4.0], [0.0, 0.0], [10.0, 0.0]]);
        assert!(join_strings(&a, &square()).is_err(), "closed strings do not join");
    }

    #[test]
    fn smoothing_passes_through_every_vertex_and_keeps_straights_straight() {
        let l = line(&[[0.0, 0.0, 0.0], [100.0, 0.0, 10.0], [200.0, 0.0, 20.0]], false);
        let s = smooth_string(&l, 10.0).unwrap();
        assert_eq!(s.vertices.len(), 21, "a vertex every 10 m");
        assert_eq!((s.vertices[0], s.vertices[10], s.vertices[20]), (l.vertices[0], l.vertices[1], l.vertices[2]));
        // Collinear in 3D, so the curve stays on the line and keeps its 10 % grade.
        assert!(s.vertices.iter().all(|v| (v[1] - N).abs() < 1e-6 && (v[2] - 0.1 * (v[0] - E)).abs() < 1e-6));
        let steps = s.vertices.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).fold(0.0, f64::max);
        assert!(steps <= 10.0 + 1e-6);
    }

    #[test]
    fn a_smoothed_square_rounds_its_corners_and_stays_closed() {
        let s = smooth_string(&square(), 5.0).unwrap();
        assert!(s.closed);
        assert_eq!(s.vertices.len(), 80, "four 100 m sides at 5 m");
        for corner in &square().vertices {
            assert!(s.vertices.contains(corner), "keeps {corner:?}");
        }
        // Bulges outward between corners, but stays inside the circle through them (pi x 5,000 m2).
        let circle = std::f64::consts::PI * 5_000.0;
        assert!(area(&s) > 10_000.0 && area(&s) < circle, "{}", area(&s));
        let centre = |v: &[f64; 3]| (v[0] - E - 50.0).hypot(v[1] - N - 50.0);
        assert!(s.vertices.iter().all(|v| centre(v) <= 50.0 * 2f64.sqrt() + 1e-6));
        assert!(smooth_string(&square(), 0.0).is_err());
    }

    #[test]
    fn filtering_drops_vertices_on_the_line_and_keeps_the_grade() {
        // On a straight grade in 3D, every inner vertex goes.
        let straight = line(&(0..=10).map(|i| [i as f64 * 10.0, 0.0, i as f64]).collect::<Vec<_>>(), false);
        let f = filter_string(&straight, 1e-6).unwrap();
        assert_eq!(f.vertices, [straight.vertices[0], straight.vertices[10]]);
        // A 1 m zigzag survives a 0.5 m tolerance and goes at 2 m.
        let zig = line(&(0..=10).map(|i| [i as f64 * 10.0, if i % 2 == 1 { 1.0 } else { 0.0 }, 0.0]).collect::<Vec<_>>(), false);
        assert_eq!(filter_string(&zig, 0.5).unwrap().vertices.len(), 11);
        assert_eq!(filter_string(&zig, 2.0).unwrap().vertices.len(), 2);
        // A step in elevation alone is kept: the filter works in 3D.
        let step = line(&[[0.0, 0.0, 0.0], [50.0, 0.0, 5.0], [100.0, 0.0, 0.0]], false);
        assert_eq!(filter_string(&step, 1.0).unwrap().vertices.len(), 3);
    }

    #[test]
    fn a_closed_filter_keeps_the_corners() {
        // The square with a vertex in the middle of each side: the mid vertices go, the corners stay.
        let mut dense = Vec::new();
        for w in [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]].windows(2).chain(std::iter::once(&[[0.0, 100.0], [0.0, 0.0]][..])) {
            dense.push([w[0][0], w[0][1], 50.0]);
            dense.push([(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0, 50.0]);
        }
        let f = filter_string(&line(&dense, true), 1e-6).unwrap();
        assert!(f.closed);
        assert_eq!(local(&f), [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]);
    }
}
