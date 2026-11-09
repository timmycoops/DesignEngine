//! A whole face (Phase 3): a toe registered on its grid, then a list of steps going one way from it, each
//! a wall to a surface or RL or a berm. Built oversized on purpose: it runs on past the pit and the cut
//! does the fitting (decision 0023).
//!
//! Each wall comes from `project_wall`, and each row says whether its line is conditioned and, for a berm,
//! registered, before the next one. Steps at one batter with no berm between them are one wall: a face
//! projected to the topography and on to an RL is the same plane carrying straight through, not a new wall
//! from a rough line. Going down is the same thing upside down: the sheet carries on below the toe on the
//! far side, so heights are mirrored and the side swapped, and a wall that "rises" there is going down.
//! Plan winding doesn't change, so the sheet stays wound upward.
//!
//! A wall can have a road cut into it as it's built: see `road`.

use std::collections::HashMap;

use zd_core::{FaceDirection, Polyline, TriMesh};

use crate::road::{FaceRamp, FaceRoad, Flat, NO_WALL, NoRoad, RoadOnWall, RoadTop, Stopped, WallCut, across_the_berm, berm_round_roads, cut_roads, flat_past_top, metres_along, nearest_on, point_along, start_on, straight_across, with_road_top};
use crate::string::TriangleBins;
use crate::wall::{LeastBench, Stretch, at_crest_heights, height_on, nearest_along, next_line, plan_gap, segment_count, triangles_of, with_crest_vertices};
use crate::{EdgeRule, GeomError, OffsetSide, drape_string, filter_string, next_toe, project_wall_by_edge};

/// What a wall rises to.
#[derive(Debug, Clone, Copy)]
pub enum FaceTarget<'a> {
    Surface(&'a TriMesh),
    Rl(f64),
}

/// One step of a face.
#[derive(Debug, Clone, Copy)]
pub enum FaceStep<'a> {
    /// A wall at a batter in degrees. With a tolerance, its crest is conditioned for the next step. `ramps` are
    /// the roads that climb it, as places in the face's ramps. A road carries on up the walls that name it, and
    /// those in `turns` turn back before this one: see `Climbing::turn_at`.
    Project { to: FaceTarget<'a>, batter: EdgeRule<'a>, condition: Option<f64>, ramps: &'a [usize], turns: &'a [usize] },
    /// A berm of a width in metres. Its line registers on `on`, or on what the last wall reached if that's
    /// left out. With `register` off it keeps its crest's heights instead. With a tolerance it's
    /// conditioned after it's moved out. With `min_bench` there's no berm where the wall below it is lower. With `join`,
    /// switchbacks turning on it, on box pads facing each other, are one pad: the line it ends on runs straight from the
    /// back of one box to the back of the next (decision 0047). `to_come` is the wall after it, where that isn't in the face, as
    /// for a berm built on its own (decision 0048); with a wall after it in the face, that wall says.
    Berm { on: Option<FaceTarget<'a>>, width: EdgeRule<'a>, register: bool, condition: Option<f64>, min_bench: Option<f64>, join: JoinBoxes, to_come: Option<WallToCome<'a>> },
}

/// The wall to come after a berm built on its own (decision 0048): the roads turning back on the berm before it, as places in
/// the face's ramps, and the batter it's likely to stand at, in degrees. That sets where each next road's flat start cuts
/// into it, which the berm's line shows.
#[derive(Debug, Clone, Copy)]
pub struct WallToCome<'a> {
    pub turns: &'a [usize],
    pub batter: f64,
}

/// What a berm does with switchbacks turning on it, on box pads facing each other (decision 0047).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JoinBoxes {
    /// Boxes that come within a road's width of each other, or overlap, meet half way between the roads' tops as one pad:
    /// a berm between them that close is pinched to nothing (Tim, 2026-11-06). Others stand apart.
    #[default]
    Auto,
    /// One pad wherever they are: the line it ends on runs straight from the back of one box to the back of the next.
    On,
    /// Apart wherever they are, as before Join.
    Off,
}

/// One wall of a face.
#[derive(Debug, Clone)]
pub struct FaceBench {
    /// Where it is in the steps, from 0.
    pub step: usize,
    pub toe: Polyline,
    pub crest: Polyline,
}

/// What one step did.
#[derive(Debug, Clone)]
pub struct StepReport {
    pub step: usize,
    pub walls: usize,
    pub squeezed_out: usize,
    /// The step has no height anywhere: its target is on or below the string all the way along.
    pub no_wall: bool,
    /// Stretches where the target is below the string.
    pub no_height: Vec<Stretch>,
    /// Stretches where the target has no data and the crest bridges the gap.
    pub no_target: Vec<Stretch>,
    /// Vertices of the next toe with no surface under them, which hold their elevations.
    pub off_surface: usize,
    /// Pieces of a berm line left out when an offset pinched into several.
    pub pieces_dropped: usize,
    /// Stretches of a berm left out, as the wall below is under the minimum bench.
    pub no_berm: Vec<Stretch>,
}

/// A line of a face redrawn by hand (decision 0046): the line a step ends on, a wall's crest or a berm's line, as you
/// drew it. The steps above start from it, and the step it ends re-forms to meet it.
#[derive(Debug, Clone, Copy)]
pub struct FaceEdit<'a> {
    /// The step whose line it is, from 0.
    pub after: usize,
    pub line: &'a Polyline,
}

/// What a line redrawn by hand did.
#[derive(Debug, Clone, PartialEq)]
pub struct EditReport {
    /// The step whose line it is, from 0.
    pub after: usize,
    /// The berm it ends, its narrowest and widest across from the crest below (m).
    pub berm: Option<[f64; 2]>,
    /// The wall it ends, its flattest and steepest from the toe to your crest (degrees).
    pub batter: Option<[f64; 2]>,
    /// Roads on their way whose start wasn't on your line, which go in from the nearest place on it as they would on the
    /// line the steps made: the ramp, and how far your line is from that start (m).
    pub roads_off: Vec<(usize, f64)>,
    /// For a join, not a line redrawn by hand: the ramps whose box pads it joined, two by two (decision 0047).
    pub joined: Vec<[usize; 2]>,
}

/// A berm set to join switchbacks that had none to join (decision 0047): the run goes on as if it weren't set.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinNote {
    /// The berm's step, from 0.
    pub after: usize,
    /// The ramps turning back on a box pad on it, apart from those whose boxes met.
    pub turning: Vec<usize>,
    /// Neighbouring boxes on it that face away from each other, so nothing lies between them to join.
    pub facing_away: Vec<[usize; 2]>,
}

/// A whole face.
#[derive(Debug, Clone)]
pub struct Face {
    /// Walls and berms as one sheet, wound upward.
    pub surface: TriMesh,
    pub benches: Vec<FaceBench>,
    pub steps: Vec<StepReport>,
    pub roads: Vec<FaceRoad>,
    /// Ramps that didn't go in. The face is built without them.
    pub no_roads: Vec<NoRoad>,
    /// Where the face ends on a berm, the berm's line: where a wall after it would start. A step design shows it, and
    /// its next step starts there.
    pub berm_line: Option<Polyline>,
    /// What each line redrawn by hand did, in step order, and each join.
    pub edits: Vec<EditReport>,
    /// Berms set to join switchbacks that had none to join.
    pub join_notes: Vec<JoinNote>,
    /// Boxes made one pad by themselves, being within a road's width of each other or overlapping.
    pub met: Vec<MetBoxes>,
    /// Switchbacks whose tops are too close for both to turn back on one berm.
    pub too_close: Vec<TooClose>,
    /// Corners of the face's pieces made one with another within a millimetre, and the furthest any was apart (m).
    pub welded: (usize, f64),
}

/// Two switchbacks facing each other on a berm whose roads' tops are too close for both to turn on it (decision 0047):
/// boxes short enough to stand a road's width apart would be too short to turn on.
#[derive(Debug, Clone, PartialEq)]
pub struct TooClose {
    /// The berm's step, from 0.
    pub after: usize,
    /// The ramps, in the crest's order.
    pub ramps: [usize; 2],
    /// How far apart the tops are along the crest (m).
    pub apart: f64,
    /// How much further apart they'd need to be (m).
    pub short: f64,
}

/// Two switchbacks' boxes on a berm that would have come within a road's width of each other, or overlapped, and so are one
/// pad: each shortened to stand just over that width from the other, and joined from the back of one to the back of the
/// other (decision 0047).
#[derive(Debug, Clone, PartialEq)]
pub struct MetBoxes {
    /// The berm's step, from 0.
    pub after: usize,
    /// The ramps, in the crest's order.
    pub ramps: [usize; 2],
    /// How far apart the boxes would have been along the crest (m), less than nothing where they'd have overlapped.
    pub gap: f64,
}

/// What registering a toe did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegisterLog {
    /// Vertices with no grid under them, which carry on at the grade beside them.
    pub off_grid: usize,
}

/// A string extended straight on past each end by `start` and `end` metres in plan. Closed strings
/// have no ends. The new ends take the end's elevation, as they're registered next.
pub fn run_on(line: &Polyline, [start, end]: [f64; 2]) -> Result<Polyline, GeomError> {
    if !(start.is_finite() && end.is_finite() && start >= 0.0 && end >= 0.0) {
        return Err(GeomError::InvalidInput("a run-on must be zero or more".into()));
    }
    let n = line.vertices.len();
    if line.closed || n < 2 {
        return Ok(line.clone());
    }
    let past = |from: [f64; 3], towards: [f64; 3], distance: f64| {
        let length = (from[0] - towards[0]).hypot(from[1] - towards[1]);
        (length > 0.0).then(|| [from[0] + (from[0] - towards[0]) / length * distance, from[1] + (from[1] - towards[1]) / length * distance, from[2]])
    };
    let mut vertices = Vec::with_capacity(n + 2);
    if start > 0.0 {
        vertices.extend(past(line.vertices[0], line.vertices[1], start));
    }
    vertices.extend_from_slice(&line.vertices);
    if end > 0.0 {
        vertices.extend(past(line.vertices[n - 1], line.vertices[n - 2], end));
    }
    Ok(Polyline { vertices, closed: false })
}

/// A toe's plan shape simplified to within `tolerance` metres, ready to be registered. Its ends stay.
pub fn conditioned_toe(line: &Polyline, tolerance: f64) -> Result<Polyline, GeomError> {
    filter_string(&Polyline { vertices: line.vertices.iter().map(|v| [v[0], v[1], 0.0]).collect(), closed: line.closed }, tolerance)
}

/// A toe laid on the grid it starts from, following the grid between vertices. Where there's no grid
/// under it, in a no-seam hole or past an edge, it carries on at the grade beside it: straight across a
/// hole, and at its last grade past an end.
pub fn register_toe(line: &Polyline, surface: &TriMesh) -> Result<(Polyline, RegisterLog), GeomError> {
    let (mut toe, _) = drape_string(line, surface, true)?;
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &toe.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let on: Vec<bool> = toe.vertices.iter().map(|p| bins.height(surface, [p[0], p[1]]).is_some()).collect();
    let off_grid = on.iter().filter(|o| !**o).count();
    if off_grid == toe.vertices.len() {
        return Err(GeomError::InvalidInput("the toe doesn't cross the grid it registers to".into()));
    }
    let along = plan_distances(&toe.vertices);
    let known: Vec<usize> = (0..on.len()).filter(|&k| on[k]).collect();
    for k in (0..on.len()).filter(|&k| !on[k]) {
        let after = known.partition_point(|&i| i < k);
        let z = match (after.checked_sub(1).map(|i| known[i]), known.get(after).copied()) {
            (Some(a), Some(b)) => {
                let (za, zb) = (toe.vertices[a][2], toe.vertices[b][2]);
                za + (zb - za) * (along[k] - along[a]) / (along[b] - along[a])
            }
            (Some(a), None) => toe.vertices[a][2] + last_grade(&toe.vertices, &along, &known[..after], true) * (along[k] - along[a]),
            (None, Some(b)) => toe.vertices[b][2] + last_grade(&toe.vertices, &along, &known[after..], false) * (along[k] - along[b]),
            (None, None) => unreachable!("some vertex is on the grid"),
        };
        toe.vertices[k][2] = z;
    }
    Ok((toe, RegisterLog { off_grid }))
}

/// Plan distance along a string at each vertex.
fn plan_distances(points: &[[f64; 3]]) -> Vec<f64> {
    let mut along = Vec::with_capacity(points.len());
    let mut total = 0.0;
    for (k, p) in points.iter().enumerate() {
        if k > 0 {
            total += (p[0] - points[k - 1][0]).hypot(p[1] - points[k - 1][1]);
        }
        along.push(total);
    }
    along
}

/// How far back from an end of the known vertices a grade is read (m).
const GRADE_OVER: f64 = 20.0;

/// The grade per metre along the string over the last stretch of known vertices.
fn last_grade(points: &[[f64; 3]], along: &[f64], known: &[usize], at_the_far_end: bool) -> f64 {
    let (Some(&first), Some(&last)) = (known.first(), known.last()) else { return 0.0 };
    let (near, far) = if at_the_far_end {
        (last, known.iter().rev().copied().find(|&k| along[last] - along[k] >= GRADE_OVER).unwrap_or(first))
    } else {
        (first, known.iter().copied().find(|&k| along[k] - along[first] >= GRADE_OVER).unwrap_or(last))
    };
    if near == far { 0.0 } else { (points[near][2] - points[far][2]) / (along[near] - along[far]) }
}

