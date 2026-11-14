//! A face laid on the grids it was built to (decision 0064). A berm registered on a grid touches it at its corners, but
//! its triangles reach across the grid's cells, so between corners it's a chord: up to centimetres above or below the
//! grid, weaving through it. Cut by the same grid as a seam's roof or floor, the strip either side of the berm gets a
//! film of coal or rock a few millimetres thick that tapers to a knife edge at every corner.
//!
//! So wherever the face lies on one of its grids, it's split along that grid's triangles and takes its heights from them:
//! - a triangle with all three corners on the grid is cut into the pieces of the grid's triangles under it in plan, each
//!   on its grid triangle's plane;
//! - an edge with both ends on the grid, of such a triangle, gets a corner wherever it crosses a grid edge, at the grid's
//!   height there, and a triangle beside it that isn't on the grid (a wall up from the crest) is fanned to those corners,
//!   so the face stays one sheet.
//!
//! A triangle already on the grid, one the grid doesn't wholly cover in plan, or one more than half a metre off it between
//! its corners (not following it, as a berm on a real seam grid does to within centimetres) is left as it was. Nothing
//! else moves.

use std::collections::HashMap;


use zd_core::TriMesh;

/// A corner within this height of a grid (m) lies on it.
const ON: f64 = 1e-6;

/// A triangle further off its grid than this (m) between its corners isn't laid on it: it doesn't follow the grid, as a
/// berm built to a real seam grid does to within centimetres, and laid on it could fold over the walls beside it.
const FURTHEST: f64 = 0.5;

/// Plan points closer than this (m) are one corner.
const SAME: f64 = 1e-7;

/// What draping did.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GridDrape {
    /// Triangles laid on a grid, and how many they became.
    pub draped: usize,
    pub draped_into: usize,
    /// Triangles beside them given corners on their shared edge.
    pub fanned: usize,
    /// Triangles on a grid it doesn't cover in plan, left as they were.
    pub left: usize,
    /// Triangles with their corners on a grid but more than half a metre off it between them, left as they were.
    pub too_far: usize,
    /// The furthest any point of the face moved to lie on the grid (m): how far the chords were off it.
    pub furthest: f64,
}

/// A grid's triangles indexed by plan cell, near the face.
struct Index<'a> {
    mesh: &'a TriMesh,
    origin: [f64; 2],
    cell: f64,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

impl<'a> Index<'a> {
    /// The triangles of `mesh` reaching into `[lo, hi]` in plan.
    fn new(mesh: &'a TriMesh, lo: [f64; 2], hi: [f64; 2]) -> Option<Self> {
        let tri = |t: &[u32; 3]| t.map(|i| mesh.vertices[i as usize]);
        let near: Vec<u32> = (0..mesh.triangles.len() as u32)
            .filter(|&k| {
                let p = tri(&mesh.triangles[k as usize]);
                !(p.iter().all(|v| v[0] < lo[0]) || p.iter().all(|v| v[0] > hi[0]) || p.iter().all(|v| v[1] < lo[1]) || p.iter().all(|v| v[1] > hi[1]))
            })
            .collect();
        if near.is_empty() {
            return None;
        }
        let area = (hi[0] - lo[0]).max(1e-3) * (hi[1] - lo[1]).max(1e-3);
        let cell = (area / near.len() as f64).sqrt().max(1e-3) * 2.0;
        let (cols, rows) = (((hi[0] - lo[0]) / cell).ceil() as usize + 1, ((hi[1] - lo[1]) / cell).ceil() as usize + 1);
        let mut index = Index { mesh, origin: lo, cell, cols, rows, cells: vec![Vec::new(); cols * rows] };
        for k in near {
            let p = tri(&mesh.triangles[k as usize]);
            let [c0, c1, r0, r1] = index.span(
                [p[0][0].min(p[1][0]).min(p[2][0]), p[0][1].min(p[1][1]).min(p[2][1])],
                [p[0][0].max(p[1][0]).max(p[2][0]), p[0][1].max(p[1][1]).max(p[2][1])],
            );
            for r in r0..=r1 {
                for c in c0..=c1 {
                    index.cells[r * cols + c].push(k);
                }
            }
        }
        Some(index)
    }

