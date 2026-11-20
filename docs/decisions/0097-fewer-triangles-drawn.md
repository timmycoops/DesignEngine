# 0097 — Fewer triangles drawn: each chunk at the coarsest version under a pixel

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20, the Fewer triangles plan tab: all seven decisions, "I agree -
let's do it", meshoptimizer included)

## Why

Tim opened the made-up large mine (`testdata/synthetic/large_mine.py`, 10 km by 3 km, a 1 m lidar, 140,220 blocks): about
30 s to open, then 12 fps with the lidar and the blocks shown, 188 M triangles. The card draws about 2.3 billion triangles
a second whatever the scene (Phase 0: 25 M at 59.5 fps, 125 M at 22.8), so the only way up is to draw fewer.

Counted on the 1 m project: the lidar 84.8 M triangles; waste blocks 91.2 M, of which the overburden tops, the lidar
again, are about 60 M; coal blocks 12.4 M; and 19.0 M (18 %) are walls two blocks share, the same triangle in both, drawn
twice and never seen while both are shown.

Picking, snapping, box select and sections never read what's drawn: they work on the engine's f64 meshes. So a coarser
drawing moves no click, snap, cut or volume. The one exception was picking a seam horizon in the view
(`SceneSync.seamAt`), which read the drawn chunks.

## Decision

1. **How coarse, and when.** Each chunk gets a few coarser versions, each knowing the most it moves any point (m). Every
   frame each chunk is drawn with the coarsest version whose error, seen from the camera, is under 1 pixel; close in,
   full detail. A Detail setting sets the pixels (1 by default, 0 always full detail).
2. **What makes them: meshoptimizer**, through the `meshopt` crate (MIT), a new dependency outside the stack as it was. It
   collapses edges by least error with each chunk's border locked, so neighbouring chunks meet with no cracks whatever
   version each is drawn at. Every version indexes the chunk's own vertices, so only the index lists are added.
3. **Solids.** The walls two blocks share are left out of what's drawn while every block of the tile is shown, kept as a
   second part drawn only when one of its blocks is hidden, filtered off or isolated. Block tops and floors are
   simplified as surfaces are, each block's edges locked, so blocks still meet and a picked block's outline stays exact.
4. **When they're made.** In the shell, the first time a chunk is sent, on all cores, full detail drawn first so the view
   never waits; kept in memory for the session, not saved. If making them adds more than about 10 s to opening the large
   mine, they're saved in the `.zdp` as display data, decided then with the numbers.
