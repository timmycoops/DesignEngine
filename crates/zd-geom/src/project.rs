//! A string projected at a batter (decision 0079): the line a wall from it comes out on, up or down, at an RL, on a
//! surface or grid, or so many metres above or below the string.
//!
//! To an RL or a surface it's one wall of a face (`project_wall`): each segment's plane at the batter, hips where the
//! planes meet, the line walked across the target, a gap in the target bridged and reported. Going down is the same wall
//! upside down: heights are mirrored, the wall built, and what it made mirrored back. The side is where the new line
//! comes out, looking along the string, whichever way it goes.
//!
//! By a height there's no target to meet: every point of the string goes out the same distance in plan, the height
//! over the tangent of the batter, and up or down by the height. So it's the string's plan offset with true mitres, each
//! vertex at the height of the nearest point on the string, lifted. A vertex on a straight run of the string, where
//! only its grade changes, is kept on the new line square across from it, so a graded string's line has its grades.
//! On a level string that's the wall to an RL, corner for corner. On a string that rises and falls it's the line the
//! incumbents' project by a height gives: each corner goes out along its plan bisector, where the hip between two
//! graded walls would lean off it by a little.

use zd_core::{FaceDirection, Polyline, TriMesh};

use crate::face::{band, level_sheet, mirrored_line, mirrored_mesh, sheet_reach};
use crate::wall::{Stretch, WallLog, segment_count, with_crest_vertices};
use crate::{Corner, GeomError, OffsetSide, filter_string, offset_string, project_wall};

/// Where a string is projected to.
#[derive(Debug, Clone, Copy)]
pub enum ProjectTo<'a> {
    /// A surface, or a grid as its triangulation.
    Surface(&'a TriMesh),
    Rl(f64),
    /// So many metres above or below the string, all the way along.
    Height(f64),
}

/// A string projected.
#[derive(Debug, Clone)]
pub struct ProjectedString {
    /// The new line, running the same way as the string. By a height there's one for each piece, where the offset
    /// pinches off.
    pub lines: Vec<Polyline>,
    /// The wall between the string and its line, wound upward. By a height it's empty where the line came back in
    /// pieces.
    pub wall: TriMesh,
    pub log: WallLog,
}

/// A mitre however sharp the corner, as the hip between two walls runs.
const TRUE_MITRE: f64 = 1e9;
/// A vertex this close to the straight line through its neighbours is in line with them (m).
const IN_LINE: f64 = 1e-6;
/// How far a vertex's place square across may be from the new line and still go on it (m).
const ON_THE_LINE: f64 = 1e-6;
/// A string this straight at a vertex, as the sine of its turn, has no corner there for an offset to keep.
const STRAIGHT: f64 = 1e-9;

/// `line` projected on `side`, looking along it, `direction` at `batter_degrees` from horizontal, measured square to
/// the string in plan, to `to`.
pub fn project_string(line: &Polyline, side: OffsetSide, direction: FaceDirection, batter_degrees: f64, to: ProjectTo<'_>) -> Result<ProjectedString, GeomError> {
    if !(batter_degrees > 0.0 && batter_degrees < 90.0) {
        return Err(GeomError::Refused("the batter must be between 0 and 90 degrees".into()));
    }
    let down = direction == FaceDirection::Down;
    let no_height = |what: String| {
        let (past, way) = if down { ("below", "down") } else { ("above", "up") };
        GeomError::Refused(format!("the string is on or {past} {what} all the way along, so there's nothing to project {way} to"))
    };
    match to {
        ProjectTo::Height(height) => by_height(line, side, if down { -1.0 } else { 1.0 }, batter_degrees, height),
        ProjectTo::Rl(rl) if !rl.is_finite() => Err(GeomError::Refused("the RL isn't a number".into())),
        ProjectTo::Rl(rl) if down => {
            let flipped = mirrored_line(line);
            let level = level_sheet(&flipped, -rl, sheet_reach(&flipped, -rl, batter_degrees));
            to_surface(&flipped, side, batter_degrees, &level).and_then(straight_runs).map(right_way_up).map_err(|e| if matches!(e, GeomError::NoHeight) { no_height(format!("RL {rl}")) } else { e })
        }
        ProjectTo::Rl(rl) => {
            let level = level_sheet(line, rl, sheet_reach(line, rl, batter_degrees));
            to_surface(line, side, batter_degrees, &level).and_then(straight_runs).map_err(|e| if matches!(e, GeomError::NoHeight) { no_height(format!("RL {rl}")) } else { e })
        }
        ProjectTo::Surface(surface) if down => {
            to_surface(&mirrored_line(line), side, batter_degrees, &mirrored_mesh(surface)).map(right_way_up).map_err(|e| if matches!(e, GeomError::NoHeight) { no_height("the surface".into()) } else { e })
        }
        ProjectTo::Surface(surface) => to_surface(line, side, batter_degrees, surface).map_err(|e| if matches!(e, GeomError::NoHeight) { no_height("the surface".into()) } else { e }),
    }
}

