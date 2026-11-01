//! One wall step (Phase 3): a string rises at a batter until it meets a target surface.
//!
//! Each segment of the string makes a wall plane, through the segment and rising at the batter square
//! to it in plan. Neighbouring walls meet in a hip, the line their planes share, so a string that rises
//! and falls gets exact walls and exact mitres. The sweep up the walls is a straight skeleton with
//! planes in place of plan offsets: where the two hips of a wall close, it is squeezed out and its
//! neighbours meet in a new hip. A plan offset can't do this: on a string that isn't level, the mitre
//! between two walls leaves the bisector, and where the string climbs along its length a hip can even
//! run outward and downhill. So a hip is taken outward in plan, and a closing goes ahead once it's the
//! first thing to happen along both of its hips.
//!
//! The walls are swept past the highest point of the target, and the crest is then walked across it,
//! triangle by triangle, wherever target and wall cross. Everything above the crest stays out of the
//! result, so a closing above the target changes nothing. An edge of the target is crossed once, as in
//! a section, so neighbouring triangles agree on the crest to the bit, and a target vertex on a wall
//! counts as above it, so a crest never runs through a corner.
//!
//! Each segment can have its own batter. Where neighbours' batters differ their planes still meet in a
//! hip at a corner, but along a straight run they share only the string, so the crest would step sideways
//! with nothing but a vertical wall to carry it. There a transition stands at the vertex: one plane with
//! no foot, through the fall line of the flatter wall and a hip that runs back across the steeper one. The
//! crest then bevels from one to the other at 45 degrees in plan, and the flatter batter reaches into the
//! steeper stretch and never the other way, so blending between batters can only flatten a slope.
//!
//! The target is a surface single-valued in plan, wound either way. Where it has no data under a wall,
//! in a no-seam hole or past its edge, it's bridged and the stretch is reported: straight across a hole
//! between its edges, and level with the nearest edge past the outside, so the crest stays whole and a
//! face stays one sheet. A wall that closes against a far wall (a split in skeleton terms) is refused,
//! as it would make the crest cross itself.

use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{BinaryHeap, HashMap};

use spade::{Point2, Triangulation};
use zd_core::{Polyline, TriMesh};

use crate::section_fill::{Cdt, faces_inside};
use crate::string::{TriangleBins, cross as cross2, segment_crossing};
use crate::face::level_sheet;
use crate::{Corner, EdgeRule, GeomError, OffsetSide, drape_string, filter_string, offset_string};

type P3 = [f64; 3];

/// Plan distance under which two string vertices are one (m).
const SAME_POINT: f64 = 1e-6;
/// Room for rounding on a position (m).
const NEAR: f64 = 1e-9;
/// A gap up to the target this small is rounding, not height: the string is on the target there. A
/// string registered on a grid and stepped to that same grid is only ever this far off it.
const ON_TARGET: f64 = 1e-7;
/// Hips this close to parallel never meet: 1e-10 rad squared.
const PARALLEL: f64 = 1e-20;
/// How much of a wall polygon the triangles may miss.
const AREA_TOLERANCE: f64 = 1e-6;
/// How many times the plan distance a hip that doesn't rise is run out, more than a sharp mitre needs.
const MITRE_REACH: f64 = 8.0;
/// The most a transition's hip leans, short of running along the string.
const MOST_LEAN: f64 = 89.9999;
/// Mitres on a berm's inside line reaching farther than this many widths are bevelled.
const BERM_MITRE_LIMIT: f64 = 4.0;
/// A berm of no width on one edge next to a wider one, which a plane can't quite be (m).
const LEAST_WIDTH: f64 = 1e-3;
/// How often along a line a wall's height is read, to find where it passes a minimum bench (m).
const BENCH_SAMPLE: f64 = 5.0;
/// The turn past which a line has a corner, where walls meet in a hip (degrees).
const CORNER_TURN: f64 = 30.0;
/// Slopes this close, as a share of the steeper, are one batter where a string turns at all. A wall above a
/// road differs from the wall behind it by this little, on purpose, so the two are parallel.
const SAME_SLOPE: f64 = 0.02;
/// How far a string has to turn for two walls to have a hip of their own: the sine of the angle.
const TURNS: f64 = 1e-3;
/// Neither a wall nor a string end: the edge a wall stands on.
const FOOT: usize = usize::MAX;
/// The level edge left at the top of the sweep.
const TOP: usize = usize::MAX - 1;
const NONE: usize = usize::MAX - 2;

/// What a wall step did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WallLog {
    /// Walls the string made.
    pub walls: usize,
    /// Walls squeezed out by their neighbours below the crest.
    pub squeezed_out: usize,
    /// String vertices dropped for sitting on the one before in plan.
    pub vertices_dropped: usize,
    /// Stretches where the target is below the string, so the step has no height.
    pub no_height: Vec<Stretch>,
    /// Stretches of crest where the target has no data and was bridged.
    pub no_target: Vec<Stretch>,
}

/// A stretch along a string or crest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stretch {
    pub from: [f64; 3],
    pub to: [f64; 3],
}

/// A wall from a string up to a target.
#[derive(Debug, Clone)]
pub struct WallStep {
    /// Where the wall meets the target, the same way along as the string.
    pub crest: Polyline,
    /// The wall from the string to the crest, wound upward.
    pub wall: TriMesh,
    pub log: WallLog,
}

/// The wall from `toe` rising on `side` at `batter_degrees` (from horizontal, measured square to the
/// string in plan) until it meets `target`.
pub fn project_wall(toe: &Polyline, side: OffsetSide, batter_degrees: f64, target: &TriMesh) -> Result<WallStep, GeomError> {
    project_wall_by_edge(toe, side, &vec![batter_degrees; segment_count(toe)], target)
}

/// The same with a batter for each segment of `toe`, in order.
pub fn project_wall_by_edge(toe: &Polyline, side: OffsetSide, batters: &[f64], target: &TriMesh) -> Result<WallStep, GeomError> {
    if batters.len() != segment_count(toe) {
        return Err(GeomError::InvalidInput(format!("{} batters for {} segments", batters.len(), segment_count(toe))));
    }
    if !batters.iter().all(|b| *b > 0.0 && *b < 90.0) {
        return Err(GeomError::InvalidInput("the batter must be between 0 and 90 degrees".into()));
    }
    let Some(bounds) = target.bounds() else { return Err(GeomError::InvalidInput("the target has no triangles".into())) };
    let mut log = WallLog::default();
    let slopes: Vec<f64> = batters.iter().map(|b| b.to_radians().tan()).collect();
    let (string, slopes, origin) = local_string(toe, &slopes, side, &mut log)?;
    let flattest = slopes.iter().copied().fold(f64::INFINITY, f64::min);
    let highest = string.iter().map(|p| p[2]).fold(bounds.max[2] - origin[2], f64::max);
    let lowest = string.iter().map(|p| p[2]).fold(bounds.min[2] - origin[2], f64::min);
    let mut sweep = Sweep::new(&string, toe.closed, &slopes, origin)?;
    sweep.run(highest + 1.0, (highest + 1.0 - lowest) / flattest, &mut log);
    let edges: Vec<Vec<Edge>> = sweep.walls.iter().take(sweep.segments).map(edges_round).collect();
    let near = target_near(target, origin, &edges)?;
    let mut attempt = log.clone();
    let walked = match Walk::new(&near, near.triangles.len(), &sweep, &edges, origin).run(&mut attempt) {
        Ok(walked) => {
            log = attempt;
            walked
        }
        Err(GeomError::OffTarget { .. }) => {
            let bridged = bridged_target(&near, &edges);
            Walk::new(&bridged, near.triangles.len(), &sweep, &edges, origin).run(&mut log)?
        }
        Err(e) => return Err(e),
    };
    if let Some(p) = crossing_of(&walked.crest, toe.closed) {
        return Err(GeomError::InvalidInput(format!(
            "the crest crosses itself near {:.3}, {:.3}: a wall has closed against a far wall",
            p[0] + origin[0],
            p[1] + origin[1]
        )));
    }
    let wall = wall_mesh(&sweep, &edges, &walked, origin)?;
    if wall.triangles.is_empty() {
        return Err(GeomError::NoHeight);
    }
    let mut vertices: Vec<[f64; 3]> = walked.crest.iter().map(|p| world(*p, origin)).collect();
    if side == OffsetSide::Right {
        vertices.reverse();
        for stretches in [&mut log.no_height, &mut log.no_target] {
            stretches.reverse();
            for stretch in stretches {
                std::mem::swap(&mut stretch.from, &mut stretch.to);
            }
        }
    }
    Ok(WallStep { crest: Polyline { vertices, closed: toe.closed }, wall, log })
}

/// The string about a whole-metre origin, running so the wall rises on its left, and each segment's slope
/// the same way along.
fn local_string(toe: &Polyline, slopes: &[f64], side: OffsetSide, log: &mut WallLog) -> Result<(Vec<P3>, Vec<f64>, P3), GeomError> {
    let Some(&first) = toe.vertices.first() else { return Err(GeomError::InvalidInput("the string has no vertices".into())) };
    if toe.vertices.iter().flatten().any(|c| !c.is_finite()) {
        return Err(GeomError::InvalidInput("the string has a vertex that isn't a number".into()));
    }
    let origin = [first[0].round(), first[1].round(), first[2].round()];
    let mut string: Vec<P3> = Vec::with_capacity(toe.vertices.len());
    let mut kept: Vec<f64> = Vec::with_capacity(slopes.len());
    for (i, v) in toe.vertices.iter().enumerate() {
        let p = [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];
        match string.last() {
            Some(q) if plan_gap(*q, p) < SAME_POINT => log.vertices_dropped += 1,
            _ => {
                // A vertex dropped leaves a segment of no length, so the one that arrives here counts.
                if i > 0 {
                    kept.push(slopes[i - 1]);
                }
                string.push(p);
            }
        }
    }
    if toe.closed && string.len() > 1 && plan_gap(string[0], string[string.len() - 1]) < SAME_POINT {
        // The last segment kept is the one that closes the string.
        string.pop();
        log.vertices_dropped += 1;
    } else if toe.closed {
        kept.push(slopes[slopes.len() - 1]);
    }
    if string.len() < if toe.closed { 3 } else { 2 } {
        return Err(GeomError::InvalidInput("the string needs at least two distinct vertices in plan (three if closed)".into()));
    }
    if side == OffsetSide::Right {
        string.reverse();
        // A closed string's closing segment stays last, running from the new last vertex to the new first.
        let running = if toe.closed { kept.len() - 1 } else { kept.len() };
        kept[..running].reverse();
    }
    Ok((string, kept, origin))
}

/// What getting a line ready for the next step did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConditionLog {
    pub vertices_in: usize,
    /// Vertices left in the plan shape.
    pub plan_vertices: usize,
    /// Vertices on the line after it's registered back on the surface.
    pub vertices_out: usize,
    /// Vertices with no surface under them, which keep their elevations.
    pub off_surface: usize,
    /// Stretches of berm left out, as the wall below is under the minimum bench.
    pub no_berm: Vec<Stretch>,
}

/// The string the next step starts from: `crest` simplified in plan to within `tolerance` metres, moved
/// out a berm of `berm` metres on `side` if there is one, and registered back on `surface`.
///
/// A line cut from a surface has a vertex at every triangle it crosses, and a wall from each of those
/// short segments would face its own way. Walls follow the plan shape alone: collinear segments in plan
/// share a wall's azimuth however they rise and fall, and the line still lies exactly on the surface.
/// An offset can come back in pieces, as `offset_string` says, and each piece is a string to go on with.
pub fn next_toe(crest: &Polyline, berm: f64, side: OffsetSide, tolerance: f64, surface: &TriMesh) -> Result<(Vec<Polyline>, ConditionLog), GeomError> {
    let (pieces, mut log) = next_line(crest, berm.into(), None, side, tolerance, 0.0)?;
    let mut toes = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        let (toe, draped) = drape_string(piece, surface, true)?;
        log.vertices_out += toe.vertices.len();
        log.off_surface += draped.off_surface;
        toes.push(toe);
    }
    Ok((toes, log))
}

