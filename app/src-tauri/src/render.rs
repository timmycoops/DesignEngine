//! Render buffers for the viewport: the only place engine f64 coordinates become f32.
//!
//! Two compact layouts. Both carry no normals (the viewport shades flat from screen-space
//! derivatives) and end in a UTF-8 JSON header followed by the header's length as a `u32`, so the
//! arrays start 4-byte aligned at offset 0 and the encoder can time itself without a copy.
//!
//! - `triangulation` (any TIN): the mesh is split into spatially compact chunks of at most
//!   65,535 vertices so indices fit in `u16`. Data: `f32` positions (offset from the scene origin)
//!   for every chunk, then `u16` indices for every chunk, zero-padded to 4 bytes.
//!   A layer's solids (`encode_solids`) add which solid each vertex is of: an `f32` per vertex, chunk after chunk,
//!   after the indices, with the solids' ids in the header in that order (`solids`). The viewport hides and lights
//!   up a solid by its number, so neither sends the mesh again (decision 0060, workstream 3).
//! - `grid` (surfaces read from a regular grid): only elevations travel, as `u16` multiples of one
//!   z step over the surface's range (65,535 = null), in tiles of 255 x 255 nodes. The viewport
//!   rebuilds x and y from the lattice and the triangles by the same rule as
//!   `zd_geom::triangulate_grid`. One step is the whole surface's z range / 65,534, so neighbouring
//!   tiles quantise shared nodes identically and no cracks open between them. A grid may carry a
//!   second number per node to colour by (seam thickness): `u16` values in the same tile order,
//!   straight after the elevations, quantised over `[min, max]` with 65,535 for no value and
//!   65,534 for a node flagged with an issue.

use std::time::Instant;

use serde_json::{Value, json};
use std::sync::Arc;

use zd_core::{Aabb, Grid, ObjectId, TriMesh};
use zd_geom::SectionPlane;

use crate::section::SectionGroup;

/// Most vertices a chunk may hold so its indices fit in `u16`.
pub const MAX_CHUNK_VERTICES: usize = 65_535;
/// Chunks are cut from plan bins sized for about this many triangles.
const TARGET_CHUNK_TRIANGLES: usize = 100_000;
/// Nodes along each side of a grid tile (254 cells; 65,025 vertices).
pub const GRID_TILE_NODES: usize = 255;
/// Quantised elevation that marks a null node.
pub const GRID_NULL: u16 = u16::MAX;
/// Quantised value that marks a node flagged with an issue.
pub const VALUE_FLAG: u16 = u16::MAX - 1;

/// A second number per node to colour a grid by. NaN is no value (drawn plain).
pub struct NodeValues<'a> {
    pub name: &'static str,
    pub values: &'a [f64],
    pub flagged: &'a [bool],
    /// The value range the colours span.
    pub range: [f64; 2],
}

/// A section for the viewport: `f32` segment ends offset from the scene origin, group after
/// group, then the JSON header and its length, as the other layouts end.
pub fn encode_section(groups: &[SectionGroup], origin: [f64; 3], plane: &SectionPlane, ms: f64) -> Vec<u8> {
    let total: usize = groups.iter().map(|g| g.segments.len()).sum();
    let filled: usize = groups.iter().map(|g| g.fill.triangles.len()).sum();
    let mut out = Vec::with_capacity(total * 24 + filled * 36 + 256 + groups.len() * 96);
    let (mut along, mut rl) = ([f64::INFINITY, f64::NEG_INFINITY], [f64::INFINITY, f64::NEG_INFINITY]);
    for p in groups.iter().flat_map(|g| g.segments.iter()).flatten() {
        let a = plane.along(*p);
        (along[0], along[1]) = (along[0].min(a), along[1].max(a));
        (rl[0], rl[1]) = (rl[0].min(p[2]), rl[1].max(p[2]));
        for k in 0..3 {
            out.extend_from_slice(&((p[k] - origin[k]) as f32).to_le_bytes());
        }
    }
    for p in groups.iter().flat_map(|g| g.fill.triangles.iter()).flatten() {
        for k in 0..3 {
            out.extend_from_slice(&((p[k] - origin[k]) as f32).to_le_bytes());
        }
    }
    let described: Vec<Value> = groups
        .iter()
        .map(|g| json!({ "layer": g.layer, "kind": g.kind, "object": g.object, "material": g.material, "segments": g.segments.len(), "triangles": g.fill.triangles.len(), "unfilled": g.fill.unfilled }))
        .collect();
    let bounds = (total > 0).then(|| json!({ "along": along, "rl": rl }));
    let header = json!({ "groups": described, "bounds": bounds, "ms": ms }).to_string();
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out
}

pub fn encode_triangulation(mesh: &TriMesh, origin: [f64; 3], header: Value) -> Vec<u8> {
    encode_triangulation_of(mesh, None, origin, header)
}

/// A layer's solids as one triangulation, each vertex saying which solid it's of: the number of the solid in `ids`,
/// whose meshes `meshes` are, in the same order. The walls two of them share (the same triangle in both) are sent as a
/// part of their own after the rest, their chunks marked `shared`: the view draws them only when a solid of the tile is
/// hidden or the layer is seen through, since otherwise they're inside and never seen (decision 0097, workstream 4).
pub fn encode_solids(ids: &[ObjectId], meshes: &[Arc<TriMesh>], origin: [f64; 3], header: Value) -> Vec<u8> {
    let parts = split_shared_walls(meshes);
    let mut header = header;
    header["solids"] = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().into();
    header["shared_triangles"] = parts.walls.triangle_count().into();
    encode_parts(&[Part { mesh: &parts.outer, solid_of: Some(&parts.outer_of), shared: false }, Part { mesh: &parts.walls, solid_of: Some(&parts.walls_of), shared: true }], origin, header)
}

/// A tile's solids split in two: the walls two of them share, and the rest.
pub struct SolidParts {
    /// Every triangle not shared with another solid, each vertex saying which solid it's of in `outer_of`.
    pub outer: TriMesh,
    pub outer_of: Vec<u32>,
    /// Every triangle another solid has too (the same three corners, to the bit), with `walls_of` likewise.
    pub walls: TriMesh,
    pub walls_of: Vec<u32>,
}