/// One wall up to a surface, as a face's.
fn to_surface(line: &Polyline, side: OffsetSide, batter_degrees: f64, surface: &TriMesh) -> Result<ProjectedString, GeomError> {
    let step = project_wall(line, side, batter_degrees, surface)?;
    Ok(ProjectedString { lines: vec![step.crest], wall: step.wall, log: step.log })
}

/// A line to an RL without the vertices the level sheet gave it: the wall's line has one wherever it crosses a triangle
/// of its target, and an RL's two triangles are the tool's own, not the ground's. Only vertices within a micrometre of
/// the straight line through their neighbours go, so every corner and every change of grade stays.
fn straight_runs(mut made: ProjectedString) -> Result<ProjectedString, GeomError> {
    made.lines = made.lines.iter().map(|line| filter_string(line, IN_LINE)).collect::<Result<_, _>>()?;
    Ok(made)
}

/// What was built upside down, put back. Plan winding doesn't change, so the wall stays wound upward.
fn right_way_up(mut made: ProjectedString) -> ProjectedString {
    made.lines = made.lines.iter().map(mirrored_line).collect();
    made.wall = mirrored_mesh(&made.wall);
    for stretch in made.log.no_height.iter_mut().chain(&mut made.log.no_target) {
        *stretch = Stretch { from: [stretch.from[0], stretch.from[1], -stretch.from[2]], to: [stretch.to[0], stretch.to[1], -stretch.to[2]] };
    }
    made
}

/// The string moved out in plan by the height over the tangent of the batter, and up or down by the height.
fn by_height(line: &Polyline, side: OffsetSide, way: f64, batter_degrees: f64, height: f64) -> Result<ProjectedString, GeomError> {
    if !(height.is_finite() && height > 0.0) {
        return Err(GeomError::Refused("the height must be more than zero: up or down says which way it goes".into()));
    }
    let out = height / batter_degrees.to_radians().tan();
    let pieces = offset_string(line, out, side, Corner::Mitre { limit: TRUE_MITRE }, None)?;
    let across = Polyline { vertices: square_across(line, side, out), closed: false };
    let lines: Vec<Polyline> = pieces
        .iter()
        .map(|piece| {
            let mut with = with_crest_vertices(piece, &across, |from, on, gap| (gap <= ON_THE_LINE).then_some([on[0], on[1], from[2]]));
            for v in &mut with.vertices {
                v[2] += way * height;
            }
            with
        })
        .collect();
    // One piece is the whole string's line, and the wall is the sheet between the two. Pieces leave stretches of the
    // string with no line beside them, so there's no one sheet to make.
    let wall = match lines.as_slice() {
        [only] => band(line, only)?,
        _ => TriMesh::default(),
    };
    Ok(ProjectedString { lines, wall, log: WallLog { walls: segment_count(line), ..WallLog::default() } })
}