/// A toe registered from a line a design run made, the crest of the design below, put back at that line's heights
/// wherever it runs within `within` of it in plan. That crest is already where its design left it: on its grid, above it
/// where the wall below had no height, and level across a road's top, so laying it on the grid again would move it off
/// what's below. Past the line's ends, where it was run on, the toe keeps the grid's heights (decision 0041).
pub fn at_line_heights(toe: &Polyline, line: &Polyline, within: f64) -> Polyline {
    let n = line.vertices.len();
    let height = |p: [f64; 3]| {
        (0..segment_count(line))
            .map(|i| {
                let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
                let (t, gap) = nearest_along(a, b, p);
                (gap, a[2] + t * (b[2] - a[2]))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .filter(|(gap, _)| *gap <= within)
            .map_or(p[2], |(_, z)| z)
    };
    Polyline { vertices: toe.vertices.iter().map(|p| [p[0], p[1], height(*p)]).collect(), closed: toe.closed }
}

/// How far past a toe a level floor reaches, in plan (m): past any entrance or berm laid on it.
const FLOOR_REACH: f64 = 1000.0;

/// A level floor at an RL under a string, for a toe to register on and lie flat. It goes through the same path as
/// any other surface: projection targets are triangulations.
pub fn level_floor(line: &Polyline, rl: f64) -> TriMesh {
    level_sheet(line, rl, FLOOR_REACH)
}

/// The whole face from a toe already on its grid, going one way. `register` is the surface the toe lies on.
///
/// Roads are cut back into its walls as they're built. On a face going down a road comes down from the top
/// line, as it climbs the face upside down. If what's built on a road can't be, the road stops a wall short,
/// the furthest road first, so a road stops at the last bench it can make, the face still builds and the run
/// says why.
///
/// A road with its end held has its start worked out here: see `face_with_held_ends`.
pub fn project_face(toe: &Polyline, register: &TriMesh, side: OffsetSide, direction: FaceDirection, steps: &[FaceStep<'_>], ramps: &[FaceRamp]) -> Result<Face, GeomError> {
    project_edited_face(toe, register, side, direction, steps, ramps, &[])
}

/// The whole face, with lines redrawn by hand standing in for those the steps make (decision 0046). A berm's line stands for
/// the toe of the wall above as the steps would make it, roads and all; a wall's crest for its crest, the wall below
/// re-formed to meet it. Your line is refused where it crosses the line below or comes out on its pit side.
pub fn project_edited_face(toe: &Polyline, register: &TriMesh, side: OffsetSide, direction: FaceDirection, steps: &[FaceStep<'_>], ramps: &[FaceRamp], edits: &[FaceEdit<'_>]) -> Result<Face, GeomError> {
    if let Some(edit) = edits.iter().find(|edit| edit.after >= steps.len()) {
        return Err(GeomError::Refused(format!("a line redrawn by hand is after step {}, and there are {} steps", edit.after + 1, steps.len())));
    }
    // Refused before anything runs, as a road that can't go in otherwise stops a wall lower and the face builds without it.
    if let Some(edit) = edits.iter().find(|edit| ramps_of(&steps[edit.after]).is_some_and(|ramps| !ramps.is_empty())) {
        return Err(GeomError::Refused(format!("step {}: {ROAD_ON_CREST}", edit.after + 1)));
    }
    if ramps.iter().any(|ramp| ramp.ends_at.is_some()) {
        return face_with_held_ends(toe, register, side, direction, steps, ramps, edits);
    }
    face_with_roads(toe, register, side, direction, steps, ramps, &HashMap::new(), edits)
}

/// A wall step's ramps, if it's a wall.
fn ramps_of<'a>(step: &FaceStep<'a>) -> Option<&'a [usize]> {
    if let FaceStep::Project { ramps, .. } = step { Some(ramps) } else { None }
}

/// The first and last steps a ramp climbs.
fn walls_of(steps: &[FaceStep<'_>], ramp: usize) -> Option<[usize; 2]> {
    let on = |step: &FaceStep<'_>| ramps_of(step).is_some_and(|ramps| ramps.contains(&ramp));
    Some([steps.iter().position(on)?, steps.iter().rposition(on)?])
}

/// How far from its held end a road may finish (m): finer than a point can be picked on a crest.
pub(crate) const END_TOLERANCE: f64 = 0.25;
/// How many times a face is run to bring its roads out at their held ends.
const MOST_END_RUNS: usize = 8;
/// The most a run moves a road's start, as a multiple of what the end missed by. A surface climbing nearly
/// as fast as the road makes the end move much further than the start, and a step can't trust that far.
const MOST_END_STEP: f64 = 4.0;
/// How far a start is tried either side when a road from it can't reach the top (m).
const END_NUDGE: f64 = 5.0;

/// A road being brought out at its held end.
struct HeldEnd {
    ramp: usize,
    /// Along the toe, in metres: where it has to come out, and where it starts from now.
    end: f64,
    start: f64,
    /// The last start that got to the top, and what it missed by.
    last: Option<(f64, f64)>,
    nudges: usize,
}

impl HeldEnd {
    /// Takes in what a run missed by, and where to start next. Moving the start usually moves the end by as
    /// much, but a surface that rises along the wall stretches that, so the last two runs say by how much.
    /// A road can't start past the end of its string, so it stays on it.
    fn missed(&mut self, miss: f64, length: f64) {
        let by_one = -miss;
        let step = match self.last {
            Some((start, before)) if (miss - before).abs() > 1e-9 && start != self.start => -miss * (self.start - start) / (miss - before),
            _ => by_one,
        };
        self.last = Some((self.start, miss));
        self.start = (self.start + step.clamp(-MOST_END_STEP * miss.abs(), MOST_END_STEP * miss.abs())).clamp(0.0, length);
    }

    /// A start that didn't get to the top: try a little either side of the last one that was going to.
    fn nudged(&mut self, length: f64) {
        self.nudges += 1;
        let way = if self.nudges % 2 == 1 { 1.0 } else { -1.0 };
        self.start = (self.start + way * self.nudges as f64 * END_NUDGE).clamp(0.0, length);
    }
}

/// A face whose roads come out where they have to. A road's height is settled by its walls, so its length
/// nearly is, and moving its start along the toe moves its end. The first guess comes from the walls' heights
/// at the end, and each run moves the start as a planner would by hand. It's usually there in three or four
/// runs. If it can't get there, as when it would have to start past the end of the string, the closest it came
/// is what's kept, and the road says how far off it is.
fn face_with_held_ends(toe: &Polyline, register: &TriMesh, side: OffsetSide, direction: FaceDirection, steps: &[FaceStep<'_>], ramps: &[FaceRamp], edits: &[FaceEdit<'_>]) -> Result<Face, GeomError> {
    let no_roads: HashMap<usize, usize> = (0..ramps.len()).map(|r| (r, 0)).collect();
    let plain = face_with_roads(toe, register, side, direction, steps, ramps, &no_roads, edits)?;
    let length = toe.plan_length();
    let mut held: Vec<HeldEnd> = Vec::new();
    for (r, ramp) in ramps.iter().enumerate() {
        let Some(at) = ramp.ends_at else { continue };
        let at = [at[0], at[1], 0.0];
        let end = metres_along(toe, at);
        let bench_at = |step: usize| plain.benches.iter().find(|b| b.step >= step);
        // Down as much as up: on a face going down the last crest is the lowest line.
        let rise = walls_of(steps, r).and_then(|[first, last]| Some(height_on(&bench_at(last)?.crest, at) - height_on(&bench_at(first)?.toe, at))).unwrap_or(0.0).abs();
        let road = rise / ramp.grade;
        held.push(HeldEnd { ramp: r, end, start: (if ramp.back { end + road } else { end - road }).clamp(0.0, length), last: None, nudges: 0 });
    }
    let mut spec = ramps.to_vec();
    // The face with the most roads at the top, and of those the one with the least left between them. A road that
    // can't get any closer, as at the end of the string, mustn't hide the others getting there.
    let mut best: Option<((usize, f64), Face)> = None;
    for _ in 0..MOST_END_RUNS {
        for road in &held {
            let p = point_along(toe, road.start);
            spec[road.ramp].start = [p[0], p[1]];
        }
        let mut face = face_with_roads(toe, register, side, direction, steps, &spec, &HashMap::new(), edits)?;
        let (mut short, mut left) = (0, 0.0);
        let tried: Vec<f64> = held.iter().map(|road| road.start).collect();
        for road in &mut held {
            let stopped_short = face.no_roads.iter().any(|n| n.ramp == road.ramp);
            let reached = face.roads.iter_mut().find(|r| r.ramp == road.ramp && !stopped_short);
            let Some(reached) = reached else {
                short += 1;
                road.nudged(length);
                continue;
            };
            let miss = metres_along(toe, *reached.pit_edge.vertices.last().expect("a road has an end")) - road.end;
            reached.off_end = (miss.abs() > END_TOLERANCE).then_some(miss.abs());
            if miss.abs() > END_TOLERANCE {
                left += miss.abs();
                road.missed(miss, length);
            }
        }
        let closer = best.as_ref().is_none_or(|(least, _)| (short, left) < *least);
        if closer {
            best = Some(((short, left), face));
        }
        if (short == 0 && left == 0.0) || held.iter().map(|road| road.start).eq(tried) {
            break;
        }
    }
    let (_, face) = best.expect("a face was run");
    Ok(face)
}

/// The face with its roads as far as they go. A road that what's built on it can't stand on stops a wall lower.
#[allow(clippy::too_many_arguments)]
fn face_with_roads(toe: &Polyline, register: &TriMesh, side: OffsetSide, direction: FaceDirection, steps: &[FaceStep<'_>], ramps: &[FaceRamp], limits: &HashMap<usize, usize>, edits: &[FaceEdit<'_>]) -> Result<Face, GeomError> {
    let mut limits = limits.clone();
    let mut held_back: HashMap<usize, NoRoad> = HashMap::new();
    loop {
        let mut face = FaceParts::default();
        match run_face(toe, register, side, direction, steps, ramps, &limits, edits, &mut face) {
            Ok(()) => {
                face.no_roads.extend(held_back.into_values());
                face.no_roads.sort_by_key(|n| n.ramp);
                let (surface, welded) = face_sheet(&face.meshes, &face.meets);
                return Ok(Face { surface, welded, benches: face.benches, steps: face.steps, roads: face.roads, no_roads: face.no_roads, berm_line: face.berm_line, edits: face.edits, join_notes: face.join_notes, met: face.met, too_close: face.too_close });
            }
            Err(e @ GeomError::Refused(_)) => return Err(e),
            Err(e) => {
                let highest = face.last_wall.iter().max_by_key(|(_, step)| **step).map(|(r, step)| (*r, *step));
                let Some((r, step)) = highest else { return Err(e) };
                limits.insert(r, face.walls_climbed[&r] - 1);
                let why = format!("what's built on it can't be: {}", e.to_string().replace("invalid input: ", ""));
                let why = if direction == FaceDirection::Down { going_down(&why) } else { why };
                held_back.insert(r, NoRoad { ramp: r, step, at: [ramps[r].start[0], ramps[r].start[1], 0.0], why, crosses: None, too_close: None });
            }
        }
    }
}

/// The steps run, with each ramp in `limits` stopping after that many walls.
#[allow(clippy::too_many_arguments)]
fn run_face(toe: &Polyline, register: &TriMesh, side: OffsetSide, direction: FaceDirection, steps: &[FaceStep<'_>], ramps: &[FaceRamp], limits: &HashMap<usize, usize>, edits: &[FaceEdit<'_>], face: &mut FaceParts) -> Result<(), GeomError> {
    match direction {
        FaceDirection::Up => run_steps(toe, register, side, steps, ramps, limits, edits, face)?,
        FaceDirection::Down => {
            let (toe, surface) = (mirrored_line(toe), mirrored_mesh(register));
            let swapped = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
            let surfaces: Vec<Option<TriMesh>> = steps
                .iter()
                .map(|step| match target_of(step) {
                    Some(FaceTarget::Surface(mesh)) => Some(mirrored_mesh(mesh)),
                    _ => None,
                })
                .collect();
            let mirrored: Vec<FaceStep<'_>> = steps
                .iter()
                .zip(&surfaces)
                .map(|(step, mesh)| match *step {
                    FaceStep::Project { to, batter, condition, ramps, turns } => FaceStep::Project { to: mirrored_target(to, mesh.as_ref()), batter: batter.upside_down(), condition, ramps, turns },
                    FaceStep::Berm { on, width, register, condition, min_bench, join, to_come } => FaceStep::Berm { on: on.map(|on| mirrored_target(on, mesh.as_ref())), width: width.upside_down(), register, condition, min_bench, join, to_come },
                })
                .collect();
            let lines: Vec<Polyline> = edits.iter().map(|edit| mirrored_line(edit.line)).collect();
            let upside_down: Vec<FaceEdit<'_>> = edits.iter().zip(&lines).map(|(edit, line)| FaceEdit { after: edit.after, line }).collect();
            run_steps(&toe, &surface, swapped, &mirrored, ramps, limits, &upside_down, face)?;
            face.mirror();
        }
    }
    Ok(())
}

/// What a step goes to, where it names one.
fn target_of<'a>(step: &FaceStep<'a>) -> Option<FaceTarget<'a>> {
    match step {
        FaceStep::Project { to, .. } => Some(*to),
        FaceStep::Berm { on, .. } => *on,
    }
}

fn mirrored_target<'a>(target: FaceTarget<'_>, mesh: Option<&'a TriMesh>) -> FaceTarget<'a> {
    match (target, mesh) {
        (FaceTarget::Rl(rl), _) => FaceTarget::Rl(-rl),
        (FaceTarget::Surface(_), Some(mesh)) => FaceTarget::Surface(mesh),
        (FaceTarget::Surface(_), None) => unreachable!("every surface is mirrored first"),
    }
}

/// What the steps have made so far.
#[derive(Default)]
struct FaceParts {
    meshes: Vec<TriMesh>,
    /// Points where one piece meets another part way along an edge of it: where a road wider than the berm starts its
    /// next stretch on the top of the last (decision 0038). The sheet is split there once it's welded.
    meets: Vec<[f64; 3]>,
    benches: Vec<FaceBench>,
    steps: Vec<StepReport>,
    roads: Vec<FaceRoad>,
    no_roads: Vec<NoRoad>,
    berm_line: Option<Polyline>,
    edits: Vec<EditReport>,
    join_notes: Vec<JoinNote>,
    met: Vec<MetBoxes>,
    too_close: Vec<TooClose>,
    /// By ramp: how many walls it's climbed so far, and the last of them.
    walls_climbed: HashMap<usize, usize>,
    last_wall: HashMap<usize, usize>,
}

impl FaceParts {
    fn mirror(&mut self) {
        for mesh in &mut self.meshes {
            *mesh = mirrored_mesh(mesh);
        }
        for p in &mut self.meets {
            p[2] = -p[2];
        }
        for bench in &mut self.benches {
            bench.toe = mirrored_line(&bench.toe);
            bench.crest = mirrored_line(&bench.crest);
        }
        if let Some(line) = &mut self.berm_line {
            *line = mirrored_line(line);
        }
        for stretch in self.steps.iter_mut().flat_map(|s| s.no_height.iter_mut().chain(s.no_target.iter_mut())) {
            stretch.from[2] = -stretch.from[2];
            stretch.to[2] = -stretch.to[2];
        }
        // Upside down, the edge a road climbed against its wall is the one under the wall above it the right way
        // up, so the two edges swap: the pit side is still the side the face goes down to.
        for road in &mut self.roads {
            (road.pit_edge, road.wall_edge) = (mirrored_line(&road.wall_edge), mirrored_line(&road.pit_edge));
            for (at, _) in &mut road.steep_flats {
                at[2] = -at[2];
            }
        }
        for stopped in &mut self.no_roads {
            stopped.at[2] = -stopped.at[2];
            stopped.why = going_down(&stopped.why);
        }
    }
}

/// Why a road stopped, written for a road climbing a face upside down, put right for one coming down it: RLs the
/// right way up, and the words that say which way it goes.
pub(crate) fn going_down(why: &str) -> String {
    const TURNED: [(&str, &str); 8] = [
        ("the toe rises faster than", "the top line falls faster than"),
        ("it can't leave the toe", "it can't leave the top line"),
        ("it drops back to the toe", "it climbs back to the top line"),
        ("short of the top", "short of the bottom"),
        ("it starts on the road below it: move it along the toe", "it starts on the road above it: move it along the top line"),
        ("the wall above it can't be built", "the wall below it can't be built"),
        ("a way up the wall", "a way down the wall"),
        ("the road's top isn't on the crest", "the road's foot isn't on the toe"),
    ];
    let mut why = TURNED.iter().fold(why.to_string(), |why, (up, down)| why.replace(up, down));
    // Every RL a message gives comes from the face upside down.
    let mut out = String::with_capacity(why.len());
    while let Some(at) = why.find("RL ") {
        let (before, rest) = why.split_at(at + 3);
        out.push_str(before);
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
        match rest[..end].trim_end_matches('.').parse::<f64>() {
            Ok(rl) => {
                let number = &rest[..end].trim_end_matches('.');
                let decimals = number.split('.').nth(1).map_or(0, str::len);
                // Adding 0.0 turns -0.0 into 0.0, so RL 0 doesn't read "-0.0".
                out.push_str(&format!("{:.*}", decimals, -rl + 0.0));
                why = rest[number.len()..].to_string();
            }
            Err(_) => why = rest.to_string(),
        }
    }
    out.push_str(&why);
    out
}

/// One mesh, sharing vertices within a micrometre of each other. A wall and a berm work out the same point apart, so the
/// two can land either side of a rounding step: each point looks in the cells round its own.
pub(crate) fn weld<'a>(meshes: impl IntoIterator<Item = &'a TriMesh>) -> TriMesh {
    weld_within(meshes, WELD).0
}

/// The same within `tolerance`, with how many corners were made one with another and the furthest any was (m).
fn weld_within<'a>(meshes: impl IntoIterator<Item = &'a TriMesh>, tolerance: f64) -> (TriMesh, usize, f64) {
    let (mut merged, mut furthest) = (0, 0.0_f64);
    let mut out = TriMesh::default();
    let mut numbered: HashMap<[i64; 3], u32> = HashMap::new();
    for mesh in meshes {
        let renumbered: Vec<u32> = mesh
            .vertices
            .iter()
            .map(|p| {
                let cell = p.map(|c| (c / tolerance).round() as i64);
                let apart = |q: [f64; 3]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
                let same = |v: &u32| apart(out.vertices[*v as usize]) <= tolerance;
                match cells_round(cell).find_map(|c| numbered.get(&c).filter(|v| same(v)).copied()) {
                    Some(v) => {
                        let gap = apart(out.vertices[v as usize]);
                        if gap > WELD {
                            merged += 1;
                            furthest = furthest.max(gap);
                        }
                        v
                    }
                    None => {
                        out.vertices.push(*p);
                        numbered.insert(cell, out.vertices.len() as u32 - 1);
                        out.vertices.len() as u32 - 1
                    }
                }
            })
            .collect();
        for t in &mesh.triangles {
            let [a, b, c] = t.map(|v| renumbered[v as usize]);
            if a != b && b != c && c != a {
                out.triangles.push([a, b, c]);
            }
        }
    }
    (out, merged, furthest)
}

/// A face's walls and berms as one sheet. They're welded where their corners meet, split where a road's top `meets`
/// them, and stitched where a corner of one lies part way along an edge of another, as it does where a wall has no
/// height and its crest runs along the berm's line with its own vertices (fault 5 of decision 0034: a sliver of a
/// crack, with no area, that left the face in more than one loop). Nothing moves.
fn face_sheet(meshes: &[TriMesh], meets: &[[f64; 3]]) -> (TriMesh, (usize, f64)) {
    let (welded, merged, furthest) = weld_within(meshes, SHEET_WELD);
    (crate::strips::stitch(split_at(welded, meets, SHEET_WELD), SHEET_WELD).0, (merged, furthest))
}

/// How close two corners of a face's pieces are to be made one (m), as `face.join` welds a strip's faces (0049). A
/// corner worked out two ways, by a road climbing the wall and by the wall above it, or a wall that only just survives
/// to the crest, lands a hair from itself, and the sliver between the two can fold.
const SHEET_WELD: f64 = 1e-3;

