//! A road up the walls of a face (Phase 3). A ramp cuts back into each wall it runs along, so that stretch
//! splits in three: the lower wall stays where it was and stops at the road's pit-side edge, the road climbs
//! at its grade at full width, and the upper wall rises from the road's wall-side edge, a road width back.
//!
//! The pit-side edge is the inside edge, which holds the grade as rise over plan run. It's walked up the
//! wall one facet at a time: on a plane there's one way forward that rises at the grade, so the edge is
//! exact on any wall, however the toe rises and falls, and it can't drift. The wall is then split along it.
//!
//! The upper wall is an ordinary wall from an ordinary string, up the road's wall-side edge. A road climbs a
//! wall at an angle to its strike, so a batter measured square to the road would lean the wall above: it's
//! set so the two walls are parallel.
//!
//! Trucks need a way on. At the bottom of a road the toe behind its start moves back by the road, so the
//! wall there and the wall above the road are one plane, and the floor runs straight onto the road.
//!
//! A level bench can't hold a grade, so a road can't climb across a berm. It's the bench instead: the berm
//! narrows to nothing where the road comes up through it, and the road carries on up the next wall from
//! the same edge. Ahead of the road the berm opens out over a long taper, so the road eases across the
//! berm's width with no step in it. The walls below the road keep their berms.
//!
//! Where a road can't be built the rest of the face still is, and the run says where and why.

use std::collections::HashMap;
use std::iter::once;

use zd_core::{Polyline, TriMesh};

use crate::face::{band, place_on, stretch_of, weld, with_touches};
use crate::error::GeomError;
use crate::rule::inside_polygon;
use crate::string::{TriangleBins, cross};
use crate::wall::{WallLog, WallStep, at_crest_heights, nearest_along, plan_gap, segment_count};
use crate::{Corner, OffsetSide, drape_string, filter_string, offset_string, project_wall_by_edge};

type P3 = [f64; 3];

/// Plan distance within which a road passes through a wall's vertex (m).
const AT_VERTEX: f64 = 1e-7;
/// Plan distance under which a road's start is on a toe vertex (m).
const SAME_POINT: f64 = 1e-6;
/// A gap up to the target this small is rounding (m).
const ON_TARGET: f64 = 1e-7;
/// How far past the end of its pit-side edge a road's wall-side edge may run to come out on the surface (m).
const MOST_RUN_OUT: f64 = 200.0;
/// How often that run out reads the surface (m).
const RUN_OUT_STEP: f64 = 1.0;
/// How far a wall-side edge may leave its offset in plan, so a wall rises from a line, not every facet (m).
const WALL_EDGE_TOLERANCE: f64 = 0.01;
/// Mitres on a wall-side edge reaching farther than this many widths are bevelled.
const EDGE_MITRE_LIMIT: f64 = 4.0;
/// How many times its width a berm takes to open out ahead of a road: the road bends this gently across it.
const BERM_TAPER: f64 = 10.0;
/// How far a wall-side edge must be past the top of the road below to take up its offset again (m).
const EDGE_RESUME: f64 = 1.0;

/// A road to cut into the walls of a face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceRamp {
    /// Where it leaves the first wall's toe: the nearest point on the toe to this, in plan.
    pub start: [f64; 2],
    /// It runs back against the way the string runs.
    pub back: bool,
    /// Square to the road (m).
    pub width: f64,
    /// Rise over plan run on the pit-side edge, as a fraction.
    pub grade: f64,
    /// Where it has to come out on the crest of its last wall, if that's what's held. `start` is then worked
    /// out by the run, moving it along the toe as a planner would.
    pub ends_at: Option<[f64; 2]>,
}

/// A road a face has cut.
#[derive(Debug, Clone)]
pub struct FaceRoad {
    /// Which ramp it is, in the order the rows give them.
    pub ramp: usize,
    /// The first and last steps with a wall it climbs, from 0.
    pub steps: [usize; 2],
    /// The inside edge, from the bottom of the road to the top.
    pub pit_edge: Polyline,
    /// From the bottom of the road to the top.
    pub wall_edge: Polyline,
    /// How far along the toe it came out from its held end, if that's further than the tolerance (m).
    pub off_end: Option<f64>,
}

/// A ramp that stopped short of its stop row, where it stopped and why. Any walls it had climbed keep it.
#[derive(Debug, Clone, PartialEq)]
pub struct NoRoad {
    pub ramp: usize,
    pub step: usize,
    pub at: P3,
    pub why: String,
}

/// The top edge of a road so far, where it carries on from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoadTop {
    pub pit: P3,
    pub wall: P3,
}

/// How a stretch of road starts.
pub(crate) enum RoadBottom<'a> {
    /// At the bottom of the road. The toe behind it moves back, lying on `floor` if it lies on a surface.
    Entrance { floor: Option<&'a TriMesh> },
    /// Carrying on from the wall below, whose road's wall-side edge came in at this vertex of the toe.
    CarriedOn { wall_side: usize },
}

/// Why a road stopped, and where.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stopped {
    pub at: P3,
    pub why: String,
}

/// A wall with a road cut into it.
pub(crate) struct RoadCut {
    pub lower: TriMesh,
    pub road: TriMesh,
    pub upper: TriMesh,
    /// The foot of it all, in the string's order: round the bottom of the road at its entrance.
    pub toe: Polyline,
    /// Where the wall meets its target, in the string's order, stepping back around the road.
    pub crest: Polyline,
    pub pit_edge: Polyline,
    pub wall_edge: Polyline,
    pub upper_log: WallLog,
}

/// A line with a vertex at the nearest point to `start` in plan, its batters to match, and where that
/// vertex is.
pub(crate) fn with_road_start(line: &Polyline, batters: &[f64], start: [f64; 2]) -> (Polyline, Vec<f64>, usize) {
    let point = [start[0], start[1], 0.0];
    let segments = 0..segment_count(line);
    let along = |i: usize| nearest_along(line.vertices[i], line.vertices[i + 1], point);
    let nearest = segments.min_by(|a, b| along(*a).1.total_cmp(&along(*b).1)).expect("a wall's string has a segment");
    let (a, b, t) = (line.vertices[nearest], line.vertices[nearest + 1], along(nearest).0);
    let length = plan_gap(a, b);
    if t * length < SAME_POINT {
        return (line.clone(), batters.to_vec(), nearest);
    }
    if (1.0 - t) * length < SAME_POINT {
        return (line.clone(), batters.to_vec(), nearest + 1);
    }
    let mut vertices = line.vertices.clone();
    vertices.insert(nearest + 1, [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]);
    let mut widened = batters.to_vec();
    widened.insert(nearest, batters[nearest]);
    (Polyline { vertices, closed: false }, widened, nearest + 1)
}

/// A wall with a road cut back into it, from the toe's vertex `at` up to the target. `wall` is the plain
/// wall from that toe.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cut_road(toe: &Polyline, batters: &[f64], side: OffsetSide, ramp: &FaceRamp, at: usize, bottom: RoadBottom<'_>, wall: &WallStep, target: &TriMesh) -> Result<RoadCut, Stopped> {
    if !ramp.back {
        return cut_road_along(toe, batters, side, ramp, at, bottom, wall, target);
    }
    let last = toe.vertices.len() - 1;
    let turned = WallStep { crest: backwards(&wall.crest), wall: wall.wall.clone(), log: wall.log.clone() };
    let batters: Vec<f64> = batters.iter().rev().copied().collect();
    let other = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
    let bottom = match bottom {
        RoadBottom::CarriedOn { wall_side } => RoadBottom::CarriedOn { wall_side: last - wall_side },
        entrance => entrance,
    };
    let mut cut = cut_road_along(&backwards(toe), &batters, other, ramp, last - at, bottom, &turned, target)?;
    cut.crest.vertices.reverse();
    cut.toe.vertices.reverse();
    Ok(cut)
}

