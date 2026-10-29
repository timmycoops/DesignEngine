# CLAUDE.md — Zovlyn Pit Design Engine

Read this first in every session. Decisions here are locked; do not re-argue them unless the user asks.

## What this is

A fast, modern 3D design and coal reserves tool for open-pit coal mines. It replaces Deswik for:
highwall/endwall/lowwall face projection, ramps, surfaces, strip and block layout, block solids,
and reserves interrogation. Scheduling stays in Spry. Owner: Tim (Zovlyn Pty Ltd), an experienced
mine planner — use correct mining terms (toe, crest, batter, berm, highwall, lowwall, endwall,
strip, block, seam, parting, roof, floor, base of weathering, subcrop, interburden, strip ratio).

Priorities, in order: **correctness of geometry and volumes**, **speed**, **feel**, **look**.

## Locked stack

| Layer | Choice |
|---|---|
| App shell | Tauri 2 (Windows first) |
| Engine | Rust workspace under `crates/` |
| Solid booleans | Manifold via `manifold-csg` / `manifold3d` crate (64-bit MeshGL64) |
| Spatial queries | `parry3d` (BVH ray casting) |
| Triangulation | `spade` or Manifold's constrained Delaunay |
| Wall offsets | Weighted straight skeleton in `zd-geom` (edge speed from batter, berms as delayed starts); no offset-then-repair |
| Planar offsets | Clipper2 or Manifold `CrossSection` for plain 2D offsets |
| Viewport | three.js with `WebGPURenderer` (auto WebGL2 fallback) |
| Interface | React + TypeScript, shadcn/ui, Tailwind, dockview, TanStack Table |
| Data at rest | Parquet/Arrow using the OMF2 mesh layout (vertices `x, y, z` f64; triangles `a, b, c` u32, counter-clockwise about an outward normal); DXF/CSV/OMF only for exchange. Evaluate the `omf` Rust crate before writing our own |
| Python | PyO3 bindings in `crates/zd-py` from Phase 1, built with maturin (reconciliation and test scripts run from Python) |
| Package manager (JS) | pnpm |

## Engine rules (non-negotiable)

1. **Engine crates contain no UI code** and never depend on `app/`.
2. **f64 everywhere in the engine.** Mine grid coordinates (e.g. MGA eastings/northings) are stored exactly.
   Only the viewport subtracts a local origin before sending f32 data to the GPU.
3. **Every operation is a typed command** in `zd-commands`: named inputs, parameters, outputs, and a
   serialisable record of the run.
4. **Commands never modify their inputs.** They create new objects on a named layer. No live dependency
   graph, no automatic rebuilds — a new design means the user re-runs the command.
5. **Cuts are solid booleans.** To cut a solid by an open surface, close the surface into a half-space
   just beyond the solid's bounding box, then run a Manifold boolean.
6. **Projection targets are triangulations.** Grids convert to triangulations on import; one projection path.
   Handle convoluted surfaces at low angles, and fall back to a level where the projection never hits the surface.
7. **Large meshes cross the Tauri bridge as binary** (typed arrays), never JSON.
8. **Validity is explicit.** Every mesh carries computed flags: `Closed`, `Manifold`, `ConsistentWinding`,
   `NonDegenerate`, `NonSelfIntersecting` (Seequent Evo vocabulary). Imports pass through a repair step first
   (weld by tolerance, drop empty and duplicate faces, orient by flood-fill, find crossings) and log every fix.
   Nothing invalid reaches Manifold or a volume report.
9. **Solids are the volume of record.** Qualities and thicknesses are sampled from seam grids inside each solid.
   A 2.5D column calculation (vertical-walled polygons, exact partial cells) runs alongside as a reconciliation
   check against Vulcan and Minex. Report any difference beyond tolerance; never hide it.

## Object model

- `Polyline` (string): ordered `[f64; 3]` vertices, closed flag, attributes.
- `Surface`: triangulated mesh (vertices f64, triangles u32), name, layer.
- `Solid`: closed manifold mesh; must always be watertight.
- `Grid`: regular raster (origin, cell size, rows, cols, z values with NaN for nulls).
- `SeamModel`: an explicit, ordered stack of horizons (the Deswik "surface stacking" pattern), defined in a CSV
  the user edits; `seam.suggest_definition` writes a starting one from the grids' geometry, never from names.
  Each horizon has a code, material, parent horizon (for splits), working flag, roof and floor grids, quality
  grids and a density grid. The model links to its grid files in place (fingerprinted, never copied into the
  project); grids used for design are imported as objects. The stack is checked, never fixed: a floor above
  its roof, a crossing in the working section or a daughter outside its parent is flagged with its location,
  and blocks solids and reserves until the source grids are corrected. Existence polygons and a crop surface
  (topography or base of weathering) come later. Faulting applies to structure, never to thickness.
  See `docs/decisions/0007-seam-model.md`.
- `Rule`: a function from a string to one value per edge. Types: fixed, fixed list, inside polygon,
  inside solid, segment azimuth, by horizon. Later: block model, geotech solid with transition distance.
  Blending between rules may only widen berms and flatten slopes.
- `FaceProfile`: an ordered list of steps, each "project from surface A up to surface B or an RL at a batter
  rule" or "berm of a width rule", with berm on roof and berm on floor per horizon, minimum and maximum bench
  height, wall type (face, side, end), and side (left or right for open highwall strings).