/// Splits solids' triangles into the walls two of them share and the rest. A triangle is shared when another solid
/// has one with the same three corners exactly, as two blocks cut from one solid by one polygon do along their side;
/// one only nearly the same is left with the rest, drawn as now. Each triangle is fingerprinted on its own thread's
/// share of the solids, the fingerprints sorted, and only triangles whose fingerprints match are compared corner by
/// corner.
pub fn split_shared_walls(meshes: &[Arc<TriMesh>]) -> SolidParts {
    let corners = |m: &TriMesh, tri: &[u32; 3]| {
        let mut c = tri.map(|v| m.vertices[v as usize].map(f64::to_bits));
        c.sort_unstable();
        c
    };
    let fingerprint = |c: &[[u64; 3]; 3]| c.iter().flatten().fold(0x243F_6A88_85A3_08D3_u64, |h, &x| (h ^ x).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(29));
    // (fingerprint, solid, triangle), every triangle of every solid.
    let threads = worker_threads().min(meshes.len()).max(1);
    let per = meshes.len().div_ceil(threads).max(1);
    let mut all: Vec<(u64, u32, u32)> = std::thread::scope(|scope| {
        let handles: Vec<_> = meshes
            .chunks(per)
            .enumerate()
            .map(|(g, group)| {
                scope.spawn(move || {
                    let mut mine = Vec::with_capacity(group.iter().map(|m| m.triangles.len()).sum());
                    for (j, m) in group.iter().enumerate() {
                        let k = (g * per + j) as u32;
                        for (i, tri) in m.triangles.iter().enumerate() {
                            mine.push((fingerprint(&corners(m, tri)), k, i as u32));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("fingerprinting a solid's triangles doesn't panic")).collect()
    });
    all.sort_unstable();
    let mut shared: Vec<Vec<bool>> = meshes.iter().map(|m| vec![false; m.triangles.len()]).collect();
    let mut run = 0;
    while run < all.len() {
        let mut end = run + 1;
        while end < all.len() && all[end].0 == all[run].0 {
            end += 1;
        }
        if end - run > 1 {
            for a in run..end {
                let (_, ka, ia) = all[a];
                let ca = corners(&meshes[ka as usize], &meshes[ka as usize].triangles[ia as usize]);
                if (run..end).any(|b| all[b].1 != ka && corners(&meshes[all[b].1 as usize], &meshes[all[b].1 as usize].triangles[all[b].2 as usize]) == ca) {
                    shared[ka as usize][ia as usize] = true;
                }
            }
        }
        run = end;
    }
    let mut parts = SolidParts { outer: TriMesh::default(), outer_of: Vec::new(), walls: TriMesh::default(), walls_of: Vec::new() };
    for (k, m) in meshes.iter().enumerate() {
        let (mut in_outer, mut in_walls) = (vec![u32::MAX; m.vertices.len()], vec![u32::MAX; m.vertices.len()]);
        for (i, tri) in m.triangles.iter().enumerate() {
            let (mesh, of, map) = if shared[k][i] { (&mut parts.walls, &mut parts.walls_of, &mut in_walls) } else { (&mut parts.outer, &mut parts.outer_of, &mut in_outer) };
            let made = tri.map(|v| {
                if map[v as usize] == u32::MAX {
                    map[v as usize] = mesh.vertices.len() as u32;
                    mesh.vertices.push(m.vertices[v as usize]);
                    of.push(k as u32);
                }
                map[v as usize]
            });
            mesh.triangles.push(made);
        }
    }
    parts
}

/// A part of a triangulation to encode: its mesh, which solid each vertex is of, and whether it's the walls solids share.
struct Part<'a> {
    mesh: &'a TriMesh,
    solid_of: Option<&'a [u32]>,
    shared: bool,
}

fn encode_triangulation_of(mesh: &TriMesh, solid_of: Option<&[u32]>, origin: [f64; 3], header: Value) -> Vec<u8> {
    encode_parts(&[Part { mesh, solid_of, shared: false }], origin, header)
}

/// The parts' chunks, part after part: chunk k of what's sent is chunk k here.
fn chunks_of_parts<'a>(parts: &[Part<'a>], threads: usize) -> Vec<(&'a TriMesh, Chunk, usize)> {
    parts
        .iter()
        .enumerate()
        .flat_map(|(p, part)| chunk_triangles_on(part.mesh, MAX_CHUNK_VERTICES, TARGET_CHUNK_TRIANGLES, threads).into_iter().map(move |c| (part.mesh, c, p)))
        .collect()
}

fn encode_parts(parts: &[Part], origin: [f64; 3], header: Value) -> Vec<u8> {
    let start = Instant::now();
    let threads = worker_threads();
    let chunks = chunks_of_parts(parts, threads);

    let vertex_total: usize = chunks.iter().map(|c| c.1.vertices.len()).sum();
    let index_total: usize = chunks.iter().map(|c| c.1.indices.len()).sum();
    let with_solids = parts.iter().any(|p| p.solid_of.is_some());
    // One zeroed allocation with room for the header, so appending it never reallocates.
    let data_len = vertex_total * 12 + index_total * 2;
    let solids_len = if with_solids { vertex_total * 4 + 4 } else { 0 };
    let mut out = vec![0u8; data_len + solids_len + 4096 + 320 * chunks.len()];
    out.truncate(data_len);

    // Each chunk owns a disjoint slice of the position bytes and of the index bytes, so threads
    // can fill them without coordination.
    let (mut positions, mut indices) = out.split_at_mut(vertex_total * 12);
    let mut jobs = Vec::with_capacity(chunks.len());
    for (mesh, chunk, _) in &chunks {
        let (p, rest) = positions.split_at_mut(chunk.vertices.len() * 12);
        let (i, rest_i) = indices.split_at_mut(chunk.indices.len() * 2);
        positions = rest;
        indices = rest_i;
        jobs.push(WriteJob { mesh, chunk, positions: p, indices: i, bounds: None });
    }
    let per_thread = jobs.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for group in jobs.chunks_mut(per_thread) {
            scope.spawn(move || group.iter_mut().for_each(|job| job.write(origin)));
        }
    });

    let mut meta = Vec::with_capacity(jobs.len());
    let (mut vertex_start, mut index_start) = (0, 0);
    for (job, (_, _, p)) in jobs.iter().zip(&chunks) {
        let bounds = job.bounds.expect("chunks are never empty");
        let mut m = json!({
            "vertex_start": vertex_start,
            "vertex_count": job.chunk.vertices.len(),
            "index_start": index_start,
            "index_count": job.chunk.indices.len(),
            "min": bounds.min,
            "max": bounds.max,
        });
        if parts[*p].shared {
            m["shared"] = true.into();
        }
        meta.push(m);
        vertex_start += job.chunk.vertices.len();
        index_start += job.chunk.indices.len();
    }
    drop(jobs);
    pad_to_4(&mut out);
    if with_solids {
        for (_, chunk, p) in &chunks {
            let solid_of = parts[*p].solid_of.expect("every part of a layer's solids says which solid");
            for &v in &chunk.vertices {
                out.extend_from_slice(&(solid_of[v as usize] as f32).to_le_bytes());
            }
        }
    }

    let mut header = header;
    header["kind"] = "triangulation".into();
    header["vertices"] = vertex_total.into();
    header["chunks"] = meta.into();
    let triangles = parts.iter().map(|p| p.mesh.triangle_count()).sum();
    let bounds = parts.iter().filter_map(|p| p.mesh.bounds()).reduce(|a, b| Aabb { min: std::array::from_fn(|k| a.min[k].min(b.min[k])), max: std::array::from_fn(|k| a.max[k].max(b.max[k])) });
    finish(out, header, triangles, bounds.map(|b| local_bounds(b, origin)), start)
}

