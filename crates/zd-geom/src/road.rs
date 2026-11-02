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

/// Why a road can't go on a wall that has no height where it gets there. It tries the next wall that names it.
pub(crate) const NO_WALL: &str = "there's no wall at its start";
/// Why a road doesn't go on a wall holding one that runs the other way.
pub(crate) const OTHER_WAY: &str = "another road on this wall runs the other way, and roads on one wall run the same way for now";

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

/// A road going into a wall: where it got to on the wall below if it's carrying on, and if not, what's under
/// its entrance if the toe lies on a surface.
pub(crate) struct RoadOnWall<'a> {
    pub spec: FaceRamp,
    pub top: Option<RoadTop>,
    pub floor: Option<&'a TriMesh>,
}

/// How a stretch of road starts.
enum RoadBottom<'a> {
    /// At the bottom of the road. The toe behind it moves back as far as the vertex `from`, lying on `floor` if it
    /// lies on a surface.
    Entrance { floor: Option<&'a TriMesh>, from: usize },
    /// Carrying on from the wall below, whose road's wall-side edge came in at this vertex of the toe.
    CarriedOn { wall_side: usize },
}

/// Why a road stopped, and where.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stopped {
    pub at: P3,
    pub why: String,
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
    pub logs: Vec<WallLog>,
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

/// A wall with its roads cut back into it, up to the target. They all run one way. `wall` is the plain wall from
/// that toe. A road that can't go in goes to `stop` by its place in the list, and the others carry on without it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cut_roads(toe: &Polyline, batters: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], wall: &WallStep, target: &TriMesh, stop: &mut dyn FnMut(usize, Stopped)) -> Option<WallCut> {
    if !roads.first()?.spec.back {
        return cut_roads_along(toe, batters, side, roads, wall.clone(), target, stop);
    }
    let turned = WallStep { crest: backwards(&wall.crest), wall: wall.wall.clone(), log: wall.log.clone() };
    let batters: Vec<f64> = batters.iter().rev().copied().collect();
    let other = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
    let mut cut = cut_roads_along(&backwards(toe), &batters, other, roads, turned, target, stop)?;
    cut.crest.vertices.reverse();
    cut.toe.vertices.reverse();
    Some(cut)
}

