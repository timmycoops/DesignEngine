//! The crossing check (decision 0010): pairs of triangles whose insides pass through each other or
//! fold back onto each other by more than a tolerance. Crossings are found and listed, never fixed.
//!
//! Candidates are the pairs whose boxes, grown by the tolerance, overlap (BVH queries split across
//! cores, against a BVH per core-sized tile of the mesh); each is then tested exactly. Two triangles in different planes
//! cross when each has corners more than the tolerance either side of the other's plane and the
//! pieces of the two on the line where the planes meet overlap by more than the tolerance (the
//! interval test, which holds when edges meet exactly, as grid-aligned coordinates often do). Two in
//! one plane (within the tolerance) cross when they overlap by more than it, and two on one edge
//! when they fold back onto each other. Triangles that only touch, at a corner, along an edge or
//! within the tolerance, do not cross.
//!
//! On a closed solid (decision 0014), two faces within the tolerance of one plane that face opposite
//! ways and lie apart are thin, not crossing: the solid tapers to a knife edge there, or two of its
//! parts come within a hairline. They are counted apart from crossings. Faces that coincide (within
//! [`COINCIDENT`]), or overlap facing the same way, still cross.
//!
//! Two exceptions (decision 0017). Two faces folded flat on a shared edge are thin even when they
//! coincide: a knife edge of no thickness, enclosing nothing. Two faces overlapping the same way are
//! thin when a face between them points the other way: going through all three runs solid, gap,
//! solid, gap, a lip over a gap. With nothing between them the solid would be there twice.

use parry3d_f64::bounding_volume::Aabb;
use parry3d_f64::math::Vector;
use parry3d_f64::partitioning::{Bvh, BvhBuildStrategy};
use zd_core::{CrossingCheck, Crossings, TriMesh};

use crate::worker_threads;

type P = [f64; 3];

/// Faces and points closer than this (m) coincide: rounding, in mine grid coordinates worked about
/// a whole-metre origin.
pub const COINCIDENT: f64 = 1e-9;

/// Which rules a search applies to faces within the tolerance of one plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rules {
    /// They cross where they overlap, whichever way they face.
    Surface,
    /// Facing opposite ways and apart, they are thin.
    Solid,
}

/// Every crossing pair in the mesh, by the surface rules, with where the first
/// [`Crossings::SITES`] cross (in triangle order, so the result is the same on any number of cores).
pub fn find_crossings(mesh: &TriMesh, tolerance: f64) -> Crossings {
    search(mesh, tolerance, None, Rules::Surface)
}

/// Every crossing pair in a closed solid, by the solid rules, with the thin pairs counted apart.
pub fn find_solid_crossings(mesh: &TriMesh, tolerance: f64) -> Crossings {
    search(mesh, tolerance, None, Rules::Solid)
}

