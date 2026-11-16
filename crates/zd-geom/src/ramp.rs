//! A ramp placed from a start (decision 0079): its polygon, from where it starts, the way it runs, its width, its grade
//! and what it runs to. Tim, 16 November 2026: "you place a start, set the width and grade and draws the ramp polygon
//! essentially". Placed again from another start, it has moved.
//!
//! The ramp's inside edge is its own line: along a string picked, from the nearest place on it to the start and the
//! way the second click says, or straight from the start towards that click. It takes its plan shape from there and
//! its heights from the grade alone, measured along the inside edge in plan (as a ramp's grade is, by default). It goes up or down as its
//! target says: to an RL above the start it climbs and to one below it falls, and to a surface it heads for the surface
//! from the side of it the start is on. It ends where the inside edge reaches the RL or meets the surface, squared
//! off. The grade is never eased to get there: a line that runs out first, or a surface never met, ends the ramp short,
//! and it says how far it got.
//!
//! The outside edge is the inside edge a road's width out, square across: each vertex moved out along its plan
//! bisector at its own height, so the road has no crossfall and a bend's corner is at one RL from edge to edge. A
//! bend too sharp for the width on that side, where the outside edge would turn back on itself, is refused.

use zd_core::{Polyline, TriMesh};

use crate::string::TriangleBins;
use crate::wall::nearest_along;
use crate::{GeomError, OffsetSide, drape_string};

type P3 = [f64; 3];

/// What a ramp runs to.
#[derive(Debug, Clone, Copy)]
pub enum RampTo<'a> {
    Rl(f64),
    /// A surface, or a grid as its triangulation.
    Surface(&'a TriMesh),
}

/// A ramp's polygon and what it measures.
#[derive(Debug, Clone)]
pub struct RampPolygon {
    /// The road's outline, closed: along the inside edge the way it runs, and back along the outside edge.
    pub polygon: Polyline,
    /// The inside edge, the way it runs.
    pub inside: Polyline,
    /// Its length along the inside edge, in plan (m).
    pub length: f64,
    /// The grade the way it runs (%): positive climbing, negative falling.
    pub grade: f64,
    pub from_rl: f64,
    pub to_rl: f64,
    /// How far the start was from the string it runs along, in plan, before it was put on it (m).
    pub start_moved: f64,
    /// It ended short of what it runs to, and why.
    pub short: Option<RampShort>,
}

/// Why a ramp ended short of what it runs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RampShort {
    /// The string it runs along ended first: it would have needed this much more of it (m).
    LineEnded { more: f64 },
    /// It never met the surface, as far as the surface or the string goes.
    NeverMet,
}

/// A height this near the target is on it (m).
const ON_TARGET: f64 = 1e-6;
/// Plan distance under which two points are one (m).
const SAME_POINT: f64 = 1e-9;
/// A mitre reaching further than this many widths is a bend back on itself.
const MOST_MITRE: f64 = 4.0;

