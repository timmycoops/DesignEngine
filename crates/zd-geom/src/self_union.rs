//! The self-union (decision 0013, slice B): a closed mesh whose surface passes through itself is cut
//! where it does and reduced to the surface of the solid it encloses, the faces with solid behind
//! them and empty space in front.
//!
//! 1. Every crossing pair's segment is found by the crossing search's interval test, at the cut
//!    tolerance: at [`COINCIDENT`](crate::COINCIDENT), every crossing however shallow (decision 0014),
//!    so the lines where the surface passes through itself close into loops and the regions between
//!    them stay apart. Faces that only come close (a knife edge, a hairline gap) are not crossings
//!    there and are left as they are. Two faces that overlap in one plane (or fold back flat on an
//!    edge) give segments too: each one's edges, clipped to the other. Where they face opposite ways, the overlap is a sheet with no thickness,
//!    and its faces drop out at step 4 (the winding number is the same on both sides of it). Where
//!    they face the same way, the later triangle's faces inside the earlier one drop out at once, so
//!    the face is kept once.
//! 2. The segments' ends, and any point where two segments in one triangle cross, are shared by every
//!    triangle they lie on, merged within a micrometre. A point on a triangle's edge splits that edge
//!    for both triangles on it.
//! 3. Each triangle with a segment, or a point on one of its edges, is triangulated again in its own
//!    plane by spade's constrained Delaunay triangulation, with its edges (split at their points) and
//!    its segments as constraints, so neighbours stay joined edge to edge.
//! 4. The faces fall into regions bounded by the crossing lines. For the largest face of each region
//!    the original surface's winding number is taken just behind it and just in front: the region is
//!    kept when solid lies behind (winding 1 or more) and empty space in front (0 or less). "Just" is a
//!    quarter of the distance to the next surface either way (at most 1 mm), so solids a fraction of a
//!    millimetre thick are judged right.
//!
//! 5. The result is checked before it is accepted: along vertical rays on a plan grid, the length
//!    where the original surface's winding number is 1 or more must equal the length inside the
//!    result, ray by ray (exact along each ray, whatever the spacing). A disagreement of more than
//!    0.01 % of the volume refuses the mesh, with its size.
//!
//! Everything is worked about a whole-metre origin, so mine grid coordinates keep their precision.

use std::collections::HashMap;

use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use zd_core::TriMesh;

use crate::crossings::{Crossing, Rules, crossing_list};
use crate::separate_touching_edges;

type P = [f64; 3];

/// Points closer than this (m) are the same point.
const SNAP: f64 = 1e-6;

/// What [`self_union`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnionLog {
    /// Crossing pairs resolved.
    pub pairs: usize,
    /// Triangles cut along the crossings (or split at a neighbour's point).
    pub triangles_cut: usize,
    /// Regions between the crossing lines, and how many were kept.
    pub regions: usize,
    pub regions_kept: usize,
    /// The volume as given (an overlap counted twice) and of the solid enclosed (m³).
    pub volume_before: f64,
    pub volume_after: f64,
    /// The check along vertical rays: how far the result and the original's winding disagree (m³),
    /// out of the volume the rays measured.
    pub ray_disagreement: f64,
    pub ray_volume: f64,
}

/// Rays on a side of the plan grid for the check.
const CHECK_RAYS: usize = 160;
/// The most the check may disagree, as a share of the volume.
const CHECK_SHARE: f64 = 1e-4;

