# Phase 0 results

Branch `phase0/stress-test`, run 2026-10-28. Targets are from
[docs/decisions/0001-stack.md](../docs/decisions/0001-stack.md).

> **The gate passed on real data on 2026-10-30:** see
> [Real data](#real-data-golden-pit-2026-10-29). `testdata/golden/` held no pit data for the first
> run, so the [Summary](#summary) and everything after it used the **synthetic pit** (see
> [Data](#data)). The harness picks real data up automatically (see [README.md](README.md)).

## Real data (golden pit, 2026-10-29)

A client's open-cut coal pit in Queensland, described by a local `pit.json` (see
[testdata/golden/README.md](../testdata/golden/README.md)); the data stays local and is not named
here. Same machine as below; release builds.

| Item | Size |
|---|---|
| Topography | Vulcan triangulation, 1.64 M vertices, 3.28 M triangles (stored facing down; turned up by the repair step) |
| Large surface | Vulcan triangulation, 9.25 M vertices, 18.5 M triangles (holds 11 crossing pairs) |
| Seams for Check 2 | three parent seams' roof and floor Vulcan grids, 266,031 nodes each at 10 m |
| Seam model | 15 horizons from the site's stack definition (decision 0007) |
| Solids | 42,223 relimiting solids (coal and waste, by strip, block, seam and bench) as `.00t` |

### Phase 0 Check 2 on real seams: PASS

Each seam solid (4.2 M triangles) cut by the topography (3.3 M triangles), `cargo bench -p bench
--bench check2_booleans`:

| Seam | Solid tris | Cut total | Below + above tris | Watertight | Conservation error | Column vs mesh |
|---|---:|---:|---:|---|---:|---:|
| A | 4,248,240 | 4.9 s | 6.0 M | yes | 5.2e-13 | 4.6e-6 m³ |
| B | 4,247,080 | 4.1 s | 4.6 M | yes | 2.6e-13 | 1.5e-6 m³ |
| C | 4,229,036 | 4.7 s | 5.5 M | yes | 3.9e-14 | 8.9e-7 m³ |

The grids reach past the topography's extent; the uncovered coal reports as above, as the check
notes.

### Phase 0 Checks 1, 3 and 4 on real data: PASS

`.\bench\golden_app_bench.ps1 testdata\golden\<pit>\pit.json`, 2026-10-30: the release app on WebGPU
(canvas 2400 x 1363) with the topography, the large surface and six seam grids, **24,958,820 real
triangles**. The app was built before PRs #24 and #25, which touch neither the viewport, the transfer
nor the snap.

**Check 1, viewport.** The bar is met at 25 M real triangles. Tiled copies of the real scene:

| Triangles | Mean fps | p50 frame | p99 frame | Worst frame | Frames over 33 ms |
|---:|---:|---:|---:|---:|---:|
| **25.0 M** | **59.5** | 16.7 ms | 18.9 ms | 34.1 ms | 1 of 475 |
| 49.9 M | 43.9 | 22.5 ms | 27.5 ms | 28.3 ms | 0 of 350 |
| 74.9 M | 34.8 | 29.8 ms | 60.6 ms | 68.9 ms | 38 of 277 |
| 124.8 M | 22.8 | 44.7 ms | 54.8 ms | 83.5 ms | 163 of 181 |

At 50 M the real scene runs slower than the synthetic pit did (49.5 fps), at a slightly smaller canvas.

**Check 3, transfer.** Real surfaces over the shared buffer, from the request to the frame submitted
and to the GPU finishing. Reading, repairing and the crossing search, which come first, are shown
beside them:

| Surface | Triangles | Bytes | To frame submitted | To GPU done | Import and encode |
|---|---:|---:|---:|---:|---:|
| Topography | 3.28 M | 39.6 MB | 0.06 s | 0.09 s | 1.1 s |
| Large surface | 18.5 M | 223.7 MB | **0.24 s** | 0.37 s | 8.3 s |
| Seam grid | 0.53 M | 6.4 MB | 0.02 s | 0.04 s | 0.06 s |

**Check 4, editing.** Against the full 25.0 M triangles: snap round trip **1.6 ms** p50, 3.3 ms p99,
3.8 ms worst, with 0 misses in 300 updates; each update in a frame submitted within 19.8 ms. The snap
index takes 6.1 s to build for this scene (2.4 s on the synthetic pit). This is measured, not felt:
the synthetic pit's hands-on "snappy" has not been repeated on these surfaces.

**Gate: passed, 2026-10-30.** All four checks pass on real data. Tim closed the gate on these numbers.

### Phase 1 gate on real data

`bench/phase1_gate.py`, through `zd` (decision 0012):

| # | Check | Result |
|---|---|---|
| 1 | Import topography, seam grids and a toe string; every repair logged | topography in 1.1 s (read, repair 0.5 s, crossing search 0.3 s) and 27 seam grids in 55 ms; **toe string pending** (to be exported from Deswik) |
| 2 | Toe on the topography: insert, move, delete, grade, offset | pass by command (drape added 1,760 edge crossings; edits 0.17 s); digitising in plan view is by hand |
| 3 | Undo and redo back to empty | pass: 11 runs undone and redone, same 30 objects and revisions |
| 4 | Save, close, reopen bit for bit | pass: 81 MB saved in 0.66 s, opened in 0.54 s, every coordinate the same |
| 5 | DXF export read back | pass bit for bit; Deswik's read confirmed by hand |
| 6 | Seam model from the real grids | pass: 15 horizons, 3 issues flagged, none blocking, 61 ms |
| 7 | Python: open, list, run, read the log | pass (the script itself) |

The toe string came on a second pit, with its own topography (8.06 M triangles, in 3.0 s), 30 grids of 15
seams and 3 closed strings exported from Deswik, so items 1 to 7 ran together on one pit. All pass: 23
runs undone to empty and redone, 88 MB saved in 0.8 s and opened in 0.9 s with every coordinate the same,
and a seam model of 15 horizons with 7 blocking crossings flagged at 36 nodes (decision 0020). Tim then
ran item 2 by hand in plan view: drawing with snapping, inserting, moving and deleting vertices, setting
a grade and offsetting.

**Gate: passed, 2026-10-31.** Tim's notes for later: zooming and rotating about a fixed centre feels
wrong when zoomed in on an object, and snapping has no off switch.

### Phase 2 gate on real data

The release app through its real toolbar, canvas and keys (`zd-app.exe --bench --solids <zip> --surfaces <topo>
--strings <dxf> --seam <csv>`), and `a_real_pit_slices_exactly_and_fast` in `zd-commands` on both pits. PIT A is
11.7 M triangles, 21,469 solids and 5,724 strings. PIT B is 8.4 M triangles, about 42,000 solids and the only real
coal seam models, so seams are judged on it. Details are in decision 0022.

| # | Check | Result |
|---|---|---|
| 1 | Pick two points: the view becomes the slice, one click goes back | pass, by hand |
| 2 | Every cut line on the plane and on what it came from, to 1 mm | pass: 41 sections on each pit, 1,183,889 cut segments all met their mesh within the 0.5 mm the test allows, and no point lies more than 6e-11 m off the plane |
| 3 | Each step under 50 ms on the full scene | pass: 28 ms p50 and 30 ms worst on PIT A, 46 ms and 47 ms on PIT B with its seam model |
| 4 | The overview shows where the section is, and moving it there moves the section | pass, by hand |
| 5 | Line weights, an RL grid, distance along the section, solids and seams filled | pass: labels read against the engine to 0.12 mm over 9,708 points, and all 485 solid loops a section on PIT A fill |
| 6 | Digitise and edit a string in section, snapped to the cut lines | pass on PIT B: vertices exact to the bit, 6e-10 m off the plane, snap 1.3 ms p50, drag and undo through the real tools |
| 7 | A section beside the same section in Deswik reads as clearly | passed by Tim on look and feel, without a side-by-side |

**Gate: passed, 2026-10-31.** Tim: "the look and feel is very good for the section so I'm happy with that". Left
for later: lightening the RL labels on a smaller viewport, the plan map's size, setting colours, where line weight
lives, and the odd solid loop on PIT B that stays unfilled, about one a section and counted on the status line.

### Solids at scale

All 42,223 relimiting solids through `solid.import`: **41,585 imported** (98.5 %) at 3.8 ms each
once unpacked (2.7 minutes), 955 of them with touching pieces taken apart, 204 M m³ in all. **638
refused**, each with where: 327 cross themselves, 298 have open edges together with edges of three
or more faces, 13 have open edges only.

With the solid rules for crossings (decision 0014), **41,882 import as read** (99.2 %), 276 of them
with thin pairs reported (knife edges under 1 mm, which the first check took for folds). With repair
(decision 0013), 529 of the 638 import; **109 are refused**: 86 with open edges or edges of three or
more faces, 16 with holes over 10 % of the surface, and 7 that cross themselves (4 of them on a
surface that is not closed, 2 still crossing after the cut, 1 partly inside out).

With hanging and loose sheets taken off and one piece touching itself split on one side (decision
0015), **41,891 import as read** and 326 with repair; **6 are refused** (the 332,000 m³ waste solid
over a sub-millimetre fold, three small coal solids with interior walls, two files that are only a
sheet). Of the 109, 103 import (121,535 m³), every one with no volume change. The pass takes 168 s.
Nothing that imported before is refused; 38 solids repaired before come out differently, because a
sheet is now taken off rather than its loose edge filled as a hole: 26 keep their volume, and 12 lose
24.3 m³ in all (the largest 9.7 m³ of 12.1), the volume that fill enclosed outside the solid.

With lips and flat folds counted as thin (decision 0017), **41,917 import as read** and 301 with repair;
**5 are refused**: three small coal solids with interior walls and two files that are only a sheet.
The 332,000 m³ waste solid imports. The pass takes 143 s. 34 solids no longer go through the union,
each keeping the volume the union gave it; 14 of them come in as several pieces rather than one (108
pieces, 65 of them under 0.001 m³, the same total volume).

The general cut (decision 0016) on the same solids: all 44,175 cut by the mined-out topography (3.28 M
triangles) into **51,783 parts in 187 s**, 4.2 ms a solid, every part checked by the solid rules. Each
solid and its parts agree to 0.15 m³ at most (1.4e-5 of 10,996 m³). One solid is left uncut and named.
With decision 0017's rules: 44,270 solids into 51,875 parts in 112 s, 2.5 ms a solid, no whole-surface
retries, two solids left uncut and named, and solids and parts within 0.27 m³ (3.2e-5 of 8,407 m³).
With one file making one solid (decision 0018): 42,218 solids into 49,764 parts in 107 s, 2.5 ms a solid,
no whole-surface retries, one solid left uncut and named, and solids and parts within 0.27 m³. Every
file keeps its volume (to 1e-6 m³) and its result: 41,917 as read, 301 with repair, 5 refused.

One run for them all (decision 0019): **42,218 solids from a 72 MB zip in 10.6 s** with repair on, 7.7 s
with it off, on 48 threads. In the release app, 4.9 M triangles from the zip to drawn with the snap index
built in **12.6 s**, then orbiting at 60 fps with no frame over 33 ms, snapping at 0.9 ms p50, undo 24 ms.

## Summary

| Check | Target (0001-stack) | Result | Verdict |
|---|---|---|---|
| 1 · Viewport | Smooth orbit and zoom, ~60 fps, at tens of millions of triangles | **60 fps at 24.3 M unique triangles** (p99 frame 17.3 ms; no frame over 33 ms in one run, two up to 50 ms in a repeat). Tiled copies: 50 fps at 48.5 M, 36 fps at 72.8 M, 23 fps at 121 M | **PASS** up to ~25 M on this entry-level GPU; ≥ 50 fps to ~50 M here (44 fps on the real pit) |
| 2 · Booleans | Watertight output; seconds, not minutes, per cut | Every part watertight; below + above = input to ≤ 1e-12. 0.24 s (0.2 M tris), 0.74 s (0.9 M), 2.4 s (3.5 M), 9.4 s (14 M) | **PASS** |
| 3 · Transfer | Multi-million-triangle mesh Rust → viewport well under 1 s, binary | Request → frame submitted over a WebView2 shared buffer: **0.06 s at 2 M, 0.09 s at 8 M, 0.20 s at 18 M** for a general triangulation; 0.015, 0.035 and 0.065 s for grid surfaces | **PASS** (first run: 0.23, 0.88 and 2.2 s over IPC) |
| 4 · Editing | Vertex drag with surface snapping feels instant | Snap round trip **1.2 ms** p50 (8.2 ms worst) against 24.3 M triangles; every update is in the next frame submitted (worst 21.5 ms) | **PASS**; feel confirmed hands-on ("snappy") |

All four checks pass here, on synthetic data; the gate itself
[passed on the real pit](#phase-0-checks-1-3-and-4-on-real-data-pass). The first run left Check 3 partial; that was fixed in the transfer layer alone, as 0001 anticipates ("a failure
points to one layer to fix"). See [Check 3](#check-3-binary-transfer-rust--viewport) and
[Follow-ups](#follow-ups).

## Machine

| Item | Value |
|---|---|
| CPU | AMD Ryzen Threadripper PRO 7965WX, 24 cores / 48 threads |
| RAM | 256 GB |
| GPU | NVIDIA RTX A1000 (Ampere, 8 GB; entry-level workstation card), driver 32.0.15.7680 |
| Display | 3840 x 2160 at 60 Hz, 150 % scaling |
| OS | Windows 11 Pro, build 26300 |
| WebView2 | 155.0.4283.62 (Chromium 155) |
| Toolchain | Rust 1.99.1 (MSVC), Node 22.22, pnpm 12.15, three.js 0.186.1, Tauri 2.12.5, Manifold 3.5 via manifold-csg 0.4.1, parry3d-f64 0.31.1, webview2-com 0.39.1 |
| Builds | Release (`cargo bench`, `pnpm tauri build --no-bundle`) |

## Data

Synthetic pit at MGA-scale coordinates (SW corner E 650 000, N 7 450 000), generated by
`cargo run --release -p bench --bin make_synthetic`:

| Surface | Form | Size | Triangles |
|---|---|---|---|
| Topography | XYZ grid, 2001 x 2001 nodes at 5 m (10 km x 10 km), undulating | 135 MB | 8,000,000 |
| Seam roof | XYZ grid, same lattice, dipping ~2.3° east with rolls | 136 MB | 8,000,000 |
| Seam floor | XYZ grid, same lattice; thickness −1 to 5 m, so the seam pinches out in places | 136 MB | 8,000,000 |
| Design surface | DXF 3DFACE, 27.4 m lattice (offset from the seam lattice), weaving ±6 m through the seam | 61 MB | 272,322 |

Check 2 builds its own seam solids from the same functions at 10 m cells (250 to 2000 cells a side).

## Check 2: seam solids cut by a design surface (Manifold)

Target: watertight output; seconds, not minutes, per cut. Pass line used here: both parts
watertight (closed, manifold, consistently wound), both volumes > 0, below + above = input
within 1e-6 relative, and the whole cut (closing the design surface, Manifold in and out, boolean)
under 10 s.

| Case | Solid tris | Design tris | Half-space tris | Close ms | To Manifold ms | Boolean ms | From Manifold ms | **Cut total** | Below / above tris | Watertight | Below / above m³ | Conservation error | Result |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|---|
| 250 x 250 @ 10 m | 201,716 | 69,938 | 72,180 | 21 | 108 | 103 | 11 | **0.24 s** | 108,224 / 123,008 | yes | 4,993,493 / 6,050,996 | 3e-15 | PASS |
| 500 x 500 @ 10 m | 870,832 | 272,322 | 276,748 | 88 | 314 | 288 | 45 | **0.74 s** | 512,826 / 487,834 | yes | 25,551,687 / 25,871,098 | 1e-14 | PASS |
| 1000 x 1000 @ 10 m | 3,498,796 | 1,077,512 | 1,086,318 | 357 | 997 | 822 | 183 | **2.4 s** | 2,012,782 / 2,004,300 | yes | 102,610,080 / 102,734,116 | 5e-14 | PASS |
| 2000 x 2000 @ 10 m | 13,975,988 | 4,286,592 | 4,304,158 | 1,512 | 4,457 | 2,598 | 841 | **9.4 s** | 8,053,672 / 7,993,088 | yes | 410,433,392 / 411,215,057 | 1e-12 | PASS (marginal) |

- Seam solid build time: 3, 12, 43 and 173 ms. Each solid's enclosed volume matches its 2.5D column
  volume to within 1.5e-5 m³ (the reconciliation check from engine rule 9).
- The boolean itself scales well (2.6 s for 14 M triangles). At the top size, the cost is Manifold taking
  the mesh in (4.5 s: merge, half-edge build, validity) and closing the 4.3 M-triangle design surface
  (1.5 s). A real seam solid per strip or block is far smaller than a single 20 km x 20 km seam, so
  the 0.2 to 2.4 s rows are the representative ones.
- Seam logs (cells left out because a corner was under 0.1 m thick, nodes where the floor was clamped
  to the roof) are in `bench/out/check2.md`. No corner-pinch cells were needed on this data.

## App checks (1, 3, 4): how they were run

`zd-app.exe --bench` (release build) runs Checks 1, 3 and 4 unattended in the real Tauri window and
writes `bench/out/app-results.json`. Renderer: three.js `WebGPURenderer` on the **WebGPU** backend
(no WebGL2 fallback was needed), 4x MSAA, reversed-Z depth, flat shading, canvas 2400 x 1500
(1600 x 1000 window at 150 %). Frame times are `requestAnimationFrame` intervals, so 60 fps is the
display's ceiling.

**Render buffers (v2).** Meshes cross the bridge in one of two compact layouts (details in
[0003](../docs/decisions/0003-phase0-choices.md)):

- *General triangulation:* chunks of at most 65,535 vertices with f32 positions and u16 indices,
  and no normals (the viewport shades flat). About 12 bytes per triangle, down from 24.
- *Grid surface:* elevations only, as u16 steps over the surface's z range, about 1 byte per
  triangle. The viewport rebuilds the lattice and the triangles by the engine's rule; a Rust test
  proves the rule matches, and the viewport checks the triangle count on every load. Elevations
  are within half a step: ≤ 0.3 mm for the topography (34 m range) and ≤ 3.1 mm for the roof
  and floor, which fall about 407 m across the 10 km pit. This is display only; snapping and
  every engine calculation use the f64 surfaces. Hands-on check: no cracks visible along the tile
  seams when zoomed in.

They travel through a **WebView2 shared buffer**: Rust copies the bytes once into shared memory, and
the page gets an `ArrayBuffer` over the same memory. Raw bytes over Tauri IPC remain the fallback.

Loading the four files through the Open path (Rust reads, triangulates for snapping and encodes;
the page fetches, decodes and draws):

| File | Layout | Triangles | Buffer | Rust read | Triangulate | Encode | Fetch (shared) | Decode | **Total to first frame** | First run |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| topo.xyz | grid | 8,000,000 | 8.1 MB | 398 ms | 40 ms | 28 ms | 6 ms | 31 ms | **0.53 s** | 1.62 s |
| roof.xyz | grid | 8,000,000 | 8.1 MB | 413 ms | 43 ms | 27 ms | 5 ms | 25 ms | **0.52 s** | 1.50 s |
| floor.xyz | grid | 8,000,000 | 8.1 MB | 406 ms | 44 ms | 27 ms | 5 ms | 23 ms | **0.52 s** | 1.49 s |
| design.dxf | triangulation | 272,322 | 3.3 MB | 207 ms | — | 8 ms | 3 ms | 0 ms | **0.23 s** | 0.25 s |

Reading the 135 MB text file is now three-quarters of a load.

## Check 1: viewport

One full orbit of 8 s while zooming in and out twice, after 30 warm-up frames. The first row is
the real scene (three 8 M surfaces plus the design, all unique geometry). Later rows tile copies
beside it until the scene passes 25, 50 and 100 M triangles. Copies share GPU buffers, so they
test vertex and raster throughput, not GPU memory.

| Scene | Triangles | Frames | Mean fps | p50 frame | p95 | p99 | Worst | Frames > 33 ms | First run (fps) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Real size (unique) | 24,272,322 | 479 | **60.0** | 16.7 ms | 17.1 ms | 17.3 ms | 17.5 ms | 0 | 60.0 |
| 2 x tiled | 48,544,644 | 400 | 50.2 | 20.1 ms | 22.1 ms | 22.8 ms | 25.7 ms | 0 | 45.5 |
| 3 x tiled | 72,816,966 | 285 | 35.8 | 27.7 ms | 32.6 ms | 33.3 ms | 33.5 ms | 2 | 32.0 |
| 5 x tiled | 121,361,610 | 184 | 23.1 | 42.2 ms | 50.0 ms | 50.9 ms | 51.0 ms | 184 | 20.6 |

Frame pacing stays even at every size (p99 within 21 % of p50), so the scene degrades smoothly
rather than stuttering. A repeat run (after the encode change, with no rendering change) gave 59.6,
51.2, 36.5 and 23.6 fps. Its real-size orbit had two frames over 33 ms (worst 50 ms) in 476, with
p99 still 17.4 ms, so occasional hitches do happen at real size. The v2 buffers sped up the larger scenes by 10 to 12 %: half the vertex
data, u16 indices, and per-chunk frustum culling. The trade is flat shading, which shows each
triangle's facet. That's honest for a TIN, but smooth shading for grids can come back later
(normals from the lattice, computed in the vertex shader). More headroom if needed: drop MSAA, or
level of detail for distant tiles.

## Check 3: binary transfer, Rust → viewport

A synthetic topography is generated and encoded in Rust first (not timed). It is then handed to
the page and drawn. Each size is sent as a general triangulation and as grid elevations, over a
WebView2 shared buffer and over IPC.

| Triangles | Layout | Buffer | Transport | Fetch | Decode + geometry | **Request → frame submitted** | Request → GPU done |
|---:|---|---:|---|---:|---:|---:|---:|
| 2,000,000 | triangulation | 24 MB | shared | 10 ms | 0 ms | **64 ms** | 186 ms |
| 2,000,000 | triangulation | 24 MB | IPC | 119 ms | 0 ms | 124 ms | 170 ms |
| 2,000,000 | grid | 2.0 MB | shared | 2 ms | 10 ms | **15 ms** | 62 ms |
| 2,000,000 | grid | 2.0 MB | IPC | 13 ms | 7 ms | 31 ms | 78 ms |
| 8,000,000 | triangulation | 97 MB | shared | 37 ms | 0 ms | **93 ms** | 149 ms |
| 8,000,000 | triangulation | 97 MB | IPC | 445 ms | 3 ms | 459 ms | 488 ms |
| 8,000,000 | grid | 8.1 MB | shared | 5 ms | 22 ms | **35 ms** | 60 ms |
| 8,000,000 | grid | 8.1 MB | IPC | 42 ms | 18 ms | 67 ms | 93 ms |
| 18,000,000 | triangulation | 217 MB | shared | 86 ms | 1 ms | **202 ms** | 287 ms |
| 18,000,000 | triangulation | 217 MB | IPC | 1,068 ms | 1 ms | 1,090 ms | 1,128 ms |
| 18,000,000 | grid | 18 MB | shared | 9 ms | 44 ms | **65 ms** | 96 ms |
| 18,000,000 | grid | 18 MB | IPC | 90 ms | 38 ms | 141 ms | 169 ms |

The first run (f32 positions + f32 normals + u32 indices over IPC) took 0.23 s, 0.88 s and 2.23 s.

- The shared buffer moves 2.5 GB/s, including the copy into shared memory, against IPC's flat
  ~210 MB/s. The 18 M-triangle triangulation still lands in 0.2 s, so even the worst case is
  well under 1 s.
- Not in these times: encoding in Rust, which happens before the request (in the real app, at
  load). Grids take 6, 26 and 57 ms. Triangulation encoding now runs on every core: binning,
  chunking (workers take plan bins from a shared counter) and byte writing. A test proves the
  output is identical on 1 thread and on 16.

  | Triangulation encode | 2 M | 8 M | 18 M |
  |---|---:|---:|---:|
  | Serial, best of 3 (`encode_timing` test) | 53 ms | 208 ms | 464 ms |
  | Parallel, best of 3 (`encode_timing` test) | 21 ms | 63 ms | 115 ms |
  | In the app, single cold run: serial → parallel | 63 → 41 ms | 248 → 120 ms | 558 → 227 ms |

  At 18 M the rest is the serial counting sort (34 ms) and memory-bound chunking (42 ms over about
  180 bins). Run the timing test with
  `cargo test --release -p zd-app -- --ignored --nocapture encode_timing`.

## Check 4: editing (polyline drag with snapping)

Six vertices are placed by ray casting from the screen onto the surfaces. Vertex 2 is then dragged
round a circle in 300 updates. Each update casts a ray in Rust (`parry3d-f64` BVH over all four
surfaces, 24.3 M triangles), moves the vertex and waits for the frame. The same drag was repeated
with only the 272 k-triangle design surface drawn, to separate render cost from snapping.

| Scene drawn | Snap round trip (JS → Rust → JS) | Update → frame submitted | Update → GPU-done callback |
|---|---|---|---|
| 24.3 M triangles | p50 **1.2 ms**, p95 2.3, max 8.2 | p50 4.2 ms, p95 18.6, max **21.5** | p50 16.9 ms, p95 31.3, max 33.8 |
| 0.27 M triangles | p50 1.1 ms, p95 1.5, max 1.8 | p50 13.3 ms, p95 14.1, max 17.7 | p50 16.7 ms, p95 18.0, max 22.5 |

- 300 of 300 updates hit a surface in both runs; no misses.
- Every update is drawn in the next frame submitted. The submit figure is just the wait for the next
  60 Hz frame plus the snap, so its p50 depends on where in the frame the drag loop happens to start.
- The GPU-done callback came back within one or two frames (16.9 ms p50, 33.8 ms worst). In the
  first build it read a constant 66.7 ms whatever the scene size. That delay was Chromium's callback
  timing, not render cost, and it has not recurred.
- BVH build for snapping: **2.4 s** for the four surfaces in parallel (24.3 M triangles), run in the
  background after loading. The first serial build took 7.1 s.
- Hands-on check (Tim, 2026-10-28, synthetic topo, roof and floor loaded): placing and dragging
  vertices "all feels snappy".

## Follow-ups

1. **Re-run on real data.** Put a pit in `testdata/golden/<pit>/` (topo, `<seam>_roof`,
   `<seam>_floor`, optional `design`). Check 2 picks it up automatically. For the app, run
   `zd-app.exe --bench --surfaces <topo>,<roof>,<floor>,<design>`.
2. ~~Triangulation encode time~~: done, now multithreaded (115 ms at 18 M, best of 3; was 464 ms).
   A parallel counting sort would save about 25 ms more, if it ever matters.
3. **Smooth shading for grids**, if flat shading looks too faceted in use: rebuild normals from the
   lattice in the vertex shader. That costs no transfer bytes.
4. **Check 2 at the top size.** Manifold's mesh intake (4.5 s of 9.4 s at 14 M triangles) dominates.
   Cutting per strip or block keeps real cuts in the 0.2 to 2.4 s range shown.

## Reproduce

See [README.md](README.md). In short:

```powershell
cargo bench -p bench --bench check2_booleans                 # Check 2
cargo run --release -p bench --bin make_synthetic             # data for Checks 1, 3, 4
cd app; pnpm tauri build --no-bundle; ..\target\release\zd-app.exe --bench
```