/// The ramp from `start` running towards `towards`: along `line` if there is one, else straight. `width` metres wide on
/// `side`, looking the way it runs, at `grade_percent` (its size: the target says whether it climbs or falls), to `to`.
pub fn ramp_polygon(start: P3, towards: P3, line: Option<&Polyline>, width: f64, grade_percent: f64, side: OffsetSide, to: RampTo<'_>) -> Result<RampPolygon, GeomError> {
    if !(width.is_finite() && width > 0.0) {
        return Err(GeomError::Refused("the ramp's width must be more than zero".into()));
    }
    let grade = grade_percent.abs() / 100.0;
    if !(grade.is_finite() && grade > 0.0) {
        return Err(GeomError::Refused("the ramp's grade must be more than zero: a level road is an offset".into()));
    }
    if start.iter().chain(&towards).any(|c| !c.is_finite()) {
        return Err(GeomError::Refused("the start or the way it runs isn't a number".into()));
    }
    // The way it runs in plan, as far as it could go, from the start at chainage 0.
    let (path, start_moved, endless) = match line {
        Some(line) => {
            let (path, moved) = along_line(line, start, towards)?;
            (path, moved, false)
        }
        None => {
            let (dx, dy) = (towards[0] - start[0], towards[1] - start[1]);
            let length = dx.hypot(dy);
            if length < SAME_POINT {
                return Err(GeomError::Refused("the second click is on the start: click the way the ramp runs".into()));
            }
            (vec![[start[0], start[1]], [start[0] + dx / length, start[1] + dy / length]], 0.0, true)
        }
    };
    let z0 = start[2];
    let (way, chainage, short) = match to {
        RampTo::Rl(rl) => {
            if !rl.is_finite() {
                return Err(GeomError::Refused("the RL isn't a number".into()));
            }
            if (rl - z0).abs() <= ON_TARGET {
                return Err(GeomError::Refused(format!("the start is already at RL {rl}")));
            }
            let needed = (rl - z0).abs() / grade;
            let reach = if endless { needed } else { plan_length(&path) };
            let way = if rl > z0 { 1.0 } else { -1.0 };
            if needed <= reach + SAME_POINT { (way, needed.min(reach), None) } else { (way, reach, Some(RampShort::LineEnded { more: needed - reach })) }
        }
        RampTo::Surface(surface) => {
            let Some(bounds) = surface.bounds() else { return Err(GeomError::Refused("the surface has no triangles".into())) };
            // Straight from the start, it could go as far as the far side of the surface.
            let path = if endless {
                let far = [bounds.min, bounds.max].iter().flat_map(|c| [[c[0], bounds.min[1]], [c[0], bounds.max[1]]]).map(|c| (c[0] - start[0]).hypot(c[1] - start[1])).fold(0.0, f64::max);
                let (ux, uy) = (path[1][0] - path[0][0], path[1][1] - path[0][1]);
                vec![path[0], [path[0][0] + ux * far.max(1.0), path[0][1] + uy * far.max(1.0)]]
            } else {
                path.clone()
            };
            let (way, met) = meets_surface(&path, z0, grade, surface)?;
            return finish(&path, z0, way, grade, met.unwrap_or_else(|| plan_length(&path)), met.is_none().then_some(RampShort::NeverMet), width, side, start_moved);
        }
    };
    let path = if endless { vec![path[0], [path[0][0] + (path[1][0] - path[0][0]) * chainage, path[0][1] + (path[1][1] - path[0][1]) * chainage]] } else { path };
    finish(&path, z0, way, grade, chainage, short, width, side, start_moved)
}

/// The polygon, once it's known how far along its path the ramp goes.
#[allow(clippy::too_many_arguments)]
fn finish(path: &[[f64; 2]], z0: f64, way: f64, grade: f64, chainage: f64, short: Option<RampShort>, width: f64, side: OffsetSide, start_moved: f64) -> Result<RampPolygon, GeomError> {
    if chainage <= SAME_POINT {
        return Err(GeomError::Refused("the ramp has no length: its start is at the end of the line, or already on what it runs to".into()));
    }
    // The inside edge: the path as far as the chainage, at the grade.
    let mut inside: Vec<P3> = Vec::with_capacity(path.len());
    let mut along = 0.0;
    for (k, p) in path.iter().enumerate() {
        if k > 0 {
            let step = (p[0] - path[k - 1][0]).hypot(p[1] - path[k - 1][1]);
            if along + step >= chainage - SAME_POINT {
                let t = if step > 0.0 { ((chainage - along) / step).clamp(0.0, 1.0) } else { 0.0 };
                let q = path[k - 1];
                inside.push([q[0] + t * (p[0] - q[0]), q[1] + t * (p[1] - q[1]), z0 + way * grade * chainage]);
                break;
            }
            along += step;
        }
        inside.push([p[0], p[1], z0 + way * grade * along]);
    }
    inside.dedup_by(|b, a| (a[0] - b[0]).hypot(a[1] - b[1]) < SAME_POINT);
    if inside.len() < 2 {
        return Err(GeomError::Refused("the ramp has no length".into()));
    }
    let outside = edge_across(&inside, width, side)?;
    let to_rl = inside[inside.len() - 1][2];
    let mut vertices = inside.clone();
    vertices.extend(outside.iter().rev());
    Ok(RampPolygon {
        polygon: Polyline { vertices, closed: true },
        inside: Polyline { vertices: inside, closed: false },
        length: chainage,
        grade: way * grade * 100.0,
        from_rl: z0,
        to_rl,
        start_moved,
        short,
    })
}

fn plan_length(path: &[[f64; 2]]) -> f64 {
    path.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum()
}

