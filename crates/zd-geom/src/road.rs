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
//! A wall can hold several roads running the same way. They stack: at any place along the wall the road that
//! started furthest ahead is the lowest, and what it cuts back moves everything above it. So roads go in lowest
//! first, each into the wall above the one before, and the wall above the last rises from its wall-side edge
//! as ever. At one grade two roads stay the same height apart, so they never meet.
//!
//! Where a road can't be built the rest of the face still is, and the run says where and why.

use std::collections::HashMap;
use std::iter::once;

use zd_core::{Polyline, TriMesh};

use crate::face::{band, place_on, stretch_of, weld, with_touches};
use crate::error::GeomError;
use crate::rule::inside_polygon;
use crate::string::{TriangleBins, cross, segment_crossing};
use crate::wall::{WallLog, WallStep, at_crest_heights, nearest_along, plan_gap, segment_count};
use crate::{Corner, EdgeRule, OffsetSide, drape_string, filter_string, offset_string, project_wall_by_edge};

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

/// Why a road can't go on a wall that has no height where it gets there. It tries the next wall that names it.
pub(crate) const NO_WALL: &str = "there's no wall at its start";

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
    /// It goes past a berm after it, crossing the full berm level, rather than up through it.
    pub after_berm: bool,
    /// Where it turns back with no pad past its top, how far on from the turn, along the line, the toe past it joins back
    /// to the line from the next road's wall-side start (m). A set length, so the pad doesn't depend on where the string
    /// has vertices.
    pub turn_join: f64,
    /// Where it turns back, how far the switchback's flat box runs on past its top, in line with it, on what the berm lies
    /// on (m): the road, the berm and the next road wide. The next road climbs from straight across the berm from the top,
    /// at the box's near end, and the wall above goes round the box.
    pub turn_flat: f64,
    /// Where it turns back on a box, the angle in plan between the box's far end and the strip (degrees): 90 is square
    /// across, and less flares it out past the box, from its back corner to the berm's line past it.
    pub turn_end: f64,
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
    /// Each flat it ran before a turn on a floor that rises or falls along it more than the road's grade: where the flat
    /// ends, and the steepest it goes, as a fraction.
    pub steep_flats: Vec<(P3, f64)>,
}

/// A ramp that stopped short of its stop row, where it stopped and why. Any walls it had climbed keep it.
#[derive(Debug, Clone, PartialEq)]
pub struct NoRoad {
    pub ramp: usize,
    pub step: usize,
    pub at: P3,
    pub why: String,
    /// The ramp it would cross, running the other way on the same wall, if that's why it stopped.
    pub crosses: Option<usize>,
}

/// The top edge of a road so far, where it carries on from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoadTop {
    pub pit: P3,
    pub wall: P3,
}

/// A road going into a wall: where it got to on the wall below if it's carrying on, and if not, what's under
/// its entrance if the toe lies on a surface.
#[derive(Clone, Copy)]
pub(crate) struct RoadOnWall<'a> {
    pub spec: FaceRamp,
    pub top: Option<RoadTop>,
    pub floor: Option<&'a TriMesh>,
    /// Where it came onto the line across a pad, if it turned back to start here: it runs flat along the line from
    /// there to its start, where it climbs.
    pub turned: Option<[f64; 2]>,
    /// The far end of the box pad it turned on, if it's a box: its corners on the crest and a road width in from it. The
    /// toe of the wall above goes round the box, its far end on in line with that end.
    pub pad: Option<[P3; 2]>,
}

/// How a stretch of road starts.
enum RoadBottom<'a> {
    /// At the bottom of the road. The toe behind it moves back as far as the vertex `from`, lying on `floor` if it
    /// lies on a surface. A road that `turned` back on a pad came onto the line at that vertex and runs flat from
    /// there to the bottom of its climb; the toe beside that stretch moves back by the road, and past the turn it
    /// joins back.
    Entrance { floor: Option<&'a TriMesh>, from: Option<usize>, turned: Option<usize>, pad: Option<[P3; 2]> },
    /// Carrying on from the wall below, whose road's wall-side edge came in at this vertex of the toe.
    CarriedOn { wall_side: usize },
}

/// Why a road stopped, and where.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stopped {
    pub at: P3,
    pub why: String,
    /// The road it would cross, by its place in the list a wall was given, if that's why it stopped.
    pub crosses: Option<usize>,
}

/// One road cut into a wall.
struct RoadCut {
    lower: TriMesh,
    road: TriMesh,
    /// The foot of the wall, in the string's order: round the bottom of the road at its entrance.
    toe: Polyline,
    /// How many vertices of the string above are behind the road, and how many are up its wall-side edge.
    behind: usize,
    up_the_road: usize,
    /// The crest of the wall it was cut from, and where on that crest its top is.
    crest: Polyline,
    place: f64,
    pit_edge: Polyline,
    wall_edge: Polyline,
    /// The flat stretch a road that turned back runs before it climbs, its pit side and its wall side, each up to
    /// where the edges above start. None for a road that didn't turn.
    flat: Option<[Vec<P3>; 2]>,
    /// Points the sheet is to be split at, where the toe behind comes onto a line part way along it.
    meets: Vec<P3>,
}

/// The wall above a road, which is where the next road up goes.
struct Above {
    wall: WallStep,
    string: Polyline,
    slopes: Vec<f64>,
}

/// A wall with its roads cut into it.
pub(crate) struct WallCut {
    pub meshes: Vec<TriMesh>,
    /// The foot of it all and where it meets its target, each in the string's order.
    pub toe: Polyline,
    pub crest: Polyline,
    /// Each road cut, as a place in the list it was given and its two edges from bottom to top.
    pub roads: Vec<(usize, Polyline, Polyline)>,
    /// Each road that turned back, as a place in the list, and its flat stretch: along the line from where it came onto
    /// it to where it climbs, and the wall side from its corner past the turn to where the climb's wall-side edge starts,
    /// both as the toe has them.
    pub flats: Vec<(usize, Vec<P3>, Vec<P3>)>,
    pub logs: Vec<WallLog>,
    /// Points the sheet is to be split at, where a toe comes onto a line part way along it.
    pub meets: Vec<P3>,
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

/// Where a road starts on a string: the string with any vertex it needs, its batters to match, the vertex, and
/// if the road's carrying on, the vertex its wall-side edge comes in at.
pub(crate) fn start_on(toe: &Polyline, batters: &[f64], road: &RoadOnWall<'_>) -> (Polyline, Vec<f64>, usize, Option<usize>) {
    let (mut line, mut widened) = (toe.clone(), batters.to_vec());
    let mut vertex_at = |place: [f64; 2]| {
        let at;
        (line, widened, at) = with_road_start(&line, &widened, place);
        at
    };
    let (at, wall_side) = match road.top {
        None => (vertex_at(road.spec.start), None),
        Some(top) => {
            let (wall, pit) = ([top.wall[0], top.wall[1]], [top.pit[0], top.pit[1]]);
            vertex_at(wall);
            let at = vertex_at(pit);
            (at, Some(vertex_at(wall)))
        }
    };
    (line, widened, at, wall_side)
}

/// A wall with its roads cut back into it, up to the target. `wall` is the plain wall from that toe. Roads running
/// opposite ways sit side by side along it, each cut in its own way. A road that can't go in goes to `stop` by its
/// place in the list, and the others carry on without it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cut_roads(toe: &Polyline, batters: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], wall: &WallStep, target: &TriMesh, stop: &mut dyn FnMut(usize, Stopped)) -> Option<WallCut> {
    let back = roads.first()?.spec.back;
    if roads.iter().all(|road| road.spec.back == back) {
        return cut_roads_one_way(toe, batters, side, roads, wall.clone(), target, false, stop);
    }
    cut_roads_both_ways(toe, batters, side, roads, wall, target, stop)
}

/// Roads that all run one way. `held` keeps the toe where it is at the start of the string, the way the roads run,
/// where the wall has been split for a road running the other way.
#[allow(clippy::too_many_arguments)]
fn cut_roads_one_way(toe: &Polyline, batters: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], wall: WallStep, target: &TriMesh, held: bool, stop: &mut dyn FnMut(usize, Stopped)) -> Option<WallCut> {
    if !roads.first()?.spec.back {
        return cut_roads_along(toe, batters, side, roads, wall, target, held, stop);
    }
    let turned = WallStep { crest: backwards(&wall.crest), wall: wall.wall, log: wall.log };
    let batters: Vec<f64> = batters.iter().rev().copied().collect();
    let other = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
    let mut cut = cut_roads_along(&backwards(toe), &batters, other, roads, turned, target, held, stop)?;
    cut.crest.vertices.reverse();
    cut.toe.vertices.reverse();
    Some(cut)
}

/// Why a road stops where it would share a stretch of wall with one running the other way, or come within a road's width
/// of it: climbing opposite ways, the two would cross, and tops that face each other closer than that leave a sliver of
/// wall between them.
pub(crate) const CROSSES: &str = "it would cross a road running the other way on this wall, or come within a road's width of it: move one of them along the strip";
/// The least straight stretch of toe between two roads running opposite ways for the wall to be split on (m).
const SPLIT_ROOM: f64 = 1.0;

/// Roads running both ways on one wall (Tim, 2026-11-05: a straight ramp and a switchback on one face). Two running
/// opposite ways can't share a stretch of it, as climbing opposite ways they'd cross, so each is first cut alone to
/// find the stretch it takes, and where two running opposite ways would share one, or come within the wider one's
/// width of each other, the later in the list stops. The
/// rest sit in runs along the wall, each run one way, and the wall is cut in parts split between the runs, in the
/// middle of a straight stretch of toe so the parts' walls meet on one plane. Each part takes its run's roads, which
/// stack as on any wall. Where two runs' entrances face each other, each moves the toe back as far as the split, so
/// they share the floor between them where they meet there; where they don't, each comes across to the toe at the
/// split at 45 degrees.
#[allow(clippy::too_many_arguments)]
fn cut_roads_both_ways(toe: &Polyline, batters: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], wall: &WallStep, target: &TriMesh, stop: &mut dyn FnMut(usize, Stopped)) -> Option<WallCut> {
    let mut taken: Vec<(usize, f64, f64)> = Vec::new();
    for (r, road) in roads.iter().enumerate() {
        match stretch_taken(toe, batters, side, road, wall, target) {
            Ok((from, to)) => match taken.iter().find(|(other, from_other, to_other)| {
                let room = road.spec.width.max(roads[*other].spec.width);
                roads[*other].spec.back != road.spec.back && from < *to_other + room && *from_other < to + room
            }) {
                Some(&(other, ..)) => stop(r, Stopped { at: point_along(toe, if road.spec.back { to } else { from }), why: CROSSES.into(), crosses: Some(other) }),
                None => taken.push((r, from, to)),
            },
            Err(stopped) => stop(r, stopped),
        }
    }
    taken.sort_by(|a, b| a.1.total_cmp(&b.1));
    // Runs of roads one way, in order along the toe, each with the stretch it takes, and where the wall splits between
    // each run and the next. Two runs with no straight stretch of toe between them to split on are too close, and the
    // later road stops.
    let (runs, splits) = loop {
        let mut runs: Vec<(Vec<usize>, f64, f64)> = Vec::new();
        for &(r, from, to) in &taken {
            match runs.last_mut() {
                Some((run, _, end)) if roads[run[0]].spec.back == roads[r].spec.back => {
                    run.push(r);
                    *end = end.max(to);
                }
                _ => runs.push((vec![r], from, to)),
            }
        }
        let splits: Vec<Option<f64>> = runs.windows(2).map(|pair| split_between(toe, pair[0].2, pair[1].1)).collect();
        let Some(k) = splits.iter().position(Option::is_none) else { break (runs, splits.into_iter().flatten().collect::<Vec<f64>>()) };
        let later = runs[k].0.iter().chain(&runs[k + 1].0).copied().max().expect("runs have roads");
        let other = if runs[k].0.contains(&later) { runs[k + 1].0[0] } else { runs[k].0[0] };
        let at = taken.iter().find(|(r, ..)| *r == later).map_or(toe.vertices[0], |(_, from, to)| point_along(toe, if roads[later].spec.back { *to } else { *from }));
        stop(later, Stopped { at, why: CROSSES.into(), crosses: Some(other) });
        taken.retain(|(r, ..)| *r != later);
    };
    if runs.len() < 2 {
        let run: Vec<usize> = runs.into_iter().flat_map(|(run, ..)| run).collect();
        let its: Vec<RoadOnWall<'_>> = run.iter().map(|r| roads[*r]).collect();
        let mut cut = cut_roads_one_way(toe, batters, side, &its, wall.clone(), target, false, &mut |k, stopped| stop(run[k], stopped))?;
        renumber(&mut cut, &run);
        return Some(cut);
    }
    // The parts of the string, each sharing the vertex at a split with the next.
    let mut parts: Vec<(Polyline, Vec<f64>)> = Vec::with_capacity(runs.len());
    let (mut rest, mut rest_slopes) = (toe.clone(), batters.to_vec());
    for &metres in &splits {
        let p = point_along(toe, metres);
        let (line, slopes, at) = with_road_start(&rest, &rest_slopes, [p[0], p[1]]);
        parts.push((Polyline { vertices: line.vertices[..=at].to_vec(), closed: false }, slopes[..at].to_vec()));
        (rest, rest_slopes) = (Polyline { vertices: line.vertices[at..].to_vec(), closed: false }, slopes[at..].to_vec());
    }
    parts.push((rest, rest_slopes));
    let runs: Vec<Vec<usize>> = runs.into_iter().map(|(run, ..)| run).collect();
    let back = |k: usize| roads[runs[k][0]].spec.back;
    let mut cuts = Vec::with_capacity(parts.len());
    for (k, (part, slopes)) in parts.iter().enumerate() {
        cuts.push(cut_part(part, slopes, side, roads, &runs[k], target, false));
    }
    // Where two entrances face each other and don't meet at the split, both come across to the toe there.
    for k in 1..parts.len() {
        let meet = |a: &Option<(WallCut, Vec<(usize, Stopped)>)>, b: &Option<(WallCut, Vec<(usize, Stopped)>)>| match (a, b) {
            (Some((a, _)), Some((b, _))) => a.toe.vertices.last().zip(b.toe.vertices.first()).is_some_and(|(p, q)| plan_gap(*p, *q) < SAME_POINT && (p[2] - q[2]).abs() < SAME_POINT),
            _ => true,
        };
        if back(k - 1) && !back(k) && !meet(&cuts[k - 1], &cuts[k]) {
            for j in [k - 1, k] {
                cuts[j] = cut_part(&parts[j].0, &parts[j].1, side, roads, &runs[j], target, true);
            }
        }
    }
    if let Some(k) = cuts.iter().position(Option::is_none) {
        // A part's own wall can't be built: nothing on this wall is cut, and its roads stop.
        for &r in runs.iter().flatten() {
            stop(r, Stopped { at: parts[k].0.vertices[0], why: "the wall can't be built where it's split for roads running both ways".into(), crosses: None });
        }
        return None;
    }
    let mut whole = Vec::with_capacity(cuts.len());
    for (cut, stops) in cuts.into_iter().flatten() {
        for (r, stopped) in stops {
            stop(r, stopped);
        }
        whole.push(cut);
    }
    Some(joined(whole))
}

/// Road numbers in a cut made from some of a wall's roads, `run`, put back to their places in the whole list.
fn renumber(cut: &mut WallCut, run: &[usize]) {
    for road in &mut cut.roads {
        road.0 = run[road.0];
    }
    for flat in &mut cut.flats {
        flat.0 = run[flat.0];
    }
}

/// The roads in `run` cut into one part of a wall split for roads running both ways, numbered as in `roads`, with any
/// that stopped and why. None if the part's own wall can't be built. A part with no roads left is its plain wall.
#[allow(clippy::too_many_arguments)]
fn cut_part(part: &Polyline, slopes: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], run: &[usize], target: &TriMesh, held: bool) -> Option<(WallCut, Vec<(usize, Stopped)>)> {
    // Held, the toe stays on the line for half the split's room in from the split, the way the roads run, before an
    // entrance comes across.
    let (part, slopes) = if held {
        let into = if roads[run[0]].spec.back { part.plan_length() - SPLIT_ROOM / 2.0 } else { SPLIT_ROOM / 2.0 };
        let p = point_along(part, into);
        let (line, slopes, _) = with_road_start(part, slopes, [p[0], p[1]]);
        (line, slopes)
    } else {
        (part.clone(), slopes.to_vec())
    };
    let (part, slopes) = (&part, &slopes[..]);
    let plain = project_wall_by_edge(part, side, slopes, target).ok()?;
    let its: Vec<RoadOnWall<'_>> = run.iter().map(|r| roads[*r]).collect();
    let mut stops = Vec::new();
    let cut = cut_roads_one_way(part, slopes, side, &its, plain.clone(), target, held, &mut |k, stopped| stops.push((run[k], stopped)));
    let cut = match cut {
        Some(mut cut) => {
            renumber(&mut cut, run);
            cut
        }
        None => WallCut { meshes: vec![plain.wall], toe: part.clone(), crest: plain.crest, roads: Vec::new(), flats: Vec::new(), logs: vec![plain.log], meets: Vec::new() },
    };
    Some((cut, stops))
}

/// The stretch of a wall a road takes, cut alone into it, in metres along the toe: from the bottom of its climb, or a
/// switchback's pad where the toe goes round it, to its top. An entrance isn't part of it, as it moves back only as far
/// as there's room.
fn stretch_taken(toe: &Polyline, batters: &[f64], side: OffsetSide, road: &RoadOnWall<'_>, wall: &WallStep, target: &TriMesh) -> Result<(f64, f64), Stopped> {
    let mut stopped = None;
    let cut = cut_roads_one_way(toe, batters, side, std::slice::from_ref(road), wall.clone(), target, false, &mut |_, why| stopped = Some(why));
    let cut = match (cut, stopped) {
        (_, Some(stopped)) => return Err(stopped),
        (Some(cut), None) => cut,
        (None, None) => return Err(Stopped { at: [road.spec.start[0], road.spec.start[1], 0.0], why: "it can't be cut into this wall".into(), crosses: None }),
    };
    let mut places: Vec<f64> = cut.roads.iter().flat_map(|(_, pit, wall_side)| pit.vertices.iter().chain(&wall_side.vertices)).map(|p| metres_along(toe, *p)).collect();
    places.extend(cut.flats.iter().flat_map(|(_, pit, wall_side)| pit.iter().chain(wall_side)).map(|p| metres_along(toe, *p)));
    if road.turned.is_some() {
        // Where the toe goes round its pad, it's off the line it came from.
        places.extend(cut.toe.vertices.iter().filter(|p| plan_gap(**p, point_at(toe, nearest_place(toe, **p))) > WALL_EDGE_TOLERANCE).map(|p| metres_along(toe, *p)));
    }
    let from = places.iter().copied().fold(f64::INFINITY, f64::min);
    let to = places.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok((from, to))
}

/// Where to split a string between two stretches of it, in metres along it from `after` to `before`: the middle of the
/// longest straight run of it there, so the walls either side meet on one plane. None if there's less than the room.
fn split_between(line: &Polyline, after: f64, before: f64) -> Option<f64> {
    let mut start = 0.0_f64;
    let mut best: Option<(f64, f64)> = None;
    for w in line.vertices.windows(2) {
        let length = plan_gap(w[0], w[1]);
        let (from, to) = (start.max(after), (start + length).min(before));
        if to - from >= SPLIT_ROOM && best.is_none_or(|(a, b)| to - from > b - a) {
            best = Some((from, to));
        }
        start += length;
    }
    best.map(|(from, to)| (from + to) / 2.0)
}

/// The parts of a split wall cut one by one, as one, each in the string's order. They meet on the toe at each split.
fn joined(parts: Vec<WallCut>) -> WallCut {
    let mut whole = WallCut { meshes: Vec::new(), toe: Polyline { vertices: Vec::new(), closed: false }, crest: Polyline { vertices: Vec::new(), closed: false }, roads: Vec::new(), flats: Vec::new(), logs: Vec::new(), meets: Vec::new() };
    for part in parts {
        for (line, more) in [(&mut whole.toe, part.toe), (&mut whole.crest, part.crest)] {
            let skip = usize::from(line.vertices.last().zip(more.vertices.first()).is_some_and(|(a, b)| plan_gap(*a, *b) < SAME_POINT && (a[2] - b[2]).abs() < SAME_POINT));
            line.vertices.extend_from_slice(&more.vertices[skip..]);
        }
        whole.meshes.extend(part.meshes);
        whole.roads.extend(part.roads);
        whole.flats.extend(part.flats);
        whole.logs.extend(part.logs);
        whole.meets.extend(part.meets);
    }
    whole
}

