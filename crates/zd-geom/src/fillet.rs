//! Round corners (String tools plan, workstream 2: `string.fillet`): a radius on every corner of a string that turns more
//! than an angle in plan. Each such corner becomes a circular arc in plan, tangent to both its legs, with a vertex every
//! so many metres; the heights are the legs' own where the arc meets them and graded evenly along it between. A corner
//! whose legs are too short for the radius takes the biggest that fits, and says so.

use zd_core::Polyline;

use crate::GeomError;

/// A corner that was rounded: its vertex (as the string had it, from 0), the radius it got, and whether that's less than
/// was asked because its legs are too short.
#[derive(Debug, Clone, PartialEq)]
pub struct Rounded {
    pub vertex: usize,
    pub radius: f64,
    pub tightened: bool,
}

const SAME: f64 = 1e-9;

/// Every corner of `line` turning more than `angle` degrees in plan rounded to `radius` metres, a vertex every `spacing`
/// metres along each arc. A leg between two corners being rounded lends each half its length; a leg to an open string's
/// end lends all of it; a corner whose legs can't hold the radius takes the biggest they can. Corners turning less, and a
/// string's ends, stay as they are. Gives the string and each corner rounded.
pub fn fillet_string(line: &Polyline, radius: f64, angle: f64, spacing: f64) -> Result<(Polyline, Vec<Rounded>), GeomError> {
    if !(radius.is_finite() && radius > 0.0) {
        return Err(GeomError::InvalidInput("the radius must be more than zero".into()));
    }
    if !(angle.is_finite() && (0.0..180.0).contains(&angle)) {
        return Err(GeomError::InvalidInput("the angle must be from 0° to under 180°".into()));
    }
    if !(spacing.is_finite() && spacing > 0.0) {
        return Err(GeomError::InvalidInput("the spacing along an arc must be more than zero".into()));
    }
    // Repeated vertices out, and a closed string's repeated first vertex.
    let mut points: Vec<(usize, [f64; 3])> = Vec::with_capacity(line.vertices.len());
    for (i, &v) in line.vertices.iter().enumerate() {
        if points.last().is_none_or(|(_, p)| plan(*p, v) > SAME) {
            points.push((i, v));
        }
    }
    if line.closed && points.len() > 1 && plan(points[0].1, points[points.len() - 1].1) <= SAME {
        points.pop();
    }
    let n = points.len();
    if n < 3 {
        return Ok((line.clone(), Vec::new()));
    }
    let at = |i: isize| points[i.rem_euclid(n as isize) as usize].1;
    let corners: Vec<usize> = if line.closed { (0..n).collect() } else { (1..n - 1).collect() };
    // The turn at each corner, in radians; only those past the angle are rounded.
    let turn = |i: usize| -> f64 {
        let (a, b, c) = (at(i as isize - 1), at(i as isize), at(i as isize + 1));
        let (u, w) = (unit(b, a), unit(b, c));
        let inside = (u[0] * w[0] + u[1] * w[1]).clamp(-1.0, 1.0).acos();
        std::f64::consts::PI - inside
    };
    let rounding: Vec<bool> = (0..n).map(|i| corners.contains(&i) && turn(i) > angle.to_radians() + SAME && turn(i) < std::f64::consts::PI - 1e-6).collect();
    let mut out: Vec<[f64; 3]> = Vec::new();
    let mut rounded = Vec::new();
    for i in 0..n {
        if !rounding[i] {
            out.push(at(i as isize));
            continue;
        }
        let (a, b, c) = (at(i as isize - 1), at(i as isize), at(i as isize + 1));
        let theta = turn(i);
        let half = (theta / 2.0).tan();
        // What each leg can lend: half if the corner at its other end is rounded too.
        let lend = |other: usize, length: f64| if rounding[other] { length / 2.0 } else { length };
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;
        let reach = lend(prev, plan(a, b)).min(lend(next, plan(b, c)));
        let r = radius.min(reach / half);
        let tangent = r * half;
        // Where the arc meets each leg, at the leg's own height there.
        let on_leg = |from: [f64; 3], to: [f64; 3]| -> [f64; 3] {
            let f = tangent / plan(from, to);
            [from[0] + (to[0] - from[0]) * f, from[1] + (to[1] - from[1]) * f, from[2] + (to[2] - from[2]) * f]
        };
        let (start, end) = (on_leg(b, a), on_leg(b, c));
        // The centre on the bisector, the arc swept the short way round.
        let (u, w) = (unit(b, a), unit(b, c));
        let bisector = {
            let (x, y) = (u[0] + w[0], u[1] + w[1]);
            let l = x.hypot(y);
            [x / l, y / l]
        };
        let to_centre = r / (std::f64::consts::PI - theta).mul_add(0.5, 0.0).sin();
        let centre = [b[0] + bisector[0] * to_centre, b[1] + bisector[1] * to_centre];
        let a0 = (start[1] - centre[1]).atan2(start[0] - centre[0]);
        let a1 = (end[1] - centre[1]).atan2(end[0] - centre[0]);
        let mut sweep = a1 - a0;
        while sweep > std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        }
        while sweep < -std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        let steps = ((r * sweep.abs()) / spacing).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            let t = a0 + sweep * f;
            let z = start[2] + (end[2] - start[2]) * f;
            out.push([centre[0] + r * t.cos(), centre[1] + r * t.sin(), z]);
        }
        rounded.push(Rounded { vertex: points[i].0, radius: r, tightened: r < radius - SAME });
    }
    out.dedup_by(|p, q| plan(*p, *q) <= SAME);
    Ok((Polyline { vertices: out, closed: line.closed }, rounded))
}

