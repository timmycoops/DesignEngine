use std::collections::{HashMap, HashSet, VecDeque};

use zd_core::TriMesh;

use crate::{GeomError, worker_threads};

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
    /// Closed patches inside an odd number of the others, wound to face in: cavities, with the
    /// material round them. A piece inside a cavity faces out again.
    pub cavities: usize,
}

/// Most sites listed for one kind of problem.
pub const SITES: usize = 100;

/// Makes winding consistent across each connected patch by flood fill, then turns each patch to
/// face out if it is closed (positive volume) or up if it is open (area-weighted normal). A closed
/// patch inside an odd number of the others is a cavity and is turned to face in (decision 0010,
/// amended). Edges with three or more faces stop the fill and are listed; a patch that cannot
/// be wound consistently is left as read and counted. Nothing is refused, unlike [`orient_upward`].
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
    // Closed patches are wound once it is known which lie inside others.
    let mut shells = Vec::new();
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
        if closed {
            shells.push(Shell { triangles: patch, turn: turn_patch, volume: measure.abs() });
            continue;
        }
        for &t in &patch {
            if flip[t].unwrap() ^ turn_patch {
                let [a, b, c] = oriented.triangles[t];
                oriented.triangles[t] = [a, c, b];
                log.triangles_flipped += 1;
            }
        }
    }
    let inward = if shells.len() > 1 { odd_depth(mesh, &shells, &neighbours) } else { vec![false; shells.len()] };
    for (shell, inward) in shells.iter().zip(inward) {
        log.cavities += usize::from(inward);
        for &t in &shell.triangles {
            if flip[t].unwrap() ^ shell.turn ^ inward {
                let [a, b, c] = oriented.triangles[t];
                oriented.triangles[t] = [a, c, b];
                log.triangles_flipped += 1;
            }
        }
    }
    (oriented, log)
}

/// A closed patch waiting to be wound: its triangles, whether the fill's winding is turned to face
/// out, and six times the volume it encloses.
struct Shell {
    triangles: Vec<usize>,
    turn: bool,
    volume: f64,
}

/// A corner this close to another patch's surface along the ray (m) is on it, and says nothing
/// about which side of it the patch lies.
const ON_SURFACE: f64 = 1e-6;

/// Whether each closed patch lies inside an odd number of the others, so bounds a cavity (or a
/// cavity in a piece inside a cavity). A patch is inside another when its box is, it encloses less,
/// and every corner it does not share with the other is inside it by ray parity, with at least one
/// such corner. A patch with corners both in and out crosses the other: it is not inside it, and is
/// left facing out for the crossing check to find. Only patches whose every edge is used an even
/// number of times by their own triangles count, on either side: a sheet that is "closed" only
/// because its open edges have three or more faces encloses nothing.
fn odd_depth(mesh: &TriMesh, shells: &[Shell], neighbours: &[[(u32, bool); 3]]) -> Vec<bool> {
    // Corners in a frame whose third axis is the rays' direction, about the mesh's centre.
    let origin = mesh.bounds().map_or([0.0; 3], |b| b.centre());
    let frame = ray_frame();
    let at: Vec<[f64; 3]> = mesh
        .vertices
        .iter()
        .map(|p| {
            let d = sub(*p, origin);
            frame.map(|axis| dot(d, axis))
        })
        .collect();
    let sealed: Vec<bool> = shells.iter().map(|s| closes(mesh, &s.triangles, neighbours)).collect();
    let boxes: Vec<[[f64; 3]; 2]> = shells
        .iter()
        .map(|s| {
            s.triangles.iter().flat_map(|&t| mesh.triangles[t]).fold([[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]], |[lo, hi], v| {
                let p = at[v as usize];
                [[0, 1, 2].map(|i| lo[i].min(p[i])), [0, 1, 2].map(|i| hi[i].max(p[i]))]
            })
        })
        .collect();
    let within = |p: usize, q: usize| (0..3).all(|i| boxes[p][0][i] >= boxes[q][0][i] && boxes[p][1][i] <= boxes[q][1][i]);

    let mut depth = vec![0usize; shells.len()];
    let mut on_outer = vec![false; mesh.vertices.len()];
    let mut stamp = vec![0u32; mesh.vertices.len()];
    let mut visit = 0u32;
    for (q, outer) in shells.iter().enumerate().filter(|(q, _)| sealed[*q]) {
        let inner: Vec<usize> = (0..shells.len()).filter(|&p| p != q && sealed[p] && shells[p].volume < outer.volume && within(p, q)).collect();
        if inner.is_empty() {
            continue;
        }
        outer.triangles.iter().flat_map(|&t| mesh.triangles[t]).for_each(|v| on_outer[v as usize] = true);
        // Each candidate's corners the outer patch does not share, once each, with whose they are.
        let (mut owner, mut points) = (Vec::new(), Vec::new());
        for (k, &p) in inner.iter().enumerate() {
            visit += 1;
            for v in shells[p].triangles.iter().flat_map(|&t| mesh.triangles[t]) {
                if !on_outer[v as usize] && stamp[v as usize] != visit {
                    stamp[v as usize] = visit;
                    owner.push(k);
                    points.push(at[v as usize]);
                }
            }
        }
        let mut tally = vec![(0usize, 0usize); inner.len()];
        for (k, side) in owner.into_iter().zip(sides(mesh, &outer.triangles, &at, &points)) {
            match side {
                Some(true) => tally[k].0 += 1,
                Some(false) => tally[k].1 += 1,
                None => {}
            }
        }
        for (&p, (inside, outside)) in inner.iter().zip(tally) {
            depth[p] += usize::from(inside > 0 && outside == 0);
        }
        outer.triangles.iter().flat_map(|&t| mesh.triangles[t]).for_each(|v| on_outer[v as usize] = false);
    }
    depth.iter().map(|d| d % 2 == 1).collect()
}