/// Roads running the way the toe does, lowest first: the one that starts furthest along. Each goes into the wall
/// above the one before, and takes a fresh wall if it needs a vertex that wall hasn't got. `held` keeps the toe where
/// it is at the start of the string.
#[allow(clippy::too_many_arguments)]
fn cut_roads_along(toe: &Polyline, batters: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], plain: WallStep, target: &TriMesh, held: bool, stop: &mut dyn FnMut(usize, Stopped)) -> Option<WallCut> {
    let along = |road: &RoadOnWall<'_>| {
        let at = road.top.map_or([road.spec.start[0], road.spec.start[1], 0.0], |top| top.pit);
        metres_along(toe, at)
    };
    let mut lowest_first: Vec<usize> = (0..roads.len()).collect();
    lowest_first.sort_by(|a, b| along(&roads[*b]).total_cmp(&along(&roads[*a])));
    let (mut string, mut slopes, mut wall) = (toe.clone(), batters.to_vec(), plain);
    let mut cuts: Vec<(usize, RoadCut)> = Vec::new();
    let mut logs = Vec::new();
    for r in lowest_first {
        let road = &roads[r];
        let (line, widened, at, wall_side) = start_on(&string, &slopes, road);
        // A road that turned back on a pad came onto the line before its start, and runs flat along it from there.
        let (line, widened, at, across) = match road.turned.filter(|_| wall_side.is_none()) {
            Some(onto) => {
                let (more, more_widened, across) = with_road_start(&line, &widened, onto);
                let at = if more.vertices.len() > line.vertices.len() && across <= at { at + 1 } else { at };
                (more, more_widened, at, Some(across.min(at)))
            }
            None => (line, widened, at, None),
        };
        let start = line.vertices[at];
        if wall_side.is_none() && cuts.last().is_some_and(|(_, below)| line.vertices.len() - at <= below.up_the_road) {
            stop(r, Stopped { at: start, why: "it starts on the road below it: move it along the toe".into(), crosses: None });
            continue;
        }
        let fresh = if line.vertices.len() == string.vertices.len() {
            None
        } else {
            match project_wall_by_edge(&line, side, &widened, target) {
                Ok(fresh) => Some(fresh),
                Err(e) => {
                    stop(r, Stopped { at: start, why: format!("the wall it starts on can't be built: {e}"), crosses: None });
                    continue;
                }
            }
        };
        // An entrance moves the toe behind it back only as far as a road coming up through that toe.
        // Held, it stays where it is up to the string's second vertex, half the split's room in from where the wall's been
        // split for a road running the other way, so the wall either side of the split rises square to one line.
        let from = roads.iter().filter_map(|other| other.top).filter_map(|top| line.vertices[..at].iter().rposition(|p| plan_gap(*p, top.pit) < SAME_POINT)).max().filter(|from| *from > 0).or(held.then_some(1));
        let bottom = wall_side.map_or(RoadBottom::Entrance { floor: road.floor, from, turned: across, pad: road.pad }, |wall_side| RoadBottom::CarriedOn { wall_side });
        match cut_road(&line, &widened, side, &road.spec, at, bottom, fresh.as_ref().unwrap_or(&wall), target) {
            Ok((cut, above)) => {
                logs.push(above.wall.log.clone());
                cuts.push((r, cut));
                (string, slopes, wall) = (above.string, above.slopes, above.wall);
            }
            Err(stopped) => stop(r, stopped),
        }
    }
    let (toe, crest) = (foot_of(&cuts)?, crest_of(wall.crest, &cuts));
    let mut meshes = Vec::with_capacity(2 * cuts.len() + 1);
    let mut cut_roads = Vec::with_capacity(cuts.len());
    let mut flats = Vec::new();
    let meets: Vec<P3> = cuts.iter().flat_map(|(_, cut)| cut.meets.iter().copied()).collect();
    for (r, cut) in cuts {
        meshes.extend([cut.lower, cut.road]);
        let turned = cut.flat.is_some();
        let [mut pit, mut wall_side] = cut.flat.unwrap_or_default();
        if turned {
            // The foot has the climb's wall-side start and then its pit-side start just after what's behind the road.
            let (wall_start, start) = (cut.toe.vertices[cut.behind], cut.toe.vertices[cut.behind + 1]);
            flats.push((r, pit.iter().copied().chain(once(start)).collect(), wall_side.iter().copied().chain(once(wall_start)).collect()));
        }
        pit.extend(cut.pit_edge.vertices);
        wall_side.extend(cut.wall_edge.vertices);
        cut_roads.push((r, Polyline { vertices: pit, closed: false }, Polyline { vertices: wall_side, closed: false }));
    }
    meshes.push(wall.wall);
    Some(WallCut { meshes, toe, crest, roads: cut_roads, flats, logs, meets })
}

/// The foot of a wall with roads in it. Each road up changes the toe behind the start of the one below it, and
/// from the bottom of that road on the foot is the one below's.
fn foot_of(cuts: &[(usize, RoadCut)]) -> Option<Polyline> {
    let ((_, top), below) = cuts.split_last()?;
    let mut foot = top.toe.vertices.clone();
    for (_, cut) in below.iter().rev() {
        foot.truncate(foot.len() - cut.up_the_road);
        foot.extend_from_slice(&cut.toe.vertices[cut.behind..]);
    }
    Some(Polyline { vertices: foot, closed: false })
}

/// The crest of a wall with roads in it: from the wall above the top road, down round the top of each.
fn crest_of(above: Polyline, cuts: &[(usize, RoadCut)]) -> Polyline {
    cuts.iter().rev().fold(above, |crest, (_, cut)| {
        let top = *cut.pit_edge.vertices.last().expect("a climb has an end");
        crest_round_road(&crest, top, &cut.crest, cut.place)
    })
}

fn backwards(line: &Polyline) -> Polyline {
    Polyline { vertices: line.vertices.iter().rev().copied().collect(), closed: line.closed }
}

/// A road running the way the toe does, from the toe's vertex `at` up to the target. `wall` is the wall from that
/// toe.
#[allow(clippy::too_many_arguments)]
fn cut_road(toe: &Polyline, batters: &[f64], side: OffsetSide, ramp: &FaceRamp, at: usize, bottom: RoadBottom<'_>, wall: &WallStep, target: &TriMesh) -> Result<(RoadCut, Above), Stopped> {
    let start = toe.vertices[at];
    let stopped = |why: String| Stopped { at: start, why, crosses: None };
    let facets = Facets::of(&wall.wall, start);
    let climb = facets.climb(start, side, ramp.grade, toe, &wall.crest)?;
    let pit_edge = Polyline { vertices: climb.edge.clone(), closed: false };
    let offset = wall_edge_of(&pit_edge, ramp.width, side).map_err(|e| stopped(format!("the road's width can't be held: {e}")))?;
    let mut flat = None;
    let (behind, foot, mut wall_edge) = match bottom {
        RoadBottom::Entrance { floor, from, turned, pad } => {
            let behind = match turned {
                Some(across) => toe_past_turn(toe, batters, at, across, from.unwrap_or(0), offset.vertices[0], side, &pit_edge, (ramp.turn_join, ramp.turn_end), pad, floor).map(|(behind, wall_side)| {
                    flat = Some([toe.vertices[across..at].to_vec(), wall_side]);
                    behind
                }),
                None => entrance_toe(toe, batters, at, from, offset.vertices[0], side, floor),
            };
            let behind = behind.map_err(|e| stopped(format!("its entrance can't be built: {e}")))?;
            let round = behind.vertices.iter().chain(&offset.vertices[..1]).chain(&toe.vertices[at..]).copied().collect();
            (behind, Polyline { vertices: round, closed: false }, offset)
        }
        RoadBottom::CarriedOn { wall_side } => {
            let behind = Behind { vertices: toe.vertices[..wall_side].to_vec(), batters: batters[..wall_side].to_vec(), meets: Vec::new() };
            (behind, toe.clone(), carried_on(toe.vertices[wall_side], &offset, &pit_edge, ramp.grade))
        }
    };
    run_out_onto(&mut wall_edge, ramp.grade, target).map_err(|why| Stopped { at: *pit_edge.vertices.last().expect("a climb has an end"), why, crosses: None })?;
    let (string, slopes) = string_up_the_road(&behind, toe, batters, &wall_edge, ramp.grade);
    let upper = project_wall_by_edge(&string, side, &slopes, target).map_err(|e| stopped(format!("the wall above it can't be built: {e}")))?;
    let road = band(&pit_edge, &road_inner_line(start, &wall_edge, &upper.crest)).map_err(|e| stopped(format!("the road can't be built: {e}")))?;
    let top = *pit_edge.vertices.last().expect("a climb has an end");
    let place = place_on(&wall.crest, top).ok_or_else(|| stopped("the road's top isn't on the crest".into()))?;
    let lower = facets.wall_below(&climb.cuts, &removed_by_road(toe, at, &pit_edge, &wall.crest, place));
    let cut = RoadCut { lower, road, toe: foot, behind: behind.vertices.len(), up_the_road: wall_edge.vertices.len(), crest: wall.crest.clone(), place, pit_edge, wall_edge, flat, meets: behind.meets };
    Ok((cut, Above { wall: upper, string, slopes }))
}

/// The string behind a road, and a batter for each of its segments and the one that leads to the road.
struct Behind {
    vertices: Vec<P3>,
    batters: Vec<f64>,
    /// Where it comes onto a line part way along a stretch of it that's also a crest below, for the sheet to be split at:
    /// the foot of a box's far end flared out onto the berm's line past it.
    meets: Vec<P3>,
}

/// The toe behind a road's start, moved back by the road so trucks can drive along the floor straight onto
/// it. It moves as far as the road's wall-side edge starts from the toe, so the wall there and the wall above
/// the road are one plane. It stops a road width short of that edge, and the last stretch leads up to it, so
/// a floor that isn't level at the start of the road has room to meet it. Behind the vertex `from` it stays where
/// it is, for a road coming up through it, and comes across to the moved toe at 45 degrees.
fn entrance_toe(toe: &Polyline, batters: &[f64], at: usize, from: Option<usize>, wall_start: P3, side: OffsetSide, floor: Option<&TriMesh>) -> Result<Behind, GeomError> {
    let stays = |to: usize| Behind { vertices: toe.vertices[..to].to_vec(), batters: batters[..to].to_vec(), meets: Vec::new() };
    let first = from.unwrap_or(0);
    if at == first {
        return Ok(stays(at));
    }
    let road = Polyline { vertices: toe.vertices[first..=at].to_vec(), closed: false };
    let back = (0..at - first).map(|i| nearest_along(road.vertices[i], road.vertices[i + 1], wall_start).1).fold(f64::INFINITY, f64::min);
    let reach = plan_gap(toe.vertices[at], wall_start);
    let moved = longest(offset_string(&road, back, side, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?);
    let short = place_at_gap(&moved, nearest_place(&moved, wall_start), false, wall_start, reach);
    let across = if from.is_none() { 0.0 } else { place_at_gap(&moved, 0.0, true, moved.vertices[0], back) };
    if short <= across {
        return Ok(stays(from.map_or(0, |from| from + 1)));
    }
    let line = stretch_of(&moved, across, short);
    let laid = match floor {
        Some(floor) => drape_string(&line, floor, true)?.0.vertices,
        None => line.vertices,
    };
    let moved_batters = (0..laid.len()).map(|k| {
        let (a, b) = (laid[k], *laid.get(k + 1).unwrap_or(&wall_start));
        batter_nearest(toe, batters, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, 0.0])
    });
    let Some(from) = from else {
        let batters = moved_batters.collect();
        return Ok(Behind { vertices: laid, batters, meets: Vec::new() });
    };
    let mut kept = stays(from + 1);
    kept.batters.extend(moved_batters);
    kept.vertices.extend(laid);
    Ok(kept)
}