/// The surface of the solid a self-crossing closed mesh encloses, or why it could not be found.
/// Crossings by less than `cut` (m) are not cut: [`COINCIDENT`](crate::COINCIDENT) cuts them all.
pub fn self_union(mesh: &TriMesh, cut: f64) -> Result<(TriMesh, UnionLog), String> {
    let Some(bounds) = mesh.bounds() else { return Err("the mesh is empty".into()) };
    // The winding number, and so what the surface encloses, is defined only for a closed surface: an
    // open sheet's volume as read means nothing, and rays can miss it.
    let counts = edge_counts(&mesh.triangles);
    let open = counts.values().filter(|&&n| n == 1).count();
    if let Some(&(a, b)) = mesh.triangles.iter().flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).map(|(u, v)| undirected(u, v)).collect::<Vec<_>>().iter().find(|e| counts[e] == 1) {
        let (p, q) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
        return Err(format!(
            "the surface is not closed ({open} open edges, the first at {:.3} {:.3} RL {:.3}): the union needs a closed surface",
            0.5 * (p[0] + q[0]),
            0.5 * (p[1] + q[1]),
            0.5 * (p[2] + q[2])
        ));
    }
    let origin = bounds.centre().map(f64::round);
    let local = TriMesh::new(mesh.vertices.iter().map(|p| sub(*p, origin)).collect(), mesh.triangles.clone());
    let mut log = UnionLog { volume_before: local.signed_volume(), ..UnionLog::default() };

    let crossings = crossing_list(&local, cut, Rules::Surface);
    log.pairs = crossings.len();

    // Shared points: the vertices first, then the segments' ends. Each segment goes to the triangles
    // it lies in.
    let mut points = Points::new(&local.vertices);
    let mut segments: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    let mut on_edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for &(i, j, crossing) in &crossings {
        let placed: Vec<(u32, u32, u32)> = match crossing {
            Crossing::Segment([p, q]) => {
                let (a, b) = (points.intern(p), points.intern(q));
                vec![(i, a, b), (j, a, b)]
            }
            // The surface rules find no thin pairs.
            Crossing::Thin(_) => Vec::new(),
            Crossing::Overlap(_) => {
                let (ti, tj) = (local.triangles[i as usize], local.triangles[j as usize]);
                let mut placed: Vec<(u32, u32, u32)> = clipped_edges(&mut points, tj, ti).into_iter().map(|(a, b)| (i, a, b)).collect();
                placed.extend(clipped_edges(&mut points, ti, tj).into_iter().map(|(a, b)| (j, a, b)));
                placed
            }
        };
        for (t, a, b) in placed {
            if a == b {
                continue;
            }
            segments.entry(t).or_default().push((a, b));
            for x in [a, b] {
                for owner in [i, j] {
                    if let Some(edge) = edge_holding(&points, local.triangles[owner as usize], x) {
                        let list = on_edges.entry(edge).or_default();
                        if !list.contains(&x) {
                            list.push(x);
                        }
                    }
                }
            }
        }
    }

    // Every triangle with a segment or a point on an edge, triangulated again; the rest as they were.
    let mut faces: Vec<[u32; 3]> = Vec::with_capacity(local.triangles.len() * 2);
    let mut parents: Vec<u32> = Vec::with_capacity(local.triangles.len() * 2);
    for (k, &tri) in local.triangles.iter().enumerate() {
        let own = segments.get(&(k as u32)).map_or(&[][..], Vec::as_slice);
        let split = [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])].iter().any(|&(u, v)| on_edges.contains_key(&undirected(u, v)));
        if own.is_empty() && !split {
            faces.push(tri);
            parents.push(k as u32);
            continue;
        }
        log.triangles_cut += 1;
        // spade asserts on some near-degenerate constraints; a panic there refuses this mesh, saying
        // where, rather than taking the caller down with it.
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| retriangulate(tri, own, &on_edges, &mut points)));
        let cut = match attempt {
            Ok(result) => result?,
            Err(_) => {
                let [a, b, c] = tri.map(|i| local.vertices[i as usize]);
                let centre = [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0 + origin[k]);
                return Err(format!("cutting the triangle at {:.3} {:.3} RL {:.3} along its crossings failed (the triangulator found it degenerate)", centre[0], centre[1], centre[2]));
            }
        };
        parents.extend(std::iter::repeat_n(k as u32, cut.len()));
        faces.extend(cut);
    }

    // Faces overlapping in one plane and facing the same way are kept once: the later triangle's
    // faces inside the earlier one go.
    let mut earlier: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(i, j, crossing) in &crossings {
        if matches!(crossing, Crossing::Overlap(_)) {
            let normal = |t: u32| {
                let [a, b, c] = local.triangles[t as usize].map(|v| local.vertices[v as usize]);
                cross(sub(b, a), sub(c, a))
            };
            if dot(normal(i), normal(j)) > 0.0 {
                earlier.entry(i.max(j)).or_default().push(i.min(j));
            }
        }
    }
    if !earlier.is_empty() {
        let mut keep = Vec::with_capacity(faces.len());
        for (f, parent) in faces.iter().zip(&parents) {
            let centre = {
                let [a, b, c] = f.map(|i| points.at(i));
                [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
            };
            let covered = earlier.get(parent).is_some_and(|list| list.iter().any(|&e| strictly_inside(&points, local.triangles[e as usize], centre)));
            if !covered {
                keep.push(*f);
            }
        }
        faces = keep;
    }

    // Regions: faces joined across edges with exactly two faces.
    let counts = edge_counts(&faces);
    let mut faces_on: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (k, f) in faces.iter().enumerate() {
        for (u, v) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
            if counts[&undirected(u, v)] == 2 {
                faces_on.entry(undirected(u, v)).or_default().push(k);
            }
        }
    }
    let mut root: Vec<usize> = (0..faces.len()).collect();
    for pair in faces_on.values() {
        let (a, b) = (find(&mut root, pair[0]), find(&mut root, pair[1]));
        root[a] = b;
    }
    let mut regions: HashMap<usize, Vec<usize>> = HashMap::new();
    for k in 0..faces.len() {
        regions.entry(find(&mut root, k)).or_default().push(k);
    }
    log.regions = regions.len();

    // Keep each region with solid behind it and empty space in front.
    let mut kept = Vec::with_capacity(faces.len());
    let mut members: Vec<Vec<usize>> = regions.into_values().collect();
    members.sort_by_key(|m| m[0]);
    for region in members {
        let probe = *region.iter().max_by(|&&a, &&b| area(&points, faces[a]).total_cmp(&area(&points, faces[b]))).expect("a region has faces");
        let [a, b, c] = faces[probe].map(|i| points.at(i));
        let normal = cross(sub(b, a), sub(c, a));
        let doubled = norm(normal);
        if doubled == 0.0 {
            continue;
        }
        let n = normal.map(|x| x / doubled);
        let centre = [0, 1, 2].map(|i| (a[i] + b[i] + c[i]) / 3.0);
        let clearance = [n, n.map(|x| -x)].map(|d| nearest_hit(&points, &faces, probe, centre, d)).into_iter().fold(4e-3, f64::min);
        let step = (0.25 * clearance).max(1e-9);
        let behind = winding(&local, [0, 1, 2].map(|i| centre[i] - step * n[i])).round();
        let ahead = winding(&local, [0, 1, 2].map(|i| centre[i] + step * n[i])).round();
        if behind >= 1.0 && ahead <= 0.0 {
            log.regions_kept += 1;
            kept.extend(region.iter().map(|&k| faces[k]));
        }
    }

    // The kept faces, their vertices only, back at mine grid coordinates.
    let mut renumber = vec![u32::MAX; points.all.len()];
    let mut vertices = Vec::new();
    for f in &mut kept {
        for i in f.iter_mut() {
            if renumber[*i as usize] == u32::MAX {
                renumber[*i as usize] = vertices.len() as u32;
                vertices.push(add(points.at(*i), origin));
            }
            *i = renumber[*i as usize];
        }
    }
    let mut result = TriMesh::new(vertices, kept);
    if !result.validity().is_watertight() {
        result = separate_touching_edges(&result).0;
    }
    let validity = result.validity();
    if !validity.is_watertight() {
        return Err(format!(
            "the solid enclosed could not be closed: {} open, {} many-face and {} inconsistently wound edges after cutting",
            validity.boundary_edges, validity.non_manifold_edges, validity.inconsistent_edges
        ));
    }
    log.volume_after = result.signed_volume();
    if log.volume_after <= 0.0 {
        return Err(format!("the solid enclosed holds no volume (as read {:.3} m3): too thin to judge", log.volume_before));
    }

    let local_result = TriMesh::new(result.vertices.iter().map(|p| sub(*p, origin)).collect(), result.triangles.clone());
    let (disagreement, measured, inside_out) = ray_check(&local, &local_result);
    log.ray_disagreement = disagreement;
    log.ray_volume = measured;
    // The union keeps where the winding is 1 or more: a part of the surface facing in (winding -1)
    // would be dropped, not resolved.
    if inside_out > CHECK_SHARE * measured.max(log.volume_after) {
        return Err(format!(
            "parts of the surface face inward ({inside_out:.4} m3 lies inside out along vertical rays, against {measured:.3} m3 inside); the union would drop them: not resolved"
        ));
    }
    if disagreement > CHECK_SHARE * measured.max(log.volume_after) {
        return Err(format!(
            "the solid enclosed ({:.3} m3) disagrees with the original surface by {disagreement:.4} m3 along vertical rays ({:.3} m3 measured); not resolved",
            log.volume_after, measured
        ));
    }
    Ok((result, log))
}