/// Where each vertex on a straight run of a string lands, square across from it: the string doesn't turn there, so an
/// offset keeps no vertex for it, but its grade may change. Each at its vertex's height.
fn square_across(line: &Polyline, side: OffsetSide, out: f64) -> Vec<[f64; 3]> {
    let n = line.vertices.len();
    let sign = if side == OffsetSide::Left { 1.0 } else { -1.0 };
    let way = |a: [f64; 3], b: [f64; 3]| {
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        (length > 0.0).then(|| [(b[0] - a[0]) / length, (b[1] - a[1]) / length])
    };
    let inner: Vec<usize> = if line.closed { (0..n).collect() } else { (1..n.saturating_sub(1)).collect() };
    inner
        .into_iter()
        .filter_map(|i| {
            let (before, at, after) = (line.vertices[(i + n - 1) % n], line.vertices[i], line.vertices[(i + 1) % n]);
            let (d0, d1) = (way(before, at)?, way(at, after)?);
            let straight = (d0[0] * d1[1] - d0[1] * d1[0]).abs() < STRAIGHT && d0[0] * d1[0] + d0[1] * d1[1] > 0.0;
            straight.then(|| [at[0] - d1[1] * sign * out, at[1] + d1[0] * sign * out, at[2]])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mine grid coordinates, so nothing passes only near the origin.
    const E: f64 = 650_000.0;
    const N: f64 = 7_500_000.0;

    fn line(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [p[0] + E, p[1] + N, p[2]]).collect(), closed: false }
    }

    /// A toe 200 m long running east at RL 100: left is north.
    fn toe() -> Polyline {
        line(&[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0]])
    }

    fn upward(mesh: &TriMesh) -> bool {
        mesh.triangles.iter().all(|t| {
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]) > 0.0
        })
    }

    /// How far a point is from a line, in space.
    fn off(line: &Polyline, p: [f64; 3]) -> f64 {
        line.vertices
            .windows(2)
            .map(|w| {
                let (t, gap) = crate::wall::nearest_along(w[0], w[1], p);
                gap.hypot(w[0][2] + t * (w[1][2] - w[0][2]) - p[2])
            })
            .fold(f64::INFINITY, f64::min)
    }

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    /// Up 20 m at 45° stands 20 m out: the line at RL 120, the wall 200 × 20√2 m². Tolerances: 1 µm and 1e-6 m².
    #[test]
    fn up_to_an_rl_at_45_degrees_stands_the_height_out() {
        for (side, y) in [(OffsetSide::Left, 20.0), (OffsetSide::Right, -20.0)] {
            let made = project_string(&toe(), side, FaceDirection::Up, 45.0, ProjectTo::Rl(120.0)).unwrap();
            let [only] = made.lines.as_slice() else { panic!("one line") };
            assert!(only.vertices.iter().all(|p| close(p[1] - N, y, 1e-6) && close(p[2], 120.0, 1e-6)), "{:?}", only.vertices);
            assert_eq!(only.vertices.len(), 2, "no vertex where it crosses the level sheet's own triangles");
            assert!(close(only.vertices[0][0] - E, 0.0, 1e-6) && close(only.vertices[1][0] - E, 200.0, 1e-6), "runs the string's way");
            assert!(close(made.wall.area(), 200.0 * 20.0 * 2f64.sqrt(), 1e-6), "{}", made.wall.area());
            assert!(upward(&made.wall));
        }
    }

    /// Down 20 m at 45° comes out on the side asked for, 20 m out at RL 80, with the same wall.
    #[test]
    fn down_to_an_rl_comes_out_on_the_side_asked_for() {
        for (side, y) in [(OffsetSide::Left, 20.0), (OffsetSide::Right, -20.0)] {
            let made = project_string(&toe(), side, FaceDirection::Down, 45.0, ProjectTo::Rl(80.0)).unwrap();
            let [only] = made.lines.as_slice() else { panic!("one line") };
            assert!(only.vertices.iter().all(|p| close(p[1] - N, y, 1e-6) && close(p[2], 80.0, 1e-6)), "{:?}", only.vertices);
            assert!(close(only.vertices[0][0] - E, 0.0, 1e-6), "runs the string's way");
            assert!(close(made.wall.area(), 200.0 * 20.0 * 2f64.sqrt(), 1e-6), "{}", made.wall.area());
            assert!(upward(&made.wall));
        }
    }

    /// A surface rising 1 in 10 away from the toe, RL 120 over it: a 45° wall meets it where 100 + y = 120 + y / 10,
    /// 22.222 m out at RL 122.222. Going down to one falling 1 in 10 from RL 80 is the same upside down. 1 µm.
    #[test]
    fn to_a_sloping_surface_up_and_down() {
        let sloping = |rl: f64, grade: f64| {
            let z = |y: f64| rl + grade * y;
            TriMesh::new(vec![[E - 500.0, N - 500.0, z(-500.0)], [E + 700.0, N - 500.0, z(-500.0)], [E + 700.0, N + 500.0, z(500.0)], [E - 500.0, N + 500.0, z(500.0)]], vec![[0, 1, 2], [0, 2, 3]])
        };
        let out = 20.0 / 0.9;
        let up = project_string(&toe(), OffsetSide::Left, FaceDirection::Up, 45.0, ProjectTo::Surface(&sloping(120.0, 0.1))).unwrap();
        assert!(up.lines[0].vertices.iter().all(|p| close(p[1] - N, out, 1e-6) && close(p[2], 100.0 + out, 1e-6)), "{:?}", up.lines[0].vertices);
        let down = project_string(&toe(), OffsetSide::Left, FaceDirection::Down, 45.0, ProjectTo::Surface(&sloping(80.0, -0.1))).unwrap();
        assert!(down.lines[0].vertices.iter().all(|p| close(p[1] - N, out, 1e-6) && close(p[2], 100.0 - out, 1e-6)), "{:?}", down.lines[0].vertices);
        assert!(upward(&up.wall) && upward(&down.wall));
        assert!(close(up.wall.area(), 200.0 * out * 2f64.sqrt(), 1e-6) && close(down.wall.area(), up.wall.area(), 1e-6));
    }

    /// On a level string a height is the wall to that RL, corner for corner: an L-shaped toe, inside and outside of
    /// the corner, up and down. 1 µm on every vertex, 1e-6 m² on the wall.
    #[test]
    fn by_a_height_on_a_level_string_is_the_wall_to_that_rl() {
        let bent = line(&[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0], [200.0, 150.0, 100.0]]);
        for side in [OffsetSide::Left, OffsetSide::Right] {
            for (direction, rl) in [(FaceDirection::Up, 115.0), (FaceDirection::Down, 85.0)] {
                let by_rl = project_string(&bent, side, direction, 60.0, ProjectTo::Rl(rl)).unwrap();
                let by_height = project_string(&bent, side, direction, 60.0, ProjectTo::Height(15.0)).unwrap();
                // The wall's line has a vertex wherever it crosses a triangle of its target, so the lines are compared, not
                // their vertices: each one's vertices lie on the other.
                let (a, b) = (&by_rl.lines[0], &by_height.lines[0]);
                assert!(a.vertices.iter().all(|p| off(b, *p) <= 1e-6) && b.vertices.iter().all(|p| off(a, *p) <= 1e-6), "{a:?} {b:?}");
                assert!(close(by_rl.wall.area(), by_height.wall.area(), 1e-6), "{} {}", by_rl.wall.area(), by_height.wall.area());
                assert!(upward(&by_height.wall));
            }
        }
    }

    /// A road's edge, straight in plan: level for 100 m, then up at 10 % for 100 m. 10 m up at 45° is the same line
    /// 10 m out and 10 m higher, its grade break kept square across from the string's. 1 µm.
    #[test]
    fn by_a_height_a_graded_string_keeps_its_grades() {
        let edge = line(&[[0.0, 0.0, 100.0], [100.0, 0.0, 100.0], [200.0, 0.0, 110.0]]);
        let made = project_string(&edge, OffsetSide::Left, FaceDirection::Up, 45.0, ProjectTo::Height(10.0)).unwrap();
        let want = [[0.0, 10.0, 110.0], [100.0, 10.0, 110.0], [200.0, 10.0, 120.0]];
        let got = &made.lines[0].vertices;
        assert_eq!(got.len(), 3, "{got:?}");
        assert!(got.iter().zip(want).all(|(p, q)| close(p[0] - E, q[0], 1e-6) && close(p[1] - N, q[1], 1e-6) && close(p[2], q[2], 1e-6)), "{got:?}");
        // A rectangle 100 × 10√2, then a parallelogram on sides (100, 0, 10) and (0, 10, 10): |cross| = √(100² + 1000² + 1000²).
        let second = ((100.0f64 * 10.0).powi(2) * 2.0 + (10.0f64 * 10.0).powi(2)).sqrt();
        assert!(close(made.wall.area(), 100.0 * 10.0 * 2f64.sqrt() + second, 1e-6), "{}", made.wall.area());
        let down = project_string(&edge, OffsetSide::Right, FaceDirection::Down, 45.0, ProjectTo::Height(10.0)).unwrap();
        assert!(down.lines[0].vertices.iter().zip(want).all(|(p, q)| close(p[1] - N, -10.0, 1e-6) && close(p[2], q[2] - 20.0, 1e-6)), "{:?}", down.lines[0].vertices);
    }

    /// What can't be done says why, the right way up.
    #[test]
    fn refusals_say_which_way() {
        let why = |direction, to| project_string(&toe(), OffsetSide::Left, direction, 45.0, to).unwrap_err().to_string();
        assert_eq!(why(FaceDirection::Up, ProjectTo::Rl(90.0)), "the string is on or above RL 90 all the way along, so there's nothing to project up to");
        assert_eq!(why(FaceDirection::Down, ProjectTo::Rl(110.0)), "the string is on or below RL 110 all the way along, so there's nothing to project down to");
        assert!(why(FaceDirection::Up, ProjectTo::Height(0.0)).starts_with("the height must be more than zero"));
        assert!(project_string(&toe(), OffsetSide::Left, FaceDirection::Up, 90.0, ProjectTo::Rl(120.0)).unwrap_err().to_string().starts_with("the batter must be between"));
    }
}
