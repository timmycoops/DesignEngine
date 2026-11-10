//! One face a strip, and strip solids (decision 0049). The faces built for one strip, up and down or over several
//! runs, are joined into one surface; and a solid is sliced by the strips' faces into a solid a strip, each face closed
//! into the half-space behind it, past the solid (engine rule 5), and each strip cut from what the strips before it
//! left, so the strips add up to the solid.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::time::Instant;

use manifold_csg::{CrossSection, JoinType, Manifold, triangulate_polygons};
use zd_core::{MeshValidity, TriMesh};

use crate::cut::{TinyEdges, from_manifold, join_tiny_edges, to_manifold};
use crate::{GeomError, RepairLog, Side, boundary_loops, close_surface_to_halfspace, orient_upward, repair_mesh};

/// A gap closed all round by the faces being joined, filled.
#[derive(Debug, Clone, PartialEq)]
pub struct FilledGap {
    /// Its area in plan (m²).
    pub area: f64,
    /// Its middle.
    pub at: [f64; 3],
}

/// What [`join_faces`] did, every change counted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JoinLog {
    /// The faces joined.
    pub faces: usize,
    /// The weld: vertices within the tolerance made one, and the triangles that dropped.
    pub weld: RepairLog,
    /// Corners of one face lying on another's edge, that edge split there so the two meet corner to corner.
    pub stitched: usize,
    /// Gaps the faces closed all round, filled.
    pub filled: Vec<FilledGap>,
}

/// The faces of one strip joined into one surface, wound upward: welded where they meet within `tolerance` (m),
/// stitched where a corner of one lies on another's edge, and any gap they close all round filled. Faces that don't
/// meet, or that overlap so an edge is shared three ways, are refused.
pub fn join_faces(faces: &[&TriMesh], tolerance: f64) -> Result<(TriMesh, JoinLog), GeomError> {
    if faces.is_empty() {
        return Err(GeomError::InvalidInput("there are no faces to join".into()));
    }
    if !(tolerance.is_finite() && tolerance >= 0.0) {
        return Err(GeomError::InvalidInput(format!("a join tolerance of {tolerance} m isn't one")));
    }
    let mut all = TriMesh::default();
    for face in faces {
        if face.triangles.is_empty() {
            return Err(GeomError::InvalidInput("a face has no triangles".into()));
        }
        let base = all.vertices.len() as u32;
        all.vertices.extend_from_slice(&face.vertices);
        all.triangles.extend(face.triangles.iter().map(|t| t.map(|v| v + base)));
    }
    let (welded, weld) = repair_mesh(&all, tolerance);
    let (stitched_mesh, stitched) = stitch(welded, tolerance);
    let pieces = pieces_of(&stitched_mesh);
    if pieces.len() > 1 {
        let (gap, at) = nearest_between(&stitched_mesh, &pieces);
        return Err(GeomError::Refused(format!(
            "the faces don't meet: they're in {} pieces, the nearest {gap:.2} m apart near {}",
            pieces.len(),
            place(at)
        )));
    }
    let (mut mesh, _) = orient_upward(&stitched_mesh).map_err(|e| match e {
        GeomError::InvalidInput(why) if why.contains("shared by") => GeomError::Refused(format!("the faces overlap: {why}")),
        other => other,
    })?;
    let filled = fill_gaps(&mut mesh)?;
    Ok((mesh, JoinLog { faces: faces.len(), weld, stitched, filled }))
}

/// A triangle's edge from one corner to the next, and the corners lying on it with how far along each is.
type Split = (u32, u32, Vec<(f64, u32)>);

