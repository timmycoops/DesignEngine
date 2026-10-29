use std::collections::{HashMap, HashSet, VecDeque};

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

/// The mesh cut into its patches (triangles joined across edges with exactly two faces), in the
/// order of their first triangles, each with its own copy of the vertices it uses. Pieces that meet
/// only along edges of three or more faces come apart; no coordinate changes.
pub fn split_patches(mesh: &TriMesh) -> Vec<TriMesh> {
    let Adjacency { neighbours, .. } = adjacency(mesh);
    let mut patch_of = vec![u32::MAX; mesh.triangles.len()];
    let mut members: Vec<Vec<u32>> = Vec::new();
    let mut queue = VecDeque::new();
    for seed in 0..mesh.triangles.len() {
        if patch_of[seed] != u32::MAX {
            continue;
        }
        let patch = members.len() as u32;
        let mut list = vec![seed as u32];
        patch_of[seed] = patch;
        queue.push_back(seed);
        while let Some(t) = queue.pop_front() {
            for &(u, _) in &neighbours[t] {
                if u != NONE && patch_of[u as usize] == u32::MAX {
                    patch_of[u as usize] = patch;
                    list.push(u);
                    queue.push_back(u as usize);
                }
            }
        }
        list.sort_unstable();
        members.push(list);
    }
    let mut renumber = vec![u32::MAX; mesh.vertices.len()];
    members
        .iter()
        .map(|list| {
            let mut vertices = Vec::new();
            let triangles = list
                .iter()
                .map(|&t| {
                    mesh.triangles[t as usize].map(|v| {
                        let slot = &mut renumber[v as usize];
                        if *slot == u32::MAX {
                            *slot = vertices.len() as u32;
                            vertices.push(mesh.vertices[v as usize]);
                        }
                        *slot
                    })
                })
                .collect();
            // Ready for the next patch, which may use some of the same vertices.
            for t in list {
                for v in mesh.triangles[*t as usize] {
                    renumber[v as usize] = u32::MAX;
                }
            }
            TriMesh::new(vertices, triangles)
        })
        .collect()
}

/// What [`separate_touching_edges`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Separation {
    /// Edges with three or more faces whose faces paired up into sheets, and were taken apart.
    pub edges: usize,
    /// Vertices copied so each fan of faces round them has its own.
    pub vertex_copies: usize,
}