pub fn encode_grid(grid: &Grid, origin: [f64; 3], header: Value) -> Vec<u8> {
    encode_grid_with(grid, None, origin, header)
}

/// A grid with a second number per node (see the module notes).
pub fn encode_grid_with(grid: &Grid, values: Option<&NodeValues<'_>>, origin: [f64; 3], header: Value) -> Vec<u8> {
    let start = Instant::now();
    let value_step = values.map(|v| if v.range[1] > v.range[0] { (v.range[1] - v.range[0]) / f64::from(VALUE_FLAG - 1) } else { 1.0 });
    let value_bytes = |i: usize| -> [u8; 2] {
        let (Some(v), Some(step)) = (values, value_step) else { return [0; 2] };
        let q = if v.flagged[i] {
            VALUE_FLAG
        } else if v.values[i].is_nan() {
            GRID_NULL
        } else {
            ((v.values[i] - v.range[0]) / step).round().clamp(0.0, f64::from(VALUE_FLAG - 1)) as u16
        };
        q.to_le_bytes()
    };
    let mut value_out = Vec::with_capacity(if values.is_some() { grid.z.len() * 2 } else { 0 });
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
                    if values.is_some() {
                        value_out.extend_from_slice(&value_bytes(grid.index(col, row)));
                    }
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
    out.extend_from_slice(&value_out);
    pad_to_4(&mut out);

    let mut header = header;
    if let (Some(v), Some(step)) = (values, value_step) {
        header["value"] = json!({ "name": v.name, "min": v.range[0], "max": v.range[1], "step": step, "null": GRID_NULL, "flag": VALUE_FLAG });
    }
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

/// The coarser versions of each chunk `encode_triangulation` makes of `mesh` (decision 0097, workstream 2), fetched by
/// the view after it has drawn full detail, so it never waits on them. The chunks are cut again, the same way, so chunk
/// k here is chunk k there and each version's `u16` indices point at that chunk's vertices as sent. Data: every
/// version's indices, chunk after chunk, padded to 4 bytes; the header lists each chunk's versions with where their
/// indices start, how many, and the error, the farthest the version lies from a vertex of the full chunk (m).
pub fn encode_levels(mesh: &TriMesh, origin: [f64; 3], header: Value) -> Vec<u8> {
    let start = Instant::now();
    let chunks = chunks_of_parts(&[Part { mesh, solid_of: None, shared: false }], worker_threads());
    let jobs: Vec<LevelJob> = chunks.iter().map(|(m, c, _)| LevelJob { mesh: m, chunk: c, locked: None }).collect();
    levels_buffer(&jobs, mesh.triangle_count(), origin, header, start)
}

/// The coarser versions of the chunks `encode_solids` makes of a tile's solids (decision 0097, workstream 4): chunk k
/// here is chunk k there. Each block's edges stay as they are, so blocks still meet and a picked block's outline is
/// exact: every corner of an edge where the surface turns by more than 60° is locked (where a top meets a wall, a wall
/// a floor), and tops, floors, walls and faces thin between them. The shared walls' chunks get no versions.
pub fn encode_solid_levels(meshes: &[Arc<TriMesh>], origin: [f64; 3], header: Value) -> Vec<u8> {
    let start = Instant::now();
    let parts = split_shared_walls(meshes);
    let locked = creased_corners(&parts.outer);
    let chunks = chunks_of_parts(&[Part { mesh: &parts.outer, solid_of: None, shared: false }, Part { mesh: &parts.walls, solid_of: None, shared: true }], worker_threads());
    let jobs: Vec<LevelJob> = chunks.iter().map(|(m, c, p)| LevelJob { mesh: m, chunk: c, locked: (*p == 0).then_some(locked.as_slice()) }).collect();
    let mut buf_header = header;
    buf_header["walls_without_versions"] = chunks.iter().filter(|c| c.2 == 1).count().into();
    levels_buffer(&jobs, parts.outer.triangle_count() + parts.walls.triangle_count(), origin, buf_header, start)
}

/// The corners of every edge of `mesh` where it turns by more than 60°: a block's edges, which its versions keep where
/// they are. Edges are found by sorting each triangle's three, not by a map, for speed on a tile of millions.
pub fn creased_corners(mesh: &TriMesh) -> Vec<bool> {
    let normal = |tri: &[u32; 3]| {
        let [a, b, c] = tri.map(|v| mesh.vertices[v as usize]);
        let (u, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
    };
    let mut edges: Vec<(u32, u32, u32)> = Vec::with_capacity(3 * mesh.triangles.len());
    for (t, tri) in mesh.triangles.iter().enumerate() {
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            edges.push((a.min(b), a.max(b), t as u32));
        }
    }
    edges.sort_unstable();
    let mut locked = vec![false; mesh.vertices.len()];
    for pair in edges.windows(2) {
        let ((a, b, s), (c, d, u)) = (pair[0], pair[1]);
        if (a, b) != (c, d) {
            continue;
        }
        let (n, m) = (normal(&mesh.triangles[s as usize]), normal(&mesh.triangles[u as usize]));
        let dot = n[0] * m[0] + n[1] * m[1] + n[2] * m[2];
        let lengths = ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]) * (m[0] * m[0] + m[1] * m[1] + m[2] * m[2])).sqrt();
        if dot < 0.5 * lengths {
            locked[a as usize] = true;
            locked[b as usize] = true;
        }
    }
    locked
}

/// One chunk to make versions of: none when `locked` is `None` for a part that gets none, as the shared walls.
struct LevelJob<'a> {
    mesh: &'a TriMesh,
    chunk: &'a Chunk,
    /// Vertices of the whole part that mustn't move, by its vertex numbers; a surface's chunks lock only their border.
    locked: Option<&'a [bool]>,
}