/// A welded sheet with every triangle that has one of `points` part way along an edge split there, so the pieces that
/// meet at the point share it. Nothing moves.
fn split_at(mut mesh: TriMesh, points: &[[f64; 3]], tolerance: f64) -> TriMesh {
    for p in points {
        let v = mesh.vertices.iter().position(|q| q.iter().zip(p).all(|(a, b)| (a - b).abs() <= tolerance)).unwrap_or_else(|| {
            mesh.vertices.push(*p);
            mesh.vertices.len() - 1
        }) as u32;
        let mut more = Vec::new();
        for t in &mut mesh.triangles {
            let Some(slot) = (0..3).find(|&s| {
                let (a, b) = (t[s], t[(s + 1) % 3]);
                a != v && b != v && {
                    let (pa, pb) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
                    let (along, gap) = nearest_along(pa, pb, *p);
                    along > 0.0 && along < 1.0 && gap <= tolerance && (pa[2] + along * (pb[2] - pa[2]) - p[2]).abs() <= tolerance
                }
            }) else {
                continue;
            };
            let [a, b, c] = [t[slot], t[(slot + 1) % 3], t[(slot + 2) % 3]];
            *t = [a, v, c];
            more.push([v, b, c]);
        }
        mesh.triangles.extend(more);
    }
    mesh
}

/// How far past a string a level sheet reaches, in plan: well past any wall to it.
fn sheet_reach(line: &Polyline, rl: f64, batter: f64) -> f64 {
    let lowest = line.vertices.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    100.0 + 8.0 * (rl - lowest).abs() / batter.to_radians().tan()
}

/// A level sheet at an RL over a string's plan area, as a target.
pub(crate) fn level_sheet(line: &Polyline, rl: f64, reach: f64) -> TriMesh {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &line.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let (x0, y0, x1, y1) = (lo[0] - reach, lo[1] - reach, hi[0] + reach, hi[1] + reach);
    TriMesh::new(vec![[x0, y0, rl], [x1, y0, rl], [x1, y1, rl], [x0, y1, rl]], vec![[0, 1, 2], [0, 2, 3]])
}

/// A crest waiting to become the next toe, and how its row conditions it.
struct Waiting {
    crest: Polyline,
    condition: Option<f64>,
}

/// A road on its way up the walls.
struct Climbing {
    ramp: usize,
    spec: FaceRamp,
    /// The steps with a wall it's climbed so far.
    steps: Option<[usize; 2]>,
    top: Option<RoadTop>,
    /// Where it came onto the line across a pad, if it turned back, so its next stretch runs flat along the line from
    /// there a road width before it climbs, and the toe past the turn joins back.
    turned: Option<[f64; 2]>,
    /// The wall-side top of the road before a turn, which its crossing of the pad passes.
    pad_corner: Option<[f64; 3]>,
    pit_edge: Vec<[f64; 3]>,
    wall_edge: Vec<[f64; 3]>,
    /// Flats before a turn steeper along than the road's grade: where each ends, and how steep it goes.
    steep_flats: Vec<([f64; 3], f64)>,
    /// The far end of the box pad it's turning on, its corners on the crest and a road width in: none with no box.
    pad: Option<[[f64; 3]; 2]>,
    /// Its box came within a road's width of another's on the berm it's turning on, or overlapped it, so the two are one
    /// pad (decision 0047).
    met: bool,
}

impl Climbing {
    fn new(ramp: usize, spec: FaceRamp) -> Self {
        Climbing { ramp, spec, steps: None, top: None, turned: None, pad_corner: None, pit_edge: Vec::new(), wall_edge: Vec::new(), steep_flats: Vec::new(), pad: None, met: false }
    }

    /// Turns on a box pad past its top: the road ends at its top, and the box runs on in line with it this far.
    fn boxed(&mut self, flat: &Flat) {
        let end = *flat.pit.vertices.last().expect("a flat has an end");
        self.pad = Some([end, *flat.wall.vertices.last().expect("a flat has an end")]);
        let steepest = flat.steepest();
        if steepest > self.spec.grade + 1e-9 {
            self.steep_flats.push((end, steepest));
        }
    }

    /// Takes in a wall's stretch of road, and where the road has got to.
    fn climbed(&mut self, step: usize, pit_edge: &Polyline, wall_edge: &Polyline) {
        self.steps = Some([self.steps.map_or(step, |s| s[0]), step]);
        // A pad on a floor that isn't level is crossed level across the top of the last road, then down or up the floor
        // to where this stretch came onto the line, and from there it's level across the road. The pit-side edge turns
        // at the last road's wall-side top, and the wall-side edge comes from there to where this one came onto the line
        // before it crosses this road, as the pad does.
        if let (Some(corner), Some(&last), Some(&first)) = (self.pad_corner.take(), self.pit_edge.last(), pit_edge.vertices.first()) {
            let apart = |p: &[f64; 3], q: &[f64; 3]| (p[0] - q[0]).hypot(p[1] - q[1]) > WELD;
            if (last[2] - first[2]).abs() > WELD && apart(&last, &corner) && apart(&first, &corner) {
                self.pit_edge.push(corner);
            }
            let from_the_top = self.wall_edge.last().is_some_and(|wall| !apart(wall, &corner));
            if (corner[2] - first[2]).abs() > WELD && from_the_top && apart(&first, &corner) && wall_edge.vertices.first().is_some_and(|w| apart(w, &first)) {
                self.wall_edge.push(first);
            }
        }
        for (edge, more) in [(&mut self.pit_edge, pit_edge), (&mut self.wall_edge, wall_edge)] {
            let skip = usize::from(edge.last() == more.vertices.first());
            edge.extend_from_slice(&more.vertices[skip..]);
        }
        self.top = Some(RoadTop { pit: *self.pit_edge.last().expect("it's climbed"), wall: *self.wall_edge.last().expect("it's climbed") });
    }

    /// Turns back: a switchback on a pad, level on an RL and on the floor on a grid. The next wall's stretch comes onto the
    /// line at `at`, straight across the berm from the top, and climbs from there the other way. Past the turn the toe of
    /// that wall goes round the box pad, or with none comes out to the next road's wall-side edge and joins back.
    fn turn_at(&mut self, at: [f64; 3]) {
        self.spec.back = !self.spec.back;
        self.spec.start = [at[0], at[1]];
        self.pad_corner = self.top.map(|top| top.wall);
        self.top = None;
        self.turned = Some([at[0], at[1]]);
    }

    /// Carries on from its start on a line redrawn by hand, as a road carries on from its top: with no entrance, turn or
    /// pad to make again, as the line has them, or left them out on purpose (decision 0046). A road that turned keeps the
    /// corner its edges cross a pad by, on a floor that isn't level.
    fn carry_on_from(&mut self, top: RoadTop) {
        self.spec.start = [top.wall[0], top.wall[1]];
        (self.top, self.turned, self.pad) = (Some(top), None, None);
    }

    /// The road as far as it got.
    fn road(self) -> Option<FaceRoad> {
        let steps = self.steps?;
        Some(FaceRoad { ramp: self.ramp, steps, pit_edge: Polyline { vertices: self.pit_edge, closed: false }, wall_edge: Polyline { vertices: self.wall_edge, closed: false }, off_end: None, back: self.spec.back, steep_flats: self.steep_flats })
    }
}