/// Roads running the way the toe does, lowest first: the one that starts furthest along. Each goes into the wall
/// above the one before, and takes a fresh wall if it needs a vertex that wall hasn't got.
fn cut_roads_along(toe: &Polyline, batters: &[f64], side: OffsetSide, roads: &[RoadOnWall<'_>], plain: WallStep, target: &TriMesh, stop: &mut dyn FnMut(usize, Stopped)) -> Option<WallCut> {
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
        let start = line.vertices[at];
        if wall_side.is_none() && cuts.last().is_some_and(|(_, below)| line.vertices.len() - at <= below.up_the_road) {
            stop(r, Stopped { at: start, why: "it starts on the road below it: move it along the toe".into() });
            continue;
        }
        let fresh = if line.vertices.len() == string.vertices.len() {
            None
        } else {
            match project_wall_by_edge(&line, side, &widened, target) {
                Ok(fresh) => Some(fresh),
                Err(e) => {
                    stop(r, Stopped { at: start, why: format!("the wall it starts on can't be built: {e}") });
                    continue;
                }
            }
        };
        // An entrance moves the toe behind it back only as far as a road coming up through that toe.
        let from = roads.iter().filter_map(|other| other.top).filter_map(|top| line.vertices[..at].iter().rposition(|p| plan_gap(*p, top.pit) < SAME_POINT)).max().unwrap_or(0);
        let bottom = wall_side.map_or(RoadBottom::Entrance { floor: road.floor, from }, |wall_side| RoadBottom::CarriedOn { wall_side });
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
    for (r, cut) in cuts {
        meshes.extend([cut.lower, cut.road]);
        cut_roads.push((r, cut.pit_edge, cut.wall_edge));
    }
    meshes.push(wall.wall);
    Some(WallCut { meshes, toe, crest, roads: cut_roads, logs })
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
    let stopped = |why: String| Stopped { at: start, why };
    let facets = Facets::of(&wall.wall, start);
    let climb = facets.climb(start, side, ramp.grade, toe, &wall.crest)?;
    let pit_edge = Polyline { vertices: climb.edge.clone(), closed: false };
    let offset = wall_edge_of(&pit_edge, ramp.width, side).map_err(|e| stopped(format!("the road's width can't be held: {e}")))?;
    let (behind, foot, mut wall_edge) = match bottom {
        RoadBottom::Entrance { floor, from } => {
            let behind = entrance_toe(toe, batters, at, from, offset.vertices[0], side, floor).map_err(|e| stopped(format!("its entrance can't be built: {e}")))?;
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
    let cut = RoadCut { lower, road, toe: foot, behind: behind.vertices.len(), up_the_road: wall_edge.vertices.len(), crest: wall.crest.clone(), place, pit_edge, wall_edge };
    Ok((cut, Above { wall: upper, string, slopes }))
}

/// The string behind a road, and a batter for each of its segments and the one that leads to the road.
struct Behind {
    vertices: Vec<P3>,
    batters: Vec<f64>,
}

/// The toe behind a road's start, moved back by the road so trucks can drive along the floor straight onto
/// it. It moves as far as the road's wall-side edge starts from the toe, so the wall there and the wall above
/// the road are one plane. It stops a road width short of that edge, and the last stretch leads up to it, so
/// a floor that isn't level at the start of the road has room to meet it. Behind the vertex `from` it stays where
/// it is, for a road coming up through it, and comes across to the moved toe at 45 degrees.
fn entrance_toe(toe: &Polyline, batters: &[f64], at: usize, from: usize, wall_start: P3, side: OffsetSide, floor: Option<&TriMesh>) -> Result<Behind, GeomError> {
    let stays = |to: usize| Behind { vertices: toe.vertices[..to].to_vec(), batters: batters[..to].to_vec() };
    if at == from {
        return Ok(stays(from));
    }
    let road = Polyline { vertices: toe.vertices[from..=at].to_vec(), closed: false };
    let back = (0..at - from).map(|i| nearest_along(road.vertices[i], road.vertices[i + 1], wall_start).1).fold(f64::INFINITY, f64::min);
    let reach = plan_gap(toe.vertices[at], wall_start);
    let moved = longest(offset_string(&road, back, side, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?);
    let short = place_at_gap(&moved, nearest_place(&moved, wall_start), false, wall_start, reach);
    let across = if from == 0 { 0.0 } else { place_at_gap(&moved, 0.0, true, moved.vertices[0], back) };
    if short <= across {
        return Ok(stays(if from == 0 { 0 } else { from + 1 }));
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
    if from == 0 {
        let batters = moved_batters.collect();
        return Ok(Behind { vertices: laid, batters });
    }
    let mut kept = stays(from + 1);
    kept.batters.extend(moved_batters);
    kept.vertices.extend(laid);
    Ok(kept)
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
/// berm's width. `lay` puts each stretch on what the berm lies on.
pub(crate) fn berm_round_roads(line: &Polyline, crest: &Polyline, tops: &[RoadTop], back: bool, lay: &mut dyn FnMut(&Polyline) -> Result<Polyline, GeomError>) -> Result<Polyline, GeomError> {
    if back {
        let turned = berm_round_roads_along(&backwards(line), &backwards(crest), tops, lay)?;
        return Ok(backwards(&turned));
    }
    berm_round_roads_along(line, crest, tops, lay)
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
        let no_wall = || Stopped { at: start, why: NO_WALL.into() };
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
            format!("it reaches the end of the wall at RL {:.1}, short of the top", at[2])
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
        FaceStep::Project { to, batter: EdgeRule::Fixed(batter), condition: None, ramps: &[0] }
    }

    fn plain_wall(to: FaceTarget<'_>, batter: f64) -> FaceStep<'_> {
        FaceStep::Project { to, batter: EdgeRule::Fixed(batter), condition: None, ramps: &[] }
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
        FaceStep::Project { to, batter: EdgeRule::Fixed(45.0), condition: None, ramps }
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
    #[test]
    fn a_road_running_the_other_way_on_a_wall_says_so() {
        let face = long_face(&[climbed_by(FaceTarget::Rl(140.0), &[0, 1])], &[ramp_at(200.0), FaceRamp { back: true, ..ramp_at(1200.0) }]);
        assert_eq!(face.no_roads.len(), 1, "{:?}", face.no_roads);
        assert_eq!(face.no_roads[0].ramp, 1);
        assert_eq!(face.no_roads[0].why, OTHER_WAY);
        holds_its_grade_to(road_of(&face, 0), 140.0, 400.0);
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
        let above = FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(45.0), condition: None, ramps: &[1] };
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
            FaceStep::Project { to: FaceTarget::Surface(&surface), batter: EdgeRule::Fixed(60.0), condition: Some(0.1), ramps: &[0] },
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
        let first = FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramps: &[0] };
        let bench = FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: Some(5.0) };
        let second = FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramps: if through { &[0] } else { &[] } };
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

    /// A road that climbs two walls, up to a third with no height where it gets there and more wall above that
    /// can't be built on the road, stops a wall lower: at the last bench it can make. It says so, and everything
    /// else still builds.
    #[test]
    fn a_road_stops_a_wall_lower_if_whats_built_on_it_cant_be() {
        let ground = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| 110.0 + 30.0 * ((260.0 - (x - E)) / 60.0).clamp(0.0, 1.0)));
        let (low, on) = (FaceTarget::Rl(110.0), FaceTarget::Rl(120.0));
        let rows = vec![wall(low, 45.0), berm(low, 10.0), wall(on, 45.0), berm(on, 10.0), wall(FaceTarget::Surface(&ground), 45.0), berm(on, 10.0), wall(FaceTarget::Rl(160.0), 45.0)];
        let face = ramped(RAMP, rows, &level(FLOOR));
        assert_eq!(face.roads.iter().map(|r| r.steps).collect::<Vec<_>>(), [[0, 0]], "{:?}", face.no_roads);
        assert_eq!(face.no_roads[0].step, 2, "{:?}", face.no_roads);
        assert!(face.no_roads[0].why.contains("what's built on it can't be"), "{:?}", face.no_roads);
        assert_eq!(face.benches.iter().map(|b| b.step).collect::<Vec<_>>(), [0, 2, 4, 6], "all four walls are there");
    }
}
