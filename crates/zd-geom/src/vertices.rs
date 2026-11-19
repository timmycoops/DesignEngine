//! Vertices a few at a time (String tools plan, workstream 4): the vertices picked on a string moved together, deleted
//! together, set to a grade or an RL on their own; a vertex added wherever another string crosses; and a vertex every so
//! many metres, the line unchanged.

use zd_core::Polyline;

use crate::GeomError;
use crate::string::plan_crossings;

/// A vertex this close in plan to one already there is that vertex (m).
const ON_A_VERTEX: f64 = 1e-6;

/// The vertices picked, each once, in order along the string; refused if one isn't on it or none is picked.
pub fn picked_in_order(line: &Polyline, picked: &[usize]) -> Result<Vec<usize>, GeomError> {
    let n = line.vertices.len();
    if picked.is_empty() {
        return Err(GeomError::InvalidInput("no vertex picked on it".into()));
    }
    if let Some(&past) = picked.iter().find(|&&i| i >= n) {
        return Err(GeomError::InvalidInput(format!("vertex {} is past the end of a {n}-vertex string", past + 1)));
    }
    let mut sorted = picked.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    Ok(sorted)
}

/// The vertices picked moved together by an easting, northing and RL; the others stay exactly where they are.
pub fn move_vertices(line: &Polyline, picked: &[usize], by: [f64; 3]) -> Result<Polyline, GeomError> {
    let picked = picked_in_order(line, picked)?;
    let mut out = line.clone();
    for i in picked {
        let v = &mut out.vertices[i];
        *v = [v[0] + by[0], v[1] + by[1], v[2] + by[2]];
    }
    Ok(out)
}

/// The vertices picked taken out. Refused if that leaves an open string fewer than two vertices or a closed one fewer
/// than three.
pub fn delete_vertices(line: &Polyline, picked: &[usize]) -> Result<Polyline, GeomError> {
    let picked = picked_in_order(line, picked)?;
    let vertices: Vec<[f64; 3]> = line.vertices.iter().enumerate().filter(|(i, _)| picked.binary_search(i).is_err()).map(|(_, v)| *v).collect();
    let needed = if line.closed { 3 } else { 2 };
    if vertices.len() < needed {
        return Err(GeomError::InvalidInput(format!(
            "deleting {} of its {} vertices would leave {}, and {} string needs {needed}",
            picked.len(),
            line.vertices.len(),
            vertices.len(),
            if line.closed { "a closed" } else { "an open" }
        )));
    }
    Ok(Polyline { vertices, closed: line.closed })
}

/// The vertices picked set to an RL; the others keep theirs.
pub fn set_vertices_rl(line: &Polyline, picked: &[usize], rl: f64) -> Result<Polyline, GeomError> {
    let picked = picked_in_order(line, picked)?;
    let mut out = line.clone();
    for i in picked {
        out.vertices[i][2] = rl;
    }
    Ok(out)
}

/// The vertices picked set to a grade: the first along the string takes `start_rl`, and each later one rises
/// `grade_percent` per 100 m of plan length along the string from it, through the vertices between whether picked or
/// not. Vertices not picked keep their RLs.
pub fn grade_vertices(line: &Polyline, picked: &[usize], start_rl: f64, grade_percent: f64) -> Result<Polyline, GeomError> {
    if !start_rl.is_finite() || !grade_percent.is_finite() {
        return Err(GeomError::InvalidInput("the start RL and grade must be finite".into()));
    }
    let picked = picked_in_order(line, picked)?;
    let mut out = line.clone();
    let first = picked[0];
    let mut walked = 0.0;
    let mut next = picked.iter().peekable();
    for i in first..=*picked.last().expect("one picked at least") {
        if i > first {
            let (a, b) = (line.vertices[i - 1], line.vertices[i]);
            walked += (b[0] - a[0]).hypot(b[1] - a[1]);
        }
        if next.peek() == Some(&&i) {
            next.next();
            out.vertices[i][2] = start_rl + grade_percent / 100.0 * walked;
        }
    }
    Ok(out)
}