#[allow(clippy::too_many_arguments)]
fn run_steps<'a>(toe: &Polyline, register: &'a TriMesh, side: OffsetSide, steps: &[FaceStep<'a>], ramps: &[FaceRamp], limits: &HashMap<usize, usize>, edits: &[FaceEdit<'_>], face: &mut FaceParts) -> Result<(), GeomError> {
    let edit_after = |step: usize| edits.iter().find(|edit| edit.after == step).map(|edit| edit.line);
    let mut from = toe.clone();
    let mut waiting: Option<Waiting> = None;
    // The crest a berm starts from. The berm waits for the wall above it, as that wall's crest can come
    // down onto `from` part way along a segment.
    let mut berm_from: Option<Polyline> = None;
    // The crest under the last berm as the wall below left it, before any box rounds it: what a line redrawn by hand for
    // that berm stands clear of and is measured from, as a crest going round a box loops back on itself.
    let mut berm_crest: Option<Polyline> = None;
    let mut on = Resting::Surface(register);
    // Whether `from` lies on `on`. A berm that isn't registered keeps its crest's heights.
    let mut lying = true;
    let mut climbing: Vec<Climbing> = Vec::new();
    // A road that came out on this toe in the design below is already on its way, if the first wall is one of its own.
    // The toe takes its top back in, level across the road, as the crest it came out on had it, and its edges start
    // there, so a pad crossed level across that top keeps its corner.
    // With no wall in the face at all, as a berm built on its own, each one crosses it, for the berm to make way for it.
    let first_wall = steps.iter().find_map(ramps_of);
    for (r, ramp) in ramps.iter().enumerate() {
        if let Some(came_out) = ramp.carries_on.filter(|_| first_wall.is_none_or(|wall| wall.contains(&r)) && limits.get(&r) != Some(&0)) {
            let top;
            (from, top) = with_road_top(&from, came_out);
            climbing.push(Climbing { top: Some(top), pit_edge: vec![top.pit], wall_edge: vec![top.wall], ..Climbing::new(r, *ramp) });
        }
    }
    // Roads that have stopped, which walls further up can't start again.
    let mut stopped: Vec<usize> = Vec::new();
    let fail = |k: usize, e: GeomError| match e {
        GeomError::Refused(why) => GeomError::Refused(format!("step {}: {why}", k + 1)),
        e => GeomError::InvalidInput(format!("step {}: {e}", k + 1)),
    };
    let mut k = 0;
    while k < steps.len() {
        match steps[k] {
            FaceStep::Project { batter, ramps: here, turns, .. } => {
                if let Some(Waiting { crest, condition }) = waiting.take() {
                    from = match condition.filter(|t| *t > 0.0) {
                        // A conditioned line weaves either side of its crest, so with no berm here it
                        // stands clear of it by the tolerance, which can only widen what's there.
                        Some(tolerance) => {
                            let (toes, _) = next_toe(&crest, tolerance, side, tolerance, on.mesh()).map_err(|e| fail(k, e))?;
                            berm_from = Some(crest);
                            lying = true;
                            longest(toes)
                        }
                        None => crest,
                    };
                }
                // A road turning back with no berm since its last wall turns on the crest, from its wall-side top, with no
                // flat: there's no berm for one to lie on.
                for open in climbing.iter_mut().filter(|open| turns.contains(&open.ramp)) {
                    if let Some(top) = open.top {
                        open.turn_at(top.wall);
                    }
                }
                let joining: Vec<usize> = here.iter().copied().filter(|r| !stopped.contains(r) && limits.get(r) != Some(&0) && !climbing.iter().any(|open| open.ramp == *r)).collect();
                // Roads running opposite ways sit side by side on a wall: where they'd share a stretch of it, the wall says so.
                for r in joining {
                    climbing.push(Climbing::new(r, ramps[r]));
                }
                let (line, mut batters) = batter.applied_to(&from).map_err(|e| fail(k, e))?;
                from = line;
                let mut flattest = batters.iter().copied().fold(f64::INFINITY, f64::min);
                // Steps at one batter with no berm between them are one wall carrying straight on: the
                // targets on the way only mark where its crest lines fall.
                // A road turning back starts a new wall, and a crest redrawn by hand ends one.
                let mut ended = false;
                let wall: Vec<(usize, FaceTarget<'a>, Option<f64>)> = (k..steps.len())
                    .map_while(|j| match steps[j] {
                        FaceStep::Project { to, batter: b, condition, ramps: r, turns: t } if !ended && b.same_as(&batter) && r == here && (j == k || t.is_empty()) => {
                            ended = edit_after(j).is_some();
                            Some((j, to, condition))
                        }
                        _ => None,
                    })
                    .collect();
                let (&(last, to, condition), marks) = wall.split_last().expect("this step at least");
                if from.closed {
                    for open in climbing.drain(..) {
                        face.no_roads.push(NoRoad { ramp: open.ramp, step: k, at: from.vertices[0], why: "a ramp needs an open string for now".into(), crosses: None, too_close: None });
                        stopped.push(open.ramp);
                        face.roads.extend(open.road());
                    }
                }
                // A berm's line redrawn by hand stands for this wall's toe as the steps would make it, roads and all
                // (decision 0046). A road on its way whose start on that toe is on your line carries on from there as it
                // is: your line has its entrance or the pad it turned on, or left them out on purpose. One whose start
                // isn't goes in from the nearest place on your line, as it would on the steps' line, and is reported.
                // A berm set to join the switchbacks on it does the same with the steps' own line joined from the back of
                // one box to the back of the next (decision 0047). Your line, if there is one, comes first.
                let mut toe_edited = false;
                let berm_before = k.checked_sub(1).filter(|before| matches!(steps[*before], FaceStep::Berm { .. }));
                let yours = berm_before.and_then(edit_after);
                let mut redrawn: Option<Redrawn> = None;
                let joining = |before: &usize| match steps[*before] {
                    FaceStep::Berm { join: JoinBoxes::On, .. } => true,
                    FaceStep::Berm { join: JoinBoxes::Auto, .. } => climbing.iter().any(|open| open.met),
                    _ => false,
                };
                if let Some(before) = berm_before.filter(|before| yours.is_none() && joining(before)) {
                    // Left to itself the berm joins only the boxes it brought a road's width apart.
                    let only_met = matches!(steps[before], FaceStep::Berm { join: JoinBoxes::Auto, .. });
                    let (starts, made, mut boxes) = road_starts(&from, &batters, side, &climbing, lying.then(|| on.mesh()), to, flattest);
                    if only_met {
                        boxes.retain(|(r, ..)| climbing[*r].met);
                    }
                    let ramp = |r: usize| climbing[r].ramp;
                    match joined_boxes(&made, &boxes) {
                        Ok((line, pairs)) => redrawn = Some(Redrawn { line, starts, made, joined: pairs.iter().map(|[a, b]| [ramp(*a), ramp(*b)]).collect() }),
                        Err(away) => face.join_notes.push(JoinNote { after: before, turning: boxes.iter().map(|(r, ..)| ramp(*r)).collect(), facing_away: away.iter().map(|[a, b]| [ramp(*a), ramp(*b)]).collect() }),
                    }
                }
                if let (None, Some(drawn)) = (&redrawn, yours) {
                    let (starts, made, _) = road_starts(&from, &batters, side, &climbing, lying.then(|| on.mesh()), to, flattest);
                    redrawn = Some(Redrawn { line: drawn.clone(), starts, made, joined: Vec::new() });
                }
                if let Some(Redrawn { line: drawn, starts, made, joined }) = redrawn {
                    let drawn = &drawn;
                    let line = match (&berm_from, lying) {
                        (_, true) => laid_on(drawn, &made, Floor::Surface(on.mesh())),
                        (Some(crest), false) => laid_on(drawn, &made, Floor::Crest(crest)),
                        (None, false) => laid_on(drawn, &made, Floor::None),
                    };
                    toe_edited = true;
                    let mut report = EditReport { after: k - 1, berm: None, batter: None, roads_off: Vec::new(), joined };
                    if let Some(crest) = berm_crest.as_ref().or(berm_from.as_ref()) {
                        stands_clear(&line, crest, side).map_err(|e| fail(k - 1, e))?;
                        report.berm = across(&line, crest).map(|[narrowest, widest, ..]| [narrowest, widest]);
                    }
                    for (open, start) in climbing.iter_mut().zip(starts) {
                        let Some(start) = start else { continue };
                        let off = gap_to(&line, start.pit).max(gap_to(&line, start.wall));
                        if off > ON_YOUR_LINE {
                            report.roads_off.push((open.ramp, off));
                            continue;
                        }
                        // Its corners go into your line where it reaches them, at your line's heights, as they would into the steps'.
                        open.carry_on_from(start);
                    }
                    (from, batters) = batter.applied_to(&line).map_err(|e| fail(k, e))?;
                    flattest = batters.iter().copied().fold(f64::INFINITY, f64::min);
                    face.edits.push(report);
                }
                let on_wall: Vec<RoadOnWall<'_>> = climbing.iter().map(|open| RoadOnWall { spec: open.spec, top: open.top, floor: lying.then(|| on.mesh()), turned: open.turned, pad: open.pad }).collect();
                for road in &on_wall {
                    (from, batters, ..) = start_on(&from, &batters, road);
                }
                let target = Resting::of(to, &from, flattest);
                let built = project_wall_by_edge(&from, side, &batters, target.mesh());
                let mut gave_up: Vec<(usize, Stopped)> = Vec::new();
                let cut: Option<WallCut> = built.as_ref().ok().and_then(|wall| cut_roads(&from, &batters, side, &on_wall, wall, target.mesh(), &mut |r, stop| gave_up.push((r, stop))));
                // Where the wall crosses each target on its way, with the roads that went into it cut in too, so the
                // line runs round their entrances and pads and across them as the wall does, and stands on its foot.
                let kept: Vec<RoadOnWall<'_>> = on_wall.iter().enumerate().filter(|(r, _)| !gave_up.iter().any(|(gone, _)| gone == r)).map(|(_, road)| *road).collect();
                for &(j, to, _) in marks {
                    let mark = Resting::of(to, &from, flattest);
                    match project_wall_by_edge(&from, side, &batters, mark.mesh()) {
                        Ok(wall) => {
                            face.steps.push(wall_report(j, &wall));
                            let mut all_in = true;
                            let with_roads = if cut.is_some() && !kept.is_empty() { cut_roads(&from, &batters, side, &kept, &wall, mark.mesh(), &mut |_, _| all_in = false) } else { None };
                            let (toe, crest) = match with_roads.filter(|_| all_in) {
                                Some(marked) => (marked.toe, marked.crest),
                                None => (from.clone(), wall.crest),
                            };
                            face.benches.push(FaceBench { step: j, toe, crest });
                        }
                        // The wall carries on through here whatever this target does.
                        Err(_) => face.steps.push(no_wall_report(j)),
                    }
                }
                match built {
                    Ok(wall) => {
                        let mut report = wall_report(last, &wall);
                        let (foot, crest) = match &cut {
                            Some(cut) => (cut.toe.clone(), cut.crest.clone()),
                            None => (from.clone(), wall.crest.clone()),
                        };
                        let gaps: Vec<Stretch> = cut.as_ref().map_or_else(Vec::new, |cut| cut.logs.iter().flat_map(|log| &log.no_height).filter(|stretch| !across_a_road_end(stretch, &cut.roads)).copied().collect());
                        let no_height = !wall.log.no_height.is_empty() || !gaps.is_empty();
                        let toe = if no_height { with_touches(&foot, &crest) } else { foot };
                        // A crest redrawn by hand: the wall below re-forms to meet it, straight from its toe (decision 0046).
                        let redrawn = match edit_after(last) {
                            Some(drawn) => Some(redrawn_crest(drawn, &toe, &crest, side, target.mesh(), last, !on_wall.is_empty()).map_err(|e| fail(last, e))?),
                            None => None,
                        };
                        // The targets on the way are all on this one wall, which stands on this toe.
                        for bench in face.benches.iter_mut().filter(|bench| marks.iter().any(|(j, ..)| *j == bench.step)) {
                            bench.toe = toe.clone();
                        }
                        let pads: Vec<Pad> = cut.iter().flat_map(|cut| &cut.flats).filter_map(|(r, pit, wall)| climbing[*r].pad_corner.map(|top| Pad { top, pit: pit.clone(), wall: wall.clone() })).collect();
                        // A berm that can't reach your line where it is says so, not the road on it.
                        finish_berm_round_pads(&mut berm_from, &toe, &pads, face).map_err(|e| if toe_edited { refused_beside(k - 1, &e) } else { fail(last, e) })?;
                        match cut {
                            Some(cut) => {
                                merge_stretches(&mut report.no_height, &gaps);
                                for log in &cut.logs {
                                    merge_stretches(&mut report.no_target, &log.no_target);
                                }
                                for (r, pit_edge, wall_edge) in &cut.roads {
                                    let open = &mut climbing[*r];
                                    open.climbed(last, pit_edge, wall_edge);
                                    *face.walls_climbed.entry(open.ramp).or_default() += 1;
                                    face.last_wall.insert(open.ramp, last);
                                }
                                face.meshes.extend(cut.meshes);
                                face.meets.extend(cut.meets);
                            }
                            None => face.meshes.push(match &redrawn {
                                Some((crest, _)) => band(&toe, crest).map_err(|e| refused_beside(last, &e))?,
                                None => wall.wall,
                            }),
                        }
                        // The road one would cross, by its ramp, before any are taken out of the list.
                        let gave_up: Vec<(usize, Stopped)> = gave_up.into_iter().map(|(r, stop)| (r, Stopped { crosses: stop.crosses.map(|other| climbing[other].ramp), ..stop })).collect();
                        let mut gave_up = gave_up;
                        gave_up.sort_by_key(|(r, _)| std::cmp::Reverse(*r));
                        for (r, stop) in gave_up {
                            // A wall with no height where a road gets there has nothing to cut, so the road goes
                            // on to the next wall that names it.
                            let more_walls = steps[last + 1..].iter().any(|step| ramps_of(step).is_some_and(|ramps| ramps.contains(&climbing[r].ramp)));
                            if stop.why == NO_WALL && more_walls {
                                continue;
                            }
                            let open = climbing.remove(r);
                            // With none left and a road already on its way, it has come out of the pit.
                            if open.top.is_none() || stop.why != NO_WALL {
                                face.no_roads.push(NoRoad { ramp: open.ramp, step: last, at: stop.at, why: stop.why, crosses: stop.crosses, too_close: None });
                            }
                            stopped.push(open.ramp);
                            face.roads.extend(open.road());
                        }
                        face.steps.push(report);
                        let crest = match redrawn {
                            Some((line, edited)) => {
                                face.edits.push(edited);
                                line
                            }
                            None => crest,
                        };
                        face.benches.push(FaceBench { step: last, toe, crest: crest.clone() });
                        waiting = Some(Waiting { crest, condition });
                        on = target;
                        lying = true;
                    }
                    Err(GeomError::NoHeight) => {
                        // Nothing to build here: the sheet carries on from the same string.
                        face.steps.push(no_wall_report(last));
                        finish_berm(&mut berm_from, &from, face).map_err(|e| fail(last, e))?;
                        waiting = Some(Waiting { crest: from.clone(), condition });
                    }
                    Err(e) => return Err(fail(last, e)),
                }
                k = last + 1;
                // A road finishes at the top of its last wall, or where it's held back to.
                let next_wall = steps[k..].iter().find_map(ramps_of).unwrap_or(&[]);
                let finished = |open: &Climbing| !next_wall.contains(&open.ramp) || face.walls_climbed.get(&open.ramp).is_some_and(|walls| limits.get(&open.ramp).is_some_and(|limit| walls >= limit));
                let (done, going): (Vec<Climbing>, Vec<Climbing>) = climbing.drain(..).partition(|open| finished(open));
                climbing = going;
                for open in done {
                    stopped.push(open.ramp);
                    face.roads.extend(open.road());
                }
            }
            FaceStep::Berm { on: named, width, register, condition, min_bench, join, to_come } => {
                let waited = waiting.take();
                // With no wall below it in this design, as where it starts from the crest of the one below, there's no
                // height to hold a minimum bench to, so the berm stays.
                let walled = waited.is_some();
                let Waiting { crest, condition: before } = waited.unwrap_or_else(|| Waiting { crest: from.clone(), condition: None });
                finish_berm(&mut berm_from, &from, face).map_err(|e| fail(k, e))?;
                if let Some(target) = named {
                    on = Resting::of(target, &crest, BERM_BATTER);
                }
                let turning: &[usize] = steps[k + 1..].iter().find_map(|step| if let FaceStep::Project { turns, .. } = step { Some(*turns) } else { None }).unwrap_or(to_come.map_or(&[][..], |wall| wall.turns));
                // A road turning back at the next wall turns on a flat box past its top, in line with itself, on what the
                // berm lies on, so on a seam floor no coal is left under it (Tim, 2026-11-04): the road's width of it
                // along the crest, the berm beyond, and the next road's width beyond that, where the wall above goes round
                // it. The next road starts at its near end, straight across the berm from the top.
                let mut crest = crest;
                let plain_crest = crest.clone();
                let mut boxes: Vec<Flat> = Vec::new();
                // The crest each turning road's pad holds up, from its wall-side top before any flat to its pit-side
                // corner at the end: the berm's line beside it lies on the target.
                let mut pads: Vec<RoadTop> = Vec::new();
                for open in climbing.iter_mut() {
                    open.met = false;
                }
                // Switchbacks facing each other whose boxes would come within a road's width of each other, or overlap, are
                // one pad unless the berm keeps them apart (decision 0047): a berm between boxes that close is pinched to
                // nothing, and folds. Each box is shortened, or lengthened, to stand just over a road's width from the
                // other, where the berm between them builds, and the wall above joins them from the back of one to the
                // back of the other, as Join does.
                let (mut meeting, too_close) = if join == JoinBoxes::Off { (Vec::new(), Vec::new()) } else { boxes_meeting(&crest, &climbing, turning, width, side) };
                // Two whose tops are too close for both to turn on this berm: the later ramp stops at its top, and says why.
                if !too_close.is_empty() {
                    let mut gone: Vec<(usize, usize, f64, f64)> = too_close.iter().map(|&(a, b, apart, short)| if climbing[a].ramp > climbing[b].ramp { (a, climbing[b].ramp, apart, short) } else { (b, climbing[a].ramp, apart, short) }).collect();
                    gone.sort_by_key(|x| std::cmp::Reverse(x.0));
                    for (r, other, apart, short) in gone {
                        let open = climbing.remove(r);
                        let at = open.top.map_or(from.vertices[0], |top| top.pit);
                        face.no_roads.push(NoRoad { ramp: open.ramp, step: k, at, why: String::new(), crosses: None, too_close: Some((other, apart, short)) });
                        face.too_close.push(TooClose { after: k, ramps: [other, open.ramp], apart, short });
                        stopped.push(open.ramp);
                        face.roads.extend(open.road());
                    }
                    meeting = boxes_meeting(&crest, &climbing, turning, width, side).0;
                }
                let mut lengths: HashMap<usize, f64> = HashMap::new();
                for &(a, b, [long_a, long_b], gap) in &meeting {
                    lengths.insert(a, long_a);
                    lengths.insert(b, long_b);
                    climbing[a].met = true;
                    climbing[b].met = true;
                    face.met.push(MetBoxes { after: k, ramps: [climbing[a].ramp, climbing[b].ramp], gap });
                }
                for (r, open) in climbing.iter_mut().enumerate().filter(|(_, open)| turning.contains(&open.ramp)) {
                    let Some(top) = open.top else { continue };
                    let turn_flat = lengths.get(&r).copied().unwrap_or(open.spec.turn_flat);
                    let mut lay_flat = |part: &Polyline| -> Result<Polyline, GeomError> { if register { Ok(drape_string(part, on.mesh(), true)?.0) } else { Ok(at_crest_heights(part, &crest)) } };
                    let flat = if turn_flat > 0.0 { flat_past_top(&crest, top, turn_flat, width, side, &mut lay_flat).map_err(|e| fail(k, e))? } else { None };
                    let Some(flat) = flat else {
                        open.pad = None;
                        pads.push(top);
                        continue;
                    };
                    let (strip_pit, strip_wall) = flat.road_strip();
                    face.meshes.push(band(&strip_pit, &strip_wall).map_err(|e| fail(k, e))?);
                    if !flat.to_on_vertex {
                        face.meets.extend(flat.pit.vertices.last().copied());
                    }
                    open.boxed(&flat);
                    crest = flat.round(&crest);
                    pads.push(RoadTop { pit: *flat.pit.vertices.last().expect("a flat has an end"), wall: top.wall });
                    boxes.push(flat);
                }
                let least = min_bench.filter(|h| *h > 0.0 && walled).map(|height| LeastBench { toe: &from, height });
                let (lines, log) = next_line(&crest, width, least, side, before.unwrap_or(0.0), condition.unwrap_or(0.0)).map_err(|e| fail(k, e))?;
                let dropped = lines.len().saturating_sub(1);
                let mut line = longest(lines);
                // Each box's far end square across, with the berm standing out into the wall beside it and past it, not on
                // round its end (Tim, 2026-11-04): past the box it's the berm's line with no box.
                if !boxes.is_empty() {
                    let plain = longest(next_line(&plain_crest, width, least, side, before.unwrap_or(0.0), condition.unwrap_or(0.0)).map_err(|e| fail(k, e))?.0);
                    for flat in &boxes {
                        if let Some((squared, part_way)) = flat.squared_end(&line, &plain) {
                            line = squared;
                            face.meets.extend(part_way);
                        }
                    }
                }
                let mut report = no_wall_report(k);
                let mut lay = |part: &Polyline| -> Result<Polyline, GeomError> {
                    if !register {
                        return Ok(at_crest_heights(&with_crest_vertices(part, &crest, |_, on, _| Some(on)), &crest));
                    }
                    let (mut toe, draped) = drape_string(part, on.mesh(), true)?;
                    hold_above_the_target(&mut toe, &crest, on.mesh(), &pads);
                    report.off_surface += draped.off_surface;
                    Ok(toe)
                };
                // A road going up through the berm takes it in to its top. One going after it leaves it whole, and
                // carries on from where the berm's line steps round its top. One turning back at the next wall
                // leaves it whole too, and starts that wall straight across the berm from its wall-side top.
                let through: Vec<(RoadTop, bool)> = climbing.iter().filter(|open| !open.spec.after_berm && !turning.contains(&open.ramp)).filter_map(|open| open.top.map(|top| (top, open.spec.back))).collect();
                from = if through.is_empty() { lay(&line) } else { berm_round_roads(&line, &crest, &through, &mut lay) }.map_err(|e| fail(k, e))?;
                for open in climbing.iter_mut().filter(|open| open.spec.after_berm && !turning.contains(&open.ramp)) {
                    if let Some(top) = open.top {
                        let (line, landing, meets) = across_the_berm(&from, &crest, top, width, side);
                        (from, open.top) = (line, Some(landing));
                        face.meets.extend(meets);
                    }
                }
                for open in climbing.iter_mut().filter(|open| turning.contains(&open.ramp)) {
                    if let Some(top) = open.top {
                        // On a box the next road starts straight above the top, on the line nearest the road's wall-side
                        // top; with none, in line with the top crossing carried on across the berm.
                        let at = if open.pad.is_some() { nearest_on(&from, top.wall) } else { straight_across(&from, &crest, top, width, side) };
                        open.turn_at(at);
                    }
                }
                // A berm's line redrawn by hand with no wall after it: the design ends there, or another berm starts from it.
                // One with a wall after it stands for that wall's toe, which the wall takes in.
                if let Some(drawn) = edit_after(k).filter(|_| !matches!(steps.get(k + 1), Some(FaceStep::Project { .. }))) {
                    let line = laid_on(drawn, &from, if register { Floor::Surface(on.mesh()) } else { Floor::Crest(&crest) });
                    stands_clear(&line, &plain_crest, side).map_err(|e| fail(k, e))?;
                    face.edits.push(EditReport { after: k, berm: across(&line, &plain_crest).map(|[narrowest, widest, ..]| [narrowest, widest]), batter: None, roads_off: Vec::new(), joined: Vec::new() });
                    from = line;
                }
                lying = register;
                berm_from = Some(crest);
                berm_crest = Some(plain_crest);
                (report.no_wall, report.pieces_dropped, report.no_berm) = (false, dropped, log.no_berm);
                face.steps.push(report);
                k += 1;
            }
        }
    }
    if let Some(&FaceStep::Berm { join, to_come, .. }) = steps.last() {
        let batter = to_come.map_or(STAND_IN_BATTER, |wall| wall.batter);
        face.berm_line = Some(line_for_the_wall_above(&from, side, &climbing, lying.then(|| on.mesh()), join, batter, steps.len() - 1, face));
    }
    finish_berm(&mut berm_from, &from, face).map_err(|e| fail(steps.len().saturating_sub(1), e))?;
    face.roads.extend(climbing.drain(..).filter_map(Climbing::road));
    // The last crest goes nowhere, so conditioning it only cleans the string: the sheet keeps its own edge.
    // A crest redrawn by hand stays as it was drawn.
    if let Some(Waiting { crest, condition: Some(tolerance) }) = waiting
        && tolerance > 0.0
        && edit_after(steps.len() - 1).is_none()
        && let Some(bench) = face.benches.last_mut().filter(|b| b.step + 1 == steps.len())
    {
        let (lines, _) = next_toe(&crest, 0.0, side, tolerance, on.mesh()).map_err(|e| fail(bench.step, e))?;
        bench.crest = longest(lines);
    }
    Ok(())
}

/// The line a face ends on, a berm's, as the wall above will stand on it where roads turn back on the berm: each next road's
/// flat start cut back into that wall across its box, as the wall's toe has it, and the boxes joined as one pad where the
/// berm says to, or left to itself where they met (decisions 0038, 0047). A berm built on its own shows the line the next
/// wall will stand on (decision 0048). The flat starts lie on the berm, but where each meets the wall above at its far end
/// moves with the wall's batter, by a third of a metre between 60° and 65° on a 30 m road; its height doesn't change them.
/// A stand-in wall `STAND_IN_HEIGHT` high at `batter` cuts them. The berm's sheet stays as it was, as the wall above builds
/// the berm again round the pads.
#[allow(clippy::too_many_arguments)]
fn line_for_the_wall_above(from: &Polyline, side: OffsetSide, climbing: &[Climbing], floor: Option<&TriMesh>, join: JoinBoxes, batter: f64, k: usize, face: &mut FaceParts) -> Polyline {
    if !climbing.iter().any(|open| open.turned.is_some()) {
        return from.clone();
    }
    let Ok((line, batters)) = EdgeRule::Fixed(batter).applied_to(from) else { return from.clone() };
    let top = from.vertices.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max) + STAND_IN_HEIGHT;
    let (_, made, mut boxes) = road_starts(&line, &batters, side, climbing, floor, FaceTarget::Rl(top), batter);
    let joining = match join {
        JoinBoxes::On => true,
        JoinBoxes::Auto => climbing.iter().any(|open| open.met),
        JoinBoxes::Off => false,
    };
    if !joining {
        return made;
    }
    if join == JoinBoxes::Auto {
        boxes.retain(|(r, ..)| climbing[*r].met);
    }
    let ramp = |r: usize| climbing[r].ramp;
    match joined_boxes(&made, &boxes) {
        Ok((line, pairs)) => {
            face.edits.push(EditReport { after: k, berm: None, batter: None, roads_off: Vec::new(), joined: pairs.iter().map(|[a, b]| [ramp(*a), ramp(*b)]).collect() });
            line
        }
        Err(away) => {
            face.join_notes.push(JoinNote { after: k, turning: boxes.iter().map(|(r, ..)| ramp(*r)).collect(), facing_away: away.iter().map(|[a, b]| [ramp(*a), ramp(*b)]).collect() });
            made
        }
    }
}

