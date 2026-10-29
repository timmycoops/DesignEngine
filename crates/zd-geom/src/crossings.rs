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

use parry3d_f64::bounding_volume::Aabb;
use parry3d_f64::math::Vector;
use parry3d_f64::partitioning::{Bvh, BvhBuildStrategy};
use zd_core::{CrossingCheck, Crossings, TriMesh};

type P = [f64; 3];

/// Every crossing pair in the mesh, with where the first [`Crossings::SITES`] cross (in triangle
/// order, so the result is the same on any number of cores).
pub fn find_crossings(mesh: &TriMesh, tolerance: f64) -> Crossings {
    search(mesh, tolerance, None)
}

/// The search, with the number of tiles fixed (for tests) or chosen by the mesh's size.
fn search(mesh: &TriMesh, tolerance: f64, tile_count: Option<usize>) -> Crossings {
    let tolerance = tolerance.max(0.0);
    let check = CrossingCheck::Search { tolerance };
    let Some(bounds) = mesh.bounds() else {
        return Crossings { check, pairs: 0, at: Vec::new() };
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
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, triangles.len().max(1));
    let tiles = Tiles::build(&boxes, tile_count.unwrap_or(if triangles.len() < SMALL { 1 } else { threads }));
    let chunk = triangles.len().div_ceil(threads);
    // Each core takes a run of triangles in order and keeps its count and its first sites.
    let parts: Vec<(usize, Vec<P>)> = std::thread::scope(|scope| {
        let (tiles, boxes, local) = (&tiles, &boxes, &local);
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                scope.spawn(move || {
                    let mut count = 0;
                    let mut sites = Vec::new();
                    let mut hits: Vec<(u32, P)> = Vec::new();
                    for i in k * chunk..((k + 1) * chunk).min(triangles.len()) {
                        hits.clear();
                        for j in tiles.overlapping(&boxes[i]) {
                            if j as usize > i
                                && let Some(at) = crossing(triangles[i], triangles[j as usize], local, tolerance)
                            {
                                hits.push((j, at));
                            }
                        }
                        hits.sort_by_key(|h| h.0);
                        count += hits.len();
                        sites.extend(hits.iter().take(Crossings::SITES - sites.len().min(Crossings::SITES)).map(|h| h.1));
                    }
                    (count, sites)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a crossing search thread panicked")).collect()
    });

    let pairs = parts.iter().map(|p| p.0).sum();
    let at = parts.into_iter().flat_map(|p| p.1).take(Crossings::SITES).map(|p| add(p, origin)).collect();
    Crossings { check, pairs, at }
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

/// Where two triangles cross, if they do: the middle of the piece they share.
fn crossing(ta: [u32; 3], tb: [u32; 3], v: &[P], tol: f64) -> Option<P> {
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
        return folded.then(|| mid(centroid(a), centroid(b)));
    }
    if coplanar {
        return overlap_in_plane(a, na, b, tol);
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
    let ((a_lo, pa_lo), (a_hi, pa_hi)) = span_on_line(a, from_b, line, tol)?;
    let ((b_lo, pb_lo), (b_hi, pb_hi)) = span_on_line(b, from_a, line, tol)?;
    let (lo, p_lo) = if a_lo >= b_lo { (a_lo, pa_lo) } else { (b_lo, pb_lo) };
    let (hi, p_hi) = if a_hi <= b_hi { (a_hi, pa_hi) } else { (b_hi, pb_hi) };
    (hi - lo > tol).then(|| mid(p_lo, p_hi))
}

/// The piece of triangle `t` lying in the other triangle's plane (`d`: its corners measured from
/// that plane), as its two ends measured along `line` with their points.
fn span_on_line(t: [P; 3], d: [f64; 3], line: P, tol: f64) -> Option<((f64, P), (f64, P))> {
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
        if d[i].abs() <= tol {
            take(t[i]);
        }
        if (d[i] > tol && d[j] < -tol) || (d[i] < -tol && d[j] > tol) {
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
        let one = search(&mesh, 0.001, Some(1));
        assert!(one.pairs > 0);
        for tiles in [2, 7, 48] {
            assert_eq!(search(&mesh, 0.001, Some(tiles)), one, "{tiles} tiles");
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