/// Along vertical rays on a plan grid over the original: the summed difference between the length
/// where the original's winding number is 1 or more and the length inside `result`, the volume the
/// rays measured in the original, and the volume where its winding is -1 or less (inside out). Rays
/// sit off the grid's lines by irrational fractions of a cell, so they do not run along edges.
fn ray_check(original: &TriMesh, result: &TriMesh) -> (f64, f64, f64) {
    let Some(bounds) = original.bounds() else { return (0.0, 0.0, 0.0) };
    let (lo, hi) = (bounds.min, bounds.max);
    let size = [(hi[0] - lo[0]).max(1e-9), (hi[1] - lo[1]).max(1e-9)];
    let cell = [size[0] / CHECK_RAYS as f64, size[1] / CHECK_RAYS as f64];
    let at = |i: usize, j: usize| [lo[0] + (i as f64 + 0.5 + 0.0731) * cell[0], lo[1] + (j as f64 + 0.5 - 0.1137) * cell[1]];

    // Each mesh's triangles binned by the rays their plan boxes cover.
    let bin = |mesh: &TriMesh| {
        let mut bins: Vec<Vec<u32>> = vec![Vec::new(); CHECK_RAYS * CHECK_RAYS];
        for (k, t) in mesh.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            let range = |axis: usize| {
                let (mn, mx) = (a[axis].min(b[axis]).min(c[axis]), a[axis].max(b[axis]).max(c[axis]));
                let first = (((mn - lo[axis]) / cell[axis]).floor() as i64 - 1).clamp(0, CHECK_RAYS as i64 - 1) as usize;
                let last = (((mx - lo[axis]) / cell[axis]).ceil() as i64 + 1).clamp(0, CHECK_RAYS as i64 - 1) as usize;
                first..=last
            };
            for i in range(0) {
                for j in range(1) {
                    bins[i * CHECK_RAYS + j].push(k as u32);
                }
            }
        }
        bins
    };
    let (bins_a, bins_b) = (bin(original), bin(result));
    let length = |mesh: &TriMesh, list: &[u32], x: f64, y: f64| {
        let mut hits: Vec<(f64, i32)> = Vec::new();
        for &k in list {
            let [a, b, c] = mesh.triangles[k as usize].map(|i| mesh.vertices[i as usize]);
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if area == 0.0 {
                continue;
            }
            let w0 = ((b[0] - x) * (c[1] - y) - (c[0] - x) * (b[1] - y)) / area;
            let w1 = ((c[0] - x) * (a[1] - y) - (a[0] - x) * (c[1] - y)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            // Going up through a face that faces up leaves the solid; facing down enters it.
            hits.push((w0 * a[2] + w1 * b[2] + w2 * c[2], if area > 0.0 { -1 } else { 1 }));
        }
        hits.sort_by(|p, q| p.0.total_cmp(&q.0));
        let (mut count, mut inside, mut inverted, mut last) = (0, 0.0, 0.0, 0.0);
        for (z, step) in hits {
            if count >= 1 {
                inside += z - last;
            } else if count <= -1 {
                inverted += z - last;
            }
            count += step;
            last = z;
        }
        (inside, inverted)
    };
    let (mut disagreement, mut measured, mut inside_out) = (0.0, 0.0, 0.0);
    for i in 0..CHECK_RAYS {
        for j in 0..CHECK_RAYS {
            let [x, y] = at(i, j);
            let k = i * CHECK_RAYS + j;
            let ((la, inverted), (lb, _)) = (length(original, &bins_a[k], x, y), length(result, &bins_b[k], x, y));
            disagreement += (la - lb).abs() * cell[0] * cell[1];
            measured += la * cell[0] * cell[1];
            inside_out += inverted * cell[0] * cell[1];
        }
    }
    (disagreement, measured, inside_out)
}

