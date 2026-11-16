//! A surface between strings (decision 0079): the sheet a road, a batter or a berm makes when it's drawn by hand as its
//! edges. Between two strings it's the band between a crest and the next toe that a face is built from (`band`):
//! their outline triangulated in plan, so it can't reach across either line, wound upward. So it's a sheet that's
//! single-valued in plan, as every wall under 90° and every road is, and strings that cross in plan have none.
//!
//! Several strings across a road and its batters (the toe of the fill, the road's two edges, the crest of the cut) make
//! one sheet: put in order across, each run the same way as the one before, a band between each and the next, and the
//! bands welded where they share a string. Nothing moves: every corner of the sheet is a vertex of a string.

use zd_core::{Polyline, TriMesh};

use crate::face::{band, weld};
use crate::wall::{nearest_along, segment_count};
use crate::GeomError;

/// Strings ready for sheets between them: in order across, each running the same way as the one before.
#[derive(Debug, Clone)]
pub struct Across {
    /// The strings, by their places as given, in order across.
    pub order: Vec<usize>,
    /// Each as it goes into the sheet, in that order.
    pub lines: Vec<Polyline>,
    /// Those turned to run the other way, by their places as given.
    pub reversed: Vec<usize>,
}

/// The strings in order across, each running the same way as the one before: the first given keeps its way, and the
/// order starts from whichever end of the row comes first as given. Two or more, all open or all closed.
pub fn strings_across(lines: &[&Polyline]) -> Result<Across, GeomError> {
    if lines.len() < 2 {
        return Err(GeomError::Refused("a surface needs two strings or more to lie between".into()));
    }
    if let Some(short) = lines.iter().position(|line| line.vertices.len() < if line.closed { 3 } else { 2 }) {
        return Err(GeomError::Refused(format!("string {} has too few vertices to be an edge", short + 1)));
    }
    if lines.iter().any(|line| line.closed != lines[0].closed) {
        return Err(GeomError::Refused("the strings are a mix of open and closed: a surface lies between open strings, or between closed ones".into()));
    }
    let order = order_across(lines);
    let mut out: Vec<Polyline> = Vec::with_capacity(lines.len());
    let mut reversed = Vec::new();
    // The first given sets the way they all run, wherever it lies across.
    let lead = order.iter().position(|&i| i == 0).expect("the first given is among them");
    let mut placed: Vec<Option<Polyline>> = vec![None; order.len()];
    placed[lead] = Some(lines[0].clone());
    let fit = |k: usize, beside: &Polyline, reversed: &mut Vec<usize>| {
        let line = lines[order[k]];
        let (turned, was) = the_same_way(line, beside);
        if was {
            reversed.push(order[k]);
        }
        turned
    };
    for k in lead + 1..order.len() {
        let beside = placed[k - 1].clone().expect("placed just before");
        placed[k] = Some(fit(k, &beside, &mut reversed));
    }
    for k in (0..lead).rev() {
        let beside = placed[k + 1].clone().expect("placed just after");
        placed[k] = Some(fit(k, &beside, &mut reversed));
    }
    out.extend(placed.into_iter().map(|line| line.expect("every string is placed")));
    reversed.sort_unstable();
    Ok(Across { order, lines: out, reversed })
}

/// The sheet between two strings running the same way, wound upward. Refused where they are one line, or cross in plan.
pub fn sheet_between(a: &Polyline, b: &Polyline) -> Result<TriMesh, GeomError> {
    let sheet = band(a, b).map_err(|e| match e {
        GeomError::InvalidInput(why) if why.contains("folds over itself") => {
            GeomError::Refused(format!("they cross in plan, or one doubles back past the other{}: a surface between them would fold over itself", why.strip_prefix("the berm").and_then(|rest| rest.strip_suffix(" folds over itself")).unwrap_or("")))
        }
        e => e,
    })?;
    if sheet.triangles.is_empty() {
        return Err(GeomError::Refused("they run along each other, so there's nothing between them".into()));
    }
    Ok(sheet)
}

/// The sheets between each string and the next as one, sharing the vertices of the string between them.
pub fn sheets_as_one(sheets: &[TriMesh]) -> TriMesh {
    weld(sheets)
}

/// How far apart two strings lie, in plan: the mean distance from each one's vertices and segment middles to the other.
fn apart(a: &Polyline, b: &Polyline) -> f64 {
    let off = |from: &Polyline, to: &Polyline| {
        let n = from.vertices.len();
        let mut places: Vec<[f64; 3]> = from.vertices.clone();
        places.extend((0..segment_count(from)).map(|i| {
            let (p, q) = (from.vertices[i], from.vertices[(i + 1) % n]);
            [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, 0.0]
        }));
        let m = to.vertices.len();
        let sum: f64 = places.iter().map(|p| (0..segment_count(to)).map(|i| nearest_along(to.vertices[i], to.vertices[(i + 1) % m], *p).1).fold(f64::INFINITY, f64::min)).sum();
        sum / places.len() as f64
    };
    (off(a, b) + off(b, a)) / 2.0
}