/// The next line before it's registered: `crest` simplified in plan to within `before` metres, moved out
/// a berm of `berm` metres on `side` if there is one, and simplified to within `after`. Each vertex takes
/// the height of the nearest point on the crest, which it keeps if there's no surface under it.
pub(crate) fn next_line(crest: &Polyline, berm: EdgeRule<'_>, least: Option<LeastBench<'_>>, side: OffsetSide, before: f64, after: f64) -> Result<(Vec<Polyline>, ConditionLog), GeomError> {
    let flat = Polyline { vertices: crest.vertices.iter().map(|v| [v[0], v[1], 0.0]).collect(), closed: crest.closed };
    let shape = filter_string(&flat, before)?;
    let mut log = ConditionLog { vertices_in: crest.vertices.len(), plan_vertices: shape.vertices.len(), ..ConditionLog::default() };
    // The rule looks at the line where it really is, but the offset is a plan job.
    let (placed, widths) = berm.applied_to(&at_crest_heights(&shape, crest))?;
    if !widths.iter().all(|w| w.is_finite() && *w >= 0.0) {
        return Err(GeomError::InvalidInput("the berm width must be zero or more".into()));
    }
    let moved_out = |line: &Polyline, widths: &[f64]| -> Result<Vec<Polyline>, GeomError> {
        let width = widths.first().copied().unwrap_or(0.0);
        if widths.iter().any(|w| *w != width) {
            Ok(vec![offset_by_edge(line, widths, side)?])
        } else if width > 0.0 {
            offset_string(&shape, width, side, Corner::Mitre { limit: BERM_MITRE_LIMIT }, None)
        } else {
            Ok(vec![shape.clone()])
        }
    };
    let pieces = match least {
        None => moved_out(&placed, &widths)?,
        Some(least) => {
            let (line, narrowed, no_berm) = without_low_benches(&placed, widths.clone(), crest, least, before, after, false);
            log.no_berm = no_berm;
            match moved_out(&line, &narrowed) {
                Ok(pieces) => pieces,
                // A change of width just short of a corner has no room to bevel, as beside a road.
                Err(refused) => {
                    let (line, narrowed, no_berm) = without_low_benches(&placed, widths, crest, least, before, after, true);
                    log.no_berm = no_berm;
                    moved_out(&line, &narrowed).map_err(|_| refused)?
                }
            }
        }
    };
    let lines = pieces.iter().map(|piece| Ok(at_crest_heights(&filter_string(piece, after)?, crest))).collect::<Result<_, GeomError>>()?;
    Ok((lines, log))
}

/// What a berm needs under it: a wall from `toe` at least `height` high.
#[derive(Clone, Copy)]
pub(crate) struct LeastBench<'a> {
    pub toe: &'a Polyline,
    pub height: f64,
}

/// A line split wherever the wall below it passes the least height, and no width along the stretches
/// where it's lower. A stretch too short to matter takes on its neighbours, so a berm doesn't stutter on
/// and off along an undulating seam: a dip shorter than the berm is wide is bermed across, and so short
/// a stretch of height isn't bermed at all. The crest is simplified by `before` and the line by `after`, and
/// each can weave that far, so with no berm it stands clear by the two together. A line that isn't
/// conditioned is the crest, vertex for vertex, and stays that way, so it isn't split. With `at_corners`, a
/// change of width near a corner moves to it.
fn without_low_benches(line: &Polyline, widths: Vec<f64>, crest: &Polyline, least: LeastBench<'_>, before: f64, after: f64, at_corners: bool) -> (Polyline, Vec<f64>, Vec<Stretch>) {
    let low = |p: [f64; 3]| height_on(crest, p) - height_on(least.toe, p) < least.height;
    let n = line.vertices.len();
    let (mut vertices, mut split) = (Vec::with_capacity(n), Vec::with_capacity(widths.len()));
    for (i, &width) in widths.iter().enumerate() {
        let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
        let at = |t: f64| [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
        vertices.push(a);
        let samples = if before > 0.0 { (plan_gap(a, b) / BENCH_SAMPLE).ceil().max(1.0) as usize } else { 0 };
        let (mut from, mut was) = (0.0, low(a));
        for k in 1..=samples {
            let to = k as f64 / samples as f64;
            if low(at(to)) != was {
                let (mut before, mut after) = (from, to);
                for _ in 0..24 {
                    let middle = (before + after) / 2.0;
                    if low(at(middle)) == was {
                        before = middle;
                    } else {
                        after = middle;
                    }
                }
                vertices.push(at(after));
                split.push(width);
                was = !was;
            }
            from = to;
        }
        split.push(width);
    }
    if !line.closed {
        vertices.push(line.vertices[n - 1]);
    }
    let result = Polyline { vertices, closed: line.closed };
    let m = result.vertices.len();
    let middle = |i: usize| {
        let (a, b) = (result.vertices[i], result.vertices[(i + 1) % m]);
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
    };
    let mut lows: Vec<bool> = (0..split.len()).map(|i| low(middle(i))).collect();
    let lengths: Vec<f64> = (0..split.len()).map(|i| plan_gap(result.vertices[i], result.vertices[(i + 1) % m])).collect();
    let widest = split.iter().copied().fold(0.0, f64::max);
    for short in [true, false] {
        even_out(&mut lows, &lengths, widest, short);
    }
    if at_corners {
        move_changes_to_corners(&mut lows, &result, &lengths, widest);
    }
    let mut no_berm = Vec::new();
    let mut stretch: Option<[f64; 3]> = None;
    for i in 0..=lows.len() {
        match (stretch, lows.get(i).copied().unwrap_or(false)) {
            (None, true) => stretch = Some(result.vertices[i]),
            (Some(from), false) => {
                no_berm.push(Stretch { from, to: result.vertices[i % m] });
                stretch = None;
            }
            _ => {}
        }
    }
    let widths = split.iter().zip(&lows).map(|(width, low)| if *low { width.min(before + after) } else { *width }).collect();
    (result, widths, no_berm)
}

/// A change of width bevels over as far as the berm is wide, and it can't do that round a corner. One nearer
/// a corner than that moves to the corner, where the walls can meet in a hip with no bevel.
fn move_changes_to_corners(lows: &mut [bool], line: &Polyline, lengths: &[f64], reach: f64) {
    let n = line.vertices.len();
    let corner = |i: usize| {
        let (a, b, c) = (line.vertices[i - 1], line.vertices[i], line.vertices[(i + 1) % n]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        cross2(u, v).abs().atan2(u[0] * v[0] + u[1] * v[1]).to_degrees() > CORNER_TURN
    };
    for i in 1..lows.len() {
        if lows[i - 1] == lows[i] || corner(i) {
            continue;
        }
        let ahead = (i + 1..lows.len()).take_while(|&j| lengths[i..j].iter().sum::<f64>() < reach).find(|&j| corner(j));
        let behind = (1..i).rev().take_while(|&j| lengths[j..i].iter().sum::<f64>() < reach).find(|&j| corner(j));
        match (ahead, behind) {
            (Some(j), _) => {
                let before = lows[i - 1];
                lows[i..j].fill(before);
            }
            (None, Some(j)) => {
                let after = lows[i];
                lows[j..i].fill(after);
            }
            (None, None) => {}
        }
    }
}

/// Turns over any run of `state` shorter than `reach`.
fn even_out(flags: &mut [bool], lengths: &[f64], reach: f64, state: bool) {
    let mut i = 0;
    while i < flags.len() {
        let same = flags[i..].iter().take_while(|f| **f == flags[i]).count();
        if flags[i] == state && lengths[i..i + same].iter().sum::<f64>() < reach {
            flags[i..i + same].fill(!state);
        }
        i += same;
    }
}

/// A line moved out by a width for each edge: a wall up a metre over the width of each edge, read off at
/// that height, so the walls' hips and transitions do what a mitre can't.
fn offset_by_edge(line: &Polyline, widths: &[f64], side: OffsetSide) -> Result<Polyline, GeomError> {
    let flat = Polyline { vertices: line.vertices.iter().map(|v| [v[0], v[1], 0.0]).collect(), closed: line.closed };
    let batters: Vec<f64> = widths.iter().map(|w| w.max(LEAST_WIDTH).recip().atan().to_degrees()).collect();
    let widest = widths.iter().copied().fold(0.0, f64::max);
    let sheet = level_sheet(&flat, 1.0, 100.0 + 8.0 * widest);
    let mut moved = project_wall_by_edge(&flat, side, &batters, &sheet)?.crest;
    // An edge of no width stays on the line exactly.
    let n = flat.vertices.len();
    let still: Vec<([f64; 3], [f64; 3])> = (0..widths.len()).filter(|&i| widths[i] == 0.0).map(|i| (flat.vertices[i], flat.vertices[(i + 1) % n])).collect();
    for p in &mut moved.vertices {
        let nearest = still.iter().map(|&(a, b)| (a, b, nearest_along(a, b, *p))).min_by(|x, y| x.2.1.total_cmp(&y.2.1));
        if let Some((a, b, (t, gap))) = nearest
            && gap <= 2.0 * LEAST_WIDTH
        {
            let on = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), p[2]];
            let corner = [a, b].into_iter().find(|v| plan_gap(*v, on) <= 2.0 * LEAST_WIDTH);
            *p = corner.map_or(on, |v| [v[0], v[1], p[2]]);
        }
    }
    moved.vertices.dedup_by(|a, b| plan_gap(*a, *b) < SAME_POINT);
    Ok(moved)
}

/// How far along a segment in plan the nearest point to `p` is, from 0 to 1, and how far off it is.
pub(crate) fn nearest_along(a: [f64; 3], b: [f64; 3], p: [f64; 3]) -> (f64, f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0) } else { 0.0 };
    (t, (a[0] + t * dx - p[0]).hypot(a[1] + t * dy - p[1]))
}

pub(crate) fn segment_count(line: &Polyline) -> usize {
    if line.closed { line.vertices.len() } else { line.vertices.len().saturating_sub(1) }
}

/// The height of the nearest point on a line in plan.
fn height_on(line: &Polyline, p: [f64; 3]) -> f64 {
    let n = line.vertices.len();
    let mut nearest = (f64::INFINITY, line.vertices.first().map_or(p[2], |v| v[2]));
    for i in 0..segment_count(line) {
        let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
        let (t, gap) = nearest_along(a, b, p);
        if gap < nearest.0 {
            nearest = (gap, a[2] + t * (b[2] - a[2]));
        }
    }
    nearest.1
}

/// A line's vertices at the height of the nearest point on `crest` in plan.
pub(crate) fn at_crest_heights(line: &Polyline, crest: &Polyline) -> Polyline {
    Polyline { vertices: line.vertices.iter().map(|p| [p[0], p[1], height_on(crest, *p)]).collect(), closed: line.closed }
}

/// A line with a new vertex wherever `place` puts one for a crest vertex. It's given the crest vertex, the
/// nearest point on the line, and how far apart they are in plan.
pub(crate) fn with_crest_vertices(line: &Polyline, crest: &Polyline, place: impl Fn([f64; 3], [f64; 3], f64) -> Option<[f64; 3]>) -> Polyline {
    let (n, segments) = (line.vertices.len(), segment_count(line));
    let mut falling: Vec<Vec<(f64, [f64; 3])>> = vec![Vec::new(); segments];
    for &c in &crest.vertices {
        let nearest = (0..segments).map(|i| (i, nearest_along(line.vertices[i], line.vertices[(i + 1) % n], c))).min_by(|a, b| a.1.1.total_cmp(&b.1.1));
        let Some((i, (t, gap))) = nearest else { continue };
        let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
        if let Some(p) = place(c, [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])], gap) {
            falling[i].push((t, p));
        }
    }
    let mut vertices: Vec<[f64; 3]> = Vec::with_capacity(n + crest.vertices.len());
    let apart = |a: [f64; 3], b: [f64; 3]| (a[0] - b[0]).hypot(a[1] - b[1]) > SAME_POINT;
    for i in 0..n {
        let a = line.vertices[i];
        vertices.push(a);
        let Some(along) = falling.get_mut(i) else { continue };
        let b = line.vertices[(i + 1) % n];
        along.sort_by(|x, y| x.0.total_cmp(&y.0));
        for &(_, p) in along.iter() {
            if vertices.last().is_some_and(|&last| apart(last, p)) && apart(p, b) {
                vertices.push(p);
            }
        }
    }
    Polyline { vertices, closed: line.closed }
}

