use std::collections::{HashMap, VecDeque};

use zd_core::TriMesh;

use crate::GeomError;

/// Makes winding consistent across each connected patch by flood fill, then turns each patch so
/// its area-weighted normal points up. Returns the oriented surface and the number of triangles
/// whose winding changed.
pub fn orient_upward(surface: &TriMesh) -> Result<(TriMesh, usize), GeomError> {
    let half_edges = sorted_half_edges(surface);
    let tri_count = surface.triangles.len();

    // Neighbour across each of a triangle's three edges (NONE on the boundary), with whether
    // both faces run the shared edge the same way.
    let mut neighbours = vec![[(NONE, false); 3]; tri_count];
    for run in half_edges.chunk_by(|x, y| x.key == y.key) {
        match run {
            [_] => {}
            [p, q] => {
                let same_direction = p.forward == q.forward;
                neighbours[p.tri][p.slot] = (q.tri as u32, same_direction);
                neighbours[q.tri][q.slot] = (p.tri as u32, same_direction);
            }
            _ => {
                return Err(GeomError::InvalidInput(format!(
                    "surface has an edge shared by {} triangles; it cannot be oriented",
                    run.len()
                )));
            }
        }
    }

    let mut flip: Vec<Option<bool>> = vec![None; tri_count];
    let mut oriented = surface.clone();
    let mut flipped = 0;
    let mut queue = VecDeque::new();
    for seed in 0..tri_count {
        if flip[seed].is_some() {
            continue;
        }
        let mut patch = vec![seed];
        flip[seed] = Some(false);
        queue.push_back(seed);
        while let Some(t) = queue.pop_front() {
            let ft = flip[t].unwrap();
            for &(u, same_direction) in &neighbours[t] {
                if u == NONE {
                    continue;
                }
                let u = u as usize;
                // Neighbours must run their shared edge in opposite directions once flipped.
                let want = ft ^ same_direction;
                match flip[u] {
                    None => {
                        flip[u] = Some(want);
                        patch.push(u);
                        queue.push_back(u);
                    }
                    Some(have) if have != want => {
                        return Err(GeomError::InvalidInput("surface is not orientable".into()));
                    }
                    Some(_) => {}
                }
            }
        }
        let up: f64 = patch
            .iter()
            .map(|&t| {
                let nz = surface.triangle_cross(surface.triangles[t])[2];
                if flip[t].unwrap() { -nz } else { nz }
            })
            .sum();
        let turn_patch = up < 0.0;
        for &t in &patch {
            if flip[t].unwrap() ^ turn_patch {
                let [a, b, c] = oriented.triangles[t];
                oriented.triangles[t] = [a, c, b];
                flipped += 1;
            }
        }
    }
    Ok((oriented, flipped))
}

/// Boundary loops of a consistently wound surface, each as vertex indices in the direction the
/// surface's own triangles run them (counter-clockwise around the outside of an upward surface,
/// clockwise around holes).
pub fn boundary_loops(surface: &TriMesh) -> Result<Vec<Vec<u32>>, GeomError> {
    let half_edges = sorted_half_edges(surface);
    let mut next: HashMap<u32, u32> = HashMap::new();
    for run in half_edges.chunk_by(|x, y| x.key == y.key) {
        if let [h] = run
            && next.insert(h.from, h.to).is_some()
        {
            return Err(GeomError::InvalidInput(format!("surface boundary touches itself at vertex {}", h.from)));
        }
    }

    let mut loops = Vec::new();
    let mut starts: Vec<u32> = next.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        if !next.contains_key(&start) {
            continue;
        }
        let mut ring = vec![start];
        let mut v = next.remove(&start).unwrap();
        while v != start {
            ring.push(v);
            v = next.remove(&v).ok_or_else(|| {
                GeomError::InvalidInput("surface boundary does not close into loops".into())
            })?;
        }
        loops.push(ring);
    }
    Ok(loops)
}

const NONE: u32 = u32::MAX;

struct HalfEdge {
    key: u64,
    from: u32,
    to: u32,
    forward: bool,
    tri: usize,
    /// Which of the triangle's three edges this is.
    slot: usize,
}

fn sorted_half_edges(surface: &TriMesh) -> Vec<HalfEdge> {
    let mut half_edges = Vec::with_capacity(surface.triangles.len() * 3);
    for (tri, &[a, b, c]) in surface.triangles.iter().enumerate() {
        for (slot, (from, to)) in [(a, b), (b, c), (c, a)].into_iter().enumerate() {
            let (lo, hi) = if from < to { (from, to) } else { (to, from) };
            let key = (u64::from(lo) << 32) | u64::from(hi);
            half_edges.push(HalfEdge { key, from, to, forward: from < to, tri, slot });
        }
    }
    half_edges.sort_unstable_by_key(|h| h.key);
    half_edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    #[test]
    fn scrambled_surface_is_turned_up() {
        let g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 6, 6, |x, y| x * 0.1 + y * 0.2);
        let reference = triangulate_grid(&g);
        let mut scrambled = reference.flipped();
        for t in scrambled.triangles.iter_mut().step_by(3) {
            let [a, b, c] = *t;
            *t = [a, c, b];
        }
        let (oriented, flipped) = orient_upward(&scrambled).unwrap();
        assert_eq!(oriented, reference);
        assert_eq!(flipped, reference.triangles.len() - reference.triangles.len().div_ceil(3));
    }

    #[test]
    fn grid_with_hole_has_outer_ccw_and_inner_cw_loops() {
        let mut g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 5, 5, |_, _| 0.0);
        let centre = g.index(2, 2);
        g.z[centre] = f64::NAN; // punches a diamond-shaped hole
        let surface = triangulate_grid(&g);
        let loops = boundary_loops(&surface).unwrap();
        assert_eq!(loops.len(), 2);
        let signed_area = |ring: &Vec<u32>| {
            let mut a = 0.0;
            for i in 0..ring.len() {
                let p = surface.vertices[ring[i] as usize];
                let q = surface.vertices[ring[(i + 1) % ring.len()] as usize];
                a += p[0] * q[1] - q[0] * p[1];
            }
            0.5 * a
        };
        let mut areas: Vec<f64> = loops.iter().map(signed_area).collect();
        areas.sort_by(f64::total_cmp);
        assert!(areas[0] < 0.0 && areas[1] > 0.0, "{areas:?}");
        assert!((areas[1] - 16.0).abs() < 1e-12);
    }
}