    fn span(&self, lo: [f64; 2], hi: [f64; 2]) -> [usize; 4] {
        let at = |v: f64, o: f64, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        [at(lo[0], self.origin[0], self.cols), at(hi[0], self.origin[0], self.cols), at(lo[1], self.origin[1], self.rows), at(hi[1], self.origin[1], self.rows)]
    }

    /// The triangles that may reach into `[lo, hi]`, each once.
    fn candidates(&self, lo: [f64; 2], hi: [f64; 2]) -> Vec<u32> {
        let [c0, c1, r0, r1] = self.span(lo, hi);
        let mut out: Vec<u32> = (r0..=r1).flat_map(|r| (c0..=c1).flat_map(move |c| self.cells[r * self.cols + c].iter().copied())).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn triangle(&self, k: u32) -> [[f64; 3]; 3] {
        self.mesh.triangles[k as usize].map(|i| self.mesh.vertices[i as usize])
    }

    /// The grid's height at a plan point, if it's over the grid.
    fn height(&self, p: [f64; 2]) -> Option<f64> {
        self.candidates(p, p).into_iter().find_map(|k| {
            let t = self.triangle(k);
            let (u, w) = barycentric(t, p)?;
            (u >= -1e-12 && w >= -1e-12 && u + w <= 1.0 + 1e-12).then(|| t[0][2] + u * (t[1][2] - t[0][2]) + w * (t[2][2] - t[0][2]))
        })
    }
}

/// `p` in the plan triangle `t` as weights on its second and third corners.
fn barycentric(t: [[f64; 3]; 3], p: [f64; 2]) -> Option<(f64, f64)> {
    let d = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1]);
    if d.abs() < 1e-18 {
        return None;
    }
    let u = ((p[0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (p[1] - t[0][1])) / d;
    let w = ((t[1][0] - t[0][0]) * (p[1] - t[0][1]) - (p[0] - t[0][0]) * (t[1][1] - t[0][1])) / d;
    Some((u, w))
}

/// Twice the plan area, positive counter-clockwise.
fn plan_area2(t: [[f64; 3]; 3]) -> f64 {
    (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1])
}