/// Splits each boundary edge where a boundary corner of another face lies on it within `tolerance`, so faces that
/// share a line but not its vertices meet corner to corner. A triangle is split along one edge a pass. Nothing moves.
/// A face's own pieces meet the same way where a wall has no height (`face::face_sheet`).
pub(crate) fn stitch(mut mesh: TriMesh, tolerance: f64) -> (TriMesh, usize) {
    const CELL: f64 = 5.0;
    let mut stitched = 0;
    for _ in 0..8 {
        let edges = boundary_edges(&mesh);
        if edges.is_empty() {
            break;
        }
        let key = |p: [f64; 3]| ((p[0] / CELL).floor() as i64, (p[1] / CELL).floor() as i64);
        let mut grid: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        let mut corners: Vec<u32> = edges.iter().flat_map(|e| [e.0, e.1]).collect();
        corners.sort_unstable();
        corners.dedup();
        for &v in &corners {
            grid.entry(key(mesh.vertices[v as usize])).or_default().push(v);
        }
        let mut splits: HashMap<usize, Split> = HashMap::new();
        for &(a, b, tri) in &edges {
            if splits.contains_key(&tri) {
                continue;
            }
            let (pa, pb) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
            let lo = key([pa[0].min(pb[0]) - tolerance, pa[1].min(pb[1]) - tolerance, 0.0]);
            let hi = key([pa[0].max(pb[0]) + tolerance, pa[1].max(pb[1]) + tolerance, 0.0]);
            let d = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let length2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if length2 == 0.0 {
                continue;
            }
            let mut on = Vec::new();
            for i in lo.0..=hi.0 {
                for j in lo.1..=hi.1 {
                    for &v in grid.get(&(i, j)).into_iter().flatten() {
                        if v == a || v == b {
                            continue;
                        }
                        let p = mesh.vertices[v as usize];
                        let t = ((p[0] - pa[0]) * d[0] + (p[1] - pa[1]) * d[1] + (p[2] - pa[2]) * d[2]) / length2;
                        if t <= 1e-9 || t >= 1.0 - 1e-9 {
                            continue;
                        }
                        let q = [pa[0] + t * d[0] - p[0], pa[1] + t * d[1] - p[1], pa[2] + t * d[2] - p[2]];
                        if (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt() <= tolerance {
                            on.push((t, v));
                        }
                    }
                }
            }
            if !on.is_empty() {
                on.sort_by(|x, y| x.0.total_cmp(&y.0));
                on.dedup_by_key(|x| x.1);
                splits.insert(tri, (a, b, on));
            }
        }
        if splits.is_empty() {
            break;
        }
        let mut triangles = Vec::with_capacity(mesh.triangles.len() + splits.len() * 2);
        for (t, tri) in mesh.triangles.iter().enumerate() {
            let Some((a, b, on)) = splits.get(&t) else {
                triangles.push(*tri);
                continue;
            };
            let c = tri.iter().copied().find(|v| v != a && v != b).expect("a triangle has a third corner");
            let mut run = vec![*a];
            run.extend(on.iter().map(|x| x.1));
            run.push(*b);
            stitched += on.len();
            triangles.extend(run.windows(2).map(|w| [w[0], w[1], c]));
        }
        mesh.triangles = triangles;
    }
    (mesh, stitched)
}

/// An edge from one corner to the next, with the triangle that runs it that way.
type Edge = (u32, u32, usize);

/// Each edge used by one triangle, in the direction that triangle runs it, with the triangle.
fn boundary_edges(mesh: &TriMesh) -> Vec<Edge> {
    let mut count: HashMap<(u32, u32), (usize, Edge)> = HashMap::new();
    for (t, &[x, y, z]) in mesh.triangles.iter().enumerate() {
        for (a, b) in [(x, y), (y, z), (z, x)] {
            let entry = count.entry((a.min(b), a.max(b))).or_insert((0, (a, b, t)));
            entry.0 += 1;
        }
    }
    let mut edges: Vec<Edge> = count.into_values().filter(|(n, _)| *n == 1).map(|(_, e)| e).collect();
    edges.sort_unstable();
    edges
}

/// The triangles of each connected piece, pieces joined through a shared corner.
fn pieces_of(mesh: &TriMesh) -> Vec<Vec<usize>> {
    let mut parent: Vec<u32> = (0..mesh.vertices.len() as u32).collect();
    fn root(parent: &mut [u32], mut v: u32) -> u32 {
        while parent[v as usize] != v {
            parent[v as usize] = parent[parent[v as usize] as usize];
            v = parent[v as usize];
        }
        v
    }
    for &[a, b, c] in &mesh.triangles {
        for (x, y) in [(a, b), (b, c)] {
            let (rx, ry) = (root(&mut parent, x), root(&mut parent, y));
            if rx != ry {
                parent[rx as usize] = ry;
            }
        }
    }
    let mut pieces: HashMap<u32, Vec<usize>> = HashMap::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        pieces.entry(root(&mut parent, tri[0])).or_default().push(t);
    }
    let mut pieces: Vec<Vec<usize>> = pieces.into_values().collect();
    pieces.sort_by_key(|p| p[0]);
    pieces
}

