//! Cutting one string by another (String tools plan, workstream 1): trim a string back to a cutting string, keeping the
//! side clicked or both; extend a string on to one; split a string at a point clicked on it. Every place a string is
//! cut or run on takes the string's own height there: interpolated along the segment it falls on, or carried on at
//! the last segment's grade. The cutting string's heights are never used.

use zd_core::Polyline;

use crate::GeomError;
use crate::string::ON_THE_STRING;

/// Cuts closer together along a string than this are the one cut, as where a cutting string passes through a vertex
/// and so crosses both segments that meet there (m).
const SAME_CUT: f64 = 1e-6;

/// A place along a string closer to a vertex than this is that vertex (m).
const AT_A_VERTEX: f64 = 1e-9;

/// A point clicked this close to a vertex splits the string there, not a hair beside it (m).
const SPLIT_AT_VERTEX: f64 = 1e-3;

/// What a trim keeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Keep {
    /// The parts of the string on the same side of the cutting string as this point in plan: inside or outside a
    /// closed cutting string, left or right of an open one.
    Side([f64; 2]),
    /// Every part: the string is split at each place the cutting string crosses it.
    Both,
}

/// A string trimmed: the parts kept, in the order the string runs, how many places it was cut, and how many parts were
/// dropped. A string the cutting string doesn't cross comes back whole with no cuts.
#[derive(Debug, Clone, PartialEq)]
pub struct Trimmed {
    pub pieces: Vec<Polyline>,
    pub cuts: usize,
    pub dropped: usize,
}

/// A place along a string as its segment index plus the fraction along it: vertex `k` is at `k`.
type Along = f64;

fn plan(p: [f64; 3]) -> [f64; 2] {
    [p[0], p[1]]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// How many segments a string has: a closed one's last runs back to its first vertex.
fn segments(line: &Polyline) -> usize {
    if line.closed { line.vertices.len() } else { line.vertices.len().saturating_sub(1) }
}

fn vertex(line: &Polyline, k: usize) -> [f64; 3] {
    line.vertices[k % line.vertices.len()]
}

/// The point at `u` along a string, its height interpolated along the segment it falls on.
fn point_at(line: &Polyline, u: Along) -> [f64; 3] {
    let k = u.floor() as usize;
    let t = u - k as f64;
    if t == 0.0 {
        return vertex(line, k);
    }
    let (a, b) = (vertex(line, k), vertex(line, k + 1));
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]
}

/// The part of a string from `from` to `to` along it (`to` past the last segment wraps round a closed string).
fn part(line: &Polyline, from: Along, to: Along) -> Polyline {
    let mut vertices = vec![point_at(line, from)];
    let mut j = from.floor() as usize + 1;
    while (j as f64) < to {
        vertices.push(vertex(line, j));
        j += 1;
    }
    vertices.push(point_at(line, to));
    Polyline { vertices, closed: false }
}

/// Where along `line` the open or closed `cutter` crosses it in plan, in order, each place once. A crossing within
/// [`AT_A_VERTEX`] of a vertex is put on the vertex, so the vertex is kept exactly.
fn cuts_along(line: &Polyline, cutter: &Polyline) -> Vec<(Along, f64)> {
    let mut found: Vec<(Along, f64)> = Vec::new();
    let mut walked = 0.0;
    for k in 0..segments(line) {
        let (a, b) = (plan(vertex(line, k)), plan(vertex(line, k + 1)));
        let span = length(sub(b, a));
        for c in 0..segments(cutter) {
            let (p, q) = (plan(vertex(cutter, c)), plan(vertex(cutter, c + 1)));
            let Some(t) = segment_crossing([a, b], [p, q]) else { continue };
            let (u, along) = if t * span < AT_A_VERTEX {
                (k as f64, walked)
            } else if (1.0 - t) * span < AT_A_VERTEX {
                ((k + 1) as f64, walked + span)
            } else {
                (k as f64 + t, walked + t * span)
            };
            found.push((u, along));
        }
        walked += span;
    }
    found.sort_by(|x, y| x.1.total_cmp(&y.1));
    found.dedup_by(|later, earlier| later.1 - earlier.1 < SAME_CUT);
    found
}

