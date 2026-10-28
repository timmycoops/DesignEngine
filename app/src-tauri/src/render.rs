//! Render buffers for the viewport: the only place engine f64 coordinates become f32.
//!
//! Two compact layouts. Both carry no normals (the viewport shades flat from screen-space
//! derivatives) and end in a UTF-8 JSON header followed by the header's length as a `u32`, so the
//! arrays start 4-byte aligned at offset 0 and the encoder can time itself without a copy.
//!
//! - `triangulation` (any TIN): the mesh is split into spatially compact chunks of at most
//!   65,535 vertices so indices fit in `u16`. Data: `f32` positions (offset from the scene origin)
//!   for every chunk, then `u16` indices for every chunk, zero-padded to 4 bytes.
//! - `grid` (surfaces read from a regular grid): only elevations travel, as `u16` multiples of one
//!   z step over the surface's range (65,535 = null), in tiles of 255 x 255 nodes. The viewport
//!   rebuilds x and y from the lattice and the triangles by the same rule as
//!   `zd_geom::triangulate_grid`. One step is the whole surface's z range / 65,534, so neighbouring
//!   tiles quantise shared nodes identically and no cracks open between them.

use std::time::Instant;

use serde_json::{Value, json};
use zd_core::{Aabb, Grid, TriMesh};

/// Most vertices a chunk may hold so its indices fit in `u16`.
pub const MAX_CHUNK_VERTICES: usize = 65_535;
/// Chunks are cut from plan bins sized for about this many triangles.
const TARGET_CHUNK_TRIANGLES: usize = 100_000;
/// Nodes along each side of a grid tile (254 cells; 65,025 vertices).
pub const GRID_TILE_NODES: usize = 255;
/// Quantised elevation that marks a null node.
pub const GRID_NULL: u16 = u16::MAX;