/// A plane, with `above` positive on the side its normal points to.
#[derive(Debug, Clone, Copy)]
struct Plane {
    normal: P3,
    through: P3,
}

impl Plane {
    fn above(&self, p: P3) -> f64 {
        dot(self.normal, sub(p, self.through))
    }
}

/// One string segment's wall, or a vertical plane closing off an end of an open string.
#[derive(Debug, Clone)]
struct Wall {
    plane: Plane,
    /// The unit plan direction the wall rises in.
    out: [f64; 2],
    /// Rise over run, square to the string in plan.
    slope: f64,
    /// A transition's two ends are one point.
    foot: [P3; 2],
    /// The boundary up from `foot[0]` and from `foot[1]`, one hip at a time.
    start_side: Vec<Rise>,
    end_side: Vec<Rise>,
    closed_at: Option<P3>,
}

/// The end of a stretch of wall boundary, and the wall across it.
#[derive(Debug, Clone, Copy)]
struct Rise {
    to: P3,
    across: usize,
}

/// The line two walls share, up from where they first meet.
#[derive(Debug, Clone, Copy)]
struct Hip {
    from: P3,
    /// Unit, outward from the string in plan.
    direction: P3,
    behind: usize,
    ahead: usize,
    before: usize,
    after: usize,
    live: bool,
}

/// Two hips meeting, which closes the wall between them.
#[derive(Debug, Clone, Copy)]
struct Closing {
    at: P3,
    /// How far out from its foot the wall closes: only the order to look at closings in.
    reach: f64,
    start: usize,
    end: usize,
}

impl PartialEq for Closing {
    fn eq(&self, other: &Self) -> bool {
        self.reach == other.reach
    }
}
impl Eq for Closing {}
impl PartialOrd for Closing {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Closing {
    /// The nearest first, out of a max-heap.
    fn cmp(&self, other: &Self) -> Ordering {
        other.reach.total_cmp(&self.reach)
    }
}

/// The walls of a string and the hips between them.
struct Sweep {
    walls: Vec<Wall>,
    hips: Vec<Hip>,
    /// Real walls, which come first in `walls`.
    segments: usize,
    closed: bool,
}

impl Sweep {
    fn new(string: &[P3], closed: bool, slopes: &[f64], origin: P3) -> Result<Self, GeomError> {
        let n = string.len();
        let plain: Vec<Wall> = slopes.iter().enumerate().map(|(i, slope)| wall_of(string[i], string[(i + 1) % n], *slope)).collect();
        let (points, mut walls) = with_transitions(string, &plain, closed);
        let string = &points[..];
        let n = string.len();
        let segments = walls.len();
        if !closed {
            walls.push(end_plane(string[0], &walls[0]));
            walls.push(end_plane(string[n - 1], &walls[segments - 1]));
        }
        let (start, finish) = (segments, segments + 1);
        let mut sweep = Sweep { walls, hips: Vec::with_capacity(2 * n), segments, closed };
        for (i, &vertex) in string.iter().enumerate() {
            let behind = if i > 0 {
                i - 1
            } else if closed {
                n - 1
            } else {
                start
            };
            let ahead = if i < segments { i } else { finish };
            let Some(direction) = sweep.hip_direction(behind, ahead) else {
                return Err(GeomError::InvalidInput(format!(
                    "no wall can rise from the string at {:.3}, {:.3}: it doubles back there",
                    vertex[0] + origin[0],
                    vertex[1] + origin[1]
                )));
            };
            let before = if i > 0 {
                i - 1
            } else if closed {
                n - 1
            } else {
                NONE
            };
            let after = if i + 1 < n {
                i + 1
            } else if closed {
                0
            } else {
                NONE
            };
            sweep.hips.push(Hip { from: vertex, direction, behind, ahead, before, after, live: true });
        }
        Ok(sweep)
    }

    fn is_end(&self, wall: usize) -> bool {
        wall >= self.segments
    }

    fn is_transition(&self, wall: usize) -> bool {
        !self.is_end(wall) && self.walls[wall].foot[0] == self.walls[wall].foot[1]
    }

    /// The line two walls share, outward from the string in plan. Walls in one plane share all of
    /// it, so the fall line stands in. A string that turns straight back has no outward.
    fn hip_direction(&self, behind: usize, ahead: usize) -> Option<P3> {
        let (a, b) = (&self.walls[behind], &self.walls[ahead]);
        let c = cross(a.plane.normal, b.plane.normal);
        let length = norm(c);
        if length < 1e-12 {
            let real = if self.is_end(behind) { b } else { a };
            return Some(unit([real.out[0], real.out[1], real.slope]));
        }
        let c = scale(c, 1.0 / length);
        let out = [a.out[0] + b.out[0], a.out[1] + b.out[1]];
        if out[0] * out[0] + out[1] * out[1] < 1e-12 {
            return None;
        }
        let outward = c[0] * out[0] + c[1] * out[1];
        // Square to the way out, which only a level ridge can be, a hip rises.
        let flip = if outward.abs() > 1e-12 { outward < 0.0 } else { c[2] < 0.0 };
        Some(if flip { scale(c, -1.0) } else { c })
    }

    /// Closes walls wherever two hips meet below `top`, then runs every hip left out to `top`, or as
    /// far as a mitre could reach past `furthest` in plan if it never gets there.
    fn run(&mut self, top: f64, furthest: f64, log: &mut WallLog) {
        log.walls = self.segments;
        let mut closings = BinaryHeap::new();
        for h in 0..self.hips.len() {
            if self.hips[h].after != NONE {
                closings.extend(self.closing(h, self.hips[h].after));
            }
        }
        while let Some(event) = closings.pop() {
            if event.at[2] >= top || !self.hips[event.start].live || !self.hips[event.end].live || !self.is_first_on_its_hips(&event) {
                continue;
            }
            let Some(hip) = self.close_wall(event) else { break };
            log.squeezed_out += 1;
            // The closings next along may have been waiting on this one, so they're all looked at again.
            let (before, after) = (self.hips[hip].before, self.hips[hip].after);
            for (a, b) in [(self.before(before), before), (before, hip), (hip, after), (after, self.after(after))] {
                if a != NONE && b != NONE {
                    closings.extend(self.closing(a, b));
                }
            }
        }
        for h in 0..self.hips.len() {
            if self.hips[h].live {
                let hip = self.hips[h];
                let rise = if hip.direction[2] > 1e-12 { ((top - hip.from[2]) / hip.direction[2]).max(0.0) } else { f64::INFINITY };
                let plan = hip.direction[0].hypot(hip.direction[1]);
                let out = if plan > 1e-12 { MITRE_REACH * furthest / plan } else { f64::INFINITY };
                self.end_hip(h, add(hip.from, scale(hip.direction, rise.min(out))));
            }
        }
    }

    fn before(&self, hip: usize) -> usize {
        if hip == NONE { NONE } else { self.hips[hip].before }
    }

    fn after(&self, hip: usize) -> usize {
        if hip == NONE { NONE } else { self.hips[hip].after }
    }

    /// Whether nothing happens sooner along either hip of a closing.
    fn is_first_on_its_hips(&self, event: &Closing) -> bool {
        let (a, b) = (&self.hips[event.start], &self.hips[event.end]);
        let along = |hip: &Hip, p: P3| dot(sub(p, hip.from), hip.direction);
        let sooner = |hip: &Hip, other: Option<Closing>| other.is_some_and(|c| along(hip, c.at) < along(hip, event.at) - NEAR);
        let (before, after) = (a.before, b.after);
        !(before != NONE && sooner(a, self.closing(before, event.start))) && !(after != NONE && sooner(b, self.closing(event.end, after)))
    }

    /// Where the hips either side of a wall meet, if they do on their way out.
    fn closing(&self, start: usize, end: usize) -> Option<Closing> {
        let (a, b) = (&self.hips[start], &self.hips[end]);
        let gap = sub(b.from, a.from);
        // A transition's two hips leave one point and only open out.
        if gap == [0.0; 3] && self.is_transition(a.ahead) {
            return None;
        }
        let n = cross(a.direction, b.direction);
        let nn = dot(n, n);
        if nn < PARALLEL {
            // Head on along one line: two walls meeting in a level ridge.
            let along = dot(gap, a.direction);
            let aside = norm(sub(gap, scale(a.direction, along)));
            return (along > 0.0 && dot(a.direction, b.direction) < 0.0 && aside < NEAR).then(|| self.closing_at(scale(add(a.from, b.from), 0.5), start, end));
        }
        let s = dot(cross(gap, b.direction), n) / nn;
        let t = dot(cross(gap, a.direction), n) / nn;
        (s >= -NEAR && t >= -NEAR).then(|| self.closing_at(add(a.from, scale(a.direction, s.max(0.0))), start, end))
    }

    fn closing_at(&self, at: P3, start: usize, end: usize) -> Closing {
        let wall = &self.walls[self.hips[start].ahead];
        let from_foot = sub(at, wall.foot[0]);
        Closing { at, reach: wall.out[0] * from_foot[0] + wall.out[1] * from_foot[1], start, end }
    }

    /// Takes a wall out: its hips end where they meet, and a new hip joins its neighbours. No hip
    /// comes back when nothing is left.
    fn close_wall(&mut self, event: Closing) -> Option<usize> {
        let Closing { at, start, end, .. } = event;
        let wall = self.hips[start].ahead;
        let (behind, ahead) = (self.hips[start].behind, self.hips[end].ahead);
        let (before, after) = (self.hips[start].before, self.hips[end].after);
        self.end_hip(start, at);
        self.end_hip(end, at);
        self.walls[wall].closed_at = Some(at);
        if behind == ahead || (self.is_end(behind) && self.is_end(ahead)) {
            return None;
        }
        // Neighbours squeezed together may be back to back. The hip runs on away from the closed wall.
        let foot = self.walls[wall].foot;
        let from_foot = sub(at, scale(add(foot[0], foot[1]), 0.5));
        let mut direction = self.hip_direction(behind, ahead).unwrap_or_else(|| unit([from_foot[0], from_foot[1], 0.0]));
        if direction[0] * from_foot[0] + direction[1] * from_foot[1] < 0.0 {
            direction = scale(direction, -1.0);
        }
        let hip = self.hips.len();
        self.hips.push(Hip { from: at, direction, behind, ahead, before, after, live: true });
        if before != NONE {
            self.hips[before].after = hip;
        }
        if after != NONE {
            self.hips[after].before = hip;
        }
        Some(hip)
    }

    fn end_hip(&mut self, h: usize, to: P3) {
        let hip = &mut self.hips[h];
        hip.live = false;
        let (behind, ahead) = (hip.behind, hip.ahead);
        self.walls[behind].end_side.push(Rise { to, across: ahead });
        self.walls[ahead].start_side.push(Rise { to, across: behind });
    }