/// The search, with the number of tiles fixed (for tests) or chosen by the mesh's size.
fn search(mesh: &TriMesh, tolerance: f64, tile_count: Option<usize>, rules: Rules) -> Crossings {
    let tolerance = tolerance.max(0.0);
    let check = match rules {
        Rules::Surface => CrossingCheck::Search { tolerance },
        Rules::Solid => CrossingCheck::SolidSearch { tolerance },
    };
    let Some(bounds) = mesh.bounds() else {
        return Crossings { check, pairs: 0, at: Vec::new(), thin: 0, thin_at: Vec::new() };
    };
    // Worked about a whole-metre origin so mine grid coordinates keep their precision.
    let origin = bounds.centre().map(f64::round);
    let local: Vec<P> = mesh.vertices.iter().map(|p| sub(*p, origin)).collect();
    let triangles = &mesh.triangles;
    let boxes: Vec<Aabb> = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| local[i as usize]);
            let lo = [0, 1, 2].map(|k| a[k].min(b[k]).min(c[k]) - tolerance);
            let hi = [0, 1, 2].map(|k| a[k].max(b[k]).max(c[k]) + tolerance);
            Aabb::new(Vector::new(lo[0], lo[1], lo[2]), Vector::new(hi[0], hi[1], hi[2]))
        })
        .collect();
    let threads = worker_threads(triangles.len());
    let tiles = Tiles::build(&boxes, tile_count.unwrap_or(if triangles.len() < SMALL { 1 } else { threads }));
    let chunk = triangles.len().div_ceil(threads);
    // Each core takes a run of triangles in order and keeps its counts and first sites, of
    // crossings and of thin pairs.
    let parts: Vec<[(usize, Vec<P>); 2]> = std::thread::scope(|scope| {
        let (tiles, boxes, local) = (&tiles, &boxes, &local);
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                scope.spawn(move || {
                    let mut found: [(usize, Vec<P>); 2] = [(0, Vec::new()), (0, Vec::new())];
                    let mut hits: Vec<(u32, Crossing)> = Vec::new();
                    for i in k * chunk..((k + 1) * chunk).min(triangles.len()) {
                        hits.clear();
                        for j in tiles.overlapping(&boxes[i]) {
                            if j as usize > i
                                && let Some(at) = crossing(triangles[i], triangles[j as usize], local, tolerance, rules)
                            {
                                hits.push((
                                    j,
                                    match at {
                                        Crossing::Overlap(p) if rules == Rules::Solid && lip_between(i, j as usize, p, triangles, local, tiles) => Crossing::Thin(p),
                                        other => other,
                                    },
                                ));
                            }
                        }
                        hits.sort_by_key(|h| h.0);
                        for (_, at) in &hits {
                            let (count, sites) = &mut found[usize::from(matches!(at, Crossing::Thin(_)))];
                            *count += 1;
                            if sites.len() < Crossings::SITES {
                                sites.push(at.site());
                            }
                        }
                    }
                    found
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a crossing search thread panicked")).collect()
    });

    let total = |k: usize| parts.iter().map(|p| p[k].0).sum();
    let sites = |k: usize| parts.iter().flat_map(|p| p[k].1.iter()).take(Crossings::SITES).map(|p| add(*p, origin)).collect();
    Crossings { check, pairs: total(0), at: sites(0), thin: total(1), thin_at: sites(1) }
}

/// How two triangles cross: along a segment (in different planes), or overlapping in one plane or
/// folded back on one edge, with a point in the overlap. By the solid rules, `Thin` is two faces
/// within the tolerance of one plane that are not a crossing: facing opposite ways and apart or
/// folded on an edge, or facing the same way with a lip between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Crossing {
    Segment([P; 2]),
    Overlap(P),
    Thin(P),
}

impl Crossing {
    fn site(&self) -> P {
        match *self {
            Crossing::Segment([a, b]) => mid(a, b),
            Crossing::Overlap(p) | Crossing::Thin(p) => p,
        }
    }
}

/// Every crossing pair in the mesh with how it crosses (thin pairs too, by the solid rules), in world
/// coordinates, in triangle order. For the self-union, on one core (solids are small).
pub(crate) fn crossing_list(mesh: &TriMesh, tolerance: f64, rules: Rules) -> Vec<(u32, u32, Crossing)> {
    let tolerance = tolerance.max(0.0);
    let Some(bounds) = mesh.bounds() else { return Vec::new() };
    let origin = bounds.centre().map(f64::round);
    let local: Vec<P> = mesh.vertices.iter().map(|p| sub(*p, origin)).collect();
    let boxes: Vec<Aabb> = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| local[i as usize]);
            let lo = [0, 1, 2].map(|k| a[k].min(b[k]).min(c[k]) - tolerance);
            let hi = [0, 1, 2].map(|k| a[k].max(b[k]).max(c[k]) + tolerance);
            Aabb::new(Vector::new(lo[0], lo[1], lo[2]), Vector::new(hi[0], hi[1], hi[2]))
        })
        .collect();
    let tiles = Tiles::build(&boxes, 1);
    let mut found = Vec::new();
    for (i, query) in boxes.iter().enumerate() {
        let mut hits: Vec<(u32, Crossing)> = tiles
            .overlapping(query)
            .filter(|&j| j as usize > i)
            .filter_map(|j| crossing(mesh.triangles[i], mesh.triangles[j as usize], &local, tolerance, rules).map(|c| (j, c)))
            .collect();
        hits.sort_by_key(|h| h.0);
        found.extend(hits.into_iter().map(|(j, c)| {
            let world = match c {
                Crossing::Segment([a, b]) => Crossing::Segment([add(a, origin), add(b, origin)]),
                Crossing::Overlap(p) => Crossing::Overlap(add(p, origin)),
                Crossing::Thin(p) => Crossing::Thin(add(p, origin)),
            };
            (i as u32, j, world)
        }));
    }
    found
}

/// Meshes smaller than this get one BVH; larger ones one per core.
const SMALL: usize = 100_000;

/// The triangles' boxes cut into tiles by median splits of their centres (along the longer plan
/// axis each time), with a BVH per tile built on its own core: building one BVH over millions of
/// boxes is a single core's work, and most of a search's time. A box is checked against every
/// tile whose bounds it reaches.
struct Tiles {
    tiles: Vec<(Aabb, Bvh)>,
}