/// Points shared across triangles, merged within [`SNAP`].
struct Points {
    all: Vec<P>,
    cells: HashMap<[i64; 3], Vec<u32>>,
}

impl Points {
    fn new(vertices: &[P]) -> Self {
        let mut points = Points { all: Vec::with_capacity(vertices.len() * 2), cells: HashMap::new() };
        for &v in vertices {
            // The vertices stay as they are, even if two lie within a micrometre.
            let id = points.all.len() as u32;
            points.all.push(v);
            points.cells.entry(cell(v)).or_default().push(id);
        }
        points
    }

    fn at(&self, id: u32) -> P {
        self.all[id as usize]
    }

    fn intern(&mut self, p: P) -> u32 {
        let c = cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = self.cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz])
                        && let Some(&id) = ids.iter().find(|&&id| norm(sub(self.all[id as usize], p)) <= SNAP)
                    {
                        return id;
                    }
                }
            }
        }
        let id = self.all.len() as u32;
        self.all.push(p);
        self.cells.entry(c).or_default().push(id);
        id
    }
}

fn cell(p: P) -> [i64; 3] {
    p.map(|x| (x / SNAP).floor() as i64)
}

/// The edges of triangle `from`, clipped to triangle `to` lying in the same plane: the pieces of
/// `from`'s edges inside `to`, as point pairs (a vertex of `from` where an edge starts or ends
/// inside, else a new point where it meets `to`'s boundary).
fn clipped_edges(points: &mut Points, from: [u32; 3], to: [u32; 3]) -> Vec<(u32, u32)> {
    let [a, b, c] = to.map(|i| points.at(i));
    let Some(n) = unit(cross(sub(b, a), sub(c, a))) else { return Vec::new() };
    let Some(u) = unit(sub(b, a)) else { return Vec::new() };
    let w = cross(n, u);
    let flat = |p: P| [dot(sub(p, a), u), dot(sub(p, a), w)];
    // `to` runs counter-clockwise in this frame: inside is left of each of its edges.
    let corners = [a, b, c].map(flat);
    let mut out = Vec::new();
    for (s, e) in [(from[0], from[1]), (from[1], from[2]), (from[2], from[0])] {
        let (ps, pe) = (points.at(s), points.at(e));
        let (fs, fe) = (flat(ps), flat(pe));
        let d = [fe[0] - fs[0], fe[1] - fs[1]];
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if length == 0.0 {
            continue;
        }
        let (mut enter, mut exit) = (0.0_f64, 1.0_f64);
        let mut empty = false;
        for k in 0..3 {
            let (p, q) = (corners[k], corners[(k + 1) % 3]);
            let edge = [q[0] - p[0], q[1] - p[1]];
            let edge_length = (edge[0] * edge[0] + edge[1] * edge[1]).sqrt();
            // Distance of the segment's start left of this edge, and how it changes along the segment.
            let side = (edge[0] * (fs[1] - p[1]) - edge[1] * (fs[0] - p[0])) / edge_length;
            let rate = (edge[0] * d[1] - edge[1] * d[0]) / edge_length;
            if rate.abs() < 1e-15 {
                if side < -SNAP {
                    empty = true;
                }
                continue;
            }
            let t = -side / rate;
            if rate > 0.0 {
                enter = enter.max(t);
            } else {
                exit = exit.min(t);
            }
        }
        if empty || (exit - enter) * length <= SNAP {
            continue;
        }
        let at = |t: f64, points: &mut Points| {
            if t * length <= SNAP {
                s
            } else if (1.0 - t) * length <= SNAP {
                e
            } else {
                points.intern([0, 1, 2].map(|i| ps[i] + t * (pe[i] - ps[i])))
            }
        };
        let (p0, p1) = (at(enter, points), at(exit, points));
        if p0 != p1 {
            out.push((p0, p1));
        }
    }
    out
}