/// The wall standing in for the one above a berm built on its own, to cut the next roads' flat starts: how high, and its
/// batter where the berm doesn't say (m, degrees).
const STAND_IN_HEIGHT: f64 = 20.0;
const STAND_IN_BATTER: f64 = 65.0;

/// Why a crest can't be redrawn where a road climbs its wall: the road's cut is built with the wall.
const ROAD_ON_CREST: &str = "a road climbs this wall, so its crest can't be redrawn yet: redraw the line after the berm above it instead";

/// How far a road's start may be from a line redrawn by hand for it to carry on from there (m): further than conditioning
/// can move a line, as for a road carrying on from a crest (decision 0041).
const ON_YOUR_LINE: f64 = 2.0;

/// How far a line redrawn by hand may stand past the line below, on its pit side, before it's refused (m): the usual
/// conditioning, as far as the lines the steps make stand past each other where a wall has no height.
const PIT_SIDE: f64 = 0.1;

/// The toe the steps make for a wall, and where each road on its way up it starts on that toe, as its two edges' first
/// corners, by its place in `climbing`: what a line redrawn by hand in place of that toe carries it on from. None for a
/// road that wouldn't go in.
/// Box pads on a berm's line, each as a place in the roads climbing, the line's vertex at the back of its far end, and
/// where along the line its road's top is (`boxes_on`).
type BoxCorners = Vec<(usize, usize, f64)>;

/// Boxes joined, or facing away from each other, two by two: places in a list of roads.
type BoxPairs = Vec<[usize; 2]>;

/// A line a wall stands on in place of the one the steps make, a berm's line redrawn by hand or joined from box to box
/// (decision 0047): the line, where the roads would start on the steps' own, that line, and the box pads joined, by ramp.
struct Redrawn {
    line: Polyline,
    starts: Vec<Option<RoadTop>>,
    made: Polyline,
    joined: BoxPairs,
}

fn road_starts(from: &Polyline, batters: &[f64], side: OffsetSide, climbing: &[Climbing], floor: Option<&TriMesh>, to: FaceTarget<'_>, flattest: f64) -> (Vec<Option<RoadTop>>, Polyline, BoxCorners) {
    let on_wall: Vec<RoadOnWall<'_>> = climbing.iter().map(|open| RoadOnWall { spec: open.spec, top: open.top, floor, turned: open.turned, pad: open.pad }).collect();
    let (mut toe, mut slopes) = (from.clone(), batters.to_vec());
    for road in &on_wall {
        (toe, slopes, ..) = start_on(&toe, &slopes, road);
    }
    let target = Resting::of(to, &toe, flattest);
    let Ok(wall) = project_wall_by_edge(&toe, side, &slopes, target.mesh()) else { return (vec![None; climbing.len()], toe, Vec::new()) };
    let cut = cut_roads(&toe, &slopes, side, &on_wall, &wall, target.mesh(), &mut |_, _| ());
    let starts = (0..climbing.len()).map(|r| cut.as_ref()?.roads.iter().find(|(i, ..)| *i == r).map(|(_, pit, wall)| RoadTop { pit: pit.vertices[0], wall: wall.vertices[0] })).collect();
    // As the wall's toe is kept: with a vertex wherever its crest touches it, at the crest's height.
    let made = match &cut {
        Some(cut) => with_touches(&cut.toe, &cut.crest),
        None => with_touches(&toe, &wall.crest),
    };
    let boxes = boxes_on(&made, climbing);
    (starts, made, boxes)
}

/// Each road turning back on a box pad, as a place in `climbing`, with the vertex of `made` at the back of the box's far
/// end, and where along `made` the road's top is: the box runs from there to its far end. The box's far end runs square
/// off the crest (decision 0038), so its back corner is the vertex of the line on it furthest out from the crest.
fn boxes_on(made: &Polyline, climbing: &[Climbing]) -> BoxCorners {
    climbing
        .iter()
        .enumerate()
        .filter_map(|(r, open)| {
            let ([pit, wall], top) = (open.pad?, open.pad_corner?);
            let across = plan_gap(pit, wall);
            if across < ON_THE_LINE {
                return None;
            }
            let way = [(wall[0] - pit[0]) / across, (wall[1] - pit[1]) / across];
            let off = |q: [f64; 3]| ((q[0] - pit[0]) * way[1] - (q[1] - pit[1]) * way[0]).abs();
            let out = |q: [f64; 3]| (q[0] - pit[0]) * way[0] + (q[1] - pit[1]) * way[1];
            let far = (0..made.vertices.len()).filter(|&i| off(made.vertices[i]) < ON_THE_LINE && out(made.vertices[i]) > across - ON_THE_LINE).max_by(|&i, &j| out(made.vertices[i]).total_cmp(&out(made.vertices[j])))?;
            Some((r, far, place_along(made, top)))
        })
        .collect()
}

/// Where along a line the nearest place to a point is in plan, as a segment and the way along it: 2.5 is half way along
/// the third segment.
fn place_along(line: &Polyline, p: [f64; 3]) -> f64 {
    let along = |i: usize| nearest_along(line.vertices[i], line.vertices[i + 1], p);
    (0..line.vertices.len().saturating_sub(1)).map(|i| (i as f64 + along(i).0, along(i).1)).min_by(|a, b| a.1.total_cmp(&b.1)).map_or(0.0, |(place, _)| place)
}

/// How near a box's back corner is taken to be the line's vertex there (m): it's the same point, put in by the box.
const ON_THE_LINE: f64 = 0.01;

/// Pairs of boxes to be one pad, as places in the roads climbing in the crest's order, how far each runs past its road's
/// top, and the gap they'd have had (`boxes_meeting`).
type Meetings = Vec<(usize, usize, [f64; 2], f64)>;

/// Pairs of roads whose tops are too close to share a berm, as places in the roads climbing, how far apart the tops are and
/// how much further apart they'd need to be (`boxes_meeting`).
type TopsTooClose = Vec<(usize, usize, f64, f64)>;

/// The least a box is let run past its road's top when it's shortened to stand clear of another, as a share of the road's
/// width: shorter, the road can't turn on it.
const LEAST_BOX: f64 = 1.0 / 3.0;
/// How far from a bend in the crest a box shortened to stand clear of another is kept from ending (m).
const CLEAR_OF_A_BEND: f64 = 5.0;

/// How far to move the gap between two boxes along the crest, as metres onto the first box and off the second, so both
/// their ends, `ends` metres along it, stand `CLEAR_OF_A_BEND` from its inner vertices: the least move that does, no more
/// than each box can give (`spare`, first and second), or none.
fn clear_of_bends(crest: &Polyline, ends: [f64; 2], spare: [f64; 2]) -> f64 {
    let mut along = 0.0;
    let mut bends = Vec::new();
    for w in crest.vertices.windows(2) {
        along += plan_gap(w[0], w[1]);
        bends.push(along);
    }
    bends.pop();
    let clear = |shift: f64| bends.iter().all(|bend| (ends[0] + shift - bend).abs() >= CLEAR_OF_A_BEND && (ends[1] + shift - bend).abs() >= CLEAR_OF_A_BEND);
    let mut step = 0.0;
    while step <= 2.0 * CLEAR_OF_A_BEND {
        for shift in [step, -step] {
            if shift <= spare[1] && -shift <= spare[0] && clear(shift) {
                return shift;
            }
        }
        step += 0.5;
    }
    0.0
}
/// How much further than a road's width apart two close boxes are set, so the roads leaving them are clear of each other
/// and the berm between them builds (m).
const CLEAR_OF_A_ROAD: f64 = 1.0;

/// Switchbacks turning back on a berm whose boxes face each other and would come within the wider road's width of each
/// other along the crest, or overlap (decision 0047, Tim 2026-11-06: "one road width is a good trigger"): each pair, as
/// places in `climbing` in the crest's order, how far each box runs past its road's top to stand just over that width from
/// the other, the difference shared between the two, and the gap they'd have had (less than nothing for an overlap).
fn boxes_meeting(crest: &Polyline, climbing: &[Climbing], turning: &[usize], berm: EdgeRule<'_>, side: OffsetSide) -> (Meetings, TopsTooClose) {
    // Each turning road with a box: where its top is along the crest, which way its box runs, and how long it is.
    let mut keep = |part: &Polyline| -> Result<Polyline, GeomError> { Ok(part.clone()) };
    let mut boxes: Vec<(usize, f64, bool, f64, f64)> = climbing
        .iter()
        .enumerate()
        .filter(|(_, open)| turning.contains(&open.ramp) && open.spec.turn_flat > 0.0)
        .filter_map(|(r, open)| {
            let top = open.top?;
            let flat = flat_past_top(crest, top, open.spec.turn_flat, berm, side, &mut keep).ok()??;
            Some((r, metres_along(crest, top.pit), flat.forward, open.spec.turn_flat, open.spec.width))
        })
        .collect();
    boxes.sort_by(|x, y| x.1.total_cmp(&y.1));
    let (mut meeting, mut too_close) = (Vec::new(), Vec::new());
    for pair in boxes.windows(2) {
        let [(a, at_a, ahead_a, long_a, wide_a), (b, at_b, ahead_b, long_b, wide_b)] = [pair[0], pair[1]];
        if !ahead_a || ahead_b || meeting.iter().any(|m: &(usize, usize, [f64; 2], f64)| m.1 == a) {
            continue;
        }
        let (gap, apart) = (at_b - at_a - long_a - long_b, wide_a.max(wide_b));
        if gap >= apart {
            continue;
        }
        let (long_a, long_b) = (long_a + (gap - apart - CLEAR_OF_A_ROAD) / 2.0, long_b + (gap - apart - CLEAR_OF_A_ROAD) / 2.0);
        let least = apart * LEAST_BOX;
        if long_a >= least && long_b >= least {
            // A box ending just short of or past a bend in the crest folds the berm there, so the gap between them moves
            // along a little, the nearest way, until both ends stand clear of the crest's bends, where it can.
            let shift = clear_of_bends(crest, [at_a + long_a, at_b - long_b], [long_a - least, long_b - least]);
            meeting.push((a, b, [long_a + shift, long_b - shift], gap));
        } else {
            // The tops are so close that boxes short enough to stand a road's width apart are too short to turn on: how far
            // apart the tops are, and how much further they'd need to be.
            let need = 2.0 * least + apart + CLEAR_OF_A_ROAD;
            too_close.push((a, b, at_b - at_a, need - (at_b - at_a)));
        }
    }
    (meeting, too_close)
}

/// The steps' line after a berm with the switchbacks on it joined (decision 0047): neighbouring boxes along it that face
/// each other, each with its far end towards the other, become one pad, the line running straight from the back of one's
/// far end to the back of the other's, as Tim drew it by hand on 2026-11-06. Gives the line and the boxes it joined, as
/// places in `boxes`' list, or the neighbours that face away from each other when none join.
fn joined_boxes(made: &Polyline, boxes: &[(usize, usize, f64)]) -> Result<(Polyline, BoxPairs), BoxPairs> {
    let mut on = boxes.to_vec();
    on.sort_by_key(|&(_, far, _)| far);
    let (mut cuts, mut pairs, mut away) = (Vec::new(), Vec::new(), Vec::new());
    for pair in on.windows(2) {
        let [(a, a_far, a_top), (b, b_far, b_top)] = [pair[0], pair[1]];
        // Facing each other: the first runs on from its road's top towards the second, and the second back towards it.
        if a_top < a_far as f64 && (b_far as f64) < b_top {
            cuts.push((a_far, b_far));
            pairs.push([a, b]);
        } else {
            away.push([a, b]);
        }
    }
    if cuts.is_empty() {
        return Err(away);
    }
    let mut vertices = Vec::with_capacity(made.vertices.len());
    let mut next = 0;
    for (from, to) in cuts {
        vertices.extend_from_slice(&made.vertices[next..=from]);
        next = to;
    }
    vertices.extend_from_slice(&made.vertices[next..]);
    Ok((Polyline { vertices, closed: made.closed }, pairs))
}

/// Why a berm or wall between the line below and a line redrawn by hand can't be built: your line's doing, so it's refused,
/// not taken as a road that can't go in.
fn refused_beside(step: usize, e: &GeomError) -> GeomError {
    GeomError::Refused(format!("step {}: between the line below and your line, {}", step + 1, e.to_string().replace("invalid input: ", "")))
}

/// A crest redrawn by hand, on the wall's target, for the wall below to re-form to: straight from its toe to your line.
/// Not yet where a road climbs the wall, whose cut is built with the wall.
fn redrawn_crest(drawn: &Polyline, toe: &Polyline, made: &Polyline, side: OffsetSide, target: &TriMesh, after: usize, roads: bool) -> Result<(Polyline, EditReport), GeomError> {
    if roads {
        return Err(GeomError::Refused(ROAD_ON_CREST.into()));
    }
    let crest = laid_on(drawn, made, Floor::Surface(target));
    stands_clear(&crest, toe, side)?;
    let batter = across(&crest, toe).map(|[.., flattest, steepest]| [flattest, steepest]);
    Ok((crest, EditReport { after, berm: None, batter, roads_off: Vec::new(), joined: Vec::new() }))
}

