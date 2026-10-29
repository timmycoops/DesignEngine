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

/// What [`orient_patches`] found and changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrientLog {
    /// Connected patches: triangles joined across edges with exactly two faces.
    pub patches: usize,
    /// Patches with no boundary edge, turned to face out rather than up.
    pub closed_patches: usize,
    /// Triangles whose winding was reversed.
    pub triangles_flipped: usize,
    /// Edges with three or more faces; the fill does not cross them.
    pub non_manifold_edges: usize,
    /// The midpoints of the first [`SITES`] non-manifold edges.
    pub non_manifold_at: Vec<[f64; 3]>,
    /// Patches that cannot be wound consistently (one-sided), left as read.
    pub non_orientable_patches: usize,
}

/// Most sites listed for one kind of problem.
pub const SITES: usize = 100;

/// Makes winding consistent across each connected patch by flood fill, then turns each patch to
/// face out if it is closed (positive volume) or up if it is open (area-weighted normal). Edges with
/// three or more faces stop the fill and are listed; a patch that cannot be wound consistently is
/// left as read and counted. Nothing is refused, unlike [`orient_upward`].
pub fn orient_patches(mesh: &TriMesh) -> (TriMesh, OrientLog) {
    let tri_count = mesh.triangles.len();
    let mut log = OrientLog::default();
    let Adjacency { neighbours, on_boundary, non_manifold } = adjacency(mesh);
    log.non_manifold_edges = non_manifold.len();
    log.non_manifold_at = non_manifold
        .iter()
        .take(SITES)
        .map(|&(u, v)| {
            let (a, b) = (mesh.vertices[u as usize], mesh.vertices[v as usize]);
            [0, 1, 2].map(|i| 0.5 * (a[i] + b[i]))
        })
        .collect();

    // Volumes and normals about a local origin, so mine grid coordinates keep their precision.
    let origin = mesh.bounds().map_or([0.0; 3], |b| b.centre());
    let mut flip: Vec<Option<bool>> = vec![None; tri_count];
    let mut oriented = mesh.clone();
    let mut queue = VecDeque::new();
    for seed in 0..tri_count {
        if flip[seed].is_some() {
            continue;
        }
        log.patches += 1;
        let mut patch = vec![seed];
        let mut consistent = true;
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
                    Some(have) => consistent &= have == want,
                }
            }
        }
        if !consistent {
            log.non_orientable_patches += 1;
            continue;
        }
        let closed = patch.iter().all(|&t| !on_boundary[t]);
        let measure: f64 = patch
            .iter()
            .map(|&t| {
                let [a, b, c] = mesh.triangles[t].map(|i| sub(mesh.vertices[i as usize], origin));
                let v = if closed { dot(a, cross(b, c)) } else { cross(sub(b, a), sub(c, a))[2] };
                if flip[t].unwrap() { -v } else { v }
            })
            .sum();
        log.closed_patches += usize::from(closed);
        let turn_patch = measure < 0.0;
        for &t in &patch {
            if flip[t].unwrap() ^ turn_patch {
                let [a, b, c] = oriented.triangles[t];
                oriented.triangles[t] = [a, c, b];
                log.triangles_flipped += 1;
            }
        }
    }
    (oriented, log)
}

/// Who is across each triangle's edges.
struct Adjacency {
    /// Per triangle and edge: the one other triangle on it and whether both run it the same way;
    /// `NONE` on the boundary and where three or more faces meet.
    neighbours: Vec<[(u32, bool); 3]>,
    /// Triangles with an edge no other triangle has.
    on_boundary: Vec<bool>,
    /// Edges with three or more faces, once each, in triangle order.
    non_manifold: Vec<(u32, u32)>,
}

/// Finds each edge's other faces through a list of the triangles at each vertex (no sort of every
/// edge), split across cores.
fn adjacency(mesh: &TriMesh) -> Adjacency {
    let tris = &mesh.triangles;
    let mut start = vec![0u32; mesh.vertices.len() + 1];
    for t in tris {
        for &v in t {
            start[v as usize + 1] += 1;
        }
    }
    for i in 0..mesh.vertices.len() {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut incident = vec![0u32; tris.len() * 3];
    for (k, t) in tris.iter().enumerate() {
        for &v in t {
            incident[fill[v as usize] as usize] = k as u32;
            fill[v as usize] += 1;
        }
    }

    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, tris.len().max(1));
    let chunk = tris.len().div_ceil(threads).max(1);
    let (start, incident) = (&start, &incident);
    let parts: Vec<Adjacency> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                scope.spawn(move || {
                    let range = (k * chunk).min(tris.len())..((k + 1) * chunk).min(tris.len());
                    let mut neighbours = vec![[(NONE, false); 3]; range.len()];
                    let mut boundary = vec![false; range.len()];
                    let mut non_manifold = Vec::new();
                    for (local, t) in range.enumerate() {
                        let t = t as u32;
                        let tri = tris[t as usize];
                        for slot in 0..3 {
                            let (u, v) = (tri[slot], tri[(slot + 1) % 3]);
                            let (mut others, mut found, mut lowest) = (0, (NONE, false), t);
                            for &o in &incident[start[u as usize] as usize..start[u as usize + 1] as usize] {
                                if o == t {
                                    continue;
                                }
                                let other = tris[o as usize];
                                let Some(at) = other.iter().position(|&x| x == u) else { continue };
                                let same = other[(at + 1) % 3] == v;
                                if same || other[(at + 2) % 3] == v {
                                    others += 1;
                                    found = (o, same);
                                    lowest = lowest.min(o);
                                }
                            }
                            match others {
                                0 => boundary[local] = true,
                                1 => neighbours[local][slot] = found,
                                _ if lowest == t => non_manifold.push((u, v)),
                                _ => {}
                            }
                        }
                    }
                    Adjacency { neighbours, on_boundary: boundary, non_manifold }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("an adjacency thread panicked")).collect()
    });
    let mut out = Adjacency { neighbours: Vec::with_capacity(tris.len()), on_boundary: Vec::with_capacity(tris.len()), non_manifold: Vec::new() };
    for part in parts {
        out.neighbours.extend(part.neighbours);
        out.on_boundary.extend(part.on_boundary);
        out.non_manifold.extend(part.non_manifold);
    }
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