    /// The walls either end of an open string, or none for a closed one.
    fn ends(&self) -> Option<(usize, usize)> {
        (!self.closed).then_some((self.segments, self.segments + 1))
    }
}

fn wall_of(a: P3, b: P3, slope: f64) -> Wall {
    let along = sub(b, a);
    let length = along[0].hypot(along[1]);
    let out = [-along[1] / length, along[0] / length];
    let normal = unit(cross(along, [out[0], out[1], slope]));
    Wall { plane: Plane { normal, through: a }, out, slope, foot: [a, b], start_side: Vec::new(), end_side: Vec::new(), closed_at: None }
}

/// Whether two neighbouring walls need a transition: they differ, and the line their planes share doesn't
/// run out between the ways they face. On a straight run it's the string itself.
fn needs_transition(a: &Wall, b: &Wall) -> bool {
    if a.slope == b.slope || ((a.slope - b.slope).abs() <= SAME_SLOPE * a.slope.max(b.slope) && cross2(a.out, b.out).abs() > TURNS) {
        return false;
    }
    let shared = cross(a.plane.normal, b.plane.normal);
    let plan = shared[0].hypot(shared[1]);
    if plan < 1e-12 {
        return true;
    }
    let outward = shared[0] * (a.out[0] + b.out[0]) + shared[1] * (a.out[1] + b.out[1]);
    let way = if outward < 0.0 { -1.0 } else { 1.0 };
    let p = [way * shared[0] / plan, way * shared[1] / plan];
    let turn = cross2(a.out, b.out);
    let reaches = |out: [f64; 2]| p[0] * out[0] + p[1] * out[1] > 1e-9;
    !(reaches(a.out) && reaches(b.out) && cross2(a.out, p) * turn >= 0.0 && cross2(p, b.out) * turn >= 0.0)
}

/// A direction in `wall`'s plane over a plan step.
fn across_plane(wall: &Wall, step: [f64; 2]) -> P3 {
    let n = wall.plane.normal;
    [step[0], step[1], -(n[0] * step[0] + n[1] * step[1]) / n[2]]
}

/// The transition at `vertex` between walls `a` and `b`: through the flatter wall's fall line and a hip
/// across the steeper wall, leaning back along its foot.
fn transition_at(vertex: P3, a: &Wall, b: &Wall) -> Wall {
    let (steep, flat, sign) = if a.slope > b.slope { (a, b, -1.0) } else { (b, a, 1.0) };
    let along = [steep.out[1], -steep.out[0]];
    let back = [along[0] * sign, along[1] * sign];
    // At any height the flatter wall's fall line stands off the steeper wall's crest by some distance, and
    // the crest runs back along the steeper wall by that distance again to bevel at 45 degrees in plan.
    let (flat_reach, steep_reach) = (flat.slope.recip(), steep.slope.recip());
    let beyond = flat_reach * (flat.out[0] * steep.out[0] + flat.out[1] * steep.out[1]) - steep_reach;
    let behind = flat_reach * (flat.out[0] * back[0] + flat.out[1] * back[1]);
    let lean = (behind + beyond.max(0.0)).atan2(steep_reach).to_degrees().clamp(0.0, MOST_LEAN).to_radians();
    let plan = [steep.out[0] * lean.cos() + back[0] * lean.sin(), steep.out[1] * lean.cos() + back[1] * lean.sin()];
    let normal = unit(cross(across_plane(flat, flat.out), across_plane(steep, plan)));
    let normal = if normal[2] < 0.0 { scale(normal, -1.0) } else { normal };
    let out = [a.out[0] + b.out[0], a.out[1] + b.out[1]];
    let length = out[0].hypot(out[1]);
    Wall { plane: Plane { normal, through: vertex }, out: [out[0] / length, out[1] / length], slope: flat.slope, foot: [vertex, vertex], start_side: Vec::new(), end_side: Vec::new(), closed_at: None }
}

/// The string and its walls with a transition wherever neighbours need one. A transition is a segment of
/// no length, so its vertex is in the string twice.
fn with_transitions(string: &[P3], plain: &[Wall], closed: bool) -> (Vec<P3>, Vec<Wall>) {
    let count = plain.len();
    let mut points = Vec::with_capacity(string.len());
    let mut walls = Vec::with_capacity(count);
    for (i, wall) in plain.iter().enumerate() {
        points.push(string[i]);
        walls.push(wall.clone());
        let next = i + 1;
        if next == count && !closed {
            continue;
        }
        let after = &plain[next % count];
        if needs_transition(wall, after) {
            let vertex = string[next % string.len()];
            points.push(vertex);
            walls.push(transition_at(vertex, wall, after));
        }
    }
    if !closed {
        points.push(string[string.len() - 1]);
    }
    (points, walls)
}

/// The vertical plane square to an end wall through the end of the string.
fn end_plane(end: P3, wall: &Wall) -> Wall {
    let normal = [wall.out[1], -wall.out[0], 0.0];
    Wall { plane: Plane { normal, through: end }, out: wall.out, slope: wall.slope, foot: [end, end], start_side: Vec::new(), end_side: Vec::new(), closed_at: None }
}

/// One stretch of a wall's boundary.
#[derive(Debug, Clone, Copy)]
struct Edge {
    from: P3,
    to: P3,
    across: usize,
}

/// A wall's boundary, anticlockwise in plan: along the foot, up the end side, back along the top if
/// the wall reached it, and down the start side.
fn edges_round(wall: &Wall) -> Vec<Edge> {
    let mut edges = if wall.foot[0] == wall.foot[1] { Vec::new() } else { vec![Edge { from: wall.foot[0], to: wall.foot[1], across: FOOT }] };
    let mut at = wall.foot[1];
    for rise in &wall.end_side {
        edges.push(Edge { from: at, to: rise.to, across: rise.across });
        at = rise.to;
    }
    let last = wall.start_side.last().map_or(wall.foot[0], |r| r.to);
    if wall.closed_at.is_none() {
        edges.push(Edge { from: at, to: last, across: TOP });
    }
    for k in (0..wall.start_side.len()).rev() {
        let to = if k > 0 { wall.start_side[k - 1].to } else { wall.foot[0] };
        edges.push(Edge { from: wall.start_side[k].to, to, across: wall.start_side[k].across });
    }
    edges
}

/// The target's triangles under the walls, about the string's origin.
fn target_near(target: &TriMesh, origin: P3, edges: &[Vec<Edge>]) -> Result<TriMesh, GeomError> {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in edges.iter().flatten().flat_map(|e| [e.from, e.to]) {
        lo = [lo[0].min(p[0] + origin[0]), lo[1].min(p[1] + origin[1])];
        hi = [hi[0].max(p[0] + origin[0]), hi[1].max(p[1] + origin[1])];
    }
    let mut renumbered = vec![u32::MAX; target.vertices.len()];
    let mut near = TriMesh::default();
    for triangle in &target.triangles {
        let [a, b, c] = triangle.map(|v| target.vertices[v as usize]);
        let (low, high) = ([a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1])], [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1])]);
        if low[0] > hi[0] || high[0] < lo[0] || low[1] > hi[1] || high[1] < lo[1] {
            continue;
        }
        // The walk goes by which way round a triangle runs in plan. A triangle on edge in plan has no
        // way round, and no height the walk could use.
        let plan = area2([a[0], a[1]], [b[0], b[1]], [c[0], c[1]]);
        if plan == 0.0 {
            continue;
        }
        let triangle = if plan > 0.0 { *triangle } else { [triangle[0], triangle[2], triangle[1]] };
        near.triangles.push(triangle.map(|v| {
            if renumbered[v as usize] == u32::MAX {
                renumbered[v as usize] = near.vertices.len() as u32;
                let p = target.vertices[v as usize];
                near.vertices.push([p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]]);
            }
            renumbered[v as usize]
        }));
    }
    if near.triangles.is_empty() {
        return Err(GeomError::InvalidInput("the target doesn't reach the string".into()));
    }
    Ok(near)
}