fn plan(a: [f64; 3], b: [f64; 3]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn unit(from: [f64; 3], to: [f64; 3]) -> [f64; 2] {
    let l = plan(from, to);
    [(to[0] - from[0]) / l, (to[1] - from[1]) / l]
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn plan_length(line: &Polyline) -> f64 {
        line.vertices.windows(2).map(|w| plan(w[0], w[1])).sum()
    }

    /// A right-angled corner with 100 m legs at mine coordinates, rounded at 20 m: every arc vertex 20 m from the centre
    /// (80 m along each leg, 20 m in), the plan length 2 × 80 + 20 × π/2 = 191.42 m from 200 m; heights graded along the
    /// arc from the legs' own (a leg climbing 10 % to the corner at RL 110 from 100, then flat).
    #[test]
    fn a_right_angle_rounded_at_20_m() {
        let line = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 110.0], [E + 100.0, N + 100.0, 110.0]], closed: false };
        let (round, corners) = fillet_string(&line, 20.0, 10.0, 2.0).unwrap();
        assert_eq!(corners, [Rounded { vertex: 1, radius: 20.0, tightened: false }]);
        let centre = [E + 80.0, N + 20.0];
        let arc = &round.vertices[1..round.vertices.len() - 1];
        assert!(arc.len() > 10);
        for v in arc {
            assert!(((v[0] - centre[0]).hypot(v[1] - centre[1]) - 20.0).abs() < 1e-9, "{v:?}");
        }
        assert!((plan_length(&round) - (160.0 + 10.0 * std::f64::consts::PI)).abs() < 0.02, "{}", plan_length(&round));
        // Where the arc starts, 80 m up the climbing leg at RL 108; where it ends, on the flat leg at RL 110.
        assert!((arc[0][2] - 108.0).abs() < 1e-9 && (arc[arc.len() - 1][2] - 110.0).abs() < 1e-9);
        assert!(arc.windows(2).all(|w| w[1][2] >= w[0][2] - 1e-12), "graded evenly up");
    }

    /// The same corner with 10 m legs: the most they hold is 10 m, which it gets, and says so.
    #[test]
    fn a_corner_too_tight_takes_the_biggest_radius_that_fits() {
        let line = Polyline { vertices: vec![[E, N, 100.0], [E + 10.0, N, 100.0], [E + 10.0, N + 10.0, 100.0]], closed: false };
        let (round, corners) = fillet_string(&line, 20.0, 10.0, 2.0).unwrap();
        assert_eq!(corners.len(), 1);
        assert!(corners[0].vertex == 1 && (corners[0].radius - 10.0).abs() < 1e-9 && corners[0].tightened, "{corners:?}");
        let centre = [E, N + 10.0];
        for v in &round.vertices {
            assert!(((v[0] - centre[0]).hypot(v[1] - centre[1]) - 10.0).abs() < 1e-9, "{v:?}");
        }
    }

    /// A 100 m square, closed, rounded at 10 m: four corners, each leg shared by two lending 50 m, so all get 10 m; the
    /// plan length 4 × 80 m of straight and each quarter circle (15.71 m) as the chords of its 16 steps a metre or less
    /// long, 4 × (80 + 16 × 2 × 10 × sin(π/64)) = 382.81 m. A corner of 5° stays sharp at 10°; a string of two vertices is
    /// as it was.
    #[test]
    fn a_closed_square_and_corners_under_the_angle() {
        let square = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0, N + 100.0, 100.0], [E, N + 100.0, 100.0]], closed: true };
        let (round, corners) = fillet_string(&square, 10.0, 10.0, 1.0).unwrap();
        assert_eq!(corners.len(), 4);
        assert!(corners.iter().all(|c| (c.radius - 10.0).abs() < 1e-12 && !c.tightened));
        let closing = plan(round.vertices[round.vertices.len() - 1], round.vertices[0]);
        let chords = 4.0 * (80.0 + 16.0 * 2.0 * 10.0 * (std::f64::consts::PI / 64.0).sin());
        assert!((plan_length(&round) + closing - chords).abs() < 1e-9, "{} against {chords}", plan_length(&round) + closing);
        let bend = (5.0f64).to_radians();
        let gentle = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0 + 100.0 * bend.cos(), N + 100.0 * bend.sin(), 100.0]], closed: false };
        let (same, none) = fillet_string(&gentle, 10.0, 10.0, 2.0).unwrap();
        assert!(none.is_empty() && same.vertices == gentle.vertices);
        let two = Polyline { vertices: vec![[E, N, 100.0], [E + 1.0, N, 100.0]], closed: false };
        assert_eq!(fillet_string(&two, 10.0, 10.0, 2.0).unwrap().0, two);
    }
}