/// The strings' places in order across: the two furthest apart are the ends of the row, and the rest fall between by
/// how far each lies from one end. It starts from whichever end comes first as given.
fn order_across(lines: &[&Polyline]) -> Vec<usize> {
    let n = lines.len();
    if n == 2 {
        return vec![0, 1];
    }
    let mut gaps = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            gaps[i][j] = apart(lines[i], lines[j]);
            gaps[j][i] = gaps[i][j];
        }
    }
    let mut ends = (0, 1);
    for i in 0..n {
        for j in i + 1..n {
            if gaps[i][j] > gaps[ends.0][ends.1] {
                ends = (i, j);
            }
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&x, &y| gaps[ends.0][x].total_cmp(&gaps[ends.0][y]).then(x.cmp(&y)));
    order
}

/// `line` running the same way as `beside`, and whether it had to be turned. Open strings go by their ends: start to
/// start and end to end is shorter than crossed over. Closed strings go by the way they wind in plan, and start from
/// the vertex nearest the other's first, as the sheet between them is slit from first vertex to first vertex.
fn the_same_way(line: &Polyline, beside: &Polyline) -> (Polyline, bool) {
    let gap = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).hypot(p[1] - q[1]);
    if !line.closed {
        let (s, e) = (line.vertices[0], line.vertices[line.vertices.len() - 1]);
        let (bs, be) = (beside.vertices[0], beside.vertices[beside.vertices.len() - 1]);
        let turn = gap(s, be) + gap(e, bs) < gap(s, bs) + gap(e, be);
        let mut vertices = line.vertices.clone();
        if turn {
            vertices.reverse();
        }
        return (Polyline { vertices, closed: false }, turn);
    }
    let winding = |ring: &Polyline| {
        let o = ring.vertices[0];
        let n = ring.vertices.len();
        (0..n).map(|i| (ring.vertices[i][0] - o[0]) * (ring.vertices[(i + 1) % n][1] - o[1]) - (ring.vertices[(i + 1) % n][0] - o[0]) * (ring.vertices[i][1] - o[1])).sum::<f64>()
    };
    let turn = (winding(line) > 0.0) != (winding(beside) > 0.0);
    let mut vertices = line.vertices.clone();
    if turn {
        vertices.reverse();
    }
    let first = (0..vertices.len()).min_by(|&i, &j| gap(vertices[i], beside.vertices[0]).total_cmp(&gap(vertices[j], beside.vertices[0]))).expect("a closed string has vertices");
    vertices.rotate_left(first);
    (Polyline { vertices, closed: true }, turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary_loops;

    /// Mine grid coordinates, so nothing passes only near the origin.
    const E: f64 = 650_000.0;
    const N: f64 = 7_500_000.0;

    fn line(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [p[0] + E, p[1] + N, p[2]]).collect(), closed: false }
    }

    fn ring(points: &[[f64; 3]]) -> Polyline {
        Polyline { closed: true, ..line(points) }
    }

    fn upward(mesh: &TriMesh) -> bool {
        mesh.triangles.iter().all(|t| {
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]) > 0.0
        })
    }

    fn sheet(lines: &[&Polyline]) -> (TriMesh, Across) {
        let across = strings_across(lines).unwrap();
        let sheets: Vec<TriMesh> = across.lines.windows(2).map(|pair| sheet_between(&pair[0], &pair[1]).unwrap()).collect();
        (sheets_as_one(&sheets), across)
    }

    /// A road 30 m wide and 200 m long at 10 %: its two edges make a sheet of 30 × 200 × √1.01 m², to 1e-6 m², wound
    /// upward, with only the edges' own four vertices. Given running opposite ways, it's the same sheet, and said.
    #[test]
    fn a_road_between_its_two_edges() {
        let inside = line(&[[0.0, 0.0, 100.0], [200.0, 0.0, 120.0]]);
        let outside = line(&[[0.0, 30.0, 100.0], [200.0, 30.0, 120.0]]);
        let (road, across) = sheet(&[&inside, &outside]);
        assert!((road.area() - 30.0 * 200.0 * 1.01f64.sqrt()).abs() < 1e-6, "{}", road.area());
        assert!(upward(&road) && road.vertices.len() == 4 && across.reversed.is_empty());
        let backwards = Polyline { vertices: outside.vertices.iter().rev().copied().collect(), closed: false };
        let (again, across) = sheet(&[&inside, &backwards]);
        assert!((again.area() - road.area()).abs() < 1e-9 && upward(&again));
        assert_eq!(across.reversed, [1], "the second was turned to run with the first");
    }

    /// A road cut into a slope, drawn as four strings: the toe of the fill at RL 90, the road's edges at RL 100 30 m
    /// apart, and the crest of the cut at RL 115, each batter 45°. Given out of order and one backwards, they make one
    /// sheet in order across: 200 × (10√2 + 30 + 15√2) m² to 1e-6 m², one boundary loop, eight vertices.
    #[test]
    fn four_strings_across_a_road_make_one_sheet() {
        let fill_toe = line(&[[0.0, -10.0, 90.0], [200.0, -10.0, 90.0]]);
        let outer = line(&[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0]]);
        let inner = line(&[[200.0, 30.0, 100.0], [0.0, 30.0, 100.0]]);
        let crest = line(&[[0.0, 45.0, 115.0], [200.0, 45.0, 115.0]]);
        let (made, across) = sheet(&[&inner, &crest, &fill_toe, &outer]);
        // The crest and the fill's toe are the ends of the row; the crest comes first as given.
        assert_eq!(across.order, [1, 0, 3, 2]);
        assert_eq!(across.reversed, [1, 2, 3], "the first given keeps its way, and the rest run with it");
        let want = 200.0 * (10.0 * 2f64.sqrt() + 30.0 + 15.0 * 2f64.sqrt());
        assert!((made.area() - want).abs() < 1e-6, "{} against {want}", made.area());
        assert!(upward(&made));
        assert_eq!(made.vertices.len(), 8, "the bands share the strings between them");
        assert_eq!(boundary_loops(&made).unwrap().len(), 1, "one sheet, with no crack along a string");
    }

    /// A pit's toe and crest as closed strings: a 60 m square at RL 90 inside a 100 m square at RL 100, drawn opposite
    /// ways round and from different corners. The wall between is four trapezia, (100 + 60) / 2 × √(20² + 10²) each.
    #[test]
    fn a_ring_between_two_closed_strings() {
        let crest = ring(&[[0.0, 0.0, 100.0], [100.0, 0.0, 100.0], [100.0, 100.0, 100.0], [0.0, 100.0, 100.0]]);
        let toe = ring(&[[80.0, 80.0, 90.0], [80.0, 20.0, 90.0], [20.0, 20.0, 90.0], [20.0, 80.0, 90.0]]);
        let (wall, across) = sheet(&[&crest, &toe]);
        let want = 4.0 * 80.0 * 500f64.sqrt();
        assert!((wall.area() - want).abs() < 1e-6, "{} against {want}", wall.area());
        assert!(upward(&wall));
        assert_eq!(across.reversed, [1]);
        assert_eq!(boundary_loops(&wall).unwrap().len(), 2, "open at the toe and at the crest");
    }

    /// Curved edges: a road round a quarter of a circle, its edges arcs of 100 and 130 m radius at one RL. The sheet is
    /// the polygons' own area exactly (the arcs as drawn, 16 chords each), and within 1 % of the quarter annulus.
    #[test]
    fn a_road_round_a_bend() {
        let arc = |r: f64| line(&(0..=16).map(|k| (k as f64 / 16.0 * std::f64::consts::FRAC_PI_2).sin_cos()).map(|(s, c)| [r * c, r * s, 100.0]).collect::<Vec<_>>());
        let (inner, outer) = (arc(100.0), arc(130.0));
        let (road, _) = sheet(&[&inner, &outer]);
        let chords = 16.0 * 0.5 * (130.0f64.powi(2) - 100.0f64.powi(2)) * (std::f64::consts::FRAC_PI_2 / 16.0).sin();
        assert!((road.area() - chords).abs() < 1e-6, "{} against {chords}", road.area());
        let annulus = std::f64::consts::FRAC_PI_4 * (130.0f64.powi(2) - 100.0f64.powi(2));
        assert!((road.area() - annulus).abs() < 0.01 * annulus);
        assert!(upward(&road));
    }

    /// What can't be a sheet says why.
    #[test]
    fn refusals() {
        let a = line(&[[0.0, 0.0, 100.0], [200.0, 0.0, 100.0]]);
        let crossing = line(&[[0.0, -50.0, 100.0], [200.0, 50.0, 100.0]]);
        assert!(strings_across(&[&a]).unwrap_err().to_string().contains("two strings or more"));
        let same = strings_across(&[&a, &a]).unwrap();
        assert!(sheet_between(&same.lines[0], &same.lines[1]).unwrap_err().to_string().contains("nothing between them"));
        let crossed = strings_across(&[&a, &crossing]).unwrap();
        let why = sheet_between(&crossed.lines[0], &crossed.lines[1]).unwrap_err().to_string();
        assert!(why.starts_with("they cross in plan") && why.contains("fold over itself"), "{why}");
        let closed = ring(&[[0.0, 0.0, 100.0], [100.0, 0.0, 100.0], [100.0, 100.0, 100.0]]);
        assert!(strings_across(&[&a, &closed]).unwrap_err().to_string().contains("a mix of open and closed"));
    }
}