/// More than [`SNAP`] inside triangle `tri`, measured in its plane.
fn strictly_inside(points: &Points, tri: [u32; 3], p: P) -> bool {
    let [a, b, c] = tri.map(|i| points.at(i));
    let Some(n) = unit(cross(sub(b, a), sub(c, a))) else { return false };
    [(a, b), (b, c), (c, a)].iter().all(|&(s, e)| {
        let edge = sub(e, s);
        dot(cross(edge, sub(p, s)), n) / norm(edge) > SNAP
    })
}

/// The edge of `tri` that point `x` lies on (within [`SNAP`], away from its ends), if any.
fn edge_holding(points: &Points, tri: [u32; 3], x: u32) -> Option<(u32, u32)> {
    if tri.contains(&x) {
        return None;
    }
    let p = points.at(x);
    [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])].into_iter().find_map(|(u, v)| {
        let (a, b) = (points.at(u), points.at(v));
        let ab = sub(b, a);
        let t = dot(sub(p, a), ab) / dot(ab, ab);
        let off = sub(sub(p, a), ab.map(|c| c * t));
        (t > 0.0 && t < 1.0 && norm(off) <= SNAP).then_some(undirected(u, v))
    })
}

/// One triangle triangulated again with its edge points and segments, counter-clockwise like it.
fn retriangulate(tri: [u32; 3], segments: &[(u32, u32)], on_edges: &HashMap<(u32, u32), Vec<u32>>, points: &mut Points) -> Result<Vec<[u32; 3]>, String> {
    let [a, b, c] = tri.map(|i| points.at(i));
    let n = unit(cross(sub(b, a), sub(c, a))).ok_or("a triangle to cut has no area")?;
    let u = unit(sub(b, a)).ok_or("a triangle to cut has no area")?;
    let w = cross(n, u);
    let flat = |p: P| Point2::new(dot(sub(p, a), u), dot(sub(p, a), w));

    let mut cdt = ConstrainedDelaunayTriangulation::<Point2<f64>>::new();
    let mut ids: Vec<u32> = Vec::new();
    let mut handles: HashMap<u32, FixedVertexHandle> = HashMap::new();
    let mut insert = |cdt: &mut ConstrainedDelaunayTriangulation<Point2<f64>>, ids: &mut Vec<u32>, id: u32, p: P| -> Result<FixedVertexHandle, String> {
        if let Some(&h) = handles.get(&id) {
            return Ok(h);
        }
        let h = cdt.insert(flat(p)).map_err(|e| format!("triangulating a cut triangle: {e:?}"))?;
        if h.index() == ids.len() {
            ids.push(id);
        }
        handles.insert(id, h);
        Ok(h)
    };

    // The boundary: each edge from its start, through its points in order, to its end.
    let mut boundary = Vec::new();
    for (s, e) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
        let (ps, pe) = (points.at(s), points.at(e));
        let along = sub(pe, ps);
        let mut on: Vec<(f64, u32)> = on_edges.get(&undirected(s, e)).map_or(Vec::new(), |list| list.iter().map(|&x| (dot(sub(points.at(x), ps), along), x)).collect());
        on.sort_by(|x, y| x.0.total_cmp(&y.0));
        boundary.push(s);
        boundary.extend(on.into_iter().map(|(_, x)| x));
    }
    for &id in &boundary {
        insert(&mut cdt, &mut ids, id, points.at(id))?;
    }
    for i in 0..boundary.len() {
        let (h0, h1) = (handles_of(&cdt, &ids, boundary[i]), handles_of(&cdt, &ids, boundary[(i + 1) % boundary.len()]));
        if let (Some(h0), Some(h1)) = (h0, h1)
            && h0 != h1
        {
            cdt.add_constraint_and_split(h0, h1, |p| p);
        }
    }
    for &(p, q) in segments {
        let h0 = insert(&mut cdt, &mut ids, p, points.at(p))?;
        let h1 = insert(&mut cdt, &mut ids, q, points.at(q))?;
        if h0 != h1 {
            cdt.add_constraint_and_split(h0, h1, |p| p);
        }
    }
    // Points spade added where segments crossed: back into three dimensions and shared.
    while ids.len() < cdt.num_vertices() {
        let at = cdt.vertex(FixedVertexHandle::from_index(ids.len())).position();
        let p = [0, 1, 2].map(|i| a[i] + at.x * u[i] + at.y * w[i]);
        ids.push(points.intern(p));
    }

    // Faces outside the boundary (where a point on an edge fell just inside it) are dropped: they
    // are reached from the hull across edges that are not constraints.
    // Inner faces are numbered from 1 (0 is the outer face).
    let mut outside = vec![false; cdt.num_inner_faces() + 1];
    let mut queue = Vec::new();
    for face in cdt.inner_faces() {
        if face.adjacent_edges().iter().any(|e| e.rev().face().is_outer() && !e.is_constraint_edge()) {
            outside[face.fix().index()] = true;
            queue.push(face.fix());
        }
    }
    while let Some(handle) = queue.pop() {
        for e in cdt.face(handle).adjacent_edges() {
            if e.is_constraint_edge() {
                continue;
            }
            if let Some(next) = e.rev().face().as_inner()
                && !outside[next.fix().index()]
            {
                outside[next.fix().index()] = true;
                queue.push(next.fix());
            }
        }
    }

    let mut out = Vec::new();
    for face in cdt.inner_faces() {
        if outside[face.fix().index()] {
            continue;
        }
        let vs = face.vertices();
        let mut f = vs.map(|v| ids[v.fix().index()]);
        if f[0] == f[1] || f[1] == f[2] || f[0] == f[2] {
            continue;
        }
        let [p0, p1, p2] = vs.map(|v| v.position());
        let signed = (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y);
        if signed < 0.0 {
            f.swap(1, 2);
        }
        out.push(f);
    }
    Ok(out)
}