/// The grid triangle `b` clipped in plan to the triangle `a`, with heights on `b`'s plane.
fn clip(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> Vec<[f64; 3]> {
    let ccw = plan_area2(a) > 0.0;
    let mut poly: Vec<[f64; 2]> = b.iter().map(|p| [p[0], p[1]]).collect();
    for k in 0..3 {
        let (p, q) = (a[k], a[(k + 1) % 3]);
        let side = |r: [f64; 2]| {
            let s = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            if ccw { s } else { -s }
        };
        let mut out = Vec::with_capacity(poly.len() + 1);
        for i in 0..poly.len() {
            let (r, s) = (poly[i], poly[(i + 1) % poly.len()]);
            let (dr, ds) = (side(r), side(s));
            if dr >= 0.0 {
                out.push(r);
            }
            if (dr >= 0.0) != (ds >= 0.0) {
                let t = dr / (dr - ds);
                out.push([r[0] + t * (s[0] - r[0]), r[1] + t * (s[1] - r[1])]);
            }
        }
        poly = out;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly.dedup_by(|x, y| (x[0] - y[0]).hypot(x[1] - y[1]) < SAME);
    if poly.len() > 1 && (poly[0][0] - poly[poly.len() - 1][0]).hypot(poly[0][1] - poly[poly.len() - 1][1]) < SAME {
        poly.pop();
    }
    let on_b = |r: [f64; 2]| {
        let (u, w) = barycentric(b, r).unwrap_or((0.0, 0.0));
        [r[0], r[1], b[0][2] + u * (b[1][2] - b[0][2]) + w * (b[2][2] - b[0][2])]
    };
    poly.into_iter().map(on_b).collect()
}

/// Where the plan segment `p`–`q` crosses the grid's edges, strictly between its ends: the share along it and the point,
/// at the grid's height.
fn crossings(index: &Index<'_>, p: [f64; 3], q: [f64; 3]) -> Vec<(f64, [f64; 3])> {
    let lo = [p[0].min(q[0]), p[1].min(q[1])];
    let hi = [p[0].max(q[0]), p[1].max(q[1])];
    let mut out: Vec<(f64, [f64; 3])> = Vec::new();
    let d = [q[0] - p[0], q[1] - p[1]];
    let length = d[0].hypot(d[1]);
    if length < SAME {
        return out;
    }
    for k in index.candidates(lo, hi) {
        let t = index.triangle(k);
        // A grid corner on the segment, as where it runs along a grid edge: the triangles either side of it get it.
        for v in t {
            let s = ((v[0] - p[0]) * d[0] + (v[1] - p[1]) * d[1]) / (length * length);
            let off = ((v[0] - p[0]) * d[1] - (v[1] - p[1]) * d[0]).abs() / length;
            if off < SAME && s * length > SAME && (1.0 - s) * length > SAME {
                out.push((s, v));
            }
        }
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            let f = [b[0] - a[0], b[1] - a[1]];
            let den = d[0] * f[1] - d[1] * f[0];
            if den.abs() < 1e-18 {
                continue;
            }
            let g = [a[0] - p[0], a[1] - p[1]];
            let s = (g[0] * f[1] - g[1] * f[0]) / den;
            let u = (g[0] * d[1] - g[1] * d[0]) / den;
            if s * length > SAME && (1.0 - s) * length > SAME && (-1e-12..=1.0 + 1e-12).contains(&u) {
                out.push((s, [p[0] + s * d[0], p[1] + s * d[1], a[2] + u.clamp(0.0, 1.0) * (b[2] - a[2])]));
            }
        }
    }
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    out.dedup_by(|x, y| (x.0 - y.0).abs() * length < SAME);
    out
}

/// `sheet` laid on `grids` wherever it lies on one of them, with what was done.
pub fn drape_on_grids(sheet: &TriMesh, grids: &[&TriMesh]) -> (TriMesh, GridDrape) {
    let mut log = GridDrape::default();
    let Some(bounds) = sheet.bounds() else { return (sheet.clone(), log) };
    let (lo, hi) = ([bounds.min[0] - 1.0, bounds.min[1] - 1.0], [bounds.max[0] + 1.0, bounds.max[1] + 1.0]);
    let indexes: Vec<Index<'_>> = grids.iter().filter_map(|g| Index::new(g, lo, hi)).collect();
    if indexes.is_empty() {
        return (sheet.clone(), log);
    }
    // Which grid each corner lies on, if any.
    let on: Vec<Option<usize>> = sheet
        .vertices
        .iter()
        .map(|v| indexes.iter().position(|ix| ix.height([v[0], v[1]]).is_some_and(|z| (z - v[2]).abs() <= ON)))
        .collect();
    let tri = |t: &[u32; 3]| t.map(|i| sheet.vertices[i as usize]);
    // The triangles that lie on one grid, cut by its triangles; those the grid doesn't cover in plan stay as they are.
    let mut pieces: Vec<Option<Vec<[[f64; 3]; 3]>>> = vec![None; sheet.triangles.len()];
    for (k, t) in sheet.triangles.iter().enumerate() {
        let g = on[t[0] as usize];
        if g.is_none() || on[t[1] as usize] != g || on[t[2] as usize] != g {
            continue;
        }
        let a = tri(t);
        let area = plan_area2(a);
        if area.abs() < 1e-9 {
            continue;
        }
        let index = &indexes[g.unwrap()];
        let lo = [a[0][0].min(a[1][0]).min(a[2][0]), a[0][1].min(a[1][1]).min(a[2][1])];
        let hi = [a[0][0].max(a[1][0]).max(a[2][0]), a[0][1].max(a[1][1]).max(a[2][1])];
        let mut out = Vec::new();
        let mut covered = 0.0;
        for c in index.candidates(lo, hi) {
            let poly = clip(a, index.triangle(c));
            if poly.len() < 3 {
                continue;
            }
            // A triangle as it is; more corners from the middle, as some may lie in a line along one edge.
            let fan: Vec<[[f64; 3]; 3]> = if poly.len() == 3 {
                vec![[poly[0], poly[1], poly[2]]]
            } else {
                let n = poly.len() as f64;
                let middle = [0, 1, 2].map(|k| poly.iter().map(|p| p[k]).sum::<f64>() / n);
                (0..poly.len()).map(|i| [middle, poly[i], poly[(i + 1) % poly.len()]]).collect()
            };
            for mut piece in fan {
                let pa = plan_area2(piece);
                if pa.abs() < 1e-12 {
                    continue;
                }
                if (pa > 0.0) != (area > 0.0) {
                    piece.swap(1, 2);
                }
                covered += pa.abs();
                out.push(piece);
            }
        }
        if (covered - area.abs()).abs() > 1e-6 * area.abs().max(1.0) {
            log.left += 1;
            continue;
        }
        // How far the triangle is off the grid, at the corners it would gain: none, and it already lies on it.
        let off = out
            .iter()
            .flatten()
            .map(|p| {
                let (u, w) = barycentric(a, [p[0], p[1]]).unwrap_or((0.0, 0.0));
                (a[0][2] + u * (a[1][2] - a[0][2]) + w * (a[2][2] - a[0][2]) - p[2]).abs()
            })
            .fold(0.0, f64::max);
        if off > FURTHEST {
            log.too_far += 1;
            continue;
        }
        if out.len() > 1 && off > ON {
            log.draped += 1;
            log.draped_into += out.len();
            log.furthest = log.furthest.max(off);
            pieces[k] = Some(out);
        }
    }
    // The edges of draped triangles, with the corners they gained where they cross the grid's edges.
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut split: HashMap<(u32, u32), Vec<[f64; 3]>> = HashMap::new();
    for (k, t) in sheet.triangles.iter().enumerate() {
        if pieces[k].is_none() {
            continue;
        }
        let g = on[t[0] as usize].unwrap();
        for e in 0..3 {
            let (a, b) = key(t[e], t[(e + 1) % 3]);
            split.entry((a, b)).or_insert_with(|| crossings(&indexes[g], sheet.vertices[a as usize], sheet.vertices[b as usize]).into_iter().map(|(_, p)| p).collect());
        }
    }
    // Everything else, fanned to the corners gained on its edges.
    let mut meshes: Vec<[[f64; 3]; 3]> = Vec::with_capacity(sheet.triangles.len() * 2);
    for (k, t) in sheet.triangles.iter().enumerate() {
        if let Some(out) = pieces[k].take() {
            meshes.extend(out);
            continue;
        }
        let ring: Vec<[f64; 3]> = (0..3)
            .flat_map(|e| {
                let (a, b) = (t[e], t[(e + 1) % 3]);
                let mut along = vec![sheet.vertices[a as usize]];
                if let Some(points) = split.get(&key(a, b)) {
                    let mut points = points.clone();
                    if a > b {
                        points.reverse();
                    }
                    along.extend(points);
                }
                along
            })
            .collect();
        if ring.len() == 3 {
            meshes.push(tri(t));
            continue;
        }
        log.fanned += 1;
        let split_edges: Vec<usize> = (0..3).filter(|&e| split.get(&key(t[e], t[(e + 1) % 3])).is_some_and(|p| !p.is_empty())).collect();
        if split_edges.len() == 1 {
            // From the corner opposite the split edge, so no piece lies along it.
            let apex = sheet.vertices[t[(split_edges[0] + 2) % 3] as usize];
            let at = ring.iter().position(|p| *p == apex).unwrap_or(0);
            let ordered: Vec<[f64; 3]> = ring[at..].iter().chain(&ring[..at]).copied().collect();
            for i in 1..ordered.len() - 1 {
                meshes.push([ordered[0], ordered[i], ordered[i + 1]]);
            }
        } else {
            // From its middle, on its own plane.
            let a = tri(t);
            let middle = [0, 1, 2].map(|c| (a[0][c] + a[1][c] + a[2][c]) / 3.0);
            for i in 0..ring.len() {
                meshes.push([middle, ring[i], ring[(i + 1) % ring.len()]]);
            }
        }
    }
    let loose = TriMesh::new(meshes.iter().flatten().copied().collect(), (0..meshes.len() as u32).map(|k| [3 * k, 3 * k + 1, 3 * k + 2]).collect());
    // A corner a draped triangle gained on an edge it shares with one left as it was, where the two already lie on the
    // grid (an edge along a grid edge, or a neighbour already on the grid), splits that edge too.
    (crate::strips::stitch(crate::face::weld([&loose]), ON).0, log)
}