/// The way along a string from the place on it nearest `start`, towards the place nearest `towards`: its plan
/// vertices from there to the string's end (round to the start again, if it's closed), and how far `start` was off it.
fn along_line(line: &Polyline, start: P3, towards: P3) -> Result<(Vec<[f64; 2]>, f64), GeomError> {
    let n = line.vertices.len();
    let segments = if line.closed { n } else { n.saturating_sub(1) };
    if segments == 0 {
        return Err(GeomError::Refused("the line the ramp runs along has no length".into()));
    }
    // A place on the string: its segment, how far along it, and how far off the point was.
    let place = |p: P3| {
        (0..segments)
            .map(|i| {
                let (t, gap) = nearest_along(line.vertices[i], line.vertices[(i + 1) % n], p);
                (i, t, gap)
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .expect("a segment at least")
    };
    let lengths: Vec<f64> = (0..segments).map(|i| (line.vertices[(i + 1) % n][0] - line.vertices[i][0]).hypot(line.vertices[(i + 1) % n][1] - line.vertices[i][1])).collect();
    let total: f64 = lengths.iter().sum();
    let chainage = |(i, t, _): (usize, f64, f64)| lengths[..i].iter().sum::<f64>() + t * lengths[i];
    let (from, aim) = (place(start), place(towards));
    let (s, a) = (chainage(from), chainage(aim));
    if (s - a).abs() < SAME_POINT {
        return Err(GeomError::Refused("the second click is beside the start: click further along the line, the way the ramp runs".into()));
    }
    // A closed string is followed the shorter way round to the second click.
    let forward = if line.closed { (a - s).rem_euclid(total) <= total / 2.0 } else { a > s };
    let at = |i: usize| [line.vertices[i % n][0], line.vertices[i % n][1]];
    let (i, t) = (from.0, from.1);
    let (p, q) = (at(i), at(i + 1));
    let on = [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])];
    let mut path = vec![on];
    if forward {
        let last = if line.closed { i + n } else { n - 1 };
        path.extend((i + 1..=last).map(at));
    } else {
        let first = if line.closed { i + 1 } else { 0 };
        path.extend((first..=i + if line.closed { n } else { 0 }).rev().map(at));
    }
    if line.closed {
        path.push(on);
    }
    path.dedup_by(|b, a| (a[0] - b[0]).hypot(a[1] - b[1]) < SAME_POINT);
    if path.len() < 2 {
        return Err(GeomError::Refused("the start is at the end of the line, with nowhere to run that way".into()));
    }
    Ok((path, from.2))
}

/// Which way a ramp from `z0` heads for a surface, up (1) or down (-1), and the chainage along `path` at which its
/// inside edge meets it, if it does. The surface is read along the path with a vertex wherever the path crosses one of
/// its triangles' edges, so between two of them both the surface and the edge are straight and where they cross is
/// exact. Where the surface has no data the ramp carries on, and meets it at the first place it has data again if it's
/// past it by then.
fn meets_surface(path: &[[f64; 2]], z0: f64, grade: f64, surface: &TriMesh) -> Result<(f64, Option<f64>), GeomError> {
    let flat = Polyline { vertices: path.iter().map(|p| [p[0], p[1], 0.0]).collect(), closed: false };
    let (draped, _) = drape_string(&flat, surface, true)?;
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &draped.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let mut along = 0.0;
    // Each place along the path with its chainage and the surface's height there, if it has one.
    let read: Vec<(f64, Option<f64>)> = draped
        .vertices
        .iter()
        .enumerate()
        .map(|(k, p)| {
            if k > 0 {
                along += (p[0] - draped.vertices[k - 1][0]).hypot(p[1] - draped.vertices[k - 1][1]);
            }
            (along, bins.height(surface, [p[0], p[1]]).map(|_| p[2]))
        })
        .collect();
    let Some(&(_, Some(first))) = read.iter().find(|(_, z)| z.is_some()) else {
        return Err(GeomError::Refused("the surface isn't under the ramp anywhere along the way it runs".into()));
    };
    if read[0].1.is_some_and(|z| (z - z0).abs() <= ON_TARGET) {
        return Err(GeomError::Refused("the start is already on the surface the ramp runs to".into()));
    }
    let way = if first > z0 { 1.0 } else { -1.0 };
    // How far the edge still has to go to the surface, at a place: nothing or less once it has met it.
    let to_go = |(s, z): (f64, Option<f64>)| z.map(|z| way * (z - (z0 + way * grade * s)));
    let mut before: Option<(f64, f64)> = None;
    for &place in &read {
        match to_go(place) {
            Some(left) if left <= 0.0 => {
                return Ok((way, Some(match before {
                    Some((s, was)) if was > 0.0 => s + (place.0 - s) * was / (was - left),
                    _ => place.0,
                })));
            }
            Some(left) => before = Some((place.0, left)),
            None => before = None,
        }
    }
    Ok((way, None))
}

