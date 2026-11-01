//! What a step's batter or berm width comes to along a line: one value per edge.
//!
//! A step works from a different line each time: the string, then a conditioned crest, then a berm's line.
//! So a rule is asked again for each one. Rules that go by place look at the line itself, with a new vertex
//! wherever it crosses their boundary, so the change falls on the boundary rather than at the nearest
//! vertex. Rules that go by the string look at the string segment nearest each edge: a conditioned crest
//! has short segments that wander, and a value from them would flicker along the wall.

use parry3d_f64::math::Vector;
use parry3d_f64::query::{PointQuery, Ray, RayCast};
use parry3d_f64::shape::{TriMesh as ParryTriMesh, TriMeshFlags};
use zd_core::{AzimuthSector, Polyline, TriMesh};

use crate::string::segment_crossing;
use crate::wall::{nearest_along, segment_count};
use crate::{GeomError, OffsetSide};

/// Crossings of a solid closer than this along an edge are one (m).
const SAME_CROSSING: f64 = 1e-6;

/// A closed solid to ask whether a point is inside. Built once for a run, as it holds a BVH.
pub struct SolidRegion {
    origin: [f64; 3],
    mesh: ParryTriMesh,
}

impl std::fmt::Debug for SolidRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SolidRegion about {:?}", self.origin)
    }
}

impl SolidRegion {
    pub fn new(solid: &TriMesh) -> Result<Self, GeomError> {
        let Some(bounds) = solid.bounds() else { return Err(GeomError::InvalidInput("the solid has no triangles".into())) };
        let c = bounds.centre();
        let origin = [c[0].round(), c[1].round(), c[2].round()];
        let vertices = solid.vertices.iter().map(|p| Vector::new(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2])).collect();
        let mesh = ParryTriMesh::with_flags(vertices, solid.triangles.clone(), TriMeshFlags::ORIENTED).map_err(|e| GeomError::InvalidInput(format!("solid rule: {e:?}")))?;
        Ok(SolidRegion { origin, mesh })
    }

    fn local(&self, p: [f64; 3]) -> Vector {
        Vector::new(p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2])
    }

    pub fn contains(&self, p: [f64; 3]) -> bool {
        self.mesh.contains_local_point(self.local(p))
    }

    /// How far along `a`-`b`, from 0 to 1, the stretch crosses the solid's skin.
    fn crossings(&self, a: [f64; 3], b: [f64; 3]) -> Vec<f64> {
        let (from, to) = (self.local(a), self.local(b));
        let length = (to - from).length();
        let mut found = Vec::new();
        if length == 0.0 {
            return found;
        }
        let direction = (to - from) / length;
        let mut gone = 0.0;
        while let Some(toi) = self.mesh.cast_local_ray(&Ray::new(from + direction * gone, direction), length - gone, false) {
            gone += toi + SAME_CROSSING;
            found.push((gone - SAME_CROSSING) / length);
            if gone >= length {
                break;
            }
        }
        found
    }
}

/// How one step's value comes.
#[derive(Debug, Clone, Copy)]
pub enum EdgeRule<'a> {
    Fixed(f64),
    /// One value for each segment of `string`.
    List { string: &'a Polyline, values: &'a [f64] },
    /// One value inside a closed polygon in plan and another outside.
    Polygon { polygon: &'a Polyline, inside: f64, outside: f64 },
    /// One value inside a solid and another outside. `upside_down` is for a face going down, which runs
    /// with every height negated.
    Solid { region: &'a SolidRegion, inside: f64, outside: f64, upside_down: bool },
    /// A value by the way the wall faces: `faces` is the side of `string` it looks towards.
    Azimuth { string: &'a Polyline, faces: OffsetSide, sectors: &'a [AzimuthSector], otherwise: f64 },
}

impl From<f64> for EdgeRule<'_> {
    fn from(value: f64) -> Self {
        EdgeRule::Fixed(value)
    }
}