fn handles_of(cdt: &ConstrainedDelaunayTriangulation<Point2<f64>>, ids: &[u32], id: u32) -> Option<FixedVertexHandle> {
    ids.iter().position(|&x| x == id).filter(|&i| i < cdt.num_vertices()).map(FixedVertexHandle::from_index)
}

/// How far a ray from `from` along unit `d` travels before it meets a face other than `skip`
/// (infinity if it meets none).
fn nearest_hit(points: &Points, faces: &[[u32; 3]], skip: usize, from: P, d: P) -> f64 {
    let mut nearest = f64::INFINITY;
    for (k, f) in faces.iter().enumerate() {
        if k == skip {
            continue;
        }
        // Moller-Trumbore.
        let [a, b, c] = f.map(|i| points.at(i));
        let (e1, e2) = (sub(b, a), sub(c, a));
        let pv = cross(d, e2);
        let det = dot(e1, pv);
        if det.abs() < 1e-18 {
            continue;
        }
        let tv = sub(from, a);
        let u = dot(tv, pv) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let qv = cross(tv, e1);
        let v = dot(d, qv) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = dot(e2, qv) / det;
        if t > 1e-12 && t < nearest {
            nearest = t;
        }
    }
    nearest
}

/// The generalized winding number of a closed mesh at `x`: the solid angle its triangles subtend,
/// over 4π (van Oosterom and Strackee).
fn winding(mesh: &TriMesh, x: P) -> f64 {
    let total: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| sub(mesh.vertices[i as usize], x));
            let (la, lb, lc) = (norm(a), norm(b), norm(c));
            let numerator = dot(a, cross(b, c));
            let denominator = la * lb * lc + dot(a, b) * lc + dot(b, c) * la + dot(c, a) * lb;
            2.0 * numerator.atan2(denominator)
        })
        .sum();
    total / (4.0 * std::f64::consts::PI)
}

fn edge_counts(faces: &[[u32; 3]]) -> HashMap<(u32, u32), usize> {
    let mut counts = HashMap::new();
    for f in faces {
        for (u, v) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
            *counts.entry(undirected(u, v)).or_default() += 1;
        }
    }
    counts
}

fn find(root: &mut [usize], mut i: usize) -> usize {
    while root[i] != i {
        root[i] = root[root[i]];
        i = root[i];
    }
    i
}

fn area(points: &Points, f: [u32; 3]) -> f64 {
    let [a, b, c] = f.map(|i| points.at(i));
    0.5 * norm(cross(sub(b, a), sub(c, a)))
}