/// Whether every edge of a closed patch is used an even number of times by its own triangles: it
/// encloses a volume. An edge with a neighbour across is used twice; only edges of three or more
/// faces, with none, need counting.
fn closes(mesh: &TriMesh, patch: &[usize], neighbours: &[[(u32, bool); 3]]) -> bool {
    let mut edges: Vec<(u32, u32)> = patch
        .iter()
        .flat_map(|&t| (0..3).filter(move |&s| neighbours[t][s].0 == NONE).map(move |s| (t, s)))
        .map(|(t, s)| {
            let (u, v) = (mesh.triangles[t][s], mesh.triangles[t][(s + 1) % 3]);
            (u.min(v), u.max(v))
        })
        .collect();
    edges.sort_unstable();
    edges.chunk_by(|x, y| x == y).all(|run| run.len() % 2 == 0)
}

/// The nesting test's axes: the third is the rays' direction, a little off the vertical so that no
/// face of a solid drawn square to the grid lies along it; the first two span the plane the
/// patches are seen on along it.
fn ray_frame() -> [[f64; 3]; 3] {
    let along = unit([0.0923, 0.0371, 1.0]).expect("a direction");
    let across = unit(cross([0.0, 1.0, 0.0], along)).expect("not along north");
    [across, cross(along, across), along]
}