pub fn encode_triangulation(mesh: &TriMesh, origin: [f64; 3], header: Value) -> Vec<u8> {
    let start = Instant::now();
    let chunks = chunk_triangles(mesh, MAX_CHUNK_VERTICES, TARGET_CHUNK_TRIANGLES);
    let local = |v: [f64; 3]| [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];

    let vertex_total: usize = chunks.iter().map(|c| c.vertices.len()).sum();
    let index_total: usize = chunks.iter().map(|c| c.indices.len()).sum();
    let mut out = Vec::with_capacity(vertex_total * 12 + index_total * 2 + 4 + 160 * chunks.len());
    let mut meta = Vec::with_capacity(chunks.len());
    let (mut vertex_start, mut index_start) = (0, 0);
    for chunk in &chunks {
        let points: Vec<[f64; 3]> = chunk.vertices.iter().map(|&v| local(mesh.vertices[v as usize])).collect();
        for p in &points {
            for c in p {
                out.extend_from_slice(&(*c as f32).to_le_bytes());
            }
        }
        let bounds = Aabb::from_points(&points).expect("chunks are never empty");
        meta.push(json!({
            "vertex_start": vertex_start,
            "vertex_count": chunk.vertices.len(),
            "index_start": index_start,
            "index_count": chunk.indices.len(),
            "min": bounds.min,
            "max": bounds.max,
        }));
        vertex_start += chunk.vertices.len();
        index_start += chunk.indices.len();
    }
    for chunk in &chunks {
        for i in &chunk.indices {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }
    pad_to_4(&mut out);

    let mut header = header;
    header["kind"] = "triangulation".into();
    header["vertices"] = vertex_total.into();
    header["chunks"] = meta.into();
    finish(out, header, mesh.triangle_count(), mesh.bounds().map(|b| local_bounds(b, origin)), start)
}

pub fn encode_grid(grid: &Grid, origin: [f64; 3], header: Value) -> Vec<u8> {
    let start = Instant::now();
    let live = || grid.z.iter().copied().filter(|z| !z.is_nan());
    let z_min = live().fold(f64::INFINITY, f64::min);
    let z_max = live().fold(f64::NEG_INFINITY, f64::max);
    let (z_min, z_max) = if z_min.is_finite() { (z_min, z_max) } else { (0.0, 0.0) };
    let z_step = if z_max > z_min { (z_max - z_min) / f64::from(GRID_NULL - 1) } else { 1.0 };
    let quantise = |z: f64| {
        if z.is_nan() { GRID_NULL } else { ((z - z_min) / z_step).round().clamp(0.0, f64::from(GRID_NULL - 1)) as u16 }
    };

    let mut out = Vec::with_capacity(grid.z.len() * 2 + 4096);
    let mut tiles = Vec::new();
    let mut offset = 0;
    let mut triangles = 0;
    let stride = GRID_TILE_NODES - 1;
    for row0 in (0..grid.rows.saturating_sub(1)).step_by(stride) {
        let rows = (grid.rows - row0).min(GRID_TILE_NODES);
        for col0 in (0..grid.cols.saturating_sub(1)).step_by(stride) {
            let cols = (grid.cols - col0).min(GRID_TILE_NODES);
            let tile_triangles = count_cell_triangles(grid, col0, row0, cols, rows);
            if tile_triangles == 0 {
                continue;
            }
            let mut nulls = false;
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for row in row0..row0 + rows {
                for col in col0..col0 + cols {
                    let z = grid.z_at(col, row);
                    if z.is_nan() {
                        nulls = true;
                    } else {
                        lo = lo.min(z);
                        hi = hi.max(z);
                    }
                    out.extend_from_slice(&quantise(z).to_le_bytes());
                }
            }
            let [x0, y0] = grid.node_xy(col0, row0);
            let [x1, y1] = grid.node_xy(col0 + cols - 1, row0 + rows - 1);
            tiles.push(json!({
                "col": col0,
                "row": row0,
                "cols": cols,
                "rows": rows,
                "offset": offset,
                "nulls": nulls,
                "triangles": tile_triangles,
                "min": [x0 - origin[0], y0 - origin[1], lo - origin[2]],
                "max": [x1 - origin[0], y1 - origin[1], hi - origin[2]],
            }));
            offset += cols * rows;
            triangles += tile_triangles;
        }
    }
    pad_to_4(&mut out);

    let mut header = header;
    header["kind"] = "grid".into();
    header["vertices"] = offset.into();
    header["lattice"] = json!({
        "origin": [grid.origin[0] - origin[0], grid.origin[1] - origin[1]],
        "cell_size": grid.cell_size,
    });
    header["z"] = json!({ "min": z_min - origin[2], "step": z_step, "null": GRID_NULL });
    header["tiles"] = tiles.into();
    let bounds = Aabb::from_points(&[
        [grid.origin[0], grid.origin[1], z_min],
        [grid.node_xy(grid.cols - 1, grid.rows - 1)[0], grid.node_xy(grid.cols - 1, grid.rows - 1)[1], z_max],
    ]);
    finish(out, header, triangles, bounds.map(|b| local_bounds(b, origin)), start)
}

/// A chunk of a triangulation: global vertex ids and `u16` indices into them.
pub struct Chunk {
    pub vertices: Vec<u32>,
    pub indices: Vec<u16>,
}

/// Splits a triangulation into chunks of at most `max_vertices` vertices. Triangles are binned in
/// plan by centroid (about `target_triangles` per bin) so each chunk covers a compact area; a bin
/// that needs more vertices than allowed is split into several chunks. Every triangle lands in
/// exactly one chunk; vertices on chunk seams are repeated.
pub fn chunk_triangles(mesh: &TriMesh, max_vertices: usize, target_triangles: usize) -> Vec<Chunk> {
    let Some(bounds) = mesh.bounds() else { return Vec::new() };
    let [w, h, _] = bounds.size();
    let bins_wanted = mesh.triangles.len().div_ceil(target_triangles).max(1) as f64;
    let aspect = if h > 0.0 { (w / h).max(1e-6) } else { 1.0 };
    let nx = (bins_wanted * aspect).sqrt().ceil().max(1.0) as usize;
    let ny = (bins_wanted / nx as f64).ceil().max(1.0) as usize;
    let bin_of = |&[a, b, c]: &[u32; 3]| {
        let centroid = |axis: usize| {
            (mesh.vertices[a as usize][axis] + mesh.vertices[b as usize][axis] + mesh.vertices[c as usize][axis]) / 3.0
        };
        let fx = if w > 0.0 { (centroid(0) - bounds.min[0]) / w } else { 0.0 };
        let fy = if h > 0.0 { (centroid(1) - bounds.min[1]) / h } else { 0.0 };
        let ix = ((fx * nx as f64) as usize).min(nx - 1);
        let iy = ((fy * ny as f64) as usize).min(ny - 1);
        iy * nx + ix
    };

    // Counting sort of triangle ids by bin.
    let bins: Vec<usize> = mesh.triangles.iter().map(bin_of).collect();
    let mut starts = vec![0usize; nx * ny + 1];
    for &b in &bins {
        starts[b + 1] += 1;
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }
    let mut cursor = starts.clone();
    let mut order = vec![0u32; mesh.triangles.len()];
    for (t, &b) in bins.iter().enumerate() {
        order[cursor[b]] = t as u32;
        cursor[b] += 1;
    }

    // `stamp[v]` is the chunk that last took vertex v, `local[v]` its index there.
    let mut stamp = vec![usize::MAX; mesh.vertices.len()];
    let mut local = vec![0u16; mesh.vertices.len()];
    let mut chunks = Vec::new();
    let mut current = Chunk { vertices: Vec::new(), indices: Vec::new() };
    for bin in 0..nx * ny {
        for &t in &order[starts[bin]..starts[bin + 1]] {
            let tri = mesh.triangles[t as usize];
            let id = chunks.len();
            let new_vertices = tri.iter().filter(|&&v| stamp[v as usize] != id).count();
            if current.vertices.len() + new_vertices > max_vertices {
                chunks.push(std::mem::replace(&mut current, Chunk { vertices: Vec::new(), indices: Vec::new() }));
            }
            let id = chunks.len();
            for v in tri {
                let v = v as usize;
                if stamp[v] != id {
                    stamp[v] = id;
                    local[v] = current.vertices.len() as u16;
                    current.vertices.push(v as u32);
                }
                current.indices.push(local[v]);
            }
        }
        if !current.vertices.is_empty() {
            chunks.push(std::mem::replace(&mut current, Chunk { vertices: Vec::new(), indices: Vec::new() }));
        }
    }
    chunks
}

/// Triangles `triangulate_grid` makes in the cells of one tile: two per cell with four live
/// corners, one with three.
fn count_cell_triangles(grid: &Grid, col0: usize, row0: usize, cols: usize, rows: usize) -> usize {
    let live = |c: usize, r: usize| !grid.z_at(c, r).is_nan();
    let mut n = 0;
    for row in row0..row0 + rows - 1 {
        for col in col0..col0 + cols - 1 {
            let corners = [live(col, row), live(col + 1, row), live(col + 1, row + 1), live(col, row + 1)];
            n += match corners.iter().filter(|&&l| l).count() {
                4 => 2,
                3 => 1,
                _ => 0,
            };
        }
    }
    n
}

fn local_bounds(b: Aabb, origin: [f64; 3]) -> Value {
    let local = |v: [f64; 3]| [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];
    json!({ "min": local(b.min), "max": local(b.max) })
}

fn pad_to_4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// Appends the header (with counts, origin, bounds and encode time) and its length.
fn finish(mut out: Vec<u8>, mut header: Value, triangles: usize, bounds: Option<Value>, start: Instant) -> Vec<u8> {
    header["triangles"] = triangles.into();
    if let Some(b) = bounds {
        header["bounds_local"] = b;
    }
    if let Some(timings) = header.get_mut("timings").and_then(|t| t.as_object_mut()) {
        timings.insert("encode_ms".into(), (start.elapsed().as_secs_f64() * 1e3).into());
    }
    let header = serde_json::to_vec(&header).expect("header serialises");
    out.extend_from_slice(&header);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zd_geom::triangulate_grid;

    const ORIGIN: [f64; 3] = [650_000.0, 7_450_000.0, 100.0];

    fn header_of(buf: &[u8]) -> (Value, usize) {
        let n = buf.len();
        let len = u32::from_le_bytes(buf[n - 4..].try_into().unwrap()) as usize;
        (serde_json::from_slice(&buf[n - 4 - len..n - 4]).unwrap(), n - 4 - len)
    }

    fn f32_at(buf: &[u8], i: usize) -> f32 {
        f32::from_le_bytes(buf[i * 4..i * 4 + 4].try_into().unwrap())
    }

    fn u16_at(buf: &[u8], byte: usize) -> u16 {
        u16::from_le_bytes(buf[byte..byte + 2].try_into().unwrap())
    }

    fn sample_grid(cols: usize, rows: usize) -> Grid {
        Grid::from_fn([650_003.0, 7_450_007.0], [5.0, 4.0], cols, rows, |x, y| {
            150.0 + 0.01 * (x - 650_000.0) - 0.02 * (y - 7_450_000.0) + ((x + y) / 50.0).sin()
        })
    }

    #[test]
    fn chunks_hold_every_triangle_once_within_the_vertex_limit() {
        let mesh = triangulate_grid(&sample_grid(120, 90));
        // Small limits force many bins and bins that overflow into several chunks.
        let chunks = chunk_triangles(&mesh, 500, 300);
        assert!(chunks.len() > 20);
        let mut seen = Vec::new();
        for c in &chunks {
            assert!(c.vertices.len() <= 500);
            for t in c.indices.as_chunks::<3>().0 {
                seen.push(t.iter().map(|&i| c.vertices[i as usize]).collect::<Vec<_>>());
            }
        }
        let mut expected: Vec<Vec<u32>> = mesh.triangles.iter().map(|t| t.to_vec()).collect();
        seen.sort();
        expected.sort();
        assert_eq!(seen, expected, "same triangles, same winding, each exactly once");
    }

    #[test]
    fn triangulation_buffer_decodes_to_the_mesh() {
        let mesh = triangulate_grid(&sample_grid(300, 260)); // ~155k triangles: several chunks
        let buf = encode_triangulation(&mesh, ORIGIN, json!({ "timings": {} }));
        let (header, _) = header_of(&buf);
        assert_eq!(header["kind"], "triangulation");
        assert_eq!(header["triangles"], mesh.triangle_count());
        let index_base = header["vertices"].as_u64().unwrap() as usize * 12;
        let mut decoded = Vec::new();
        for chunk in header["chunks"].as_array().unwrap() {
            let get = |k: &str| chunk[k].as_u64().unwrap() as usize;
            assert!(get("vertex_count") <= MAX_CHUNK_VERTICES);
            for t in 0..get("index_count") / 3 {
                let corner = |k: usize| {
                    let i = u16_at(&buf, index_base + (get("index_start") + 3 * t + k) * 2) as usize;
                    let v = get("vertex_start") + i;
                    [0, 1, 2].map(|a| f64::from(f32_at(&buf, v * 3 + a)) + ORIGIN[a])
                };
                decoded.push([corner(0), corner(1), corner(2)]);
            }
        }
        let original: Vec<_> = mesh.triangles.iter().map(|t| t.map(|i| mesh.vertices[i as usize])).collect();
        // Match triangles on plan position (whole metres on this lattice, exact in f32), then
        // check every coordinate: f32 offsets within ~1.5 km of the origin hold 0.1 mm easily.
        let (mut decoded, mut original) = (decoded, original);
        let plan = |t: &[[f64; 3]; 3]| {
            let mut k = t.map(|p| [(p[0] * 100.0).round() as i64, (p[1] * 100.0).round() as i64]);
            k.sort();
            k
        };
        decoded.sort_by_key(plan);
        original.sort_by_key(plan);
        assert_eq!(decoded.len(), original.len());
        for (d, o) in decoded.iter().zip(&original) {
            assert_eq!(plan(d), plan(o));
            let mut dc = d.to_vec();
            let mut oc = o.to_vec();
            dc.sort_by(|a, b| a.partial_cmp(b).unwrap());
            oc.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for (a, b) in dc.iter().zip(&oc) {
                for axis in 0..3 {
                    assert!((a[axis] - b[axis]).abs() < 1e-4, "{a:?} vs {b:?}");
                }
            }
        }
        assert!(buf.len() < mesh.triangle_count() * 13, "about 12 bytes per triangle, down from 24");
    }

    /// Rebuilds triangles from a grid buffer the way the viewport does.
    fn decode_grid(buf: &[u8]) -> (Vec<[[f64; 3]; 3]>, f64) {
        let (h, _) = header_of(buf);
        let [ox, oy] = [0, 1].map(|i| h["lattice"]["origin"][i].as_f64().unwrap());
        let [dx, dy] = [0, 1].map(|i| h["lattice"]["cell_size"][i].as_f64().unwrap());
        let (z_min, z_step) = (h["z"]["min"].as_f64().unwrap(), h["z"]["step"].as_f64().unwrap());
        let mut tris = Vec::new();
        for tile in h["tiles"].as_array().unwrap() {
            let get = |k: &str| tile[k].as_u64().unwrap() as usize;
            let (c0, r0, nc, nr, offset) = (get("col"), get("row"), get("cols"), get("rows"), get("offset"));
            let q = |c: usize, r: usize| u16_at(buf, (offset + r * nc + c) * 2);
            let p = |c: usize, r: usize| {
                [ox + (c0 + c) as f64 * dx + ORIGIN[0], oy + (r0 + r) as f64 * dy + ORIGIN[1], z_min + f64::from(q(c, r)) * z_step + ORIGIN[2]]
            };
            for r in 0..nr - 1 {
                for c in 0..nc - 1 {
                    let ring: Vec<(usize, usize)> = [(c, r), (c + 1, r), (c + 1, r + 1), (c, r + 1)]
                        .into_iter()
                        .filter(|&(c, r)| q(c, r) != GRID_NULL)
                        .collect();
                    match ring.len() {
                        4 => {
                            tris.push([p(ring[0].0, ring[0].1), p(ring[1].0, ring[1].1), p(ring[2].0, ring[2].1)]);
                            tris.push([p(ring[0].0, ring[0].1), p(ring[2].0, ring[2].1), p(ring[3].0, ring[3].1)]);
                        }
                        3 => tris.push([p(ring[0].0, ring[0].1), p(ring[1].0, ring[1].1), p(ring[2].0, ring[2].1)]),
                        _ => {}
                    }
                }
            }
        }
        (tris, z_step)
    }

    #[test]
    fn grid_buffer_rebuilds_the_engine_triangulation() {
        // 600 x 300 nodes: three tile columns, two tile rows (partial tiles on the far edges),
        // with a null patch and scattered nulls.
        let mut grid = sample_grid(600, 300);
        for row in 100..140 {
            for col in 250..275 {
                let i = grid.index(col, row);
                grid.z[i] = f64::NAN;
            }
        }
        for i in (7..grid.z.len()).step_by(997) {
            grid.z[i] = f64::NAN;
        }
        let engine = triangulate_grid(&grid);
        let buf = encode_grid(&grid, ORIGIN, json!({ "timings": {} }));
        let (header, _) = header_of(&buf);
        assert_eq!(header["triangles"], engine.triangle_count());
        assert!(buf.len() < grid.z.len() * 2 + 8192, "two bytes per node plus header");

        let (decoded, z_step) = decode_grid(&buf);
        assert_eq!(decoded.len(), engine.triangle_count());
        // Same triangles: match on plan position (exact lattice), then check elevations are
        // within half a z step (the quantisation bound).
        let plan_key = |t: &[[f64; 3]; 3]| {
            let mut k = t.map(|p| [(p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64]);
            k.sort();
            k
        };
        let mut engine_tris: Vec<_> = engine.triangles.iter().map(|t| t.map(|i| engine.vertices[i as usize])).collect();
        let mut decoded = decoded;
        engine_tris.sort_by_key(plan_key);
        decoded.sort_by_key(plan_key);
        let corners_by_plan = |t: &[[f64; 3]; 3]| {
            let mut c: Vec<_> = t.iter().map(|p| ([(p[0] * 1e4).round() as i64, (p[1] * 1e4).round() as i64], p[2])).collect();
            c.sort_by_key(|&(k, _)| k);
            c
        };
        for (e, d) in engine_tris.iter().zip(&decoded) {
            assert_eq!(plan_key(e), plan_key(d));
            for (a, b) in corners_by_plan(e).iter().zip(&corners_by_plan(d)) {
                assert!((a.1 - b.1).abs() <= 0.5 * z_step + 1e-9, "{} vs {}", a.1, b.1);
            }
        }
    }
}