/// How far apart the first piece is from the nearest other, and where, over their boundary corners.
fn nearest_between(mesh: &TriMesh, pieces: &[Vec<usize>]) -> (f64, [f64; 3]) {
    let corners = |piece: &Vec<usize>| {
        let mut vs: Vec<u32> = piece.iter().flat_map(|&t| mesh.triangles[t]).collect();
        vs.sort_unstable();
        vs.dedup();
        vs.into_iter().map(|v| mesh.vertices[v as usize]).collect::<Vec<_>>()
    };
    let first = corners(&pieces[0]);
    let mut best = (f64::INFINITY, first[0]);
    for other in &pieces[1..] {
        for q in corners(other) {
            for p in &first {
                let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
                if d < best.0 {
                    best = (d, [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0, (p[2] + q[2]) / 2.0]);
                }
            }
        }
    }
    best
}

/// Fills each hole of an upward surface in one piece: every boundary loop but the outer one, triangulated in plan.
fn fill_gaps(mesh: &mut TriMesh) -> Result<Vec<FilledGap>, GeomError> {
    let loops = boundary_loops(mesh)?;
    if loops.len() <= 1 {
        return Ok(Vec::new());
    }
    let plan = |ring: &[u32]| ring.iter().map(|&v| [mesh.vertices[v as usize][0], mesh.vertices[v as usize][1]]).collect::<Vec<_>>();
    let areas: Vec<f64> = loops.iter().map(|ring| signed_area(&plan(ring))).collect();
    let outer = (0..loops.len()).max_by(|&i, &j| areas[i].abs().total_cmp(&areas[j].abs())).expect("there are loops");
    let mut filled = Vec::new();
    for (k, ring) in loops.iter().enumerate().filter(|(k, _)| *k != outer) {
        // A hole runs clockwise round an upward surface; its fill runs it the other way.
        let ccw: Vec<u32> = ring.iter().rev().copied().collect();
        let points = plan(&ccw);
        let tris = triangulate_polygons(std::slice::from_ref(&points), 1e-9).ok_or_else(|| {
            GeomError::Refused(format!("the faces leave a gap near {} that can't be filled in plan", place(centroid(mesh, ring))))
        })?;
        for t in tris {
            let [a, b, c] = t.map(|i| ccw[i as usize]);
            let up = signed_area(&[points[t[0] as usize], points[t[1] as usize], points[t[2] as usize]]) > 0.0;
            mesh.triangles.push(if up { [a, b, c] } else { [a, c, b] });
        }
        filled.push(FilledGap { area: areas[k].abs(), at: centroid(mesh, ring) });
    }
    Ok(filled)
}

fn centroid(mesh: &TriMesh, ring: &[u32]) -> [f64; 3] {
    let n = ring.len() as f64;
    let mut sum = [0.0; 3];
    for &v in ring {
        let p = mesh.vertices[v as usize];
        sum = [sum[0] + p[0], sum[1] + p[1], sum[2] + p[2]];
    }
    sum.map(|s| s / n)
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let mut twice = 0.0;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        twice += p[0] * q[1] - q[0] * p[1];
    }
    0.5 * twice
}

/// A place as a run reports it.
pub fn place(p: [f64; 3]) -> String {
    format!("E {:.1}, N {:.1}, RL {:.1}", p[0], p[1], p[2])
}