fn backwards(line: &Polyline) -> Polyline {
    Polyline { vertices: line.vertices.iter().rev().copied().collect(), closed: line.closed }
}

/// A road running the way the toe does.
#[allow(clippy::too_many_arguments)]
fn cut_road_along(toe: &Polyline, batters: &[f64], side: OffsetSide, ramp: &FaceRamp, at: usize, bottom: RoadBottom<'_>, wall: &WallStep, target: &TriMesh) -> Result<RoadCut, Stopped> {
    let start = toe.vertices[at];
    let stopped = |why: String| Stopped { at: start, why };
    let facets = Facets::of(&wall.wall, start);
    let climb = facets.climb(start, side, ramp.grade, toe, &wall.crest)?;
    let pit_edge = Polyline { vertices: climb.edge.clone(), closed: false };
    let offset = wall_edge_of(&pit_edge, ramp.width, side).map_err(|e| stopped(format!("the road's width can't be held: {e}")))?;
    let (behind, foot, mut wall_edge) = match bottom {
        RoadBottom::Entrance { floor } => {
            let behind = entrance_toe(toe, batters, at, offset.vertices[0], side, floor).map_err(|e| stopped(format!("its entrance can't be built: {e}")))?;
            let round = behind.vertices.iter().chain(&offset.vertices[..1]).chain(&toe.vertices[at..]).copied().collect();
            (behind, Polyline { vertices: round, closed: false }, offset)
        }
        RoadBottom::CarriedOn { wall_side } => {
            let behind = Behind { vertices: toe.vertices[..wall_side].to_vec(), batters: batters[..wall_side].to_vec() };
            (behind, toe.clone(), carried_on(toe.vertices[wall_side], &offset, &pit_edge, ramp.grade))
        }
    };
    run_out_onto(&mut wall_edge, ramp.grade, target).map_err(|why| Stopped { at: *pit_edge.vertices.last().expect("a climb has an end"), why })?;
    let (string, slopes) = string_up_the_road(&behind, toe, batters, &wall_edge, ramp.grade);
    let upper = project_wall_by_edge(&string, side, &slopes, target).map_err(|e| stopped(format!("the wall above it can't be built: {e}")))?;
    let road = band(&pit_edge, &road_inner_line(start, &wall_edge, &upper.crest)).map_err(|e| stopped(format!("the road can't be built: {e}")))?;
    let top = *pit_edge.vertices.last().expect("a climb has an end");
    let place = place_on(&wall.crest, top).ok_or_else(|| stopped("the road's top isn't on the crest".into()))?;
    let lower = facets.wall_below(&climb.cuts, &removed_by_road(toe, at, &pit_edge, &wall.crest, place));
    let crest = crest_round_road(&upper.crest, top, &wall.crest, place);
    Ok(RoadCut { lower, road, upper: upper.wall, toe: foot, crest, pit_edge, wall_edge, upper_log: upper.log })
}

/// The string behind a road, and a batter for each of its segments and the one that leads to the road.
struct Behind {
    vertices: Vec<P3>,
    batters: Vec<f64>,
}

/// The toe behind a road's start, moved back by the road so trucks can drive along the floor straight onto
/// it. It moves as far as the road's wall-side edge starts from the toe, so the wall there and the wall above
/// the road are one plane. It stops a road width short of that edge, and the last stretch leads up to it, so
/// a floor that isn't level at the start of the road has room to meet it.
fn entrance_toe(toe: &Polyline, batters: &[f64], at: usize, wall_start: P3, side: OffsetSide, floor: Option<&TriMesh>) -> Result<Behind, GeomError> {
    if at == 0 {
        return Ok(Behind { vertices: Vec::new(), batters: Vec::new() });
    }
    let road = Polyline { vertices: toe.vertices[..=at].to_vec(), closed: false };
    let back = (0..at).map(|i| nearest_along(road.vertices[i], road.vertices[i + 1], wall_start).1).fold(f64::INFINITY, f64::min);
    let reach = plan_gap(toe.vertices[at], wall_start);
    let moved = longest(offset_string(&road, back, side, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?);
    let short = place_at_gap(&moved, nearest_place(&moved, wall_start), false, wall_start, reach);
    if short <= 0.0 {
        return Ok(Behind { vertices: Vec::new(), batters: Vec::new() });
    }
    let line = stretch_of(&moved, 0.0, short);
    let vertices = match floor {
        Some(floor) => drape_string(&line, floor, true)?.0.vertices,
        None => line.vertices,
    };
    let batters = (0..vertices.len())
        .map(|k| {
            let (a, b) = (vertices[k], *vertices.get(k + 1).unwrap_or(&wall_start));
            batter_nearest(toe, batters, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, 0.0])
        })
        .collect();
    Ok(Behind { vertices, batters })
}

/// The wall-side edge of a stretch of road carried on from the one below: from where the edge below came in,
/// then its offset once that's past it. The road can bend at the top of a wall, and on the inside of the bend
/// the offset starts behind the edge below. The edge below can also have run on to reach its surface, so a
/// short stretch can finish with its offset still behind it, and the edge just carries straight on.
fn carried_on(from: P3, offset: &Polyline, pit_edge: &Polyline, grade: f64) -> Polyline {
    let (a, b) = (pit_edge.vertices[0], pit_edge.vertices[pit_edge.vertices.len() - 1]);
    let length = plan_gap(a, b);
    let way = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    let past = |p: &&P3| (p[0] - from[0]) * way[0] + (p[1] - from[1]) * way[1] > EDGE_RESUME;
    let ahead: Vec<P3> = offset.vertices.iter().filter(past).copied().collect();
    let on = [from[0] + way[0] * EDGE_RESUME, from[1] + way[1] * EDGE_RESUME, from[2] + grade * EDGE_RESUME];
    let ahead = if ahead.is_empty() { vec![on] } else { ahead };
    Polyline { vertices: once(from).chain(ahead).collect(), closed: false }
}