/// The toe behind a road that turned back on a pad. The road came onto the line at the vertex `across` and runs flat
/// along it to `at`, where it starts to climb, a road width on, so trucks come off the turn onto a straight, flat road:
/// beside that stretch the toe moves back as far as the road's wall-side edge starts from it. Past the turn it comes
/// out to that stretch's wall-side corner and runs straight back to the toe `turn_join` on from the turn, along the
/// line, whatever vertices the string has, lying on the floor but for its last road width, which leads up to the corner.
/// On its way it can pass over a bend of the toe out into the pad, as where the berm's line goes round the last road's
/// top, if the stretch beyond the toe is shorter than the road is wide; a longer one stops the join where it first meets
/// the toe. It goes no further than the end of the string, or than a road coming up through the toe at the vertex
/// `from`. Round a box `pad` it goes along the box's back to its far end, and from there square down it to the line, or
/// at `end` degrees to the strip out past it to the line there (Tim, 2026-11-04). Gives the toe behind, and the flat
/// stretch's wall side up to where the road's wall-side edge starts.
#[allow(clippy::too_many_arguments)]
fn toe_past_turn(toe: &Polyline, batters: &[f64], at: usize, across: usize, from: usize, wall_start: P3, side: OffsetSide, pit_edge: &Polyline, (turn_join, end): (f64, f64), pad: Option<[P3; 2]>, floor: Option<&TriMesh>) -> Result<(Behind, Vec<P3>), GeomError> {
    let laid_on = |line: Polyline| -> Result<Vec<P3>, GeomError> {
        Ok(match floor {
            Some(floor) => drape_string(&line, floor, true)?.0.vertices,
            None => line.vertices,
        })
    };
    let mut flat = Vec::new();
    if across < at {
        let stretch = Polyline { vertices: toe.vertices[across..=at].to_vec(), closed: false };
        let back = (0..at - across).map(|i| nearest_along(stretch.vertices[i], stretch.vertices[i + 1], wall_start).1).fold(f64::INFINITY, f64::min);
        let moved = longest(offset_string(&stretch, back, side, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?);
        // A road is level across: the flat stretch follows the line along its pit side, on the floor, and its wall side
        // stands level with that, as the road's own ends do on a floor that dips into the wall.
        flat = at_crest_heights(&stretch_of(&moved, 0.0, nearest_place(&moved, wall_start)), &stretch).vertices;
        if flat.last().is_some_and(|last| plan_gap(*last, wall_start) < SAME_POINT) {
            flat.pop();
        }
    }
    let corner = flat.first().copied().unwrap_or(wall_start);
    let with_flat = |mut behind: Behind| {
        for k in 0..flat.len() {
            let (p, q) = (flat[k], *flat.get(k + 1).unwrap_or(&wall_start));
            behind.batters.push(batter_nearest(toe, batters, [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, 0.0]));
        }
        behind.vertices.extend(&flat);
        (behind, flat.clone())
    };
    let stays = |to: usize| Behind { vertices: toe.vertices[..to].to_vec(), batters: batters[..to].to_vec(), meets: Vec::new() };
    if across == from {
        return Ok(with_flat(stays(from)));
    }
    let plan = |p: P3| [p[0], p[1]];
    let point = |m: usize, s: f64| {
        let (p, q) = (toe.vertices[m], toe.vertices[m + 1]);
        [p[0] + s * (q[0] - p[0]), p[1] + s * (q[1] - p[1]), p[2] + s * (q[2] - p[2])]
    };
    // Back along the line from where the road came onto it, or back down the climb at the end of the string. Round a box,
    // the way the line runs beside it, past the turn.
    let (a, b) = match across.checked_sub(1).filter(|_| pad.is_some()) {
        Some(before) => (toe.vertices[before], toe.vertices[across]),
        None if across + 1 < toe.vertices.len() => (toe.vertices[across], toe.vertices[across + 1]),
        None => (pit_edge.vertices[0], pit_edge.vertices[1]),
    };
    let length = plan_gap(a, b);
    let back = [(a[0] - b[0]) / length, (a[1] - b[1]) / length];
    let turn = toe.vertices[across];
    // Round a box pad, to its far end: on in line with the end of it on the crest, across the berm to the line. With none,
    // out to the join.
    let to_end = pad.map(|[_, wall]| (wall[0] - turn[0]) * back[0] + (wall[1] - turn[1]) * back[1]).filter(|reach| *reach > SAME_POINT);
    let boxed = to_end.is_some();
    let reach = to_end.unwrap_or(turn_join);
    let on = |p: P3| (p[0] - turn[0]) * back[0] + (p[1] - turn[1]) * back[1] - reach;
    let (mut kept, mut join) = (from + 1, toe.vertices[from]);
    for m in (from..across).rev() {
        let (far, near) = (on(toe.vertices[m]), on(toe.vertices[m + 1]));
        if far >= 0.0 {
            (kept, join) = (m + 1, point(m, if near >= 0.0 { 1.0 } else { far / (far - near) }));
            break;
        }
    }
    // A box's far end is where the end of it on the crest, carried on square across, meets the line. The berm's line
    // comes square down the far end from there (`Flat::squared_end`), so that's the first vertex of the toe on it from
    // the box's wall side out, the wall side's end itself where there's no berm beside the box; a line that doesn't, it
    // crosses. Its back corner is a road width on out from there.
    let road = plan_gap(corner, turn);
    let mut back_corner = None;
    let mut flared_onto = None;
    if let (true, Some([pit, wall])) = (boxed, pad) {
        let across_end = plan_gap(pit, wall);
        let reach = 3.0 * (across_end + plan_gap(wall, turn));
        let way = [(wall[0] - pit[0]) / across_end, (wall[1] - pit[1]) / across_end];
        let off = |q: P3| ((q[0] - pit[0]) * way[1] - (q[1] - pit[1]) * way[0]).abs();
        let out = |q: P3| (q[0] - pit[0]) * way[0] + (q[1] - pit[1]) * way[1];
        let ray = (plan(pit), [pit[0] + way[0] * reach, pit[1] + way[1] * reach]);
        if let Some(m) = (from..across).rev().find(|&m| off(toe.vertices[m]) < 1e-6 && out(toe.vertices[m]) > across_end - SAME_POINT) {
            (kept, join) = (m + 1, toe.vertices[m]);
        } else if let Some((m, u)) = (from..across).rev().find_map(|m| segment_crossing(ray.0, ray.1, plan(toe.vertices[m]), plan(toe.vertices[m + 1])).map(|(_, u)| (m, u))) {
            (kept, join) = (m + 1, point(m, u));
        }
        let corner_out = [join[0] + way[0] * road, join[1] + way[1] * road, join[2]];
        back_corner = Some(corner_out);
        // Flared out past the box: from its back corner at the end's angle to the strip, on the way the line runs past the
        // turn and back towards the pit, out to the line past the box, or the end of the string if it doesn't get there.
        let (sin, cos) = end.to_radians().sin_cos();
        if cos > 1e-9 {
            let square = [way[1], -way[0]];
            let along = if square[0] * back[0] + square[1] * back[1] >= 0.0 { square } else { [-square[0], -square[1]] };
            let slant = [along[0] * cos - way[0] * sin, along[1] * cos - way[1] * sin];
            let reach = 4.0 * (out(corner_out).abs() + road) / sin.max(0.1);
            let ray = (plan(corner_out), [corner_out[0] + slant[0] * reach, corner_out[1] + slant[1] * reach]);
            let met = (from..kept - 1).rev().find_map(|m| segment_crossing(ray.0, ray.1, plan(toe.vertices[m]), plan(toe.vertices[m + 1])).map(|(_, u)| (m, u)));
            (kept, join) = match met {
                Some((m, u)) => (m + 1, point(m, u)),
                None => (from + 1, toe.vertices[from]),
            };
            flared_onto = Some(join);
        }
    }
    // Where the join crosses the toe on its way, nearest the corner first: out over it, back in, and so on.
    let span = plan_gap(corner, join);
    let mut crossings: Vec<(f64, usize, f64)> = (kept..across).filter_map(|m| segment_crossing(plan(corner), plan(join), plan(toe.vertices[m]), plan(toe.vertices[m + 1])).map(|(t, u)| (t * span, m, u))).collect();
    crossings.sort_by(|x, y| x.0.total_cmp(&y.0));
    // Through a vertex of the toe the join crosses it once. One that only touches it there goes out and straight back in.
    let which_side = |p: P3| (join[0] - corner[0]) * (p[1] - corner[1]) - (join[1] - corner[1]) * (p[0] - corner[0]);
    crossings.dedup_by(|x, y| {
        let first = x.1.min(y.1);
        (x.0 - y.0).abs() < SAME_POINT && (x.1.abs_diff(y.1) != 1 || which_side(toe.vertices[first]) * which_side(toe.vertices[first + 2]) < 0.0)
    });
    for pair in crossings.chunks(2).filter(|_| !boxed) {
        let back_in = pair.get(1).map_or(span, |c| c.0);
        if back_in - pair[0].0 >= road {
            (kept, join) = (pair[0].1 + 1, point(pair[0].1, pair[0].2));
            break;
        }
    }
    let mut behind = stays(kept);
    behind.meets.extend(flared_onto);
    if behind.vertices.last().is_some_and(|last| plan_gap(*last, join) < SAME_POINT) {
        behind.vertices.pop();
        behind.batters.pop();
    }
    // A box pad's far end runs off the line, square or flared, to its back corner, a road width in from the line as the
    // next road's wall-side start is, and its back runs from there to that start: the wall above stands round it.
    let mut laid: Vec<P3> = Vec::new();
    let mut from_join = join;
    if boxed {
        let back_corner = back_corner.unwrap_or([join[0] + corner[0] - turn[0], join[1] + corner[1] - turn[1], join[2]]);
        laid = laid_on(Polyline { vertices: vec![join, back_corner], closed: false })?;
        from_join = laid.pop().unwrap_or(back_corner);
    }
    // On to the corner on the floor. The corner is level with the road's start across the road, so a floor dipping into
    // the wall leaves it standing above the floor: then the toe lies on the floor to a road width short of it and leads
    // straight up to it, as an entrance's toe does, with no step at the corner.
    let mut tail = laid_on(Polyline { vertices: vec![from_join, corner], closed: false })?;
    let on_the_floor = tail.pop().is_none_or(|under| (under[2] - corner[2]).abs() <= SAME_POINT);
    let (out, lead) = (plan_gap(from_join, corner), plan_gap(from_join, corner) - road);
    if !on_the_floor {
        tail = if lead > SAME_POINT {
            let short = [from_join[0] + lead / out * (corner[0] - from_join[0]), from_join[1] + lead / out * (corner[1] - from_join[1]), from_join[2]];
            laid_on(Polyline { vertices: vec![from_join, short], closed: false })?
        } else {
            vec![from_join]
        };
    }
    laid.extend(tail);
    for k in 0..laid.len() {
        let (p, q) = (laid[k], *laid.get(k + 1).unwrap_or(&corner));
        behind.batters.push(batter_nearest(toe, batters, [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, 0.0]));
    }
    behind.vertices.extend(laid);
    Ok(with_flat(behind))
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

/// The line a berm runs out to, where roads come up through it. A road is the bench there, so the line comes in
/// to the top of each: at 45 degrees behind it, and over a long taper ahead so the road can ease across the
/// berm's width. Each top says whether its road runs back against the line. `lay` puts each stretch on what the berm
/// lies on.
pub(crate) fn berm_round_roads(line: &Polyline, crest: &Polyline, tops: &[(RoadTop, bool)], lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Polyline, GeomError> {
    let only: Vec<RoadTop> = tops.iter().map(|(top, _)| *top).collect();
    if tops.iter().all(|(_, back)| *back) {
        let turned = berm_round_roads_along(&backwards(line), &backwards(crest), &only, lay)?;
        return Ok(backwards(&turned));
    }
    if tops.iter().all(|(_, back)| !*back) {
        return berm_round_roads_along(line, crest, &only, lay);
    }
    berm_round_roads_both_ways(line, crest, tops, lay)
}

/// A berm's line round the tops of roads running both ways, as the line runs: behind a road along it the line comes in
/// to its wall-side top and opens out ahead from its pit-side top, and round one running back it's the other way about.
/// Between two roads whose tops face each other the berm opens out from each along the crest until the two tapers meet,
/// where they're equally deep.
fn berm_round_roads_both_ways(line: &Polyline, crest: &Polyline, tops: &[(RoadTop, bool)], lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Polyline, GeomError> {
    struct Round {
        top: RoadTop,
        back: bool,
        /// Where it leaves the line for the road and where it's back, as the line runs.
        behind: f64,
        ahead: f64,
        /// The points from the line in to the road's top, and out from it.
        enter: Vec<P3>,
        leave: Vec<P3>,
        /// How far along the crest the berm takes to open out from its top.
        taper: f64,
    }
    let last = (line.vertices.len() - 1) as f64;
    let (turned_line, turned_crest) = (backwards(line), backwards(crest));
    let mut rounds: Vec<Round> = tops
        .iter()
        .filter_map(|&(top, back)| {
            let taper = BERM_TAPER * plan_gap(point_at(line, nearest_place(line, top.pit)), top.pit);
            if back {
                let t = taper_at(&turned_line, &turned_crest, top);
                (t.behind < t.ahead).then(|| Round { top, back, behind: last - t.ahead, ahead: last - t.behind, enter: t.opening.iter().rev().copied().chain(once(top.pit)).collect(), leave: vec![top.wall], taper })
            } else {
                let t = taper_at(line, crest, top);
                (t.behind < t.ahead).then(|| Round { top, back, behind: t.behind, ahead: t.ahead, enter: vec![top.wall], leave: once(top.pit).chain(t.opening.iter().copied()).collect(), taper })
            }
        })
        .collect();
    rounds.sort_by(|a, b| a.behind.total_cmp(&b.behind));
    let Some(first) = rounds.first() else { return lay(line) };
    // A stretch laid on what the berm lies on, with the road tops at its ends kept where they are.
    let mut laid_between = |vertices: Vec<P3>, first_kept: bool, last_kept: bool| -> Result<Vec<P3>, GeomError> {
        if vertices.len() < 2 {
            return Ok(vertices);
        }
        let (start, end) = (vertices[0], vertices[vertices.len() - 1]);
        let mut laid = lay(&Polyline { vertices, closed: false })?.vertices;
        if first_kept {
            laid[0] = start;
        }
        if last_kept {
            *laid.last_mut().expect("a line has an end") = end;
        }
        Ok(laid)
    };
    let before: Vec<P3> = if first.behind > 0.0 { stretch_of(line, 0.0, first.behind).vertices } else { Vec::new() };
    let mut vertices = laid_between(before.into_iter().chain(first.enter.iter().copied()).collect(), false, true)?;
    let crest_length = crest.plan_length();
    for pair in rounds.windows(2) {
        let (from, to) = (&pair[0], &pair[1]);
        let stretch: Vec<P3> = if !from.back && to.back && from.ahead > to.behind {
            // Tops facing, too close for both to open out all the way: each opens along the crest to where they meet.
            let (at_from, at_to) = (metres_along(crest, from.top.pit), metres_along(crest, to.top.pit));
            let apart = at_to - at_from;
            let (reach_from, reach_to) = (from.taper.min(crest_length - at_from), to.taper.min(at_to));
            let mut stretch = vec![from.top.pit];
            if apart > 0.0 && reach_from + reach_to > 0.0 {
                let meet = apart * reach_from / (reach_from + reach_to);
                stretch.extend(berm_opening_part(crest, line, from.top.pit, from.taper, meet));
                let theirs = berm_opening_part(&turned_crest, line, to.top.pit, to.taper, apart - meet);
                stretch.extend(theirs.into_iter().rev().skip(1));
            }
            stretch.push(to.top.pit);
            stretch
        } else {
            // Two roads close together leave no room for the berm to open out all the way between them.
            let leaving = from.leave.iter().enumerate().filter(|(k, p)| *k == 0 || nearest_place(line, **p) < to.behind).map(|(_, p)| *p);
            let on: Vec<P3> = if from.ahead < to.behind { stretch_of(line, from.ahead, to.behind).vertices } else { Vec::new() };
            let coming = to.enter.iter().enumerate().filter(|(k, p)| *k + 1 == to.enter.len() || nearest_place(line, **p) > from.ahead).map(|(_, p)| *p);
            leaving.chain(on).chain(coming).collect()
        };
        vertices.extend(laid_between(stretch, true, true)?);
    }
    let end = rounds.last().expect("there's a first");
    let on_past = if end.ahead < last { stretch_of(line, end.ahead, last).vertices } else { vec![line.vertices[line.vertices.len() - 1]] };
    vertices.extend(laid_between(end.leave.iter().copied().chain(on_past).collect(), true, false)?);
    Ok(Polyline { vertices, closed: false })
}

/// A berm's line opening out from a road's pit-side top along the crest ahead of it over `taper`, as `berm_opening` has
/// it, as far as `most` along the crest from the top, where it ends on the taper part way out.
fn berm_opening_part(crest: &Polyline, line: &Polyline, top: P3, taper: f64, most: f64) -> Vec<P3> {
    let ahead = stretch_of(crest, nearest_place(crest, top), (crest.vertices.len() - 1) as f64);
    let reach = taper.min(ahead.plan_length());
    let stop = most.min(reach);
    let on_taper = |p: P3, gone: f64| {
        let (share, out) = (if reach > 0.0 { gone / reach } else { 0.0 }, point_at(line, nearest_place(line, p)));
        [p[0] + share * (out[0] - p[0]), p[1] + share * (out[1] - p[1]), p[2]]
    };
    let mut gone = 0.0;
    let mut vertices = Vec::new();
    for w in ahead.vertices.windows(2) {
        let length = plan_gap(w[0], w[1]);
        if gone + length >= stop {
            vertices.push(on_taper(point_along(&ahead, stop), stop));
            return vertices;
        }
        gone += length;
        vertices.push(on_taper(w[1], gone));
    }
    vertices
}

/// Straight across a berm from a road's top, square to the crest behind the road, by the berm's width there: the way
/// across in plan, and the way the road was going along the crest when it came up.
fn across_crest(crest: &Polyline, top: RoadTop, width: EdgeRule<'_>, side: OffsetSide) -> ([f64; 2], [f64; 2]) {
    const ON: f64 = 1e-3;
    let n = crest.vertices.len();
    let ends = |i: usize| (crest.vertices[i], crest.vertices[(i + 1) % n]);
    let gap = |i: usize| {
        let (a, b) = ends(i);
        nearest_along(a, b, top.wall).1
    };
    // Of the crest's segments at the wall-side top, the one behind the road, not the one across the road's top. A top
    // that crosses the road at a skew, as on a grid, brings the crest just behind it as near the pit-side corner as the
    // wall-side corner is, so nearness alone can't tell them apart: a segment lying along the crossing is the crossing.
    let segments: Vec<usize> = (0..segment_count(crest)).collect();
    let far_from_road = |i: usize| {
        let (a, b) = ends(i);
        plan_gap(a, top.pit).max(plan_gap(b, top.pit))
    };
    let across_the_top = |i: usize| {
        let (a, b) = ends(i);
        nearest_along(top.wall, top.pit, a).1 < ON && nearest_along(top.wall, top.pit, b).1 < ON
    };
    let behind = match segments.iter().copied().filter(|&i| gap(i) < ON && !across_the_top(i)).max_by(|&i, &j| far_from_road(i).total_cmp(&far_from_road(j))) {
        Some(i) => i,
        None => segments.iter().copied().min_by(|&i, &j| gap(i).total_cmp(&gap(j))).expect("a crest has a segment"),
    };
    let (a, b) = ends(behind);
    let length = plan_gap(a, b);
    let along = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    let out = if side == OffsetSide::Left { [-along[1], along[0]] } else { [along[1], -along[0]] };
    let middle = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
    let w = width.value_at(middle);
    // The road came along the crest behind it to its top.
    let heading = if (top.wall[0] - middle[0]) * along[0] + (top.wall[1] - middle[1]) * along[1] < 0.0 { [-along[0], -along[1]] } else { along };
    ([out[0] * w, out[1] * w], heading)
}

/// Straight across a berm from a road's top: the road's top crossing carried on out to the berm's line, so the road on
/// the next wall starts in line with the top of this one, as Deswik's do (decision 0038). Square to the crest by the
/// berm's width where the crossing doesn't reach the line. Gives the way across, and whether it's in line.
fn across_to_line(line: &Polyline, crest: &Polyline, top: RoadTop, width: EdgeRule<'_>, side: OffsetSide) -> ([f64; 2], bool) {
    let (square, _) = across_crest(crest, top, width, side);
    let across = [top.wall[0] - top.pit[0], top.wall[1] - top.pit[1]];
    let road = across[0].hypot(across[1]);
    if road <= SAME_POINT {
        return (square, false);
    }
    let reach = 3.0 * (square[0].hypot(square[1]) + road);
    let (from, to) = ([top.wall[0], top.wall[1]], [top.wall[0] + across[0] / road * reach, top.wall[1] + across[1] / road * reach]);
    let hit = line.vertices.windows(2).filter_map(|w| segment_crossing(from, to, [w[0][0], w[0][1]], [w[1][0], w[1][1]]).map(|(t, _)| t)).fold(f64::INFINITY, f64::min);
    if hit.is_finite() { ([(to[0] - from[0]) * hit, (to[1] - from[1]) * hit], true) } else { (square, false) }
}

/// The flat a road that turns back runs past its top before it turns (decision 0038, Tim 2026-11-04): on along the crest
/// the way it came up, `length` metres, a road wide on the wall side of the crest, in line with the road and in front of
/// where the next road starts, so nothing sits back over the road below. Its pit side is the crest's own stretch on from
/// the road's pit-side top and its wall side is that moved back by the road's width at its top, laid by `lay` on what the
/// berm lies on: on a seam floor the flat lies on the floor, so no coal is left under it. None where the crest has no
/// room past the top, or the top isn't on it. Gives the two sides from the road's top, in the road's direction, and the
/// places on the crest the flat starts and ends at.
pub(crate) fn flat_past_top(crest: &Polyline, top: RoadTop, length: f64, berm: EdgeRule<'_>, side: OffsetSide, lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Option<Flat>, GeomError> {
    let Some(at) = crest.vertices.iter().position(|p| plan_gap(*p, top.pit) < SAME_POINT) else { return Ok(None) };
    // On along the crest away from the road's own top crossing, which is one of the crest's two ways from its pit-side
    // top. Where neither is, the way the road was going along the crest.
    let crossing = |k: usize| crest.vertices.get(k).is_some_and(|p| nearest_along(top.pit, top.wall, *p).1 < 1e-3);
    let forward = match (at.checked_sub(1).is_some_and(crossing), crossing(at + 1)) {
        (true, false) => true,
        (false, true) => false,
        _ => {
            let (_, heading) = across_crest(crest, top, berm, side);
            let ahead = |k: usize| (crest.vertices[k][0] - top.pit[0]) * heading[0] + (crest.vertices[k][1] - top.pit[1]) * heading[1];
            match (crest.vertices.get(at + 1), at.checked_sub(1)) {
                (Some(_), Some(before)) => ahead(at + 1) >= ahead(before),
                (Some(_), None) => true,
                (None, _) => false,
            }
        }
    };
    // The crest from the road's pit-side top on the way the road came up, `length` metres of it.
    let mut pit = vec![top.pit];
    let mut left = length;
    let mut k = at;
    let mut end = None;
    while left > SAME_POINT {
        let next = if forward { k + 1 } else { match k.checked_sub(1) { Some(j) => j, None => break } };
        let Some(&q) = crest.vertices.get(next) else { break };
        let p = crest.vertices[k];
        let gap = plan_gap(p, q);
        if gap >= left {
            let t = left / gap;
            let stop = [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])];
            if (1.0 - t) * gap < SAME_POINT {
                pit.push(q);
                end = Some((next, true));
            } else {
                pit.push(stop);
                end = Some((next, false));
            }
            left = 0.0;
        } else {
            pit.push(q);
            left -= gap;
            k = next;
            end = Some((next, true));
        }
    }
    let Some((last, on_vertex)) = end else { return Ok(None) };
    if pit.len() < 2 {
        return Ok(None);
    }
    // A road wide square to the crest: as far in as the road's own wall-side top stands from it.
    let road = pit.windows(2).map(|w| nearest_along(w[0], w[1], top.wall).1).fold(f64::INFINITY, f64::min);
    let way = if forward { side } else if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
    let plan = Polyline { vertices: pit.clone(), closed: false };
    let moved = longest(offset_string(&plan, road, way, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?);
    // It starts at the road's own wall-side top, so the two share the road's top edge, and runs on along the crest moved
    // back from there. A top that crosses the road at a skew leaves that corner off the moved crest, ahead of where it
    // starts or behind: it comes straight onto it, on ahead of the corner, never back.
    let corner = nearest_place(&moved, top.wall);
    if corner >= (moved.vertices.len() - 1) as f64 - 1e-9 {
        return Ok(None);
    }
    let ahead: Vec<P3> = moved.vertices[corner.floor() as usize + 1..].to_vec();
    let mut wall = lay(&Polyline { vertices: once(point_at(&moved, corner)).chain(ahead).collect(), closed: false })?.vertices;
    let square = usize::from(plan_gap(wall[0], top.wall) >= SAME_POINT);
    if square == 0 {
        wall[0] = top.wall;
    } else {
        wall.insert(0, top.wall);
    }
    // Its far end, square across from the end of it on the crest, laid as its sides are.
    let (end, wall_end) = (*pit.last().expect("a flat has an end"), *wall.last().expect("a flat has a wall side"));
    let far_end = lay(&Polyline { vertices: vec![wall_end, end], closed: false })?.vertices;
    let far = far_end.get(1..far_end.len().saturating_sub(1)).unwrap_or(&[]).to_vec();
    let flat = Flat { pit: Polyline { vertices: pit, closed: false }, wall: Polyline { vertices: wall, closed: false }, far, square, from: at, to: last, to_on_vertex: on_vertex, forward };
    Ok(Some(flat))
}

/// A flat a road runs past its top before it turns: its two sides from the road's top, in the road's direction, and where it
/// is on the crest: the vertex at the road's pit-side top, the vertex it ends at or past, whether it ends on that vertex,
/// and whether it runs the way the crest does.
#[derive(Debug, Clone)]
pub(crate) struct Flat {
    pub pit: Polyline,
    pub wall: Polyline,
    /// The far end between the two sides' ends, from the wall side's: the points it's laid on between them.
    pub far: Vec<P3>,
    /// Where the wall side comes square across from the pit side: after the road's own corner, if its top is skewed.
    pub square: usize,
    pub from: usize,
    pub to: usize,
    pub to_on_vertex: bool,
    pub forward: bool,
}

impl Flat {
    /// The crest a berm runs out from with the flat in it: round the flat's wall side, as round the top of a road, so the
    /// berm stands out beyond the flat and the next road comes onto its line in front of the flat's end.
    pub(crate) fn round(&self, crest: &Polyline) -> Polyline {
        let c = &crest.vertices;
        let wall = &self.wall.vertices;
        let end = *self.pit.vertices.last().expect("a flat has an end");
        let mut vertices = Vec::with_capacity(c.len() + wall.len());
        // The crest steps from the road's wall-side top to its pit-side top; that step moves on to the flat's end.
        if self.forward {
            vertices.extend_from_slice(&c[..self.from]);
            vertices.extend_from_slice(wall);
            vertices.extend_from_slice(&self.far);
            vertices.push(end);
            let after = if self.to_on_vertex { self.to + 1 } else { self.to };
            vertices.extend_from_slice(&c[after.min(c.len())..]);
        } else {
            let before = if self.to_on_vertex { self.to } else { self.to + 1 };
            vertices.extend_from_slice(&c[..before]);
            vertices.push(end);
            vertices.extend(self.far.iter().rev());
            vertices.extend(wall.iter().rev());
            vertices.extend_from_slice(&c[self.from + 1..]);
        }
        vertices.dedup_by(|a, b| plan_gap(*a, *b) < SAME_POINT);
        Polyline { vertices, closed: crest.closed }
    }

    /// The road's strip of the box, as a band: its pit side along the crest and round its far end, and its wall side.
    pub(crate) fn road_strip(&self) -> (Polyline, Polyline) {
        let pit = self.pit.vertices.iter().copied().chain(self.far.iter().rev().copied()).collect();
        (Polyline { vertices: pit, closed: false }, self.wall.clone())
    }

    /// The berm's line round the box squared off at its far end (Tim, 2026-11-04). The berm stands out into the wall from
    /// the crest, not on past the box along it, so the line runs beside the box, out from its wall side, only as far as
    /// the box's far end carried on square across, and comes straight back down that to `plain`, the berm's line with no
    /// box, along the crest's own far end where the two run together. It goes on along `plain` till it's back on `line`.
    /// `line` is the berm's line from the crest with the box in it, which stands round the far end too. Gives the squared
    /// line and where it comes onto the crest's far end part way along it, for the sheet to be split at; None, and the
    /// line as it is, where the far end doesn't meet the two lines that way.
    pub(crate) fn squared_end(&self, line: &Polyline, plain: &Polyline) -> Option<(Polyline, Option<P3>)> {
        if !self.forward {
            let (squared, meets) = self.squared_ahead(&backwards(line), &backwards(plain))?;
            return Some((backwards(&squared), meets));
        }
        self.squared_ahead(line, plain)
    }

    /// The same for a box running the way the lines do.
    fn squared_ahead(&self, line: &Polyline, plain: &Polyline) -> Option<(Polyline, Option<P3>)> {
        let (end, wall_end) = (*self.pit.vertices.last()?, *self.wall.vertices.last()?);
        let across = plan_gap(end, wall_end);
        if across < SAME_POINT {
            return None;
        }
        let out = [(wall_end[0] - end[0]) / across, (wall_end[1] - end[1]) / across];
        // The far end, from the end on the crest out to the wall side, by how far out each of its points is.
        let mut far: Vec<(f64, P3)> = once(end).chain(self.far.iter().rev().copied()).chain(once(wall_end)).map(|p| ((p[0] - end[0]) * out[0] + (p[1] - end[1]) * out[1], p)).collect();
        far.sort_by(|a, b| a.0.total_cmp(&b.0));
        let height_out = |t: f64| {
            let k = far.windows(2).position(|w| t <= w[1].0).unwrap_or(far.len() - 2);
            let ((ta, a), (tb, b)) = (far[k], far[k + 1]);
            let s = if tb > ta { ((t - ta) / (tb - ta)).clamp(0.0, 1.0) } else { 0.0 };
            a[2] + s * (b[2] - a[2])
        };
        // Where a line crosses the far end carried on both ways: how far out, on which segment and how far along it.
        let crossings = |l: &Polyline| -> Vec<(f64, usize, f64)> {
            l.vertices
                .windows(2)
                .enumerate()
                .filter_map(|(i, w)| {
                    let d = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
                    let denom = d[0] * out[1] - d[1] * out[0];
                    if denom.abs() < 1e-9 * (d[0].hypot(d[1]) + 1.0) {
                        return None;
                    }
                    let e = [end[0] - w[0][0], end[1] - w[0][1]];
                    let s = (e[0] * out[1] - e[1] * out[0]) / denom;
                    let t = (w[0][0] + s * d[0] - end[0]) * out[0] + (w[0][1] + s * d[1] - end[1]) * out[1];
                    (-1e-9..=1.0 + 1e-9).contains(&s).then_some((t, i, s.clamp(0.0, 1.0)))
                })
                .collect()
        };
        let at = |l: &Polyline, i: usize, s: f64| {
            let (a, b) = (l.vertices[i], l.vertices[i + 1]);
            [a[0] + s * (b[0] - a[0]), a[1] + s * (b[1] - a[1]), a[2] + s * (b[2] - a[2])]
        };
        // Beside the box: the line out from its wall side, where it crosses the far end beyond the wall side's end. Where
        // the berm has no width along the far end, as where the minimum bench drops it past the box, the line comes down
        // onto the far end there and runs down it through the wall side's end: from where it comes onto it, which is the
        // wall side's end itself where the berm has none beside the box either.
        let off = |q: P3| ((q[0] - end[0]) * out[1] - (q[1] - end[1]) * out[0]).abs();
        let out_to = |q: P3| (q[0] - end[0]) * out[0] + (q[1] - end[1]) * out[1];
        let onto_end = (0..line.vertices.len().saturating_sub(1)).find(|&m| nearest_along(line.vertices[m], line.vertices[m + 1], wall_end).1 < ON_THE_END && off(line.vertices[m]) < ON_THE_END).map(|mut j| {
            while j > 0 && off(line.vertices[j - 1]) < ON_THE_END && out_to(line.vertices[j - 1]) >= out_to(line.vertices[j]) - SAME_POINT {
                j -= 1;
            }
            j
        });
        let (t_beside, i, beside) = match onto_end {
            Some(j) if j > 0 => {
                let q = line.vertices[j];
                let t = out_to(q);
                (t, j - 1, [end[0] + out[0] * t, end[1] + out[1] * t, q[2]])
            }
            _ => {
                let (t, i, s) = crossings(line).into_iter().filter(|c| c.0 > across + SAME_POINT).min_by(|a, b| a.0.total_cmp(&b.0))?;
                (t, i, at(line, i, s))
            }
        };
        // Past it: the berm's line with no box, nearest the crest.
        let (t_past, k, _) = crossings(plain).into_iter().filter(|c| c.0 > -SAME_POINT).min_by(|a, b| a.0.total_cmp(&b.0))?;
        if t_beside < across - SAME_POINT || t_past >= t_beside - SAME_POINT {
            return None;
        }
        let past = [end[0] + out[0] * t_past.max(0.0), end[1] + out[1] * t_past.max(0.0), height_out(t_past.max(0.0))];
        let mut vertices: Vec<P3> = line.vertices[..=i].to_vec();
        vertices.push(beside);
        // Down the far end, on the crest's own far end where the two run together.
        vertices.extend(far.iter().rev().filter(|(t, _)| *t > t_past + SAME_POINT && *t < t_beside - SAME_POINT).map(|(_, p)| *p));
        vertices.push(past);
        // On along the line with no box, until it's back on the line round the box.
        let back_on = plain.vertices[k + 1..].iter().enumerate().find_map(|(q, v)| line.vertices[i + 1..].iter().position(|w| plan_gap(*v, *w) < SAME_POINT).map(|r| (k + 1 + q, i + 1 + r)));
        match back_on {
            Some((q, r)) => {
                vertices.extend_from_slice(&plain.vertices[k + 1..q]);
                vertices.extend_from_slice(&line.vertices[r..]);
            }
            None => vertices.extend_from_slice(&plain.vertices[k + 1..]),
        }
        vertices.dedup_by(|a, b| plan_gap(*a, *b) < SAME_POINT);
        let part_way = (t_past > SAME_POINT && t_past < across - SAME_POINT).then_some(past);
        Some((Polyline { vertices, closed: line.closed }, part_way))
    }

    /// The steepest the flat's two sides rise or fall along it, as a fraction: not across the road from a skewed top.
    pub(crate) fn steepest(&self) -> f64 {
        let along = self.pit.vertices.windows(2).chain(self.wall.vertices[self.square..].windows(2));
        along.filter(|w| plan_gap(w[0], w[1]) > SAME_POINT).map(|w| ((w[1][2] - w[0][2]) / plan_gap(w[0], w[1])).abs()).fold(0.0, f64::max)
    }
}

/// The nearest point on a line to a point, in plan, at the line's height there.
pub(crate) fn nearest_on(line: &Polyline, p: P3) -> P3 {
    point_at(line, nearest_place(line, p))
}

/// A berm's line nearer the far end of a switchback box than this runs along it (m): a line with no width there comes
/// out of the offset that close to it.
const ON_THE_END: f64 = 0.1;

/// Where a road turning back starts the next wall: straight across the berm from its wall-side top. That's a road width
/// plus the berm out from where the road came up, so the pad it turns on is the two roads and the berm between them.
pub(crate) fn straight_across(line: &Polyline, crest: &Polyline, top: RoadTop, width: EdgeRule<'_>, side: OffsetSide) -> P3 {
    let (shift, _) = across_to_line(line, crest, top, width, side);
    [top.wall[0] + shift[0], top.wall[1] + shift[1], top.wall[2]]
}

/// Two roads meeting across a berm closer than this at its corner meet there: a berm narrower than this between two
/// walls of no height is no berm (m).
const ROADS_MEET: f64 = 0.5;

/// Where a road that goes past a berm after it carries on from, and the berm's line to suit. The road crosses the berm
/// straight across, level, in line with its top, and climbs the next wall from there (decision 0038). The full berm runs
/// round the top of the road, so its line steps straight across at the road, from the top of the road's pit side to
/// where its wall side carries on. Where the road is as wide as the berm or wider, the next stretch starts on the top
/// of this one and the berm's line runs down it to there: the two roads meet with no berm between. Where the berm's line
/// can't be read that way, the road crosses a berm width on along itself, as it did before (0035).
/// Also gives the points where the two stretches meet part way along an edge of one of them, for the sheet to be split
/// at.
pub(crate) fn across_the_berm(line: &Polyline, crest: &Polyline, top: RoadTop, width: EdgeRule<'_>, side: OffsetSide) -> (Polyline, RoadTop, Vec<P3>) {
    let (_, heading) = across_crest(crest, top, width, side);
    let (shift, in_line) = across_to_line(line, crest, top, width, side);
    let wall_plan = [top.wall[0] + shift[0], top.wall[1] + shift[1], 0.0];
    let pit_plan = [top.pit[0] + shift[0], top.pit[1] + shift[1], 0.0];
    let at_wall = nearest_place(line, wall_plan);
    let last = (line.vertices.len() - 1) as f64;
    // The line steps round the road's top ahead of it, the way the road was going. Where the road is as wide as the
    // berm, the top of its pit side is as near the line behind as ahead, so the line's own nearness can't say.
    let (behind, ahead) = (point_at(line, (at_wall - 1.0).max(0.0)), point_at(line, (at_wall + 1.0).min(last)));
    let towards_pit = if (ahead[0] - behind[0]) * heading[0] + (ahead[1] - behind[1]) * heading[1] < 0.0 { -1.0 } else { 1.0 };
    // The step is about a berm width on from the road's top; three times that and the road's width leaves room for a
    // line on a grid, and keeps the walk off a bend in the line further on.
    let reach = 3.0 * (shift[0].hypot(shift[1]) + plan_gap(top.wall, top.pit));
    let Some(corner) = past_the_step(line, at_wall, towards_pit, heading, reach) else {
        let (line, landing) = along_the_road(line, top);
        return (line, landing, Vec::new());
    };
    let wall = [wall_plan[0], wall_plan[1], point_at(line, at_wall)[2]];
    // In line with the road's top, the next stretch's pit side lies on that top where the road is wider than the berm,
    // and at its wall-side corner where they're as wide: the berm's line runs down the top of the road to there.
    let (road, out) = (plan_gap(top.wall, top.pit), shift[0].hypot(shift[1]));
    let (step, pit, meets) = if in_line && road > out + ROADS_MEET {
        let share = (road - out) / road;
        let pit = [pit_plan[0], pit_plan[1], top.wall[2] + share * (top.pit[2] - top.wall[2])];
        (vec![wall, top.wall, pit], pit, vec![pit, top.wall])
    } else if in_line && road > out - ROADS_MEET {
        (vec![wall, top.wall], top.wall, Vec::new())
    } else {
        let pit = [pit_plan[0], pit_plan[1], line.vertices[corner][2]];
        (vec![wall, pit], pit, Vec::new())
    };
    let mut vertices = if towards_pit < 0.0 {
        let mut before = stretch_of(line, 0.0, corner as f64).vertices;
        before.extend(step.iter().rev());
        before.extend(stretch_of(line, at_wall, last).vertices);
        before
    } else {
        let mut before = stretch_of(line, 0.0, at_wall).vertices;
        before.extend(step);
        before.extend(stretch_of(line, corner as f64, last).vertices);
        before
    };
    vertices.dedup_by(|a, b| plan_gap(*a, *b) < SAME_POINT);
    (Polyline { vertices, closed: line.closed }, RoadTop { pit, wall }, meets)
}

/// The vertex where a berm's line, walked from `from` one way, has come past the step it makes round a road's top, a
/// stretch running across the crest, and runs on along the crest again. None if it doesn't within `reach` metres.
fn past_the_step(line: &Polyline, from: f64, way: f64, along: [f64; 2], reach: f64) -> Option<usize> {
    let start = point_at(line, from);
    let order: Vec<usize> = if way < 0.0 { (0..=from.floor() as usize).rev().collect() } else { (from.ceil() as usize..line.vertices.len()).collect() };
    let (mut prev, mut prev_at, mut crossed) = (start, None, false);
    for i in order {
        let v = line.vertices[i];
        let d = plan_gap(prev, v);
        if d > SAME_POINT {
            let lengthwise = ((v[0] - prev[0]) * along[0] + (v[1] - prev[1]) * along[1]).abs() / d >= std::f64::consts::FRAC_1_SQRT_2;
            match (lengthwise, crossed) {
                (false, _) => crossed = true,
                (true, true) => return prev_at,
                (true, false) => {}
            }
        }
        if plan_gap(start, v) > reach {
            return None;
        }
        (prev, prev_at) = (v, Some(i));
    }
    None
}

/// The berm crossed a berm width on along the road, where its line steps round the road's top. The foot of the next
/// stretch of road is a straight edge across it, so the line runs straight across too: on a grid the step's corner can
/// stand off it a little, and would leave a sliver between the berm and the road.
fn along_the_road(line: &Polyline, top: RoadTop) -> (Polyline, RoadTop) {
    let at_pit = nearest_place(line, top.pit);
    let pit = point_at(line, at_pit);
    let at_wall = nearest_place(line, [pit[0] + top.wall[0] - top.pit[0], pit[1] + top.wall[1] - top.pit[1], 0.0]);
    let wall = point_at(line, at_wall);
    let (from, to) = if at_wall < at_pit { (at_wall, at_pit) } else { (at_pit, at_wall) };
    let mut vertices = stretch_of(line, 0.0, from).vertices;
    vertices.extend(stretch_of(line, to, (line.vertices.len() - 1) as f64).vertices);
    vertices.dedup_by(|a, b| plan_gap(*a, *b) < SAME_POINT);
    (Polyline { vertices, closed: line.closed }, RoadTop { pit, wall })
}

/// The berm opening out ahead of a road. Each crest vertex over the taper moves towards the berm line, by a
/// share that grows with the metres from the road, so it turns every corner the crest does and stays off
/// it. Gives the vertices and the place on the line where the berm has its full width.
fn berm_opening(crest: &Polyline, line: &Polyline, top: P3, taper: f64) -> (Vec<P3>, f64) {
    let ahead = stretch_of(crest, nearest_place(crest, top), (crest.vertices.len() - 1) as f64);
    let reach = taper.min(ahead.plan_length());
    let mut gone = 0.0;
    let mut vertices = Vec::new();
    for w in ahead.vertices.windows(2) {
        gone += plan_gap(w[0], w[1]);
        if gone >= reach {
            break;
        }
        let (share, out) = (gone / reach, point_at(line, nearest_place(line, w[1])));
        vertices.push([w[1][0] + share * (out[0] - w[1][0]), w[1][1] + share * (out[1] - w[1][1]), w[1][2]]);
    }
    (vertices, nearest_place(line, point_along(&ahead, reach)))
}

/// Where a road's top takes a berm's line in, and how it opens out again.
struct Taper {
    top: RoadTop,
    /// The places on the line where it leaves for the road and where it's back at full width.
    behind: f64,
    ahead: f64,
    opening: Vec<P3>,
}

fn taper_at(line: &Polyline, crest: &Polyline, top: RoadTop) -> Taper {
    let (at_wall, at_pit) = (nearest_place(line, top.wall), nearest_place(line, top.pit));
    let width = plan_gap(point_at(line, at_pit), top.pit);
    let behind = place_at_gap(line, at_wall, false, top.wall, std::f64::consts::SQRT_2 * width);
    let (opening, ahead) = berm_opening(crest, line, top.pit, BERM_TAPER * width);
    Taper { top, behind, ahead, opening }
}

fn berm_round_roads_along(line: &Polyline, crest: &Polyline, tops: &[RoadTop], lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Polyline, GeomError> {
    let last = (line.vertices.len() - 1) as f64;
    let mut tapers: Vec<Taper> = tops.iter().map(|top| taper_at(line, crest, *top)).filter(|taper| taper.behind < taper.ahead).collect();
    tapers.sort_by(|a, b| a.behind.total_cmp(&b.behind));
    let Some(first) = tapers.first() else { return lay(line) };
    let mut vertices = if first.behind > 0.0 {
        let mut before = stretch_of(line, 0.0, first.behind);
        before.vertices.push(first.top.wall);
        let mut laid = lay(&before)?.vertices;
        *laid.last_mut().expect("a line has an end") = first.top.wall;
        laid
    } else {
        vec![first.top.wall]
    };
    for pair in tapers.windows(2) {
        let (from, to) = (&pair[0], &pair[1]);
        // Two roads close together leave no room for the berm to open out all the way between them.
        let opening = from.opening.iter().copied().filter(|p| nearest_place(line, *p) < to.behind);
        let on: Vec<P3> = if from.ahead < to.behind { stretch_of(line, from.ahead, to.behind).vertices } else { Vec::new() };
        let stretch = Polyline { vertices: once(from.top.pit).chain(opening).chain(on).chain(once(to.top.wall)).collect(), closed: false };
        let mut laid = lay(&stretch)?.vertices;
        laid[0] = from.top.pit;
        *laid.last_mut().expect("a line has an end") = to.top.wall;
        vertices.extend(laid);
    }
    let end = tapers.last().expect("there's a first");
    let far = line.vertices[line.vertices.len() - 1];
    let on_past = if end.ahead < last { stretch_of(line, end.ahead, last).vertices } else { vec![far] };
    let mut laid = lay(&Polyline { vertices: once(end.top.pit).chain(end.opening.iter().copied()).chain(on_past).collect(), closed: false })?.vertices;
    laid[0] = end.top.pit;
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
        upper_batter(slope_nearest(toe, batters, middle), grade)
    }));
    (Polyline { vertices, closed: false }, slopes)
}

/// The toe segment nearest a point in plan.
fn nearest_segment(toe: &Polyline, batters: &[f64], p: P3) -> usize {
    let gap = |i: usize| nearest_along(toe.vertices[i], toe.vertices[i + 1], p).1;
    (0..batters.len()).min_by(|a, b| gap(*a).total_cmp(&gap(*b))).expect("a wall has a batter")
}

/// The batter at the toe segment nearest a point in plan.
fn batter_nearest(toe: &Polyline, batters: &[f64], p: P3) -> f64 {
    batters[nearest_segment(toe, batters, p)]
}

/// How steep the wall is at the toe segment nearest a point in plan, as rise over run straight up it. The batter
/// is square to a toe, and a toe that rises adds its own rise, as a road's wall-side edge does.
fn slope_nearest(toe: &Polyline, batters: &[f64], p: P3) -> f64 {
    let nearest = nearest_segment(toe, batters, p);
    let (a, b) = (toe.vertices[nearest], toe.vertices[nearest + 1]);
    let rise = if plan_gap(a, b) > 0.0 { (b[2] - a[2]) / plan_gap(a, b) } else { 0.0 };
    batters[nearest].to_radians().tan().hypot(rise)
}

/// The batter square to a road climbing a wall of this slope at `grade` that gives a wall parallel to the one
/// below. The road's rise is part of that wall's slope, and what's left of it is square to the road.
fn upper_batter(slope: f64, grade: f64) -> f64 {
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
        let no_wall = || Stopped { at: start, why: NO_WALL.into(), crosses: None };
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
        Err(Stopped { at: self.world(p), why: "it can't find a way up the wall here".into(), crosses: None })
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
            format!("it reaches the end of the wall at RL {:.1}, short of the top", at[2])
        };
        Err(Stopped { at, why, crosses: None })
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
    const RAMP: FaceRamp = FaceRamp { start: [E + 100.0, N], back: false, width: 10.0, grade: 0.1, ends_at: None, after_berm: false, turn_join: 50.0, turn_flat: 10.0, turn_end: 90.0 };

    fn level(rl: f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |_, _| rl))
    }

    fn string(points: &[[f64; 2]], rl: f64) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [E + p[0], N + p[1], rl]).collect(), closed: false }
    }

    /// A wall the road climbs.
    fn wall(to: FaceTarget<'_>, batter: f64) -> FaceStep<'_> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(batter), condition: None, ramps: &[0], turns: &[] }
    }

    fn plain_wall(to: FaceTarget<'_>, batter: f64) -> FaceStep<'_> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(batter), condition: None, ramps: &[], turns: &[] }
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
        TriangleBins::new(sheet, [E - 100.0, N - 100.0], [E + 1100.0, N + 300.0]).height(sheet, [E + x, N + y])
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
        assert!(stopped.why.contains("reaches the end of the wall"), "{}", stopped.why);
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

    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} is not {b} to {tolerance}");
    }

    /// How far a point is from a line in plan.
    fn off_line(line: &Polyline, p: [f64; 3]) -> f64 {
        line.vertices.windows(2).map(|w| nearest_along(w[0], w[1], p).1).fold(f64::INFINITY, f64::min)
    }

    /// Whether a point is on an edge of one of a face's roads, in plan.
    fn on_road(face: &Face, p: [f64; 3]) -> bool {
        face.roads.iter().any(|road| off_line(&road.pit_edge, p) < 1e-6 || off_line(&road.wall_edge, p) < 1e-6)
    }

    /// The same two walls and berm with the road going after the berm. It climbs the first wall, 200 m from RL 100
    /// to 120, crosses the berm level and straight across, in line with its top, and climbs the second 200 m more to RL
    /// 140 (decision 0038). The road is as wide as the berm, so the second stretch starts at the wall-side corner of the
    /// first one's top, and the landing is that top: 10 m across, a metre back as the road climbs at sin(angle) = 0.1.
    /// The berm keeps its full 10 m, where going through it narrows it to nothing at the road. Only the road's own corners
    /// on the berm's line come closer to the crest: where the two stretches meet, neither wall has any height.
    #[test]
    fn a_road_after_the_berm_crosses_it_level_and_climbs_on_from_the_next_toe() {
        let on = FaceTarget::Rl(120.0);
        let steps = || vec![wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Rl(140.0), 45.0)];
        let face = ramped(FaceRamp { after_berm: true, ..RAMP }, steps(), &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        assert_eq!(road.steps, [0, 2]);
        let pit = &road.pit_edge;
        let (climbing, flat): (Vec<_>, Vec<_>) = pit.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6).partition(|w| (w[1][2] - w[0][2]).abs() > 1e-9);
        let climbed: f64 = climbing.iter().map(|w| plan_gap(w[0], w[1])).sum();
        assert!((climbed - 400.0).abs() < 1e-3, "{climbed}");
        for w in &climbing {
            close((w[1][2] - w[0][2]) / plan_gap(w[0], w[1]), 0.1, 1e-9);
        }
        assert_eq!(flat.len(), 1, "one landing: {:?}", pit.vertices);
        let (top, foot) = (flat[0][0], flat[0][1]);
        close(top[2], 120.0, 1e-6);
        close(foot[2], 120.0, 1e-6);
        // Across the top of the first stretch.
        close(foot[0] - top[0], -10.0 * 0.1, 1e-6);
        close(foot[1] - top[1], 10.0 * COS, 1e-6);
        close(height(&face.surface, (top[0] + foot[0]) / 2.0 - E, (top[1] + foot[1]) / 2.0 - N), 120.0, 1e-6);
        close(pit.vertices.last().unwrap()[2], 140.0, 1e-6);
        one_clean_sheet(&face);

        let narrowest = |face: &Face, corners: bool| {
            let bench = |step: usize| face.benches.iter().find(|b| b.step == step).unwrap_or_else(|| panic!("no bench {step}"));
            let (crest, toe) = (&bench(0).crest, &bench(2).toe);
            toe.vertices.iter().filter(|p| corners || !on_road(face, **p)).map(|p| off_line(crest, *p)).fold(f64::INFINITY, f64::min)
        };
        close(narrowest(&face, false), 10.0, 1e-6);
        let through = ramped(RAMP, steps(), &level(FLOOR));
        assert!(narrowest(&through, true) < 1e-6, "going through takes the berm in to the road: {}", narrowest(&through, true));
    }

    /// Roads wider and narrower than the berm land in line with their tops too. The second stretch starts the berm's
    /// width on along that line, 10 m square to the crest: on the first stretch's top where the road is wider, out past
    /// it where it's narrower, so the landing is 10 m / cos(angle) long either way. Where it's within half a metre of the
    /// road's width, the two meet at the top's wall-side corner. Each face is one clean sheet with its berm whole.
    #[test]
    fn a_road_wider_or_narrower_than_the_berm_lands_in_line_with_its_top() {
        let on = FaceTarget::Rl(120.0);
        for (width, landing) in [(4.0, 10.0 / COS), (9.8, 9.8), (10.2, 10.2), (25.0, 10.0 / COS), (40.0, 10.0 / COS)] {
            let face = ramped(FaceRamp { width, after_berm: true, ..RAMP }, vec![wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Rl(140.0), 45.0)], &level(FLOOR));
            assert!(face.no_roads.is_empty(), "{width} m: {:?}", face.no_roads);
            let pit = &face.roads[0].pit_edge;
            let flat: Vec<_> = pit.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6 && (w[1][2] - w[0][2]).abs() <= 1e-9).collect();
            assert_eq!(flat.len(), 1, "{width} m: one landing: {:?}", pit.vertices);
            close(plan_gap(flat[0][0], flat[0][1]), landing, 1e-6);
            // In line with the road's top: a tenth back for every metre across.
            close((flat[0][1][0] - flat[0][0][0]) / (flat[0][1][1] - flat[0][0][1]), -0.1 / COS, 1e-9);
            close(pit.vertices.last().unwrap()[2], 140.0, 1e-6);
            one_clean_sheet(&face);
            let bench = |step: usize| face.benches.iter().find(|b| b.step == step).unwrap_or_else(|| panic!("no bench {step}"));
            let (crest, toe) = (&bench(0).crest, &bench(2).toe);
            close(toe.vertices.iter().filter(|p| !on_road(&face, **p)).map(|p| off_line(crest, *p)).fold(f64::INFINITY, f64::min), 10.0, 1e-6);
        }
    }

    /// A face from a level top line along +x at RL `top`, going down to the south.
    fn ramped_down(ramp: FaceRamp, steps: Vec<FaceStep<'_>>, top: f64) -> Face {
        let line = string(&[[0.0, 0.0], [600.0, 0.0]], top);
        project_face(&line, &level(top), OffsetSide::Left, FaceDirection::Down, &steps, &[ramp]).unwrap()
    }

    /// The first road turned upside down: at 10 % down a 45 degree wall 20 m high, 10 m wide, from a top line at RL
    /// 120. It leaves the top line where it's put, runs 200 m in plan down to RL 100, and finishes 198.997 m along and
    /// 20 m out to the south, the way the face goes down. Its pit-side edge is on that side, a road width out.
    #[test]
    fn a_road_down_a_straight_wall_holds_its_grade_and_its_width() {
        let face = ramped_down(RAMP, vec![wall(FaceTarget::Rl(100.0), 45.0)], 120.0);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        let (pit, wall) = (&road.pit_edge, &road.wall_edge);
        close(wall.plan_length(), 200.0, 1e-3);
        for grade in rise_over_run(pit).into_iter().chain(rise_over_run(wall)) {
            close(grade, -0.1, 1e-9);
        }
        let (start, end) = (wall.vertices[0], wall.vertices[wall.vertices.len() - 1]);
        assert_eq!([start[0] - E, start[1] - N, start[2]], [100.0, 0.0, 120.0], "it starts on the top line where it's put");
        close(end[0] - E, 100.0 + 200.0 * COS, 1e-3);
        close(end[1] - N, -20.0, 1e-3);
        close(end[2], 100.0, 1e-6);
        for (p, q) in [(start, pit.vertices[0]), (end, pit.vertices[pit.vertices.len() - 1])] {
            close(plan_gap(p, q), 10.0, 1e-6);
            close(p[2], q[2], 1e-6);
            assert!(q[1] < p[1], "the pit side is the side the face goes down to: {q:?} against {p:?}");
        }
        one_clean_sheet(&face);
        // Half way down, the road is level across its width at RL 110.
        let (p0, p1) = (pit.vertices[0], pit.vertices[pit.vertices.len() - 1]);
        let middle = [(start[0] + end[0] + p0[0] + p1[0]) / 4.0 - E, (start[1] + end[1] + p0[1] + p1[1]) / 4.0 - N];
        close(height(&face.surface, middle[0], middle[1]), 110.0, 1e-6);
    }

    /// A road's reasons for stopping are written as it climbs a face upside down; going down they're put right.
    #[test]
    fn a_road_going_down_says_why_it_stopped_the_right_way_up() {
        use crate::face::going_down;
        assert_eq!(going_down("it reaches the end of the wall at RL -105.3, short of the top"), "it reaches the end of the wall at RL 105.3, short of the bottom");
        assert_eq!(going_down("it drops back to the toe at RL -30.0: the toe rises faster than 10.0 % here"), "it climbs back to the top line at RL 30.0: the top line falls faster than 10.0 % here");
        assert_eq!(going_down("step 2: no wall at RL 0.0 or RL 12"), "step 2: no wall at RL 0.0 or RL -12");
    }

    /// Going down after the berm, mirroring the road up: down the first wall 200 m from RL 140 to 120, straight across
    /// the berm level on one landing in line with the road's top, and down the second 200 m more to RL 100. The berm
    /// keeps its full 10 m but at the road's own corners.
    #[test]
    fn a_road_down_after_the_berm_crosses_it_level_and_goes_on_down() {
        let on = FaceTarget::Rl(120.0);
        let face = ramped_down(FaceRamp { after_berm: true, ..RAMP }, vec![wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Rl(100.0), 45.0)], 140.0);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        assert_eq!(road.steps, [0, 2]);
        let edge = &road.wall_edge;
        let (going, flat): (Vec<_>, Vec<_>) = edge.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6).partition(|w| (w[1][2] - w[0][2]).abs() > 1e-9);
        close(going.iter().map(|w| plan_gap(w[0], w[1])).sum(), 400.0, 1e-3);
        for w in &going {
            close((w[1][2] - w[0][2]) / plan_gap(w[0], w[1]), -0.1, 1e-9);
        }
        assert_eq!(flat.len(), 1, "one landing: {:?}", edge.vertices);
        close(flat[0][0][2], 120.0, 1e-6);
        close(flat[0][1][0] - flat[0][0][0], -10.0 * 0.1, 1e-6);
        close(flat[0][1][1] - flat[0][0][1], -10.0 * COS, 1e-6);
        close(edge.vertices.last().unwrap()[2], 100.0, 1e-6);
        one_clean_sheet(&face);
        let bench = |step: usize| face.benches.iter().find(|b| b.step == step).unwrap_or_else(|| panic!("no bench {step}"));
        let (foot, next) = (&bench(0).crest, &bench(2).toe);
        close(next.vertices.iter().filter(|p| !on_road(&face, **p)).map(|p| off_line(foot, *p)).fold(f64::INFINITY, f64::min), 10.0, 1e-6);
    }

    /// A wall at 45 degrees climbed by ramp 0, turning back before it.
    fn turned_on<'a>(to: FaceTarget<'a>) -> FaceStep<'a> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(45.0), condition: None, ramps: &[0], turns: &[0] }
    }

    /// On a face with a road turning back on a pad: where the next road comes onto the berm's line and climbs from, its
    /// wall-side start, and the toe's vertex on past that corner: the back corner of a box pad, or the join past the turn
    /// with none.
    fn second_road_start(face: &Face) -> [P3; 3] {
        let road = road_of(face, 0);
        let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
        let onto = *road.pit_edge.vertices.iter().rev().find(|p| toe.iter().any(|q| plan_gap(**p, *q) < 1e-6)).expect("the next road starts on the toe");
        let at = toe.iter().position(|q| plan_gap(*q, onto) < 1e-6).expect("it's on the toe");
        [onto, toe[at + 1], toe[at + 2]]
    }

    /// How far on from where the next road came onto the line a point past the turn is, the way the line runs there.
    fn on_from_the_turn(face: &Face, step: usize, onto: P3, p: P3) -> f64 {
        let toe = &face.benches.iter().find(|b| b.step == step).expect("the wall above the pad").toe.vertices;
        let at = toe.iter().position(|q| plan_gap(*q, onto) < 1e-6).expect("the next road starts on the toe");
        let before = toe[at - 1];
        let length = plan_gap(before, onto);
        ((p[0] - onto[0]) * (onto[0] - before[0]) + (p[1] - onto[1]) * (onto[1] - before[1])) / length
    }

    /// A switchback on a flat pad, the standard box (Tim, 2026-11-04): the road climbs the first wall east at 10 %, 200 m
    /// from RL 100 to 120, to its top on the crest. The next road starts straight across the berm from that top, in line
    /// with the top crossing, and climbs the second wall back west at once, 200 m more to RL 140. The box is flat at RL
    /// 120 and runs on past the top in line with the road, its 10 m width unless the ramp says: the road's width of it
    /// along the crest, the 10 m berm beyond, and the next road's width beyond that, where the wall above goes round it.
    /// Its far end is square across from the end of it on the crest: the toe of the wall above runs from the next road's
    /// wall-side start along the box's back to that end, then square to the berm's line, which carries on from there.
    #[test]
    fn a_road_turns_back_on_a_flat_pad() {
        let low = FaceTarget::Rl(120.0);
        let face = long_face(&[climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))], &[ramp_at(500.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        let road = road_of(&face, 0);
        assert_eq!(road.steps, [0, 2]);
        close(climbs_and_landings(road).0, 400.0, 1e-3);
        let pit = &road.pit_edge.vertices;
        let top = pit[1];
        close(top[0] - E, 500.0 + 200.0 * COS, 1e-6);
        close(top[1] - N, 20.0, 1e-6);
        // Straight above the road's wall-side top, a road width back as the road climbs at sin(angle) = 0.1: on the line
        // where it's nearest that top, 10 m across the berm. The line has a 1 m bend there where it meets the line beside
        // the box, so nearest is its corner, 0.25 m along. From there the next road climbs at once.
        let [onto, corner, back_corner] = second_road_start(&face);
        let wall_top = road.wall_edge.vertices[1];
        close(wall_top[0] - top[0], -1.0, 1e-6);
        close(onto[0] - wall_top[0], 0.0, 0.3);
        close(plan_gap(onto, wall_top), 10.0, 0.01);
        close(onto[2], 120.0, 1e-6);
        assert_eq!(pit.iter().position(|p| plan_gap(*p, onto) < 1e-6), Some(2), "straight across the pad from the top: {pit:?}");
        assert!(pit[3][2] > 120.0 + 1e-6, "it climbs from there: {pit:?}");
        close(pit[pit.len() - 1][2], 140.0, 1e-6);
        close(plan_gap(corner, onto), 10.0, 1e-6);
        // Round the box: along its back, to its far end in line with the end of it on the crest, 10 m on past the top.
        let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
        let c = toe.iter().position(|p| plan_gap(*p, corner) < 1e-6).expect("the toe runs round the next road's start");
        let far = toe[c + 2];
        close(far[0], top[0] + 10.0, 1e-6);
        close(far[1] - N, 40.0, 1e-6);
        // Its far end square across, out to its back corner the next road's width from the line.
        close(back_corner[0], far[0], 1e-6);
        close(back_corner[1] - far[1], 10.0, 1e-6);
        // Squared off: from the line beside the box the toe comes straight back down the far end to the berm's line past
        // it, 10 m out from the crest, where the box's wall-side corner is too, and goes on along that.
        let down = toe[c + 3];
        close(down[0], far[0], 1e-6);
        close(down[1] - N, 30.0, 1e-6);
        // Flat in the box, its three strips; the wall above past its far end, with no berm round it; and the berm's line
        // from there to the end.
        for (along, out) in [(5.0, 5.0), (5.0, 15.0), (5.0, 25.0), (9.0, 28.0)] {
            close(height(&face.surface, top[0] - E + along, top[1] - N + out), 120.0, 1e-6);
        }
        assert!(height(&face.surface, top[0] - E + 15.0, top[1] - N + 25.0) > 121.0, "the wall above stands past the box");
        close(height(&face.surface, top[0] - E + 15.0, top[1] - N + 15.0), 125.0, 1e-6);
        let at = |x: f64| toe.windows(2).find(|w| w[0][0] - E <= x && w[1][0] - E >= x).map(|w| w[0][1] - N).expect("the toe runs there");
        close(at(1500.0), 30.0, 1e-6);
    }

    /// A box for a 20 m road and a 10 m berm, 25 m long, squared off at its far end, on a road climbing east and on one
    /// climbing west (Tim, 2026-11-04): the berm stands out into the wall from the crest, not on past the box along it.
    /// Along the far end, 25 m on past the road's top, the toe of the wall above has the box's back corner, 50 m out from
    /// the crest; the berm's line beside the box, 30 m out; the box's wall-side corner, 20 m out; and the berm's line past
    /// the box, 10 m out, where it goes on. Flat at RL 120 in the box and on the berm past it. The wall above rises from
    /// the far end at 45 degrees: 2 m past it and 15 m out it's at RL 122, the nose where the two walls meet.
    #[test]
    fn a_box_for_a_road_wider_than_its_berm_is_squared_off_at_its_far_end() {
        let low = FaceTarget::Rl(120.0);
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))];
        for (ramp, way) in [(ramp_at(500.0), 1.0), (FaceRamp { back: true, ..ramp_at(1000.0) }, -1.0)] {
            let face = long_face(&rows, &[FaceRamp { width: 20.0, turn_flat: 25.0, ..ramp }]);
            assert!(face.no_roads.is_empty(), "{way}: {:?}", face.no_roads);
            one_clean_sheet(&face);
            let top = *road_of(&face, 0).pit_edge.vertices.iter().find(|p| (p[2] - 120.0).abs() < 1e-6).expect("the road's top");
            close(top[1] - N, 20.0, 1e-6);
            let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
            let end = top[0] + way * 25.0;
            let far: Vec<f64> = toe.iter().filter(|p| (p[0] - end).abs() < 1e-6).map(|p| p[1] - top[1]).collect();
            for want in [10.0, 20.0, 30.0, 50.0] {
                assert!(far.iter().any(|got| (got - want).abs() < 1e-6), "{way}: the toe along the far end has {want} m out: {far:?}");
            }
            assert!(far.iter().all(|got| (10.0 - 1e-6..=50.0 + 1e-6).contains(got)), "{way}: nothing past the far end: {far:?}");
            assert!(toe.iter().all(|p| (p[0] - end) * way <= 1e-6 || p[1] - top[1] < 10.0 + 1e-6), "{way}: no berm round the far end: {toe:?}");
            let past = toe.iter().position(|p| (p[0] - end).abs() < 1e-6 && (p[1] - top[1] - 10.0).abs() < 1e-6).expect("down to the berm's line");
            let on = [toe.get(past + 1), past.checked_sub(1).and_then(|q| toe.get(q))].into_iter().flatten().find(|p| (p[0] - end).abs() > 1e-6).expect("then along it");
            close(on[1] - top[1], 10.0, 1e-6);
            assert!((on[0] - end) * way > 0.0, "{way}: on past the box, not back over it: {on:?}");
            let at = |along: f64, out: f64| height(&face.surface, top[0] - E + way * along, top[1] - N + out);
            for (along, out) in [(5.0, 5.0), (24.0, 5.0), (24.0, 15.0), (24.0, 25.0), (24.0, 45.0), (27.0, 5.0), (40.0, 8.0)] {
                close(at(along, out), 120.0, 1e-6);
            }
            close(at(27.0, 15.0), 122.0, 1e-6);
            close(at(27.0, 40.0), 122.0, 1e-6);
        }
    }

    /// A box's far end flared out past the box at 45 and 60 degrees to the strip, for a 20 m road, a 10 m berm and a 25 m
    /// box, on a road climbing east and one climbing west (Tim, 2026-11-04). The toe of the wall above runs along the box's
    /// back, 50 m out from the crest, to its back corner 25 m on past the road's top, and from there straight out at the
    /// angle to the berm's line past the box, 10 m out: 40 m down, so 40 / tan(angle) on, 40 m at 45 and 23.094 m at 60.
    /// The flare is flat at RL 120 like the box; 2 m out past its middle the wall above is at RL 122. One sheet, nothing
    /// crossing.
    #[test]
    fn a_box_end_flared_out_past_the_box_runs_from_its_back_corner_at_the_angle() {
        let low = FaceTarget::Rl(120.0);
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))];
        for end in [45.0f64, 60.0] {
            for (ramp, way) in [(ramp_at(500.0), 1.0), (FaceRamp { back: true, ..ramp_at(1000.0) }, -1.0)] {
                let face = long_face(&rows, &[FaceRamp { width: 20.0, turn_flat: 25.0, turn_end: end, ..ramp }]);
                assert!(face.no_roads.is_empty(), "{end} {way}: {:?}", face.no_roads);
                one_clean_sheet(&face);
                let top = *road_of(&face, 0).pit_edge.vertices.iter().find(|p| (p[2] - 120.0).abs() < 1e-6).expect("the road's top");
                close(top[1] - N, 20.0, 1e-6);
                let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
                let on = 40.0 / end.to_radians().tan();
                let corner = [top[0] + way * 25.0, top[1] + 50.0];
                let foot = [top[0] + way * (25.0 + on), top[1] + 10.0];
                let at = |q: [f64; 2]| toe.iter().position(|p| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6);
                let (b, d) = (at(corner).unwrap_or_else(|| panic!("{end} {way}: the back corner: {toe:?}")), at(foot).unwrap_or_else(|| panic!("{end} {way}: the foot of the flare: {toe:?}")));
                let slant = |p: &P3| ((p[0] - corner[0]) * (foot[1] - corner[1]) - (p[1] - corner[1]) * (foot[0] - corner[0])).abs() / plan_gap([corner[0], corner[1], 0.0], [foot[0], foot[1], 0.0]);
                assert!(toe[b.min(d)..=b.max(d)].iter().all(|p| slant(p) < 1e-6), "{end} {way}: straight from the back corner to the line: {:?}", &toe[b.min(d)..=b.max(d)]);
                let height = |x: f64, y: f64| height(&face.surface, x - E, y - N);
                close(height(top[0] + way * (25.0 + on / 3.0), top[1] + 70.0 / 3.0), 120.0, 1e-6);
                close(height(foot[0] + way * 5.0, top[1] + 5.0), 120.0, 1e-6);
                let (sin, cos) = end.to_radians().sin_cos();
                let middle = [(corner[0] + foot[0]) / 2.0 + way * sin * 2.0, (corner[1] + foot[1]) / 2.0 + cos * 2.0];
                close(height(middle[0], middle[1]), 122.0, 1e-6);
            }
        }
    }

    /// A box where the minimum bench drops the berm, all along and past it, as on Tim's seven-row design (2026-11-04): a
    /// 30 m least bench over a 20 m wall. The box is then the two roads, 10 m each, with no berm between, 25 m long and
    /// squared off at its far end: along it the toe of the wall above has the box's back corner 20 m out from the crest,
    /// its wall-side corner 10 m out, and the crest itself, which it runs along on past the box with no berm. Flat at RL
    /// 120 in the box; the wall above rises from its far end and from the crest past it at 45 degrees, so 2 m past the end
    /// and 5 m out it's at RL 122, the nose where the two meet. Nothing crosses. (A berm the minimum bench drops all along
    /// on an RL leaves the sheet split along the crest with no turn at all, as before: the crest and the toe on it have
    /// their vertices in different places.)
    #[test]
    fn a_box_with_no_berm_is_the_two_roads_squared_off() {
        let low = FaceTarget::Rl(120.0);
        let bench = FaceStep::Berm { on: Some(low), width: EdgeRule::Fixed(10.0), register: true, condition: None, min_bench: Some(30.0) };
        let face = long_face(&[climbed_by(low, &[0]), bench, turned_on(FaceTarget::Rl(140.0))], &[FaceRamp { turn_flat: 25.0, ..ramp_at(500.0) }]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0, "nothing crossing");
        let top = *road_of(&face, 0).pit_edge.vertices.iter().find(|p| (p[2] - 120.0).abs() < 1e-6).expect("the road's top");
        close(top[1] - N, 20.0, 1e-6);
        let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
        let end = top[0] + 25.0;
        let far: Vec<f64> = toe.iter().filter(|p| (p[0] - end).abs() < 1e-6).map(|p| p[1] - top[1]).collect();
        for want in [0.0, 10.0, 20.0] {
            assert!(far.iter().any(|got| (got - want).abs() < 1e-6), "the toe along the far end has {want} m out: {far:?}");
        }
        assert!(toe.iter().all(|p| p[0] - end <= 1e-6 || (p[1] - top[1]).abs() < 1e-6), "on past the box it's the crest: {toe:?}");
        let at = |along: f64, out: f64| height(&face.surface, top[0] - E + along, top[1] - N + out);
        for (along, out) in [(5.0, 5.0), (24.0, 5.0), (24.0, 15.0)] {
            close(at(along, out), 120.0, 1e-6);
        }
        close(at(27.0, 5.0), 122.0, 1e-6);
    }

    /// The box past a turn is as long as the ramp says: 10 m, the road's width, or 25 m, its far end that far on past the
    /// top. With none the road turns at its top, the next road starts in line with the top crossing, and the toe past the
    /// turn joins back the ramp's join on, 50 m, as before. With a box the next road starts straight above the top, the
    /// same place whatever its length, and climbs its 200 m to RL 140.
    #[test]
    fn the_box_past_a_turn_is_as_long_as_the_ramp_says() {
        let low = FaceTarget::Rl(120.0);
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))];
        let mut starts = Vec::new();
        for turn_flat in [0.0, 10.0, 25.0] {
            let face = long_face(&rows, &[FaceRamp { turn_flat, ..ramp_at(500.0) }]);
            assert!(face.no_roads.is_empty(), "{turn_flat}: {:?}", face.no_roads);
            one_clean_sheet(&face);
            let road = road_of(&face, 0);
            close(climbs_and_landings(road).0, 400.0, 1e-3);
            let top = road.pit_edge.vertices[1];
            let [onto, corner, after] = second_road_start(&face);
            if turn_flat > 0.0 {
                let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
                let c = toe.iter().position(|p| plan_gap(*p, corner) < 1e-6).expect("the toe runs round the next road's start");
                close(toe[c + 2][0], top[0] + turn_flat, 1e-6);
                close(toe[c + 2][1] - N, 40.0, 1e-6);
            } else {
                close(after[1] - N, 30.0, 1e-6);
                close(on_from_the_turn(&face, 2, onto, after), 50.0, 1e-6);
            }
            if turn_flat > 0.0 {
                starts.push(onto);
            }
        }
        close(plan_gap(starts[0], starts[1]), 0.0, 1e-6);
    }

    /// A switchback on a seam floor: the berm lies on a floor dipping 5 % into the wall, as a seam's does, and the box is
    /// matched to the floor, so no coal is left under it and none ends up in the wrong strip (Tim, 2026-11-04). The road
    /// climbs to the floor at RL 120 on the crest. The box runs 10 m on past its top on the floor: the crest at RL 120, the
    /// road's width in 0.5 m lower, the berm beyond lower again. The next road starts on the floor straight across the
    /// berm from the top, at RL 119, and climbs from there at 10 % to RL 140: 210 m. Every vertex of the line the second
    /// wall stands on lies on the floor but the next road's own wall-side start, which is level with its start across the
    /// road, and the box's back leads up to that from the floor over the last road width.
    #[test]
    fn a_road_turns_back_on_a_pad_on_a_dipping_floor() {
        const DIP: f64 = 0.05;
        let plane = |y: f64| 120.0 - DIP * (y - N - 20.0);
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |_, y| plane(y)));
        let on = FaceTarget::Surface(&floor);
        let face = long_face(&[climbed_by(on, &[0]), berm(on, 10.0), turned_on(FaceTarget::Rl(140.0))], &[ramp_at(500.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        let road = road_of(&face, 0);
        let pit = &road.pit_edge.vertices;
        let top = pit[1];
        close(top[1] - N, 20.0, 1e-6);
        close(top[2], 120.0, 1e-6);
        // The box's flat and the berm beyond lie on the floor.
        for (along, out) in [(2.0, 2.5), (5.0, 5.0), (8.0, 7.5), (5.0, 15.0)] {
            let (x, y) = (top[0] + along, top[1] + out);
            close(height(&face.surface, x - E, y - N), plane(y), 1e-6);
        }
        // The next road starts on the floor straight across the berm from the top and climbs at once.
        let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
        let onto = *pit.iter().rev().find(|p| toe.iter().any(|q| plan_gap(**p, *q) < 1e-6)).expect("it starts on the toe");
        close(onto[2], plane(onto[1]), 1e-6);
        assert!((onto[0] - top[0]).abs() < 3.0, "in line with the top: {onto:?} against {top:?}");
        let at = pit.iter().position(|p| plan_gap(*p, onto) < 1e-6).expect("on its pit-side edge");
        let climb: f64 = pit[at..].windows(2).map(|w| plan_gap(w[0], w[1])).sum();
        close(climb, (140.0 - onto[2]) / 0.1, 1e-3);
        close(pit.last().unwrap()[2], 140.0, 1e-6);
        let c = toe.iter().position(|q| plan_gap(*q, onto) < 1e-6).unwrap() + 1;
        let corner = toe[c];
        close(corner[2], onto[2], 1e-6);
        close(plan_gap(corner, onto), 10.0, 1e-6);
        for p in toe.iter().filter(|p| plan_gap(**p, corner) > 1e-6) {
            close(p[2], plane(p[1]), 1e-6);
        }
        close(plan_gap(toe[c + 1], corner), 10.0, 1e-6);
    }

    /// A flat before a turn on a floor that falls along the strip faster than the road climbs, 12 % to the east: the
    /// crest follows the floor, so the flat does too, and the road says so, with where the flat ends, on the floor, and
    /// how steep it goes along the crest: 12 % on a crest that runs off square by 12 in 100, 11.91 % along it.
    #[test]
    fn a_flat_on_a_floor_steeper_than_the_road_says_so() {
        let plane = |x: f64| 120.0 - 0.12 * (x - E - 700.0);
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |x, _| plane(x)));
        let on = FaceTarget::Surface(&floor);
        let face = long_face(&[climbed_by(on, &[0]), berm(on, 10.0), turned_on(FaceTarget::Rl(160.0))], &[ramp_at(500.0)]);
        let road = road_of(&face, 0);
        assert_eq!(road.steep_flats.len(), 1, "{:?}", road.steep_flats);
        let (at, steepest) = road.steep_flats[0];
        close(steepest, 0.12 / (1.0_f64 + 0.12 * 0.12).sqrt(), 1e-6);
        close(at[2], plane(at[0]), 1e-6);
    }

    /// A road topping out on a seam rising 5 % into the wall: its wall-side edge runs on past its inside edge's end to
    /// reach the seam, so its top crosses at a skew, the wall-side corner ahead, and the crest steps back from that corner
    /// to the pit-side one before it goes on. The box still runs on past the top the way the road came, east: on the seam
    /// 5 m on past the top, and square across from the end of it on the crest 10 m on, not back over the road below.
    #[test]
    fn the_box_past_a_skewed_top_runs_on_the_way_the_road_came() {
        let plane = |y: f64| 120.0 + 0.05 * (y - N - 20.0);
        let seam = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |_, y| plane(y)));
        let on = FaceTarget::Surface(&seam);
        let face = long_face(&[climbed_by(on, &[0]), berm(on, 20.0), turned_on(FaceTarget::Rl(160.0))], &[ramp_at(500.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        let road = road_of(&face, 0);
        let (pit, wall) = (&road.pit_edge.vertices, &road.wall_edge.vertices);
        // The wall-side edge runs on from where it reaches RL 120 along the seam to its top.
        let (top, wall_top) = (pit[1], wall[2]);
        assert!(wall_top[0] > top[0] + 0.5, "the top crosses ahead: {wall_top:?} against {top:?}");
        let (x, y) = (top[0] + 5.0, top[1] + 2.0);
        close(height(&face.surface, x - E, y - N), plane(y), 1e-6);
        let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe.vertices;
        assert!(toe.iter().any(|p| (p[0] - top[0] - 10.0).abs() < 1e-6), "the box's far end 10 m on: {toe:?}");
    }

    /// Two switchbacks, each on a seam floor dipping 5 % into the wall: the first road climbs east to the lower floor's
    /// crest and turns back on a box on that floor, the second climbs west to the upper floor's and turns again on a box on
    /// that one, and the third climbs east to RL 160. Each box is matched to its floor and runs on past the top the way its
    /// road came: both road edges lie on the face everywhere, to 1 µm; the box's flat and the berm beyond lie on the floor,
    /// to 1 µm, so no coal is left under them; each next road starts on the floor straight across the berm from the top.
    /// Past each turn the toe of the wall above lies on the floor round the box until a road width short of the next road's
    /// wall-side start, which is level with its start across the road, and leads straight up to it. The wall-side edge
    /// crosses the pad as the pad does: from the road's wall-side top down the floor to where the next road came onto the
    /// line, then level across that road.
    #[test]
    fn the_face_on_a_pad_on_a_dipping_floor_is_the_road_and_the_berm_beside_it() {
        const DIP: f64 = 0.05;
        let lower = |y: f64| 120.0 - DIP * (y - N - 20.0);
        let upper = |y: f64| 140.0 - DIP * (y - N - 70.0);
        let floors = [lower, upper];
        let grid = |plane: &dyn Fn(f64) -> f64| triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |_, y| plane(y)));
        let (low, high) = (grid(&lower), grid(&upper));
        let (on_low, on_high) = (FaceTarget::Surface(&low), FaceTarget::Surface(&high));
        let face = long_face(&[climbed_by(on_low, &[0]), berm(on_low, 10.0), turned_on(on_high), berm(on_high, 10.0), turned_on(FaceTarget::Rl(160.0))], &[ramp_at(500.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        let road = road_of(&face, 0);
        close(road.pit_edge.vertices.last().unwrap()[2], 160.0, 1e-6);
        assert!(road.steep_flats.is_empty(), "{:?}", road.steep_flats);
        // Both edges on the face all along, every half metre.
        for edge in [&road.pit_edge, &road.wall_edge] {
            for w in edge.vertices.windows(2) {
                let steps = (plan_gap(w[0], w[1]) / 0.5).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = k as f64 / steps as f64;
                    let p = [w[0][0] + t * (w[1][0] - w[0][0]), w[0][1] + t * (w[1][1] - w[0][1]), w[0][2] + t * (w[1][2] - w[0][2])];
                    close(height(&face.surface, p[0] - E, p[1] - N) - p[2], 0.0, 1e-6);
                }
            }
        }
        let (pit, wall) = (&road.pit_edge.vertices, &road.wall_edge.vertices);
        for (turn, floor) in floors.iter().enumerate() {
            let toe = &face.benches.iter().find(|b| b.step == 2 * turn + 2).expect("the wall above the pad").toe.vertices;
            let onto = *pit.iter().rev().find(|p| toe.iter().any(|q| plan_gap(**p, *q) < 1e-6)).expect("the next road starts on the toe");
            close(onto[2], floor(onto[1]), 1e-6);
            // Across the pad from the road's top: its pit-side corner on the crest, then its wall-side top.
            let at = pit.iter().position(|p| plan_gap(*p, onto) < 1e-6).expect("on the pit-side edge");
            let (top, wall_top) = (pit[at - 2], pit[at - 1]);
            assert!((onto[0] - top[0]).abs() < 3.0, "in line with the top: {onto:?} against {top:?}");
            // The box runs on past the top the way the road came, east then west, on the floor, with the berm beyond.
            let way = if turn == 0 { 1.0 } else { -1.0 };
            for out in [5.0, 15.0] {
                let (x, y) = (top[0] + way * 5.0, top[1] + out);
                close(height(&face.surface, x - E, y - N), floor(y), 1e-6);
            }
            // The wall-side edge comes down the floor from the road's wall-side top to where the next road came onto the
            // line, then crosses that road level to its wall-side start.
            let w = wall.iter().position(|p| plan_gap(*p, onto) < 1e-6).expect("the wall-side edge passes where the road came onto the line");
            close(plan_gap(wall[w - 1], wall_top), 0.0, 1e-6);
            let corner = wall[w + 1];
            close(corner[2], onto[2], 1e-6);
            close(plan_gap(corner, onto), 10.0, 1e-6);
            // Round the box the toe leads up to that corner from the floor a road width short of it.
            let c = toe.iter().position(|p| plan_gap(*p, corner) < 1e-6).expect("the toe runs round the road's start");
            let climbs = pit[at + 1];
            let past = [c.checked_sub(1).map(|k| toe[k]), toe.get(c + 1).copied()].into_iter().flatten().max_by(|p, q| plan_gap(*p, climbs).total_cmp(&plan_gap(*q, climbs))).expect("the toe goes on past the corner");
            close(plan_gap(past, corner), 10.0, 1e-6);
            close(past[2], floor(past[1]), 1e-6);
        }
    }

    /// Past a turn with no box the toe of the wall above runs from the next road's wall-side start straight back to the
    /// berm's line the ramp's join on from the turn, along the line, then carries on as the berm's line, whatever vertices the string has:
    /// one only at its ends, one 9 m past the turn, or one every 25 m as on a line from a grid. A road wider than the berm
    /// does the same from further on. Joins of 20, 50 and 90 m each land where they say, the pad inside the join is level,
    /// and the wall rises from the line just past it.
    #[test]
    fn the_toe_past_a_turn_joins_back_a_set_length_on_whatever_vertices_the_string_has() {
        let low = FaceTarget::Rl(120.0);
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))];
        let ends = vec![[0.0, 0.0], [2000.0, 0.0]];
        let near = vec![[0.0, 0.0], [500.0, 0.0], [707.0, 0.0], [2000.0, 0.0]];
        let cells: Vec<[f64; 2]> = (0..=80).map(|k| [k as f64 * 25.0, 0.0]).collect();
        for width in [10.0, 25.0] {
            for turn_join in [20.0, 50.0, 90.0] {
                let mut joins = Vec::new();
                for points in [&ends, &near, &cells] {
                    let ramp = FaceRamp { width, turn_join, turn_flat: 0.0, ..ramp_at(500.0) };
                    let face = project_face(&string(points, FLOOR), &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[ramp]).unwrap();
                    assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
                    one_clean_sheet(&face);
                    let [onto, corner, join] = second_road_start(&face);
                    close(plan_gap(corner, onto), width, 1e-6);
                    close(join[1] - N, 30.0, 1e-6);
                    close(on_from_the_turn(&face, 2, onto, join), turn_join, 1e-6);
                    close(join[2], 120.0, 1e-6);
                    let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe;
                    let at = toe.vertices.iter().position(|p| plan_gap(*p, join) < 1e-6).expect("the toe joins the line");
                    for p in &toe.vertices[at..] {
                        close(p[1] - N, 30.0, 1e-6);
                    }
                    let inside = [(corner[0] + join[0]) / 2.0 - E, (corner[1] + join[1]) / 2.0 - N - 1.0];
                    close(height(&face.surface, inside[0], inside[1]), 120.0, 1e-6);
                    close(height(&face.surface, join[0] - E + 5.0, 35.0), 125.0, 1e-6);
                    joins.push(join);
                }
                for join in &joins[1..] {
                    assert!(plan_gap(*join, joins[0]) < 1e-6, "{width} m road, {turn_join} m join: {join:?} against {:?}", joins[0]);
                }
            }
        }
    }

    /// A join longer than the berm's line goes on past the turn runs from the corner straight to the end of the string,
    /// as Tim's own lines do past the back strip's east turns: past the corner the toe is that straight line, and the
    /// pad under the join is level.
    #[test]
    fn a_join_longer_than_the_line_runs_to_the_end_of_the_string() {
        let low = FaceTarget::Rl(120.0);
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))];
        let face = long_face(&rows, &[FaceRamp { turn_join: 5000.0, turn_flat: 0.0, ..ramp_at(500.0) }]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        let [_, corner, _] = second_road_start(&face);
        let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe;
        let at = toe.vertices.iter().position(|p| plan_gap(*p, corner) < 1e-6).expect("the toe runs round the corner");
        let end = *toe.vertices.last().unwrap();
        close(end[0] - E, 2000.0, 1e-6);
        close(end[1] - N, 30.0, 1e-6);
        for p in &toe.vertices[at + 1..] {
            close(off_line(&Polyline { vertices: vec![corner, end], closed: false }, *p), 0.0, 1e-6);
        }
        close(height(&face.surface, 780.0, 35.0), 120.0, 1e-6);
    }

    /// A toe that jogs 15 m into the wall just past the turn, 16 m on from the flat's end: the toe past the turn joins the
    /// jogged line 50 m on, as it would a straight one. One that bumps 25 m into the wall for 50 m just past the turn,
    /// with a 150 m join: a join straight to 150 m on would cross the berm's line round the bump and cut the berm, so it
    /// stops where it first meets that line, and the toe follows the line from there. Either way the face is one sheet
    /// with nothing crossing.
    #[test]
    fn the_toe_past_a_turn_joins_a_jogging_line_without_cutting_the_berm() {
        let low = FaceTarget::Rl(120.0);
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(FaceTarget::Rl(140.0))];
        let jog = vec![[0.0, 0.0], [725.0, 0.0], [735.0, 15.0], [2000.0, 15.0]];
        let bump = vec![[0.0, 0.0], [730.0, 0.0], [740.0, 25.0], [790.0, 25.0], [800.0, 0.0], [2000.0, 0.0]];
        for (points, jogged) in [(&jog, true), (&bump, false)] {
            let ramp = FaceRamp { turn_join: if jogged { 50.0 } else { 150.0 }, turn_flat: 0.0, ..ramp_at(500.0) };
            let face = project_face(&string(points, FLOOR), &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[ramp]).unwrap();
            assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
            one_clean_sheet(&face);
            let [onto, corner, join] = second_road_start(&face);
            let toe = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe;
            let at = toe.vertices.iter().position(|p| plan_gap(*p, corner) < 1e-6).expect("the toe runs round the corner");
            if jogged {
                close(join[1] - N, 45.0, 1e-6);
                close(on_from_the_turn(&face, 2, onto, join), 50.0, 1e-6);
            } else {
                assert!(on_from_the_turn(&face, 2, onto, join) < 150.0 - 1.0, "it stops at the bump: {join:?}");
                let line = &face.benches.iter().find(|b| b.step == 2).expect("the second wall").toe;
                assert!(line.vertices[at + 1..].iter().all(|p| p[1] - N <= 55.0 + 1e-6), "and follows the line from there: {:?}", line.vertices);
            }
        }
    }

    /// Walls carried straight on through an RL with no berm, as walls are through a seam's roof, with a road up them, then
    /// a turn and two more. The line where each wall crosses the RL on its way is drawn from the wall as built, roads and
    /// all: it lies at that RL everywhere, goes round the road's entrance and the flat stretch after the turn, across the
    /// road where the road passes that RL, and stands on the same toe as the wall above it, which it never crosses.
    #[test]
    fn where_a_wall_crosses_an_rl_on_its_way_the_line_goes_round_its_roads() {
        let rows = [
            climbed_by(FaceTarget::Rl(110.0), &[0]),
            climbed_by(FaceTarget::Rl(120.0), &[0]),
            berm(FaceTarget::Rl(120.0), 10.0),
            turned_on(FaceTarget::Rl(130.0)),
            climbed_by(FaceTarget::Rl(140.0), &[0]),
        ];
        let face = long_face(&rows, &[ramp_at(500.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        one_clean_sheet(&face);
        let crosses = |a: &Polyline, b: &Polyline| {
            a.vertices.windows(2).any(|p| {
                b.vertices.windows(2).any(|q| segment_crossing([p[0][0], p[0][1]], [p[1][0], p[1][1]], [q[0][0], q[0][1]], [q[1][0], q[1][1]]).is_some_and(|(t, u)| t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9))
            })
        };
        let bench = |step: usize| face.benches.iter().find(|b| b.step == step).unwrap_or_else(|| panic!("no bench {step}"));
        for (mark, last, rl) in [(0, 1, 110.0), (3, 4, 130.0)] {
            let (marked, above) = (bench(mark), bench(last));
            assert_eq!(marked.toe.vertices.len(), above.toe.vertices.len(), "step {mark} stands on the toe of the wall it's part of");
            for (p, q) in marked.toe.vertices.iter().zip(&above.toe.vertices) {
                assert!((0..3).all(|i| (p[i] - q[i]).abs() < 1e-6), "step {mark} stands on the toe of the wall it's part of: {p:?} against {q:?}");
            }
            for p in &marked.crest.vertices {
                close(p[2], rl, 1e-6);
            }
            assert!(!crosses(&marked.crest, &above.toe), "step {mark}'s line crosses the toe: {:?}", marked.crest.vertices);
            assert!(!crosses(&marked.crest, &above.crest), "step {mark}'s line crosses the crest above");
        }
        // Past the road it's the plain wall: 10 m up a 45 degree wall from a level toe is 10 m out.
        let past = |line: &Polyline, x: f64| line.vertices.windows(2).find(|w| w[0][0] - E <= x && w[1][0] - E >= x).map(|w| w[0][1] - N).expect("the line runs there");
        close(past(&bench(0).crest, 1500.0), 10.0, 1e-6);
    }

    /// A pit wall curving round, as a metals pit's does, with a road turning back twice on its way up three walls. Each
    /// leg holds 10 % round the curve, each pad is level, and the face is one sheet with nothing crossing.
    #[test]
    fn a_road_turns_back_on_a_curving_wall() {
        // Clockwise round a centre 600 m to the south, so the walls rise outwards, away from the pit.
        let toe = Polyline { vertices: (0..=40).map(|k| (k as f64 * 2.5 - 50.0).to_radians()).map(|a| [E + 700.0 + 600.0 * a.sin(), N - 600.0 + 600.0 * a.cos(), FLOOR]).collect(), closed: false };
        let start = toe.vertices[8];
        let (low, mid) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0));
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), turned_on(mid), berm(mid, 10.0), turned_on(FaceTarget::Rl(160.0))];
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &rows, &[FaceRamp { start: [start[0], start[1]], ..RAMP }]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = road_of(&face, 0);
        assert_eq!(road.steps, [0, 4]);
        let (climbed, _) = climbs_and_landings(road);
        close(climbed, 600.0, 1e-2);
        // Each pad is crossed, then a road width flat round the curving line, in one level run.
        let level: Vec<bool> = road.pit_edge.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6).map(|w| (w[1][2] - w[0][2]).abs() <= 1e-9).collect();
        assert_eq!(level.windows(2).filter(|w| !w[0] && w[1]).count(), 2, "two pads: {:?}", road.pit_edge.vertices);
        close(road.pit_edge.vertices.last().unwrap()[2], 160.0, 1e-6);
        one_clean_sheet(&face);
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

    /// A wall at 45 degrees climbed by these ramps.
    fn climbed_by<'a>(to: FaceTarget<'a>, ramps: &'a [usize]) -> FaceStep<'a> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(45.0), condition: None, ramps, turns: &[] }
    }

    fn ramp_at(x: f64) -> FaceRamp {
        FaceRamp { start: [E + x, N], ..RAMP }
    }

    /// A face from a level toe 2 km long along +x, rising to the north.
    fn long_face(steps: &[FaceStep<'_>], ramps: &[FaceRamp]) -> Face {
        let toe = string(&[[0.0, 0.0], [2000.0, 0.0]], FLOOR);
        project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, steps, ramps).unwrap()
    }

    fn road_of(face: &Face, ramp: usize) -> &FaceRoad {
        face.roads.iter().find(|road| road.ramp == ramp).unwrap_or_else(|| panic!("no road {ramp}: {:?}", face.no_roads))
    }

    fn holds_its_grade_to(road: &FaceRoad, rl: f64, length: f64) {
        for grade in rise_over_run(&road.pit_edge) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        assert!((road.pit_edge.vertices.last().unwrap()[2] - rl).abs() < 1e-6, "{:?}", road.pit_edge.vertices.last());
        assert!((road.pit_edge.plan_length() - length).abs() < 1e-3, "{}", road.pit_edge.plan_length());
    }

    /// Two roads 300 m apart on one wall, 40 m high at 45 degrees, overlapping along the wall. The one further along
    /// is the lower wherever they're both there. A section through both reads wall, road, wall, road, wall, each
    /// wall on a plane parallel to the plain one and a road width further back than the one below it, to 1 mm.
    #[test]
    fn two_roads_on_one_wall_sit_one_above_the_other() {
        let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1])], &[ramp_at(200.0), ramp_at(500.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads.len(), 2);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        one_clean_sheet(&face);
        // A road climbs at sin(angle) = 0.1 off the strike and moves the wall above back 10 m square to itself.
        let back = 10.0 * COS;
        let on_road = |x0: f64, y0: f64, x: f64, y: f64| FLOOR + 0.1 * ((x - x0) * COS + (y - y0) * 0.1);
        let section = [(2.0, FLOOR + 2.0), (10.0, on_road(500.0, 0.0, 550.0, 10.0)), (25.0, FLOOR + 25.0 - back), (50.0, on_road(200.0, back, 550.0, 50.0)), (58.0, FLOOR + 58.0 - 2.0 * back)];
        for (y, rl) in section {
            assert!((height(&face.surface, 550.0, y) - rl).abs() < 1e-3, "at {y} m out: {} against {rl}", height(&face.surface, 550.0, y));
        }
        // The crest is two road widths back from the plain wall's, and past the top of the upper road it's one.
        assert!(height_at(&face.surface, 550.0, 40.0 + 2.0 * back + 0.5).is_none(), "the sheet stops at the crest");
        assert!(height_at(&face.surface, 550.0, 40.0 + 2.0 * back - 0.5).is_some());
        assert!(height_at(&face.surface, 700.0, 40.0 + back + 0.5).is_none());
        assert!((height(&face.surface, 700.0, 40.0 + back - 0.5) - 139.5).abs() < 1e-3);
    }

    /// Roads far enough apart that they don't overlap, each with the wall it needs. The upper road sits on the toe
    /// moved back by the entrance of the one ahead of it, as a planner would have it.
    #[test]
    fn two_roads_apart_along_one_wall_each_climb_it() {
        let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1])], &[ramp_at(200.0), ramp_at(1200.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        one_clean_sheet(&face);
    }

    /// The whole of it on one strip: three roads up three walls to set RLs, across two berms. Each holds its grade
    /// from the floor to the top, 60 m at 10 %, and the face is one sheet with nothing crossing.
    #[test]
    fn three_roads_climb_three_walls_and_their_berms_together() {
        let (low, high, all) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0), &[0, 1, 2]);
        let rows = [climbed_by(low, all), berm(low, 10.0), climbed_by(high, all), berm(high, 10.0), climbed_by(FaceTarget::Rl(160.0), all)];
        let face = long_face(&rows, &[ramp_at(100.0), ramp_at(500.0), ramp_at(900.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        for ramp in 0..3 {
            assert_eq!(road_of(&face, ramp).steps, [0, 4]);
            holds_its_grade_to(road_of(&face, ramp), 160.0, 600.0);
        }
        one_clean_sheet(&face);
    }

    /// Roads close together along a wall leave a berm no room to open out between them, and it still builds.
    #[test]
    fn a_berm_between_two_roads_close_together_still_builds() {
        let (low, all) = (FaceTarget::Rl(120.0), &[0, 1]);
        let rows = [climbed_by(low, all), berm(low, 10.0), climbed_by(FaceTarget::Rl(140.0), all)];
        let face = long_face(&rows, &[ramp_at(300.0), ramp_at(400.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        one_clean_sheet(&face);
    }

    /// How much of a road climbs, in plan, and how many level stretches it has: its landings.
    fn climbs_and_landings(road: &FaceRoad) -> (f64, usize) {
        let gaps = road.pit_edge.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6);
        let (climbing, flat): (Vec<_>, Vec<_>) = gaps.partition(|w| (w[1][2] - w[0][2]).abs() > 1e-9);
        for w in &climbing {
            close((w[1][2] - w[0][2]) / plan_gap(w[0], w[1]), 0.1, 1e-9);
        }
        (climbing.iter().map(|w| plan_gap(w[0], w[1])).sum(), flat.len())
    }

    /// Going after the berm works however roads come: three up three walls together, two close together, two stacked
    /// on one wall running back with the wall on the right. Each climbs its 20 m a wall at 10 % and crosses each berm
    /// on one level landing, and each face is one sheet with nothing crossing.
    #[test]
    fn roads_after_the_berm_go_together_close_together_and_back() {
        let after = |x: f64| FaceRamp { after_berm: true, ..ramp_at(x) };
        let (low, high, all) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0), &[0, 1, 2]);
        let rows = [climbed_by(low, all), berm(low, 10.0), climbed_by(high, all), berm(high, 10.0), climbed_by(FaceTarget::Rl(160.0), all)];
        let three = long_face(&rows, &[after(100.0), after(500.0), after(900.0)]);
        let two_rows = [climbed_by(low, &[0, 1]), berm(low, 10.0), climbed_by(high, &[0, 1])];
        let close_together = long_face(&two_rows, &[after(300.0), after(400.0)]);
        let toe = string(&[[2000.0, 0.0], [0.0, 0.0]], FLOOR);
        let back = |x: f64| FaceRamp { back: true, ..after(x) };
        let stacked = project_face(&toe, &level(FLOOR), OffsetSide::Right, FaceDirection::Up, &two_rows, &[back(200.0), back(500.0)]).unwrap();
        for (face, roads, walls) in [(&three, 3, 3), (&close_together, 2, 2), (&stacked, 2, 2)] {
            assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
            for ramp in 0..roads {
                let road = road_of(face, ramp);
                let (climbed, landings) = climbs_and_landings(road);
                close(climbed, 200.0 * walls as f64, 1e-3);
                assert_eq!(landings, walls - 1, "{:?}", road.pit_edge.vertices);
                close(road.pit_edge.vertices.last().unwrap()[2], FLOOR + 20.0 * walls as f64, 1e-6);
            }
            one_clean_sheet(face);
        }
    }

    /// Running back against the string is the same thing the other way round.
    #[test]
    fn roads_stack_going_back_with_the_wall_on_the_right() {
        let toe = string(&[[2000.0, 0.0], [0.0, 0.0]], FLOOR);
        let (low, all) = (FaceTarget::Rl(120.0), &[0, 1]);
        let rows = [climbed_by(low, all), berm(low, 10.0), climbed_by(FaceTarget::Rl(140.0), all)];
        let back = |x: f64| FaceRamp { back: true, ..ramp_at(x) };
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Right, FaceDirection::Up, &rows, &[back(200.0), back(500.0)]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        one_clean_sheet(&face);
    }

    /// A berm touching its crest either side of a corner still fills the corner, with no hole left between them.
    #[test]
    fn a_berm_fills_a_corner_it_cuts_across() {
        let crest = string(&[[0.0, 0.0], [100.0, 0.0], [200.0, 40.0], [300.0, 40.0]], 120.0);
        let line = string(&[[0.0, 0.0], [50.0, 0.0], [150.0, 20.0], [300.0, 40.0]], 120.0);
        let berm = crate::face::band(&crest, &line).unwrap();
        let area: f64 = berm.triangles.iter().map(|t| {
            let [a, b, c] = t.map(|v| berm.vertices[v as usize]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
        }).sum();
        // It touches the crest at 50 m and 150 m and at the end, so what's left are the two corners it cuts across:
        // triangles of 500 and 1,000 square metres.
        assert!((area - 1500.0).abs() < 1e-6, "{area}");
    }

    /// Roads can share some walls and not others: one finishes part way up, and one starts part way up behind it.
    #[test]
    fn roads_can_share_some_walls_and_not_others() {
        let (low, high) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0));
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), climbed_by(high, &[0, 1]), berm(high, 10.0), climbed_by(FaceTarget::Rl(160.0), &[1])];
        let face = long_face(&rows, &[ramp_at(600.0), ramp_at(200.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(road_of(&face, 0).steps, [0, 2]);
        assert_eq!(road_of(&face, 1).steps, [2, 4]);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 160.0, 400.0);
        one_clean_sheet(&face);
    }

    /// A road starting on a wall ahead of one coming up through it moves the toe back only as far as that road, so
    /// the road coming up carries on from where it is and both reach the top.
    #[test]
    fn an_entrance_ahead_of_a_road_coming_up_stops_short_of_it() {
        let (low, high) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0));
        let rows = [climbed_by(low, &[0]), berm(low, 10.0), climbed_by(high, &[0, 1])];
        let face = long_face(&rows, &[ramp_at(200.0), ramp_at(800.0)]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 200.0);
        one_clean_sheet(&face);
    }

    /// Two roads starting at one place can't both be there. The second says so and the first builds.
    #[test]
    fn two_roads_from_one_place_says_so() {
        let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1])], &[ramp_at(500.0), ramp_at(500.0)]);
        assert_eq!(face.roads.len(), 1, "{:?}", face.no_roads);
        assert!(face.no_roads[0].why.contains("it starts on the road below it"), "{:?}", face.no_roads);
        one_clean_sheet(&face);
    }

    /// Roads on one wall run the same way for now. The other one says so and the face builds with the first.
    /// The height of a sheet anywhere along the 2 km toe of `long_face`.
    fn height_on_long(sheet: &TriMesh, x: f64, y: f64) -> Option<f64> {
        TriangleBins::new(sheet, [E - 100.0, N - 100.0], [E + 2100.0, N + 300.0]).height(sheet, [E + x, N + y])
    }

    /// Where the foot of the first wall is, out from the toe, at a place along it.
    fn foot_out_at(face: &Face, x: f64) -> f64 {
        let foot = &face.benches.iter().find(|b| b.step == 0).expect("the first wall").toe;
        let crossing = foot.vertices.windows(2).find(|w| (w[0][0] - E - x) * (w[1][0] - E - x) <= 0.0 && w[0][0] != w[1][0]).unwrap_or_else(|| panic!("the foot doesn't reach {x}"));
        let t = (E + x - crossing[0][0]) / (crossing[1][0] - crossing[0][0]);
        crossing[0][1] + t * (crossing[1][1] - crossing[0][1]) - N
    }

    /// A road on its own on a face, to set one beside others against.
    fn alone(steps: &[FaceStep<'_>], ramp: FaceRamp) -> FaceRoad {
        let face = long_face(steps, &[ramp]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        face.roads[0].clone()
    }

    /// How far a point is from a line in three dimensions.
    fn off_line_3d(line: &Polyline, p: P3) -> f64 {
        line.vertices
            .windows(2)
            .map(|w| {
                let d = [w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]];
                let length = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                let t = if length > 0.0 { (((p[0] - w[0][0]) * d[0] + (p[1] - w[0][1]) * d[1] + (p[2] - w[0][2]) * d[2]) / length).clamp(0.0, 1.0) } else { 0.0 };
                ((p[0] - w[0][0] - t * d[0]).powi(2) + (p[1] - w[0][1] - t * d[1]).powi(2) + (p[2] - w[0][2] - t * d[2]).powi(2)).sqrt()
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Two roads' edges are one line, wherever the walls they were cut from split them: the same ends, and every vertex
    /// of each on the other, to 1 µm.
    fn same_edges(road: &FaceRoad, on_its_own: &FaceRoad) {
        for (edge, own) in [(&road.pit_edge, &on_its_own.pit_edge), (&road.wall_edge, &on_its_own.wall_edge)] {
            for (p, q) in [(edge.vertices[0], own.vertices[0]), (*edge.vertices.last().unwrap(), *own.vertices.last().unwrap())] {
                assert!(plan_gap(p, q) < 1e-6 && (p[2] - q[2]).abs() < 1e-6, "{p:?} against {q:?}");
            }
            for (one, other) in [(edge, own), (own, edge)] {
                for p in &one.vertices {
                    assert!(off_line_3d(other, *p) < 1e-6, "{p:?} is {} off {:?}", off_line_3d(other, *p), other.vertices);
                }
            }
        }
    }

    /// Two roads climbing towards each other on one wall 40 m high at 45 degrees (Tim, 2026-11-05: a straight ramp and a
    /// switchback on one face): one east from 200 m along, one west from 1200 m. Each is 400 m to RL 140 at 10 % and just
    /// as it is on its own, a section through each 200 m from its start is the other's mirrored, and between their tops
    /// the wall is the plain one. Behind each start the toe moves back by the road, 10 m square to it.
    #[test]
    fn roads_climbing_towards_each_other_on_one_wall_both_climb_it() {
        let row = [climbed_by(FaceTarget::Rl(140.0), &[0, 1])];
        let (east, west) = (ramp_at(200.0), FaceRamp { back: true, ..ramp_at(1200.0) });
        let face = long_face(&row, &[east, west]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        let one = [climbed_by(FaceTarget::Rl(140.0), &[0])];
        same_edges(road_of(&face, 0), &alone(&one, east));
        same_edges(road_of(&face, 1), &alone(&one, west));
        let top = road_of(&face, 1).pit_edge.vertices.last().copied().unwrap();
        close(top[0] - E, 1200.0 - 400.0 * COS, 1e-6);
        one_clean_sheet(&face);
        for k in 1..60 {
            let y = k as f64;
            let (one, other) = (height_on_long(&face.surface, 400.0, y), height_on_long(&face.surface, 1000.0, y));
            assert!(one.zip(other).is_none_or(|(a, b)| (a - b).abs() < 1e-6) && one.is_some() == other.is_some(), "{y} m out: {one:?} against {other:?}");
            let plain = height_on_long(&face.surface, 700.0, y);
            if y <= 40.0 {
                close(plain.unwrap_or_else(|| panic!("nothing at 700, {y}")), FLOOR + y, 1e-6);
            } else {
                assert!(plain.is_none(), "past the crest at 700, {y}: {plain:?}");
            }
        }
        close(foot_out_at(&face, 100.0), 10.0 * COS, 1e-6);
        close(foot_out_at(&face, 700.0), 0.0, 1e-9);
        close(foot_out_at(&face, 1300.0), 10.0 * COS, 1e-6);
    }

    /// Two roads climbing away from each other: one west from 700 m along, one east from 1100 m. Their entrances face
    /// each other and share the floor between them, the toe 10 m square to the roads back from the line all the way, and
    /// a section between them is one wall from there to its crest. Each road is 400 m to RL 140, and past their tops the
    /// toe is the line.
    #[test]
    fn roads_climbing_away_from_each_other_share_the_floor_between_their_entrances() {
        let row = [climbed_by(FaceTarget::Rl(140.0), &[0, 1])];
        let (west, east) = (FaceRamp { back: true, ..ramp_at(700.0) }, ramp_at(1100.0));
        let face = long_face(&row, &[west, east]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        one_clean_sheet(&face);
        let back = 10.0 * COS;
        for x in [750.0, 800.0, 900.0, 1000.0, 1050.0] {
            close(foot_out_at(&face, x), back, 1e-6);
        }
        assert!(height_on_long(&face.surface, 900.0, back - 1.0).is_none(), "the floor in front of the toe isn't the face's");
        for (y, rl) in [(back + 1.0, FLOOR + 1.0), (back + 39.0, FLOOR + 39.0)] {
            close(height_on_long(&face.surface, 900.0, y).unwrap_or_else(|| panic!("nothing at 900, {y}")), rl, 1e-6);
        }
        assert!(height_on_long(&face.surface, 900.0, back + 40.5).is_none(), "the crest is a road width back");
        close(foot_out_at(&face, 200.0), 0.0, 1e-9);
        close(foot_out_at(&face, 1700.0), 0.0, 1e-9);
    }

    /// The same with roads of different widths, 10 m west and 20 m east: their moved toes can't meet. The wall splits
    /// halfway between the stretches they take, from the west road's wall-side start, a metre on from its start as it climbs
    /// at sin(angle) = 0.1, at 701 m, to the east road's, 2 m back at 1098 m: at 899.5 m. The toe stays on the line for 0.5 m
    /// either side, so the walls either side rise square to it, and each entrance comes across from there at 45 degrees
    /// and is back by its own road beyond that.
    #[test]
    fn entrances_of_different_widths_facing_each_other_come_across_at_the_split() {
        let row = [climbed_by(FaceTarget::Rl(140.0), &[0, 1])];
        let face = long_face(&row, &[FaceRamp { back: true, ..ramp_at(700.0) }, FaceRamp { width: 20.0, ..ramp_at(1100.0) }]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        holds_its_grade_to(road_of(&face, 1), 140.0, 400.0);
        one_clean_sheet(&face);
        for x in [899.1, 899.5, 899.9] {
            close(foot_out_at(&face, x), 0.0, 1e-6);
        }
        close(foot_out_at(&face, 895.0), 4.0, 1e-6);
        close(foot_out_at(&face, 905.0), 5.0, 1e-6);
        close(foot_out_at(&face, 850.0), 10.0 * COS, 1e-6);
        close(foot_out_at(&face, 950.0), 20.0 * COS, 1e-6);
    }

    /// Roads running opposite ways over one stretch of wall would cross: the one later in the list stops, and says which
    /// it would cross. The other builds as on its own. Tops facing closer than a road's width stop it too. The stretch each
    /// takes runs to its wall-side top, a metre back from its pit-side top as it climbs at sin(angle) = 0.1.
    #[test]
    fn roads_running_opposite_ways_over_one_stretch_stop_the_later() {
        let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1])], &[ramp_at(200.0), FaceRamp { back: true, ..ramp_at(700.0) }]);
        assert_eq!(face.no_roads.len(), 1, "{:?}", face.no_roads);
        assert_eq!((face.no_roads[0].ramp, face.no_roads[0].crosses), (1, Some(0)));
        assert_eq!(face.no_roads[0].why, CROSSES);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
        one_clean_sheet(&face);
        // Tops facing 7 m apart, closer than a road's 10 m: the same. 17 m apart, both build.
        for (gap, both) in [(5.0, false), (15.0, true)] {
            let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1])], &[ramp_at(200.0), FaceRamp { back: true, ..ramp_at(200.0 + 2.0 * 400.0 * COS + 2.0 + gap) }]);
            assert_eq!(face.no_roads.is_empty(), both, "{gap}: {:?}", face.no_roads);
            one_clean_sheet(&face);
        }
    }

    /// Roads both ways up two walls and through the berm between: one east from 200 m, one west from 1500 m, each 200 m up
    /// a wall 20 m high, the berm's line coming in to each road's top the way that road runs, and 200 m more. Each is just
    /// as it is on its own.
    #[test]
    fn roads_both_ways_through_one_berm_are_as_they_are_alone() {
        let (low, high) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0));
        let rows = [climbed_by(low, &[0, 1]), berm(low, 10.0), climbed_by(high, &[0, 1])];
        let alone_rows = [climbed_by(low, &[0]), berm(low, 10.0), climbed_by(high, &[0])];
        let (east, west) = (ramp_at(200.0), FaceRamp { back: true, ..ramp_at(1500.0) });
        let face = long_face(&rows, &[east, west]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        for (r, ramp) in [(0, east), (1, west)] {
            assert_eq!(road_of(&face, r).steps, [0, 2]);
            holds_its_grade_to(road_of(&face, r), 140.0, 400.0);
            same_edges(road_of(&face, r), &alone(&alone_rows, ramp));
        }
        one_clean_sheet(&face);
    }

    /// A straight ramp and a switchback on one face (Tim, 2026-11-05): ramp 0 climbs east from 100 m up two walls and
    /// after the berm between, and ramp 1 climbs east from 1200 m and turns back on a box on that berm to climb the second
    /// wall west. On the first wall both run east and the straight ramp sits on the toe moved back by the switchback's
    /// entrance, as roads one way do. On the second they run opposite ways, side by side, and the switchback is just as it
    /// is on its own: it still turns, where it used to be cut running on east. Each climbs 400 m to RL 140.
    #[test]
    fn a_straight_ramp_and_a_switchback_on_one_face_are_as_they_are_alone() {
        let (low, high) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0));
        let upper = |ramps: &'static [usize], turns: &'static [usize]| FaceStep::Project { to: high, batter: EdgeRule::Fixed(45.0), condition: None, ramps, turns };
        let rows = [climbed_by(low, &[0, 1]), berm(low, 10.0), upper(&[0, 1], &[1])];
        let (straight, switchback) = (FaceRamp { after_berm: true, ..ramp_at(100.0) }, ramp_at(1200.0));
        let face = long_face(&rows, &[straight, switchback]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(road_of(&face, 0).steps, [0, 2]);
        close(climbs_and_landings(road_of(&face, 0)).0, 400.0, 1e-3);
        close(road_of(&face, 0).pit_edge.vertices.last().unwrap()[2], 140.0, 1e-6);
        same_edges(road_of(&face, 1), &alone(&[climbed_by(low, &[0]), berm(low, 10.0), upper(&[0], &[0])], switchback));
        let turned = road_of(&face, 1);
        let (bottom, top) = (turned.pit_edge.vertices[0], *turned.pit_edge.vertices.last().unwrap());
        let furthest = turned.pit_edge.vertices.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        assert!(top[0] < furthest - 150.0, "it turns back: {:?}", turned.pit_edge.vertices);
        close(bottom[0] - E, 1200.0, 1e-6);
        close(top[2], 140.0, 1e-6);
        close(climbs_and_landings(turned).0, 400.0, 1e-3);
        one_clean_sheet(&face);
    }

    /// The other way about: the switchback climbs west from 1200 m and turns back on its box to climb the second wall east.
    /// Now the two run opposite ways on the first wall and one way on the second, where the switchback's toe past its turn
    /// joins back long before the straight ramp. Each is just as it is on its own.
    #[test]
    fn a_switchback_running_back_first_beside_a_straight_ramp_is_as_it_is_alone() {
        let (low, high) = (FaceTarget::Rl(120.0), FaceTarget::Rl(140.0));
        let upper = |ramps: &'static [usize], turns: &'static [usize]| FaceStep::Project { to: high, batter: EdgeRule::Fixed(45.0), condition: None, ramps, turns };
        let rows = [climbed_by(low, &[0, 1]), berm(low, 10.0), upper(&[0, 1], &[1])];
        let (straight, switchback) = (FaceRamp { after_berm: true, ..ramp_at(100.0) }, FaceRamp { back: true, ..ramp_at(1200.0) });
        let face = long_face(&rows, &[straight, switchback]);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        same_edges(road_of(&face, 0), &alone(&[climbed_by(low, &[0]), berm(low, 10.0), upper(&[0], &[])], straight));
        same_edges(road_of(&face, 1), &alone(&[climbed_by(low, &[0]), berm(low, 10.0), upper(&[0], &[0])], switchback));
        let turned = road_of(&face, 1);
        let top = *turned.pit_edge.vertices.last().unwrap();
        let furthest = turned.pit_edge.vertices.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        assert!(top[0] > furthest + 150.0, "it turns back: {:?}", turned.pit_edge.vertices);
        close(top[2], 140.0, 1e-6);
        one_clean_sheet(&face);
    }

    /// A berm opens out from the road to 30 m over 300 m. Its toe turns 30 degrees away from the berm 100 m
    /// past the top of the road, as a strip's toe does, and a straight taper would cut across the crest there.
    #[test]
    fn a_berm_opening_ahead_of_a_road_turns_the_corner_with_the_crest() {
        let on = FaceTarget::Rl(120.0);
        let (turn, run) = (30_f64.to_radians(), 700.0);
        let toe = string(&[[0.0, 0.0], [400.0, 0.0], [400.0 + run * turn.cos(), -run * turn.sin()]], FLOOR);
        let steps = vec![wall(on, 45.0), berm(on, 30.0), wall(FaceTarget::Rl(140.0), 45.0)];
        let face = project_face(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &steps, &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads[0].steps, [0, 2]);
        one_clean_sheet(&face);
        let (crest, next_toe) = (&face.benches[0].crest, &face.benches[1].toe);
        let top = face.roads[0].pit_edge.vertices.iter().find(|p| (p[2] - 120.0).abs() < 1e-6).copied().unwrap();
        let gap_at = |metres: f64| {
            let ahead = stretch_of(crest, nearest_place(crest, top), (crest.vertices.len() - 1) as f64);
            let p = point_along(&ahead, metres);
            (0..next_toe.vertices.len() - 1).map(|i| nearest_along(next_toe.vertices[i], next_toe.vertices[i + 1], p).1).fold(f64::INFINITY, f64::min)
        };
        // The taper is ten times the 30 m berm and the road's width. Past the corner the gap grows with the
        // metres along the crest, measured square to the line it's opening along, to a micrometre.
        let taper = 10.0 * (30.0 + 10.0 * COS);
        let square = 1.0 / (1.0 + (30.0 / taper) * (30.0 / taper)).sqrt();
        for metres in [150.0, 250.0, 350.0] {
            let gap = 30.0 * metres / taper * square;
            assert!((gap_at(metres) - gap).abs() < 1e-6, "{metres} m along: {} against {gap}", gap_at(metres));
        }
        assert!((gap_at(420.0) - 30.0).abs() < 1e-6, "a full berm past the taper: {}", gap_at(420.0));
    }

    /// A road on a wall with no height where it gets there has come out of the pit. That's a finished road, not
    /// one that stopped.
    #[test]
    fn a_road_that_comes_out_where_the_next_wall_has_no_height_is_finished() {
        let ground = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| 110.0 + 30.0 * ((260.0 - (x - E)) / 60.0).clamp(0.0, 1.0)));
        let on = FaceTarget::Rl(120.0);
        let face = ramped(RAMP, vec![wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Surface(&ground), 45.0)], &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads[0].steps, [0, 0]);
        assert!((face.roads[0].pit_edge.vertices.last().unwrap()[2] - 120.0).abs() < 1e-6);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0, "nothing crossing");
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
        assert!(face.no_roads[0].why.contains("reaches the end of the wall"), "{}", face.no_roads[0].why);
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
        let above = FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(45.0), condition: None, ramps: &[1], turns: &[] };
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
            FaceStep::Project { to: FaceTarget::Surface(&surface), batter: EdgeRule::Fixed(60.0), condition: Some(0.1), ramps: &[0], turns: &[] },
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

    /// A held end too close to the end of the string for the road's length starts at the end of the string, and
    /// says how far off it came out. 200 m of road from the start of the string finishes 199 m along, not 100.
    #[test]
    fn a_held_end_the_string_cant_reach_says_how_far_short() {
        let face = wall_to(120.0, 45.0, FaceRamp { ends_at: Some([E + 100.0, N + 20.0]), ..RAMP });
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert!((face.roads[0].pit_edge.vertices[0][0] - E).abs() < 1e-6, "{:?}", face.roads[0].pit_edge.vertices[0]);
        let miss = face.roads[0].off_end.unwrap();
        assert!((miss - (200.0 * COS - 100.0)).abs() < 1e-3, "{miss}");
    }

    /// A road that can't go in doesn't stop the others finding their held ends. The first here is well inside the
    /// strip, and the other two want the end of the string, where only one fits. At 45 degrees a first guess is 2 m out.
    #[test]
    fn a_road_that_cant_go_in_leaves_the_others_finding_their_ends() {
        let held = |x: f64| FaceRamp { back: true, ends_at: Some([E + x, N + 40.0]), ..RAMP };
        let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1, 2])], &[held(1200.0), held(1900.0), held(1950.0)]);
        assert!(face.no_roads.iter().any(|n| n.ramp == 2 && n.why.contains("it starts on the road below it")), "{:?}", face.no_roads);
        let top = *road_of(&face, 0).pit_edge.vertices.last().unwrap();
        assert!((top[0] - E - 1200.0).abs() <= END_TOLERANCE, "{top:?}");
        assert_eq!(road_of(&face, 0).off_end, None);
        assert!((road_of(&face, 1).off_end.unwrap() - (400.0 * COS - 100.0)).abs() < 1e-3, "the end of the string is as close as it gets");
    }

    /// A toe that rises along the strip leaves a bench with no height at a road's start. The road starts on the
    /// first wall with height there, from the same place on the floor.
    #[test]
    fn a_road_starts_on_the_first_wall_with_height_where_it_is() {
        let toe = string(&[[0.0, 0.0], [1000.0, 0.0]], FLOOR);
        let toe = Polyline { vertices: toe.vertices.iter().map(|p| [p[0], p[1], FLOOR + 0.05 * (p[0] - E)]).collect(), closed: false };
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |x, _| FLOOR + 0.05 * (x - E)));
        let (low, high) = (FaceTarget::Rl(110.0), FaceTarget::Rl(160.0));
        let bench = FaceStep::Berm { on: Some(low), width: EdgeRule::Fixed(10.0), register: true, condition: None, min_bench: Some(1.0) };
        let rows = [climbed_by(low, &[0]), bench, climbed_by(high, &[0])];
        let face = project_face(&toe, &floor, OffsetSide::Left, FaceDirection::Up, &rows, &[ramp_at(400.0)]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(road_of(&face, 0).steps, [2, 2], "the first wall tops out at x = 200");
        let bottom = road_of(&face, 0).pit_edge.vertices[0];
        assert!((bottom[0] - E - 400.0).abs() < 1e-6 && (bottom[2] - 120.0).abs() < 1e-6, "{bottom:?}");
        for grade in rise_over_run(&road_of(&face, 0).pit_edge) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0, "nothing crossing");
    }

    /// A gap in a wall part way up doesn't stop a road: it carries on from the same place on the next wall. The
    /// second wall's ground falls below the first bench 200 m along, before the road gets there.
    #[test]
    fn a_road_carries_on_past_a_wall_with_no_height() {
        let ground = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| (140.0 - 0.1 * (x - E)).clamp(110.0, 140.0)));
        let on = FaceTarget::Rl(120.0);
        let rows = [wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Surface(&ground), 45.0), berm(on, 10.0), wall(FaceTarget::Rl(140.0), 45.0)];
        let face = ramped(RAMP, rows.to_vec(), &level(FLOOR));
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.roads[0].steps, [0, 4]);
        for grade in rise_over_run(&face.roads[0].pit_edge) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        assert!((face.roads[0].pit_edge.vertices.last().unwrap()[2] - 140.0).abs() < 1e-6);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0, "nothing crossing");
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
        let first = FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramps: &[0], turns: &[] };
        let bench = FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: Some(5.0) };
        let second = FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramps: if through { &[0] } else { &[] }, turns: &[] };
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

    /// A wall wandering round the minimum bench takes a road, and the berm above follows the crest round it.
    #[test]
    fn a_berm_above_a_road_follows_a_wall_wandering_round_the_minimum_bench() {
        let face = wandering_face(120.0, 0.01, 4.0, 10.0, OffsetSide::Right, true).unwrap();
        assert!(!face.roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(face.benches.len(), 2, "both walls are there");
        one_clean_sheet(&face);
    }

    /// A road that climbs two walls, up to a third with no height where it gets there and a fourth above that
    /// it would need 400 m of and has 300 m left on, stops on the second, 200 m from RL 100 to 120. It says the
    /// fourth runs out short of the top, and everything else builds. Before folds (0034) the wall above it on the
    /// fourth couldn't be built, and that's what it said.
    #[test]
    fn a_road_stops_below_a_wall_too_short_for_it() {
        let ground = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| 110.0 + 30.0 * ((260.0 - (x - E)) / 60.0).clamp(0.0, 1.0)));
        let (low, on) = (FaceTarget::Rl(110.0), FaceTarget::Rl(120.0));
        let rows = vec![wall(low, 45.0), berm(low, 10.0), wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Surface(&ground), 45.0), berm(on, 10.0), wall(FaceTarget::Rl(160.0), 45.0)];
        let face = ramped(RAMP, rows, &level(FLOOR));
        one_clean_sheet(&face);
        assert_eq!(face.roads.iter().map(|r| r.steps).collect::<Vec<_>>(), [[0, 2]], "{:?}", face.no_roads);
        let edge = &face.roads[0].pit_edge;
        assert!((edge.plan_length() - 200.0).abs() < 1e-6 && (edge.vertices.last().unwrap()[2] - 120.0).abs() < 1e-6, "{:?}", edge.vertices);
        assert_eq!(face.no_roads[0].step, 6, "{:?}", face.no_roads);
        assert!(face.no_roads[0].why.contains("short of the top"), "{:?}", face.no_roads);
        assert_eq!(face.benches.iter().map(|b| b.step).collect::<Vec<_>>(), [0, 2, 4, 6], "all four walls are there");
    }

    /// A road topping out on a seam that rises 5 % into the wall: its wall-side edge runs on past the inside edge's end to
    /// reach the seam, so its top crosses the road at a skew, the wall-side corner ahead. The crest just behind that corner
    /// is then as near the inside corner as the corner itself, and the crossing was taken for the crest behind the road:
    /// the road's heading came out across the berm, the line's step was taken from its top, not its foot, and the next
    /// toe zigzagged back over the step, folding the next stretch's wall-side edge out several road widths. Now the road
    /// crosses the berm and climbs on along the wall: its inside edge never turns back toward the pit, its wall-side edge
    /// is never more than a road width off it, and the face is one sheet with nothing crossing.
    #[test]
    fn a_road_after_the_berm_from_a_skewed_top_climbs_on_along_the_wall() {
        let plane = |y: f64| 120.0 + 0.05 * (y - N - 20.0);
        let seam = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 30, 20, |_, y| plane(y)));
        let on = FaceTarget::Surface(&seam);
        for width in [10.0, 20.0] {
            let face = long_face(&[climbed_by(on, &[0]), berm(on, 20.0), climbed_by(FaceTarget::Rl(160.0), &[0])], &[FaceRamp { width, after_berm: true, ..ramp_at(500.0) }]);
            assert!(face.no_roads.is_empty(), "{width}: {:?}", face.no_roads);
            one_clean_sheet(&face);
            let road = road_of(&face, 0);
            assert_eq!(road.steps, [0, 2]);
            // Each edge's top on the first wall is its last vertex before it crosses the berm.
            let before_the_berm = |edge: &Polyline| edge.vertices.windows(2).position(|w| w[1][1] - w[0][1] > 15.0 && w[1][1] - w[0][1] > (w[1][0] - w[0][0]).abs()).expect("across the berm");
            let top = before_the_berm(&road.pit_edge);
            let (pit, wall) = (road.pit_edge.vertices[top], road.wall_edge.vertices[before_the_berm(&road.wall_edge)]);
            assert!(wall[0] > pit[0] + 0.5, "{width}: the top crosses ahead, wall-side corner {wall:?}, inside corner {pit:?}");
            for w in road.pit_edge.vertices[top..].windows(2) {
                assert!(w[1][1] - w[0][1] > -1e-6, "{width}: the inside edge turns back toward the pit: {w:?}");
            }
            for p in &road.wall_edge.vertices {
                assert!(off_line(&road.pit_edge, *p) < width + 0.5, "{width}: the wall-side edge is {:.1} m off at {p:?}", off_line(&road.pit_edge, *p));
            }
            close(road.pit_edge.vertices.last().unwrap()[2], 160.0, 1e-6);
        }
    }
}