/// The nearest segment of `string` to a point, in plan.
fn nearest_segment(string: &Polyline, p: [f64; 3]) -> Option<usize> {
    let n = string.vertices.len();
    (0..segment_count(string)).map(|i| (i, nearest_along(string.vertices[i], string.vertices[(i + 1) % n], p).1)).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
}

/// Whether a plan point is inside a closed line, by counting crossings.
pub(crate) fn inside_polygon(polygon: &Polyline, p: [f64; 3]) -> bool {
    let n = polygon.vertices.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (polygon.vertices[i], polygon.vertices[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            inside = !inside;
        }
    }
    inside
}

/// The way a segment of `string` faces, in degrees clockwise from grid north.
fn facing(string: &Polyline, segment: usize, faces: OffsetSide) -> f64 {
    let n = string.vertices.len();
    let (a, b) = (string.vertices[segment], string.vertices[(segment + 1) % n]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (x, y) = if faces == OffsetSide::Left { (-dy, dx) } else { (dy, -dx) };
    x.atan2(y).to_degrees().rem_euclid(360.0)
}

/// Whether a sector, running clockwise from `from` to `to`, holds an azimuth. One that ends where it
/// starts holds them all.
fn holds(sector: &AzimuthSector, azimuth: f64) -> bool {
    let width = (sector.to - sector.from).rem_euclid(360.0);
    width == 0.0 || (azimuth - sector.from).rem_euclid(360.0) < width
}

/// `line` with a vertex at each of `splits`: for each segment, how far along it from 0 to 1.
fn with_vertices(line: &Polyline, mut splits: Vec<Vec<f64>>) -> Polyline {
    let n = line.vertices.len();
    let mut vertices = Vec::with_capacity(n);
    for i in 0..n {
        let a = line.vertices[i];
        vertices.push(a);
        let Some(along) = splits.get_mut(i) else { continue };
        let b = line.vertices[(i + 1) % n];
        along.sort_by(f64::total_cmp);
        vertices.extend(along.iter().map(|&t| [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]));
    }
    Polyline { vertices, closed: line.closed }
}

impl<'a> EdgeRule<'a> {
    /// The same rule for a face going down, which runs with every height negated.
    pub(crate) fn upside_down(self) -> Self {
        match self {
            EdgeRule::Solid { region, inside, outside, upside_down } => EdgeRule::Solid { region, inside, outside, upside_down: !upside_down },
            other => other,
        }
    }

    /// Two steps with one rule, whose walls can carry straight on from one to the other.
    pub(crate) fn same_as(&self, other: &EdgeRule<'_>) -> bool {
        match (self, other) {
            (EdgeRule::Fixed(a), EdgeRule::Fixed(b)) => a == b,
            (EdgeRule::List { values: a, .. }, EdgeRule::List { values: b, .. }) => std::ptr::eq(*a, *b),
            (EdgeRule::Polygon { polygon: a, inside: i, outside: o }, EdgeRule::Polygon { polygon: b, inside: j, outside: p }) => std::ptr::eq(*a, *b) && i == j && o == p,
            (EdgeRule::Solid { region: a, inside: i, outside: o, .. }, EdgeRule::Solid { region: b, inside: j, outside: p, .. }) => std::ptr::eq(*a, *b) && i == j && o == p,
            (EdgeRule::Azimuth { sectors: a, otherwise: i, .. }, EdgeRule::Azimuth { sectors: b, otherwise: j, .. }) => std::ptr::eq(*a, *b) && i == j,
            _ => false,
        }
    }

    /// The value at the middle of an edge.
    fn value_at(&self, middle: [f64; 3]) -> f64 {
        match *self {
            EdgeRule::Fixed(value) => value,
            EdgeRule::List { string, values } => nearest_segment(string, middle).map_or(0.0, |i| values[i]),
            EdgeRule::Polygon { polygon, inside, outside } => {
                if inside_polygon(polygon, middle) {
                    inside
                } else {
                    outside
                }
            }
            EdgeRule::Solid { region, inside, outside, upside_down } => {
                let z = if upside_down { -middle[2] } else { middle[2] };
                if region.contains([middle[0], middle[1], z]) { inside } else { outside }
            }
            EdgeRule::Azimuth { string, faces, sectors, otherwise } => {
                let azimuth = nearest_segment(string, middle).map(|i| facing(string, i, faces));
                azimuth.and_then(|a| sectors.iter().find(|s| holds(s, a))).map_or(otherwise, |s| s.value)
            }
        }
    }

    /// `line`, with a vertex wherever the rule's boundary crosses it, and one value for each of its segments.
    pub fn applied_to(&self, line: &Polyline) -> Result<(Polyline, Vec<f64>), GeomError> {
        let n = line.vertices.len();
        let segments = segment_count(line);
        let ends = |i: usize| (line.vertices[i], line.vertices[(i + 1) % n]);
        let split = match *self {
            EdgeRule::Polygon { polygon, .. } => {
                if polygon.vertices.len() < 3 {
                    return Err(GeomError::InvalidInput("a rule's polygon needs at least three vertices".into()));
                }
                let m = polygon.vertices.len();
                let plan = |p: [f64; 3]| [p[0], p[1]];
                let crossings = |i: usize| -> Vec<f64> {
                    let (a, b) = ends(i);
                    (0..m).filter_map(|k| segment_crossing(plan(a), plan(b), plan(polygon.vertices[k]), plan(polygon.vertices[(k + 1) % m])).map(|(t, _)| t)).collect()
                };
                Some(with_vertices(line, (0..segments).map(crossings).collect()))
            }
            EdgeRule::Solid { region, upside_down, .. } => {
                let real = |p: [f64; 3]| if upside_down { [p[0], p[1], -p[2]] } else { p };
                let crossings = |i: usize| {
                    let (a, b) = ends(i);
                    region.crossings(real(a), real(b))
                };
                Some(with_vertices(line, (0..segments).map(crossings).collect()))
            }
            _ => None,
        };
        let split = split.as_ref().unwrap_or(line);
        let m = split.vertices.len();
        let values = (0..segment_count(split))
            .map(|i| {
                let (a, b) = (split.vertices[i], split.vertices[(i + 1) % m]);
                self.value_at([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0])
            })
            .collect();
        Ok((split.clone(), values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.to_vec(), closed: false }
    }

    /// A 10 m cube from 40 to 60 east and north, RL 0 to 20.
    fn cube() -> TriMesh {
        let v = (0..8).map(|i| [40.0 + (i & 1) as f64 * 20.0, 40.0 + (i >> 1 & 1) as f64 * 20.0, (i >> 2) as f64 * 20.0]).collect();
        TriMesh::new(v, vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]])
    }

    #[test]
    fn a_polygon_splits_an_edge_where_it_crosses_and_gives_a_value_either_side() {
        let square = Polyline { vertices: vec![[40.0, 0.0, 0.0], [60.0, 0.0, 0.0], [60.0, 100.0, 0.0], [40.0, 100.0, 0.0]], closed: true };
        let rule = EdgeRule::Polygon { polygon: &square, inside: 60.0, outside: 70.0 };
        let (split, values) = rule.applied_to(&line(&[[0.0, 50.0, 100.0], [100.0, 50.0, 200.0]])).unwrap();
        assert_eq!(split.vertices, [[0.0, 50.0, 100.0], [40.0, 50.0, 140.0], [60.0, 50.0, 160.0], [100.0, 50.0, 200.0]]);
        assert_eq!(values, [70.0, 60.0, 70.0]);
    }

    #[test]
    fn a_solid_splits_an_edge_at_its_skin_and_goes_by_height_too() {
        let region = SolidRegion::new(&cube()).unwrap();
        let rule = EdgeRule::Solid { region: &region, inside: 60.0, outside: 70.0, upside_down: false };
        let (split, values) = rule.applied_to(&line(&[[0.0, 50.0, 10.0], [100.0, 50.0, 10.0]])).unwrap();
        assert_eq!(split.vertices.len(), 4, "{:?}", split.vertices);
        assert!((split.vertices[1][0] - 40.0).abs() < 1e-5 && (split.vertices[2][0] - 60.0).abs() < 1e-5, "{:?}", split.vertices);
        assert_eq!(values, [70.0, 60.0, 70.0]);

        // Over the top of it there's nothing to cross.
        let (split, values) = rule.applied_to(&line(&[[0.0, 50.0, 30.0], [100.0, 50.0, 30.0]])).unwrap();
        assert_eq!((split.vertices.len(), values), (2, vec![70.0]));

        // A face going down runs with heights negated, and still finds the solid.
        let down = rule.upside_down();
        let (split, values) = down.applied_to(&line(&[[0.0, 50.0, -10.0], [100.0, 50.0, -10.0]])).unwrap();
        assert_eq!((split.vertices.len(), values), (4, vec![70.0, 60.0, 70.0]));
    }

    /// A string running east with the wall rising to its left faces south, and running north it faces east.
    #[test]
    fn azimuth_goes_by_the_way_the_wall_faces_and_the_nearest_string_segment() {
        let string = line(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]);
        let sectors = [AzimuthSector { from: 135.0, to: 225.0, value: 60.0 }, AzimuthSector { from: 315.0, to: 45.0, value: 65.0 }];
        let rule = EdgeRule::Azimuth { string: &string, faces: OffsetSide::Right, sectors: &sectors, otherwise: 70.0 };
        let (_, values) = rule.applied_to(&string).unwrap();
        assert_eq!(values, [60.0, 70.0], "south-facing in the sector, east-facing not");

        // The other side of the string faces north then west, and north is the sector that wraps past 360.
        let rule = EdgeRule::Azimuth { string: &string, faces: OffsetSide::Left, sectors: &sectors, otherwise: 70.0 };
        assert_eq!(rule.applied_to(&string).unwrap().1, [65.0, 70.0]);

        // A crest off to one side, wandering in short pieces, takes the string's value all along it.
        let wander = line(&[[10.0, -20.0, 5.0], [14.0, -18.0, 5.0], [16.0, -22.0, 5.0], [90.0, -20.0, 5.0]]);
        let rule = EdgeRule::Azimuth { string: &string, faces: OffsetSide::Right, sectors: &sectors, otherwise: 70.0 };
        assert_eq!(rule.applied_to(&wander).unwrap().1, [60.0, 60.0, 60.0]);
    }

    #[test]
    fn a_list_gives_each_edge_the_value_of_the_string_segment_it_follows() {
        let string = line(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        let rule = EdgeRule::List { string: &string, values: &[60.0, 70.0] };
        let crest = line(&[[10.0, 20.0, 5.0], [50.0, 20.0, 5.0], [90.0, 20.0, 5.0], [150.0, 20.0, 5.0], [190.0, 20.0, 5.0]]);
        assert_eq!(rule.applied_to(&crest).unwrap().1, [60.0, 60.0, 70.0, 70.0]);
    }

    #[test]
    fn a_closed_line_gets_a_value_for_its_closing_edge_too() {
        let ring = Polyline { vertices: vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 100.0, 0.0]], closed: true };
        assert_eq!(EdgeRule::Fixed(60.0).applied_to(&ring).unwrap().1.len(), 4);
        let west = Polyline { vertices: vec![[-10.0, -10.0, 0.0], [50.0, -10.0, 0.0], [50.0, 110.0, 0.0], [-10.0, 110.0, 0.0]], closed: true };
        let (split, values) = EdgeRule::Polygon { polygon: &west, inside: 60.0, outside: 70.0 }.applied_to(&ring).unwrap();
        assert_eq!(split.vertices.len(), 6);
        assert_eq!(values, [60.0, 70.0, 70.0, 70.0, 60.0, 60.0]);
    }
}
