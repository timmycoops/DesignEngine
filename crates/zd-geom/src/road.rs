//! A road up a wall (Phase 3). A ramp cuts back into the wall it runs along, so that stretch splits in
//! three: the lower wall stays where it was and stops at the road's pit-side edge, the road climbs at its
//! grade at full width, and the upper wall rises from the road's wall-side edge, a road width further back.
//!
//! The pit-side edge is the inside edge, which holds the grade as rise over plan run. It's walked up the
//! wall one facet at a time: on a plane there's one way forward that rises at the grade, so the edge is
//! exact on any wall, however the toe rises and falls, and it can't drift. The wall is then split along it.
//!
//! The upper wall is an ordinary wall from an ordinary string: along the toe to the road's start, back
//! across the road, and on up its wall-side edge. A road climbs a wall at an angle to its strike, so a
//! batter measured square to the road would lean the wall above: it's set so the two walls are parallel.
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
use crate::{Corner, OffsetSide, filter_string, offset_string, project_wall_by_edge};

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

/// A road to cut into a wall of a face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceRamp {
    /// Where it leaves the wall's toe: the nearest point on the toe to this, in plan.
    pub start: [f64; 2],
    /// It runs back against the way the string runs.
    pub back: bool,
    /// The step whose wall it climbs, from 0.
    pub step: usize,
    /// Square to the road (m).
    pub width: f64,
    /// Rise over plan run on the pit-side edge, as a fraction.
    pub grade: f64,
}

/// A road a face has cut.
#[derive(Debug, Clone)]
pub struct FaceRoad {
    /// Which ramp it is, in the order they were given.
    pub ramp: usize,
    pub step: usize,
    /// The inside edge, from the bottom of the road to the top.
    pub pit_edge: Polyline,
    /// From the bottom of the road to the top.
    pub wall_edge: Polyline,
}

/// A ramp that didn't go in, and where it stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct NoRoad {
    pub ramp: usize,
    pub step: usize,
    pub at: P3,
    pub why: String,
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
pub(crate) fn cut_road(toe: &Polyline, batters: &[f64], side: OffsetSide, ramp: &FaceRamp, at: usize, wall: &WallStep, target: &TriMesh) -> Result<RoadCut, Stopped> {
    if !ramp.back {
        return cut_road_along(toe, batters, side, ramp, at, wall, target);
    }
    let backwards = |line: &Polyline| Polyline { vertices: line.vertices.iter().rev().copied().collect(), closed: false };
    let turned = WallStep { crest: backwards(&wall.crest), wall: wall.wall.clone(), log: wall.log.clone() };
    let batters: Vec<f64> = batters.iter().rev().copied().collect();
    let other = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
    let mut cut = cut_road_along(&backwards(toe), &batters, other, ramp, toe.vertices.len() - 1 - at, &turned, target)?;
    cut.crest.vertices.reverse();
    Ok(cut)
}

/// A road running the way the toe does.
fn cut_road_along(toe: &Polyline, batters: &[f64], side: OffsetSide, ramp: &FaceRamp, at: usize, wall: &WallStep, target: &TriMesh) -> Result<RoadCut, Stopped> {
    let start = toe.vertices[at];
    let stopped = |why: String| Stopped { at: start, why };
    let facets = Facets::of(&wall.wall, start);
    let climb = facets.climb(start, side, ramp.grade, toe, &wall.crest)?;
    let pit_edge = Polyline { vertices: climb.edge.clone(), closed: false };
    let mut wall_edge = wall_edge_of(&pit_edge, ramp.width, side).map_err(|e| stopped(format!("the road's width can't be held: {e}")))?;
    run_out_onto(&mut wall_edge, ramp.grade, target).map_err(|why| Stopped { at: *pit_edge.vertices.last().expect("a climb has an end"), why })?;
    let (string, slopes) = string_up_the_road(toe, batters, at, &wall_edge, ramp.grade);
    let upper = project_wall_by_edge(&string, side, &slopes, target).map_err(|e| stopped(format!("the wall above it can't be built: {e}")))?;
    let road = band(&pit_edge, &road_inner_line(start, &wall_edge, &upper.crest)).map_err(|e| stopped(format!("the road can't be built: {e}")))?;
    let top = *pit_edge.vertices.last().expect("a climb has an end");
    let place = place_on(&wall.crest, top).ok_or_else(|| stopped("the road's top isn't on the crest".into()))?;
    let lower = facets.wall_below(&climb.cuts, &removed_by_road(toe, at, &pit_edge, &wall.crest, place));
    let crest = crest_round_road(&upper.crest, top, &wall.crest, place);
    Ok(RoadCut { lower, road, upper: upper.wall, crest, pit_edge, wall_edge, upper_log: upper.log })
}