/// An edge a width out from another, square across: each vertex out along its plan bisector, at its own height.
fn edge_across(inside: &[P3], width: f64, side: OffsetSide) -> Result<Vec<P3>, GeomError> {
    let sign = if side == OffsetSide::Left { 1.0 } else { -1.0 };
    let n = inside.len();
    let normal = |i: usize| {
        let (dx, dy) = (inside[i + 1][0] - inside[i][0], inside[i + 1][1] - inside[i][1]);
        let length = dx.hypot(dy);
        [-dy / length * sign, dx / length * sign]
    };
    let mut out: Vec<P3> = Vec::with_capacity(n);
    for i in 0..n {
        let p = inside[i];
        let (a, b) = (normal(i.saturating_sub(1).min(n - 2)), normal(i.min(n - 2)));
        let cos = a[0] * b[0] + a[1] * b[1];
        let reach = if 1.0 + cos > 1e-12 { (2.0 / (1.0 + cos)).sqrt() } else { f64::INFINITY };
        if reach > MOST_MITRE {
            return Err(GeomError::Refused(format!("the line doubles back too sharply near {:.1}, {:.1} for a road to follow it", p[0], p[1])));
        }
        let m = [(a[0] + b[0]) / (1.0 + cos), (a[1] + b[1]) / (1.0 + cos)];
        out.push([p[0] + m[0] * width, p[1] + m[1] * width, p[2]]);
    }
    // On the inside of a bend the outside edge shortens; where it would turn back on itself there's no room for the road.
    for i in 0..n - 1 {
        let along = (inside[i + 1][0] - inside[i][0]) * (out[i + 1][0] - out[i][0]) + (inside[i + 1][1] - inside[i][1]) * (out[i + 1][1] - out[i][1]);
        if along <= 0.0 {
            return Err(GeomError::Refused(format!(
                "the line turns too sharply near {:.1}, {:.1} for a road {width} m wide on its {}: its edge would turn back on itself",
                inside[i][0],
                inside[i][1],
                if side == OffsetSide::Left { "left" } else { "right" }
            )));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mine grid coordinates, so nothing passes only near the origin.
    const E: f64 = 650_000.0;
    const N: f64 = 7_500_000.0;

    fn at(x: f64, y: f64, z: f64) -> P3 {
        [E + x, N + y, z]
    }

    fn line(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.iter().map(|p| at(p[0], p[1], p[2])).collect(), closed: false }
    }

    /// A polygon's vertices about the origin, to the micrometre.
    fn local(polygon: &Polyline) -> Vec<[f64; 3]> {
        let r = |v: f64| (v * 1e6).round() / 1e6 + 0.0;
        polygon.vertices.iter().map(|p| [r(p[0] - E), r(p[1] - N), r(p[2])]).collect()
    }

    /// A plane through `rl` at the origin, rising `grade` per metre going east, as two triangles.
    fn sloping(rl: f64, grade: f64) -> TriMesh {
        let z = |x: f64| rl + grade * x;
        TriMesh::new(vec![at(-500.0, -500.0, z(-500.0)), at(900.0, -500.0, z(900.0)), at(900.0, 500.0, z(900.0)), at(-500.0, 500.0, z(-500.0))], vec![[0, 1, 2], [0, 2, 3]])
    }

    /// Straight east from RL 100 at 10 % to RL 120 is 200 m long; 30 m wide on the right, its polygon is the four
    /// corners, to 1 µm. To RL 90 it falls, 100 m long. On the left the road lies north of its inside edge.
    #[test]
    fn straight_from_a_start_to_an_rl() {
        let up = ramp_polygon(at(0.0, 0.0, 100.0), at(40.0, 0.0, 0.0), None, 30.0, 10.0, OffsetSide::Right, RampTo::Rl(120.0)).unwrap();
        assert_eq!(local(&up.polygon), [[0.0, 0.0, 100.0], [200.0, 0.0, 120.0], [200.0, -30.0, 120.0], [0.0, -30.0, 100.0]]);
        assert!(up.polygon.closed && up.short.is_none());
        assert!((up.length - 200.0).abs() < 1e-9 && (up.grade - 10.0).abs() < 1e-12 && up.from_rl == 100.0 && (up.to_rl - 120.0).abs() < 1e-9);
        let down = ramp_polygon(at(0.0, 0.0, 100.0), at(40.0, 0.0, 0.0), None, 30.0, 10.0, OffsetSide::Left, RampTo::Rl(90.0)).unwrap();
        assert_eq!(local(&down.polygon), [[0.0, 0.0, 100.0], [100.0, 0.0, 90.0], [100.0, 30.0, 90.0], [0.0, 30.0, 100.0]]);
        assert!((down.grade + 10.0).abs() < 1e-12, "falling the way it runs");
    }

    /// Along a toe that runs 150 m east and turns north, from 50 m along it and towards the north leg: 10 % to RL 120
    /// is 100 m to the corner and 100 m on. The corner is at RL 110 from edge to edge, on the inside of the bend (left,
    /// the outside edge 30 m short each way) and on the outside of it (right, 30 m long each way). The toe's own
    /// heights don't come into it.
    #[test]
    fn along_a_line_round_a_corner() {
        let toe = line(&[[0.0, 0.0, 87.0], [150.0, 0.0, 91.0], [150.0, 150.0, 95.0]]);
        let (start, towards) = (at(50.0, 0.4, 100.0), at(150.0, 100.0, 0.0));
        let left = ramp_polygon(start, towards, Some(&toe), 30.0, 10.0, OffsetSide::Left, RampTo::Rl(120.0)).unwrap();
        assert_eq!(local(&left.polygon), [[50.0, 0.0, 100.0], [150.0, 0.0, 110.0], [150.0, 100.0, 120.0], [120.0, 100.0, 120.0], [120.0, 30.0, 110.0], [50.0, 30.0, 100.0]]);
        assert!((left.start_moved - 0.4).abs() < 1e-9, "the start was put on the line");
        let right = ramp_polygon(start, towards, Some(&toe), 30.0, 10.0, OffsetSide::Right, RampTo::Rl(120.0)).unwrap();
        assert_eq!(local(&right.polygon), [[50.0, 0.0, 100.0], [150.0, 0.0, 110.0], [150.0, 100.0, 120.0], [180.0, 100.0, 120.0], [180.0, -30.0, 110.0], [50.0, -30.0, 100.0]]);
    }

    /// The other way along the same toe, from the north leg back towards its start: the path is the toe's backwards.
    #[test]
    fn the_second_click_says_which_way_along_the_line() {
        let toe = line(&[[0.0, 0.0, 100.0], [150.0, 0.0, 100.0], [150.0, 150.0, 100.0]]);
        let back = ramp_polygon(at(150.0, 50.0, 100.0), at(100.0, 0.0, 0.0), Some(&toe), 20.0, 10.0, OffsetSide::Left, RampTo::Rl(110.0)).unwrap();
        // 50 m south to the corner and 50 m west; looking that way, left of going south is east, and of going west, south.
        assert_eq!(local(&back.polygon), [[150.0, 50.0, 100.0], [150.0, 0.0, 105.0], [100.0, 0.0, 110.0], [100.0, -20.0, 110.0], [170.0, -20.0, 105.0], [170.0, 50.0, 100.0]]);
    }

    /// To a surface. From RL 100 going east at 10 % under a surface falling 1 in 20 from RL 130: 100 + x / 10 meets
    /// 130 - x / 20 at x = 200, RL 120. Going down to a floor rising 1 in 20 from RL 80: 100 - x / 10 meets 80 + x / 20
    /// at x = 133.333, RL 86.667. Each to 1 µm, and each across the surface's diagonal, where its triangles change.
    #[test]
    fn to_a_surface_up_and_down() {
        let up = ramp_polygon(at(0.0, 0.0, 100.0), at(10.0, 0.0, 0.0), None, 30.0, 10.0, OffsetSide::Right, RampTo::Surface(&sloping(130.0, -0.05))).unwrap();
        assert_eq!(local(&up.polygon), [[0.0, 0.0, 100.0], [200.0, 0.0, 120.0], [200.0, -30.0, 120.0], [0.0, -30.0, 100.0]]);
        assert!(up.short.is_none() && up.grade > 0.0);
        let down = ramp_polygon(at(0.0, 0.0, 100.0), at(10.0, 0.0, 0.0), None, 30.0, 10.0, OffsetSide::Right, RampTo::Surface(&sloping(80.0, 0.05))).unwrap();
        let (x, z) = (20.0 / 0.15, 100.0 - 2.0 / 0.15);
        let end = down.inside.vertices[1];
        assert!((end[0] - E - x).abs() < 1e-6 && (end[2] - z).abs() < 1e-6 && down.grade < 0.0, "{end:?}");
        // Along a line to the same surface: the corner at 100 m, then north, where the surface doesn't change: 130 - 150 / 20 = 122.5, 225 m along.
        let toe = line(&[[0.0, 0.0, 0.0], [150.0, 0.0, 0.0], [150.0, 400.0, 0.0]]);
        let round = ramp_polygon(at(0.0, 0.0, 100.0), at(150.0, 50.0, 0.0), Some(&toe), 30.0, 10.0, OffsetSide::Left, RampTo::Surface(&sloping(130.0, -0.05))).unwrap();
        let end = round.inside.vertices[round.inside.vertices.len() - 1];
        assert!((round.length - 225.0).abs() < 1e-6 && (end[1] - N - 75.0).abs() < 1e-6 && (end[2] - 122.5).abs() < 1e-6, "{end:?} {}", round.length);
    }

    /// A line that ends before the ramp gets there ends the ramp, at its grade, and says how much more it needed; so
    /// does a surface it never meets. A toe 150 m long to RL 120 at 10 % gets to RL 115, 50 m short.
    #[test]
    fn a_ramp_that_runs_out_of_line_ends_short_and_says() {
        let toe = line(&[[0.0, 0.0, 100.0], [150.0, 0.0, 100.0]]);
        let short = ramp_polygon(at(0.0, 0.0, 100.0), at(50.0, 0.0, 0.0), Some(&toe), 30.0, 10.0, OffsetSide::Right, RampTo::Rl(120.0)).unwrap();
        assert!((short.length - 150.0).abs() < 1e-9 && (short.to_rl - 115.0).abs() < 1e-9);
        assert!(matches!(short.short, Some(RampShort::LineEnded { more }) if (more - 50.0).abs() < 1e-9));
        // A level surface at RL 200 is 1,000 m away at 10 %: the toe ends first.
        let never = ramp_polygon(at(0.0, 0.0, 100.0), at(50.0, 0.0, 0.0), Some(&toe), 30.0, 10.0, OffsetSide::Right, RampTo::Surface(&sloping(200.0, 0.0))).unwrap();
        assert_eq!(never.short, Some(RampShort::NeverMet));
        assert!((never.to_rl - 115.0).abs() < 1e-9);
    }

    /// What can't be a ramp says why.
    #[test]
    fn refusals() {
        let toe = line(&[[0.0, 0.0, 100.0], [150.0, 0.0, 100.0], [150.0, 20.0, 100.0], [0.0, 20.0, 100.0]]);
        let why = |r: Result<RampPolygon, GeomError>| r.unwrap_err().to_string();
        let straight = |width, grade, to| ramp_polygon(at(0.0, 0.0, 100.0), at(40.0, 0.0, 0.0), None, width, grade, OffsetSide::Right, to);
        assert!(why(straight(0.0, 10.0, RampTo::Rl(120.0))).contains("width must be more than zero"));
        assert!(why(straight(30.0, 0.0, RampTo::Rl(120.0))).contains("grade must be more than zero"));
        assert_eq!(why(straight(30.0, 10.0, RampTo::Rl(100.0))), "the start is already at RL 100");
        assert!(why(straight(30.0, 10.0, RampTo::Surface(&sloping(100.0, 0.0)))).contains("already on the surface"));
        assert!(why(ramp_polygon(at(0.0, 0.0, 100.0), at(0.0, 0.0, 50.0), None, 30.0, 10.0, OffsetSide::Right, RampTo::Rl(120.0))).contains("click the way the ramp runs"));
        // A hairpin 20 m across has no room for a road 30 m wide on its inside.
        let hairpin = ramp_polygon(at(0.0, 0.0, 100.0), at(150.0, 20.0, 0.0), Some(&toe), 30.0, 10.0, OffsetSide::Left, RampTo::Rl(130.0));
        assert!(why(hairpin).contains("turns too sharply"), "refused on the inside of a tight bend");
    }
}