- `Ramp`: centreline or start point, direction, start and end bench, width, grade (%, ratio or degrees,
  measured on the inside edge by default), inside and outside taper, entry type, flat or graded switchbacks
  with radius. It reports the bench where the grade can't be met instead of drifting.
- Every object has a stable id (128-bit, generated in-house), a name, a layer, and provenance (the command
  run that made it). Edits write a new revision of the same object; earlier revisions stay for undo
  (see `docs/decisions/0004-phase1-foundations.md`). A project is a `<name>.zdproj/` folder of Parquet files.

## Scope limits for v1

- Seam dip below 30° and normal faults only, with fault blocks as separate domains. Reverse faults,
  thrusts and overturned seams wait for v2 (the incumbents switch methods there too).
- No scheduling, spoil design, drill and blast, or underground.

## Reserves conventions

- JORC Coal Guidelines 2014: tonnes on an in situ moisture and in situ density basis (store air-dried RD,
  convert with Preston–Sanders at reporting). RD is composited by thickness, raw qualities by mass
  (thickness × RD), clean-coal qualities by mass × yield.
- Mining factors in this order: roof and floor loss and gain, then recovery. Seam rules in this order:
  minimum separable parting, minimum mining thickness, minimum product-to-waste ratio, quality limits.
  Loss and dilution material carries its own density and quality. Edge loss is supported.
- Overburden and interburden attach to the next seam below, with explicit rules for masked or missing seams.
- Output one record per pit / strip / block / bench / seam / material with volume, tonnes, qualities,
  areas, dip and centroid. Block names are unique. Coal is reported in tonnes, waste in BCM.

## Exchange formats

- Import first: Vulcan `.00t` triangulations and grids (the formats sites exchange day to day; see
  `docs/decisions/0006-vulcan-grids.md` and `0011-vulcan-triangulations.md`), DXF (3DFACE, polyface and
  strings), CSV strings and XYZ grids; then Surpac `.str`/`.dtm`, Datamine pt/tr ASCII, OBJ.
- Then: OMF v1 and v2, and an export to Seequent Evo schemas (triangle mesh, design geometry).
- Vulcan formats are read, not written. Deswik `.dcf` has no public spec; exchange with Deswik through DXF
  (R12, confirmed reading at the same coordinates) or OMF.

## Reference material

- `docs/decisions/0002-industry-research.md`: how Vulcan, Deswik, Minex, Datamine, Surpac and Micromine
  do face design, ramps, seam models and reserves, with sources. Read it before designing a command.
- Deswik 2025.2 API reference (local only): `D:\Deswik Helper\API Reference\`. Key files are
  `Assemblies\Deswik.Graphics\Deswik.Graphics.PitDesign.md` (rules, ramps, settings),
  `…RampRules.md`, `…Ramps.md`, `…SurfaceStacking.md`, `…CutBlocks.md`, and `Assemblies\Deswik.Tools.md`
  (`StripDesigner`). Use it for parameter sets and error cases. Never copy anything from client process
  maps in that folder into this repo.

## Repo layout

```
crates/zd-core       object types, ids, layers
crates/zd-geom       projection, offsets, triangulation, booleans
crates/zd-commands   typed commands, registry, run log
crates/zd-io         DXF/CSV/OMF import-export, Parquet project store
crates/zd-py         PyO3 bindings (Phase 1)
app/src-tauri        Tauri shell, command bridge, binary transfer
app/src              React: viewport/, panels/, forms/, design-system/
bench/               Phase 0 stress tests + timing harness
testdata/synthetic   generated cases with known answers
testdata/golden      real pit data + Deswik reference volumes (may be git-ignored)
docs/decisions       one short record per locked decision
```

Conventions the CI relies on: the Tauri crate in `app/src-tauri` is named `zd-app` and is a workspace
member; the bench crate is named `bench`; `app/package.json` has `typecheck` and `build` scripts and a
`packageManager` field pinning pnpm.

## Build and test

```powershell
cargo test --workspace            # engine tests
cargo bench -p bench              # Phase 0 timings (once bench exists)
cd app; pnpm install; pnpm tauri dev

# Python (`import zd`), into a project virtual environment (no activation, which PowerShell blocks by default):
py -3.13 -m venv .venv
.venv\Scripts\python -m pip install "maturin>=1.9.4,<2"
$env:VIRTUAL_ENV = "$PWD\.venv"; .venv\Scripts\maturin develop --release -m crates/zd-py/Cargo.toml
.venv\Scripts\python crates/zd-py/tests/test_session.py
```

## How to work

- Start each task with a short plan; wait for the user to agree when the task touches the object model,
  the command interface, or the stack.
- Work on a branch; open a PR; CI must be green before merge.
- **Every engine change ships with tests.** At least one synthetic case with an analytic answer
  (e.g. a cube cut by a plane = known volume; a 45° batter from a flat toe = known offset), plus a
  golden-data case when one exists. Volume tolerances must be stated in the test.
- Report timings for anything geometry-heavy (triangles in, milliseconds, triangles out).
- Never silently "fix" degenerate geometry — log what was cleaned and why.
- Keep functions small and named in mining language (`project_face_profile`, `cut_solid_by_surface`).
- Don't add dependencies outside the locked stack without asking.

## Current phase

**Phase 0 — stress test.** See `docs/decisions/0001-stack.md` and `FIRST_TASK.md`.
Gate: all four checks pass on real data. The viewport bar is tens of millions of triangles: the best
incumbent claim (Datamine MineScape on HOOPS) is at that level, and every incumbent runs OpenGL 3.3–4.6.