5. **On the card,** every version of every chunk, so nothing arrives late as the view moves. (The plan said this adds
   about a third; measured on the large mine it's about half as much again, 505 MB of indices beside the lidar's 1,024 MB,
   since every version shares its chunk's vertices and only the indices are added.)
6. **The status bar** shows triangles drawn this frame beside those held: `60 fps · 4.2 M drawn of 188 M`.
7. **Picking a seam horizon** moves to the engine, a ray cast on the seam grids as every other pick is.

Workstreams, a PR each: (1) coarser versions in `zd-geom`; (2) sent with each chunk; (3) drawn by the error on screen, with
the Detail setting, the status bar and the seam pick; (4) blocks; (5) at scale, a `--lod` bench and the large mine by hand.

## Built

**Workstream 1, coarser versions** (`crates/zd-geom/src/display_levels.rs`): `display_levels(positions, indices, locked)`
makes a chunk's versions, each half the one before, down to 256 triangles, stopping where one can't take at least a fifth
off. Each is an index list into the chunk's own vertices, and its `error` is measured: the farthest any vertex it drops
lies from it (parry's point distance). meshoptimizer's own figure understated it, 0.19 m against 0.33 m measured on
lidar-like ground, so it isn't used. The border is found from the edges one triangle uses and locked by hand:
meshoptimizer's own border lock let a border vertex go on a rolling chunk. `locked` adds vertices of the caller's, for
workstream 4's block edges.

| Test (zd-geom) | Known answer | Tolerance |
|---|---|---|
| `a_flat_chunk_keeps_only_its_border` | 224 by 224 nodes flat, 99,458 triangles: every version error 0; the last has at most 4 vertices inside its 892 border corners, and 890 + 2 a vertex inside triangles | 1e-9 m; exact |
| `each_versions_error_is_the_farthest_a_vertex_lies_from_it` | A rolling 60 by 60 surface: each stated error found again by brute force over every triangle; each version at most 0.8 of the one before, never fewer than 256 | 1e-9 m |
| `chunks_side_by_side_meet_at_every_version` | Two rolling chunks sharing column 100: every version of each keeps all 119 edges along it, so no crack | Exact |
| `locked_vertices_stay` | A diagonal of vertices locked: in every version | Exact |

Timing in a release build (`timing_display_levels`, ignored): lidar-like ground, a 1 m lattice with up to 30 cm of
roughness, 40 chunks of 99,458 triangles (3,978,320) on their own threads: 660 ms on all 48 cores, 518 ms a chunk. One
chunk's versions: 49,728 triangles (0.315 m), 24,864 (0.505 m), 12,430 (0.576 m), 6,212 (0.658 m), 3,106 (0.850 m),
1,552 (1.155 m), 904 (7.747 m). At that rate the 85 M-triangle lidar takes about 15 s on 48 cores; workstream 2 measures
it on the large mine and settles decision 4 with the number.

**Workstream 2, sent with each chunk:** `render::encode_levels` cuts a mesh into the same chunks
`encode_triangulation` does (chunking doesn't depend on the thread count) and makes each chunk's versions, a chunk to a
core in turn. Its buffer is every version's `u16` indices, chunk after chunk, then a header listing each chunk's versions
with where their indices start, how many and the error. The shell's `stage_levels(id)` makes it for a surface or a solid
and holds it to be fetched as any render buffer is; a grid is refused and drawn at full detail as before. The view reads
it with `decodeLevels` (`app/src/viewport/levels.ts`, views onto the buffer, no copying). Nothing draws with it until
workstream 3.

| Test | Known answer | Tolerance |
|---|---|---|
| `each_chunks_versions_point_at_its_vertices_and_keep_its_seams` (zd-app) | A 450 by 450 node grid surface in at least 4 chunks: a list of versions for every chunk the drawn buffer has, every index inside its chunk, the errors growing, every edge two chunks share in every version of each | Exact |
| `levels.test.mjs` | A buffer laid out as the shell writes it reads back chunk by chunk, versions, indices and errors; another kind is refused | Exact |

On the large mine's 1 m lidar (`timing_levels_on_the_large_mine`, ignored, `ZD_LARGE_MINE`), release build, 48 cores:

| What | Number |
|---|---|
| Lidar | 84,800,000 triangles in 864 chunks |
| Drawn buffer | 1,024 MB in 683 ms |
| Versions | 5,728 of them, 84,040,594 triangles, 505 MB, in 9,778 ms (412 s of CPU) |
| Coarsest | 1,061,978 triangles for the whole lidar, the worst chunk 18.98 m out |

Decision 4 stands as agreed: the versions stay in memory and aren't saved. They're made after full detail is drawn, so
they add nothing to opening, which is the 10 s the decision set. On 48 cores they arrive about 10 s later. On an 8-core
laptop the same 412 s of CPU is about a minute, during which the lidar is drawn at full detail as today. If that
proves too slow by hand, saving them in the `.zdp` is the step decision 4 left open.

## Known limits

- A version can hold a sliver standing on three locked border vertices: it adds an edge along the border, opens no crack.
- The error is measured at the chunk's vertices; between them the version can stand off the full surface by a little more.
- Measuring the error is most of the CPU (about two thirds on lidar-like ground). A grid's versions aren't made yet.