/// A vertex put on the string wherever `other` crosses it in plan, at the string's own RL there. Where a crossing is on
/// a vertex already there, none is added. How many were added comes back with it.
pub fn vertices_at_crossings(line: &Polyline, others: &[&Polyline]) -> (Polyline, usize) {
    // Where along the string, in plan, the crossings are, nearest the start first.
    let mut along: Vec<f64> = others.iter().flat_map(|o| plan_crossings(line, o)).map(|c| c.along).collect();
    along.sort_by(f64::total_cmp);
    along.dedup_by(|later, earlier| *later - *earlier < ON_A_VERTEX);
    let n = line.vertices.len();
    let segments = if line.closed { n } else { n.saturating_sub(1) };
    let mut vertices = Vec::with_capacity(n + along.len());
    let mut walked = 0.0;
    let mut added = 0;
    let mut next = along.iter().peekable();
    for k in 0..segments {
        let (a, b) = (line.vertices[k], line.vertices[(k + 1) % n]);
        vertices.push(a);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        while let Some(&&at) = next.peek() {
            if at > walked + length + ON_A_VERTEX {
                break;
            }
            next.next();
            let t = (at - walked) / length;
            if length > 0.0 && (at - walked) > ON_A_VERTEX && (walked + length - at) > ON_A_VERTEX {
                vertices.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]);
                added += 1;
            }
        }
        walked += length;
    }
    if !line.closed && n > 0 {
        vertices.push(line.vertices[n - 1]);
    }
    if n == 1 {
        vertices = line.vertices.clone();
    }
    (Polyline { vertices, closed: line.closed }, added)
}

