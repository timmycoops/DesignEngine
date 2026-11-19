//! Moving things whole (String tools plan, workstream 3): a move by an easting, northing and RL, a rotation in plan
//! about a point, and a mirror across a line in plan. Each is rigid, so a string keeps its lengths and grades and a
//! solid its volume. Points are worked from the point or line they turn about, so mine grid coordinates stay exact.

use zd_core::{Crossings, Grid, Polyline, TriMesh};

/// A rigid move of a whole object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Transform {
    /// By an easting, northing and RL (m).
    Move { by: [f64; 3] },
    /// About a point in plan, by an angle in degrees, anticlockwise seen from above. Heights stay as they are.
    Rotate { about: [f64; 2], degrees: f64 },
    /// Across the line through two points in plan. Heights stay as they are.
    Mirror { from: [f64; 2], to: [f64; 2] },
}

impl Transform {
    /// Why it can't be done, if it can't: something not a number, or a mirror line with no length.
    pub fn problem(&self) -> Option<&'static str> {
        let finite = |v: &[f64]| v.iter().all(|c| c.is_finite());
        match self {
            Transform::Move { by } if !finite(by) => Some("the move isn't a number"),
            Transform::Rotate { about, degrees } if !finite(about) || !degrees.is_finite() => Some("the rotation isn't a number"),
            Transform::Mirror { from, to } if !finite(from) || !finite(to) => Some("the mirror line isn't a number"),
            Transform::Mirror { from, to } if from == to => Some("the mirror line has no length: click two different points"),
            _ => None,
        }
    }

    /// Whether it leaves everything where it was: a move by nothing, or a whole number of turns.
    pub fn moves_nothing(&self) -> bool {
        match self {
            Transform::Move { by } => by.iter().all(|&c| c == 0.0),
            Transform::Rotate { degrees, .. } => degrees.rem_euclid(360.0) == 0.0,
            Transform::Mirror { .. } => false,
        }
    }

    /// Whether it turns things over (a mirror), so a mesh's triangles are wound the other way to face out still.
    pub fn turns_over(&self) -> bool {
        matches!(self, Transform::Mirror { .. })
    }

    /// Whether it only moves: what a grid can take, its rows running east and north.
    pub fn is_move(&self) -> bool {
        matches!(self, Transform::Move { .. })
    }

    /// Where a point goes.
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        match *self {
            Transform::Move { by } => [p[0] + by[0], p[1] + by[1], p[2] + by[2]],
            Transform::Rotate { about, degrees } => {
                let (sin, cos) = turn(degrees);
                let (x, y) = (p[0] - about[0], p[1] - about[1]);
                [about[0] + (x * cos - y * sin), about[1] + (x * sin + y * cos), p[2]]
            }
            Transform::Mirror { from, to } => {
                let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                let (x, y) = (p[0] - from[0], p[1] - from[1]);
                // The point's foot on the line, `t` along it; the mirror image is as far past the line as the point is
                // short of it.
                let t = (x * dx + y * dy) / (dx * dx + dy * dy);
                [from[0] + (2.0 * t * dx - x), from[1] + (2.0 * t * dy - y), p[2]]
            }
        }
    }
}

/// The sine and cosine of an angle in degrees, exact at quarter turns so a string turned square lands exactly.
fn turn(degrees: f64) -> (f64, f64) {
    let d = degrees.rem_euclid(360.0);
    let quarters = [(0.0, (0.0, 1.0)), (90.0, (1.0, 0.0)), (180.0, (0.0, -1.0)), (270.0, (-1.0, 0.0))];
    quarters.iter().find(|(q, _)| *q == d).map_or_else(|| d.to_radians().sin_cos(), |&(_, sc)| sc)
}

/// A string moved whole: the same vertices in the same order, open or closed as it was.
pub fn transform_string(line: &Polyline, t: &Transform) -> Polyline {
    Polyline { vertices: line.vertices.iter().map(|&v| t.point(v)).collect(), closed: line.closed }
}

/// A mesh moved whole. Mirrored, each triangle is wound the other way, so one that faced out still does.
pub fn transform_mesh(mesh: &TriMesh, t: &Transform) -> TriMesh {
    let vertices = mesh.vertices.iter().map(|&v| t.point(v)).collect();
    let triangles = if t.turns_over() { mesh.triangles.iter().map(|&[a, b, c]| [a, c, b]).collect() } else { mesh.triangles.clone() };
    TriMesh::new(vertices, triangles)
}