/// A face whose end stops inside the solid's outline, so what's past its end can't be told in front of it from behind.
#[derive(Debug, Clone, PartialEq)]
pub struct ShortEnd {
    /// The middle of its end.
    pub at: [f64; 3],
    /// About how much further its line would have to run on for its end to clear the solid (m), where the end could be
    /// found.
    pub run_on: Option<f64>,
}

/// Why a slice can't go ahead.
#[derive(Debug)]
pub enum SliceRefusal {
    /// The solid itself.
    Solid(GeomError),
    /// A face, by its place in the list given.
    Face { face: usize, error: GeomError },
    /// A face that stops short of the solid at an end.
    Short { face: usize, end: ShortEnd },
}

impl fmt::Display for SliceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SliceRefusal::Solid(e) => write!(f, "the solid: {e}"),
            SliceRefusal::Face { face, error } => write!(f, "face {}: {error}", face + 1),
            SliceRefusal::Short { face, end } => write!(f, "face {}: {}", face + 1, short_words(end)),
        }
    }
}

/// A short end in words.
pub fn short_words(end: &ShortEnd) -> String {
    match end.run_on {
        Some(more) => format!("it stops short of the solid at its end near {}: give its line about {:.0} m more run-on there", place(end.at), more.ceil()),
        None => format!("it stops short of the solid at its end near {}: give its line more run-on there", place(end.at)),
    }
}

/// What one face does to the solid on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceCut {
    /// The solid in front of this face, on the pit side (m³).
    pub front: f64,
    /// Where the face's toe line stands above the solid's floor, so below it the cut goes straight down: the most it
    /// stands above (m), and where on the line.
    pub toe_above_floor: Option<(f64, [f64; 3])>,
    /// Where the face's top line is below the solid's top, so above it the cut goes straight up: the most it's below
    /// (m), and where on the line.
    pub top_below_top: Option<(f64, [f64; 3])>,
    /// Triangles in the face.
    pub triangles: usize,
}

/// Solid in front of the faces before a face that lies behind it: the two cross inside the solid.
#[derive(Debug, Clone, PartialEq)]
pub struct Crossing {
    /// The face, by its place in the list given.
    pub face: usize,
    /// How much (m³). It stays with the strips before.
    pub volume: f64,
    /// Its middle.
    pub at: [f64; 3],
}

/// One strip, or what's behind the last face.
#[derive(Debug, Clone)]
pub struct StripPiece {
    pub mesh: TriMesh,
    pub volume: f64,
    pub validity: MeshValidity,
    /// Separate parts it's in.
    pub parts: usize,
    /// What became of edges under a micrometre the booleans left.
    pub tiny_edges: TinyEdges,
}

/// Milliseconds per stage of a slice.
#[derive(Debug, Clone, Copy, Default)]
pub struct SliceTimings {
    /// Closing every face behind, with the solid in front of each.
    pub close_ms: f64,
    /// Cutting the strips one after another.
    pub strips_ms: f64,
    pub from_manifold_ms: f64,
}

impl SliceTimings {
    pub fn total_ms(&self) -> f64 {
        self.close_ms + self.strips_ms + self.from_manifold_ms
    }
}

/// A solid sliced by faces.
#[derive(Debug, Clone)]
pub struct StripSlice {
    /// The faces from the pit out, by their place in the list given.
    pub order: Vec<usize>,
    /// The strip in front of each face in `order`, cut from what the strips before it left; then what's behind the
    /// last face.
    pub strips: Vec<StripPiece>,
    /// Each face on its own, by its place in the list given.
    pub faces: Vec<FaceCut>,
    pub crossings: Vec<Crossing>,
    pub input_volume: f64,
    pub timings: SliceTimings,
}