/// Takes apart edges with three or more faces where the faces pair up into sheets round solid
/// material: two closed pieces, or two parts of one, touching along an edge. The faces round such an
/// edge are sorted by angle and paired across each wedge of material (their outward winding says
/// which wedges those are); then each vertex whose faces fall into separate fans gets a copy for each
/// fan. No coordinate moves. An edge whose faces do not pair up is left as it is.
pub fn separate_touching_edges(mesh: &TriMesh) -> (TriMesh, Separation) {
    let tris = &mesh.triangles;
    // Each edge, by its vertices low then high, with the faces on it.
    let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for (f, t) in tris.iter().enumerate() {
        for s in 0..3 {
            let (a, b) = (t[s], t[(s + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(f as u32);
        }
    }
    // The face across each face's edge, where there is exactly one (keyed by face and edge).
    let mut across: HashMap<(u32, u32, u32), u32> = HashMap::new();
    let mut separation = Separation::default();
    let mut ends = HashSet::new();
    for (&(u, v), faces) in &edges {
        let pairs = match faces.len() {
            2 => vec![(faces[0], faces[1])],
            n if n >= 4 && n % 2 == 0 => match pair_round_edge(mesh, u, v, faces) {
                Some(pairs) => {
                    separation.edges += 1;
                    ends.extend([u, v]);
                    pairs
                }
                None => continue,
            },
            _ => continue,
        };
        for (f, g) in pairs {
            across.insert((f, u, v), g);
            across.insert((g, u, v), f);
        }
    }

    // Fans round each end of an edge taken apart: its faces joined across the edges they share there.
    // Every other vertex is left as it is.
    let mut incident: Vec<Vec<u32>> = vec![Vec::new(); mesh.vertices.len()];
    for (f, t) in tris.iter().enumerate() {
        for &v in t.iter().filter(|v| ends.contains(*v)) {
            incident[v as usize].push(f as u32);
        }
    }
    let mut vertices = mesh.vertices.clone();
    let mut triangles = tris.clone();
    for (v, faces) in incident.iter().enumerate().filter(|(_, faces)| !faces.is_empty()) {
        let v = v as u32;
        let mut root: Vec<usize> = (0..faces.len()).collect();
        fn find(root: &mut [usize], i: usize) -> usize {
            let mut i = i;
            while root[i] != i {
                root[i] = root[root[i]];
                i = root[i];
            }
            i
        }
        for (i, &f) in faces.iter().enumerate() {
            let t = tris[f as usize];
            let k = t.iter().position(|&x| x == v).expect("an incident face holds the vertex");
            for o in [t[(k + 1) % 3], t[(k + 2) % 3]] {
                let Some(&g) = across.get(&(f, v.min(o), v.max(o))) else { continue };
                if let Some(j) = faces.iter().position(|&x| x == g) {
                    let (a, b) = (find(&mut root, i), find(&mut root, j));
                    root[a] = b;
                }
            }
        }
        // The fan of the first face keeps the vertex; each other fan gets a copy.
        let mut copy_of: HashMap<usize, u32> = HashMap::new();
        let first = find(&mut root, 0);
        for (i, &f) in faces.iter().enumerate() {
            let r = find(&mut root, i);
            if r == first {
                continue;
            }
            let copy = *copy_of.entry(r).or_insert_with(|| {
                vertices.push(mesh.vertices[v as usize]);
                separation.vertex_copies += 1;
                vertices.len() as u32 - 1
            });
            let f = f as usize;
            let k = tris[f].iter().position(|&x| x == v).unwrap();
            triangles[f][k] = copy;
        }
    }
    (TriMesh::new(vertices, triangles), separation)
}

/// Pairs the faces round edge `u`-`v` across the wedges of material between them, or `None` if they
/// do not alternate between material and empty space as consistently wound sheets do.
fn pair_round_edge(mesh: &TriMesh, u: u32, v: u32, faces: &[u32]) -> Option<Vec<(u32, u32)>> {
    let pu = mesh.vertices[u as usize];
    let along = sub(mesh.vertices[v as usize], pu);
    let length = dot(along, along).sqrt();
    if length == 0.0 {
        return None;
    }
    let d = along.map(|c| c / length);
    // A frame square to the edge.
    let axis = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let x = unit(cross(d, axis))?;
    let y = cross(d, x);
    // Each face: its angle round the edge, the direction from the edge to its third corner, and its normal.
    let mut round: Vec<(f64, [f64; 3], [f64; 3], u32)> = faces
        .iter()
        .map(|&f| {
            let t = mesh.triangles[f as usize];
            let w = *t.iter().find(|&&c| c != u && c != v)?;
            let to_w = sub(mesh.vertices[w as usize], pu);
            let out = unit(sub(to_w, d.map(|c| c * dot(to_w, d))))?;
            let [a, b, c] = t.map(|i| sub(mesh.vertices[i as usize], pu));
            let normal = unit(cross(sub(b, a), sub(c, a)))?;
            Some((dot(out, y).atan2(dot(out, x)), out, normal, f))
        })
        .collect::<Option<_>>()?;
    round.sort_by(|p, q| p.0.total_cmp(&q.0));
    let n = round.len();
    let mut pairs = Vec::with_capacity(n / 2);
    let mut paired = vec![false; n];
    for i in 0..n {
        let j = (i + 1) % n;
        let (_, ei, ni, fi) = round[i];
        let (_, ej, nj, fj) = round[j];
        // Turning from face i towards face j about the edge, then back: material lies in the wedge
        // when both faces' outward normals point away from it.
        let solid_from_i = dot(ni, cross(d, ei)) < 0.0;
        let solid_from_j = dot(nj, cross(ej, d)) < 0.0;
        if solid_from_i != solid_from_j {
            return None;
        }
        if solid_from_i {
            if paired[i] || paired[j] {
                return None;
            }
            paired[i] = true;
            paired[j] = true;
            pairs.push((fi, fj));
        }
    }
    paired.iter().all(|p| *p).then_some(pairs)
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = dot(v, v).sqrt();
    (length > 0.0).then(|| v.map(|c| c / length))
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
    fn closed_pieces_meeting_along_an_edge_come_apart() {
        // Two 10 m cubes meeting along one vertical edge, sharing its two vertices: the edge has
        // four faces, and each cube is a patch of its own.
        let cube = |x: f64, y: f64| -> Vec<[f64; 3]> { (0..8).map(|i| [x + (i & 1) as f64 * 10.0, y + (i >> 1 & 1) as f64 * 10.0, (i >> 2) as f64 * 10.0]).collect() };
        let faces = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        let mut vertices = cube(0.0, 0.0);
        // The second cube's corners 0 and 4 are the first's 3 and 7.
        let second = cube(10.0, 10.0);
        let mut index = [0u32; 8];
        for (i, p) in second.iter().enumerate() {
            index[i] = match i {
                0 => 3,
                4 => 7,
                _ => {
                    vertices.push(*p);
                    vertices.len() as u32 - 1
                }
            };
        }
        let mut triangles: Vec<[u32; 3]> = faces.to_vec();
        triangles.extend(faces.iter().map(|f| f.map(|i| index[i as usize])));
        let both = TriMesh::new(vertices, triangles);
        assert_eq!(both.validity().non_manifold_edges, 1);
        let parts = split_patches(&both);
        assert_eq!(parts.len(), 2);
        for part in &parts {
            assert_eq!(part.vertices.len(), 8);
            assert!(part.validity().is_watertight());
            assert!((part.signed_volume() - 1000.0).abs() < 1e-9);
        }
        assert_eq!(parts[1].vertices[0], [10.0, 10.0, 0.0], "the shared corner is copied, not moved");
        assert_eq!(split_patches(&parts[0]).len(), 1);
    }

    /// The outer surface of a set of unit cells (x, y, z), scaled by `size`, wound outward.
    fn cells_surface(cells: &[[i32; 3]], size: f64) -> TriMesh {
        let filled = |c: [i32; 3]| cells.contains(&c);
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        let mut index = |p: [i32; 3]| -> u32 {
            let q = p.map(|c| c as f64 * size);
            match vertices.iter().position(|v| *v == q) {
                Some(i) => i as u32,
                None => {
                    vertices.push(q);
                    vertices.len() as u32 - 1
                }
            }
        };
        let mut triangles = Vec::new();
        for &c in cells {
            for axis in 0..3 {
                for side in [0, 1] {
                    let mut n = c;
                    n[axis] += if side == 1 { 1 } else { -1 };
                    if filled(n) {
                        continue;
                    }
                    // The face's corners, counter-clockwise seen from outside.
                    let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
                    let corner = |da: i32, db: i32| {
                        let mut p = c;
                        p[axis] += side;
                        p[a] += da;
                        p[b] += db;
                        p
                    };
                    let mut quad = [corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1)];
                    if side == 0 {
                        quad.reverse();
                    }
                    let q = quad.map(&mut index);
                    triangles.push([q[0], q[1], q[2]]);
                    triangles.push([q[0], q[2], q[3]]);
                }
            }
        }
        TriMesh::new(vertices, triangles)
    }

    #[test]
    fn a_solid_touching_itself_along_an_edge_is_taken_apart_there_and_stays_whole() {
        // A ring of 10 m cells round an empty centre, one corner cell missing: the two cells beside
        // that corner touch along one vertical edge, and the ring is still one piece.
        let cells: Vec<[i32; 3]> = (0..3).flat_map(|x| (0..3).map(move |y| [x, y, 0])).filter(|c| *c != [1, 1, 0] && *c != [0, 0, 0]).collect();
        let ring = cells_surface(&cells, 10.0);
        let before = ring.validity();
        assert!(before.closed && before.consistent_winding && before.non_manifold_edges == 1);
        let (apart, separation) = separate_touching_edges(&ring);
        assert_eq!(separation, Separation { edges: 1, vertex_copies: 2 });
        assert!(apart.validity().is_watertight(), "{:?}", apart.validity());
        assert_eq!(split_patches(&apart).len(), 1, "one piece");
        assert!((apart.signed_volume() - 7000.0).abs() < 1e-9);
        assert!(apart.vertices[ring.vertices.len()..].iter().all(|p| p[0] == 10.0 && p[1] == 10.0), "copies sit exactly on the edge's ends");

        // Two cells meeting only along an edge come apart into two pieces.
        let pair = cells_surface(&[[0, 0, 0], [1, 1, 0]], 10.0);
        let (apart, separation) = separate_touching_edges(&pair);
        assert_eq!(separation.edges, 1);
        let pieces = split_patches(&apart);
        assert_eq!(pieces.len(), 2);
        assert!(pieces.iter().all(|p| p.validity().is_watertight() && (p.signed_volume() - 1000.0).abs() < 1e-9));

        // Three fins on an edge do not pair up: left as they are.
        let fins = TriMesh::new(
            vec![[0.0, 0.0, 0.0], [0.0, 0.0, 10.0], [10.0, 0.0, 5.0], [0.0, 10.0, 5.0], [-10.0, 0.0, 5.0]],
            vec![[0, 1, 2], [0, 1, 3], [1, 0, 4]],
        );
        assert_eq!(separate_touching_edges(&fins).1, Separation::default());
    }

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