/// A vertex at least every `spacing` metres in plan: each segment split into equal parts no longer than that, the new
/// vertices on it with RLs graded evenly along it, so the line is the same line. The vertices there already stay exactly.
pub fn densify_string(line: &Polyline, spacing: f64) -> Result<Polyline, GeomError> {
    if !(spacing.is_finite() && spacing > 0.0) {
        return Err(GeomError::InvalidInput("the spacing must be more than zero".into()));
    }
    let n = line.vertices.len();
    let segments = if line.closed && n > 2 { n } else { n.saturating_sub(1) };
    let mut vertices = Vec::with_capacity(n);
    for k in 0..segments {
        let (a, b) = (line.vertices[k], line.vertices[(k + 1) % n]);
        vertices.push(a);
        let parts = ((b[0] - a[0]).hypot(b[1] - a[1]) / spacing - 1e-9).ceil().max(1.0) as usize;
        for j in 1..parts {
            let t = j as f64 / parts as f64;
            vertices.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])]);
        }
    }
    if !(line.closed && n > 2) && n > 0 {
        vertices.push(line.vertices[n - 1]);
    }
    Ok(Polyline { vertices, closed: line.closed })
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn ten() -> Polyline {
        Polyline { vertices: (0..10).map(|i| [E + 10.0 * i as f64, N, 100.0 + i as f64]).collect(), closed: false }
    }

    #[test]
    fn three_of_ten_moved_five_metres_north_and_the_rest_exactly_where_they_were() {
        let line = ten();
        let moved = move_vertices(&line, &[6, 2, 4, 2], [0.0, 5.0, 0.0]).unwrap();
        for (i, (v, was)) in moved.vertices.iter().zip(&line.vertices).enumerate() {
            let want = if [2, 4, 6].contains(&i) { [was[0], was[1] + 5.0, was[2]] } else { *was };
            assert_eq!(*v, want, "vertex {i}");
        }
        assert!(move_vertices(&line, &[10], [0.0; 3]).is_err(), "past the end");
        assert!(move_vertices(&line, &[], [0.0; 3]).is_err(), "none picked");
    }

    #[test]
    fn deleting_leaves_the_rest_and_never_too_few() {
        let line = ten();
        let kept = delete_vertices(&line, &[0, 9, 5]).unwrap();
        assert_eq!(kept.vertices.len(), 7);
        assert_eq!(kept.vertices[0], line.vertices[1]);
        assert!(delete_vertices(&line, &(0..9).collect::<Vec<_>>()).is_err(), "one left of an open string");
        let square = Polyline { vertices: vec![[E, N, 0.0], [E + 1.0, N, 0.0], [E + 1.0, N + 1.0, 0.0], [E, N + 1.0, 0.0]], closed: true };
        assert!(delete_vertices(&square, &[0, 1]).is_err(), "two left of a closed one");
    }

    #[test]
    fn a_grade_on_the_vertices_picked_alone_measured_along_the_string() {
        let line = ten();
        // Vertices 2, 3 and 7 at 10 % from RL 50: 0, 10 and 50 m along from vertex 2.
        let graded = grade_vertices(&line, &[7, 2, 3], 50.0, 10.0).unwrap();
        assert_eq!(graded.vertices[2][2], 50.0);
        assert!((graded.vertices[3][2] - 51.0).abs() < 1e-9 && (graded.vertices[7][2] - 55.0).abs() < 1e-9);
        for i in [0, 1, 4, 5, 6, 8, 9] {
            assert_eq!(graded.vertices[i], line.vertices[i], "vertex {i} not picked");
        }
        let level = set_vertices_rl(&line, &[1, 8], 90.0).unwrap();
        assert_eq!((level.vertices[1][2], level.vertices[8][2], level.vertices[2][2]), (90.0, 90.0, 102.0));
    }

    #[test]
    fn two_lines_crossing_each_gain_a_vertex_at_the_crossing() {
        // A line east at 10 % from RL 100 and one north at RL 50 crossing it 30 m along.
        let east = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 110.0]], closed: false };
        let north = Polyline { vertices: vec![[E + 30.0, N - 20.0, 50.0], [E + 30.0, N + 20.0, 50.0]], closed: false };
        let (a, added_a) = vertices_at_crossings(&east, &[&north]);
        let (b, added_b) = vertices_at_crossings(&north, &[&east]);
        assert_eq!((added_a, added_b), (1, 1));
        assert_eq!(a.vertices.len(), 3);
        let v = a.vertices[1];
        assert!((v[0] - (E + 30.0)).abs() < 1e-9 && (v[1] - N).abs() < 1e-9 && (v[2] - 103.0).abs() < 1e-9, "{v:?}: at its own RL, 103");
        let w = b.vertices[1];
        assert!((w[0] - (E + 30.0)).abs() < 1e-9 && (w[1] - N).abs() < 1e-9 && w[2] == 50.0, "{w:?}");
        // Crossed again on that vertex: nothing more added.
        let (again, more) = vertices_at_crossings(&a, &[&north]);
        assert_eq!((more, again.vertices.len()), (0, 3));
    }

    #[test]
    fn a_100_m_straight_every_2_m_is_51_vertices_all_on_the_line() {
        let line = Polyline { vertices: vec![[E, N, 100.0], [E + 60.0, N + 80.0, 110.0]], closed: false };
        let dense = densify_string(&line, 2.0).unwrap();
        assert_eq!(dense.vertices.len(), 51);
        assert_eq!((dense.vertices[0], dense.vertices[50]), (line.vertices[0], line.vertices[1]), "the ends exactly");
        for (k, v) in dense.vertices.iter().enumerate() {
            let t = k as f64 / 50.0;
            assert!((v[0] - (E + 60.0 * t)).abs() < 1e-9 && (v[1] - (N + 80.0 * t)).abs() < 1e-9 && (v[2] - (100.0 + 10.0 * t)).abs() < 1e-9, "vertex {k}");
        }
        let square = Polyline { vertices: vec![[E, N, 0.0], [E + 10.0, N, 0.0], [E + 10.0, N + 10.0, 0.0], [E, N + 10.0, 0.0]], closed: true };
        assert_eq!(densify_string(&square, 5.0).unwrap().vertices.len(), 8, "a closed square, its closing side too");
    }
}