fn undirected(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn cross(a: P, b: P) -> P {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: P) -> f64 {
    dot(a, a).sqrt()
}

fn unit(v: P) -> Option<P> {
    let length = norm(v);
    (length > 0.0).then(|| v.map(|c| c / length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::find_crossings;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A box with corners `corner(i)` for i = 0..8 (x, y, z bits), wound outward.
    fn box_from(corner: impl Fn(usize) -> P) -> TriMesh {
        let v = (0..8).map(corner).collect();
        let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh::new(v, t)
    }

    fn joined(a: &TriMesh, b: &TriMesh) -> TriMesh {
        let mut m = a.clone();
        let base = m.vertices.len() as u32;
        m.vertices.extend(&b.vertices);
        m.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + base)));
        m
    }

    #[test]
    fn two_overlapping_cubes_become_their_union() {
        let a = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let b = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 5.0 + (i >> 1 & 1) as f64 * 10.0, 105.0 + (i >> 2) as f64 * 10.0]);
        let both = joined(&a, &b);
        assert!((both.signed_volume() - 2000.0).abs() < 1e-9);
        let (union, log) = self_union(&both, 0.001).unwrap();
        assert!(union.validity().is_watertight(), "{:?}", union.validity());
        assert!((union.signed_volume() - 1875.0).abs() < 1e-6, "{}", union.signed_volume());
        assert_eq!(find_crossings(&union, 0.001).pairs, 0);
        assert!(log.pairs > 0 && log.regions_kept < log.regions);
        assert!((log.volume_before - 2000.0).abs() < 1e-9 && (log.volume_after - 1875.0).abs() < 1e-6);
    }

    #[test]
    fn a_cube_crossed_by_a_turned_box_becomes_their_union() {
        // A 10 m cube, and a 10 m square box 6 m tall turned 45° about the same centre: the overlap is an
        // octagon of 200 (√2 - 1) m² through 6 m, so the union holds 1000 + 600 - 1200 (√2 - 1) m³.
        let cube = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let r = 5.0 * std::f64::consts::SQRT_2;
        let turned = box_from(|i| {
            // Corners at 45° steps from the centre: (x bit, y bit) picks the diamond's corner.
            let angle = match (i & 1, i >> 1 & 1) {
                (0, 0) => -std::f64::consts::FRAC_PI_2,
                (1, 0) => 0.0,
                (0, 1) => std::f64::consts::PI,
                _ => std::f64::consts::FRAC_PI_2,
            };
            [E + 5.0 + r * angle.cos(), N + 5.0 + r * angle.sin(), 102.0 + (i >> 2) as f64 * 6.0]
        });
        assert!((turned.signed_volume() - 600.0).abs() < 1e-6, "{}", turned.signed_volume());
        let (union, _) = self_union(&joined(&cube, &turned), 0.001).unwrap();
        assert!(union.validity().is_watertight(), "{:?}", union.validity());
        let expected = 1000.0 + 600.0 - 1200.0 * (std::f64::consts::SQRT_2 - 1.0);
        assert!((union.signed_volume() - expected).abs() < 1e-6, "{} against {expected}", union.signed_volume());
        assert_eq!(find_crossings(&union, 0.001).pairs, 0);
    }

    #[test]
    fn a_cube_inside_another_is_swallowed() {
        let big = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let small = box_from(|i| [E + 2.0 + (i & 1) as f64 * 3.0, N + 2.0 + (i >> 1 & 1) as f64 * 3.0, 102.0 + (i >> 2) as f64 * 3.0]);
        // No crossings, but nested: the union is the big cube alone.
        let (union, log) = self_union(&joined(&big, &small), 0.001).unwrap();
        assert_eq!((log.pairs, log.regions, log.regions_kept), (0, 2, 1));
        assert!((union.signed_volume() - 1000.0).abs() < 1e-9);

    }

    #[test]
    fn a_solid_a_fraction_of_a_millimetre_thick_keeps_its_volume() {
        // A 20 x 20 m slab 0.4 mm thick crossed by a small cube: the slab is judged by probes closer
        // than its thickness, so its 0.16 m3 is kept with the cube's 1 m3 beside the overlap.
        let slab = box_from(|i| [E + (i & 1) as f64 * 20.0, N + (i >> 1 & 1) as f64 * 20.0, 100.0 + (i >> 2) as f64 * 0.0004]);
        let cube = box_from(|i| [E + 5.0 + (i & 1) as f64 * 1.0, N + 5.0 + (i >> 1 & 1) as f64 * 1.0, 99.5 + (i >> 2) as f64 * 1.0]);
        let (union, _) = self_union(&joined(&slab, &cube), 0.0001).unwrap();
        assert!(union.validity().is_watertight(), "{:?}", union.validity());
        let expected = 20.0 * 20.0 * 0.0004 + 1.0 - 1.0 * 1.0 * 0.0004;
        assert!((union.signed_volume() - expected).abs() < 1e-9, "{} against {expected}", union.signed_volume());
    }

    #[test]
    fn a_crossing_shallower_than_the_tolerance_in_places_is_cut_all_the_way_round() {
        // A box whose bottom dips into a cube's top by 0.5 mm along one side and 5 mm along the
        // other. At 1 mm the shallow side is not a crossing, so the line where the two meet stops
        // short; cut along every crossing, it closes and the union is exact. The overlap is 5 m by
        // 6 m at an average 2.75 mm.
        let cube = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let dip = |y_bit: usize| 0.0005 + 0.0045 * y_bit as f64;
        let above = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 2.0 + (i >> 1 & 1) as f64 * 6.0, if i >> 2 == 0 { 110.0 - dip(i >> 1 & 1) } else { 120.0 }]);
        let both = joined(&cube, &above);
        let expected = 1000.0 + (600.0 + 60.0 * 0.00275) - 30.0 * 0.00275;
        // Cut at 1 mm, the regions either side of the shallow side run together and the union cannot close.
        assert!(self_union(&both, 0.001).is_err_and(|why| why.contains("could not be closed")));
        let (union, _) = self_union(&both, crate::COINCIDENT).unwrap();
        assert!(union.validity().is_watertight(), "{:?}", union.validity());
        assert!((union.signed_volume() - expected).abs() < 1e-6, "{} against {expected}", union.signed_volume());
        assert_eq!(find_crossings(&union, crate::COINCIDENT).pairs, 0);
    }

    #[test]
    fn thin_features_are_left_as_they_are() {
        // Two overlapping cubes and, beside them, a wedge rising from a knife edge to 0.5 mm: the
        // cubes become their union and the wedge keeps its 0.025 m3.
        let a = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let b = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 5.0 + (i >> 1 & 1) as f64 * 10.0, 105.0 + (i >> 2) as f64 * 10.0]);
        let wedge = TriMesh::new(
            [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 0.0005], [0.0, 10.0, 0.0], [10.0, 10.0, 0.0], [10.0, 10.0, 0.0005]].iter().map(|p| [E + 30.0 + p[0], N + p[1], 100.0 + p[2]]).collect(),
            vec![[0, 3, 1], [1, 3, 4], [0, 2, 3], [2, 5, 3], [1, 4, 5], [1, 5, 2], [0, 1, 2], [3, 5, 4]],
        );
        let (union, _) = self_union(&joined(&joined(&a, &b), &wedge), crate::COINCIDENT).unwrap();
        assert!(union.validity().is_watertight(), "{:?}", union.validity());
        assert!((union.signed_volume() - 1875.025).abs() < 1e-6, "{}", union.signed_volume());
        let after = crate::find_solid_crossings(&union, 0.001);
        assert_eq!(after.pairs, 0);
        assert!(after.thin > 0);
    }

    #[test]
    fn an_open_surface_is_refused() {
        // Two overlapping cubes with a triangle missing: what they enclose is not defined.
        let a = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let b = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 5.0 + (i >> 1 & 1) as f64 * 10.0, 105.0 + (i >> 2) as f64 * 10.0]);
        let mut both = joined(&a, &b);
        both.triangles.remove(0);
        let why = self_union(&both, crate::COINCIDENT).unwrap_err();
        assert!(why.starts_with("the surface is not closed (3 open edges, the first at 650005.000 7450005.000 RL 100.000)"), "{why}");
    }

    #[test]
    fn a_part_facing_inward_is_refused_not_dropped() {
        // A cube, and an overlapping one wound inside out: its winding is -1, so the union would keep
        // only the first cube's 875 m3 outside it and drop the other's 875 m3. Refused instead.
        let a = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let mut b = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 5.0 + (i >> 1 & 1) as f64 * 10.0, 105.0 + (i >> 2) as f64 * 10.0]);
        b.triangles.iter_mut().for_each(|t| t.swap(1, 2));
        let why = self_union(&joined(&a, &b), crate::COINCIDENT).unwrap_err();
        // Measured along 160 x 160 rays: within 1 % of the 875 m3.
        let measured: f64 = why.strip_prefix("parts of the surface face inward (").and_then(|rest| rest.split(' ').next()).and_then(|v| v.parse().ok()).expect(&why);
        assert!((measured - 875.0).abs() < 8.75, "{why}");
    }

    #[test]
    fn faces_overlapping_in_one_plane_are_resolved() {
        // Two cubes overlapping with their tops and bottoms in the same planes: the faces there overlap
        // facing the same way, and the union is the L-shaped outline through 10 m: 1,750 m3.
        let big = box_from(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let beside = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 5.0 + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]);
        let (union, _) = self_union(&joined(&big, &beside), 0.001).unwrap();
        assert!(union.validity().is_watertight(), "{:?}", union.validity());
        assert!((union.signed_volume() - 1750.0).abs() < 1e-6, "{}", union.signed_volume());

        // A cube sitting on another, offset: the touching faces overlap facing opposite ways and drop
        // out, leaving one solid of 2,000 m3 with no wall between.
        let above = box_from(|i| [E + 5.0 + (i & 1) as f64 * 10.0, N + 3.0 + (i >> 1 & 1) as f64 * 10.0, 110.0 + (i >> 2) as f64 * 10.0]);
        let (stack, _) = self_union(&joined(&big, &above), 0.001).unwrap();
        assert!(stack.validity().is_watertight(), "{:?}", stack.validity());
        assert!((stack.signed_volume() - 2000.0).abs() < 1e-6, "{}", stack.signed_volume());
        assert_eq!(find_crossings(&stack, 0.001).pairs, 0);
    }
}