fn levels_buffer(jobs: &[LevelJob], triangles: usize, origin: [f64; 3], header: Value, start: Instant) -> Vec<u8> {
    let threads = worker_threads();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let surface = jobs.iter().all(|j| j.locked.is_none());
    let mut made: Vec<(usize, zd_geom::DisplayLevels)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads.min(jobs.len()).max(1))
            .map(|_| {
                let next = &next;
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(job) = jobs.get(k) else { return mine };
                        if !surface && job.locked.is_none() {
                            mine.push((k, zd_geom::DisplayLevels { levels: Vec::new(), ms: 0.0 }));
                            continue;
                        }
                        let (mesh, chunk) = (job.mesh, job.chunk);
                        let positions: Vec<[f32; 3]> = chunk.vertices.iter().map(|&v| std::array::from_fn(|a| (mesh.vertices[v as usize][a] - origin[a]) as f32)).collect();
                        let indices: Vec<u32> = chunk.indices.iter().map(|&i| i as u32).collect();
                        let locks: Option<Vec<bool>> = job.locked.map(|l| chunk.vertices.iter().map(|&v| l[v as usize]).collect());
                        mine.push((k, zd_geom::display_levels(&positions, &indices, locks.as_deref())));
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("making a chunk's versions doesn't panic")).collect()
    });
    made.sort_by_key(|(k, _)| *k);

    let level_indices: usize = made.iter().flat_map(|(_, m)| &m.levels).map(|l| l.indices.len()).sum();
    let mut out = Vec::with_capacity(level_indices * 2 + 4096 + 160 * jobs.len());
    let mut meta = Vec::with_capacity(made.len());
    let mut index_start = 0;
    let (mut coarsest, mut cpu_ms) = (0usize, 0.0);
    for ((_, m), job) in made.iter().zip(jobs) {
        let mut levels = Vec::with_capacity(m.levels.len());
        for level in &m.levels {
            for &i in &level.indices {
                out.extend_from_slice(&(i as u16).to_le_bytes());
            }
            levels.push(json!({ "index_start": index_start, "index_count": level.indices.len(), "error": level.error }));
            index_start += level.indices.len();
        }
        coarsest += m.levels.last().map_or(job.chunk.indices.len() / 3, |l| l.indices.len() / 3);
        cpu_ms += m.ms;
        meta.push(json!({ "levels": levels }));
    }
    pad_to_4(&mut out);
    let mut header = header;
    header["kind"] = "levels".into();
    header["chunks"] = meta.into();
    header["level_triangles"] = (level_indices / 3).into();
    header["coarsest_triangles"] = coarsest.into();
    if let Some(timings) = header.get_mut("timings").and_then(|t| t.as_object_mut()) {
        timings.insert("levels_cpu_ms".into(), cpu_ms.into());
    }
    finish(out, header, triangles, None, start)
}

/// A chunk of a triangulation: global vertex ids and `u16` indices into them.
#[derive(Debug, Default, PartialEq)]
pub struct Chunk {
    pub vertices: Vec<u32>,
    pub indices: Vec<u16>,
}

/// One chunk's share of the output buffer, filled on a worker thread.
struct WriteJob<'a> {
    mesh: &'a TriMesh,
    chunk: &'a Chunk,
    positions: &'a mut [u8],
    indices: &'a mut [u8],
    bounds: Option<Aabb>,
}

impl WriteJob<'_> {
    fn write(&mut self, origin: [f64; 3]) {
        let mesh = self.mesh;
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let (positions, _) = self.positions.as_chunks_mut::<12>();
        for (&v, out) in self.chunk.vertices.iter().zip(positions) {
            let p = mesh.vertices[v as usize];
            for (axis, bytes) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let c = p[axis] - origin[axis];
                lo[axis] = lo[axis].min(c);
                hi[axis] = hi[axis].max(c);
                *bytes = (c as f32).to_le_bytes();
            }
        }
        let (indices, _) = self.indices.as_chunks_mut::<2>();
        for (&i, out) in self.chunk.indices.iter().zip(indices) {
            *out = i.to_le_bytes();
        }
        self.bounds = (!self.chunk.vertices.is_empty()).then_some(Aabb { min: lo, max: hi });
    }
}

/// Threads for encoding: all available cores.
fn worker_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Splits a triangulation into chunks of at most `max_vertices` vertices. Triangles are binned in
/// plan by centroid (about `target_triangles` per bin) so each chunk covers a compact area; a bin
/// that needs more vertices than allowed is split into several chunks. Every triangle lands in
/// exactly one chunk; vertices on chunk seams are repeated. Uses every core; the result does not
/// depend on the thread count.
#[cfg(test)]
pub fn chunk_triangles(mesh: &TriMesh, max_vertices: usize, target_triangles: usize) -> Vec<Chunk> {
    chunk_triangles_on(mesh, max_vertices, target_triangles, worker_threads())
}

fn chunk_triangles_on(mesh: &TriMesh, max_vertices: usize, target_triangles: usize, threads: usize) -> Vec<Chunk> {
    assert!(max_vertices <= MAX_CHUNK_VERTICES, "chunk indices are u16");
    let Some(layout) = BinLayout::new(mesh, target_triangles) else { return Vec::new() };
    let threads = threads.max(1);

    // Bin of every triangle, in parallel over contiguous ranges.
    let mut bins = vec![0u32; mesh.triangles.len()];
    let per_thread = mesh.triangles.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (tris, out) in mesh.triangles.chunks(per_thread).zip(bins.chunks_mut(per_thread)) {
            let layout = &layout;
            scope.spawn(move || {
                for (tri, bin) in tris.iter().zip(out) {
                    *bin = layout.bin_of(mesh, tri);
                }
            });
        }
    });

    // Counting sort of triangle ids by bin (one linear pass; cheap next to the chunking).
    let bin_count = layout.count();
    let mut starts = vec![0usize; bin_count + 1];
    for &b in &bins {
        starts[b as usize + 1] += 1;
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }
    let mut cursor = starts.clone();
    let mut order = vec![0u32; mesh.triangles.len()];
    for (t, &b) in bins.iter().enumerate() {
        order[cursor[b as usize]] = t as u32;
        cursor[b as usize] += 1;
    }

    // Workers take bins from a shared counter; results go back in bin order, so the output is the
    // same whatever the thread count.
    let next_bin = std::sync::atomic::AtomicUsize::new(0);
    let mut per_bin: Vec<Vec<Chunk>> = (0..bin_count).map(|_| Vec::new()).collect();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(bin_count))
            .map(|_| {
                let (order, starts, next_bin) = (&order, &starts, &next_bin);
                scope.spawn(move || {
                    let mut local = LocalIndex::new();
                    let mut done = Vec::new();
                    loop {
                        let bin = next_bin.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if bin >= bin_count {
                            return done;
                        }
                        let tris = &order[starts[bin]..starts[bin + 1]];
                        done.push((bin, chunk_bin(mesh, tris, max_vertices, &mut local)));
                    }
                })
            })
            .collect();
        for worker in workers {
            for (bin, chunks) in worker.join().expect("chunking thread") {
                per_bin[bin] = chunks;
            }
        }
    });
    per_bin.into_iter().flatten().collect()
}