/// What a line redrawn by hand lies on where it leaves the line the steps made: a surface or grid, or for a berm that
/// keeps its crest's heights, that crest.
enum Floor<'a> {
    Surface(&'a TriMesh),
    Crest(&'a Polyline),
    None,
}

/// A line redrawn by hand on what it lies on. Where it runs on the line the steps made, it takes that line's heights, vertex
/// for vertex, so a line copied as the steps made it comes back as it was, held off a grid where the steps held it. Where
/// it leaves it, each vertex lies on what the line lies on, and on a surface or grid a stretch between them follows it,
/// with a vertex wherever it bends, as the lines the steps lay on a floor do. A vertex with nothing under it keeps its
/// height.
fn laid_on(drawn: &Polyline, made: &Polyline, floor: Floor<'_>) -> Polyline {
    let on_made = |p: [f64; 3]| {
        let n = made.vertices.len();
        (0..segment_count(made))
            .map(|i| {
                let (a, b) = (made.vertices[i], made.vertices[(i + 1) % n]);
                let (t, gap) = nearest_along(a, b, p);
                (gap, a[2] + t * (b[2] - a[2]))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .filter(|(gap, _)| *gap <= WELD)
            .map(|(_, z)| z)
    };
    let held: Vec<Option<f64>> = drawn.vertices.iter().map(|p| on_made(*p)).collect();
    let surface = match floor {
        Floor::Surface(surface) => surface,
        Floor::Crest(crest) => {
            let floored = at_crest_heights(drawn, crest);
            return Polyline { vertices: drawn.vertices.iter().zip(&floored.vertices).zip(&held).map(|((p, q), z)| [p[0], p[1], z.unwrap_or(q[2])]).collect(), closed: drawn.closed };
        }
        Floor::None => return Polyline { vertices: drawn.vertices.iter().zip(&held).map(|(p, z)| [p[0], p[1], z.unwrap_or(p[2])]).collect(), closed: drawn.closed },
    };
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &drawn.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let at = |p: [f64; 3], z: Option<f64>| [p[0], p[1], z.or_else(|| bins.height(surface, [p[0], p[1]])).unwrap_or(p[2])];
    let n = drawn.vertices.len();
    let mut vertices = Vec::with_capacity(n);
    for k in 0..n {
        let p = at(drawn.vertices[k], held[k]);
        vertices.push(p);
        let Some(j) = (k + 1 < n).then_some(k + 1).or(drawn.closed.then_some(0)) else { continue };
        // A stretch of yours off the steps' line follows the floor between its ends, bending where the floor does.
        if held[k].is_some() && held[j].is_some() {
            continue;
        }
        let q = at(drawn.vertices[j], held[j]);
        let Ok((draped, _)) = drape_string(&Polyline { vertices: vec![p, q], closed: false }, surface, true) else { continue };
        // Only where it bends: a vertex in line with the last kept and the next, in 3D, adds nothing.
        let inner = draped.vertices.len().saturating_sub(1);
        let mut last = p;
        for (i, r) in draped.vertices.iter().enumerate().take(inner).skip(1) {
            let next = if i + 1 == inner { q } else { draped.vertices[i + 1] };
            if !in_line(last, *r, next) {
                vertices.push(*r);
                last = *r;
            }
        }
    }
    Polyline { vertices, closed: drawn.closed }
}

/// Whether `r` lies on the straight line from `a` to `b`, in 3D, to within the weld.
fn in_line(a: [f64; 3], r: [f64; 3], b: [f64; 3]) -> bool {
    let (ab, ar) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [r[0] - a[0], r[1] - a[1], r[2] - a[2]]);
    let across = [ar[1] * ab[2] - ar[2] * ab[1], ar[2] * ab[0] - ar[0] * ab[2], ar[0] * ab[1] - ar[1] * ab[0]];
    let length = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).sqrt();
    length > 0.0 && (across[0] * across[0] + across[1] * across[1] + across[2] * across[2]).sqrt() / length <= WELD
}

/// Refuses a line redrawn by hand that crosses the line below it, or stands past it on the pit side by more than
/// `PIT_SIDE`: measured square to the line below from your line's vertices and the middles of its stretches, as a line
/// drawn end to end over the line below has its vertices only at its ends. Touching it, as a berm's line does at a road's
/// top, is fine.
fn stands_clear(line: &Polyline, below: &Polyline, side: OffsetSide) -> Result<(), GeomError> {
    let (n, m) = (line.vertices.len(), below.vertices.len());
    for i in 0..segment_count(line) {
        let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
        for j in 0..segment_count(below) {
            let (p, q) = (below.vertices[j], below.vertices[(j + 1) % m]);
            if let Some(at) = crossing_point(a, b, p, q) {
                return Err(GeomError::Refused(format!("your line crosses the line below it at {:.1}, {:.1}", at[0], at[1])));
            }
        }
    }
    let rising = if side == OffsetSide::Left { 1.0 } else { -1.0 };
    let with_halves = |l: &Polyline| -> Vec<[f64; 3]> {
        let n = l.vertices.len();
        let halves: Vec<[f64; 3]> = (0..segment_count(l))
            .map(|i| {
                let (a, b) = (l.vertices[i], l.vertices[(i + 1) % n]);
                [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]
            })
            .collect();
        l.vertices.iter().copied().chain(halves).collect()
    };
    let past = with_halves(line).into_iter().filter_map(|p| square_to(below, p).filter(|(_, _, way)| way * rising < 0.0).map(|(gap, ..)| (gap, p)));
    match past.max_by(|a, b| a.0.total_cmp(&b.0)) {
        Some((past, at)) if past > PIT_SIDE => Err(GeomError::Refused(format!("your line comes out {past:.2} m past the line below it, on the pit side, at {:.1}, {:.1}", at[0], at[1]))),
        _ => Ok(()),
    }
}

/// Where a point stands from a line, square to it in plan: how far, the line's height there, and which side, 1 on its left
/// and -1 on its right. None where the nearest place on it is an open end, past which nothing is square to it.
fn square_to(line: &Polyline, p: [f64; 3]) -> Option<(f64, f64, f64)> {
    let n = line.vertices.len();
    let segments = segment_count(line);
    let (i, t, gap) = (0..segments)
        .map(|i| {
            let (t, gap) = nearest_along(line.vertices[i], line.vertices[(i + 1) % n], p);
            (i, t, gap)
        })
        .min_by(|x, y| x.2.total_cmp(&y.2))?;
    if !line.closed && ((i == 0 && t <= 0.0) || (i + 1 == segments && t >= 1.0)) {
        return None;
    }
    let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
    let way = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    Some((gap, a[2] + t * (b[2] - a[2]), way.signum()))
}

/// Across from the line below to a line redrawn by hand, square to one or the other at each vertex of either that stands
/// off the other: the narrowest and widest in plan (m), then the flattest and steepest rise over that (degrees). None
/// where none do.
fn across(line: &Polyline, below: &Polyline) -> Option<[f64; 4]> {
    let up = line.vertices.iter().filter_map(|p| square_to(below, *p).map(|(gap, height, _)| (gap, p[2] - height)));
    let down = below.vertices.iter().filter_map(|p| square_to(line, *p).map(|(gap, height, _)| (gap, height - p[2])));
    let mut found: Option<[f64; 4]> = None;
    for (gap, rise) in up.chain(down).filter(|(gap, _)| *gap > WELD) {
        let angle = rise.atan2(gap).to_degrees();
        found = Some(match found {
            None => [gap, gap, angle, angle],
            Some([narrowest, widest, flattest, steepest]) => [narrowest.min(gap), widest.max(gap), flattest.min(angle), steepest.max(angle)],
        });
    }
    found
}

/// How far a point is from a line in plan (m).
fn gap_to(line: &Polyline, p: [f64; 3]) -> f64 {
    let n = line.vertices.len();
    (0..segment_count(line)).map(|i| nearest_along(line.vertices[i], line.vertices[(i + 1) % n], p).1).fold(f64::INFINITY, f64::min)
}

/// The stretches of a wall above a road added to the plain wall's, which share any before the road.
fn merge_stretches(stretches: &mut Vec<Stretch>, more: &[Stretch]) {
    for stretch in more {
        if !stretches.contains(stretch) {
            stretches.push(*stretch);
        }
    }
}

/// Whether a stretch runs across the top end of a road on its wall. A road comes out level with the crest, so
/// there's no wall there, and that's where it was meant to finish, not a gap: rounding decides whether the
/// walk sees it at all.
fn across_a_road_end(stretch: &Stretch, roads: &[(usize, Polyline, Polyline)]) -> bool {
    const SAME_POINT: f64 = 1e-6;
    roads.iter().any(|(_, pit_edge, wall_edge)| {
        let ends = [pit_edge.vertices.last(), wall_edge.vertices.last()];
        [stretch.from, stretch.to].iter().all(|p| ends.iter().flatten().any(|end| (p[0] - end[0]).hypot(p[1] - end[1]) < SAME_POINT))
    })
}

/// The berm waiting on a crest, now the line it runs out to is known.
fn finish_berm(berm_from: &mut Option<Polyline>, line: &Polyline, face: &mut FaceParts) -> Result<(), GeomError> {
    if let Some(crest) = berm_from.take() {
        face.meshes.push(band(&crest, line)?);
    }
    Ok(())
}

/// A road that turned back on the berm below a wall: its wall-side top on the crest before the turn, and the flat
/// stretch it runs before it climbs, along the line from where it came onto it to where it climbs, and its wall side from
/// its corner past the turn to where the climb's wall-side edge starts, each in the road's own direction.
struct Pad {
    top: [f64; 3],
    pit: Vec<[f64; 3]>,
    wall: Vec<[f64; 3]>,
}

/// The berm waiting on a crest, out to the toe of the wall above, where roads turned back on it (decision 0038). A flat
/// stretch is a road, level across from the line to its wall side, so it's a piece of its own, and the berm runs out to
/// the line beside it: from the last road's wall-side top straight across to where the next came onto the line, then
/// along the line. Past the turn it runs out to the toe as it does everywhere else. A pad that isn't on the crest and the
/// toe as they are leaves the berm one band out to the toe there, as it was before.
fn finish_berm_round_pads(berm_from: &mut Option<Polyline>, toe: &Polyline, pads: &[Pad], face: &mut FaceParts) -> Result<(), GeomError> {
    let Some(crest) = berm_from.take() else { return Ok(()) };
    let at = |line: &[[f64; 3]], p: &[f64; 3]| line.iter().position(|q| (p[0] - q[0]).hypot(p[1] - q[1]) <= WELD);
    let (c, t) = (&crest.vertices, &toe.vertices);
    // Each pad by where it is on the crest and the toe: the top, the flat stretch's corner, where the climb's wall-side
    // edge starts and where its pit side starts.
    let mut placed: Vec<(usize, usize, usize, usize, &Pad)> = pads
        .iter()
        .filter_map(|pad| Some((at(c, &pad.top)?, at(t, pad.wall.first()?)?, at(t, pad.wall.last()?)?, at(t, pad.pit.last()?)?, pad)))
        .filter(|&(_, corner, wall_start, start, _)| (corner <= wall_start && wall_start < start) || (start < wall_start && wall_start <= corner))
        .collect();
    if placed.is_empty() {
        face.meshes.push(band(&crest, toe)?);
        return Ok(());
    }
    placed.sort_by_key(|&(_, corner, ..)| corner);
    let piece = |inner: &[[f64; 3]], outer: &[[f64; 3]]| band(&Polyline { vertices: inner.to_vec(), closed: false }, &Polyline { vertices: outer.to_vec(), closed: false });
    let (mut inner, mut outer): (Vec<[f64; 3]>, Vec<[f64; 3]>) = (Vec::new(), Vec::new());
    let (mut ci, mut ti) = (0, 0);
    for (top, corner, wall_start, start, pad) in placed {
        let onto = pad.pit[0];
        if top < ci || corner.min(start) < ti {
            continue;
        }
        if corner <= wall_start {
            // The road runs the way the string does: the berm before it ends straight across the pad from the top to
            // where the road came onto the line, and from there out to the flat stretch's corner past the turn.
            inner.extend(&c[ci..=top]);
            inner.push(onto);
            outer.extend(&t[ti..=corner]);
            face.meshes.push(piece(&inner, &outer)?);
            face.meshes.push(piece(&pad.pit, &t[corner..=wall_start])?);
            (inner, outer) = (Vec::new(), pad.pit.clone());
            (ci, ti) = (top, start + 1);
        } else {
            // It runs back against the string: the berm before it runs out to the line up to the flat stretch, and the
            // one after it starts across the pad from where the road came onto the line.
            inner.extend(&c[ci..=top]);
            outer.extend(&t[ti..start]);
            outer.extend(pad.pit.iter().rev());
            face.meshes.push(piece(&inner, &outer)?);
            let pit: Vec<[f64; 3]> = pad.pit.iter().rev().copied().collect();
            face.meshes.push(piece(&pit, &t[wall_start..=corner])?);
            (inner, outer) = (vec![onto], Vec::new());
            (ci, ti) = (top, corner);
        }
    }
    inner.extend(&c[ci..]);
    outer.extend(&t[ti..]);
    face.meshes.push(piece(&inner, &outer)?);
    Ok(())
}

/// A toe with a vertex wherever its crest touches it, so the berm below meets the wall and the crest above
/// there with no slit between them.
pub(crate) fn with_touches(toe: &Polyline, crest: &Polyline) -> Polyline {
    with_crest_vertices(toe, crest, |vertex, on, gap| (gap <= WELD && (vertex[2] - on[2]).abs() <= WELD).then_some(vertex))
}

/// The batter a level sheet for a berm is sized for, flat enough to reach past any berm.
const BERM_BATTER: f64 = 5.0;

/// How close two points of a face are to be the same point (m).
const WELD: f64 = 1e-6;

/// A cell of the weld's lattice and those touching it.
fn cells_round([x, y, z]: [i64; 3]) -> impl Iterator<Item = [i64; 3]> {
    (-1..=1).flat_map(move |i| (-1..=1).flat_map(move |j| (-1..=1).map(move |k| [x + i, y + j, z + k])))
}

/// What a line lies on or a wall rises to.
enum Resting<'a> {
    Surface(&'a TriMesh),
    Level(TriMesh),
}

impl<'a> Resting<'a> {
    /// A target, for a wall from `from` at a batter.
    fn of(target: FaceTarget<'a>, from: &Polyline, batter: f64) -> Self {
        match target {
            FaceTarget::Surface(mesh) => Resting::Surface(mesh),
            FaceTarget::Rl(rl) => Resting::Level(level_sheet(from, rl, sheet_reach(from, rl, batter))),
        }
    }

    fn mesh(&self) -> &TriMesh {
        match self {
            Resting::Surface(mesh) => mesh,
            Resting::Level(sheet) => sheet,
        }
    }
}

fn wall_report(step: usize, wall: &crate::WallStep) -> StepReport {
    StepReport {
        walls: wall.log.walls,
        squeezed_out: wall.log.squeezed_out,
        no_wall: false,
        no_height: wall.log.no_height.clone(),
        no_target: wall.log.no_target.clone(),
        ..no_wall_report(step)
    }
}

fn no_wall_report(step: usize) -> StepReport {
    StepReport { step, walls: 0, squeezed_out: 0, no_wall: true, no_height: Vec::new(), no_target: Vec::new(), off_surface: 0, pieces_dropped: 0, no_berm: Vec::new() }
}

fn longest(toes: Vec<Polyline>) -> Polyline {
    toes.into_iter().max_by(|a, b| a.plan_length().total_cmp(&b.plan_length())).expect("next_toe gives at least one string")
}

/// A crest that ran along its string where the target is below it stays up there: a toe beside that
/// stretch keeps the crest's elevation instead of dropping onto the target. Round the top of a road that turns back on
/// this berm, one of `pads`, the crest is up only because the road's top is level across, so the toe there lies on the
/// target, and the pad on it with it, on the seam floor where the target is one (decision 0038).
fn hold_above_the_target(toe: &mut Polyline, crest: &Polyline, surface: &TriMesh, pads: &[RoadTop]) {
    if surface.triangles.is_empty() {
        return;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in crest.vertices.iter().chain(&toe.vertices) {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let n = crest.vertices.len();
    let above = |i: usize| bins.height(surface, [crest.vertices[i][0], crest.vertices[i][1]]).is_some_and(|z| crest.vertices[i][2] > z + 1e-6);
    // The crest each road's top holds up: from its top edge out to where the crest comes back down onto the target.
    let lifted: Vec<[f64; 2]> = pads
        .iter()
        .filter_map(|top| {
            let (a, b) = (place_on(crest, top.pit)?, place_on(crest, top.wall)?);
            let (mut first, mut last) = (a.min(b).floor() as usize, (a.max(b).ceil() as usize).min(n - 1));
            while first > 0 && above(first) {
                first -= 1;
            }
            while last + 1 < n && above(last) {
                last += 1;
            }
            Some([first as f64, last as f64])
        })
        .collect();
    for p in &mut toe.vertices {
        let along = |i: usize| nearest_along(crest.vertices[i], crest.vertices[(i + 1) % n], *p);
        let Some(i) = (0..segment_count(crest)).min_by(|a, b| along(*a).1.total_cmp(&along(*b).1)) else { continue };
        if lifted.iter().any(|stretch| (stretch[0]..=stretch[1]).contains(&(i as f64 + along(i).0))) {
            continue;
        }
        let (a, b, t) = (crest.vertices[i], crest.vertices[(i + 1) % n], along(i).0);
        let on = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
        if bins.height(surface, [on[0], on[1]]).is_some_and(|z| on[2] > z + 1e-6) {
            p[2] = p[2].max(on[2]);
        }
    }
}

/// The sheet between two strings running the same way, wound upward: the berm between a crest and the
/// next toe. Its outline is triangulated in plan, so it can't reach across either line. A closed pair
/// goes round as one outline, joined by a slit from first vertex to first vertex.
pub(crate) fn band(a: &Polyline, b: &Polyline) -> Result<TriMesh, GeomError> {
    if a.vertices.len() < 2 || b.vertices.len() < 2 || a.vertices == b.vertices {
        return Ok(TriMesh::default());
    }
    let along: Vec<Option<f64>> = b.vertices.iter().map(|p| place_on(a, *p)).collect();
    if (0..b.vertices.len() - 1).all(|i| runs_along(a, &along, i, b)) && !b.closed {
        return Ok(TriMesh::default());
    }
    if along.iter().all(Option::is_none) {
        return whole_band(a, b);
    }
    match along.iter().position(Option::is_some) {
        // A closed pair opens where they touch, so each stretch apart has two ends.
        Some(first) if a.closed => {
            let start = along[first].expect("it's on the line").round() as usize % a.vertices.len();
            band_where_apart(&opened(a, start), &opened(b, first))
        }
        _ => band_where_apart(a, b),
    }
}

/// Whether a segment of `b` runs along `a`. Both its ends have to lie on `a`, and all of `a` between them on the
/// segment: an end on either side of a corner in `a` leaves a piece of berm between the two lines.
fn runs_along(a: &Polyline, along: &[Option<f64>], i: usize, b: &Polyline) -> bool {
    let (Some(from), Some(to)) = (along[i], along[i + 1]) else { return false };
    let (from, to) = (from.min(to), from.max(to));
    let n = a.vertices.len();
    (from.floor() as usize + 1..=to.ceil() as usize - usize::from(to.ceil() > 0.0)).filter(|k| (*k as f64) > from && (*k as f64) < to).all(|k| nearest_along(b.vertices[i], b.vertices[i + 1], a.vertices[k % n]).1 <= WELD)
}

/// How far along a line a point on it lies, as a segment number and a fraction, if it's on it.
pub(crate) fn place_on(line: &Polyline, p: [f64; 3]) -> Option<f64> {
    let n = line.vertices.len();
    let segments = if line.closed { n } else { n - 1 };
    (0..segments).find_map(|i| {
        let (a, b) = (line.vertices[i], line.vertices[(i + 1) % n]);
        let (t, gap) = nearest_along(a, b, p);
        (gap <= WELD && (a[2] + t * (b[2] - a[2]) - p[2]).abs() <= WELD).then_some(i as f64 + t)
    })
}

/// A closed line as an open one, from one vertex round to the same vertex.
fn opened(line: &Polyline, start: usize) -> Polyline {
    let n = line.vertices.len();
    Polyline { vertices: (0..=n).map(|k| line.vertices[(start + k) % n]).collect(), closed: false }
}

/// The part of an open line between two places along it.
pub(crate) fn stretch_of(line: &Polyline, from: f64, to: f64) -> Polyline {
    let at = |place: f64| {
        let i = (place.floor() as usize).min(line.vertices.len() - 2);
        let (a, b, t) = (line.vertices[i], line.vertices[i + 1], place - i as f64);
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]
    };
    let mut vertices = vec![at(from)];
    vertices.extend((from.floor() as usize + 1..=to.ceil() as usize - 1).map(|i| line.vertices[i]));
    vertices.push(at(to));
    vertices.dedup_by(|p, q| (p[0] - q[0]).hypot(p[1] - q[1]) <= WELD);
    Polyline { vertices, closed: false }
}

/// A band for each stretch where two open lines stand apart. Where they share a stretch there's no berm.
fn band_where_apart(a: &Polyline, b: &Polyline) -> Result<TriMesh, GeomError> {
    let along: Vec<Option<f64>> = b.vertices.iter().map(|p| place_on(a, *p)).collect();
    let last = b.vertices.len() - 1;
    let mut mesh = TriMesh::default();
    let mut i = 0;
    while i < last {
        if runs_along(a, &along, i, b) {
            i += 1;
            continue;
        }
        let end = (i + 1..=last).find(|&j| along[j].is_some()).unwrap_or(last);
        let reach = [along[i].unwrap_or(0.0), along[end].unwrap_or((a.vertices.len() - 1) as f64)];
        if reach[0] >= reach[1] {
            i = end;
            continue;
        }
        let piece = whole_band(&stretch_of(a, reach[0], reach[1]), &Polyline { vertices: b.vertices[i..=end].to_vec(), closed: false })?;
        let offset = mesh.vertices.len() as u32;
        mesh.vertices.extend(piece.vertices);
        mesh.triangles.extend(piece.triangles.iter().map(|t| t.map(|v| v + offset)));
        i = end;
    }
    Ok(mesh)
}

/// The berm between two lines that stand apart all along, or touch only at their ends.
fn whole_band(a: &Polyline, b: &Polyline) -> Result<TriMesh, GeomError> {
    let mut outline: Vec<[f64; 3]> = a.vertices.clone();
    if a.closed {
        outline.push(a.vertices[0]);
        outline.push(b.vertices[0]);
    }
    outline.extend(b.vertices.iter().rev());
    if !a.closed {
        // Stretches between places the lines touch start and end on one point.
        outline.dedup_by(|p, q| (p[0] - q[0]).hypot(p[1] - q[1]) <= WELD);
        if outline.len() > 1 && (outline[0][0] - outline[outline.len() - 1][0]).hypot(outline[0][1] - outline[outline.len() - 1][1]) <= WELD {
            outline.pop();
        }
    }
    if a.closed {
        outline.pop();
        outline.insert(a.vertices.len() + 2, b.vertices[0]);
        outline.truncate(a.vertices.len() + 2 + b.vertices.len());
    }
    let about = outline[0];
    let local: Vec<[f64; 3]> = outline.iter().map(|p| [p[0] - about[0], p[1] - about[1], p[2] - about[2]]).collect();
    let triangles = std::panic::catch_unwind(|| triangles_of(&local)).ok().flatten();
    let triangles = triangles.ok_or_else(|| {
        let at = first_fold(&outline).unwrap_or([about[0], about[1]]);
        GeomError::InvalidInput(format!("the berm near {:.3}, {:.3} folds over itself", at[0], at[1]))
    })?;
    Ok(TriMesh::new(outline, triangles.into_iter().map(|t| t.map(|v| v as u32)).collect()))
}

/// Where a closed outline first crosses itself in plan, strictly inside two of its stretches: where a berm folds.
fn first_fold(outline: &[[f64; 3]]) -> Option<[f64; 2]> {
    let n = outline.len();
    let stretch = |i: usize| (outline[i], outline[(i + 1) % n]);
    (0..n).find_map(|i| {
        let (a, b) = stretch(i);
        (i + 2..n).filter(|&j| !(i == 0 && j == n - 1)).find_map(|j| {
            let (p, q) = stretch(j);
            crossing_point(a, b, p, q)
        })
    })
}

/// Where stretch `a`-`b` crosses `p`-`q` in plan, strictly inside both: not where they only touch.
fn crossing_point(a: [f64; 3], b: [f64; 3], p: [f64; 3], q: [f64; 3]) -> Option<[f64; 2]> {
    if a[0].max(b[0]) < p[0].min(q[0]) || p[0].max(q[0]) < a[0].min(b[0]) || a[1].max(b[1]) < p[1].min(q[1]) || p[1].max(q[1]) < a[1].min(b[1]) {
        return None;
    }
    let (r, s) = ([b[0] - a[0], b[1] - a[1]], [q[0] - p[0], q[1] - p[1]]);
    let across = r[0] * s[1] - r[1] * s[0];
    if across.abs() < 1e-12 {
        return None;
    }
    let ap = [p[0] - a[0], p[1] - a[1]];
    let (t, u) = ((ap[0] * s[1] - ap[1] * s[0]) / across, (ap[0] * r[1] - ap[1] * r[0]) / across);
    (t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9).then(|| [a[0] + t * r[0], a[1] + t * r[1]])
}

fn mirrored_line(line: &Polyline) -> Polyline {
    Polyline { vertices: line.vertices.iter().map(|p| [p[0], p[1], -p[2]]).collect(), closed: line.closed }
}

fn mirrored_mesh(mesh: &TriMesh) -> TriMesh {
    TriMesh::new(mesh.vertices.iter().map(|p| [p[0], p[1], -p[2]]).collect(), mesh.triangles.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn surface(height: impl Fn(f64, f64) -> f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| height(x - E, y - N)))
    }

    fn line(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [p[0] + E, p[1] + N, p[2]]).collect(), closed: false }
    }

    /// A line redrawn by hand lies on its floor (decision 0046). On a floor rising 1 in 10 to a ridge at x = 200 and falling
    /// beyond, the steps' line along y = 100 at the floor's heights, and yours moved out to y = 155 from x = 100 to 300:
    /// where yours runs on the steps' line it keeps that line's vertices and heights, held 2 m off the floor at x = 50, and
    /// its stretch across the ridge gets a vertex on the ridge at the floor's height, 30 m, to 1 µm, and none elsewhere.
    #[test]
    fn a_line_redrawn_by_hand_follows_its_floor_where_it_leaves_the_steps_line() {
        let floor = surface(|x, _| 20.0 - (x - 200.0).abs() / 10.0 + 10.0);
        let made = line(&[[0.0, 100.0, 12.0], [50.0, 100.0, 17.0], [100.0, 100.0, 20.0], [300.0, 100.0, 20.0], [400.0, 100.0, 10.0]]);
        let drawn = line(&[[0.0, 100.0, 0.0], [50.0, 100.0, 0.0], [100.0, 100.0, 0.0], [100.0, 155.0, 0.0], [300.0, 155.0, 0.0], [300.0, 100.0, 0.0], [400.0, 100.0, 0.0]]);
        let laid = laid_on(&drawn, &made, Floor::Surface(&floor));
        let heights: Vec<[f64; 3]> = laid.vertices.iter().map(|p| [p[0] - E, p[1] - N, p[2]]).collect();
        let expected = [[0.0, 100.0, 12.0], [50.0, 100.0, 17.0], [100.0, 100.0, 20.0], [100.0, 155.0, 20.0], [200.0, 155.0, 30.0], [300.0, 155.0, 20.0], [300.0, 100.0, 20.0], [400.0, 100.0, 10.0]];
        assert_eq!(heights.len(), expected.len(), "{heights:?}");
        for (p, q) in heights.iter().zip(&expected) {
            assert!((0..3).all(|k| (p[k] - q[k]).abs() < 1e-6), "{p:?} against {q:?}");
        }
    }

    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} is not {b} to {tolerance}");
    }

    /// What's left of a line's plan shape simplified to a centimetre.
    fn plan_corners(line: &Polyline) -> usize {
        conditioned_toe(line, 0.01).unwrap().vertices.len()
    }

    /// A wall and a berm as the table starts them, conditioned at 0.1 m.
    fn wall<'a>(to: FaceTarget<'a>, batter: f64) -> FaceStep<'a> {
        FaceStep::Project { to, batter: batter.into(), condition: Some(0.1), ramps: &[], turns: &[] }
    }

    fn berm<'a>(width: f64) -> FaceStep<'a> {
        FaceStep::Berm { on: None, width: width.into(), register: true, condition: Some(0.1), min_bench: None, join: JoinBoxes::Auto, to_come: None }
    }

    /// Tim's sequence on level surfaces, where every line has a known place: the toe on Grid A at RL 100,
    /// to Grid B at 120 and a berm, to Grid C at 140 and a berm, to the topography at 160, on to RL 200.
    #[test]
    fn a_whole_face_runs_up_with_its_berms_where_they_belong() {
        let (a, b, c, topo) = (surface(|_, _| 100.0), surface(|_, _| 120.0), surface(|_, _| 140.0), surface(|_, _| 160.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&b), 45.0), berm(5.0), wall(FaceTarget::Surface(&c), 45.0), berm(5.0), wall(FaceTarget::Surface(&topo), 45.0), wall(FaceTarget::Rl(200.0), 45.0)];
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap();

        let crests: Vec<(f64, f64)> = face.benches.iter().map(|b| (b.crest.vertices[0][1] - N, b.crest.vertices[0][2])).collect();
        assert_eq!(crests.len(), 4);
        for ((y, z), (want_y, want_z)) in crests.iter().zip([(120.0, 120.0), (145.0, 140.0), (170.0, 160.0), (210.0, 200.0)]) {
            close(*y, want_y, 1e-6);
            close(*z, want_z, 1e-6);
        }
        // The walls' slope lengths and two berms, over the 300 m toe.
        let walls = (20.0 + 20.0 + 20.0 + 40.0) * 2f64.sqrt();
        close(face.surface.area(), 300.0 * (walls + 5.0 + 5.0), 1e-3);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
        assert!(face.surface.triangles.iter().all(|&t| face.surface.triangle_cross(t)[2] > 0.0), "every triangle faces up");
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "one sheet, with one edge round it");
    }

    #[test]
    fn going_down_carries_on_the_same_plane_below_the_toe() {
        let a = surface(|x, _| 100.0 + 0.05 * x);
        let toe = register_toe(&line(&[[50.0, 200.0, 0.0], [350.0, 200.0, 0.0]]), &a).unwrap().0;
        let up = project_face(&toe, &a, OffsetSide::Right, FaceDirection::Up, &[wall(FaceTarget::Rl(180.0), 60.0)], &[]).unwrap();
        let down = project_face(&toe, &a, OffsetSide::Right, FaceDirection::Down, &[wall(FaceTarget::Rl(20.0), 60.0)], &[]).unwrap();
        // Right of an eastward string is south, so up goes south and down goes north, one plane through the toe.
        let k = 60f64.to_radians().tan();
        let on_the_plane = |p: &[f64; 3]| close(p[2], 100.0 + 0.05 * (p[0] - E) + k * (200.0 - (p[1] - N)), 1e-6);
        up.benches[0].crest.vertices.iter().for_each(on_the_plane);
        down.benches[0].crest.vertices.iter().for_each(on_the_plane);
        assert!(up.benches[0].crest.vertices.iter().all(|p| p[1] - N < 200.0));
        assert!(down.benches[0].crest.vertices.iter().all(|p| p[1] - N > 200.0));
        assert!(down.surface.triangles.iter().all(|&t| down.surface.triangle_cross(t)[2] > 0.0), "going down still faces up");
    }

    /// Down from a level toe through a level grid and a berm on it, to an RL.
    #[test]
    fn going_down_steps_through_a_grid_and_a_berm() {
        let (a, b) = (surface(|_, _| 100.0), surface(|_, _| 80.0));
        let toe = line(&[[50.0, 200.0, 100.0], [350.0, 200.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&b), 45.0), berm(5.0), wall(FaceTarget::Rl(50.0), 45.0)];
        // Left of an eastward string is north, so a wall rising left goes down to the south.
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Down, &steps, &[]).unwrap();
        close(face.benches[0].crest.vertices[0][1] - N, 180.0, 1e-6);
        close(face.benches[0].crest.vertices[0][2], 80.0, 1e-6);
        close(face.benches[1].crest.vertices[0][1] - N, 145.0, 1e-6);
        close(face.benches[1].crest.vertices[0][2], 50.0, 1e-6);
        close(face.surface.area(), 300.0 * ((20.0 + 30.0) * 2f64.sqrt() + 5.0), 1e-3);
    }

    /// A face ending on a berm gives the berm's line, where the next wall would start (a step design shows it). Up from
    /// a level toe at RL 100, a 45 degree wall to RL 120 and a 10 m berm: the line is 30 m out at RL 120. Down from the
    /// same toe to RL 80, the same the other way. A face ending on a wall has none.
    #[test]
    fn a_face_ending_on_a_berm_gives_the_berms_line() {
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 200.0, 100.0], [350.0, 200.0, 100.0]]);
        for (direction, rl, y) in [(FaceDirection::Up, 120.0, 230.0), (FaceDirection::Down, 80.0, 170.0)] {
            let face = project_face(&toe, &a, OffsetSide::Left, direction, &[wall(FaceTarget::Rl(rl), 45.0), berm(10.0)], &[]).unwrap();
            let berm_line = face.berm_line.unwrap_or_else(|| panic!("{direction:?}: no berm line"));
            for p in &berm_line.vertices {
                close(p[1] - N, y, 1e-6);
                close(p[2], rl, 1e-6);
            }
            close(berm_line.vertices[0][0] - E, 50.0, 1e-6);
            close(berm_line.vertices.last().unwrap()[0] - E, 350.0, 1e-6);
            let walled = project_face(&toe, &a, OffsetSide::Left, direction, &[wall(FaceTarget::Rl(rl), 45.0)], &[]).unwrap();
            assert!(walled.berm_line.is_none());
        }
    }

    #[test]
    fn a_step_with_no_height_passes_to_the_next_one() {
        let (a, lower, top) = (surface(|_, _| 100.0), surface(|_, _| 90.0), surface(|_, _| 130.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&lower), 45.0), wall(FaceTarget::Surface(&top), 45.0)], &[]).unwrap();
        assert!(face.steps[0].no_wall);
        assert_eq!(face.benches.len(), 1);
        close(face.benches[0].crest.vertices[0][1] - N, 130.0, 1e-6);
    }

    #[test]
    fn a_berm_where_the_crest_ran_along_the_string_stays_at_the_string() {
        // The target is below the toe for the middle of the string.
        let a = surface(|_, _| 100.0);
        let dipping = surface(|x, _| 90.0 + 0.5 * (x - 200.0).abs());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&dipping), 45.0), berm(5.0)], &[]).unwrap();
        assert_eq!(face.steps[0].no_height.len(), 1);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
    }

    #[test]
    fn a_berm_over_a_hole_holds_the_height_of_its_crest() {
        // A kink in the toe puts a vertex of the berm line over the hole.
        let holed = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| {
            let (x, y) = (x - E, y - N);
            if (150.0..=250.0).contains(&x) && (60.0..=200.0).contains(&y) { f64::NAN } else { 120.0 }
        }));
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 100.0, 100.0], [200.0, 80.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&holed), 45.0), berm(5.0), wall(FaceTarget::Rl(140.0), 45.0)];
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap();
        assert!(face.steps.iter().any(|s| s.off_surface > 0), "{:?}", face.steps);
        for p in &face.benches[1].toe.vertices {
            close(p[2], 120.0, 1e-6);
        }
    }

    /// A berm with Register off keeps its crest's heights: on a target with a cross fall the line stays at
    /// the crest, and along a rolling target it follows the crest between its own two ends.
    #[test]
    fn a_berm_with_register_off_keeps_the_heights_of_its_crest() {
        let a = surface(|_, _| 100.0);
        let rolling = surface(|x, y| 120.0 + 0.2 * (y - 120.0) + 4.0 * (x / 41.0).sin());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = |register| [wall(FaceTarget::Surface(&rolling), 60.0), FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register, condition: Some(0.1), min_bench: None, join: JoinBoxes::Auto, to_come: None }, wall(FaceTarget::Rl(160.0), 60.0)];
        let held = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(false), &[]).unwrap();
        let crest = &held.benches[0].crest;
        let next = &held.benches[1].toe;
        let height_of = |line: &Polyline, x: f64| {
            let w = line.vertices.windows(2).find(|w| (w[0][0] - x) * (w[1][0] - x) <= 0.0).unwrap();
            w[0][2] + (x - w[0][0]) / (w[1][0] - w[0][0]) * (w[1][2] - w[0][2])
        };
        let (first, last) = (crest.vertices[0][0].max(next.vertices[0][0]), crest.vertices[crest.vertices.len() - 1][0].min(next.vertices[next.vertices.len() - 1][0]));
        for x in (0..=100).map(|k| first + (last - first) * f64::from(k) / 100.0) {
            // The crest runs off straight in plan only by its small swing, so the two read the same to 10 cm.
            close(height_of(next, x), height_of(crest, x), 0.1);
        }
        let registered = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(true), &[]).unwrap();
        // Registered, the same line sits 10 m further up the 1 in 5 cross fall, 2 m higher.
        let middle = E + 200.0;
        close(height_of(&registered.benches[1].toe, middle) - height_of(next, middle), 2.0, 0.1);
    }

    #[test]
    fn a_berm_can_name_the_surface_or_rl_it_registers_on() {
        let (a, b, c) = (surface(|_, _| 100.0), surface(|_, _| 120.0), surface(|_, _| 125.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        for (on, want) in [(FaceTarget::Surface(&c), 125.0), (FaceTarget::Rl(123.0), 123.0)] {
            let steps = [wall(FaceTarget::Surface(&b), 45.0), FaceStep::Berm { on: Some(on), width: EdgeRule::Fixed(5.0), register: true, condition: None, min_bench: None, join: JoinBoxes::Auto, to_come: None }, wall(FaceTarget::Rl(160.0), 45.0)];
            let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap();
            for p in &face.benches[1].toe.vertices {
                close(p[2], want, 1e-6);
                close(p[1] - N, 125.0, 1e-6);
            }
        }
    }

    /// A rolling target gives a crest that weaves in plan. Each row's tick decides what happens to it.
    #[test]
    fn each_row_says_whether_its_line_is_conditioned() {
        let a = surface(|_, _| 100.0);
        let rolling = surface(|x, y| 120.0 + 3.0 * (x / 29.0).sin() + 2.0 * (y / 17.0).cos());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let corners_of_next_toe = |crest: Option<f64>, after: Option<f64>| {
            let steps = [FaceStep::Project { to: FaceTarget::Surface(&rolling), batter: EdgeRule::Fixed(60.0), condition: crest, ramps: &[], turns: &[] }, FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: after, min_bench: None, join: JoinBoxes::Auto, to_come: None }, wall(FaceTarget::Rl(170.0), 60.0)];
            let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap();
            plan_corners(&face.benches[1].toe)
        };
        let (raw, cleaned, after_only) = (corners_of_next_toe(None, None), corners_of_next_toe(Some(0.5), None), corners_of_next_toe(None, Some(0.5)));
        assert!(cleaned * 3 < raw, "{cleaned} against {raw}");
        assert!(after_only * 3 < raw, "{after_only} against {raw}");
    }

    #[test]
    fn a_change_of_batter_with_conditioning_off_starts_on_the_crest() {
        let (a, b) = (surface(|_, _| 100.0), surface(|_, _| 120.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = |condition| [FaceStep::Project { to: FaceTarget::Surface(&b), batter: EdgeRule::Fixed(45.0), condition, ramps: &[], turns: &[] }, wall(FaceTarget::Rl(140.0), 60.0)];
        let walls = 20.0 * 2f64.sqrt() + 20.0 / 60f64.to_radians().sin();
        let on_the_crest = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(None), &[]).unwrap();
        close(on_the_crest.surface.area(), 300.0 * walls, 1e-3);
        let clear = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(Some(0.1)), &[]).unwrap();
        close(clear.surface.area(), 300.0 * (walls + 0.1), 1e-3);
    }

    #[test]
    fn conditioning_between_walls_of_one_batter_changes_nothing() {
        let (a, topo) = (surface(|_, _| 100.0), surface(|x, y| 130.0 + 3.0 * (x / 29.0).sin() + 2.0 * (y / 17.0).cos()));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let run = |condition| {
            let steps = [FaceStep::Project { to: FaceTarget::Surface(&topo), batter: EdgeRule::Fixed(70.0), condition, ramps: &[], turns: &[] }, wall(FaceTarget::Rl(200.0), 70.0)];
            project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap()
        };
        let (on, off) = (run(Some(1.0)), run(None));
        assert_eq!(on.surface.triangles.len(), off.surface.triangles.len());
        close(on.surface.area(), off.surface.area(), 1e-9);
        close(on.surface.area(), 300.0 * 100.0 / 70f64.to_radians().sin(), 1e-3);
    }

    #[test]
    fn conditioning_the_last_crest_cleans_the_string_and_leaves_the_sheet_alone() {
        let (a, rolling) = (surface(|_, _| 100.0), surface(|x, y| 130.0 + 3.0 * (x / 29.0).sin() + 2.0 * (y / 17.0).cos()));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let run = |condition| project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[FaceStep::Project { to: FaceTarget::Surface(&rolling), batter: EdgeRule::Fixed(70.0), condition, ramps: &[], turns: &[] }], &[]).unwrap();
        let (raw, clean) = (run(None), run(Some(1.0)));
        // The cleaned crest is laid back on the target with a vertex at every triangle, so it's its plan
        // shape that's simpler.
        let (rough, cleaned) = (plan_corners(&raw.benches[0].crest), plan_corners(&clean.benches[0].crest));
        assert!(cleaned * 2 < rough, "{cleaned} against {rough}");
        assert_eq!(raw.surface.triangles.len(), clean.surface.triangles.len());
        close(raw.surface.area(), clean.surface.area(), 1e-9);
    }

    #[test]
    fn a_conditioned_toe_keeps_its_ends_and_stays_within_the_tolerance() {
        let wavy = line(&[[0.0, 100.0, 9.0], [50.0, 100.05, 9.0], [100.0, 99.95, 9.0], [150.0, 100.2, 9.0], [200.0, 100.0, 9.0]]);
        let cleaned = conditioned_toe(&wavy, 0.1).unwrap();
        assert_eq!(cleaned.vertices.len(), 4, "the 5 cm wiggle goes and the larger two stay");
        assert_eq!((cleaned.vertices[0][0], cleaned.vertices[3][0]), (wavy.vertices[0][0], wavy.vertices[4][0]));
        assert_eq!(conditioned_toe(&wavy, 0.5).unwrap().vertices.len(), 2);
    }

    /// A wall up to a target that dips below its toe in the middle, 2 m at `centre`, with a berm either side
    /// of it, so the sheet carries on over the stretch with no height. The last wall is steeper because a
    /// target with no cross-fall puts a line moved square off its crest in the plane of the wall below, and
    /// at that same batter neighbouring walls are one plane, with no hip between them. That's refused.
    fn over_a_dip(centre: f64) -> Face {
        let (a, b) = (surface(|_, _| 100.0), surface(|_, _| 120.0));
        let dipping = surface(|x, _| 118.0 + 0.025 * (x - centre).abs());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&b), 45.0), berm(5.0), wall(FaceTarget::Surface(&dipping), 45.0), berm(5.0), wall(FaceTarget::Rl(150.0), 60.0)];
        project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap()
    }

    fn assert_one_sheet_over_the_dip(face: &Face, centre: f64) {
        assert_eq!(face.steps[2].no_height.len(), 1, "{:?}", face.steps[2]);
        let stretch = face.steps[2].no_height[0];
        close(stretch.from[0].min(stretch.to[0]) - E, centre - 80.0, 1e-6);
        close(stretch.from[0].max(stretch.to[0]) - E, centre + 80.0, 1e-6);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "one sheet, with one edge round it");
    }

    /// The crest comes down onto its toe between two of the toe's vertices, and rises again further on.
    #[test]
    fn a_crest_touching_its_toe_part_way_along_a_segment_leaves_no_slit() {
        assert_one_sheet_over_the_dip(&over_a_dip(203.7), 203.7);
    }

    /// The target meets the toe on grid lines, where the toe has its vertices, as grids that pinch together do.
    #[test]
    fn a_crest_landing_exactly_on_a_string_vertex_carries_on() {
        assert_one_sheet_over_the_dip(&over_a_dip(200.0), 200.0);
    }

    /// Two triangles that share an edge, worked out a whisker apart either side of a rounding step.
    /// The crest and the line are each simplified by 0.1 m, so a stretch with no berm has to stand off by
    /// the two together or they can cross. This one's a wall wandering round the 5 m minimum bench.
    #[test]
    fn a_berm_left_out_stands_clear_of_both_tolerances() {
        let toe = Polyline { vertices: vec![[E, N, 100.0], [E + 600.0, N, 100.0]], closed: false };
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 20, 20, |_, _| 100.0));
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| 104.0 + 0.03 * (x - E - 80.0) + 2.0 * ((x - E) / 30.0).sin()));
        let steps = [
            FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramps: &[], turns: &[] },
            FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: Some(5.0), join: JoinBoxes::Auto, to_come: None },
            FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(70.0), condition: Some(0.1), ramps: &[], turns: &[] },
        ];
        let face = project_face(&toe, &floor, OffsetSide::Left, FaceDirection::Up, &steps, &[]).unwrap();
        assert_eq!(crate::find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
    }

    #[test]
    fn a_face_welds_across_a_rounding_step() {
        let step = 2.5e-6;
        let (below, above) = (step - 1e-12, step + 1e-12);
        let left = TriMesh::new(vec![[0.0, 0.0, 0.0], [below, 10.0, 0.0], [-5.0, 5.0, 0.0]], vec![[0, 1, 2]]);
        let right = TriMesh::new(vec![[0.0, 0.0, 0.0], [5.0, 5.0, 0.0], [above, 10.0, 0.0]], vec![[0, 1, 2]]);
        let joined = weld(&[left, right]);
        assert_eq!(joined.vertices.len(), 4);
        assert_eq!(crate::boundary_loops(&joined).unwrap().len(), 1);
    }

    /// A wall of no height along a stretch leaves its crest on the berm's line with its own vertices: the berm has a
    /// corner part way along the wall's top edge. Welded alone that's a crack, a loop of three edges on one line with no
    /// area, so the face isn't one sheet (fault 5 of decision 0034). The face's sheet splits the wall's edge at the
    /// corner: one loop, the same 10 m by 10 m and 10 m by 5 m (150 m² in plan), and no point moved or added.
    #[test]
    fn a_berm_corner_part_way_along_a_wall_edge_joins_it() {
        let wall = TriMesh::new(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 10.0], [0.0, 10.0, 10.0]], vec![[0, 1, 2], [0, 2, 3]]);
        // The berm's lower edge runs along the wall's top edge, with a corner 3.7 m along it.
        let berm = TriMesh::new(
            vec![[0.0, 10.0, 10.0], [3.7, 10.0, 10.0], [10.0, 10.0, 10.0], [10.0, 15.0, 10.0], [0.0, 15.0, 10.0]],
            vec![[0, 1, 4], [1, 3, 4], [1, 2, 3]],
        );
        let welded = weld(&[wall.clone(), berm.clone()]);
        // The crack touches the outer edge at the wall's corner, so it isn't even a set of separate loops.
        assert!(crate::boundary_loops(&welded).map_or(true, |loops| loops.len() > 1), "welded alone, the corner leaves a crack");
        let (sheet, (merged, _)) = face_sheet(&[wall, berm], &[]);
        assert_eq!(merged, 0, "nothing a hair apart to weld");
        assert_eq!(crate::boundary_loops(&sheet).unwrap().len(), 1, "one sheet");
        assert_eq!(sheet.vertices, welded.vertices, "nothing moved or added");
        let plan_area: f64 = sheet
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|v| sheet.vertices[v as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum();
        assert!((plan_area - 150.0).abs() < 1e-9, "{plan_area}");
    }

    /// A corner worked out two ways lands a hair from itself: here the berm's corner is 0.4 mm along from the wall's.
    /// Within a millimetre it's made one, so the sheet is one loop, and the face says so: one corner, 0.4 mm. At 2 mm the
    /// corners are left where they are.
    #[test]
    fn a_corner_a_hair_from_itself_is_made_one_within_a_millimetre() {
        let wall = TriMesh::new(vec![[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 10.0, 10.0], [0.0, 10.0, 10.0]], vec![[0, 1, 2], [0, 2, 3]]);
        let berm = |off: f64| TriMesh::new(vec![[0.0, 10.0, 10.0], [10.0 + off, 10.0, 10.0], [10.0, 15.0, 10.0], [0.0, 15.0, 10.0]], vec![[0, 1, 2], [0, 2, 3]]);
        let (sheet, (merged, furthest)) = face_sheet(&[wall.clone(), berm(4e-4)], &[]);
        assert_eq!(crate::boundary_loops(&sheet).unwrap().len(), 1);
        assert_eq!((sheet.vertices.len(), merged), (6, 1));
        assert!((furthest - 4e-4).abs() < 1e-12, "{furthest}");
        assert!(sheet.vertices.iter().all(|p| p[0] <= 10.0), "the first worked out stays: nothing goes past the wall's corner");

        // The wall's corner lies on the berm's longer edge, so the two still meet there, with a 2 mm jog past it.
        let (apart, (merged, _)) = face_sheet(&[wall, berm(2e-3)], &[]);
        assert_eq!((apart.vertices.len(), merged), (7, 0), "2 mm apart is two corners");
        assert_eq!(crate::boundary_loops(&apart).unwrap().len(), 1);
    }

    #[test]
    fn points_further_apart_than_the_weld_stay_apart() {
        let left = TriMesh::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![[0, 1, 2]]);
        let right = TriMesh::new(vec![[3e-6, 0.0, 0.0], [1.0, 3e-6, 0.0], [1.0, 1.0, 0.0]], vec![[0, 1, 2]]);
        assert_eq!(weld(&[left, right]).vertices.len(), 6);
    }

    #[test]
    fn run_on_extends_each_end_straight() {
        let extended = run_on(&line(&[[100.0, 100.0, 5.0], [200.0, 100.0, 6.0], [200.0, 200.0, 7.0]]), [30.0, 40.0]).unwrap();
        assert_eq!(extended.vertices.len(), 5);
        close(extended.vertices[0][0] - E, 70.0, 1e-9);
        close(extended.vertices[4][1] - N, 240.0, 1e-9);
    }

    #[test]
    fn a_toe_off_its_grid_carries_on_at_the_grade_beside_it() {
        // The grid stops at x = 200 and has a hole from 100 to 140; it rises 0.1 in 1.
        let grid = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| {
            let (x, y) = (x - E, y - N);
            if x > 200.0 || (100.0..=140.0).contains(&x) && (50.0..=150.0).contains(&y) { f64::NAN } else { 100.0 + 0.1 * x }
        }));
        let (toe, log) = register_toe(&line(&[[20.0, 100.0, 0.0], [380.0, 100.0, 0.0]]), &grid).unwrap();
        assert!(log.off_grid > 0);
        for p in &toe.vertices {
            close(p[2], 100.0 + 0.1 * (p[0] - E), 1e-6);
        }
        assert!(register_toe(&line(&[[900.0, 100.0, 0.0], [980.0, 100.0, 0.0]]), &grid).is_err());
    }

    /// A toe on a level floor lies flat at its RL, whatever heights its string had, and stays where it was in plan.
    #[test]
    fn a_toe_on_a_level_floor_lies_flat() {
        let bent = line(&[[20.0, 100.0, 37.0], [200.0, 140.0, -5.0], [380.0, 60.0, 12.0]]);
        let (toe, log) = register_toe(&bent, &level_floor(&bent, -150.5)).unwrap();
        assert_eq!(log.off_grid, 0);
        assert!(toe.vertices.iter().all(|p| (p[2] + 150.5).abs() < 1e-9), "{:?}", toe.vertices);
        close(toe.plan_length(), bent.plan_length(), 1e-9);
        assert_eq!(toe.vertices[0][..2], bent.vertices[0][..2]);
        assert_eq!(toe.vertices.last().unwrap()[..2], bent.vertices[2][..2]);
    }

    #[test]
    fn a_wall_bridges_a_hole_in_its_target_and_says_where() {
        let holed = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| {
            let (x, y) = (x - E, y - N);
            if (150.0..=250.0).contains(&x) && (60.0..=200.0).contains(&y) { f64::NAN } else { 120.0 }
        }));
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&holed), 45.0)], &[]).unwrap();
        assert_eq!(face.steps[0].no_target.len(), 1, "{:?}", face.steps[0]);
        let stretch = face.steps[0].no_target[0];
        assert!(stretch.from[0] - E <= 150.0 + 10.0 && stretch.to[0] - E >= 250.0 - 10.0, "{stretch:?}");
        // A level target bridges level, so the crest is unbroken at its one height and offset.
        for p in &face.benches[0].crest.vertices {
            close(p[2], 120.0, 1e-6);
            close(p[1] - N, 120.0, 1e-6);
        }
        close(face.surface.area(), 300.0 * 20.0 * 2f64.sqrt(), 1e-3);
    }

    #[test]
    fn a_wall_past_the_edge_of_its_target_holds_the_level_of_the_edge() {
        let short = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, _| if x - E > 200.0 { f64::NAN } else { 120.0 }));
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&short), 45.0)], &[]).unwrap();
        assert_eq!(face.steps[0].no_target.len(), 1);
        for p in &face.benches[0].crest.vertices {
            close(p[2], 120.0, 1e-6);
        }
    }

    #[test]
    fn a_target_wound_downward_gives_the_same_crest() {
        let a = surface(|_, _| 100.0);
        let target = surface(|x, y| 130.0 + 5.0 * (x / 37.0).sin() + 3.0 * (y / 23.0).cos());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let flipped = target.flipped();
        let run = |mesh| project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(mesh), 70.0)], &[]).unwrap();
        let (normal, reversed) = (run(&target), run(&flipped));
        close(normal.benches[0].crest.plan_length(), reversed.benches[0].crest.plan_length(), 1e-6);
    }
}