impl Tiles {
    fn build(boxes: &[Aabb], count: usize) -> Tiles {
        let centres: Vec<(f64, f64, u32)> = boxes.iter().enumerate().map(|(i, b)| (0.5 * (b.mins.x + b.maxs.x), 0.5 * (b.mins.y + b.maxs.y), i as u32)).collect();
        let parts = split(centres, count.max(1));
        let tiles = std::thread::scope(|scope| {
            let handles: Vec<_> = parts
                .into_iter()
                .filter(|part| !part.is_empty())
                .map(|part| {
                    scope.spawn(move || {
                        let bvh = Bvh::from_iter(BvhBuildStrategy::Binned, part.iter().map(|&(_, _, i)| (i as usize, boxes[i as usize])));
                        let bounds = part.iter().map(|&(_, _, i)| boxes[i as usize]).reduce(|a, b| Aabb::new(a.mins.min(b.mins), a.maxs.max(b.maxs)));
                        (bounds.expect("a tile has boxes"), bvh)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("a BVH thread panicked")).collect()
        });
        Tiles { tiles }
    }

    /// The boxes overlapping `query`, by index.
    fn overlapping<'a>(&'a self, query: &'a Aabb) -> impl Iterator<Item = u32> + 'a {
        let reaches = |b: &Aabb| (0..3).all(|k| b.mins[k] <= query.maxs[k] && query.mins[k] <= b.maxs[k]);
        self.tiles.iter().filter(move |(bounds, _)| reaches(bounds)).flat_map(move |(_, bvh)| bvh.intersect_aabb(query))
    }
}

/// Cuts box centres into `count` parts of near-equal size, halving each part at its median along
/// its longer axis in plan; the halves are cut on cores of their own.
fn split(mut part: Vec<(f64, f64, u32)>, count: usize) -> Vec<Vec<(f64, f64, u32)>> {
    if count <= 1 || part.len() < 2 {
        return vec![part];
    }
    let spread = |f: fn(&(f64, f64, u32)) -> f64| {
        let (lo, hi) = part.iter().map(f).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        hi - lo
    };
    let along_x = spread(|c| c.0) >= spread(|c| c.1);
    let left_count = count / 2;
    let mid = part.len() * left_count / count;
    if along_x {
        part.select_nth_unstable_by(mid, |a, b| a.0.total_cmp(&b.0));
    } else {
        part.select_nth_unstable_by(mid, |a, b| a.1.total_cmp(&b.1));
    }
    let right = part.split_off(mid);
    std::thread::scope(|scope| {
        let other = scope.spawn(move || split(right, count - left_count));
        let mut out = split(part, left_count);
        out.extend(other.join().expect("a split thread panicked"));
        out
    })
}

/// How two triangles cross, if they do: the piece they share, or a point where they overlap.
fn crossing(ta: [u32; 3], tb: [u32; 3], v: &[P], tol: f64, rules: Rules) -> Option<Crossing> {
    let shared = ta.iter().filter(|i| tb.contains(i)).count();
    if shared == 3 {
        return None;
    }
    let (a, b) = (ta.map(|i| v[i as usize]), tb.map(|i| v[i as usize]));
    let (na, nb) = (unit_normal(a)?, unit_normal(b)?);
    // Each triangle's corners measured from the other's plane.
    let from_a = b.map(|p| dot(na, sub(p, a[0])));
    let from_b = a.map(|p| dot(nb, sub(p, b[0])));
    let coplanar = from_a.iter().chain(&from_b).all(|d| d.abs() <= tol);
    // By the solid rules, faces in one plane facing opposite ways are thin.
    let opposite = rules == Rules::Solid && dot(na, nb) < 0.0;
    let thin_if = |thin: bool, overlap: P| if thin { Crossing::Thin(overlap) } else { Crossing::Overlap(overlap) };

    if shared == 2 {
        // On one edge, two triangles meet only along it unless they fold back onto each other in
        // one plane: their third corners on the same side of the edge.
        if !coplanar {
            return None;
        }
        let (Some(a3), Some(b3)) = (ta.iter().position(|i| !tb.contains(i)), tb.iter().position(|i| !ta.contains(i))) else {
            return None;
        };
        let (u, w) = (a[(a3 + 1) % 3], a[(a3 + 2) % 3]);
        let side = |p: P| dot(na, cross(sub(w, u), sub(p, u))) / norm(sub(w, u));
        let (sa, sb) = (side(a[a3]), side(b[b3]));
        let folded = (sa > tol && sb > tol) || (sa < -tol && sb < -tol);
        // Folded on an edge they enclose nothing, even coinciding.
        return folded.then(|| thin_if(opposite, mid(centroid(a), centroid(b))));
    }
    if coplanar {
        // Apart, or they would be two parts of the solid lying exactly on each other.
        let apart = from_a.iter().chain(&from_b).any(|d| d.abs() > COINCIDENT);
        return overlap_in_plane(a, na, b, tol).map(|p| thin_if(opposite && apart, p));
    }
    let straddles = |d: &[f64; 3]| d.iter().any(|x| *x > tol) && d.iter().any(|x| *x < -tol);
    if !straddles(&from_a) || !straddles(&from_b) {
        return None;
    }
    let line = cross(na, nb);
    let length = norm(line);
    if length < 1e-12 {
        return None;
    }
    let line = line.map(|x| x / length);
    let ((a_lo, pa_lo), (a_hi, pa_hi)) = span_on_line(a, from_b, line)?;
    let ((b_lo, pb_lo), (b_hi, pb_hi)) = span_on_line(b, from_a, line)?;
    let (lo, p_lo) = if a_lo >= b_lo { (a_lo, pa_lo) } else { (b_lo, pb_lo) };
    let (hi, p_hi) = if a_hi <= b_hi { (a_hi, pa_hi) } else { (b_hi, pb_hi) };
    (hi - lo > tol).then_some(Crossing::Segment([p_lo, p_hi]))
}

/// True when triangles `i` and `j`, overlapping the same way at `p`, have a face between them there
/// that points the other way: the line through `p` along their normal, from one to the other,
/// crosses more faces pointing against it than with it. Going through all three then runs solid,
/// gap, solid, gap: a lip over a gap. Without one, the solid under both is there twice.
fn lip_between(i: usize, j: usize, p: P, triangles: &[[u32; 3]], v: &[P], tiles: &Tiles) -> bool {
    let corners = |k: usize| triangles[k].map(|c| v[c as usize]);
    let (a, b) = (corners(i), corners(j));
    let (Some(na), Some(nb)) = (unit_normal(a), unit_normal(b)) else { return false };
    if dot(na, nb) <= 0.0 {
        return false;
    }
    // Where the line through `p` along `na` meets each face's plane, measured from `p`.
    let meets = |t: [P; 3], n: P, along: f64| dot(sub(t[0], p), n) / along;
    let (sa, sb) = (meets(a, na, 1.0), meets(b, nb, dot(na, nb)));
    let (lo, hi) = (sa.min(sb), sa.max(sb));
    if hi - lo <= COINCIDENT {
        return false;
    }
    let point = |s: f64| [0, 1, 2].map(|k| p[k] + s * na[k]);
    let (from, to) = (point(lo), point(hi));
    let query = Aabb::new(Vector::new(from[0].min(to[0]), from[1].min(to[1]), from[2].min(to[2])), Vector::new(from[0].max(to[0]), from[1].max(to[1]), from[2].max(to[2])));
    let mut against = 0;
    for k in tiles.overlapping(&query).map(|k| k as usize).filter(|&k| k != i && k != j) {
        let c = corners(k);
        let Some(nc) = unit_normal(c) else { continue };
        let along = dot(nc, na);
        if along.abs() < 1e-9 {
            continue;
        }
        let s = meets(c, nc, along);
        if s > lo + COINCIDENT && s < hi - COINCIDENT && holds(c, nc, point(s)) {
            against += if along < 0.0 { 1 } else { -1 };
        }
    }
    against > 0
}

/// Whether point `q`, in the plane of triangle `t` with unit normal `n`, lies in the triangle or
/// within [`COINCIDENT`] of it.
fn holds(t: [P; 3], n: P, q: P) -> bool {
    (0..3).all(|k| {
        let (u, w) = (t[k], t[(k + 1) % 3]);
        let edge = sub(w, u);
        dot(cross(edge, sub(q, u)), n) >= -COINCIDENT * norm(edge)
    })
}

/// The piece of triangle `t` lying in the other triangle's plane (`d`: its corners measured from
/// that plane), as its two ends measured along `line` with their points. Found exactly: a corner
/// counts as in the plane only when it coincides with it. (Taking corners within the tolerance as
/// in the plane stretched the piece past where the triangles meet, and found crossings between
/// triangles that only pass within the tolerance of each other; decision 0014.)
fn span_on_line(t: [P; 3], d: [f64; 3], line: P) -> Option<((f64, P), (f64, P))> {
    let mut ends: Option<((f64, P), (f64, P))> = None;
    let mut take = |p: P| {
        let s = dot(line, p);
        ends = Some(match ends {
            None => ((s, p), (s, p)),
            Some((lo, hi)) => (if s < lo.0 { (s, p) } else { lo }, if s > hi.0 { (s, p) } else { hi }),
        });
    };
    for k in 0..3 {
        let (i, j) = (k, (k + 1) % 3);
        if d[i].abs() <= COINCIDENT {
            take(t[i]);
        }
        if (d[i] > COINCIDENT && d[j] < -COINCIDENT) || (d[i] < -COINCIDENT && d[j] > COINCIDENT) {
            let s = d[i] / (d[i] - d[j]);
            take([0, 1, 2].map(|c| t[i][c] + s * (t[j][c] - t[i][c])));
        }
    }
    ends
}

/// Two triangles in one plane overlap by more than `tol`: `b` clipped to `a` shrunk by `tol` leaves
/// some area. Returns the middle of what is left.
fn overlap_in_plane(a: [P; 3], na: P, b: [P; 3], tol: f64) -> Option<P> {
    let mut piece: Vec<P> = b.to_vec();
    for k in 0..3 {
        let (p, q) = (a[k], a[(k + 1) % 3]);
        let edge = sub(q, p);
        let length = norm(edge);
        // Inside the shrunk triangle where this is positive.
        let inside = |x: P| dot(cross(edge, sub(x, p)), na) / length - tol;
        let mut clipped = Vec::with_capacity(piece.len() + 1);
        for i in 0..piece.len() {
            let (x, y) = (piece[i], piece[(i + 1) % piece.len()]);
            let (dx, dy) = (inside(x), inside(y));
            if dx >= 0.0 {
                clipped.push(x);
            }
            if (dx >= 0.0) != (dy >= 0.0) {
                let s = dx / (dx - dy);
                clipped.push([0, 1, 2].map(|c| x[c] + s * (y[c] - x[c])));
            }
        }
        piece = clipped;
        if piece.len() < 3 {
            return None;
        }
    }
    let o = piece[0];
    let twice_area: f64 = (1..piece.len() - 1).map(|i| dot(cross(sub(piece[i], o), sub(piece[i + 1], o)), na)).sum();
    if twice_area.abs() <= 2e-12 {
        return None;
    }
    let n = piece.len() as f64;
    Some([0, 1, 2].map(|c| piece.iter().map(|p| p[c]).sum::<f64>() / n))
}

fn unit_normal([a, b, c]: [P; 3]) -> Option<P> {
    let n = cross(sub(b, a), sub(c, a));
    let length = norm(n);
    (length > 0.0).then(|| n.map(|x| x / length))
}

fn centroid([a, b, c]: [P; 3]) -> P {
    [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
}

fn mid(a: P, b: P) -> P {
    [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn at(points: &[P]) -> Vec<P> {
        points.iter().map(|p| [E + p[0], N + p[1], p[2]]).collect()
    }

    /// A flat triangle in z = 0 and a vertical one in y = 2 whose lowest corner is `depth` below it.
    fn pierced(depth: f64) -> TriMesh {
        TriMesh::new(at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [1.0, 2.0, -depth], [4.0, 2.0, -depth], [2.5, 2.0, 5.0]]), vec![[0, 1, 2], [3, 4, 5]])
    }

    #[test]
    fn a_triangle_through_another_is_one_crossing_where_it_passes() {
        let found = find_crossings(&pierced(5.0), 0.001);
        assert_eq!(found.pairs, 1);
        assert_eq!(found.check, CrossingCheck::Search { tolerance: 0.001 });
        // The two meet along x = 1.75 to 3.25 on y = 2, z = 0; the site is its middle.
        assert_eq!(found.at[0], [E + 2.5, N + 2.0, 0.0]);
    }

    #[test]
    fn crossings_within_the_tolerance_are_not_counted() {
        // 0.5 mm through: a crossing at 0.1 mm, not at 1 mm.
        assert_eq!(find_crossings(&pierced(0.0005), 0.001).pairs, 0);
        assert_eq!(find_crossings(&pierced(0.0005), 0.0001).pairs, 1);
    }

    #[test]
    fn triangles_touching_near_their_corners_do_not_cross() {
        // Each passes through the other's plane by more than 1 mm, but they only touch, at one point,
        // where a corner of each comes within 1 mm of the other's plane. Taking those corners as in
        // the plane stretched the shared piece past 1 mm and called it a crossing.
        let mesh = TriMesh::new(at(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.003, -0.002, -0.0002], [0.5, 0.5, 0.011], [1.3, -0.3, -0.028]]), vec![[0, 1, 2], [3, 4, 5]]);
        for tolerance in [0.001, 0.0001, COINCIDENT] {
            assert_eq!(find_crossings(&mesh, tolerance).pairs, 0, "at {tolerance}");
        }
        assert_eq!(find_solid_crossings(&mesh, 0.001).pairs, 0);
    }

    #[test]
    fn touching_is_not_crossing() {
        // A corner resting on the flat triangle, and a triangle standing on its edge.
        let v = at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [2.0, 2.0, 0.0], [5.0, 2.0, 5.0], [2.0, 5.0, 5.0], [3.0, 0.0, 0.0], [6.0, 0.0, 4.0]]);
        let mesh = TriMesh::new(v, vec![[0, 1, 2], [3, 4, 5], [6, 1, 7]]);
        assert_eq!(find_crossings(&mesh, 0.001).pairs, 0);
    }

    #[test]
    fn a_fold_and_a_coplanar_overlap_are_crossings() {
        // Two triangles on one edge, the second folded back flat onto the first.
        let fold = TriMesh::new(at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [2.0, 3.0, 0.0]]), vec![[0, 1, 2], [1, 0, 3]]);
        assert_eq!(find_crossings(&fold, 0.001).pairs, 1);
        // Two separate triangles in one plane, overlapping.
        let overlap = TriMesh::new(at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [2.0, 2.0, 0.0], [12.0, 2.0, 0.0], [2.0, 12.0, 0.0]]), vec![[0, 1, 2], [3, 4, 5]]);
        assert_eq!(find_crossings(&overlap, 0.001).pairs, 1);
    }

    /// A box from `min` to `max`, wound outward.
    fn cuboid(min: P, max: P) -> TriMesh {
        let v = (0..8).map(|i| [if i & 1 == 0 { min[0] } else { max[0] }, if i >> 1 & 1 == 0 { min[1] } else { max[1] }, if i >> 2 == 0 { min[2] } else { max[2] }]).collect();
        TriMesh::new(v, vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]])
    }

    fn joined(a: &TriMesh, b: &TriMesh) -> TriMesh {
        let mut m = a.clone();
        let base = m.vertices.len() as u32;
        m.vertices.extend(&b.vertices);
        m.triangles.extend(b.triangles.iter().map(|t| t.map(|i| i + base)));
        m
    }

    /// A wedge 10 m long and wide whose top rises from a knife edge at x = 0 to `height` at x = 10.
    pub(crate) fn knife_edge(height: f64) -> TriMesh {
        let v = at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, height], [0.0, 10.0, 0.0], [10.0, 10.0, 0.0], [10.0, 10.0, height]]);
        TriMesh::new(v, vec![[0, 3, 1], [1, 3, 4], [0, 2, 3], [2, 5, 3], [1, 4, 5], [1, 5, 2], [0, 1, 2], [3, 5, 4]])
    }

    #[test]
    fn a_knife_edge_is_a_fold_on_a_surface_and_thin_in_a_solid() {
        // Its top and bottom meet at the knife edge and stay within 0.5 mm of each other: by the
        // surface rules they fold back onto each other; in a solid they are thin, not crossing.
        let wedge = knife_edge(0.0005);
        assert!(wedge.validity().is_watertight() && (wedge.signed_volume() - 0.025).abs() < 1e-12);
        let surface = find_crossings(&wedge, 0.001);
        assert!(surface.pairs > 0 && surface.thin == 0, "{surface:?}");
        let solid = find_solid_crossings(&wedge, 0.001);
        assert_eq!(solid.pairs, 0, "{solid:?}");
        assert_eq!(solid.check, CrossingCheck::SolidSearch { tolerance: 0.001 });
        assert!(solid.thin > 0 && solid.thin_at.len() == solid.thin);
        // At 0.1 mm the faces are not within the tolerance of one plane: nothing either way.
        assert_eq!((find_solid_crossings(&wedge, 0.0001).pairs, find_solid_crossings(&wedge, 0.0001).thin), (0, 0));
    }

    #[test]
    fn a_hairline_gap_is_thin_in_a_solid() {
        // Two cubes 0.5 mm apart: the walls facing each other are within 1 mm of one plane.
        let gap = joined(&cuboid([E, N, 0.0], [E + 10.0, N + 10.0, 10.0]), &cuboid([E + 10.0005, N + 2.0, 1.0], [E + 20.0, N + 12.0, 11.0]));
        assert!(find_crossings(&gap, 0.001).pairs > 0);
        let solid = find_solid_crossings(&gap, 0.001);
        assert_eq!(solid.pairs, 0, "{solid:?}");
        assert!(solid.thin > 0);
    }

    #[test]
    fn coinciding_faces_and_faces_overlapping_the_same_way_still_cross_in_a_solid() {
        // A cube sitting offset on another: the faces where they touch coincide, facing opposite ways.
        let sitting = joined(&cuboid([E, N, 0.0], [E + 10.0, N + 10.0, 10.0]), &cuboid([E + 5.0, N + 3.0, 10.0], [E + 15.0, N + 13.0, 20.0]));
        let found = find_solid_crossings(&sitting, 0.001);
        assert!(found.pairs > 0 && found.thin == 0, "{found:?}");
        // Two cubes overlapping with their tops and bottoms in the same planes, facing the same way.
        let beside = joined(&cuboid([E, N, 0.0], [E + 10.0, N + 10.0, 10.0]), &cuboid([E + 5.0, N + 5.0, 0.0], [E + 15.0, N + 15.0, 10.0]));
        assert!(find_solid_crossings(&beside, 0.001).pairs > 0);
        // And a pass-through is a crossing by either rules.
        assert_eq!(find_solid_crossings(&pierced(5.0), 0.001).pairs, 1);
    }

    /// A 10 m long prism whose profile (x, z) is a block with a step in its top: the top at 10.0008
    /// reaches over the top at 10 from x = 9 to 11, its underside falling back to x = 9, so at x = 10
    /// the line up runs top at 10, underside at 10.0004, top at 10.0008.
    fn lip() -> TriMesh {
        let profile = [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [9.0, 10.0], [11.0, 10.0008], [0.0, 10.0008]];
        let n = profile.len() as u32;
        let mut points = Vec::new();
        for y in [0.0, 10.0] {
            points.extend(profile.iter().map(|[x, z]| [*x, y, *z]));
        }
        let mut t = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            t.extend([[i, j + n, j], [i, i + n, j + n]]);
        }
        // Each end, fanned about the step's foot (corner 3).
        for k in [4, 5, 0, 1] {
            let next = (k + 1) % n;
            t.extend([[3, k, next], [3 + n, next + n, k + n]]);
        }
        TriMesh::new(at(&points), t)
    }

    #[test]
    fn a_lip_over_a_gap_is_thin_in_a_solid() {
        let lip = lip();
        let shape = lip.validity();
        assert!(shape.is_watertight(), "{shape:?}");
        // The two tops overlap the same way within 1 mm, with the lip's underside between them.
        assert!(find_crossings(&lip, 0.001).pairs > 0);
        let found = find_solid_crossings(&lip, 0.001);
        assert_eq!(found.pairs, 0, "{found:?}");
        assert!(found.thin > 0);

        // Two blocks overlapping with their tops 0.5 mm apart, facing the same way, and nothing
        // between them: the solid is there twice, and that still crosses.
        let stacked = joined(&cuboid([E, N, 0.0], [E + 10.0, N + 10.0, 10.0]), &cuboid([E + 5.0, N + 5.0, 0.0], [E + 15.0, N + 15.0, 10.0005]));
        assert!(find_solid_crossings(&stacked, 0.001).pairs > 0);
    }

    #[test]
    fn a_flat_fold_on_an_edge_is_thin_in_a_solid_and_crosses_on_a_surface() {
        // Two faces on one edge, folded exactly flat, facing opposite ways: a knife edge of no thickness.
        let fold = TriMesh::new(at(&[[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [2.0, 3.0, 0.0]]), vec![[0, 1, 2], [1, 0, 3]]);
        assert_eq!(find_crossings(&fold, 0.001).pairs, 1);
        let found = find_solid_crossings(&fold, 0.001);
        assert_eq!((found.pairs, found.thin), (0, 1), "{found:?}");
    }

    #[test]
    fn clean_surfaces_and_solids_have_none() {
        // Flat and rolling grids: every neighbour shares a corner or an edge, many in one plane.
        let flat = triangulate_grid(&Grid::from_fn([E, N], [5.0, 5.0], 60, 40, |_, _| 100.0));
        assert_eq!(find_crossings(&flat, 0.001).pairs, 0);
        let rolling = triangulate_grid(&Grid::from_fn([E, N], [5.0, 5.0], 60, 40, |x, y| 100.0 + 8.0 * (x / 40.0).sin() * (y / 25.0).cos()));
        assert_eq!(find_crossings(&rolling, 0.001).pairs, 0);
        let v: Vec<P> = (0..8).map(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 20.0, (i >> 2) as f64 * 5.0]).collect();
        let solid = TriMesh::new(v, vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]]);
        assert_eq!(find_crossings(&solid, 0.001).pairs, 0);
    }

    #[test]
    fn crossings_whose_edges_meet_exactly_are_found() {
        // Two boxes run into each other on a 5 m grid: every place an edge meets a face lies exactly
        // on another triangle's edge, so only the interval test sees the crossing.
        let corner = |min: [f64; 3], i: usize| [min[0] + (i & 1) as f64 * 10.0, min[1] + (i >> 1 & 1) as f64 * 10.0, min[2] + (i >> 2) as f64 * 10.0];
        let faces = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        let mut v: Vec<P> = (0..8).map(|i| corner([E, N, 0.0], i)).collect();
        v.extend((0..8).map(|i| corner([E + 5.0, N + 5.0, 5.0], i)));
        let mut t: Vec<[u32; 3]> = faces.to_vec();
        t.extend(faces.iter().map(|f| f.map(|i| i + 8)));
        let found = find_crossings(&TriMesh::new(v, t), 0.001);
        assert!(found.pairs >= 6, "{}", found.pairs);
        // Every site lies on a face of each box.
        let on_face = |p: P, min: [f64; 3]| (0..3).any(|k| (p[k] - min[k]).abs() < 1e-9 || (p[k] - min[k] - 10.0).abs() < 1e-9);
        assert!(found.at.iter().all(|p| on_face(*p, [E, N, 0.0]) && on_face(*p, [E + 5.0, N + 5.0, 5.0])), "{:?}", found.at);
    }

    #[test]
    fn a_surface_folded_over_itself_crosses_where_the_fold_passes_through() {
        // A rolling grid with a second sheet cutting through it at a slope: every triangle of the
        // sheet that the grid passes through is a crossing, and the count is the same on any number
        // of cores (each pair once).
        // (`from_fn` passes mine grid coordinates.)
        let grid = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 21, 21, |x, _| 100.0 + (x - E) / 20.0));
        let sheet = triangulate_grid(&Grid::from_fn([E + 5.0, N + 5.0], [10.0, 10.0], 3, 3, |x, _| 95.0 + (x - E - 5.0) / 2.0));
        let base = grid.vertices.len() as u32;
        let mut mesh = grid.clone();
        mesh.vertices.extend(&sheet.vertices);
        mesh.triangles.extend(sheet.triangles.iter().map(|t| t.map(|i| i + base)));
        let found = find_crossings(&mesh, 0.001);
        assert!(found.pairs > 0 && found.at.len() == found.pairs.min(Crossings::SITES));
        // Every site lies on the sheet and on the grid, to within a millimetre.
        for p in &found.at {
            let (x, z) = (p[0] - E, p[2]);
            assert!((z - (100.0 + x / 20.0)).abs() < 1e-6 && (z - (95.0 + (x - 5.0) / 2.0)).abs() < 1e-6, "{p:?}");
        }
    }

    #[test]
    fn tiles_find_exactly_what_one_bvh_finds() {
        // 125,000 triangles of rolling grid with a sheet cut through it, searched as one tile and as
        // many: the same pairs and the same first sites.
        let grid = triangulate_grid(&Grid::from_fn([E, N], [2.0, 2.0], 251, 251, |x, y| 100.0 + 3.0 * ((x - E) / 40.0).sin() + (y - N) / 100.0));
        let sheet = triangulate_grid(&Grid::from_fn([E + 1.0, N + 1.0], [25.0, 25.0], 21, 21, |x, _| 96.0 + (x - E) / 50.0));
        let base = grid.vertices.len() as u32;
        let mut mesh = grid.clone();
        mesh.vertices.extend(&sheet.vertices);
        mesh.triangles.extend(sheet.triangles.iter().map(|t| t.map(|i| i + base)));
        assert!(mesh.triangle_count() > SMALL);
        let one = search(&mesh, 0.001, Some(1), Rules::Surface);
        assert!(one.pairs > 0);
        for tiles in [2, 7, 48] {
            assert_eq!(search(&mesh, 0.001, Some(tiles), Rules::Surface), one, "{tiles} tiles");
        }
    }

    #[test]
    #[ignore = "timing: cargo test -p zd-geom --release -- --ignored --nocapture"]
    fn timing_on_two_million_triangles() {
        use crate::repair_mesh;
        let grid = Grid::from_fn([E, N], [2.0, 2.0], 1001, 1001, |x, y| 100.0 + 15.0 * (x / 300.0).sin() * (y / 200.0).cos());
        let mesh = triangulate_grid(&grid);
        let t = std::time::Instant::now();
        let (repaired, log) = repair_mesh(&mesh, 0.001);
        let repair_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = std::time::Instant::now();
        let found = find_crossings(&repaired, 0.001);
        let search_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "{} triangles in: repair {repair_ms:.0} ms ({} welded, {} flipped), crossing search {search_ms:.0} ms on {} cores, {} triangles out, {} crossings",
            mesh.triangle_count(),
            log.vertices_welded,
            log.orient.triangles_flipped,
            std::thread::available_parallelism().map_or(1, |n| n.get()),
            repaired.triangle_count(),
            found.pairs
        );
        assert_eq!(found.pairs, 0);
    }
}