/// A mesh's crossing check moved with it: a rigid move makes no crossing and takes none away, so the check needn't run
/// again, only the places it found go where the mesh went.
pub fn transform_crossings(crossings: &Crossings, t: &Transform) -> Crossings {
    Crossings { at: crossings.at.iter().map(|&p| t.point(p)).collect(), thin_at: crossings.thin_at.iter().map(|&p| t.point(p)).collect(), ..crossings.clone() }
}

/// A grid moved by an easting, northing and RL: the same nodes, its origin moved and every live height raised or
/// lowered by the RL. A null stays null.
pub fn move_grid(grid: &Grid, by: [f64; 3]) -> Grid {
    let origin = [grid.origin[0] + by[0], grid.origin[1] + by[1]];
    Grid::new(origin, grid.cell_size, grid.cols, grid.rows, grid.z.iter().map(|&z| z + by[2]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    #[test]
    fn a_quarter_turn_about_a_point_takes_east_to_north_exactly() {
        let t = Transform::Rotate { about: [E, N], degrees: 90.0 };
        assert_eq!(t.point([E + 100.0, N, 104.5]), [E, N + 100.0, 104.5]);
        let back = Transform::Rotate { about: [E, N], degrees: -270.0 };
        assert_eq!(back.point([E + 100.0, N, 0.0]), [E, N + 100.0, 0.0], "-270° is the same turn");
    }

    #[test]
    fn a_turn_of_30_degrees_keeps_the_distance_from_the_point() {
        let t = Transform::Rotate { about: [E, N], degrees: 30.0 };
        let p = t.point([E + 100.0, N, 0.0]);
        assert!((p[0] - (E + 100.0 * 3f64.sqrt() / 2.0)).abs() < 1e-9 && (p[1] - (N + 50.0)).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn a_mirror_across_a_line_north_puts_a_point_as_far_the_other_side() {
        let t = Transform::Mirror { from: [E + 10.0, N], to: [E + 10.0, N + 50.0] };
        assert_eq!(t.point([E + 30.0, N + 7.0, 99.0]), [E - 10.0, N + 7.0, 99.0]);
        let diagonal = Transform::Mirror { from: [E, N], to: [E + 1.0, N + 1.0] };
        let p = diagonal.point([E + 100.0, N, 5.0]);
        assert!((p[0] - E).abs() < 1e-9 && (p[1] - (N + 100.0)).abs() < 1e-9 && p[2] == 5.0, "{p:?}");
    }

    #[test]
    fn a_mirrored_mesh_is_wound_the_other_way() {
        let mesh = TriMesh::new(vec![[E, N, 0.0], [E + 1.0, N, 0.0], [E, N + 1.0, 0.0]], vec![[0, 1, 2]]);
        let mirrored = transform_mesh(&mesh, &Transform::Mirror { from: [E, N], to: [E, N + 1.0] });
        assert_eq!(mirrored.triangles, [[0, 2, 1]]);
        assert_eq!(mirrored.vertices[1], [E - 1.0, N, 0.0]);
    }

    #[test]
    fn a_grid_moves_with_its_nulls_kept() {
        let grid = Grid::new([E, N], [5.0, 5.0], 2, 1, vec![100.0, f64::NAN]);
        let moved = move_grid(&grid, [100.5, -200.25, 3.0]);
        assert_eq!(moved.origin, [E + 100.5, N - 200.25]);
        assert_eq!(moved.z[0], 103.0);
        assert!(moved.z[1].is_nan());
    }

    #[test]
    fn what_moves_nothing_and_what_cant_be_done() {
        assert!(Transform::Move { by: [0.0; 3] }.moves_nothing());
        assert!(Transform::Rotate { about: [E, N], degrees: 720.0 }.moves_nothing());
        assert!(!Transform::Rotate { about: [E, N], degrees: 1.0 }.moves_nothing());
        assert!(Transform::Mirror { from: [E, N], to: [E, N] }.problem().is_some());
        assert!(Transform::Move { by: [f64::NAN, 0.0, 0.0] }.problem().is_some());
    }
}
