# 0011 — Reading Vulcan triangulations (`.00t`)

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29); the layout is checked against every local
sample, and awaits a vertex-for-vertex check against Deswik's export of the same files

## Decision

Vulcan's `.00t` is the triangulation format mine sites exchange day to day, so reading it comes
forward from "deferred" (CLAUDE.md) into Phase 1, workstream 4, slice 3. It is read only; DXF stays
the way back into Deswik, which is confirmed working (decision 0009).

- `surface.import` reads `.00t` beside DXF and grids; the mesh passes through the repair step and the
  crossing check (decision 0010).
- `solid.import` (new) makes a solid from a closed triangulation in a `.00t` or DXF file, through the
  same step, and refuses one that is not watertight or crosses itself, saying why and where.
- In the app, the Surface import takes `.00t`, and a Solids button imports as many files as are
  picked, one run each.

## The layout

There is no public specification. The layout was worked out from a client's files: 55 closed coal
solids (12 to 18 KB) and three surfaces of 0.3, 3.3 and 18.5 million triangles (client data: kept
local, never committed, and not quoted here). Every sample fits it exactly. Big-endian:

| Bytes | Content |
|---|---|
| 0..64 | free text: blank in the surfaces, "Created External" and a variant name in the solids |
| 64..120 | 14 i32: the vertex count at byte 72 and the triangle count at byte 96; the rest zero in every sample |
| 120.. | per vertex: f64 x, y, z |
| then | per triangle: i32 a, b, c (vertex numbers from 1), then three i32, zero in every sample |
| then | in the solids only: 792 bytes of display properties (colour, shading, the path it was saved to), not read |

Some files are stored compressed in the `vulZ` container Vulcan grids use (decision 0006), which
unpacks to the same layout; all 55 solids were, none of the surfaces. Every closed sample has a
positive volume, so triangles run counter-clockwise about their outward normal; surfaces vary (one
stored facing down, one up), which the repair step's orientation sorts out and logs.

The reader refuses what does not check out: counts that disagree with the file's size, a header word
not seen before (a variant to look at, not guess), a vertex number out of range, a coordinate that is
not finite. Non-zero trailing triangle words and the properties block are counted in the log.

## Evidence

- Synthetic: a box written in the plain layout reads back with every coordinate exact, watertight and
  1,000 m³; the compressed layout unpacks to the same mesh; truncated files, unknown header words,
  vertex numbers out of range and NaN coordinates are refused.
- Through the registry: a sheet with every other triangle reversed imports as a surface facing up,
  coordinates exact; a box imports as a solid of 1,000 m³; an open box and two boxes run together are
  refused with the reason.
- The client's files (opt-in tests, `ZD_VULCAN_TRI_DIR`, `ZD_VULCAN_TRI_TIMING`): 53 of the 55 coal
  solids import as solids; the other two are each two or three closed pieces meeting along an edge
  with four faces, refused as non-manifold with the edge's location. The 18.5-million-triangle
  surface holds 11 crossing pairs, flagged with where the first is.
- Still to do: `ZD_VULCAN_TRI` with `ZD_VULCAN_TRI_DXF` compares a file with Deswik's DXF export of it
  vertex for vertex and triangle for triangle, and `ZD_VULCAN_TRI_VOLUME` checks a solid's volume
  against Deswik's to 0.001 m³. This waits on those exports.

## Timings

Release build, this machine's 48 threads, whole `surface.import` runs (read, repair, crossing check):

| Triangles | Read and repair | Crossing search | In all |
|---|---|---|---|
| 3.3 million | 0.5 s repair | 1.1 s | 1.8 s |
| 18.5 million (666 MB) | 3.9 s repair | 2.3 s | 8.1 s |

Getting there changed three things in slice 2's code, with the same results: the weld hashes its cells
with a fast multiply-rotate hash rather than the standard library's keyed one (5.5 s to about 2 s on
9 million vertices), the orientation fill finds each edge's neighbours through a list of the triangles
at each vertex, across cores, instead of sorting every edge (4.5 s to under 1 s), and the crossing
search builds a BVH per core-sized tile of the mesh in parallel rather than one BVH on one core (6.5 s
to under 1 s to build). A test checks that tiles find exactly what one BVH finds.