/// For each point (in the ray frame), whether it is inside the closed surface of `triangles`: an odd
/// number of them cross the ray from it. `None` where it lies on the surface. The points are
/// bucketed on a grid in the plane square to the rays, so each triangle meets only those it might
/// cover.
fn sides(mesh: &TriMesh, triangles: &[usize], at: &[[f64; 3]], points: &[[f64; 3]]) -> Vec<Option<bool>> {
    let n = (points.len() as f64).sqrt().ceil().max(1.0) as usize;
    let lo = [0, 1].map(|i| points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
    let hi = [0, 1].map(|i| points.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max));
    let size = [0, 1].map(|i| if hi[i] > lo[i] { (hi[i] - lo[i]) / n as f64 } else { 1.0 });
    let cell = |c: f64, i: usize| (((c - lo[i]) / size[i]).floor().max(0.0) as usize).min(n - 1);
    let mut start = vec![0usize; n * n + 1];
    let home: Vec<usize> = points.iter().map(|p| cell(p[1], 1) * n + cell(p[0], 0)).collect();
    home.iter().for_each(|&h| start[h + 1] += 1);
    for i in 0..n * n {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut order = vec![0usize; points.len()];
    for (k, &h) in home.iter().enumerate() {
        order[fill[h]] = k;
        fill[h] += 1;
    }

    let mut crossings = vec![0u32; points.len()];
    let mut on = vec![false; points.len()];
    for &t in triangles {
        let tri = mesh.triangles[t];
        let corners = tri.map(|v| at[v as usize]);
        let low = [0, 1].map(|i| corners.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min));
        let high = [0, 1].map(|i| corners.iter().map(|p| p[i]).fold(f64::NEG_INFINITY, f64::max));
        if (0..2).any(|i| high[i] < lo[i] || low[i] > hi[i]) {
            continue;
        }
        for row in cell(low[1], 1)..=cell(high[1], 1) {
            for column in cell(low[0], 0)..=cell(high[0], 0) {
                for &k in &order[start[row * n + column]..start[row * n + column + 1]] {
                    let q = points[k];
                    if (0..2).any(|i| q[i] < low[i] || q[i] > high[i]) {
                        continue;
                    }
                    match depth_beyond(tri, at, q) {
                        Some(d) if d.abs() <= ON_SURFACE => on[k] = true,
                        Some(d) if d > 0.0 => crossings[k] += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    crossings.iter().zip(on).map(|(&n, on)| (!on).then_some(n % 2 == 1)).collect()
}

/// How far along the ray from `q` triangle `tri` is (negative behind it), if the ray meets it; all
/// in the ray frame. Seen along the ray, `q` is taken as nudged by (ε, ε²), so it is never on an edge
/// or corner: of two triangles either side of an edge exactly one holds it, and two folded over
/// each other hold it both or neither, which leaves the parity right.
fn depth_beyond(tri: [u32; 3], at: &[[f64; 3]], q: [f64; 3]) -> Option<f64> {
    let [a, b, c] = tri;
    let side = [edge_side(at, a, b, q), edge_side(at, b, c, q), edge_side(at, c, a, q)];
    if side[0] == 0 || side[0] != side[1] || side[1] != side[2] {
        return None;
    }
    let [pa, pb, pc] = tri.map(|v| at[v as usize]);
    let area = orient(pa, pb, pc);
    if area == 0.0 {
        return None;
    }
    let (wa, wb) = (orient(pb, pc, q) / area, orient(pc, pa, q) / area);
    Some(wa * pa[2] + wb * pb[2] + (1.0 - wa - wb) * pc[2] - q[2])
}

/// Which side of the edge from corner `i` to `j` the nudged `q` is seen on: 1 left, -1 right, 0 if
/// the edge has no length seen along the ray. Worked out from the lower-numbered corner, so both
/// triangles on an edge see `q` on the same side of it.
fn edge_side(at: &[[f64; 3]], i: u32, j: u32, q: [f64; 3]) -> i8 {
    let (low, high, sign) = if i < j { (i, j, 1) } else { (j, i, -1) };
    let (a, b) = (at[low as usize], at[high as usize]);
    let (du, dv) = (b[0] - a[0], b[1] - a[1]);
    let o = du * (q[1] - a[1]) - dv * (q[0] - a[0]);
    // On the line, the nudge decides: -dv ε first, then du ε².
    let s = [o, -dv, du].into_iter().find(|x| *x != 0.0).map_or(0, |x| if x > 0.0 { 1 } else { -1 });
    s * sign
}

/// Twice the signed area of a, b, c seen along the ray.
fn orient(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
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
    /// Edges where one piece touches itself with a single fan round each end, so copies cannot take
    /// them apart: one side's two faces are split at a vertex added at the edge's midpoint, on the
    /// edge, so the sides no longer share it.
    pub edges_split: usize,
}

/// Takes apart edges with three or more faces where the faces pair up into sheets round solid
/// material: two closed pieces, or two parts of one, touching along an edge. The faces round such an
/// edge are sorted by angle and paired across each wedge of material (their outward winding says
/// which wedges those are); then each vertex whose faces fall into separate fans gets a copy for each
/// fan. Where one piece touches itself and its faces round both ends of the edge form one fan, the
/// pairs would still share the edge: each pair after the first has its two faces split at a vertex
/// added at the edge's midpoint. No coordinate moves. An edge whose faces do not pair up is left as
/// it is.
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
    // Each edge taken apart, with the faces paired round it.
    let mut taken_apart: Vec<((u32, u32), Pairs)> = Vec::new();
    for (&(u, v), faces) in &edges {
        let pairs = match faces.len() {
            2 => vec![(faces[0], faces[1])],
            n if n >= 4 && n % 2 == 0 => match pair_round_edge(mesh, u, v, faces) {
                Some(pairs) => {
                    separation.edges += 1;
                    ends.extend([u, v]);
                    taken_apart.push(((u, v), pairs.clone()));
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

    // Pairs still on the same two vertices (one piece touching itself, one fan at each end): every
    // pair after the first is split at a vertex of its own at the edge's midpoint. Copies only ever
    // replace a vertex in place, so each face's corners keep their slots.
    taken_apart.sort_unstable_by_key(|(edge, _)| *edge);
    let mut split = HashSet::new();
    for ((u, v), pairs) in taken_apart {
        let mut seen = HashSet::new();
        let mut done = false;
        for (f, g) in pairs {
            let ends_of = |face: u32| {
                let t = tris[face as usize];
                let (ku, kv) = (t.iter().position(|&x| x == u).unwrap(), t.iter().position(|&x| x == v).unwrap());
                let (a, b) = (triangles[face as usize][ku], triangles[face as usize][kv]);
                (a.min(b), a.max(b))
            };
            if seen.insert(ends_of(f)) || split.contains(&f) || split.contains(&g) {
                continue;
            }
            let [pu, pv] = [u, v].map(|i| mesh.vertices[i as usize]);
            vertices.push([0, 1, 2].map(|k| 0.5 * (pu[k] + pv[k])));
            let m = vertices.len() as u32 - 1;
            for face in [f, g] {
                let t = tris[face as usize];
                let (ku, kv) = (t.iter().position(|&x| x == u).unwrap(), t.iter().position(|&x| x == v).unwrap());
                // The slot the edge starts from, in the face's own winding.
                let s = if (ku + 1) % 3 == kv { ku } else { kv };
                let now = triangles[face as usize];
                let [a, b, c] = [now[s], now[(s + 1) % 3], now[(s + 2) % 3]];
                triangles[face as usize] = [a, m, c];
                triangles.push([m, b, c]);
                split.insert(face);
            }
            done = true;
        }
        separation.edges_split += usize::from(done);
    }
    (TriMesh::new(vertices, triangles), separation)
}

/// Pairs the faces round edge `u`-`v` across the wedges of material between them, or `None` if they
/// do not alternate between material and empty space as consistently wound sheets do.
/// Faces paired across the wedges of material round an edge.
type Pairs = Vec<(u32, u32)>;

fn pair_round_edge(mesh: &TriMesh, u: u32, v: u32, faces: &[u32]) -> Option<Pairs> {
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

    let threads = worker_threads(tris.len());
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
        assert_eq!(separation, Separation { edges: 1, vertex_copies: 2, edges_split: 0 });
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
    fn a_slot_pinched_to_an_edge_is_split_at_the_midpoint_and_keeps_its_volume() {
        // A slab of 10 m cells, 4 x 3 x 2, with one cell out of the top layer and one out of the
        // bottom layer, diagonal to each other: the two pockets meet along one edge, where the solid
        // touches itself. The slab is one patch, and round each end of the edge the pockets' end
        // walls join the faces into one fan, so copying the ends cannot take the edge apart.
        let cells: Vec<[i32; 3]> = (0..4)
            .flat_map(|x| (0..3).flat_map(move |y| (0..2).map(move |z| [x, y, z])))
            .filter(|c| *c != [1, 1, 1] && *c != [2, 1, 0])
            .collect();
        let slab = cells_surface(&cells, 10.0);
        let before = slab.validity();
        assert!(before.closed && before.consistent_winding && before.non_manifold_edges == 1, "{before:?}");
        assert_eq!(split_patches(&slab).len(), 1);
        let (apart, separation) = separate_touching_edges(&slab);
        assert_eq!(separation, Separation { edges: 1, vertex_copies: 0, edges_split: 1 });
        assert!(apart.validity().is_watertight(), "{:?}", apart.validity());
        assert_eq!(split_patches(&apart).len(), 1, "still one piece");
        assert!((apart.signed_volume() - 22_000.0).abs() < 1e-9, "{}", apart.signed_volume());
        assert_eq!(apart.vertices.len(), slab.vertices.len() + 1);
        assert_eq!(apart.vertices.last(), Some(&[20.0, 15.0, 10.0]), "the added vertex is the edge's midpoint");
        assert!(slab.vertices.iter().zip(&apart.vertices).all(|(p, q)| p == q), "no vertex moves");
    }

    /// A box's faces, wound outward, on corners numbered x + 2y + 4z.
    const BOX_FACES: [[u32; 3]; 12] = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];

    /// A box from `low` of `size`, wound outward.
    fn block(low: [f64; 3], size: [f64; 3]) -> TriMesh {
        let corners = (0..8).map(|i| [low[0] + (i & 1) as f64 * size[0], low[1] + (i >> 1 & 1) as f64 * size[1], low[2] + (i >> 2) as f64 * size[2]]).collect();
        TriMesh::new(corners, BOX_FACES.to_vec())
    }

    /// The pieces in one mesh, each keeping its own vertices.
    fn joined(pieces: &[TriMesh]) -> TriMesh {
        let (mut vertices, mut triangles) = (Vec::new(), Vec::new());
        for piece in pieces {
            let base = vertices.len() as u32;
            vertices.extend(&piece.vertices);
            triangles.extend(piece.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        TriMesh::new(vertices, triangles)
    }

    #[test]
    fn a_piece_inside_a_cavity_inside_a_solid_faces_out_in_out_in() {
        // A 100 x 100 x 20 m block with a 60 x 60 x 10 m cavity, a 20 x 20 x 4 m piece in the cavity
        // and a 10 x 10 x 2 m cavity in that, each read wound the wrong way for its depth.
        let pieces = [
            block([0.0, 0.0, 0.0], [100.0, 100.0, 20.0]).flipped(),
            block([20.0, 20.0, 5.0], [60.0, 60.0, 10.0]),
            block([40.0, 40.0, 8.0], [20.0, 20.0, 4.0]).flipped(),
            block([45.0, 45.0, 9.0], [10.0, 10.0, 2.0]),
        ];
        let (oriented, log) = orient_patches(&joined(&pieces));
        assert_eq!((log.closed_patches, log.cavities, log.triangles_flipped), (4, 2, 48));
        let volumes: Vec<f64> = split_patches(&oriented).iter().map(TriMesh::signed_volume).collect();
        let expected = [200_000.0, -36_000.0, 1_600.0, -200.0];
        // Exact to a micro-cubic metre: every corner is on the metre grid.
        assert!(volumes.iter().zip(expected).all(|(v, e)| (v - e).abs() < 1e-6), "{volumes:?}");
        assert!((oriented.signed_volume() - 165_400.0).abs() < 1e-6, "{}", oriented.signed_volume());
    }

    #[test]
    fn separate_pieces_face_out_even_inside_anothers_box() {
        // Two 10 m boxes side by side: 2,000 m3.
        let (apart, log) = orient_patches(&joined(&[block([0.0; 3], [10.0; 3]), block([20.0, 0.0, 0.0], [10.0; 3]).flipped()]));
        assert_eq!(log.cavities, 0);
        assert!((apart.signed_volume() - 2_000.0).abs() < 1e-6, "{}", apart.signed_volume());

        // An L of three 10 m cells with a 6 m box in its notch: the box is inside the L's box, and
        // smaller, but outside it, so it faces out: 3,000 + 216 m3.
        let l = cells_surface(&[[0, 0, 0], [1, 0, 0], [0, 1, 0]], 10.0);
        let (notched, log) = orient_patches(&joined(&[l.clone(), block([12.0, 12.0, 2.0], [6.0; 3]).flipped()]));
        assert_eq!((log.closed_patches, log.cavities), (2, 0));
        assert!((notched.signed_volume() - 3_216.0).abs() < 1e-6, "{}", notched.signed_volume());

        // A 10 x 10 x 6 m box across the L's inside corner, three corners in the L and one in the
        // notch, crosses it: not inside, so left facing out for the crossing check, 3,000 + 600 m3.
        let (crossing, log) = orient_patches(&joined(&[l, block([5.0, 5.0, 2.0], [10.0, 10.0, 6.0]).flipped()]));
        assert_eq!(log.cavities, 0);
        assert!((crossing.signed_volume() - 3_600.0).abs() < 1e-6, "{}", crossing.signed_volume());
    }

    #[test]
    fn a_patch_closed_only_by_edges_of_three_faces_holds_nothing() {
        // Two 10 m cells with a wall between them: the wall's edges have three faces, so the box
        // comes apart into two open halves and the wall, each "closed" (no edge of one face) but
        // none enclosing anything. A tetrahedron inside the first cell is in no closed surface:
        // it keeps facing out, as read.
        let mut cells = cells_surface(&[[0, 0, 0], [1, 0, 0]], 10.0);
        let corner = |p: [f64; 3]| cells.vertices.iter().position(|v| *v == p).unwrap() as u32;
        let wall = [[10.0, 0.0, 0.0], [10.0, 10.0, 0.0], [10.0, 10.0, 10.0], [10.0, 0.0, 10.0]].map(corner);
        cells.triangles.extend([[wall[0], wall[1], wall[2]], [wall[0], wall[2], wall[3]]]);
        let tetra = TriMesh::new(vec![[2.0, 2.0, 2.0], [5.0, 2.0, 2.0], [2.0, 5.0, 2.0], [2.0, 2.0, 5.0]], vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]);
        let (oriented, log) = orient_patches(&joined(&[cells, tetra]));
        assert_eq!((log.closed_patches, log.non_manifold_edges, log.cavities), (4, 4, 0));
        let pieces = split_patches(&oriented);
        assert!((pieces.last().unwrap().signed_volume() - 4.5).abs() < 1e-9, "the tetrahedron, 4.5 m3, faces out");
    }

    #[test]
    fn a_cavity_of_many_faces_in_a_box_of_many_faces_is_found() {
        // A 6 m cube of 1 m cells, its surface 432 triangles, round a 2 m cube of 1 m cells, 26
        // corners each tested against it: 216 - 8 m3.
        let cube = |from: i32, to: i32| -> Vec<[i32; 3]> { (from..to).flat_map(|x| (from..to).flat_map(move |y| (from..to).map(move |z| [x, y, z]))).collect() };
        let (oriented, log) = orient_patches(&joined(&[cells_surface(&cube(0, 6), 1.0), cells_surface(&cube(2, 4), 1.0)]));
        assert_eq!((log.closed_patches, log.cavities), (2, 1));
        assert!((oriented.signed_volume() - (216.0 - 8.0)).abs() < 1e-9, "{}", oriented.signed_volume());
    }

    #[test]
    #[ignore = "timing: cargo test -p zd-geom --release timing_a_slab -- --ignored --nocapture"]
    fn timing_a_slab_of_four_million_triangles_with_a_thousand_cavities() {
        // A rolling slab 2 km square and 50 m thick at mine grid coordinates: a 2 m grid on top, the
        // same 50 m lower beneath, walls round it. Then 1,000 tetrahedral cavities with 3 m legs at
        // RL 70, between the two everywhere, read facing out.
        let (e, n) = (650_000.0, 7_450_000.0);
        let top = triangulate_grid(&Grid::from_fn([e, n], [2.0, 2.0], 1001, 1001, |x, y| 100.0 + 15.0 * (x / 300.0).sin() * (y / 200.0).cos()));
        let below = top.vertices.len() as u32;
        let mut vertices = top.vertices.clone();
        vertices.extend(top.vertices.iter().map(|p| [p[0], p[1], p[2] - 50.0]));
        let mut triangles = top.triangles.clone();
        triangles.extend(top.triangles.iter().map(|&[a, b, c]| [a + below, c + below, b + below]));
        let ring = &boundary_loops(&top).unwrap()[0];
        for (k, &a) in ring.iter().enumerate() {
            let b = ring[(k + 1) % ring.len()];
            triangles.extend([[b, a, a + below], [b, a + below, b + below]]);
        }
        let slab = TriMesh::new(vertices, triangles);
        let start = std::time::Instant::now();
        let (alone, log) = orient_patches(&slab);
        let alone_ms = start.elapsed().as_secs_f64() * 1e3;
        assert_eq!((log.closed_patches, log.triangles_flipped), (1, 0));

        let tetra = |x: f64, y: f64| TriMesh::new(vec![[x, y, 70.0], [x + 3.0, y, 70.0], [x, y + 3.0, 70.0], [x, y, 73.0]], vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]);
        let cavities: Vec<TriMesh> = (0..1000).map(|k| tetra(e + 30.0 + (k % 32) as f64 * 61.7, n + 30.0 + (k / 32) as f64 * 61.3)).collect();
        let holed = joined(&[vec![slab], cavities].concat());
        let start = std::time::Instant::now();
        let (oriented, log) = orient_patches(&holed);
        let holed_ms = start.elapsed().as_secs_f64() * 1e3;
        assert_eq!((log.closed_patches, log.cavities, log.triangles_flipped), (1001, 1000, 4000));
        // The slab is 2,000 x 2,000 x 50 m wherever it is, less 1,000 x 4.5 m3; to a hundredth of a
        // cubic metre in 200 million.
        assert!((alone.signed_volume() - 2e8).abs() < 0.01, "{}", alone.signed_volume());
        assert!((oriented.signed_volume() - (2e8 - 4_500.0)).abs() < 0.01, "{}", oriented.signed_volume());
        println!(
            "{} triangles in: oriented in {alone_ms:.0} ms alone; with 1,000 cavities ({} triangles) {holed_ms:.0} ms, so {:.0} ms to find them; {} triangles out",
            alone.triangle_count(),
            holed.triangle_count(),
            holed_ms - alone_ms,
            oriented.triangle_count()
        );
    }

    #[test]
    fn a_ray_through_an_edge_or_a_corner_meets_one_triangle() {
        // A 2 m square of four triangles round its centre (corner 4), 5 m along the ray, in the ray
        // frame. Points on the edges between them and on the centre itself are each held once,
        // whichever way the triangles are wound.
        let at = [[0.0, 0.0, 5.0], [2.0, 0.0, 5.0], [2.0, 2.0, 5.0], [0.0, 2.0, 5.0], [1.0, 1.0, 5.0], [0.5, 0.0, 6.0]];
        let held = |q: [f64; 3], fan: &[[u32; 3]]| fan.iter().filter_map(|&t| depth_beyond(t, &at, q)).collect::<Vec<f64>>();
        for fan in [[[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]], [[0, 1, 4], [2, 1, 4], [2, 3, 4], [0, 3, 4]]] {
            for q in [[1.0, 1.0, 0.0], [0.5, 0.5, 0.0], [1.5, 0.5, 0.0], [1.5, 1.5, 0.0], [0.5, 1.5, 0.0], [1.0, 0.3, 0.0]] {
                let depths = held(q, &fan);
                assert!(depths.len() == 1 && (depths[0] - 5.0).abs() < 1e-12, "{q:?} in {fan:?}: {depths:?}");
            }
        }
        // Two triangles folded over each other on edge 1-4 (corners 0 and 5 on the same side of it)
        // hold a point on it both or neither.
        let folded = held([1.5, 0.5, 0.0], &[[0, 1, 4], [4, 1, 5]]).len();
        assert!(folded % 2 == 0, "{folded}");
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