/// Plan bins over a mesh's bounding box, about `target_triangles` per bin, shaped to its aspect.
struct BinLayout {
    min: [f64; 2],
    size: [f64; 2],
    nx: usize,
    ny: usize,
}

impl BinLayout {
    fn new(mesh: &TriMesh, target_triangles: usize) -> Option<Self> {
        let bounds = mesh.bounds()?;
        let [w, h, _] = bounds.size();
        let bins_wanted = mesh.triangles.len().div_ceil(target_triangles).max(1) as f64;
        let aspect = if h > 0.0 { (w / h).max(1e-6) } else { 1.0 };
        let nx = (bins_wanted * aspect).sqrt().ceil().max(1.0) as usize;
        let ny = (bins_wanted / nx as f64).ceil().max(1.0) as usize;
        Some(BinLayout { min: [bounds.min[0], bounds.min[1]], size: [w, h], nx, ny })
    }

    fn count(&self) -> usize {
        self.nx * self.ny
    }

    fn bin_of(&self, mesh: &TriMesh, &[a, b, c]: &[u32; 3]) -> u32 {
        let centroid = |axis: usize| {
            (mesh.vertices[a as usize][axis] + mesh.vertices[b as usize][axis] + mesh.vertices[c as usize][axis]) / 3.0
        };
        let cell = |axis: usize, n: usize| {
            let f = if self.size[axis] > 0.0 { (centroid(axis) - self.min[axis]) / self.size[axis] } else { 0.0 };
            ((f * n as f64) as usize).min(n - 1)
        };
        (cell(1, self.ny) * self.nx + cell(0, self.nx)) as u32
    }
}

/// Chunks for one bin's triangles, in order, starting a new chunk whenever the next triangle would
/// take the current one past `max_vertices`.
fn chunk_bin(mesh: &TriMesh, tris: &[u32], max_vertices: usize, local: &mut LocalIndex) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current = Chunk::default();
    local.clear();
    for &t in tris {
        let tri = mesh.triangles[t as usize];
        let new_vertices = tri.iter().filter(|&&v| local.get(v).is_none()).count();
        if current.vertices.len() + new_vertices > max_vertices {
            chunks.push(std::mem::take(&mut current));
            local.clear();
        }
        for v in tri {
            let i = match local.get(v) {
                Some(i) => i,
                None => {
                    let i = current.vertices.len() as u16;
                    local.insert(v, i);
                    current.vertices.push(v);
                    i
                }
            };
            current.indices.push(i);
        }
    }
    if !current.vertices.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Global vertex id → index in the chunk being built: open addressing over 2^17 slots, never more
/// than half full because a chunk holds at most 65,535 vertices. Clearing touches only used slots.
struct LocalIndex {
    keys: Vec<u32>,
    values: Vec<u16>,
    used: Vec<usize>,
}

impl LocalIndex {
    const BITS: u32 = 17;
    const EMPTY: u32 = u32::MAX;

    fn new() -> Self {
        let slots = 1 << Self::BITS;
        LocalIndex { keys: vec![Self::EMPTY; slots], values: vec![0; slots], used: Vec::with_capacity(MAX_CHUNK_VERTICES) }
    }

    fn slot(&self, key: u32) -> usize {
        let mask = (1 << Self::BITS) - 1;
        let mut s = (key.wrapping_mul(0x9E37_79B1) >> (32 - Self::BITS)) as usize;
        while self.keys[s] != Self::EMPTY && self.keys[s] != key {
            s = (s + 1) & mask;
        }
        s
    }

    fn get(&self, key: u32) -> Option<u16> {
        let s = self.slot(key);
        (self.keys[s] == key).then(|| self.values[s])
    }

    fn insert(&mut self, key: u32, value: u16) {
        let s = self.slot(key);
        if self.keys[s] == Self::EMPTY {
            self.used.push(s);
        }
        self.keys[s] = key;
        self.values[s] = value;
    }

    fn clear(&mut self) {
        for &s in &self.used {
            self.keys[s] = Self::EMPTY;
        }
        self.used.clear();
    }
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
    fn chunking_is_the_same_on_one_thread_and_many() {
        let mesh = triangulate_grid(&sample_grid(300, 260));
        // Small limits: many bins, some split, so thread scheduling has room to differ.
        let one = chunk_triangles_on(&mesh, 2000, 5000, 1);
        let many = chunk_triangles_on(&mesh, 2000, 5000, 16);
        assert!(one.len() > 20);
        assert_eq!(one, many);
    }

    #[test]
    fn encoded_buffer_is_the_same_on_one_thread_and_many() {
        // The write phase splits the output by chunk; check it against a serial reference.
        let mesh = triangulate_grid(&sample_grid(400, 300));
        let parallel = encode_triangulation(&mesh, ORIGIN, json!({}));
        let chunks = chunk_triangles_on(&mesh, MAX_CHUNK_VERTICES, TARGET_CHUNK_TRIANGLES, 1);
        let mut serial = Vec::new();
        for c in &chunks {
            for &v in &c.vertices {
                for (c, o) in mesh.vertices[v as usize].iter().zip(ORIGIN) {
                    serial.extend_from_slice(&((c - o) as f32).to_le_bytes());
                }
            }
        }
        for c in &chunks {
            for i in &c.indices {
                serial.extend_from_slice(&i.to_le_bytes());
            }
        }
        assert_eq!(&parallel[..serial.len()], &serial[..]);
    }