/// The target with its gaps filled over everything the walls reach: a triangulation in plan of its
/// open edges, keeping only the triangles where it has no data. New corners take the elevation of the
/// nearest open edge. Its own triangles stay first.
fn bridged_target(near: &TriMesh, edges: &[Vec<Edge>]) -> TriMesh {
    let mut uses: HashMap<(u32, u32), u32> = HashMap::with_capacity(near.triangles.len() * 2);
    for t in &near.triangles {
        for k in 0..3 {
            let (u, v) = (t[k], t[(k + 1) % 3]);
            *uses.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    let mut open: Vec<(u32, u32)> = uses.into_iter().filter(|(_, count)| *count == 1).map(|(edge, _)| edge).collect();
    open.sort_unstable();
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in near.vertices.iter().copied().chain(edges.iter().flatten().flat_map(|e| [e.from, e.to])) {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let mut out = near.clone();
    let mut cdt = Cdt::new();
    let mut numbers: Vec<u32> = Vec::new();
    let mut handles: HashMap<u32, _> = HashMap::new();
    let insert = |cdt: &mut Cdt, number: u32, p: P3, numbers: &mut Vec<u32>| {
        let handle = cdt.insert(Point2::new(p[0], p[1])).ok()?;
        if handle.index() >= numbers.len() {
            numbers.resize(handle.index() + 1, number);
        }
        Some(handle)
    };
    for &(u, v) in &open {
        for w in [u, v] {
            if let Entry::Vacant(slot) = handles.entry(w)
                && let Some(handle) = insert(&mut cdt, w, near.vertices[w as usize], &mut numbers)
            {
                slot.insert(handle);
            }
        }
    }
    let margin = 1.0;
    for corner in [[lo[0] - margin, lo[1] - margin], [hi[0] + margin, lo[1] - margin], [hi[0] + margin, hi[1] + margin], [lo[0] - margin, hi[1] + margin]] {
        let nearest = handles.keys().copied().min_by(|&a, &b| {
            let gap = |v: u32| (near.vertices[v as usize][0] - corner[0]).hypot(near.vertices[v as usize][1] - corner[1]);
            gap(a).total_cmp(&gap(b)).then(a.cmp(&b))
        });
        let z = nearest.map_or(0.0, |v| near.vertices[v as usize][2]);
        out.vertices.push([corner[0], corner[1], z]);
        let number = out.vertices.len() as u32 - 1;
        insert(&mut cdt, number, [corner[0], corner[1], z], &mut numbers);
    }
    for (u, v) in open {
        if let (Some(&from), Some(&to)) = (handles.get(&u), handles.get(&v))
            && from != to
            && cdt.can_add_constraint(from, to)
        {
            cdt.add_constraint(from, to);
        }
    }
    let bins = TriangleBins::new(near, lo, hi);
    for face in cdt.inner_faces() {
        let corners = face.vertices().map(|v| v.position());
        let middle = [(corners[0].x + corners[1].x + corners[2].x) / 3.0, (corners[0].y + corners[1].y + corners[2].y) / 3.0];
        if bins.height(near, middle).is_none() {
            out.triangles.push(face.vertices().map(|v| numbers[v.fix().index()]));
        }
    }
    out
}

/// Where a crest stops on a wall's boundary.
#[derive(Debug, Clone, Copy)]
struct Stop {
    edge: usize,
    along: f64,
}

/// A stretch of crest across one wall.
#[derive(Debug, Clone)]
struct Piece {
    entry: Stop,
    exit: Stop,
    points: Vec<P3>,
}

/// The crest, and what of it lies on each wall.
struct Walked {
    crest: Vec<P3>,
    pieces: Vec<Vec<Piece>>,
    /// Walls whose whole foot is above the target.
    footless: Vec<bool>,
}

/// Round the crest, one triangle and one wall at a time.
struct Walk<'a> {
    near: &'a TriMesh,
    /// Triangles from this one on bridge a gap in the target.
    real: usize,
    bins: TriangleBins,
    /// Triangles round each vertex.
    fan_starts: Vec<u32>,
    fans: Vec<u32>,
    sweep: &'a Sweep,
    edges: &'a [Vec<Edge>],
    origin: P3,
}

/// Where the crest is.
#[derive(Debug, Clone, Copy)]
struct Position {
    wall: usize,
    triangle: u32,
    at: P3,
    entered: usize,
}

impl<'a> Walk<'a> {
    fn new(near: &'a TriMesh, real: usize, sweep: &'a Sweep, edges: &'a [Vec<Edge>], origin: P3) -> Self {
        let mut fan_starts = vec![0u32; near.vertices.len() + 1];
        for v in near.triangles.iter().flatten() {
            fan_starts[*v as usize + 1] += 1;
        }
        for k in 0..near.vertices.len() {
            fan_starts[k + 1] += fan_starts[k];
        }
        let mut next = fan_starts.clone();
        let mut fans = vec![0u32; near.triangles.len() * 3];
        for (t, corners) in near.triangles.iter().enumerate() {
            for v in corners {
                fans[next[*v as usize] as usize] = t as u32;
                next[*v as usize] += 1;
            }
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &near.vertices {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        Walk { near, real, bins: TriangleBins::new(near, lo, hi), fan_starts, fans, sweep, edges, origin }
    }

    fn fan(&self, v: u32) -> &[u32] {
        &self.fans[self.fan_starts[v as usize] as usize..self.fan_starts[v as usize + 1] as usize]
    }

    fn target_at(&self, p: P3) -> Option<f64> {
        self.bins.height(self.near, [p[0], p[1]])
    }

    fn off_target(&self, p: P3) -> GeomError {
        GeomError::OffTarget { at: [p[0] + self.origin[0], p[1] + self.origin[1]] }
    }

    /// The triangle holding `p` that the wall's plane cuts.
    fn triangle_at(&self, plane: &Plane, p: P3, candidates: impl Iterator<Item = u32>) -> Option<u32> {
        candidates.into_iter().find(|&t| {
            let corners = self.near.triangles[t as usize].map(|v| self.near.vertices[v as usize]);
            let sides = corners.map(|c| plane.above(c) >= 0.0);
            sides != [sides[0]; 3] && holds(corners, p)
        })
    }

    fn triangle_around(&self, plane: &Plane, p: P3, triangle: u32) -> Option<u32> {
        let corners = self.near.triangles[triangle as usize];
        self.triangle_at(plane, p, std::iter::once(triangle).chain(corners.iter().flat_map(|&v| self.fan(v).iter().copied())))
    }

    /// The same from scratch. A point on a triangle's edge can lie in a bin the triangle the wall cuts
    /// isn't in, so the search goes out round whichever triangles hold it.
    fn triangle_on(&self, plane: &Plane, p: P3) -> Option<u32> {
        let holding = self.bins.at(p[0], p[1]).iter().copied().filter(|&t| holds(self.near.triangles[t as usize].map(|v| self.near.vertices[v as usize]), p));
        holding.into_iter().find_map(|t| self.triangle_around(plane, p, t))
    }

    /// The triangle across the edge from `u` to `v`.
    fn across(&self, triangle: u32, u: u32, v: u32) -> Option<u32> {
        self.fan(u).iter().copied().find(|&t| t != triangle && self.near.triangles[t as usize].contains(&v))
    }

    /// The first place up the stretch `p`-`q` where the target stops being above it.
    fn target_met(&self, p: P3, q: P3) -> Result<Option<P3>, GeomError> {
        self.crossing_on(p, q, |gap| gap > ON_TARGET)
    }

    /// The first place along the stretch `p`-`q` where the target goes back above it.
    fn target_rises_over(&self, p: P3, q: P3) -> Result<Option<P3>, GeomError> {
        self.crossing_on(p, q, |gap| gap <= ON_TARGET)
    }

    /// The first place along `p`-`q` where the gap up to the target leaves the state `before` holds.
    fn crossing_on(&self, p: P3, q: P3, before: impl Fn(f64) -> bool) -> Result<Option<P3>, GeomError> {
        let height = |s: P3| self.target_at(s).ok_or_else(|| self.off_target(s));
        let at = |t: f64| add(p, scale(sub(q, p), t));
        let mut last = (0.0, height(p)? - p[2]);
        let end = self.target_at(q);
        let along = |s: [f64; 3]| {
            let (d, e) = ([q[0] - p[0], q[1] - p[1]], [s[0] - p[0], s[1] - p[1]]);
            let length = d[0] * d[0] + d[1] * d[1];
            // A crest that lands exactly on a string vertex starts from the end of its stretch.
            if length > 0.0 { (d[0] * e[0] + d[1] * e[1]) / length } else { 1.0 }
        };
        let steps = if plan_gap(p, q) > 0.0 { self.bins.crossings(self.near, [p[0], p[1]], [q[0], q[1]]) } else { Vec::new() };
        for s in steps.iter().copied().chain(end.map(|z| [q[0], q[1], z])) {
            let t = along(s);
            let gap = s[2] - at(t)[2];
            if before(last.1) && !before(gap) {
                let share = (last.1 / (last.1 - gap)).clamp(0.0, 1.0);
                return Ok(Some(at(last.0 + share * (t - last.0))));
            }
            last = (t, gap);
        }
        // The stretch runs off the target before it's crossed.
        end.map(|_| None).ok_or_else(|| self.off_target(q))
    }

    fn run(&self, log: &mut WallLog) -> Result<Walked, GeomError> {
        let segments = self.sweep.segments;
        let mut walked = Walked { crest: Vec::new(), pieces: vec![Vec::new(); segments], footless: vec![false; segments] };
        let start = match self.sweep.ends() {
            Some(_) => self.start_on_open_string(&mut walked, log)?,
            None => self.start_on_closed_string()?,
        };
        let Some(mut position) = start else { return Ok(walked) };
        let first = position;
        keep(&mut walked.crest, position.at);
        let mut piece = Piece { entry: stop_on(&self.edges[position.wall], position.entered, position.at), exit: Stop { edge: 0, along: 0.0 }, points: vec![position.at] };
        let limit = 8 * (self.near.triangles.len() + self.sweep.walls.len()) + 1_000;
        // Where the crest went over a gap in the target.
        let mut gap_from: Option<P3> = None;
        let mut note_gap = |at: P3, triangle: u32, log: &mut WallLog| {
            let over = triangle as usize >= self.real;
            match gap_from {
                None if over => gap_from = Some(at),
                Some(from) if !over => {
                    log.no_target.push(Stretch { from: world(from, self.origin), to: world(at, self.origin) });
                    gap_from = None;
                }
                _ => {}
            }
        };
        for _ in 0..limit {
            note_gap(position.at, position.triangle, log);
            let plane = self.sweep.walls[position.wall].plane;
            let corners = self.near.triangles[position.triangle as usize];
            let sides = corners.map(|v| plane.above(self.near.vertices[v as usize]));
            let Some(k) = (0..3).find(|&k| sides[k] >= 0.0 && sides[(k + 1) % 3] < 0.0) else {
                position.triangle = self.triangle_around(&plane, position.at, position.triangle).filter(|&t| t != position.triangle).ok_or_else(|| self.off_target(position.at))?;
                continue;
            };
            let (u, v) = (corners[k], corners[(k + 1) % 3]);
            let next = crossing_of_edge(self.near, &plane, u, v);
            if let Some((edge, point, along)) = first_exit(&self.edges[position.wall], position.at, next, position.entered) {
                keep(&mut walked.crest, point);
                keep(&mut piece.points, point);
                piece.exit = Stop { edge, along };
                let across = self.edges[position.wall][edge].across;
                walked.pieces[position.wall].push(std::mem::replace(&mut piece, Piece { entry: Stop { edge: 0, along: 0.0 }, exit: Stop { edge: 0, along: 0.0 }, points: vec![point] }));
                if let Some((_, finish)) = self.sweep.ends()
                    && across == finish
                {
                    note_gap(point, 0, log);
                    return Ok(walked);
                }
                if across == FOOT {
                    match self.resume_along(position.wall, point, &mut walked, log)? {
                        Some(resumed) => {
                            piece.entry = stop_on(&self.edges[resumed.wall], resumed.entered, resumed.at);
                            piece.points = vec![resumed.at];
                            position = resumed;
                            continue;
                        }
                        None => return Ok(walked),
                    }
                }
                if across == TOP || self.sweep.is_end(across) {
                    return Err(GeomError::InvalidInput(format!(
                        "the crest ran out of the walls at {:.3}, {:.3}: the string climbs too steeply for this batter there",
                        point[0] + self.origin[0],
                        point[1] + self.origin[1]
                    )));
                }
                let entered = self.edges[across].iter().position(|e| e.across == position.wall).ok_or_else(|| self.off_target(point))?;
                if self.sweep.closed && across == first.wall && entered == first.entered && norm(sub(point, first.at)) < SAME_POINT {
                    note_gap(point, 0, log);
                    walked.crest.pop();
                    return Ok(walked);
                }
                piece.entry = stop_on(&self.edges[across], entered, point);
                let triangle = self.triangle_around(&self.sweep.walls[across].plane, point, position.triangle).ok_or_else(|| self.off_target(point))?;
                position = Position { wall: across, triangle, at: point, entered };
                continue;
            }
            keep(&mut walked.crest, next);
            keep(&mut piece.points, next);
            position.at = next;
            position.entered = NONE;
            position.triangle = self.across(position.triangle, u, v).ok_or_else(|| self.off_target(next))?;
        }
        Err(GeomError::InvalidInput("the crest walk didn't finish".into()))
    }

    /// Where the crest starts on an open string: up the end if the target is above the string there,
    /// or at the string itself if not.
    fn start_on_open_string(&self, walked: &mut Walked, log: &mut WallLog) -> Result<Option<Position>, GeomError> {
        let foot = self.sweep.walls[0].foot;
        let height = self.target_at(foot[0]).ok_or_else(|| self.off_target(foot[0]))?;
        if height <= foot[0][2] + ON_TARGET {
            walked.crest.push(foot[0]);
            return self.resume_along(0, foot[0], walked, log);
        }
        let (start, _) = self.sweep.ends().expect("an open string");
        self.start_up(|across| across == start).map(Some)
    }

    /// Where the crest starts on a closed string: on any hip the target meets.
    fn start_on_closed_string(&self) -> Result<Option<Position>, GeomError> {
        let mut covered = false;
        for wall in 0..self.sweep.segments {
            let p = self.sweep.walls[wall].foot[0];
            covered |= self.target_at(p).ok_or_else(|| self.off_target(p))? > p[2] + ON_TARGET;
        }
        if !covered {
            return Err(GeomError::NoHeight);
        }
        self.start_up(|across| across < self.sweep.segments).map(Some)
    }

    /// The first place, wall by wall and up each start side, the target meets an edge `wanted` takes.
    fn start_up(&self, wanted: impl Fn(usize) -> bool) -> Result<Position, GeomError> {
        for wall in 0..self.sweep.segments {
            let edges = &self.edges[wall];
            for e in (edges.len() - self.sweep.walls[wall].start_side.len()..edges.len()).rev() {
                if !wanted(edges[e].across) {
                    continue;
                }
                if let Some(point) = self.target_met(edges[e].to, edges[e].from)? {
                    let triangle = self.triangle_on(&self.sweep.walls[wall].plane, point).ok_or_else(|| self.off_target(point))?;
                    return Ok(Position { wall, triangle, at: point, entered: e });
                }
            }
        }
        Err(GeomError::InvalidInput("the walls close before they reach the target".into()))
    }

    /// The crest where it's come down onto the string: on along the string until the target rises
    /// over it again.
    fn resume_along(&self, wall: usize, from: P3, walked: &mut Walked, log: &mut WallLog) -> Result<Option<Position>, GeomError> {
        let mut wall = wall;
        let mut at = from;
        loop {
            let foot = self.sweep.walls[wall].foot;
            if let Some(point) = self.target_rises_over(at, foot[1])? {
                keep(&mut walked.crest, point);
                log.no_height.push(Stretch { from: world(from, self.origin), to: world(point, self.origin) });
                let plane = self.sweep.walls[wall].plane;
                let triangle = self.triangle_on(&plane, point).ok_or_else(|| self.off_target(point))?;
                return Ok(Some(Position { wall, triangle, at: point, entered: 0 }));
            }
            keep(&mut walked.crest, foot[1]);
            if at == foot[0] {
                walked.footless[wall] = true;
            }
            wall += 1;
            if wall == self.sweep.segments {
                if self.sweep.closed {
                    wall = 0;
                } else {
                    log.no_height.push(Stretch { from: world(from, self.origin), to: world(foot[1], self.origin) });
                    return Ok(None);
                }
            }
            at = self.sweep.walls[wall].foot[0];
        }
    }
}

fn stop_on(edges: &[Edge], edge: usize, p: P3) -> Stop {
    let (from, to) = (edges[edge].from, edges[edge].to);
    let d = sub(to, from);
    let length = dot(d, d);
    Stop { edge, along: if length > 0.0 { (dot(sub(p, from), d) / length).clamp(0.0, 1.0) } else { 0.0 } }
}

/// The first edge the plan line from `a` to `b` leaves the wall by, other than the one it came in by.
fn first_exit(edges: &[Edge], a: P3, b: P3, entered: usize) -> Option<(usize, P3, f64)> {
    let d = [b[0] - a[0], b[1] - a[1]];
    if d[0] * d[0] + d[1] * d[1] < 1e-24 {
        return None;
    }
    let mut best: Option<(usize, f64, f64)> = None;
    for (i, edge) in edges.iter().enumerate() {
        if i == entered {
            continue;
        }
        let s = [edge.to[0] - edge.from[0], edge.to[1] - edge.from[1]];
        let denominator = cross2(d, s);
        // Inside is left of an edge, so a line leaving crosses it left to right.
        if denominator <= 0.0 {
            continue;
        }
        let gap = [edge.from[0] - a[0], edge.from[1] - a[1]];
        let t = cross2(gap, s) / denominator;
        let along = cross2(gap, d) / denominator;
        if (-1e-12..=1.0).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&along) && best.is_none_or(|b| t < b.1) {
            best = Some((i, t.max(0.0), along.clamp(0.0, 1.0)));
        }
    }
    best.map(|(edge, t, along)| (edge, add(a, scale(sub(b, a), t)), along))
}

/// Where the wall crosses a target edge: one point for that edge, whichever way round it's reached.
fn crossing_of_edge(near: &TriMesh, plane: &Plane, u: u32, v: u32) -> P3 {
    let (low, high) = (u.min(v), u.max(v));
    let (p, q) = (near.vertices[low as usize], near.vertices[high as usize]);
    let (dp, dq) = (plane.above(p), plane.above(q));
    let t = dp / (dp - dq);
    add(p, scale(sub(q, p), t))
}

/// Whether a triangle holds `p` in plan.
fn holds([a, b, c]: [P3; 3], p: P3) -> bool {
    let side = |u: P3, v: P3| (v[0] - u[0]) * (p[1] - u[1]) - (v[1] - u[1]) * (p[0] - u[0]);
    let (x, y, z) = (side(a, b), side(b, c), side(c, a));
    let room = -1e-7;
    (x >= room && y >= room && z >= room) || (x <= -room && y <= -room && z <= -room)
}

/// Adds a point unless it's the one already there.
fn keep(points: &mut Vec<P3>, p: P3) {
    if points.last().is_none_or(|q| norm(sub(*q, p)) > NEAR) {
        points.push(p);
    }
}

/// Where a line crosses itself in plan, if it does. Neighbouring segments share a vertex and don't count.
fn crossing_of(points: &[P3], closed: bool) -> Option<P3> {
    let n = points.len();
    let segments = if closed { n } else { n.saturating_sub(1) };
    let extent = |k: usize| (points[k][0].min(points[(k + 1) % n][0]), points[k][0].max(points[(k + 1) % n][0]));
    let mut order: Vec<usize> = (0..segments).collect();
    order.sort_by(|&a, &b| extent(a).0.total_cmp(&extent(b).0));
    for (place, &k) in order.iter().enumerate() {
        let reach = extent(k).1;
        for &m in &order[place + 1..] {
            if extent(m).0 > reach {
                break;
            }
            let apart = k.abs_diff(m);
            if apart <= 1 || (closed && apart == segments - 1) {
                continue;
            }
            let (a, b) = (points[k], points[(k + 1) % n]);
            let (p, q) = (points[m], points[(m + 1) % n]);
            if let Some((t, _)) = segment_crossing([a[0], a[1]], [b[0], b[1]], [p[0], p[1]], [q[0], q[1]]) {
                return Some(add(a, scale(sub(b, a), t)));
            }
        }
    }
    None
}

/// The walls below the crest as one mesh.
fn wall_mesh(sweep: &Sweep, edges: &[Vec<Edge>], walked: &Walked, origin: P3) -> Result<TriMesh, GeomError> {
    let mut mesh = MeshBuilder { origin, mesh: TriMesh::default(), numbered: HashMap::new() };
    for (wall, around) in edges.iter().enumerate() {
        let pieces = &walked.pieces[wall];
        if pieces.is_empty() {
            if walked.footless[wall] {
                continue;
            }
            if sweep.walls[wall].closed_at.is_none() {
                return Err(GeomError::InvalidInput("a wall reaches past the target with no crest on it".into()));
            }
            let outline: Vec<P3> = around.iter().map(|e| e.from).collect();
            mesh.add_polygon(&outline)?;
            continue;
        }
        for outline in outlines_below(around, pieces)? {
            mesh.add_polygon(&outline)?;
        }
    }
    Ok(mesh.mesh)
}

/// The parts of a wall below its crest, each anticlockwise: round the wall from where the crest comes
/// in to where it next leaves, then back along the crest.
fn outlines_below(edges: &[Edge], pieces: &[Piece]) -> Result<Vec<Vec<P3>>, GeomError> {
    let position = |s: Stop| (s.edge, s.along);
    let mut stops: Vec<(usize, f64, usize, bool)> = Vec::with_capacity(pieces.len() * 2);
    for (k, piece) in pieces.iter().enumerate() {
        stops.push((piece.entry.edge, piece.entry.along, k, true));
        stops.push((piece.exit.edge, piece.exit.along, k, false));
    }
    stops.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut used = vec![false; pieces.len()];
    let mut outlines = Vec::new();
    for first in 0..pieces.len() {
        if used[first] {
            continue;
        }
        let mut outline: Vec<P3> = Vec::new();
        let mut piece = first;
        loop {
            // Round the wall from this piece's entry to the next exit.
            let from = position(pieces[piece].entry);
            let place = stops.iter().position(|s| s.2 == piece && s.3).expect("every piece has an entry");
            let next = (1..=stops.len()).map(|k| stops[(place + k) % stops.len()]).find(|s| !s.3).expect("every piece has an exit");
            walk_edges(edges, from, (next.0, next.1), &mut outline);
            let exiting = next.2;
            if used[exiting] {
                return Err(GeomError::InvalidInput("a crest doesn't close on a wall".into()));
            }
            used[exiting] = true;
            outline.extend(pieces[exiting].points.iter().rev().copied());
            if exiting == first {
                break;
            }
            piece = exiting;
        }
        outline.dedup_by(|a, b| norm(sub(*a, *b)) <= NEAR);
        if outline.len() > 1 && norm(sub(outline[0], outline[outline.len() - 1])) <= NEAR {
            outline.pop();
        }
        outlines.push(outline);
    }
    Ok(outlines)
}

/// The wall's corners passed going round from one stop to the next.
fn walk_edges(edges: &[Edge], from: (usize, f64), to: (usize, f64), outline: &mut Vec<P3>) {
    let n = edges.len();
    let same_edge_ahead = from.0 == to.0 && from.1 <= to.1;
    if same_edge_ahead {
        return;
    }
    let mut e = from.0;
    loop {
        outline.push(edges[e].to);
        e = (e + 1) % n;
        if e == to.0 {
            break;
        }
    }
}

/// Polygons into one mesh, sharing vertices that are exactly the same point.
struct MeshBuilder {
    origin: P3,
    mesh: TriMesh,
    numbered: HashMap<[u64; 3], u32>,
}

impl MeshBuilder {
    fn number(&mut self, p: P3) -> u32 {
        let (mesh, origin) = (&mut self.mesh, self.origin);
        *self.numbered.entry(p.map(f64::to_bits)).or_insert_with(|| {
            mesh.vertices.push(world(p, origin));
            mesh.vertices.len() as u32 - 1
        })
    }

    /// Adds a polygon lying in a wall plane, by constrained Delaunay in plan.
    fn add_polygon(&mut self, outline: &[P3]) -> Result<(), GeomError> {
        if outline.len() < 3 {
            return Ok(());
        }
        if outline.len() == 3 {
            let triangle = [self.number(outline[0]), self.number(outline[1]), self.number(outline[2])];
            self.mesh.triangles.push(triangle);
            return Ok(());
        }
        let refused = || GeomError::InvalidInput(format!("a wall near {:.3}, {:.3} won't triangulate", outline[0][0] + self.origin[0], outline[0][1] + self.origin[1]));
        let triangles = std::panic::catch_unwind(|| triangles_of(outline)).ok().flatten().ok_or_else(refused)?;
        for [a, b, c] in triangles {
            let triangle = [self.number(outline[a]), self.number(outline[b]), self.number(outline[c])];
            self.mesh.triangles.push(triangle);
        }
        Ok(())
    }
}

/// The triangles of a polygon, as places in `outline`, if they add up to its area.
pub(crate) fn triangles_of(outline: &[P3]) -> Option<Vec<[usize; 3]>> {
    let mut cdt = Cdt::new();
    let mut handles = Vec::with_capacity(outline.len());
    for p in outline {
        handles.push(cdt.insert(Point2::new(p[0], p[1])).ok()?);
    }
    let mut place = vec![usize::MAX; cdt.num_vertices()];
    for (k, handle) in handles.iter().enumerate() {
        place[handle.index()] = k;
    }
    for k in 0..handles.len() {
        let (from, to) = (handles[k], handles[(k + 1) % handles.len()]);
        if from != to {
            if !cdt.can_add_constraint(from, to) {
                return None;
            }
            cdt.add_constraint(from, to);
        }
    }
    let inside = faces_inside(&cdt);
    let triangles: Vec<[usize; 3]> = cdt.inner_faces().filter(|f| inside[f.fix().index()]).map(|f| f.vertices().map(|v| place[v.fix().index()])).collect();
    let flat = |k: usize| [outline[k][0], outline[k][1]];
    let filled: f64 = triangles.iter().map(|&[a, b, c]| area2(flat(a), flat(b), flat(c)).abs() / 2.0).sum();
    let enclosed: f64 = (1..outline.len() - 1).map(|k| area2(flat(0), flat(k), flat(k + 1))).sum::<f64>().abs() / 2.0;
    ((filled - enclosed).abs() <= AREA_TOLERANCE * enclosed.max(1e-6)).then_some(triangles)
}

fn area2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
}

fn world(p: P3, origin: P3) -> P3 {
    [p[0] + origin[0], p[1] + origin[1], p[2] + origin[2]]
}

pub(crate) fn plan_gap(a: P3, b: P3) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: P3) -> P3 {
    scale(a, 1.0 / norm(a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::{Grid, Polyline};

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A target over 400 m by 400 m of 10 m cells.
    fn target(height: impl Fn(f64, f64) -> f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| height(x - E, y - N)))
    }

    fn string(points: &[[f64; 3]], closed: bool) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [p[0] + E, p[1] + N, p[2]]).collect(), closed }
    }

    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} is not {b} to {tolerance}");
    }

    /// The only open edges are the toe's, the crest's and the two ends.
    fn watertight_between_walls(step: &WallStep, toe_segments: usize) {
        let v = step.wall.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges + v.degenerate_triangles, 0, "{v:?}");
        assert_eq!(v.boundary_edges, toe_segments + step.crest.vertices.len() - 1 + 2, "{v:?}");
    }

    /// Crest vertices about the test origin.
    fn crest(step: &WallStep) -> Vec<[f64; 3]> {
        step.crest.vertices.iter().map(|p| [p[0] - E, p[1] - N, p[2]]).collect()
    }

    #[test]
    fn a_45_degree_batter_from_a_flat_toe_lands_at_its_height() {
        let step = project_wall(&string(&[[20.0, 20.0, 100.0], [180.0, 20.0, 100.0]], false), OffsetSide::Left, 45.0, &target(|_, _| 120.0)).unwrap();
        let line = crest(&step);
        close(line[0][0], 20.0, 1e-3);
        close(line[line.len() - 1][0], 180.0, 1e-3);
        for p in &line {
            close(p[1], 40.0, 1e-3);
            close(p[2], 120.0, 1e-3);
        }
        close(step.wall.area(), 160.0 * 20.0 * 2f64.sqrt(), 1e-3);
    }

    fn triangle_area(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
        norm(cross(sub(b, a), sub(c, a))) / 2.0
    }

    /// Rise of 2 over 1: 10 m out in a 20 m rise, against 20 m at 45 degrees.
    fn steep() -> f64 {
        2f64.atan().to_degrees()
    }

    /// A straight toe, 45 degrees then steeper, up 20 m. The flatter wall's crest is 20 m out and the steeper
    /// one's 10 m. The transition stands at the vertex and leans back across the steeper wall at 45 degrees,
    /// so the crest bevels from (100, 40) to (110, 30) and the steeper stretch is the one that gives.
    #[test]
    fn a_change_of_batter_on_a_straight_toe_bevels_the_crest_into_the_steeper_wall() {
        let toe = string(&[[20.0, 20.0, 100.0], [100.0, 20.0, 100.0], [180.0, 20.0, 100.0]], false);
        let step = project_wall_by_edge(&toe, OffsetSide::Left, &[45.0, steep()], &target(|_, _| 120.0)).unwrap();
        let line = crest(&step);
        close(line[0][0], 20.0, 1e-6);
        close(line[0][1], 40.0, 1e-6);
        close(line[line.len() - 1][0], 180.0, 1e-6);
        close(line[line.len() - 1][1], 30.0, 1e-6);
        assert!(line.iter().all(|p| (p[2] - 120.0).abs() < 1e-6));
        let on = |x: f64, y: f64| line.iter().any(|p| (p[0] - x).abs() < 1e-6 && (p[1] - y).abs() < 1e-6);
        assert!(on(100.0, 40.0) && on(110.0, 30.0), "{line:?}");
        // The two walls and the triangle between them, from the vertex up to the crest's bevel.
        let flat = 80.0 * 20.0 * 2f64.sqrt();
        let steep_wall = (70.0 + 80.0) / 2.0 * 10.0 * 5f64.sqrt();
        let transition = triangle_area([100.0, 20.0, 100.0], [100.0, 40.0, 120.0], [110.0, 30.0, 120.0]);
        close(step.wall.area(), flat + steep_wall + transition, 1e-6);
        let v = step.wall.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges + v.degenerate_triangles, 0, "{v:?}");
    }

    /// Run the other way, on the other side, it's the same wall.
    #[test]
    fn a_transition_doesnt_care_which_way_the_string_runs() {
        let forward = string(&[[20.0, 20.0, 100.0], [100.0, 20.0, 100.0], [180.0, 20.0, 100.0]], false);
        let one = project_wall_by_edge(&forward, OffsetSide::Left, &[45.0, steep()], &target(|_, _| 120.0)).unwrap();
        let backward = Polyline { vertices: forward.vertices.iter().rev().copied().collect(), closed: false };
        let other = project_wall_by_edge(&backward, OffsetSide::Right, &[steep(), 45.0], &target(|_, _| 120.0)).unwrap();
        let (mut a, mut b) = (crest(&one), crest(&other));
        a.sort_by(|p, q| p[0].total_cmp(&q[0]));
        b.sort_by(|p, q| p[0].total_cmp(&q[0]));
        assert_eq!(a.len(), b.len());
        for (p, q) in a.iter().zip(&b) {
            close(p[0], q[0], 1e-6);
            close(p[1], q[1], 1e-6);
        }
    }

    /// At a real corner two different batters still meet in a hip of their own.
    #[test]
    fn different_batters_at_a_square_corner_meet_in_a_hip() {
        let toe = string(&[[40.0, 180.0, 100.0], [40.0, 40.0, 100.0], [180.0, 40.0, 100.0]], false);
        let step = project_wall_by_edge(&toe, OffsetSide::Right, &[45.0, steep()], &target(|_, _| 120.0)).unwrap();
        let line = crest(&step);
        assert!(line.iter().any(|p| (p[0] - 20.0).abs() < 1e-6 && (p[1] - 30.0).abs() < 1e-6), "{line:?}");
        assert!(line.iter().all(|p| p[0] <= 20.0 + 1e-6 || (p[1] - 30.0).abs() < 1e-6), "no bevel: {line:?}");
    }

    /// Going round anticlockwise with the wall on the outside: 20 m out to the west and east, 10 m to the
    /// south and north.
    #[test]
    fn a_closed_toe_takes_a_batter_for_each_side() {
        let square = string(&[[100.0, 100.0, 100.0], [100.0, 60.0, 100.0], [140.0, 60.0, 100.0], [140.0, 100.0, 100.0]], true);
        let step = project_wall_by_edge(&square, OffsetSide::Right, &[45.0, steep(), 45.0, steep()], &target(|_, _| 120.0)).unwrap();
        let (mut low, mut high) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in crest(&step) {
            close(p[2], 120.0, 1e-6);
            low = [low[0].min(p[0]), low[1].min(p[1])];
            high = [high[0].max(p[0]), high[1].max(p[1])];
        }
        close(low[0], 80.0, 1e-6);
        close(high[0], 160.0, 1e-6);
        close(low[1], 50.0, 1e-6);
        close(high[1], 110.0, 1e-6);
    }

    /// The same square starting part way along its south side, so the change there comes at the end of the
    /// list, round the join: steeper to the east of it, flatter to the west.
    #[test]
    fn a_transition_can_fall_on_the_join_of_a_closed_toe() {
        let square = string(&[[120.0, 60.0, 100.0], [140.0, 60.0, 100.0], [140.0, 100.0, 100.0], [100.0, 100.0, 100.0], [100.0, 60.0, 100.0]], true);
        let step = project_wall_by_edge(&square, OffsetSide::Right, &[steep(), 45.0, 45.0, 45.0, 45.0], &target(|_, _| 120.0)).unwrap();
        let line = crest(&step);
        let on = |x: f64, y: f64| line.iter().any(|p| (p[0] - x).abs() < 1e-6 && (p[1] - y).abs() < 1e-6);
        assert!(on(120.0, 40.0) && on(130.0, 50.0), "{line:?}");
    }

    #[test]
    fn a_list_of_batters_must_match_the_string() {
        let toe = string(&[[20.0, 20.0, 100.0], [100.0, 20.0, 100.0], [180.0, 20.0, 100.0]], false);
        assert!(project_wall_by_edge(&toe, OffsetSide::Left, &[45.0], &target(|_, _| 120.0)).is_err());
        assert!(project_wall_by_edge(&toe, OffsetSide::Left, &[45.0, 90.0], &target(|_, _| 120.0)).is_err());
    }

    /// One batter for all gives what a single batter does, to the bit.
    #[test]
    fn a_batter_for_each_segment_that_are_all_the_same_change_nothing() {
        let toe = string(&[[20.0, 20.0, 100.0], [100.0, 40.0, 104.0], [180.0, 20.0, 100.0]], false);
        let surface = target(|x, y| 130.0 + 0.1 * x - 0.05 * y);
        let one = project_wall(&toe, OffsetSide::Left, 60.0, &surface).unwrap();
        let each = project_wall_by_edge(&toe, OffsetSide::Left, &[60.0, 60.0], &surface).unwrap();
        assert_eq!(one.crest.vertices, each.crest.vertices);
        assert_eq!(one.wall.triangles.len(), each.wall.triangles.len());
    }

    #[test]
    fn the_right_side_goes_the_other_way_and_keeps_the_string_direction() {
        let step = project_wall(&string(&[[20.0, 200.0, 100.0], [180.0, 200.0, 100.0]], false), OffsetSide::Right, 45.0, &target(|_, _| 120.0)).unwrap();
        let line = crest(&step);
        close(line[0][0], 20.0, 1e-3);
        close(line[0][1], 180.0, 1e-3);
        close(line[line.len() - 1][0], 180.0, 1e-3);
        close(step.wall.area(), 160.0 * 20.0 * 2f64.sqrt(), 1e-3);
    }

    #[test]
    fn corners_mitre_exactly_on_both_sides() {
        let toe = string(&[[20.0, 100.0, 100.0], [100.0, 100.0, 100.0], [100.0, 180.0, 100.0]], false);
        let flat = target(|_, _| 120.0);
        let has = |step: &WallStep, x: f64, y: f64| assert!(crest(step).iter().any(|p| (p[0] - x).abs() < 1e-3 && (p[1] - y).abs() < 1e-3), "no {x}, {y} in {:?}", crest(step));
        // The inside of the turn: the walls meet in a hip that closes in.
        has(&project_wall(&toe, OffsetSide::Left, 45.0, &flat).unwrap(), 80.0, 120.0);
        // The outside: the mitre reaches out square to both.
        has(&project_wall(&toe, OffsetSide::Right, 45.0, &flat).unwrap(), 120.0, 80.0);
    }

    #[test]
    fn a_closed_toe_rises_to_the_rectangle_outside_it() {
        let toe = string(&[[100.0, 100.0, 100.0], [300.0, 100.0, 100.0], [300.0, 250.0, 100.0], [100.0, 250.0, 100.0]], true);
        let step = project_wall(&toe, OffsetSide::Right, 45.0, &target(|_, _| 130.0)).unwrap();
        assert!(step.crest.closed);
        close(step.crest.plan_area().unwrap(), 260.0 * 210.0, 1e-3);
        // Four trapezoids, each 30 root 2 up the slope, growing by 60 m from toe to crest.
        close(step.wall.area(), (200.0 + 260.0 + 150.0 + 210.0) * 30.0 * 2f64.sqrt(), 1e-3);
    }

    #[test]
    fn a_step_between_two_dipping_planes_lands_on_their_line() {
        let a = |x: f64, y: f64| 100.0 + 0.05 * x + 0.02 * y;
        let b = |x: f64, y: f64| 160.0 + 0.03 * x - 0.01 * y;
        let y0 = 60.0;
        let toe = string(&[[20.0, y0, a(20.0, y0)], [380.0, y0, a(380.0, y0)]], false);
        let step = project_wall(&toe, OffsetSide::Left, 70.0, &target(b)).unwrap();
        let k = 70f64.to_radians().tan();
        for p in crest(&step) {
            let y = y0 + (b(p[0], y0) - a(p[0], y0)) / (k + 0.01);
            close(p[1], y, 1e-6);
            close(p[2], b(p[0], y), 1e-6);
        }
    }

    #[test]
    fn a_mitre_on_a_rising_string_meets_all_three_planes() {
        let a = |x: f64, y: f64| 100.0 + 0.05 * x + 0.02 * y;
        let toe = string(&[[40.0, 60.0, a(40.0, 60.0)], [200.0, 60.0, a(200.0, 60.0)], [320.0, 200.0, a(320.0, 200.0)]], false);
        let rl = 150.0;
        let step = project_wall(&toe, OffsetSide::Left, 60.0, &target(|_, _| rl)).unwrap();
        let k = 60f64.to_radians().tan();
        // Each wall's plane holds every crest vertex that belongs to it, and the corner holds both.
        let walls = [
            ([40.0, 60.0, a(40.0, 60.0)], [200.0, 60.0, a(200.0, 60.0)]),
            ([200.0, 60.0, a(200.0, 60.0)], [320.0, 200.0, a(320.0, 200.0)]),
        ];
        let rise = |(p, q): ([f64; 3], [f64; 3]), x: f64, y: f64| {
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let length = dx.hypot(dy);
            let (along, out) = (((x - p[0]) * dx + (y - p[1]) * dy) / length, (-(x - p[0]) * dy + (y - p[1]) * dx) / length);
            p[2] + along * (q[2] - p[2]) / length + k * out
        };
        let line = crest(&step);
        let corner = line.iter().find(|p| (rise(walls[0], p[0], p[1]) - rl).abs() < 1e-6 && (rise(walls[1], p[0], p[1]) - rl).abs() < 1e-6);
        assert!(corner.is_some(), "no vertex on both walls in {line:?}");
        for p in &line {
            close(p[2], rl, 1e-6);
        }
    }

    #[test]
    fn a_short_wall_squeezed_out_leaves_its_neighbours_meeting() {
        // Turning left twice: the middle wall, 7.07 m long, closes 8.54 m out.
        let toe = string(&[[100.0, 100.0, 100.0], [200.0, 100.0, 100.0], [205.0, 105.0, 100.0], [205.0, 205.0, 100.0]], false);
        let step = project_wall(&toe, OffsetSide::Left, 45.0, &target(|_, _| 120.0)).unwrap();
        assert_eq!(step.log.squeezed_out, 1);
        let line = crest(&step);
        assert!(line.iter().any(|p| (p[0] - 185.0).abs() < 1e-3 && (p[1] - 120.0).abs() < 1e-3), "{line:?}");
        // Short of the closing the middle wall still reaches the crest.
        let low = project_wall(&toe, OffsetSide::Left, 45.0, &target(|_, _| 105.0)).unwrap();
        assert!(crest(&low).iter().any(|p| (p[0] - 200.0 + 5.0 * (std::f64::consts::FRAC_PI_8).tan()).abs() < 1e-3 && (p[1] - 105.0).abs() < 1e-3), "{:?}", crest(&low));
    }

    #[test]
    fn walls_that_close_below_the_target_are_refused() {
        let toe = string(&[[100.0, 100.0, 100.0], [140.0, 100.0, 100.0], [120.0, 130.0, 100.0]], true);
        assert!(project_wall(&toe, OffsetSide::Left, 45.0, &target(|_, _| 150.0)).is_err());
    }

    #[test]
    fn an_undulating_target_is_met_exactly_along_a_straight_toe() {
        let t = |x: f64, y: f64| 130.0 + 8.0 * (x / 37.0).sin() + 5.0 * (y / 23.0).cos();
        let y0 = 100.0;
        let toe = string(&[[20.0, y0, 100.0], [380.0, y0, 100.0]], false);
        let surface = target(t);
        let step = project_wall(&toe, OffsetSide::Left, 70.0, &surface).unwrap();
        let k = 70f64.to_radians().tan();
        for p in crest(&step) {
            close(p[2], 100.0 + k * (p[1] - y0), 1e-6);
        }
        let bins = TriangleBins::new(&surface, [E, N], [E + 400.0, N + 400.0]);
        for p in &step.crest.vertices {
            close(bins.height(&surface, [p[0], p[1]]).unwrap(), p[2], 1e-6);
        }
        assert!(step.crest.vertices.len() > 40, "the crest follows the target's triangles");
        watertight_between_walls(&step, 1);
    }

    #[test]
    fn an_undulating_toe_rises_to_an_undulating_target() {
        let a = |x: f64, y: f64| 100.0 + 3.0 * (x / 41.0).sin() + 2.0 * (y / 29.0).cos();
        let t = |x: f64, y: f64| 130.0 + 6.0 * (x / 31.0).cos() + 4.0 * (y / 19.0).sin();
        let lower = target(a);
        let upper = target(t);
        let flat = Polyline { vertices: (0..=36).map(|i| [E + 20.0 + 10.0 * i as f64, N + 100.0 + 0.1 * i as f64, 0.0]).collect(), closed: false };
        let toe = crate::drape_string(&flat, &lower, true).unwrap().0;
        let step = project_wall(&toe, OffsetSide::Left, 70.0, &upper).unwrap();
        let bins = TriangleBins::new(&upper, [E, N], [E + 400.0, N + 400.0]);
        for p in &step.crest.vertices {
            close(bins.height(&upper, [p[0], p[1]]).unwrap(), p[2], 1e-6);
        }
        watertight_between_walls(&step, toe.vertices.len() - 1);
        // Every triangle faces up: the wall leans back from the string.
        for &t in &step.wall.triangles {
            assert!(step.wall.triangle_cross(t)[2] > 0.0, "a wall is wound downward");
        }
    }

    #[test]
    fn a_string_on_its_target_makes_no_wall_and_says_so() {
        let surface = target(|x, y| 130.0 + 6.0 * (x / 31.0).cos() + 4.0 * (y / 19.0).sin());
        let flat = Polyline { vertices: vec![[E + 20.0, N + 60.0, 0.0], [E + 200.0, N + 75.0, 0.0], [E + 380.0, N + 80.0, 0.0]], closed: false };
        let (toe, _) = drape_string(&flat, &surface, true).unwrap();
        for side in [OffsetSide::Left, OffsetSide::Right] {
            let refused = project_wall(&toe, side, 70.0, &surface).unwrap_err();
            assert!(refused.to_string().contains("no wall to make"), "{refused}");
        }
        // The same again for the line a step hands on, which is what's on the grid after a step.
        let lower = target(|_, _| 100.0);
        let flat_toe = string(&[[20.0, 60.0, 100.0], [380.0, 80.0, 100.0]], false);
        let step = project_wall(&flat_toe, OffsetSide::Left, 70.0, &surface).unwrap();
        let (toes, _) = next_toe(&step.crest, 10.0, OffsetSide::Left, 0.1, &surface).unwrap();
        assert!(project_wall(&toes[0], OffsetSide::Left, 70.0, &surface).unwrap_err().to_string().contains("no wall to make"));
        assert!(project_wall(&toes[0], OffsetSide::Left, 70.0, &lower).unwrap_err().to_string().contains("no wall to make"));
    }

    #[test]
    fn a_target_that_lifts_off_the_string_starts_its_wall_there() {
        // The string lies on the target to x = 200, and the target rises away from it after that.
        let base = |x: f64, y: f64| 100.0 + 3.0 * (y / 29.0).cos() + 0.02 * x;
        let surface = target(|x, y| base(x, y) + 0.3 * (x - 200.0).max(0.0));
        let flat = Polyline { vertices: vec![[E + 20.0, N + 60.0, 0.0], [E + 380.0, N + 60.0, 0.0]], closed: false };
        let lying = target(base);
        let (toe, _) = drape_string(&flat, &lying, true).unwrap();
        let step = project_wall(&toe, OffsetSide::Left, 70.0, &surface).unwrap();
        assert_eq!(step.log.no_height.len(), 1, "{:?}", step.log.no_height);
        close(step.log.no_height[0].from[0] - E, 20.0, 1e-6);
        close(step.log.no_height[0].to[0] - E, 200.0, 1e-6);
        let bins = TriangleBins::new(&surface, [E, N], [E + 400.0, N + 400.0]);
        for p in step.crest.vertices.iter().filter(|p| p[0] > E + 200.0) {
            close(bins.height(&surface, [p[0], p[1]]).unwrap(), p[2], 1e-6);
        }
        assert!(step.wall.triangles.iter().all(|&t| step.wall.triangle_cross(t)[2] > 0.0));
    }

    #[test]
    fn a_string_that_starts_above_the_target_starts_its_crest_on_the_string() {
        let toe = string(&[[20.0, 100.0, 100.0], [180.0, 100.0, 100.0]], false);
        // The target comes up through the string's RL at x = 40, and is above it from there on.
        let step = project_wall(&toe, OffsetSide::Left, 45.0, &target(|x, _| 90.0 + 0.25 * x)).unwrap();
        let line = crest(&step);
        close(line[0][0], 20.0, 1e-6);
        close(line[1][0], 40.0, 1e-6);
        for p in &line[..2] {
            close(p[1], 100.0, 1e-6);
            close(p[2], 100.0, 1e-6);
        }
        assert!(line[2][1] > 100.0, "the wall starts where the target rises over the string");
        assert_eq!(step.log.no_height.len(), 1);
        assert!(step.crest.vertices.windows(2).all(|w| w[0] != w[1]), "no vertex twice");
        // A wedge from a point on the string: its foot, its crest and the far end are all that's open.
        let v = step.wall.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges + v.degenerate_triangles, 0, "{v:?}");
        assert_eq!(v.boundary_edges, 1 + line.len() - 2 + 1, "{v:?}");
    }

    #[test]
    fn a_hip_on_a_climbing_string_runs_outward_even_if_downhill() {
        // Up the line at 40 %, turning left 3 degrees onto a flatter grade: the walls' planes meet in a
        // line nearly along the string, and only its outward end lies on the wall side.
        let turn = 3f64.to_radians();
        let (a, b) = ([100.0, 100.0, 100.0], [200.0, 100.0, 140.0]);
        let c = [200.0 + 100.0 * turn.cos(), 100.0 + 100.0 * turn.sin(), 140.0 + 100.0 * 0.40 + 100.0 * 0.05];
        let step = project_wall(&string(&[a, b, c], false), OffsetSide::Left, 70.0, &target(|_, _| 400.0)).unwrap();
        watertight_between_walls(&step, 2);
        for p in crest(&step) {
            close(p[2], 400.0, 1e-6);
        }
    }

    #[test]
    fn the_next_toe_keeps_the_plan_shape_and_lies_on_the_surface() {
        let t = |x: f64, y: f64| 130.0 + 6.0 * (x / 31.0).cos() + 4.0 * (y / 19.0).sin();
        let surface = target(t);
        let toe = string(&[[20.0, 100.0, 100.0], [380.0, 100.0, 100.0]], false);
        let step = project_wall(&toe, OffsetSide::Left, 70.0, &surface).unwrap();
        // A tolerance as wide as the crest's swing leaves a straight line in plan.
        let (toes, log) = next_toe(&step.crest, 0.0, OffsetSide::Left, 100.0, &surface).unwrap();
        assert_eq!(log.plan_vertices, 2);
        assert_eq!(log.off_surface, 0);
        let bins = TriangleBins::new(&surface, [E, N], [E + 400.0, N + 400.0]);
        for p in &toes[0].vertices {
            close(bins.height(&surface, [p[0], p[1]]).unwrap(), p[2], 1e-9);
        }
        assert!(toes[0].vertices.len() > 40, "the line still follows the surface");
        // With no tolerance every vertex of the plan shape stays.
        let (exact, _) = next_toe(&step.crest, 0.0, OffsetSide::Left, 0.0, &surface).unwrap();
        assert!(exact[0].vertices.len() >= step.crest.vertices.len() - 1);
    }

    #[test]
    fn a_berm_moves_the_next_toe_out_on_the_wall_side() {
        let surface = target(|_, _| 120.0);
        let toe = string(&[[20.0, 100.0, 100.0], [380.0, 100.0, 100.0]], false);
        let step = project_wall(&toe, OffsetSide::Left, 45.0, &surface).unwrap();
        let (toes, _) = next_toe(&step.crest, 10.0, OffsetSide::Left, 0.1, &surface).unwrap();
        assert_eq!(toes.len(), 1);
        for p in &toes[0].vertices {
            close(p[1] - N, 130.0, 1e-6);
            close(p[2], 120.0, 1e-9);
        }
    }

    #[test]
    fn steps_chain_through_a_berm_from_grid_to_grid() {
        let lower = target(|x, y| 100.0 + 3.0 * (x / 41.0).sin() + 2.0 * (y / 29.0).cos());
        let middle = target(|x, y| 125.0 + 4.0 * (x / 31.0).cos() + 3.0 * (y / 19.0).sin());
        let upper = target(|x, y| 150.0 + 5.0 * (x / 37.0).sin() + 3.0 * (y / 23.0).cos());
        let flat = Polyline { vertices: vec![[E + 20.0, N + 60.0, 0.0], [E + 380.0, N + 80.0, 0.0]], closed: false };
        let (toe, _) = drape_string(&flat, &lower, true).unwrap();
        let first = project_wall(&toe, OffsetSide::Left, 70.0, &middle).unwrap();
        let (toes, _) = next_toe(&first.crest, 8.0, OffsetSide::Left, 0.1, &middle).unwrap();
        let second = project_wall(&toes[0], OffsetSide::Left, 70.0, &upper).unwrap();
        watertight_between_walls(&second, toes[0].vertices.len() - 1);
        let bins = TriangleBins::new(&upper, [E, N], [E + 400.0, N + 400.0]);
        for p in &second.crest.vertices {
            close(bins.height(&upper, [p[0], p[1]]).unwrap(), p[2], 1e-6);
        }
    }

    #[test]
    fn where_the_target_dips_below_the_string_the_step_has_no_height() {
        let toe = string(&[[20.0, 100.0, 100.0], [180.0, 100.0, 100.0]], false);
        let surface = target(|x, _| 90.0 + 0.5 * (x - 100.0).abs());
        let step = project_wall(&toe, OffsetSide::Left, 45.0, &surface).unwrap();
        assert_eq!(step.log.no_height.len(), 1);
        assert!(step.crest.vertices.windows(2).all(|w| w[0] != w[1]), "no vertex twice");
        close(step.log.no_height[0].from[0] - E, 80.0, 1e-6);
        close(step.log.no_height[0].to[0] - E, 120.0, 1e-6);
        let line = crest(&step);
        assert!(line.iter().any(|p| (p[0] - 100.0).abs() > 19.9 && (p[1] - 100.0).abs() < 1e-6));
    }
}