/// Slices a closed solid by faces into strips. Each face, wound upward so its triangles face the pit, is closed into
/// the half-space behind it past the solid: below it down to a cap under everything, and over the solid's outline
/// behind its top line from that cap to one above everything. The faces go from the pit out in the order of how much
/// of the solid lies in front of each, so it isn't their names that order them; each strip is what's in front of its
/// face of what the strips before it left, so the strips and what's behind the last face add up to the solid.
///
/// A face that stops short of the solid at an end is refused, with about how much more run-on it needs. Faces that
/// cross inside the solid are sliced anyway, the solid in front of an earlier face staying with it, and reported.
pub fn slice_solid_by_faces(solid: &TriMesh, faces: &[&TriMesh]) -> Result<StripSlice, SliceRefusal> {
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(SliceRefusal::Solid(GeomError::NotWatertight { what: "solid", validity }));
    }
    if faces.is_empty() {
        return Err(SliceRefusal::Solid(GeomError::InvalidInput("there are no faces to slice by".into())));
    }
    let solid_bounds = solid.bounds().ok_or_else(|| SliceRefusal::Solid(GeomError::InvalidInput("the solid is empty".into())))?;
    let mut face_bounds: Vec<zd_core::Aabb> = Vec::with_capacity(faces.len());
    for (face, mesh) in faces.iter().enumerate() {
        face_bounds.push(mesh.bounds().ok_or_else(|| SliceRefusal::Face { face, error: GeomError::InvalidInput("it has no triangles".into()) })?);
    }
    let lowest = face_bounds.iter().map(|b| b.min[2]).fold(solid_bounds.min[2], f64::min);
    let highest = face_bounds.iter().map(|b| b.max[2]).fold(solid_bounds.max[2], f64::max);
    let margin = 1.0 + 0.01 * (highest - lowest);
    let (z_low, z_high) = (lowest - margin, highest + margin);

    let centre = solid_bounds.centre();
    let origin = [centre[0].round(), centre[1].round(), 0.0];
    let to_local = [-origin[0], -origin[1], 0.0];
    let solid_m = to_manifold(solid, to_local, "solid").map_err(SliceRefusal::Solid)?;
    let input_volume = solid.signed_volume();
    let tolerance = (1e-6 * input_volume.abs()).max(1e-3);
    // The solid's outline in plan, a little wider, which a face's ends must run past.
    let outline = CrossSection::from_polygons(&solid_m.project()).offset(margin, JoinType::Miter, 2.0, 0);

    let start = Instant::now();
    let closed: Vec<Result<(Manifold, FaceCut), SliceRefusal>> = std::thread::scope(|scope| {
        let handles: Vec<_> = faces
            .iter()
            .enumerate()
            .map(|(face, mesh)| {
                let (solid_m, outline) = (&solid_m, &outline);
                scope.spawn(move || {
                    let local = mesh.translated(to_local);
                    let behind = behind_face(&local, outline, z_low, z_high).map_err(|refusal| match refusal {
                        Behind::Error(error) => SliceRefusal::Face { face, error },
                        Behind::Short(mut end) => {
                            end.at = [end.at[0] + origin[0], end.at[1] + origin[1], end.at[2]];
                            SliceRefusal::Short { face, end }
                        }
                    })?;
                    let (behind, edge) = behind;
                    let front = solid_m.difference(&behind);
                    front.status().map_err(|e| SliceRefusal::Face { face, error: GeomError::Manifold(e.to_string()) })?;
                    let gap = |facing: Facing| {
                        let mut most: Option<(f64, [f64; 3])> = None;
                        for &(p, _) in edge.iter().filter(|(_, f)| *f == facing) {
                            let hits = solid_m.ray_cast([p[0], p[1], z_high], [p[0], p[1], z_low]);
                            let Some((floor, top)) = hits.iter().map(|h| h.position[2]).fold(None, |m: Option<(f64, f64)>, z| Some(m.map_or((z, z), |(lo, hi)| (lo.min(z), hi.max(z))))) else {
                                continue;
                            };
                            let by = if facing == Facing::Front { p[2] - floor } else { top - p[2] };
                            if by > 0.01 && most.is_none_or(|(m, _)| by > m) {
                                most = Some((by, [p[0] + origin[0], p[1] + origin[1], p[2]]));
                            }
                        }
                        most
                    };
                    let (toe_above_floor, top_below_top) = (gap(Facing::Front), gap(Facing::Behind));
                    Ok((behind, FaceCut { front: front.volume(), toe_above_floor, top_below_top, triangles: mesh.triangle_count() }))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a face's half-space doesn't panic")).collect()
    });
    let mut halves = Vec::with_capacity(faces.len());
    let mut cuts = Vec::with_capacity(faces.len());
    for result in closed {
        let (behind, cut) = result?;
        halves.push(behind);
        cuts.push(cut);
    }
    let close_ms = ms(start);

    let start = Instant::now();
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by(|&i, &j| cuts[i].front.total_cmp(&cuts[j].front));
    let mut rest = solid_m.clone();
    let mut pieces = Vec::with_capacity(faces.len() + 1);
    let mut crossings = Vec::new();
    let mut sliced = 0.0;
    for &face in &order {
        let (behind, front) = rest.split(&halves[face]);
        for part in [&behind, &front] {
            part.status().map_err(|e| SliceRefusal::Face { face, error: GeomError::Manifold(e.to_string()) })?;
        }
        sliced += front.volume();
        // In front of every face so far is the solid less what's left; with faces that don't cross, that's what's in
        // front of this face alone.
        let crossed = sliced - cuts[face].front;
        if crossed > tolerance {
            let region = solid_m.difference(&rest).intersection(&halves[face]);
            let at = region.bounding_box().map_or([origin[0], origin[1], 0.0], |b| {
                let (lo, hi) = (b.min(), b.max());
                [(lo[0] + hi[0]) / 2.0 + origin[0], (lo[1] + hi[1]) / 2.0 + origin[1], (lo[2] + hi[2]) / 2.0]
            });
            crossings.push(Crossing { face, volume: crossed, at });
        }
        pieces.push(front);
        rest = behind;
    }
    pieces.push(rest);
    let strips_ms = ms(start);

    let start = Instant::now();
    let strips = std::thread::scope(|scope| {
        let handles: Vec<_> = pieces
            .iter()
            .map(|piece| {
                scope.spawn(move || {
                    let (mesh, tiny_edges) = join_tiny_edges(from_manifold(piece, origin));
                    StripPiece { volume: mesh.signed_volume(), validity: mesh.validity(), parts: piece.decompose().len(), tiny_edges, mesh }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("reading a strip back doesn't panic")).collect()
    });
    let from_manifold_ms = ms(start);
    Ok(StripSlice { order, strips, faces: cuts, crossings, input_volume, timings: SliceTimings { close_ms, strips_ms, from_manifold_ms } })
}

/// A corner of a face's outer ring, and the way the edge from it looks.
type Corner = ([f64; 3], Facing);

/// Why a face couldn't be closed behind.
enum Behind {
    Error(GeomError),
    Short(ShortEnd),
}

impl From<GeomError> for Behind {
    fn from(error: GeomError) -> Self {
        Behind::Error(error)
    }
}

/// Which way a face's edge looks out from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Facing {
    /// Its pit side: the face's toe.
    Front,
    /// Away from the pit: the face's top.
    Behind,
    /// Along the face: its ends.
    Along,
}

/// The half-space behind a face (in local coordinates), past the solid's outline: below the face down to `z_low`, and
/// over the parts of the outline behind its top line from `z_low` to `z_high`, 1 mm into the face's own plan so the
/// two overlap.
fn behind_face(face: &TriMesh, outline: &CrossSection, z_low: f64, z_high: f64) -> Result<(Manifold, Vec<Corner>), Behind> {
    let (up, _) = orient_upward(face)?;
    let loops = boundary_loops(&up)?;
    if loops.len() != 1 {
        return Err(GeomError::Refused(format!("it has {} edges all round, not one: join its pieces into one face first", loops.len())).into());
    }
    let ring = &loops[0];
    let plan: Vec<[f64; 2]> = ring.iter().map(|&v| [up.vertices[v as usize][0], up.vertices[v as usize][1]]).collect();
    if signed_area(&plan) <= 0.0 {
        return Err(GeomError::Refused("it folds back on itself in plan, so it can't be told which side is the pit".into()).into());
    }
    let below = close_surface_to_halfspace(&up, Side::Below, z_low)?;
    let below_m = to_manifold(&below.mesh, [0.0; 3], "the half-space below the face")?;

    let edges = facing_edges(&up, ring, &plan);
    // Each corner of the ring with the way the edge from it looks.
    let corners: Vec<Corner> = ring.iter().zip(&edges).map(|(&v, e)| (up.vertices[v as usize], e.0)).collect();
    let index = EdgeIndex::new(&plan);
    let footprint = CrossSection::from_polygons(std::slice::from_ref(&plan));
    let mut behind = Vec::new();
    for piece in outline.difference(&footprint).decompose() {
        let polygons = piece.to_polygons();
        let (mut front, mut back, mut along) = (0, 0, Vec::new());
        for &p in polygons.iter().flatten() {
            for e in index.near(&plan, p, 1e-4) {
                match edges[e].0 {
                    Facing::Front => front += 1,
                    Facing::Behind => back += 1,
                    Facing::Along => along.push(e),
                }
            }
        }
        let is_behind = match (front > 0, back > 0) {
            (true, true) => return Err(Behind::Short(short_end(&up, ring, &plan, &edges, &along, &polygons))),
            (false, true) => true,
            (true, false) => false,
            // Not touching the face: in front of it or behind it, by the nearest edge's slope.
            (false, false) => {
                let p = polygons[0][0];
                let e = index.nearest(&plan, p);
                let (a, b) = (plan[e], plan[(e + 1) % plan.len()]);
                let h = edges[e].1;
                (p[0] - (a[0] + b[0]) / 2.0) * h[0] + (p[1] - (a[1] + b[1]) / 2.0) * h[1] < 0.0
            }
        };
        if is_behind {
            behind.push(piece);
        }
    }
    if behind.is_empty() {
        return Ok((below_m, corners));
    }
    let behind = CrossSection::batch_union(&behind).offset(1e-3, JoinType::Miter, 2.0, 0);
    let prism = Manifold::extrude(&behind, z_high - z_low).translate(0.0, 0.0, z_low);
    let half = below_m.union(&prism);
    half.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    Ok((half, corners))
}

/// Which way each edge of the face's outer ring looks, with the slope beside it in plan: the horizontal part of the
/// nearest steep triangle's upward normal, which points to the pit. A berm beside an edge is looked past.
fn facing_edges(up: &TriMesh, ring: &[u32], plan: &[[f64; 2]]) -> Vec<(Facing, [f64; 2])> {
    let mut by_edge: HashMap<(u32, u32), usize> = HashMap::new();
    let mut around: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (t, &[x, y, z]) in up.triangles.iter().enumerate() {
        for (a, b) in [(x, y), (y, z), (z, x)] {
            by_edge.insert((a, b), t);
            around.entry((a.min(b), a.max(b))).or_default().push(t);
        }
    }
    let slope = |t: usize| {
        let n = up.triangle_cross(up.triangles[t]);
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if length == 0.0 { [0.0, 0.0] } else { [n[0] / length, n[1] / length] }
    };
    let steep = |seed: usize| {
        let mut seen = vec![seed];
        let mut queue = VecDeque::from([seed]);
        while let Some(t) = queue.pop_front() {
            let h = slope(t);
            if h[0].hypot(h[1]) > 0.05 {
                return h;
            }
            if seen.len() > 256 {
                break;
            }
            let [x, y, z] = up.triangles[t];
            for (a, b) in [(x, y), (y, z), (z, x)] {
                for &u in &around[&(a.min(b), a.max(b))] {
                    if !seen.contains(&u) {
                        seen.push(u);
                        queue.push_back(u);
                    }
                }
            }
        }
        [0.0, 0.0]
    };
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let h = by_edge.get(&(a, b)).map_or([0.0, 0.0], |&t| steep(t));
            let (p, q) = (plan[i], plan[(i + 1) % plan.len()]);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let length = dx.hypot(dy);
            let (hl, out) = (h[0].hypot(h[1]), if length > 0.0 { [dy / length, -dx / length] } else { [0.0, 0.0] });
            let s = if hl > 0.0 { (out[0] * h[0] + out[1] * h[1]) / hl } else { 0.0 };
            let facing = if s > 0.3 {
                Facing::Front
            } else if s < -0.3 {
                Facing::Behind
            } else {
                Facing::Along
            };
            (facing, h)
        })
        .collect()
}

/// Where a face's end stops inside the outline, and about how far the outline runs on past it.
fn short_end(up: &TriMesh, ring: &[u32], plan: &[[f64; 2]], edges: &[(Facing, [f64; 2])], along: &[usize], polygons: &[Vec<[f64; 2]>]) -> ShortEnd {
    let z = |i: usize| (up.vertices[ring[i] as usize][2] + up.vertices[ring[(i + 1) % ring.len()] as usize][2]) / 2.0;
    let mut best: Option<(f64, [f64; 3])> = None;
    for &e in along {
        let (p, q) = (plan[e], plan[(e + 1) % plan.len()]);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let length = dx.hypot(dy);
        if length == 0.0 || edges[e].0 != Facing::Along {
            continue;
        }
        let (out, middle) = ([dy / length, -dx / length], [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]);
        let reach = polygons.iter().flatten().map(|r| (r[0] - middle[0]) * out[0] + (r[1] - middle[1]) * out[1]).fold(0.0, f64::max);
        if best.is_none_or(|(most, _)| reach > most) {
            best = Some((reach, [middle[0], middle[1], z(e)]));
        }
    }
    match best {
        Some((reach, at)) => ShortEnd { at, run_on: Some(reach) },
        None => {
            let n = polygons[0].len() as f64;
            let (x, y) = polygons[0].iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
            ShortEnd { at: [x / n, y / n, 0.0], run_on: None }
        }
    }
}

/// The edges of a ring in plan, found by place.
struct EdgeIndex {
    cells: HashMap<(i64, i64), Vec<usize>>,
}

const EDGE_CELL: f64 = 10.0;

impl EdgeIndex {
    fn new(plan: &[[f64; 2]]) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for i in 0..plan.len() {
            let (p, q) = (plan[i], plan[(i + 1) % plan.len()]);
            let lo = Self::key([p[0].min(q[0]) - 1e-3, p[1].min(q[1]) - 1e-3]);
            let hi = Self::key([p[0].max(q[0]) + 1e-3, p[1].max(q[1]) + 1e-3]);
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    cells.entry((x, y)).or_default().push(i);
                }
            }
        }
        EdgeIndex { cells }
    }

    fn key(p: [f64; 2]) -> (i64, i64) {
        ((p[0] / EDGE_CELL).floor() as i64, (p[1] / EDGE_CELL).floor() as i64)
    }

    /// The edges within `tolerance` of `p`.
    fn near(&self, plan: &[[f64; 2]], p: [f64; 2], tolerance: f64) -> Vec<usize> {
        let mut found: Vec<usize> = self.cells.get(&Self::key(p)).into_iter().flatten().copied().filter(|&e| distance(plan, e, p) <= tolerance).collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The nearest edge to `p`, looked for over all of them.
    fn nearest(&self, plan: &[[f64; 2]], p: [f64; 2]) -> usize {
        (0..plan.len()).min_by(|&a, &b| distance(plan, a, p).total_cmp(&distance(plan, b, p))).expect("a ring has edges")
    }
}

/// How far `p` is from edge `e` of a ring in plan.
fn distance(plan: &[[f64; 2]], e: usize, p: [f64; 2]) -> f64 {
    let (a, b) = (plan[e], plan[(e + 1) % plan.len()]);
    let d = [b[0] - a[0], b[1] - a[1]];
    let length2 = d[0] * d[0] + d[1] * d[1];
    let t = if length2 > 0.0 { (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length2).clamp(0.0, 1.0) } else { 0.0 };
    (a[0] + t * d[0] - p[0]).hypot(a[1] + t * d[1] - p[1])
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}