/// Where along `p`, from 0 to 1, it crosses `q`, ends included. Segments that run together don't cross.
fn segment_crossing(p: [[f64; 2]; 2], q: [[f64; 2]; 2]) -> Option<f64> {
    let r = sub(p[1], p[0]);
    let s = sub(q[1], q[0]);
    let denominator = cross(r, s);
    if denominator.abs() < 1e-12 * (length(r) * length(s)).max(f64::MIN_POSITIVE) {
        return None;
    }
    let w = sub(q[0], p[0]);
    let t = cross(w, s) / denominator;
    let u = cross(w, r) / denominator;
    let eps = 1e-12;
    ((-eps..=1.0 + eps).contains(&t) && (-eps..=1.0 + eps).contains(&u)).then_some(t.clamp(0.0, 1.0))
}

/// Which side of the cutting string a point in plan is on: inside a closed one is +1 and outside -1; left of an open
/// one, looking along it, is +1 and right -1. On it is 0.
fn side_of(cutter: &Polyline, p: [f64; 2]) -> i8 {
    if cutter.closed {
        return if on_the_line(cutter, p) {
            0
        } else if inside(cutter, p) {
            1
        } else {
            -1
        };
    }
    // The nearest place on the line, and the way it faces there: a segment's left, or at a vertex the two segments'
    // lefts together, so a point off a corner is on the side it looks to be on.
    let mut best: Option<(f64, [f64; 2], [f64; 2])> = None;
    let n = cutter.vertices.len();
    for k in 0..segments(cutter) {
        let (a, b) = (plan(cutter.vertices[k]), plan(cutter.vertices[k + 1]));
        let ab = sub(b, a);
        let span = dot(ab, ab);
        if span == 0.0 {
            continue;
        }
        let t = (dot(sub(p, a), ab) / span).clamp(0.0, 1.0);
        let q = [a[0] + t * ab[0], a[1] + t * ab[1]];
        let d = length(sub(p, q));
        let left = |from: usize| {
            let (x, y) = (plan(cutter.vertices[from]), plan(cutter.vertices[from + 1]));
            let v = sub(y, x);
            let l = length(v);
            if l == 0.0 { [0.0, 0.0] } else { [-v[1] / l, v[0] / l] }
        };
        let facing = if t == 0.0 && k > 0 {
            let (l0, l1) = (left(k - 1), left(k));
            [l0[0] + l1[0], l0[1] + l1[1]]
        } else if t == 1.0 && k + 2 < n {
            let (l0, l1) = (left(k), left(k + 1));
            [l0[0] + l1[0], l0[1] + l1[1]]
        } else {
            left(k)
        };
        if best.is_none_or(|(e, _, _)| d < e) {
            best = Some((d, q, facing));
        }
    }
    let Some((d, q, facing)) = best else { return 0 };
    if d < AT_A_VERTEX {
        return 0;
    }
    let along = dot(sub(p, q), facing);
    if along > 0.0 {
        1
    } else if along < 0.0 {
        -1
    } else {
        // Straight on past an open line's end, the side its last segment's left looks to.
        let toward = cross(facing, sub(p, q));
        if toward > 0.0 { -1 } else { 1 }
    }
}

fn on_the_line(cutter: &Polyline, p: [f64; 2]) -> bool {
    (0..segments(cutter)).any(|k| {
        let (a, b) = (plan(vertex(cutter, k)), plan(vertex(cutter, k + 1)));
        let ab = sub(b, a);
        let span = dot(ab, ab);
        let t = if span > 0.0 { (dot(sub(p, a), ab) / span).clamp(0.0, 1.0) } else { 0.0 };
        length(sub(p, [a[0] + t * ab[0], a[1] + t * ab[1]])) < AT_A_VERTEX
    })
}

