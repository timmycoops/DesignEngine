# CLAUDE.md — Zovlyn Pit Design Engine

Read this first in every session. Decisions here are locked; do not re-argue them unless the user asks.

## What this is

A fast, modern 3D design and coal reserves tool for open-pit coal mines: highwall, endwall and lowwall face
projection, ramps, surfaces, strip and block layout, block solids, reserves interrogation and medium to long-term
scheduling with a coal blender. It works alongside Deswik and Spry and replaces neither; short-term scheduling stays in
Spry. Owner: Tim (Zovlyn Pty Ltd), an experienced
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
| Wall offsets | Straight skeleton on wall planes in `zd-geom` (each segment's plane set by its batter, hips where planes meet); a berm is the conditioned crest moved out and registered back on the grid; no offset-then-repair (`docs/decisions/0023-wall-step.md`) |
| Planar offsets | Clipper2 or Manifold `CrossSection` for plain 2D offsets |
| Licences | A `.zdlic` file signed with Ed25519 (`ed25519-dalek`), checked on the machine by `zd-licence`; the engine knows only each command's module (`docs/decisions/0098-licences.md`) |
| Display detail | meshoptimizer via the `meshopt` crate, for the coarser versions the view draws far off; never for data (`docs/decisions/0097-fewer-triangles-drawn.md`) |
| Viewport | three.js with `WebGPURenderer` (auto WebGL2 fallback) |
| Interface | React + TypeScript, shadcn/ui, Tailwind, dockview, TanStack Table, React Flow (`@xyflow/react`) for node graphs (`docs/decisions/0067-workflow-canvas.md`), `cmdk` for Find a command (`docs/decisions/0076-docks-and-workflow-tabs.md`), CodeMirror 6 for the formula editor (`docs/decisions/0077-formulas-in-starlark.md`) |
| Data at rest | One `.zdp` file a project: a zip of Parquet/Arrow tables with a JSON manifest, meshes in the OMF2 layout (vertices `x, y, z` f64; triangles `a, b, c` u32, counter-clockwise about an outward normal), saved beside then swapped in (`docs/decisions/0058-one-file-a-project.md`); geology (seam grids, block models) referenced by location, never packed in; DXF/CSV/OMF only for exchange. Evaluate the `omf` Rust crate before writing our own |
| Python | PyO3 bindings in `crates/zd-py` from Phase 1, built with maturin (reconciliation and test scripts run from Python) |
| Package manager (JS) | pnpm |
| Zips | `zip` crate, deflate only (`docs/decisions/0019-solids-by-the-thousand.md`) |
| Deswik XML | `quick-xml` and `base64` (`docs/decisions/0036-deswik-drawings.md`) |
| Formulas | Starlark (`starlark` crate 0.14.2, patched in `vendor/starlark` to leave serde_json's `arbitrary_precision` off) in `zd-script`: the arithmetic and tests a site edits, kept in the dataflow; what moves material between units stays built in (`docs/decisions/0077-formulas-in-starlark.md`) |
| Scheduling | `zd-sched`: HiGHS through the `highs` crate for the blender's linear programmes, `rayon` for cases in parallel, `rust_xlsxwriter` for Excel output, `chrono` direct, Apache ECharts for charts and Gantts, each added when its workstream lands (`docs/decisions/0100-scheduler.md`) |

## Engine rules (non-negotiable)

1. **Engine crates contain no UI code** and never depend on `app/`.
2. **f64 everywhere in the engine.** Mine grid coordinates (e.g. MGA eastings/northings) are stored exactly.
   Only the viewport subtracts a local origin before sending f32 data to the GPU.
3. **Every operation is a typed command** in `zd-commands`: named inputs, parameters, outputs, and a
   serialisable record of the run.
4. **In place or as new, one rule for every tool** (`docs/decisions/0091-in-place-or-as-new.md`). A tool that changes
   an object writes its next revision (undo puts it back), or, run with `result` as new, a copy on a layer with the
   original left as it was; the session does this for every command. A tool that makes something makes new objects on
   a named layer. No live dependency graph, no automatic rebuilds — a new design means the user re-runs the command.
5. **Cuts are solid booleans.** To cut a solid by an open surface, close the surface into a half-space
   just beyond the solid's bounding box, then run a Manifold boolean.
6. **Projection targets are triangulations.** Grids convert to triangulations on import; one projection path.
   Handle convoluted surfaces at low angles, and fall back where the projection never hits the surface: a gap is
   bridged and reported (`docs/decisions/0024-face-design.md`).
7. **Large meshes cross the Tauri bridge as binary** (typed arrays), never JSON.
8. **Validity is explicit.** Every mesh carries computed flags: `Closed`, `Manifold`, `ConsistentWinding`,
   `NonDegenerate`, `NonSelfIntersecting` (Seequent Evo vocabulary). Imports pass through a repair step first
   (weld by tolerance, drop empty and duplicate faces, orient by flood-fill, find crossings) and log every fix.
   A solid is welded only as far as it needs to close, corners as written first, up to the tolerance
   (`docs/decisions/0052-weld-a-solid-only-as-far-as-it-needs.md`).
   Nothing invalid reaches Manifold or a volume report.
9. **Solids are the volume of record.** Qualities and thicknesses are sampled from seam grids inside each solid.
   A 2.5D column calculation (vertical-walled polygons, exact partial cells) runs alongside as a reconciliation
   check against Vulcan and Minex. Report any difference beyond tolerance; never hide it.

## Object model

One line a type here. The detail is in `.claude/rules/`, which loads when Claude reads that area's code; read the
rule file named before planning work on a type, even when no code has been read yet.

- `Polyline` (string) and `Surface`: ordered `[f64; 3]` vertices with a closed flag and attributes, a strip's number
  its `strip` attribute; a triangulated mesh (vertices f64, triangles u32). `.claude/rules/strings.md`.
- `Solid`, `Grid` and `SeamModel`: a closed manifold mesh, always watertight; a regular raster with NaN for nulls; an
  explicit, ordered stack of horizons from a CSV the user edits, linked to its grids in place, checked and never
  fixed or filled. `.claude/rules/solids-and-seams.md`.
- `Rule`, `FaceProfile`, `FaceDesign` and `Ramp`: a value per edge of a string; one table of steps going one way from
  a toe; the saved design the Face design panel edits; a named road set by a string across the strips.
  `.claude/rules/faces-and-ramps.md`.
- `ParameterTable`, `Dataflow` and `Table`: assumptions read top down; a reserves dataflow's fields and steps; results
  a case, as Parquet in the `.zdp`. `.claude/rules/reserves.md`.
- Every object has a stable id (128-bit, generated in-house), a name, a layer, and provenance (the command
  run that made it). Strings, surfaces, solids and grids are moved, rotated in plan or mirrored in plan whole by
  `object.transform`, a grid only moved, and copied so by `object.copy`, each copy on its original's layer named for it
  (`docs/decisions/0083-cut-strings-by-strings.md`, workstream 3). Edits write a new revision of the same object; earlier revisions stay for undo
  (see `docs/decisions/0004-phase1-foundations.md`). A project is one `<name>.zdp` file; older `<name>.zdproj/` folders still open and save as a `.zdp` beside them (`docs/decisions/0058-one-file-a-project.md`).
- Hiding an object and how it looks are display state saved with the project, not revisions. `.claude/rules/app-ui.md`.

## Scope limits for v1

- Seam dip below 30° and normal faults only, with fault blocks as separate domains. Reverse faults,
  thrusts and overturned seams wait for v2 (the incumbents switch methods there too).
- Scheduling is medium to long term only (months to years, `docs/decisions/0100-scheduler.md`): short-term
  operational scheduling, detailed haul networks and dumps stay in Spry.
- No spoil design, drill and blast design, or underground.

## Reserves conventions

The full conventions are in `.claude/rules/reserves.md`. In short:

- JORC Coal Guidelines 2014: tonnes on an in situ moisture and in situ density basis (store air-dried RD,
  convert with Preston–Sanders at reporting). RD is composited by thickness, raw qualities by mass
  (thickness × RD), clean-coal qualities by mass × yield.
- Mining factors: roof and floor loss and gain, then recovery; seam rules in Spry's order
  (`docs/decisions/0073-working-sections.md`).
- Coal is reported in tonnes, waste in BCM. Block names are unique.

## Exchange formats

Vulcan formats, Deswik `.dcf` drawings and Spry `.spry` projects are read, never written; exchange back is through DXF
(R12), OMF and zips or folders of solids. The full list is in `.claude/rules/exchange.md`.

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
crates/zd-geom       projection, offsets, triangulation, booleans, section lines
crates/zd-commands   typed commands, registry, run log
crates/zd-io         DXF/CSV/OMF import-export, zips and folders of solids, Parquet project store
crates/zd-reserves   reserves dataflow: block columns, units, labels, splits, fields that add up (0070); benches joined by in situ share, put back at writes (0071)
crates/zd-script     formulas a planner edits, in Starlark: checked on save, run on units, traced line by line (0077)
crates/zd-py         PyO3 bindings (Phase 1)
crates/zd-licence    licence files: signed, checked on the machine, the modules they turn on; `zdlic` issues them (0098)
app/src-tauri        Tauri shell, command bridge, binary transfer
app/src              React: viewport/, panels/, forms/, design-system/
bench/               Phase 0 stress tests + timing harness
testdata/synthetic   generated cases with known answers
testdata/golden      real pit data + Deswik reference volumes (may be git-ignored)
docs/decisions       one short record per locked decision
.claude/rules        the detail for each area, loaded with that area's code (see Object model)
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

## Interface

Docks round the view, every tool listed once in `app/src/tools.ts`, every tool panel made from the kit in
`app/src/design-system/kit/`. The full rules are in `.claude/rules/app-ui.md`: read it before planning any UI change.

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
- CLAUDE.md gets a line only when it changes how every session works. Anything about one area goes in that area's
  file in `.claude/rules/`, and the detail in its decision record.
- Client data stays local: nothing under `testdata/golden/` or from the Deswik client process maps goes into a commit,
  a PR, an external service or a printed run. Runs on golden data print counts and timings only.

## Current phase

**Between phases: Phase 3 is closed, and what comes next is not yet agreed.** The proposed order is on the
"Review 10 Nov" tab of the Build Plan doc; ask Tim before starting any of it. PIT A's seam grids are a load test
only: judge seam work on PIT B, which holds the only real coal seam models. The scheduler (0100) is agreed but not
started: stage A waits for the Phase 4 gate, stage B for the Phase 5 gate with Tables ws3 and ws5.

The Phase 3 gate (walls and ramps) was closed as it stands on 2026-11-10
(`docs/decisions/0053-phase3-closed-as-it-stands.md`), with roads on grids, the measured match with Deswik, the merge
with the topography and the surface tools carried forward by name. Earlier gates: Phase 0 on 2026-10-30 (59.5 fps at
25 M real triangles, `bench/RESULTS.md`), Phases 1 and 2 on 2026-10-31 (`docs/decisions/0022-section-view.md`). The
viewport bar is tens of millions of triangles: the best incumbent claim (Datamine MineScape on HOOPS) is at that
level, and every incumbent runs OpenGL 3.3–4.6.