/// The line a berm runs out to, where a road comes up through it. The road is the bench there, so the line
/// comes in to the top of the road: at 45 degrees behind it, and over a long taper ahead so the road can
/// ease across the berm's width. `lay` puts each side on what the berm lies on.
pub(crate) fn berm_round_road(line: &Polyline, top: RoadTop, back: bool, lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Polyline, GeomError> {
    if back {
        let turned = berm_round_road_along(&backwards(line), top, lay)?;
        return Ok(backwards(&turned));
    }
    berm_round_road_along(line, top, lay)
}

fn berm_round_road_along(line: &Polyline, top: RoadTop, lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Polyline, GeomError> {
    let last = (line.vertices.len() - 1) as f64;
    let (at_wall, at_pit) = (nearest_place(line, top.wall), nearest_place(line, top.pit));
    let width = plan_gap(point_at(line, at_pit), top.pit);
    let behind = place_at_gap(line, at_wall, false, top.wall, std::f64::consts::SQRT_2 * width);
    let ahead = place_at_gap(line, at_pit, true, top.pit, BERM_TAPER * width);
    if behind >= ahead {
        return lay(line);
    }
    let mut vertices = if behind > 0.0 {
        let mut before = stretch_of(line, 0.0, behind);
        before.vertices.push(top.wall);
        let mut laid = lay(&before)?.vertices;
        *laid.last_mut().expect("a line has an end") = top.wall;
        laid
    } else {
        vec![top.wall]
    };
    let far = line.vertices[line.vertices.len() - 1];
    let on_past = if ahead < last { stretch_of(line, ahead, last).vertices } else { vec![far] };
    let mut laid = lay(&Polyline { vertices: once(top.pit).chain(on_past).collect(), closed: false })?.vertices;
    laid[0] = top.pit;
    vertices.extend(laid);
    Ok(Polyline { vertices, closed: false })
}

/// How far along a line a point in plan is nearest to, as a segment number and a fraction.
fn nearest_place(line: &Polyline, p: P3) -> f64 {
    let along = |i: usize| nearest_along(line.vertices[i], line.vertices[i + 1], p);
    let nearest = (0..line.vertices.len() - 1).min_by(|a, b| along(*a).1.total_cmp(&along(*b).1)).expect("a line has a segment");
    nearest as f64 + along(nearest).0
}

/// How far along a line a point in plan is nearest to, in metres from its start.
pub(crate) fn metres_along(line: &Polyline, p: P3) -> f64 {
    let place = nearest_place(line, p);
    let whole = place.floor() as usize;
    let before: f64 = line.vertices.windows(2).take(whole).map(|w| plan_gap(w[0], w[1])).sum();
    before + plan_gap(line.vertices[whole], point_at(line, place))
}

/// The point a distance in metres along a line, kept to the line's ends.
pub(crate) fn point_along(line: &Polyline, metres: f64) -> P3 {
    let mut left = metres.max(0.0);
    for w in line.vertices.windows(2) {
        let length = plan_gap(w[0], w[1]);
        if left <= length && length > 0.0 {
            let t = left / length;
            return [w[0][0] + t * (w[1][0] - w[0][0]), w[0][1] + t * (w[1][1] - w[0][1]), w[0][2] + t * (w[1][2] - w[0][2])];
        }
        left -= length;
    }
    line.vertices[line.vertices.len() - 1]
}

fn point_at(line: &Polyline, place: f64) -> P3 {
    let i = (place.floor() as usize).min(line.vertices.len() - 2);
    let (a, b, t) = (line.vertices[i], line.vertices[i + 1], place - i as f64);
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]
}

/// The place along a line, going on from `from` one way or the other, where it first stands `gap` from
/// `centre` in plan. The end of the line if it never does.
fn place_at_gap(line: &Polyline, from: f64, ahead: bool, centre: P3, gap: f64) -> f64 {
    let last = (line.vertices.len() - 1) as f64;
    let stands_off = |place: f64| plan_gap(point_at(line, place), centre) >= gap;
    let mut near = from;
    loop {
        let next = if ahead { (near.floor() + 1.0).min(last) } else { (near.ceil() - 1.0).max(0.0) };
        if stands_off(next) {
            let (mut low, mut high) = (near, next);
            for _ in 0..40 {
                let middle = (low + high) / 2.0;
                if stands_off(middle) {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            return high;
        }
        if next == if ahead { last } else { 0.0 } {
            return next;
        }
        near = next;
    }
}

/// The wall-side edge: the pit-side edge moved across the road by the width, level with it. A wall's facets
/// kink a road's edge a little at each one, and a true offset takes the folds out of the side they close
/// up on, which a mitre at every vertex wouldn't. What's left is tidied, as walls follow a line's plan shape.
fn wall_edge_of(pit_edge: &Polyline, width: f64, side: OffsetSide) -> Result<Polyline, GeomError> {
    let offset = longest(offset_string(pit_edge, width, side, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?);
    let tidied = filter_string(&Polyline { vertices: offset.vertices.iter().map(|p| [p[0], p[1], 0.0]).collect(), closed: false }, WALL_EDGE_TOLERANCE)?;
    Ok(at_crest_heights(&tidied, pit_edge))
}

fn longest(pieces: Vec<Polyline>) -> Polyline {
    pieces.into_iter().max_by(|a, b| a.plan_length().total_cmp(&b.plan_length())).expect("an offset gives at least one string")
}

/// The line round the wall side of the road: across its bottom edge and up its wall-side edge, with a vertex
/// wherever the crest above comes down to it, so the road meets the wall above with no slit between them.
fn road_inner_line(start: P3, wall_edge: &Polyline, crest: &Polyline) -> Polyline {
    let round = Polyline { vertices: once(start).chain(wall_edge.vertices.iter().copied()).collect(), closed: false };
    Polyline { vertices: with_touches(&round, crest).vertices[1..].to_vec(), closed: false }
}

/// The pit-side edge reaches the target before the wall-side edge does where the target rises behind it:
/// the edge is carried straight on at its grade until it comes out on the target too.
fn run_out_onto(edge: &mut Polyline, grade: f64, target: &TriMesh) -> Result<(), String> {
    let n = edge.vertices.len();
    let (before, end) = (edge.vertices[n - 2], edge.vertices[n - 1]);
    let length = plan_gap(before, end);
    let way = [(end[0] - before[0]) / length, (end[1] - before[1]) / length];
    let far = [end[0] + way[0] * MOST_RUN_OUT, end[1] + way[1] * MOST_RUN_OUT];
    let bins = TriangleBins::new(target, [end[0].min(far[0]), end[1].min(far[1])], [end[0].max(far[0]), end[1].max(far[1])]);
    let at = |d: f64| [end[0] + way[0] * d, end[1] + way[1] * d, end[2] + grade * d];
    let above = |d: f64| {
        let p = at(d);
        bins.height(target, [p[0], p[1]]).map(|height| height - p[2])
    };
    if above(0.0).is_none_or(|gap| gap <= ON_TARGET) {
        return Ok(());
    }
    let mut below = 0.0;
    while below < MOST_RUN_OUT {
        let next = below + RUN_OUT_STEP;
        match above(next) {
            None => return Ok(()),
            Some(gap) if gap <= 0.0 => {
                let (mut low, mut high) = (below, next);
                for _ in 0..40 {
                    let middle = (low + high) / 2.0;
                    if above(middle).is_some_and(|gap| gap > 0.0) {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                edge.vertices.push(at(high));
                return Ok(());
            }
            Some(_) => below = next,
        }
    }
    Err(format!("the road doesn't come out on the surface: the surface rises as fast as the road for {MOST_RUN_OUT} m past here"))
}

/// The string the upper wall rises from, and its batters: the string behind the road, then up its wall-side
/// edge.
fn string_up_the_road(behind: &Behind, toe: &Polyline, batters: &[f64], wall_edge: &Polyline, grade: f64) -> (Polyline, Vec<f64>) {
    let mut vertices = behind.vertices.clone();
    vertices.extend_from_slice(&wall_edge.vertices);
    let mut slopes = behind.batters.clone();
    slopes.extend(wall_edge.vertices.windows(2).map(|w| {
        let middle = [(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0, 0.0];
        upper_batter(batter_nearest(toe, batters, middle), grade)
    }));
    (Polyline { vertices, closed: false }, slopes)
}

/// The batter at the toe segment nearest a point in plan.
fn batter_nearest(toe: &Polyline, batters: &[f64], p: P3) -> f64 {
    let gap = |i: usize| nearest_along(toe.vertices[i], toe.vertices[i + 1], p).1;
    let nearest = (0..batters.len()).min_by(|a, b| gap(*a).total_cmp(&gap(*b))).expect("a wall has a batter");
    batters[nearest]
}

/// The batter square to a road climbing a wall at `grade` that gives a wall parallel to the one below. The
/// road's rise is part of that wall's slope, and what's left of it is square to the road.
fn upper_batter(batter: f64, grade: f64) -> f64 {
    let slope = batter.to_radians().tan();
    (slope * slope - grade * grade).max(0.0).sqrt().atan().to_degrees()
}

/// What the road takes away from the plain wall, as an outline in plan: back along the toe from the road's
/// start, up the wall's end, along the crest and down the pit-side edge.
fn removed_by_road(toe: &Polyline, at: usize, pit_edge: &Polyline, crest: &Polyline, place: f64) -> Polyline {
    let mut vertices = toe.vertices[..=at].to_vec();
    vertices.extend_from_slice(&pit_edge.vertices[1..]);
    if place > 0.0 {
        vertices.extend(stretch_of(crest, 0.0, place).vertices.iter().rev().skip(1));
    }
    Polyline { vertices, closed: true }
}

/// The crest of a wall with a road through it: the upper wall's crest, across the top of the road to the
/// pit-side edge, and on along the plain crest.
fn crest_round_road(upper: &Polyline, top: P3, plain: &Polyline, place: f64) -> Polyline {
    let mut vertices = upper.vertices.clone();
    vertices.push(top);
    let last = (plain.vertices.len() - 1) as f64;
    if place < last {
        vertices.extend(stretch_of(plain, place, last).vertices.into_iter().skip(1));
    }
    vertices.dedup_by(|a, b| plan_gap(*a, *b) < SAME_POINT);
    Polyline { vertices, closed: false }
}

/// A road's way across one facet of a wall.
#[derive(Debug, Clone, Copy)]
struct Cut {
    triangle: usize,
    from: P3,
    to: P3,
}

struct Climb {
    edge: Vec<P3>,
    cuts: Vec<Cut>,
}

/// Where a climb has got to.
#[derive(Clone, Copy)]
enum At {
    Vertex(u32),
    Into(usize),
}

/// A wall's facets, about a local origin, each anticlockwise in plan.
struct Facets {
    origin: P3,
    mesh: TriMesh,
    local: Vec<P3>,
    sharing: HashMap<[u32; 2], Vec<usize>>,
    round: Vec<Vec<usize>>,
}

impl Facets {
    fn of(wall: &TriMesh, near: P3) -> Self {
        let mut mesh = weld(once(wall));
        let origin = near.map(f64::round);
        let local: Vec<P3> = mesh.vertices.iter().map(|p| [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]]).collect();
        let mut sharing: HashMap<[u32; 2], Vec<usize>> = HashMap::new();
        let mut round = vec![Vec::new(); local.len()];
        for (t, triangle) in mesh.triangles.iter_mut().enumerate() {
            let [a, b, c] = triangle.map(|v| local[v as usize]);
            if cross([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]) < 0.0 {
                triangle.swap(1, 2);
            }
            let [a, b, c] = *triangle;
            for [u, v] in [[a, b], [b, c], [c, a]] {
                sharing.entry([u.min(v), u.max(v)]).or_default().push(t);
                round[u as usize].push(t);
            }
        }
        Facets { origin, mesh, local, sharing, round }
    }

    fn world(&self, p: P3) -> P3 {
        [p[0] + self.origin[0], p[1] + self.origin[1], p[2] + self.origin[2]]
    }

    /// The vertex at a point in plan, if the wall has one there.
    fn vertex_at(&self, p: P3) -> Option<u32> {
        let at = [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]];
        (0..self.local.len()).find(|&v| plan_gap(self.local[v], at) < SAME_POINT).map(|v| v as u32)
    }

    /// The way forward in plan across a facet that rises at `grade`, with the wall rising on `side`. None
    /// if the facet is flatter than that.
    fn way_up(&self, triangle: usize, side: OffsetSide, grade: f64) -> Option<[f64; 2]> {
        let [a, b, c] = self.mesh.triangles[triangle].map(|v| self.local[v as usize]);
        let (ab, ac) = (sub(b, a), sub(c, a));
        let area = cross([ab[0], ab[1]], [ac[0], ac[1]]);
        if area <= 0.0 {
            return None;
        }
        let gradient = [(ab[2] * ac[1] - ac[2] * ab[1]) / area, (ab[0] * ac[2] - ac[0] * ab[2]) / area];
        let steepest = gradient[0].hypot(gradient[1]);
        if steepest <= grade {
            return None;
        }
        let up = [gradient[0] / steepest, gradient[1] / steepest];
        let ahead = if side == OffsetSide::Left { [up[1], -up[0]] } else { [-up[1], up[0]] };
        let (rise, run) = (grade / steepest, (1.0 - (grade / steepest).powi(2)).sqrt());
        Some([rise * up[0] + run * ahead[0], rise * up[1] + run * ahead[1]])
    }

    /// The facet round a vertex that the climb goes on across, and its way up.
    fn leaving(&self, vertex: u32, side: OffsetSide, grade: f64) -> Option<(usize, [f64; 2])> {
        self.round[vertex as usize].iter().find_map(|&t| {
            let way = self.way_up(t, side, grade)?;
            let [a, b, c] = self.mesh.triangles[t];
            let [next, after] = if a == vertex {
                [b, c]
            } else if b == vertex {
                [c, a]
            } else {
                [a, b]
            };
            let v = self.local[vertex as usize];
            let (to_next, to_after) = (plan_unit(sub(self.local[next as usize], v)), plan_unit(sub(self.local[after as usize], v)));
            (cross(to_next, way) >= -1e-12 && cross(way, to_after) >= -1e-12).then_some((t, way))
        })
    }

    /// Where a way across a facet from `p` leaves it, and the edge it leaves over.
    fn exit(&self, triangle: usize, p: P3, way: [f64; 2]) -> Option<([u32; 2], P3)> {
        let [a, b, c] = self.mesh.triangles[triangle];
        let mut soonest: Option<(f64, [u32; 2], f64)> = None;
        for [u, v] in [[a, b], [b, c], [c, a]] {
            let (from, to) = (self.local[u as usize], self.local[v as usize]);
            let edge = [to[0] - from[0], to[1] - from[1]];
            let leaving = cross(way, edge);
            if leaving <= 1e-14 * edge[0].hypot(edge[1]) {
                continue;
            }
            let reach = (cross([from[0] - p[0], from[1] - p[1]], edge) / leaving).max(0.0);
            if soonest.is_none_or(|s| reach < s.0) {
                let hit = [p[0] + way[0] * reach - from[0], p[1] + way[1] * reach - from[1]];
                let along = ((hit[0] * edge[0] + hit[1] * edge[1]) / (edge[0] * edge[0] + edge[1] * edge[1])).clamp(0.0, 1.0);
                soonest = Some((reach, [u, v], along));
            }
        }
        let (_, [u, v], t) = soonest?;
        let (from, to) = (self.local[u as usize], self.local[v as usize]);
        Some(([u, v], [from[0] + t * (to[0] - from[0]), from[1] + t * (to[1] - from[1]), from[2] + t * (to[2] - from[2])]))
    }

    /// The pit-side edge, from a vertex on the toe up to the crest.
    fn climb(&self, start: P3, side: OffsetSide, grade: f64, toe: &Polyline, crest: &Polyline) -> Result<Climb, Stopped> {
        let no_wall = || Stopped { at: start, why: "there's no wall at its start".into() };
        let first = self.vertex_at(start).ok_or_else(no_wall)?;
        let mut p = self.local[first as usize];
        let mut at = At::Vertex(first);
        let mut cuts: Vec<Cut> = Vec::new();
        for _ in 0..8 * self.mesh.triangles.len() + 64 {
            let leaving = match at {
                At::Vertex(v) => self.leaving(v, side, grade),
                At::Into(t) => self.way_up(t, side, grade).map(|way| (t, way)),
            };
            let Some((triangle, way)) = leaving else { return self.ended(p, cuts, grade, toe, crest) };
            let Some(([u, v], hit)) = self.exit(triangle, p, way) else { break };
            let corner = [u, v].into_iter().find(|w| plan_gap(self.local[*w as usize], hit) < AT_VERTEX);
            let to = corner.map_or(hit, |w| self.local[w as usize]);
            cuts.push(Cut { triangle, from: p, to });
            p = to;
            if let Some(w) = corner {
                at = At::Vertex(w);
                continue;
            }
            let across = self.sharing[&[u.min(v), u.max(v)]].iter().copied().find(|&t| t != triangle);
            match across {
                Some(t) => at = At::Into(t),
                None => return self.ended(p, cuts, grade, toe, crest),
            }
        }
        Err(Stopped { at: self.world(p), why: "it can't find a way up the wall here".into() })
    }

    /// Where a climb has stopped: the crest it was going for, or somewhere it can't go on from.
    fn ended(&self, p: P3, cuts: Vec<Cut>, grade: f64, toe: &Polyline, crest: &Polyline) -> Result<Climb, Stopped> {
        let at = self.world(p);
        if place_on(crest, at).is_some() {
            let mut edge: Vec<P3> = once(cuts.first().map_or(p, |c| c.from)).chain(cuts.iter().map(|c| c.to)).map(|q| self.world(q)).collect();
            edge.dedup_by(|a, b| plan_gap(*a, *b) < AT_VERTEX);
            return Ok(Climb { edge, cuts });
        }
        let why = if cuts.is_empty() {
            format!("it can't leave the toe: the toe rises faster than {:.1} % here, or the wall is flatter than that", grade * 100.0)
        } else if place_on(toe, at).is_some() {
            format!("it drops back to the toe at RL {:.1}: the toe rises faster than {:.1} % here", at[2], grade * 100.0)
        } else {
            format!("it runs out of wall at RL {:.1}, short of the top of the wall", at[2])
        };
        Err(Stopped { at, why })
    }

    /// The part of the wall left below a road: every facet outside what the road removes, split along the
    /// road where it crosses one.
    fn wall_below(&self, cuts: &[Cut], removed: &Polyline) -> TriMesh {
        let by_facet: HashMap<usize, Cut> = cuts.iter().map(|c| (c.triangle, *c)).collect();
        let mut below = TriMesh::default();
        let mut keep = |corners: [P3; 3]| {
            let middle = [(corners[0][0] + corners[1][0] + corners[2][0]) / 3.0, (corners[0][1] + corners[1][1] + corners[2][1]) / 3.0, 0.0];
            if !inside_polygon(removed, middle) {
                let first = below.vertices.len() as u32;
                below.vertices.extend(corners);
                below.triangles.push([first, first + 1, first + 2]);
            }
        };
        for (t, triangle) in self.mesh.triangles.iter().enumerate() {
            match by_facet.get(&t) {
                None => keep(triangle.map(|v| self.mesh.vertices[v as usize])),
                Some(cut) => {
                    for piece in self.pieces_of(t, cut) {
                        keep(piece.map(|p| self.world(p)));
                    }
                }
            }
        }
        below
    }

    /// A facet cut in two along a road, as triangles.
    fn pieces_of(&self, triangle: usize, cut: &Cut) -> Vec<[P3; 3]> {
        let corners = self.mesh.triangles[triangle].map(|v| self.local[v as usize]);
        let (from, to) = (round_facet(&corners, cut.from), round_facet(&corners, cut.to));
        let mut pieces = Vec::new();
        for (a, b, start, end) in [(cut.from, cut.to, from, to), (cut.to, cut.from, to, from)] {
            let mut outline = vec![a];
            let span = (end - start).rem_euclid(3.0);
            for k in 1..=3 {
                let off = start.floor() + k as f64 - start;
                if off > 1e-12 && off < span - 1e-12 {
                    outline.push(corners[(start.floor() as usize + k) % 3]);
                }
            }
            outline.push(b);
            for k in 1..outline.len() - 1 {
                let [p, q, r] = [outline[0], outline[k], outline[k + 1]];
                if cross([q[0] - p[0], q[1] - p[1]], [r[0] - p[0], r[1] - p[1]]) > 0.0 {
                    pieces.push([p, q, r]);
                }
            }
        }
        pieces
    }
}

/// How far round a facet's edge a point on it is, as an edge number and a fraction.
fn round_facet(corners: &[P3; 3], p: P3) -> f64 {
    let along = |e: usize| nearest_along(corners[e], corners[(e + 1) % 3], p);
    let edge = (0..3).min_by(|a, b| along(*a).1.total_cmp(&along(*b).1)).expect("three edges");
    (edge as f64 + along(edge).0).rem_euclid(3.0)
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn plan_unit(a: P3) -> [f64; 2] {
    let length = a[0].hypot(a[1]);
    [a[0] / length, a[1] / length]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face::{END_TOLERANCE, Face, FaceStep, FaceTarget, project_face};
    use crate::{EdgeRule, find_crossings, triangulate_grid};
    use zd_core::{FaceDirection, Grid};

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;
    const FLOOR: f64 = 100.0;
    /// cos of the angle a 10 % road climbs a 45 degree wall at, where sin(angle) = grade / slope.
    const COS: f64 = 0.994_987_437_106_62;
    const RAMP: FaceRamp = FaceRamp { start: [E + 100.0, N], back: false, width: 10.0, grade: 0.1, ends_at: None };

    fn level(rl: f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |_, _| rl))
    }

    fn string(points: &[[f64; 2]], rl: f64) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [E + p[0], N + p[1], rl]).collect(), closed: false }
    }

    /// A wall the road climbs.
    fn wall(to: FaceTarget<'_>, batter: f64) -> FaceStep<'_> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(batter), condition: None, ramp: Some(0) }
    }

    fn plain_wall(to: FaceTarget<'_>, batter: f64) -> FaceStep<'_> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(batter), condition: None, ramp: None }
    }

    fn berm<'a>(on: FaceTarget<'a>, width: f64) -> FaceStep<'a> {
        FaceStep::Berm { on: Some(on), width: EdgeRule::Fixed(width), register: true, condition: None, min_bench: None }
    }

    /// A face from a level toe along +x, rising to the north.
    fn ramped(ramp: FaceRamp, steps: Vec<FaceStep<'_>>, floor: &TriMesh) -> Face {
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        project_face(&toe, floor, OffsetSide::Left, FaceDirection::Up, &steps, &[ramp]).unwrap()
    }

    fn wall_to(rl: f64, batter: f64, ramp: FaceRamp) -> Face {
        ramped(ramp, vec![wall(FaceTarget::Rl(rl), batter)], &level(FLOOR))
    }

    /// The height of a sheet at a point in plan, if it covers it.
    fn height_at(sheet: &TriMesh, x: f64, y: f64) -> Option<f64> {
        TriangleBins::new(sheet, [E - 100.0, N - 100.0], [E + 800.0, N + 300.0]).height(sheet, [E + x, N + y])
    }

    fn height(sheet: &TriMesh, x: f64, y: f64) -> f64 {
        height_at(sheet, x, y).unwrap_or_else(|| panic!("nothing at {x}, {y}"))
    }

    fn rise_over_run(line: &Polyline) -> Vec<f64> {
        line.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6).map(|w| (w[1][2] - w[0][2]) / plan_gap(w[0], w[1])).collect()
    }

    fn one_clean_sheet(face: &Face) {
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "one boundary round the sheet");
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0, "nothing crossing");
    }

    /// A road at 10 % up a 45 degree wall 20 m high, 10 m wide. Rise over plan run gives 200 m of road. It
    /// climbs at sin(angle) = 0.1 off the wall's strike, so it finishes 198.997 m along and 20 m out, where
    /// the wall's height is used up. Width is square to the road.
    #[test]
    fn a_road_up_a_straight_wall_holds_its_grade_and_its_width() {
        let face = wall_to(120.0, 45.0, RAMP);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        let (pit, wall) = (&road.pit_edge, &road.wall_edge);
        assert!((pit.plan_length() - 200.0).abs() < 1e-3, "{}", pit.plan_length());
        for grade in rise_over_run(pit).into_iter().chain(rise_over_run(wall)) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        let (bottom, top) = (pit.vertices[0], pit.vertices[pit.vertices.len() - 1]);
        assert_eq!([bottom[0] - E, bottom[1] - N, bottom[2]], [100.0, 0.0, FLOOR]);
        let across = 200.0 * COS;
        assert!((top[0] - E - (100.0 + across)).abs() < 1e-3 && (top[1] - N - 20.0).abs() < 1e-3 && (top[2] - 120.0).abs() < 1e-3, "{top:?}");
        for (p, q) in [(bottom, wall.vertices[0]), (top, wall.vertices[wall.vertices.len() - 1])] {
            assert!((plan_gap(p, q) - 10.0).abs() < 1e-6 && (p[2] - q[2]).abs() < 1e-6, "{p:?} {q:?}");
        }
    }

    /// Half way up the road is at RL 110, 10 m out from the toe. A section square to the road reads wall,
    /// road, wall: the road is level across, and the wall above carries on at the same batter.
    #[test]
    fn a_section_through_a_road_reads_wall_road_wall() {
        let face = wall_to(120.0, 45.0, RAMP);
        let sheet = &face.surface;
        let edge = [100.0 + 100.0 * COS, 10.0];
        let over = |reach: f64| [edge[0] - 0.1 * reach, edge[1] + COS * reach];
        assert!((height(sheet, edge[0], 5.0) - 105.0).abs() < 1e-6, "the lower wall hasn't moved");
        for reach in [0.5, 5.0, 9.5] {
            let [x, y] = over(reach);
            assert!((height(sheet, x, y) - 110.0).abs() < 1e-6, "level across the road at {reach}: {}", height(sheet, x, y));
        }
        let [x, y] = over(10.0);
        assert!((height(sheet, x, y + 4.0) - 114.0).abs() < 1e-6, "the same batter behind the road: {}", height(sheet, x, y + 4.0));
    }

    /// The toe behind the road moves back as far as the road's wall-side edge starts from it, 10 cos(angle)
    /// square to the wall, so behind the road and above it are one plane and the crest is one straight line
    /// from the end of the string to the top of the road. A batter measured square to the road would lean
    /// it by about 10 cm here. Past the road the crest is where it was.
    #[test]
    fn the_wall_behind_a_road_and_above_it_are_one_plane() {
        let face = wall_to(120.0, 45.0, RAMP);
        let crest = &face.benches[0].crest;
        let top = face.roads[0].pit_edge.vertices.last().unwrap()[0] - E;
        let back = 20.0 + 10.0 * COS;
        let along: Vec<&[f64; 3]> = crest.vertices.iter().filter(|p| p[1] - N > 25.0).collect();
        assert_eq!(along[0][0] - E, 0.0, "from the end of the string: {:?}", crest.vertices);
        assert!(along[along.len() - 1][0] - E > top - 2.0, "to the top of the road: {:?}", crest.vertices);
        for p in along {
            assert!((p[1] - N - back).abs() < 1e-6, "{p:?} against {back}");
        }
        let past = crest.vertices.iter().find(|p| p[0] - E > 320.0).unwrap();
        assert!((past[1] - N - 20.0).abs() < 1e-6, "{past:?}");
    }

    /// Behind the road the floor runs on where the toe was, with no wall on it, straight onto the road.
    #[test]
    fn an_entrance_leaves_the_floor_open_behind_a_road() {
        let face = wall_to(120.0, 45.0, RAMP);
        assert_eq!(height_at(&face.surface, 50.0, 5.0), None, "no wall where the toe was");
        assert!((height(&face.surface, 50.0, 10.0 * COS + 2.0) - 102.0).abs() < 1e-6, "the wall stands behind it");
        let toe = &face.benches[0].toe;
        assert!(toe.vertices.iter().filter(|p| p[0] - E < 90.0).all(|p| (p[1] - N - 10.0 * COS).abs() < 1e-6), "{:?}", toe.vertices);
        assert!(toe.vertices.iter().any(|p| plan_gap(*p, [E + 100.0, N, 0.0]) < 1e-9), "the toe comes round to the road's start");
    }

    /// Walls and road cover the plan between the toe and the crest once each, to a square metre in a
    /// million: a gap or an overlap anywhere would show here.
    #[test]
    fn a_road_leaves_no_gap_or_overlap_in_plan() {
        for ramp in [RAMP, FaceRamp { start: [E + 400.0, N], back: true, ..RAMP }] {
            let face = wall_to(120.0, 45.0, ramp);
            let (sheet, bench) = (&face.surface, &face.benches[0]);
            let area = |t: &[u32; 3]| {
                let [a, b, c] = t.map(|v| sheet.vertices[v as usize]);
                cross([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]) / 2.0
            };
            let covered: f64 = sheet.triangles.iter().map(area).sum();
            assert!(sheet.triangles.iter().all(|t| area(t) > 0.0), "every facet faces up");
            let outline: Vec<P3> = bench.toe.vertices.iter().chain(bench.crest.vertices.iter().rev()).copied().collect();
            let about = outline[0];
            let enclosed: f64 = (0..outline.len())
                .map(|k| {
                    let (p, q) = (outline[k], outline[(k + 1) % outline.len()]);
                    cross([p[0] - about[0], p[1] - about[1]], [q[0] - about[0], q[1] - about[1]]) / 2.0
                })
                .sum();
            assert!((covered - enclosed).abs() < 1e-6 * enclosed, "{covered} against {enclosed}");
        }
    }

    #[test]
    fn a_road_makes_one_sheet_with_nothing_crossing() {
        let face = wall_to(120.0, 70.0, RAMP);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
    }

    #[test]
    fn a_road_can_run_back_against_the_string() {
        let face = wall_to(120.0, 45.0, FaceRamp { start: [E + 400.0, N], back: true, ..RAMP });
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let pit = &face.roads[0].pit_edge;
        let top = pit.vertices[pit.vertices.len() - 1];
        assert!((top[0] - E - (400.0 - 200.0 * COS)).abs() < 1e-3 && (top[2] - 120.0).abs() < 1e-3, "{top:?}");
        one_clean_sheet(&face);
        // Behind this road is the far end of the string.
        assert!((face.benches[0].toe.vertices.last().unwrap()[1] - N - 10.0 * COS).abs() < 1e-6);
    }

    #[test]
    fn a_road_works_with_the_wall_on_the_right() {
        let toe = string(&[[600.0, 0.0], [0.0, 0.0]], FLOOR);
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Right, FaceDirection::Up, &[wall(FaceTarget::Rl(120.0), 45.0)], &[FaceRamp { start: [E + 500.0, N], ..RAMP }]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let top = *face.roads[0].pit_edge.vertices.last().unwrap();
        assert!((top[1] - N - 20.0).abs() < 1e-3, "the wall rises north of a string running west: {top:?}");
        one_clean_sheet(&face);
    }

    /// Too little string for the road's length: nothing is built past where it stops, and the wall stands.
    #[test]
    fn a_road_that_runs_out_of_wall_says_where_and_the_wall_still_builds() {
        let face = wall_to(120.0, 45.0, FaceRamp { start: [E + 500.0, N], ..RAMP });
        assert!(face.roads.is_empty());
        let stopped = &face.no_roads[0];
        assert!(stopped.why.contains("runs out of wall"), "{}", stopped.why);
        assert!((stopped.at[0] - E - 600.0).abs() < 1e-6, "it stops at the end of the string: {:?}", stopped.at);
        assert!((stopped.at[2] - (FLOOR + 0.1 * 100.0 / COS)).abs() < 1e-6, "100 m along at 10 %: {:?}", stopped.at);
        assert!((height(&face.surface, 300.0, 10.0) - 110.0).abs() < 1e-6, "the wall is the plain wall");
    }

    /// A wall flatter than the road's grade has no way up at that grade.
    #[test]
    fn a_wall_flatter_than_the_grade_is_named() {
        let face = wall_to(105.0, 20.0, FaceRamp { grade: 0.5, ..RAMP });
        assert!(face.roads.is_empty());
        assert!(face.no_roads[0].why.contains("can't leave the toe"), "{}", face.no_roads[0].why);
    }

    /// A toe climbing at 15 % outruns a road at 10 %, which can never get above it.
    #[test]
    fn a_toe_climbing_faster_than_the_road_is_named() {
        let toe = Polyline { vertices: vec![[E, N, 100.0], [E + 600.0, N, 190.0]], closed: false };
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 20, 20, |x, _| 100.0 + 0.15 * (x - E)));
        let face = project_face(&toe, &floor, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Rl(300.0), 45.0)], &[RAMP]).unwrap();
        assert!(face.roads.is_empty(), "{:?}", face.roads);
        assert!(face.no_roads[0].why.contains("can't leave the toe"), "{}", face.no_roads[0].why);
    }

    /// The target rises behind the wall, so the wall-side edge has further to go than the pit-side edge,
    /// and carries on at the same grade until it comes out on the surface.
    #[test]
    fn a_road_comes_out_on_a_surface_that_rises_behind_the_wall() {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [50.0, 50.0], 40, 40, |_, y| 120.0 + 0.05 * (y - N)));
        let face = ramped(RAMP, vec![wall(FaceTarget::Surface(&target), 45.0)], &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        for grade in rise_over_run(&road.pit_edge).into_iter().chain(rise_over_run(&road.wall_edge)) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        let end = *road.wall_edge.vertices.last().unwrap();
        let bins = TriangleBins::new(&target, [E - 100.0, N - 100.0], [E + 800.0, N + 300.0]);
        assert!((bins.height(&target, [end[0], end[1]]).unwrap() - end[2]).abs() < 1e-6, "it finishes on the surface: {end:?}");
        one_clean_sheet(&face);
    }

    /// The target falls away behind the wall, so the crest comes down to the road's wall-side edge part way
    /// up it. The road and the wall above still meet edge to edge, with one boundary round the sheet.
    #[test]
    fn a_road_meets_a_crest_that_comes_down_to_it() {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [50.0, 50.0], 40, 40, |_, y| 120.0 - 0.3 * (y - N - 15.0).max(0.0)));
        let face = ramped(RAMP, vec![wall(FaceTarget::Surface(&target), 45.0)], &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
    }

    /// A toe that rises and falls makes a wall of many planes, and the road holds one grade across them.
    #[test]
    fn a_road_holds_its_grade_over_a_toe_that_rises_and_falls() {
        let bumps = |x: f64| 100.0 + 2.0 * (x / 40.0).sin();
        let toe = Polyline { vertices: (0..=60).map(|k| [E + k as f64 * 10.0, N, bumps(k as f64 * 10.0)]).collect(), closed: false };
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| bumps(x - E)));
        let face = project_face(&toe, &floor, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Rl(125.0), 60.0)], &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let pit = &face.roads[0].pit_edge;
        for grade in rise_over_run(pit) {
            assert!((grade - 0.1).abs() < 1e-6, "{grade}");
        }
        assert!((pit.vertices.last().unwrap()[2] - 125.0).abs() < 1e-6);
        one_clean_sheet(&face);
    }

    /// Two walls and a berm: the road goes up the first, the berm narrows to nothing where it comes through,
    /// and it carries on up the second from the same edge. 40 m of rise at 10 % is 400 m of road, in one
    /// grade from bottom to top, and the berm stands again ahead of the road.
    #[test]
    fn a_road_carries_across_a_berm_and_up_the_next_wall() {
        let on = FaceTarget::Rl(120.0);
        let face = ramped(RAMP, vec![wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Rl(140.0), 45.0)], &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        assert_eq!(face.roads.len(), 1);
        assert_eq!(road.steps, [0, 2]);
        let pit = &road.pit_edge;
        assert!((pit.plan_length() - 400.0).abs() < 1e-3, "{}", pit.plan_length());
        for grade in rise_over_run(pit) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        assert!((pit.vertices.last().unwrap()[2] - 140.0).abs() < 1e-6);
        one_clean_sheet(&face);
        assert!((height(&face.surface, 590.0, 25.0) - 120.0).abs() < 1e-6, "the berm is there ahead of the road");
        assert!((height(&face.surface, 200.0, 20.0 + 10.0 * COS + 5.0) - 120.0).abs() < 1e-6, "and behind it");
    }

    /// A road bends easily across a berm: nowhere does it turn more than the taper's angle.
    #[test]
    fn a_road_eases_across_a_berm_with_no_step_in_it() {
        let on = FaceTarget::Rl(120.0);
        let face = ramped(RAMP, vec![wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Rl(140.0), 45.0)], &level(FLOOR));
        let heading = |w: &[[f64; 3]]| (w[1][1] - w[0][1]).atan2(w[1][0] - w[0][0]);
        let turns = face.roads[0].pit_edge.vertices.windows(3).map(|w| (heading(&w[1..]) - heading(&w[..2])).abs().to_degrees());
        assert!(turns.clone().fold(0.0, f64::max) < 6.0, "{:?}", turns.collect::<Vec<_>>());
    }

    #[test]
    fn a_road_climbs_three_walls_with_the_wall_on_the_right_going_back() {
        let toe = string(&[[0.0, 0.0], [900.0, 0.0]], FLOOR);
        let (one, two) = (FaceTarget::Rl(115.0), FaceTarget::Rl(130.0));
        let rows = [wall(one, 60.0), berm(one, 10.0), wall(two, 60.0), berm(two, 10.0), wall(FaceTarget::Rl(145.0), 60.0)];
        let ramp = FaceRamp { start: [E + 800.0, N], back: true, ..RAMP };
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Right, FaceDirection::Up, &rows, &[ramp]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads[0].steps, [0, 4]);
        assert!((face.roads[0].pit_edge.plan_length() - 450.0).abs() < 1e-3, "{}", face.roads[0].pit_edge.plan_length());
        one_clean_sheet(&face);
    }

    /// A road that can't make the next wall stops at the last one it could, and says so by step and place.
    #[test]
    fn a_road_stops_at_the_last_wall_it_can_make() {
        let toe = string(&[[0.0, 0.0], [350.0, 0.0]], FLOOR);
        let on = FaceTarget::Rl(120.0);
        let rows = [wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Rl(160.0), 45.0)];
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[RAMP]).unwrap();
        assert_eq!(face.roads[0].steps, [0, 0], "{:?}", face.no_roads);
        assert_eq!(face.no_roads[0].step, 2);
        assert!(face.no_roads[0].why.contains("runs out of wall"), "{}", face.no_roads[0].why);
        assert_eq!(face.benches.len(), 2);
        one_clean_sheet(&face);
    }

    /// Past its last wall a road's done: the next berm follows the crest round its top.
    #[test]
    fn a_berm_and_a_wall_carry_on_above_a_road() {
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        let on = FaceTarget::Rl(120.0);
        let rows = [wall(on, 45.0), berm(on, 10.0), plain_wall(FaceTarget::Rl(140.0), 45.0)];
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        // Beside the road the next wall starts a road width and a berm behind the plain crest.
        let back = 20.0 + 10.0 * COS + 10.0;
        assert!((height(&face.surface, 200.0, back + 5.0) - 125.0).abs() < 1e-6, "{}", height(&face.surface, 200.0, back + 5.0));
    }

    /// Two ramps in one profile, one above the other.
    #[test]
    fn a_profile_can_hold_two_ramps() {
        let toe = string(&[[0.0, 0.0], [900.0, 0.0]], FLOOR);
        let on = FaceTarget::Rl(120.0);
        let above = FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(45.0), condition: None, ramp: Some(1) };
        let rows = [wall(on, 45.0), berm(on, 10.0), above];
        let ramps = [RAMP, FaceRamp { start: [E + 600.0, N + 30.0], ..RAMP }];
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &ramps).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads.iter().map(|r| (r.ramp, r.steps)).collect::<Vec<_>>(), [(0, [0, 0]), (1, [2, 2])]);
        one_clean_sheet(&face);
    }

    /// Two rows that would be one wall are two where the ramp changes, so a road can finish part way up.
    #[test]
    fn a_road_can_finish_at_a_row_part_way_up_one_batter() {
        let (lower, upper) = (wall(FaceTarget::Rl(120.0), 45.0), plain_wall(FaceTarget::Rl(140.0), 45.0));
        let face = ramped(RAMP, vec![lower, upper], &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads[0].steps, [0, 0]);
        assert!((face.roads[0].pit_edge.vertices.last().unwrap()[2] - 120.0).abs() < 1e-6);
        assert_eq!(face.benches.len(), 2);
    }

    /// With its end held, the run moves the start along the toe until the road comes out there, up one wall.
    #[test]
    fn a_road_with_its_end_held_comes_out_there() {
        let face = wall_to(120.0, 45.0, FaceRamp { ends_at: Some([E + 400.0, N + 20.0]), ..RAMP });
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let pit = &face.roads[0].pit_edge;
        assert!((pit.vertices.last().unwrap()[0] - E - 400.0).abs() <= END_TOLERANCE, "{:?}", pit.vertices.last());
        assert!((pit.vertices[0][0] - E - (400.0 - 200.0 * COS)).abs() <= END_TOLERANCE, "the start moved 199 m back: {:?}", pit.vertices[0]);
    }

    /// The same running back along the string, through a berm, with the walls wandering.
    #[test]
    fn a_road_through_two_walls_comes_out_where_its_held() {
        let surface = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [20.0, 20.0], 90, 50, |x, _| 118.0 + 2.0 * ((x - E) / 60.0).sin()));
        let rows = [
            FaceStep::Project { to: FaceTarget::Surface(&surface), batter: EdgeRule::Fixed(60.0), condition: Some(0.1), ramp: Some(0) },
            FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: None },
            wall(FaceTarget::Rl(140.0), 60.0),
        ];
        let ramp = FaceRamp { back: true, ends_at: Some([E + 150.0, N + 40.0]), ..RAMP };
        let toe = string(&[[0.0, 0.0], [900.0, 0.0]], FLOOR);
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[ramp]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let top = *face.roads[0].pit_edge.vertices.last().unwrap();
        assert!((top[0] - E - 150.0).abs() <= END_TOLERANCE, "{top:?}");
        assert!((top[2] - 140.0).abs() < 1e-6, "{top:?}");
        assert_eq!(face.roads[0].steps, [0, 2]);
        assert_eq!(face.roads[0].off_end, None);
    }

    /// A held end too close to the end of the string for the road's length says so.
    #[test]
    fn a_held_end_the_string_cant_reach_says_how_far_short() {
        let face = wall_to(120.0, 45.0, FaceRamp { ends_at: Some([E + 100.0, N + 20.0]), ..RAMP });
        assert!(face.no_roads[0].why.contains("past the end of the string"), "{:?}", face.no_roads);
    }

    /// A start off the line takes the nearest point on it.
    #[test]
    fn a_start_off_the_line_takes_the_nearest_point_on_it() {
        let face = wall_to(120.0, 45.0, FaceRamp { start: [E + 100.0, N + 30.0], ..RAMP });
        let bottom = face.roads[0].pit_edge.vertices[0];
        assert_eq!([bottom[0] - E, bottom[1] - N], [100.0, 0.0]);
    }

    /// Nothing about a road on a plain wall needs many triangles.
    #[test]
    fn a_straight_road_stays_a_few_triangles() {
        let face = wall_to(120.0, 70.0, RAMP);
        assert!(face.surface.triangles.len() < 60, "{}", face.surface.triangles.len());
    }

    /// A wall that wanders round the minimum bench, with a berm above it, as real grids do.
    fn wandering_face(x: f64, rise: f64, height: f64, width: f64, side: OffsetSide, through: bool) -> Result<Face, crate::GeomError> {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |px, _| FLOOR + height + rise * (px - E - x) + 2.0 * ((px - E) / 30.0).sin()));
        let road = FaceRamp { start: [E + x, N], width, ..RAMP };
        let first = FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramp: Some(0) };
        let bench = FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: Some(5.0) };
        let second = FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramp: through.then_some(0) };
        let rows = [first, bench, second];
        let toe = match side {
            OffsetSide::Left => string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR),
            OffsetSide::Right => string(&[[600.0, 0.0], [0.0, 0.0]], FLOOR),
        };
        project_face(&toe, &level(FLOOR), side, FaceDirection::Up, &rows, &[road])
    }

    /// Walls wandering round the minimum bench, both sides, with the road on one wall or through to the next.
    /// Nothing is refused, and whatever road is built is one sheet with nothing crossing.
    #[test]
    fn roads_on_walls_wandering_round_the_minimum_bench_stay_one_sheet() {
        for (width, x, rise, height, side, through) in [
            (10.0, 120.0, 0.03, 4.0, OffsetSide::Left, false),
            (20.0, 200.0, -0.05, 3.0, OffsetSide::Left, false),
            (30.0, 80.0, 0.01, 5.0, OffsetSide::Left, false),
            (40.0, 160.0, -0.01, 2.0, OffsetSide::Left, true),
            (10.0, 500.0, 0.03, 4.0, OffsetSide::Right, false),
            (20.0, 400.0, -0.03, 3.0, OffsetSide::Right, true),
            (30.0, 460.0, 0.05, 5.0, OffsetSide::Right, true),
        ] {
            let face = wandering_face(x, rise, height, width, side, through).unwrap();
            assert!(!face.roads.is_empty(), "{width} {x} {rise} {height} {side:?}: {:?}", face.no_roads);
            one_clean_sheet(&face);
        }
    }

    /// If the berm above a road can't follow it, the road comes out and says why, and the face still builds.
    #[test]
    fn a_road_the_berm_above_cant_follow_comes_out_and_the_face_still_builds() {
        let face = wandering_face(120.0, 0.01, 4.0, 10.0, OffsetSide::Right, true).unwrap();
        assert!(face.roads.is_empty(), "{:?}", face.roads);
        assert!(face.no_roads[0].why.contains("what's built on it can't be"), "{:?}", face.no_roads);
        assert_eq!(face.benches.len(), 2, "both walls are there");
    }

    /// A road that makes its first wall, and whose next wall's berm can't be built on it, stops a wall lower:
    /// at the last bench it can make. It says so, and everything else still builds.
    #[test]
    fn a_road_stops_a_wall_lower_if_whats_built_on_it_cant_be() {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| FLOOR + 7.0 + 0.01 * (x - E - 120.0) + 2.0 * ((x - E) / 30.0).sin()));
        let shelf = FaceTarget::Rl(FLOOR + 4.0);
        let rows = [
            wall(shelf, 70.0),
            FaceStep::Berm { on: Some(shelf), width: EdgeRule::Fixed(10.0), register: true, condition: None, min_bench: None },
            FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramp: Some(0) },
            FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: Some(5.0) },
            FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramp: Some(0) },
        ];
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[FaceRamp { start: [E + 120.0, N], ..RAMP }]).unwrap();
        assert_eq!(face.roads.iter().map(|r| r.steps).collect::<Vec<_>>(), [[0, 0]], "{:?}", face.no_roads);
        assert_eq!(face.no_roads[0].step, 2, "{:?}", face.no_roads);
        assert!(face.no_roads[0].why.contains("what's built on it can't be"), "{:?}", face.no_roads);
        assert_eq!(face.benches.len(), 3, "all three walls are there");
    }
}