/// Inside a closed string in plan, by the crossings of a line due east.
fn inside(ring: &Polyline, p: [f64; 2]) -> bool {
    let n = ring.vertices.len();
    let mut within = false;
    for k in 0..n {
        let (a, b) = (plan(ring.vertices[k]), plan(ring.vertices[(k + 1) % n]));
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
            within = !within;
        }
    }
    within
}

/// The point halfway along a part in plan, to tell which side of the cutting string it's on.
fn middle(part: &Polyline) -> [f64; 2] {
    let spans: Vec<f64> = part.vertices.windows(2).map(|w| length(sub(plan(w[1]), plan(w[0])))).collect();
    let half = spans.iter().sum::<f64>() / 2.0;
    let mut walked = 0.0;
    for (k, &span) in spans.iter().enumerate() {
        if walked + span >= half && span > 0.0 {
            let t = (half - walked) / span;
            let (a, b) = (plan(part.vertices[k]), plan(part.vertices[k + 1]));
            return [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
        }
        walked += span;
    }
    plan(part.vertices[0])
}

/// Two parts that meet end to end as one: the second's first vertex is the first's last.
fn append(first: &mut Polyline, second: &Polyline) {
    first.vertices.extend(second.vertices.iter().skip(1).copied());
}

/// A string trimmed by a cutting string in plan (both open or closed): cut wherever the cutting string crosses it, and
/// the parts on the side kept kept, in the order the string runs. Parts kept next to each other, as either side of a
/// place the cutting string only touches, stay one. Every cut is at the string's own height there. Refused if the
/// point that says which side to keep is on the cutting string, or if nothing of the string is on that side.
pub fn trim_string(line: &Polyline, cutter: &Polyline, keep: Keep) -> Result<Trimmed, GeomError> {
    let needed = |l: &Polyline| if l.closed { 3 } else { 2 };
    if line.vertices.len() < needed(line) || cutter.vertices.len() < needed(cutter) {
        return Err(GeomError::InvalidInput("a string with too few vertices to cut or cut by".into()));
    }
    let keep_side = match keep {
        Keep::Side(at) => match side_of(cutter, at) {
            0 => return Err(GeomError::InvalidInput("the point saying which side to keep is on the cutting string; pick one to the side of it".into())),
            side => Some(side),
        },
        Keep::Both => None,
    };
    cut_keeping(line, cutter, keep_side, "it lies wholly on the side dropped")
}

/// A string clipped to a closed string in plan (String tools plan, workstream 6): cut wherever the polygon's edge
/// crosses it, and the parts inside it kept, or those outside, in the order the string runs, as a trim keeping the side
/// a point inside or outside is on. Every cut is at the string's own height there. A string the edge doesn't cross comes
/// back whole with no cuts, if it lies on the side kept. Refused if the polygon isn't closed, or if nothing of the
/// string is on the side kept.
pub fn clip_string(line: &Polyline, polygon: &Polyline, inside: bool) -> Result<Trimmed, GeomError> {
    if !polygon.closed || polygon.vertices.len() < 3 {
        return Err(GeomError::InvalidInput("the string to clip to isn't a closed string of three vertices or more".into()));
    }
    if line.vertices.len() < if line.closed { 3 } else { 2 } {
        return Err(GeomError::InvalidInput("a string with too few vertices to clip".into()));
    }
    cut_keeping(line, polygon, Some(if inside { 1 } else { -1 }), if inside { "it lies wholly outside" } else { "it lies wholly inside" })
}

/// The cut both a trim and a clip make: the string cut where the cutting string crosses it, and the parts on the side
/// kept (+1 inside or left, -1 outside or right; none for every part) kept. `dropped_words` says why a string with
/// nothing on the side kept is refused.
fn cut_keeping(line: &Polyline, cutter: &Polyline, keep_side: Option<i8>, dropped_words: &str) -> Result<Trimmed, GeomError> {
    let m = segments(line) as f64;
    let mut cuts: Vec<Along> = cuts_along(line, cutter).into_iter().map(|(u, _)| u).collect();
    if !line.closed {
        // An end lying on the cutting string isn't a cut: nothing is past it.
        cuts.retain(|&u| u > 0.0 && u < m);
    }
    let count = cuts.len();
    if count == 0 {
        let whole = Polyline { vertices: line.vertices.clone(), closed: line.closed };
        if let Some(side) = keep_side
            && side_of(cutter, middle(&Polyline { vertices: line.vertices.clone(), closed: false })) != side
        {
            return Err(GeomError::InvalidInput(dropped_words.into()));
        }
        return Ok(Trimmed { pieces: vec![whole], cuts: 0, dropped: 0 });
    }
    let parts: Vec<Polyline> = if line.closed {
        (0..count).map(|i| part(line, cuts[i], if i + 1 < count { cuts[i + 1] } else { cuts[0] + m })).collect()
    } else {
        let mut ends = vec![0.0];
        ends.extend(&cuts);
        ends.push(m);
        ends.windows(2).map(|w| part(line, w[0], w[1])).collect()
    };
    let kept: Vec<bool> = parts.iter().map(|p| keep_side.is_none_or(|side| side_of(cutter, middle(p)) == side)).collect();
    let dropped = kept.iter().filter(|&&k| !k).count();
    if dropped == parts.len() {
        return Err(GeomError::InvalidInput(dropped_words.into()));
    }
    let mut pieces: Vec<Polyline> = Vec::new();
    let mut last_kept = false;
    for (part, &keep) in parts.iter().zip(&kept) {
        if keep && last_kept && keep_side.is_some() {
            append(pieces.last_mut().expect("a piece"), part);
        } else if keep {
            pieces.push(part.clone());
        }
        last_kept = keep;
    }
    // Round a closed string, the last part kept runs on into the first.
    if line.closed && keep_side.is_some() && pieces.len() > 1 && kept[0] && kept[parts.len() - 1] {
        let first = pieces.remove(0);
        append(pieces.last_mut().expect("a piece"), &first);
        let last = pieces.pop().expect("a piece");
        pieces.insert(0, last);
    }
    // Kept all the way round, a closed string is closed again as it was.
    if line.closed && dropped == 0 && keep_side.is_some() {
        return Ok(Trimmed { pieces: vec![Polyline { vertices: line.vertices.clone(), closed: true }], cuts: count, dropped: 0 });
    }
    Ok(Trimmed { pieces, cuts: count, dropped })
}

/// A string run on: which end, how far in plan, and the string with that end moved there.
#[derive(Debug, Clone, PartialEq)]
pub struct Extended {
    pub line: Polyline,
    pub at_start: bool,
    pub length: f64,
}

/// An open string run on from its nearer end to a cutting string in plan, along its last segment and at that
/// segment's grade, no further than `reach`: the end vertex moves there, so the line is the same line, longer. The
/// end that meets the cutting string sooner is the one run on. `None` if an end already lies on it. Refused if neither
/// end meets it within the reach, or the string is closed.
pub fn extend_string(line: &Polyline, cutter: &Polyline, reach: f64) -> Result<Option<Extended>, GeomError> {
    if line.closed {
        return Err(GeomError::InvalidInput("a closed string has no ends to run on".into()));
    }
    let n = line.vertices.len();
    if n < 2 || cutter.vertices.len() < 2 {
        return Err(GeomError::InvalidInput("a string with too few vertices to run on or run to".into()));
    }
    if reach.is_nan() || reach <= 0.0 {
        return Err(GeomError::InvalidInput(format!("the reach must be more than nothing; it was {reach} m")));
    }
    let run_on = |end: usize, before: usize| -> Option<f64> {
        let (o, back) = (plan(line.vertices[end]), plan(line.vertices[before]));
        let d = sub(o, back);
        let span = length(d);
        if span == 0.0 {
            return None;
        }
        let d = [d[0] / span, d[1] / span];
        (0..segments(cutter))
            .filter_map(|c| {
                let (p, q) = (plan(vertex(cutter, c)), plan(vertex(cutter, c + 1)));
                let s = sub(q, p);
                let denominator = cross(d, s);
                if denominator.abs() < 1e-12 * length(s).max(f64::MIN_POSITIVE) {
                    return None;
                }
                let w = sub(p, o);
                let ahead = cross(w, s) / denominator;
                let on = cross(w, d) / denominator;
                ((-1e-12..=1.0 + 1e-12).contains(&on) && ahead > -AT_A_VERTEX).then_some(ahead.max(0.0))
            })
            .min_by(f64::total_cmp)
    };
    let start = run_on(0, 1);
    let end = run_on(n - 1, n - 2);
    if start.is_some_and(|s| s < AT_A_VERTEX) || end.is_some_and(|s| s < AT_A_VERTEX) {
        return Ok(None);
    }
    let within = |s: Option<f64>| s.filter(|&s| s <= reach);
    let (at_start, run) = match (within(start), within(end)) {
        (Some(s), Some(e)) if s < e => (true, s),
        (_, Some(e)) => (false, e),
        (Some(s), None) => (true, s),
        (None, None) => {
            let flat = (plan(line.vertices[0]) == plan(line.vertices[1]), plan(line.vertices[n - 1]) == plan(line.vertices[n - 2]));
            let why = if flat.0 && flat.1 { "both its end segments are straight up, with no way to run on" } else { "neither end meets it running on" };
            return Err(GeomError::InvalidInput(format!("{why} within {reach} m")));
        }
    };
    let (end, before) = if at_start { (0, 1) } else { (n - 1, n - 2) };
    let (o, b) = (line.vertices[end], line.vertices[before]);
    let span = length(sub(plan(o), plan(b)));
    let (d, grade) = ([(o[0] - b[0]) / span, (o[1] - b[1]) / span], (o[2] - b[2]) / span);
    let mut moved = line.clone();
    moved.vertices[end] = [o[0] + run * d[0], o[1] + run * d[1], o[2] + run * grade];
    Ok(Some(Extended { line: moved, at_start, length: run }))
}

/// A string split at the place on it nearest a point in plan, as a click on it: a vertex goes in there, at the
/// string's height, unless the place is within a millimetre of a vertex, which it splits at. An open string splits in
/// two sharing that vertex; a closed one opens there. Comes back with the vertex split at. Refused if the point is
/// more than [`ON_THE_STRING`] from the string, or at an open string's end.
pub fn split_string_at(line: &Polyline, at: [f64; 2]) -> Result<(Polyline, Option<Polyline>, usize), GeomError> {
    let needed = if line.closed { 3 } else { 2 };
    if line.vertices.len() < needed {
        return Err(GeomError::InvalidInput("a string with too few vertices to split".into()));
    }
    let mut best: Option<(f64, usize, f64)> = None;
    for k in 0..segments(line) {
        let (a, b) = (plan(vertex(line, k)), plan(vertex(line, k + 1)));
        let ab = sub(b, a);
        let span = dot(ab, ab);
        let t = if span > 0.0 { (dot(sub(at, a), ab) / span).clamp(0.0, 1.0) } else { 0.0 };
        let d = length(sub(at, [a[0] + t * ab[0], a[1] + t * ab[1]]));
        if best.is_none_or(|(e, _, _)| d < e) {
            best = Some((d, k, t));
        }
    }
    let (miss, k, t) = best.expect("a string has a segment");
    if miss > ON_THE_STRING {
        return Err(GeomError::InvalidInput(format!("the point is {miss:.3} m from the string in plan, more than the {ON_THE_STRING} m a click may miss it by")));
    }
    let span = length(sub(plan(vertex(line, k + 1)), plan(vertex(line, k))));
    let mut line = line.clone();
    let index = if t * span < SPLIT_AT_VERTEX {
        k
    } else if (1.0 - t) * span < SPLIT_AT_VERTEX {
        (k + 1) % line.vertices.len()
    } else {
        line.vertices.insert(k + 1, point_at(&line, k as f64 + t));
        k + 1
    };
    if !line.closed && (index == 0 || index + 1 == line.vertices.len()) {
        return Err(GeomError::InvalidInput("the point is at an end of the string, where there's nothing to split".into()));
    }
    let (first, second) = crate::string::split_string(&line, index)?;
    Ok((first, second, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn open(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.to_vec(), closed: false }
    }

    /// A line across at `x` m east, running north.
    fn across(x: f64) -> Polyline {
        open(&[[E + x, N - 50.0, 0.0], [E + x, N + 50.0, 0.0]])
    }

    /// A 100 m line due east at 10 %, from RL 100.
    fn graded() -> Polyline {
        open(&[[E, N, 100.0], [E + 50.0, N, 105.0], [E + 100.0, N, 110.0]])
    }

    #[test]
    fn a_graded_line_trimmed_keeps_its_own_height_at_the_cut() {
        // Kept west of a line across 60 m along: it ends there, at RL 106, the cutting line's RL 0 never used.
        let t = trim_string(&graded(), &across(60.0), Keep::Side([E, N + 5.0])).unwrap();
        assert_eq!((t.cuts, t.dropped, t.pieces.len()), (1, 1, 1));
        let end = *t.pieces[0].vertices.last().unwrap();
        assert!((end[0] - (E + 60.0)).abs() < 1e-9 && (end[2] - 106.0).abs() < 1e-9, "{end:?}");
        assert_eq!(t.pieces[0].vertices[..2], graded().vertices[..2], "the vertices kept, exactly");
        // East kept: it starts there.
        let t = trim_string(&graded(), &across(60.0), Keep::Side([E + 90.0, N - 5.0])).unwrap();
        assert!((t.pieces[0].vertices[0][2] - 106.0).abs() < 1e-9);
        assert_eq!(t.pieces[0].vertices.last(), graded().vertices.last());
    }

    #[test]
    fn keep_both_splits_at_each_crossing_sharing_the_vertex() {
        let t = trim_string(&graded(), &across(60.0), Keep::Both).unwrap();
        assert_eq!((t.cuts, t.dropped, t.pieces.len()), (1, 0, 2));
        assert_eq!(t.pieces[0].vertices.last(), t.pieces[1].vertices.first(), "the two share the cut");
        let plan_length = |p: &Polyline| p.vertices.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f64>();
        assert!((plan_length(&t.pieces[0]) - 60.0).abs() < 1e-9 && (plan_length(&t.pieces[1]) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn a_cut_through_a_vertex_keeps_the_vertex_exactly() {
        // The line across goes through the vertex 50 m along, crossing both segments that meet there: one cut.
        let t = trim_string(&graded(), &across(50.0), Keep::Both).unwrap();
        assert_eq!(t.cuts, 1);
        assert_eq!(t.pieces[0].vertices, graded().vertices[..2]);
        assert_eq!(t.pieces[1].vertices, graded().vertices[1..]);
    }

    #[test]
    fn a_closed_cutter_keeps_inside_or_outside() {
        let limit = Polyline { vertices: vec![[E + 20.0, N - 20.0, 0.0], [E + 80.0, N - 20.0, 0.0], [E + 80.0, N + 20.0, 0.0], [E + 20.0, N + 20.0, 0.0]], closed: true };
        let t = trim_string(&graded(), &limit, Keep::Side([E + 50.0, N + 10.0])).unwrap();
        assert_eq!((t.cuts, t.pieces.len(), t.dropped), (2, 1, 2));
        assert!((t.pieces[0].vertices[0][0] - (E + 20.0)).abs() < 1e-9 && (t.pieces[0].vertices[0][2] - 102.0).abs() < 1e-9);
        let t = trim_string(&graded(), &limit, Keep::Side([E, N + 100.0])).unwrap();
        assert_eq!((t.pieces.len(), t.dropped), (2, 1), "both ends outside, kept as two strings");
    }

    #[test]
    fn a_closed_string_trimmed_keeps_the_part_round_its_start_whole() {
        // A 100 m square cut by a line across 60 m east: west kept is one open string round three sides of it.
        let square = Polyline { vertices: vec![[E, N, 0.0], [E + 100.0, N, 0.0], [E + 100.0, N + 100.0, 0.0], [E, N + 100.0, 0.0]], closed: true };
        let cutter = open(&[[E + 60.0, N - 10.0, 0.0], [E + 60.0, N + 110.0, 0.0]]);
        let t = trim_string(&square, &cutter, Keep::Side([E + 1.0, N + 50.0])).unwrap();
        assert_eq!((t.cuts, t.pieces.len(), t.dropped), (2, 1, 1));
        let piece = &t.pieces[0];
        assert!(!piece.closed);
        assert_eq!(piece.vertices.len(), 4, "{:?}", piece.vertices);
        let plan_length = piece.vertices.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum::<f64>();
        assert!((plan_length - 220.0).abs() < 1e-9, "{plan_length}");
    }

    #[test]
    fn what_cant_be_trimmed_is_refused() {
        let all_east = trim_string(&graded(), &across(-10.0), Keep::Side([E - 20.0, N]));
        assert!(all_east.unwrap_err().to_string().contains("wholly on the side dropped"));
        let on_it = trim_string(&graded(), &across(60.0), Keep::Side([E + 60.0, N + 30.0]));
        assert!(on_it.unwrap_err().to_string().contains("on the cutting string"));
        // Not crossed and on the side kept: whole, no cuts.
        let t = trim_string(&graded(), &across(-10.0), Keep::Side([E + 5.0, N])).unwrap();
        assert_eq!((t.cuts, t.pieces[0].clone()), (0, graded()));
    }

    #[test]
    fn a_line_run_on_carries_its_grade() {
        // 50 m at 5 % from RL 100, run on to a line across 80 m along: it ends there at RL 104.
        let line = open(&[[E, N, 100.0], [E + 50.0, N, 102.5]]);
        let x = extend_string(&line, &across(80.0), 100.0).unwrap().unwrap();
        assert!(!x.at_start && (x.length - 30.0).abs() < 1e-9);
        let end = x.line.vertices[1];
        assert!((end[0] - (E + 80.0)).abs() < 1e-9 && end[1] == N && (end[2] - 104.0).abs() < 1e-9, "{end:?}");
        assert_eq!(x.line.vertices.len(), 2, "the end moved, no vertex added");
        // The nearer end: a line across behind its start.
        let x = extend_string(&line, &across(-10.0), 100.0).unwrap().unwrap();
        assert!(x.at_start && (x.line.vertices[0][2] - 99.5).abs() < 1e-9);
    }

    #[test]
    fn a_run_on_past_its_reach_or_already_there() {
        let line = open(&[[E, N, 100.0], [E + 50.0, N, 102.5]]);
        assert!(extend_string(&line, &across(80.0), 20.0).unwrap_err().to_string().contains("within 20 m"));
        assert_eq!(extend_string(&line, &across(50.0), 20.0).unwrap(), None, "an end already on it");
        let ring = Polyline { closed: true, ..graded() };
        assert!(extend_string(&ring, &across(80.0), 100.0).is_err());
    }

    #[test]
    fn a_split_at_a_point_puts_a_vertex_in_at_the_strings_height() {
        let (first, second, at) = split_string_at(&graded(), [E + 30.0, N + 0.4]).unwrap();
        let second = second.unwrap();
        assert_eq!(at, 1);
        assert_eq!(first.vertices.len(), 2);
        assert!((first.vertices[1][0] - (E + 30.0)).abs() < 1e-9 && (first.vertices[1][2] - 103.0).abs() < 1e-9);
        assert_eq!(first.vertices[1], second.vertices[0]);
        // Within a millimetre of a vertex it splits there, adding none.
        let (first, second, at) = split_string_at(&graded(), [E + 50.0005, N]).unwrap();
        assert_eq!((at, first.vertices.len(), second.unwrap().vertices.len()), (1, 2, 2));
        assert!(split_string_at(&graded(), [E + 30.0, N + 2.0]).unwrap_err().to_string().contains("more than"));
        assert!(split_string_at(&graded(), [E, N]).unwrap_err().to_string().contains("at an end"));
    }

    /// A 100 m square pit limit, 0 to 100 m east and north.
    fn limit() -> Polyline {
        Polyline { vertices: vec![[E, N, 0.0], [E + 100.0, N, 0.0], [E + 100.0, N + 100.0, 0.0], [E, N + 100.0, 0.0]], closed: true }
    }

    #[test]
    fn a_graded_toe_clipped_to_a_pit_limit_keeps_the_part_inside_at_its_own_heights() {
        // 200 m due east at 10 % from RL 100, from 50 m west of the limit, 50 m north: the 100 m inside, from RL 105
        // to RL 115, the vertices inside kept exactly.
        let toe = open(&[[E - 50.0, N + 50.0, 100.0], [E + 30.0, N + 50.0, 108.0], [E + 150.0, N + 50.0, 120.0]]);
        let t = clip_string(&toe, &limit(), true).unwrap();
        assert_eq!((t.cuts, t.dropped, t.pieces.len()), (2, 2, 1));
        let kept = &t.pieces[0].vertices;
        assert_eq!(kept.len(), 3);
        assert!((kept[0][0] - E).abs() < 1e-9 && (kept[0][2] - 105.0).abs() < 1e-9, "{:?}", kept[0]);
        assert_eq!(kept[1], [E + 30.0, N + 50.0, 108.0]);
        assert!((kept[2][0] - (E + 100.0)).abs() < 1e-9 && (kept[2][2] - 115.0).abs() < 1e-9, "{:?}", kept[2]);
        // Outside: the two ends, 50 m each.
        let t = clip_string(&toe, &limit(), false).unwrap();
        assert_eq!((t.cuts, t.dropped, t.pieces.len()), (2, 1, 2));
        assert_eq!(t.pieces[0].vertices[0], toe.vertices[0]);
        assert_eq!(t.pieces[1].vertices.last(), toe.vertices.last());
    }

    #[test]
    fn a_string_wholly_on_one_side_is_kept_whole_or_refused() {
        let inside = open(&[[E + 10.0, N + 10.0, 100.0], [E + 90.0, N + 90.0, 110.0]]);
        let t = clip_string(&inside, &limit(), true).unwrap();
        assert_eq!((t.cuts, t.dropped), (0, 0));
        assert_eq!(t.pieces[0], inside);
        let refused = clip_string(&inside, &limit(), false).unwrap_err().to_string();
        assert!(refused.contains("wholly inside"), "{refused}");
        let mut open_limit = limit();
        open_limit.closed = false;
        assert!(clip_string(&inside, &open_limit, true).is_err(), "an open string clips nothing");
    }

    #[test]
    fn a_ring_crossing_the_limit_keeps_its_part_inside_as_one_open_string() {
        // A 40 m square ring centred on the limit's east edge: 20 m of it inside, its three sides there in one piece.
        let ring = Polyline { vertices: vec![[E + 80.0, N + 30.0, 100.0], [E + 120.0, N + 30.0, 100.0], [E + 120.0, N + 70.0, 100.0], [E + 80.0, N + 70.0, 100.0]], closed: true };
        let t = clip_string(&ring, &limit(), true).unwrap();
        assert_eq!((t.cuts, t.pieces.len()), (2, 1));
        let piece = &t.pieces[0];
        assert!(!piece.closed);
        let length: f64 = piece.vertices.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
        assert!((length - 80.0).abs() < 1e-9, "{length}");
    }
}