    /// Encode timings for the synthetic topography as a triangulation. Opt-in:
    /// `cargo test --release -p zd-app -- --ignored --nocapture encode_timing`
    #[test]
    #[ignore]
    fn encode_timing() {
        for cells in [1000, 2000, 3000] {
            let mesh = triangulate_grid(&crate::synthetic::synthetic_topography(cells));
            let mut best = f64::INFINITY;
            for _ in 0..3 {
                let start = Instant::now();
                let buf = encode_triangulation(&mesh, ORIGIN, json!({ "timings": {} }));
                best = best.min(start.elapsed().as_secs_f64() * 1e3);
                std::hint::black_box(buf);
            }
            println!("encode_triangulation {:>10} triangles: best of 3 {best:.0} ms", mesh.triangle_count());
        }
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

    #[test]
    fn values_follow_the_elevations_tile_by_tile() {
        // 3 x 2 nodes, one tile. Thickness 0..4 m over the range [0, 4]; node 4 has no value and node 5
        // is flagged.
        let grid = Grid::from_fn([650_000.0, 7_450_000.0], [10.0, 10.0], 3, 2, |x, _| 100.0 + (x - 650_000.0) / 10.0);
        let values = [0.0, 1.0, 2.0, 4.0, f64::NAN, 3.0];
        let flagged = [false, false, false, false, false, true];
        let v = NodeValues { name: "thickness", values: &values, flagged: &flagged, range: [0.0, 4.0] };
        let buf = encode_grid_with(&grid, Some(&v), ORIGIN, json!({ "timings": {} }));
        let (header, _) = header_of(&buf);
        assert_eq!(header["vertices"], 6);
        let step = header["value"]["step"].as_f64().unwrap();
        assert_eq!((header["value"]["max"].as_f64(), header["value"]["flag"].as_u64()), (Some(4.0), Some(u64::from(VALUE_FLAG))));
        let value = |i: usize| u16_at(&buf, 12 + 2 * i);
        assert_eq!((value(0), value(4), value(5)), (0, GRID_NULL, VALUE_FLAG));
        for (i, expected) in [(1, 1.0), (2, 2.0), (3, 4.0)] {
            assert!((f64::from(value(i)) * step - expected).abs() <= 0.5 * step, "node {i}");
        }
        // Without values the buffer is what it always was.
        let plain = encode_grid(&grid, ORIGIN, json!({ "timings": {} }));
        assert!(header_of(&plain).0.get("value").is_none());
        assert_eq!(plain[..12], buf[..12], "elevations unchanged");
    }

    /// Each vertex of a layer's solids names its solid, so the viewport can hide or light up one by its number.
    #[test]
    fn each_vertex_of_a_layers_solids_says_which_solid_it_is_of() {
        let cube = |x: f64| {
            let v = (0..8).map(|i| [ORIGIN[0] + x + (i & 1) as f64 * 10.0, ORIGIN[1] + (i >> 1 & 1) as f64 * 10.0, ORIGIN[2] + (i >> 2) as f64 * 10.0]).collect();
            let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
            Arc::new(TriMesh::new(v, t))
        };
        let ids = [ObjectId::new(), ObjectId::new(), ObjectId::new()];
        let meshes = [cube(0.0), cube(100.0), cube(200.0)];
        let buf = encode_solids(&ids, &meshes, ORIGIN, json!({ "name": "blocks" }));
        let (header, _) = header_of(&buf);
        let solids: Vec<String> = serde_json::from_value(header["solids"].clone()).unwrap();
        assert_eq!(solids, ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(), "the solids in the order they're numbered");
        let vertices = header["vertices"].as_u64().unwrap() as usize;
        let indices: usize = header["chunks"].as_array().unwrap().iter().map(|c| c["index_count"].as_u64().unwrap() as usize).sum();
        assert_eq!(header["triangles"], 36);
        let after = (vertices * 12 + indices * 2).next_multiple_of(4);
        for v in 0..vertices {
            let x = f32_at(&buf, 3 * v) as f64;
            let solid = f32::from_le_bytes(buf[after + 4 * v..after + 4 * v + 4].try_into().unwrap());
            // Cube k is from x = 100 k to 100 k + 10, scene-local.
            assert_eq!(solid, (x / 100.0).floor() as f32, "vertex {v} at x {x}");
        }
    }
    /// A surface of four chunks: its versions buffer has a list for each chunk the drawn buffer has, every index points
    /// at a vertex of that chunk, the errors grow, and every edge two chunks share is in every version of each, so they
    /// meet with no crack at any versions.
    #[test]
    fn each_chunks_versions_point_at_its_vertices_and_keep_its_seams() {
        let mesh = triangulate_grid(&sample_grid(450, 450));
        let drawn = encode_triangulation(&mesh, ORIGIN, json!({ "name": "topo", "timings": {} }));
        let (drawn_header, _) = header_of(&drawn);
        let chunks = drawn_header["chunks"].as_array().unwrap();
        assert!(chunks.len() >= 4, "{} chunks", chunks.len());
        let buf = encode_levels(&mesh, ORIGIN, json!({ "name": "topo", "timings": {} }));
        let (header, data_end) = header_of(&buf);
        assert_eq!(header["kind"], "levels");
        let lists = header["chunks"].as_array().unwrap();
        assert_eq!(lists.len(), chunks.len());
        let u16_at = |i: usize| u16::from_le_bytes(buf[2 * i..2 * i + 2].try_into().unwrap());
        let vertices = drawn_header["vertices"].as_u64().unwrap() as usize;
        let drawn_u16 = |i: usize| {
            let at = vertices * 12 + 2 * i;
            u16::from_le_bytes(drawn[at..at + 2].try_into().unwrap())
        };
        // Each chunk's vertex ids in the whole mesh, to find the edges two chunks share.
        let all = chunk_triangles(&mesh, MAX_CHUNK_VERTICES, TARGET_CHUNK_TRIANGLES);
        let edges = |k: usize, tri: &mut dyn Iterator<Item = [u16; 3]>| -> std::collections::HashSet<(u32, u32)> {
            let ids = &all[k].vertices;
            tri.flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).map(|(a, b)| (ids[a as usize].min(ids[b as usize]), ids[a as usize].max(ids[b as usize]))).collect()
        };
        let full: Vec<std::collections::HashSet<(u32, u32)>> = chunks
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let (s, n) = (c["index_start"].as_u64().unwrap() as usize, c["index_count"].as_u64().unwrap() as usize);
                edges(k, &mut (s..s + n).step_by(3).map(|i| [drawn_u16(i), drawn_u16(i + 1), drawn_u16(i + 2)]))
            })
            .collect();
        let mut versions = 0;
        for (k, list) in lists.iter().enumerate() {
            let count = chunks[k]["vertex_count"].as_u64().unwrap() as u16;
            let shared: std::collections::HashSet<(u32, u32)> = (0..full.len()).filter(|&j| j != k).flat_map(|j| full[k].intersection(&full[j]).copied().collect::<Vec<_>>()).collect();
            let mut error = 0.0;
            for level in list["levels"].as_array().unwrap() {
                let (s, n) = (level["index_start"].as_u64().unwrap() as usize, level["index_count"].as_u64().unwrap() as usize);
                assert!(2 * (s + n) <= data_end);
                assert!((s..s + n).all(|i| u16_at(i) < count), "chunk {k} points past its vertices");
                let e = level["error"].as_f64().unwrap();
                assert!(e >= error - 1e-9, "chunk {k}: {e} after {error}");
                error = e;
                let kept = edges(k, &mut (s..s + n).step_by(3).map(|i| [u16_at(i), u16_at(i + 1), u16_at(i + 2)]));
                assert!(shared.is_subset(&kept), "chunk {k} lost {} edges it shares", shared.difference(&kept).count());
                versions += 1;
            }
        }
        assert!(versions >= 3 * lists.len(), "{versions} versions over {} chunks", lists.len());
    }
    /// Timing on the made-up large mine (`testdata/synthetic/large_mine.py --lidar 1`), named by `ZD_LARGE_MINE`: the
    /// lidar's full chunks and then its versions, as the shell makes them. Counts and timings only.
    #[test]
    #[ignore]
    fn timing_levels_on_the_large_mine() {
        let Ok(path) = std::env::var("ZD_LARGE_MINE") else { return };
        let t = Instant::now();
        let session = zd_commands::Session::open(&path).unwrap();
        println!("opened in {:.1} s", t.elapsed().as_secs_f64());
        let project = session.project();
        let (name, mesh) = project
            .objects()
            .filter_map(|o| match &o.body {
                zd_core::Body::Surface(s) => Some((o.name.clone(), s.mesh().clone())),
                _ => None,
            })
            .max_by_key(|(_, m)| m.triangle_count())
            .unwrap();
        let origin = mesh.bounds().unwrap().centre().map(f64::round);
        let t = Instant::now();
        let drawn = encode_triangulation(&mesh, origin, json!({ "name": name, "timings": {} }));
        let drawn_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let buf = encode_levels(&mesh, origin, json!({ "name": name, "timings": {} }));
        let levels_ms = t.elapsed().as_secs_f64() * 1e3;
        let (header, _) = header_of(&buf);
        let (drawn_header, _) = header_of(&drawn);
        let chunks = header["chunks"].as_array().unwrap();
        let versions: usize = chunks.iter().map(|c| c["levels"].as_array().unwrap().len()).sum();
        let worst = chunks.iter().filter_map(|c| c["levels"].as_array().unwrap().last()).map(|l| l["error"].as_f64().unwrap()).fold(0.0, f64::max);
        println!(
            "{} triangles in {} chunks: drawn buffer {:.0} MB in {drawn_ms:.0} ms; versions {:.0} MB ({} versions, {} triangles, coarsest {}, worst coarsest error {worst:.2} m) in {levels_ms:.0} ms, {:.0} ms of CPU",
            mesh.triangle_count(),
            chunks.len(),
            drawn.len() as f64 / 1e6,
            buf.len() as f64 / 1e6,
            versions,
            header["level_triangles"],
            header["coarsest_triangles"],
            header["timings"]["levels_cpu_ms"].as_f64().unwrap()
        );
        assert_eq!(chunks.len(), drawn_header["chunks"].as_array().unwrap().len());
    }
    /// Two cubes side by side sharing a face: the face's four triangles, two from each cube, go in the shared part, marked
    /// on their chunks; the other twenty stay with the rest.
    #[test]
    fn the_walls_two_solids_share_are_sent_apart() {
        let cube = |x: f64| {
            let v = (0..8).map(|i| [ORIGIN[0] + x + (i & 1) as f64 * 10.0, ORIGIN[1] + (i >> 1 & 1) as f64 * 10.0, ORIGIN[2] + (i >> 2) as f64 * 10.0]).collect();
            let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
            Arc::new(TriMesh::new(v, t))
        };
        let meshes = [cube(0.0), cube(10.0), cube(100.0)];
        let parts = split_shared_walls(&meshes);
        assert_eq!((parts.outer.triangle_count(), parts.walls.triangle_count()), (32, 4));
        let mut of: Vec<u32> = parts.walls_of.clone();
        of.dedup();
        assert_eq!(of, vec![0, 1], "two walls' corners from each of the two cubes that touch");
        assert!(parts.walls.vertices.iter().all(|p| p[0] == ORIGIN[0] + 10.0), "every shared corner on the face between them");
        let ids = [ObjectId::new(), ObjectId::new(), ObjectId::new()];
        let buf = encode_solids(&ids, &meshes, ORIGIN, json!({ "name": "blocks" }));
        let (header, _) = header_of(&buf);
        let chunks = header["chunks"].as_array().unwrap();
        let count = |shared: bool| chunks.iter().filter(|c| c["shared"].as_bool().unwrap_or(false) == shared).map(|c| c["index_count"].as_u64().unwrap() / 3).sum::<u64>();
        assert_eq!((count(false), count(true)), (32, 4));
        assert_eq!((header["triangles"].as_u64(), header["shared_triangles"].as_u64()), (Some(36), Some(4)));
        assert!(chunks.last().unwrap()["shared"].as_bool().unwrap(), "the shared part comes after the rest");
    }

    /// Two blocks side by side, each a rolling top over a flat floor with walls between: their versions thin the tops,
    /// keep every corner of every wall in every version, and give the shared wall's chunk none.
    #[test]
    fn block_versions_thin_the_tops_and_keep_the_walls() {
        let n = 61;
        let top = |x: f64, y: f64| 120.0 + 3.0 * (x / 17.0).sin() * (y / 13.0).cos();
        let block = |x0: f64| {
            let at = |c: usize, r: usize| [ORIGIN[0] + x0 + c as f64, ORIGIN[1] + r as f64];
            let mut v: Vec<[f64; 3]> = Vec::new();
            for floor in [false, true] {
                for r in 0..n {
                    for c in 0..n {
                        let [x, y] = at(c, r);
                        v.push([x, y, if floor { 100.0 } else { top(x - ORIGIN[0], y - ORIGIN[1]) }]);
                    }
                }
            }
            let id = |c: usize, r: usize, floor: bool| (r * n + c + if floor { n * n } else { 0 }) as u32;
            let mut tris = Vec::new();
            for floor in [false, true] {
                for r in 0..n - 1 {
                    for c in 0..n - 1 {
                        tris.push([id(c, r, floor), id(c + 1, r, floor), id(c + 1, r + 1, floor)]);
                        tris.push([id(c, r, floor), id(c + 1, r + 1, floor), id(c, r + 1, floor)]);
                    }
                }
            }
            // Walls along the border, each edge's diagonal from its lower end, so two blocks sharing a side share its triangles.
            let mut border: Vec<((usize, usize), (usize, usize))> = Vec::new();
            for k in 0..n - 1 {
                border.extend([((k, 0), (k + 1, 0)), ((k, n - 1), (k + 1, n - 1)), ((0, k), (0, k + 1)), ((n - 1, k), (n - 1, k + 1))]);
            }
            for ((pc, pr), (qc, qr)) in border {
                let (tp, tq, fp, fq) = (id(pc, pr, false), id(qc, qr, false), id(pc, pr, true), id(qc, qr, true));
                tris.push([tp, fp, fq]);
                tris.push([tp, fq, tq]);
            }
            Arc::new(TriMesh::new(v, tris))
        };
        let meshes = [block(0.0), block(60.0)];
        let parts = split_shared_walls(&meshes);
        assert_eq!(parts.walls.triangle_count(), 2 * 2 * (n - 1), "the side the two blocks share, from each");
        let drawn = encode_solids(&[ObjectId::new(), ObjectId::new()], &meshes, ORIGIN, json!({ "name": "blocks", "timings": {} }));
        let levels = encode_solid_levels(&meshes, ORIGIN, json!({ "name": "blocks", "timings": {} }));
        let (dh, _) = header_of(&drawn);
        let (lh, _) = header_of(&levels);
        let vertices = dh["vertices"].as_u64().unwrap() as usize;
        let drawn_index = |i: usize| u16_at(&drawn, vertices * 12 + 2 * i) as usize;
        let level_index = |i: usize| u16_at(&levels, 2 * i) as usize;
        let chunks = dh["chunks"].as_array().unwrap();
        let lists = lh["chunks"].as_array().unwrap();
        assert_eq!(chunks.len(), lists.len());
        let mut thinned = 0;
        for (chunk, list) in chunks.iter().zip(lists) {
            let versions = list["levels"].as_array().unwrap();
            if chunk["shared"].as_bool().unwrap_or(false) {
                assert!(versions.is_empty(), "a shared wall's chunk has no versions");
                continue;
            }
            let (vs, is, ic) = (chunk["vertex_start"].as_u64().unwrap() as usize, chunk["index_start"].as_u64().unwrap() as usize, chunk["index_count"].as_u64().unwrap() as usize);
            let at = |v: usize| [0, 1, 2].map(|k| f32_at(&drawn, 3 * (vs + v) + k) as f64);
            let mut wall = std::collections::HashSet::new();
            for i in (is..is + ic).step_by(3) {
                let [a, b, c] = [drawn_index(i), drawn_index(i + 1), drawn_index(i + 2)];
                let [pa, pb, pc] = [at(a), at(b), at(c)];
                let (u, w) = ([pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]], [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]]);
                let nz = u[0] * w[1] - u[1] * w[0];
                let nn = ((u[1] * w[2] - u[2] * w[1]).powi(2) + (u[2] * w[0] - u[0] * w[2]).powi(2) + nz * nz).sqrt();
                if nz.abs() < 0.7 * nn {
                    wall.extend([a, b, c]);
                }
            }
            for version in versions {
                let (s, c) = (version["index_start"].as_u64().unwrap() as usize, version["index_count"].as_u64().unwrap() as usize);
                let kept: std::collections::HashSet<usize> = (s..s + c).map(level_index).collect();
                assert!(wall.is_subset(&kept), "a version dropped {} wall corners", wall.difference(&kept).count());
            }
            if !versions.is_empty() {
                let last = versions.last().unwrap()["index_count"].as_u64().unwrap() as usize;
                assert!(last * 4 <= ic, "the coarsest version keeps {} of {} indices", last, ic);
                thinned += 1;
            }
        }
        assert!(thinned >= 1);
    }
    /// Timing on the made-up large mine's blocks (`ZD_LARGE_MINE`): each 1 km tile of '9 blocks' split and its versions
    /// made, as the shell does a tile at a time. Counts and timings only.
    #[test]
    #[ignore]
    fn timing_block_levels_on_the_large_mine() {
        let Ok(path) = std::env::var("ZD_LARGE_MINE") else { return };
        let session = zd_commands::Session::open(&path).unwrap();
        let project = session.project();
        let layer = project.layers().iter().find(|l| l.name == "9 blocks").map(|l| l.id).unwrap();
        let mut tiles: std::collections::BTreeMap<[i64; 2], Vec<Arc<TriMesh>>> = Default::default();
        for o in project.objects().filter(|o| o.layer == layer) {
            if let zd_core::Body::Solid(s) = &o.body {
                let mesh = Arc::new(s.mesh().clone());
                tiles.entry(crate::state::Tiles::place(&mesh)).or_default().push(mesh);
            }
        }
        let origin = [650_000.0, 7_450_000.0, 0.0];
        let (mut held, mut shared, mut coarsest, mut split_ms, mut levels_ms, mut cpu_ms) = (0usize, 0usize, 0usize, 0.0, 0.0, 0.0);
        for meshes in tiles.values() {
            let t = Instant::now();
            let parts = split_shared_walls(meshes);
            split_ms += t.elapsed().as_secs_f64() * 1e3;
            held += parts.outer.triangle_count() + parts.walls.triangle_count();
            shared += parts.walls.triangle_count();
            let t = Instant::now();
            let buf = encode_solid_levels(meshes, origin, json!({ "name": "blocks", "timings": {} }));
            levels_ms += t.elapsed().as_secs_f64() * 1e3;
            let (header, _) = header_of(&buf);
            coarsest += header["coarsest_triangles"].as_u64().unwrap() as usize;
            cpu_ms += header["timings"]["levels_cpu_ms"].as_f64().unwrap();
        }
        println!(
            "{} tiles, {held} triangles: {shared} shared ({:.1} %), split in {split_ms:.0} ms; versions in {levels_ms:.0} ms ({:.0} s of CPU); coarsest {coarsest} triangles with the shared walls left out",
            tiles.len(),
            100.0 * shared as f64 / held as f64,
            cpu_ms / 1e3
        );
    }
}