/// The wall-side edge: the pit-side edge moved across the road by the width, level with it. A wall's facets
/// kink a road's edge a little at each one, and a true offset takes the folds out of the side they close
/// up on, which a mitre at every vertex wouldn't. What's left is tidied, as walls follow a line's plan shape.
fn wall_edge_of(pit_edge: &Polyline, width: f64, side: OffsetSide) -> Result<Polyline, GeomError> {
    let pieces = offset_string(pit_edge, width, side, Corner::Mitre { limit: EDGE_MITRE_LIMIT }, None)?;
    let longest = pieces.into_iter().max_by(|a, b| a.plan_length().total_cmp(&b.plan_length())).expect("an offset gives at least one string");
    let tidied = filter_string(&Polyline { vertices: longest.vertices.iter().map(|p| [p[0], p[1], 0.0]).collect(), closed: false }, WALL_EDGE_TOLERANCE)?;
    Ok(at_crest_heights(&tidied, pit_edge))
}

/// The line round the wall side of the road: back across its start and up its wall-side edge, with a vertex
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

/// The string the upper wall rises from, and its batters: along the toe to the road's start, back across
/// the road, and up its wall-side edge.
fn string_up_the_road(toe: &Polyline, batters: &[f64], at: usize, wall_edge: &Polyline, grade: f64) -> (Polyline, Vec<f64>) {
    let mut vertices = toe.vertices[..=at].to_vec();
    vertices.extend_from_slice(&wall_edge.vertices);
    let mut slopes = batters[..at].to_vec();
    slopes.push(batters[at.saturating_sub(1)]);
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
    use crate::face::{FaceStep, FaceTarget, project_face_with_roads};
    use crate::{EdgeRule, find_crossings, triangulate_grid};
    use zd_core::{FaceDirection, Grid};

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;
    const FLOOR: f64 = 100.0;

    fn level(rl: f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 20, 20, |_, _| rl))
    }

    fn string(points: &[[f64; 2]], rl: f64) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [E + p[0], N + p[1], rl]).collect(), closed: false }
    }

    /// One wall from a level toe along +x, rising to the north.
    fn wall_to(rl: f64, batter: f64, ramp: FaceRamp) -> crate::Face {
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        let steps = [FaceStep::Project { to: FaceTarget::Rl(rl), batter: EdgeRule::Fixed(batter), condition: None }];
        project_face_with_roads(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &steps, &[ramp]).unwrap()
    }

    const RAMP: FaceRamp = FaceRamp { start: [E + 100.0, N], back: false, step: 0, width: 10.0, grade: 0.1 };

    /// The height of a sheet at a point in plan.
    fn height(sheet: &TriMesh, x: f64, y: f64) -> f64 {
        let bins = TriangleBins::new(sheet, [E - 100.0, N - 100.0], [E + 800.0, N + 300.0]);
        bins.height(sheet, [E + x, N + y]).unwrap_or_else(|| panic!("nothing at {x}, {y}"))
    }

    fn rise_over_run(line: &Polyline) -> Vec<f64> {
        line.vertices.windows(2).filter(|w| plan_gap(w[0], w[1]) > 1e-6).map(|w| (w[1][2] - w[0][2]) / plan_gap(w[0], w[1])).collect()
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
        let across = 200.0 * (1.0f64 - 0.01).sqrt();
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
        let (cos, sin) = ((1.0f64 - 0.01).sqrt(), 0.1);
        let edge = [100.0 + 100.0 * cos, 10.0];
        let over = |reach: f64| [edge[0] - sin * reach, edge[1] + cos * reach];
        assert!((height(sheet, edge[0], 5.0) - 105.0).abs() < 1e-6, "the lower wall hasn't moved");
        for reach in [0.5, 5.0, 9.5] {
            let [x, y] = over(reach);
            assert!((height(sheet, x, y) - 110.0).abs() < 1e-6, "level across the road at {reach}: {}", height(sheet, x, y));
        }
        let [x, y] = over(10.0);
        assert!((height(sheet, x, y + 4.0) - 114.0).abs() < 1e-6, "the same batter behind the road: {}", height(sheet, x, y + 4.0));
    }

    /// The road meets the wall's strike at an angle, so 10 m across the road is 10 cos(angle) back square
    /// to the wall. A batter measured square to the road would lean the wall above by about 10 cm here.
    #[test]
    fn the_wall_above_a_road_stays_parallel_to_the_wall_below() {
        let face = wall_to(120.0, 45.0, RAMP);
        let crest = &face.benches[0].crest;
        let back = 20.0 + 10.0 * (1.0f64 - 0.01).sqrt();
        let stepped: Vec<&[f64; 3]> = crest.vertices.iter().filter(|p| p[1] - N > 25.0).collect();
        assert_eq!(stepped.len(), 2, "{:?}", crest.vertices);
        assert!(stepped[1][0] - stepped[0][0] > 220.0, "{stepped:?}");
        for p in stepped {
            assert!((p[1] - N - back).abs() < 1e-9, "{p:?} against {back}");
        }
        let past = crest.vertices.iter().find(|p| p[0] - E > 320.0).unwrap();
        assert!((past[1] - N - 20.0).abs() < 1e-6, "past the road the crest is where it was: {past:?}");
    }

    /// Walls, road and end wall cover the plan between the toe and the crest once each, to a square metre
    /// in a million: a gap or an overlap anywhere would show here.
    #[test]
    fn a_road_leaves_no_gap_or_overlap_in_plan() {
        for ramp in [RAMP, FaceRamp { start: [E + 400.0, N], back: true, ..RAMP }] {
            let face = wall_to(120.0, 45.0, ramp);
            let (sheet, bench) = (&face.surface, &face.benches[0]);
            let covered: f64 = sheet.triangles.iter().map(|t| {
                let [a, b, c] = t.map(|v| sheet.vertices[v as usize]);
                cross([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]) / 2.0
            }).sum();
            assert!(sheet.triangles.iter().all(|t| {
                let [a, b, c] = t.map(|v| sheet.vertices[v as usize]);
                cross([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]) > 0.0
            }), "every facet faces up");
            let outline: Vec<P3> = bench.toe.vertices.iter().chain(bench.crest.vertices.iter().rev()).copied().collect();
            let about = outline[0];
            let enclosed: f64 = (0..outline.len()).map(|k| {
                let (p, q) = (outline[k], outline[(k + 1) % outline.len()]);
                cross([p[0] - about[0], p[1] - about[1]], [q[0] - about[0], q[1] - about[1]]) / 2.0
            }).sum();
            assert!((covered - enclosed).abs() < 1e-6 * enclosed, "{covered} against {enclosed}");
        }
    }

    #[test]
    fn a_road_makes_one_sheet_with_nothing_crossing() {
        let face = wall_to(120.0, 70.0, RAMP);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
    }

    #[test]
    fn a_road_can_run_back_against_the_string() {
        let ramp = FaceRamp { start: [E + 400.0, N], back: true, ..RAMP };
        let face = wall_to(120.0, 45.0, ramp);
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let pit = &face.roads[0].pit_edge;
        let top = pit.vertices[pit.vertices.len() - 1];
        let across = 200.0 * (1.0f64 - 0.01).sqrt();
        assert!((top[0] - E - (400.0 - across)).abs() < 1e-3 && (top[2] - 120.0).abs() < 1e-3, "{top:?}");
        let crest = &face.benches[0].crest;
        assert!(crest.vertices.windows(2).all(|w| w[1][0] - w[0][0] > -30.0), "the crest still runs the string's way: {:?}", crest.vertices);
    }

    #[test]
    fn a_road_works_with_the_wall_on_the_right() {
        let toe = string(&[[600.0, 0.0], [0.0, 0.0]], FLOOR);
        let steps = [FaceStep::Project { to: FaceTarget::Rl(120.0), batter: EdgeRule::Fixed(45.0), condition: None }];
        let ramp = FaceRamp { start: [E + 500.0, N], ..RAMP };
        let face = project_face_with_roads(&toe, &level(FLOOR), OffsetSide::Right, FaceDirection::Up, &steps, &[ramp]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let top = *face.roads[0].pit_edge.vertices.last().unwrap();
        assert!((top[1] - N - 20.0).abs() < 1e-3, "the wall rises north of a string running west: {top:?}");
    }

    /// Too little string for the road's length: nothing is built past where it stops, and the wall stands.
    #[test]
    fn a_road_that_runs_out_of_wall_says_where_and_the_wall_still_builds() {
        let ramp = FaceRamp { start: [E + 500.0, N], ..RAMP };
        let face = wall_to(120.0, 45.0, ramp);
        assert!(face.roads.is_empty());
        let stopped = &face.no_roads[0];
        assert!(stopped.why.contains("runs out of wall"), "{}", stopped.why);
        assert!((stopped.at[0] - E - 600.0).abs() < 1e-6, "it stops at the end of the string: {:?}", stopped.at);
        assert!((stopped.at[2] - (FLOOR + 0.1 * 100.0 / (1.0f64 - 0.01).sqrt())).abs() < 1e-6, "100 m along at 10 %: {:?}", stopped.at);
        assert!((height(&face.surface, 300.0, 10.0) - 110.0).abs() < 1e-6, "the wall is the plain wall");
    }

    /// A wall flatter than the road's grade has no way up at that grade.
    #[test]
    fn a_wall_flatter_than_the_grade_is_named() {
        let ramp = FaceRamp { grade: 0.5, ..RAMP };
        let face = wall_to(105.0, 20.0, ramp);
        assert!(face.roads.is_empty());
        assert!(face.no_roads[0].why.contains("can't leave the toe"), "{}", face.no_roads[0].why);
    }

    /// A toe climbing at 15 % outruns a road at 10 %, which can never get above it.
    #[test]
    fn a_toe_climbing_faster_than_the_road_is_named() {
        let toe = Polyline { vertices: vec![[E, N, 100.0], [E + 600.0, N, 190.0]], closed: false };
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [100.0, 100.0], 20, 20, |x, _| 100.0 + 0.15 * (x - E)));
        let steps = [FaceStep::Project { to: FaceTarget::Rl(300.0), batter: EdgeRule::Fixed(45.0), condition: None }];
        let face = project_face_with_roads(&toe, &floor, OffsetSide::Left, FaceDirection::Up, &steps, &[RAMP]).unwrap();
        assert!(face.roads.is_empty(), "{:?}", face.roads);
        assert!(face.no_roads[0].why.contains("can't leave the toe"), "{}", face.no_roads[0].why);
    }

    /// The target rises behind the wall, so the wall-side edge has further to go than the pit-side edge,
    /// and carries on at the same grade until it comes out on the surface.
    #[test]
    fn a_road_comes_out_on_a_surface_that_rises_behind_the_wall() {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [50.0, 50.0], 40, 40, |_, y| 120.0 + 0.05 * (y - N)));
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        let steps = [FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(45.0), condition: None }];
        let face = project_face_with_roads(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &steps, &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let road = &face.roads[0];
        for grade in rise_over_run(&road.pit_edge).into_iter().chain(rise_over_run(&road.wall_edge)) {
            assert!((grade - 0.1).abs() < 1e-9, "{grade}");
        }
        let end = *road.wall_edge.vertices.last().unwrap();
        let bins = TriangleBins::new(&target, [E - 100.0, N - 100.0], [E + 800.0, N + 300.0]);
        assert!((bins.height(&target, [end[0], end[1]]).unwrap() - end[2]).abs() < 1e-6, "it finishes on the surface: {end:?}");
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
    }

    /// The target falls away behind the wall, so the crest comes down to the road's wall-side edge part way
    /// up it. The road and the wall above still meet edge to edge, with one boundary round the sheet.
    #[test]
    fn a_road_meets_a_crest_that_comes_down_to_it() {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [50.0, 50.0], 40, 40, |_, y| 120.0 - 0.3 * (y - N - 15.0).max(0.0)));
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        let steps = [FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(45.0), condition: None }];
        let face = project_face_with_roads(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &steps, &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
    }

    /// A wall that wanders round the minimum bench, with a berm above it, as real grids do.
    fn wandering_face(x: f64, rise: f64, height: f64, width: f64, side: OffsetSide) -> Result<crate::Face, crate::GeomError> {
        let target = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |px, _| FLOOR + height + rise * (px - E - x) + 2.0 * ((px - E) / 30.0).sin()));
        let steps = [
            FaceStep::Project { to: FaceTarget::Surface(&target), batter: EdgeRule::Fixed(70.0), condition: Some(0.1) },
            FaceStep::Berm { on: None, width: EdgeRule::Fixed(10.0), register: true, condition: Some(0.1), min_bench: Some(5.0) },
            FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(70.0), condition: Some(0.1) },
        ];
        let toe = match side {
            OffsetSide::Left => string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR),
            OffsetSide::Right => string(&[[600.0, 0.0], [0.0, 0.0]], FLOOR),
        };
        project_face_with_roads(&toe, &level(FLOOR), side, FaceDirection::Up, &steps, &[FaceRamp { start: [E + x, N], width, ..RAMP }])
    }

    /// The minimum bench changes 1.5 m short of the corner at the top of this road, and a change of width
    /// needs room to bevel. It moves to the corner, and the road stands.
    #[test]
    fn a_berm_changing_width_beside_a_road_changes_at_its_corner() {
        let face = wandering_face(40.0, -0.01, 5.0, 10.0, OffsetSide::Left).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
    }

    /// A road whose crest the berm above can't follow comes out and says so, and the face still builds.
    #[test]
    fn a_road_the_berm_above_cant_follow_comes_out_and_the_face_still_builds() {
        let face = wandering_face(80.0, -0.03, 3.0, 10.0, OffsetSide::Left).unwrap();
        assert!(face.roads.is_empty(), "{:?}", face.roads);
        assert!(face.no_roads[0].why.contains("what's above it can't be built with it"), "{:?}", face.no_roads);
        assert_eq!(face.benches.len(), 2, "both walls are there");
    }

    /// Walls round the minimum bench, each way along: nothing is refused, and any road that's built is one
    /// sheet with nothing crossing.
    #[test]
    fn roads_on_a_wall_wandering_round_the_minimum_bench_stay_one_sheet() {
        for (width, x, rise, height, side) in [
            (10.0, 120.0, 0.03, 4.0, OffsetSide::Left),
            (20.0, 200.0, -0.05, 3.0, OffsetSide::Left),
            (30.0, 80.0, 0.01, 5.0, OffsetSide::Left),
            (40.0, 160.0, -0.01, 2.0, OffsetSide::Left),
            (10.0, 500.0, 0.03, 4.0, OffsetSide::Right),
            (20.0, 400.0, -0.03, 3.0, OffsetSide::Right),
            (30.0, 460.0, 0.05, 5.0, OffsetSide::Right),
        ] {
            let face = wandering_face(x, rise, height, width, side).unwrap();
            assert_eq!(face.roads.len(), 1, "{width} {x} {rise} {height} {side:?}: {:?}", face.no_roads);
            assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "{width} {x} {rise} {height} {side:?}");
            assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0, "{width} {x} {rise} {height} {side:?}");
        }
    }

    /// A toe that rises and falls makes a wall of many planes, and the road holds one grade across them.
    #[test]
    fn a_road_holds_its_grade_over_a_toe_that_rises_and_falls() {
        let bumps = |x: f64| 100.0 + 2.0 * (x / 40.0).sin();
        let toe = Polyline { vertices: (0..=60).map(|k| [E + k as f64 * 10.0, N, bumps(k as f64 * 10.0)]).collect(), closed: false };
        let floor = triangulate_grid(&Grid::from_fn([E - 500.0, N - 500.0], [10.0, 10.0], 180, 100, |x, _| bumps(x - E)));
        let steps = [FaceStep::Project { to: FaceTarget::Rl(125.0), batter: EdgeRule::Fixed(60.0), condition: None }];
        let face = project_face_with_roads(&toe, &floor, OffsetSide::Left, FaceDirection::Up, &steps, &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        let pit = &face.roads[0].pit_edge;
        for grade in rise_over_run(pit) {
            assert!((grade - 0.1).abs() < 1e-6, "{grade}");
        }
        assert!((pit.vertices.last().unwrap()[2] - 125.0).abs() < 1e-6);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
    }

    /// The berm above a road starts from the stepped crest and the next wall stands on it.
    #[test]
    fn a_berm_and_a_wall_carry_on_above_a_road() {
        let toe = string(&[[0.0, 0.0], [600.0, 0.0]], FLOOR);
        let steps = [
            FaceStep::Project { to: FaceTarget::Rl(120.0), batter: EdgeRule::Fixed(45.0), condition: None },
            FaceStep::Berm { on: Some(FaceTarget::Rl(120.0)), width: EdgeRule::Fixed(10.0), register: true, condition: None, min_bench: None },
            FaceStep::Project { to: FaceTarget::Rl(140.0), batter: EdgeRule::Fixed(45.0), condition: None },
        ];
        let face = project_face_with_roads(&toe, &level(FLOOR), OffsetSide::Left, FaceDirection::Up, &steps, &[RAMP]).unwrap();
        assert!(face.no_roads.is_empty(), "{:?}", face.no_roads);
        assert_eq!(find_crossings(&face.surface, crate::COINCIDENT).pairs, 0);
        // Beside the road the next wall starts a road width and a berm behind the plain crest.
        let back = 20.0 + 10.0 * (1.0f64 - 0.01).sqrt() + 10.0;
        assert!((height(&face.surface, 200.0, back + 5.0) - 125.0).abs() < 1e-6, "{}", height(&face.surface, 200.0, back + 5.0));
    }

    /// A road takes the start nearest it on whichever line its step starts from.
    #[test]
    fn a_start_off_the_line_takes_the_nearest_point_on_it() {
        let ramp = FaceRamp { start: [E + 100.0, N + 30.0], ..RAMP };
        let face = wall_to(120.0, 45.0, ramp);
        let bottom = face.roads[0].pit_edge.vertices[0];
        assert_eq!([bottom[0] - E, bottom[1] - N], [100.0, 0.0]);
    }

    /// Nothing about a road on a plain wall needs many triangles.
    #[test]
    fn a_straight_road_stays_a_few_triangles() {
        let face = wall_to(120.0, 70.0, RAMP);
        assert!(face.surface.triangles.len() < 60, "{}", face.surface.triangles.len());
    }
}
