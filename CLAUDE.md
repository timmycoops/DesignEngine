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
| Wall offsets | Straight skeleton on wall planes in `zd-geom` (each segment's plane set by its batter, hips where planes meet); a berm is the conditioned crest moved out and registered back on the grid; no offset-then-repair (`docs/decisions/0023-wall-step.md`) |
| Planar offsets | Clipper2 or Manifold `CrossSection` for plain 2D offsets |
| Viewport | three.js with `WebGPURenderer` (auto WebGL2 fallback) |
| Interface | React + TypeScript, shadcn/ui, Tailwind, dockview, TanStack Table, React Flow (`@xyflow/react`) for node graphs (`docs/decisions/0067-workflow-canvas.md`), `cmdk` for Find a command (`docs/decisions/0076-docks-and-workflow-tabs.md`), CodeMirror 6 for the formula editor (`docs/decisions/0077-formulas-in-starlark.md`) |
| Data at rest | One `.zdp` file a project: a zip of Parquet/Arrow tables with a JSON manifest, meshes in the OMF2 layout (vertices `x, y, z` f64; triangles `a, b, c` u32, counter-clockwise about an outward normal), saved beside then swapped in (`docs/decisions/0058-one-file-a-project.md`); geology (seam grids, block models) referenced by location, never packed in; DXF/CSV/OMF only for exchange. Evaluate the `omf` Rust crate before writing our own |
| Python | PyO3 bindings in `crates/zd-py` from Phase 1, built with maturin (reconciliation and test scripts run from Python) |
| Package manager (JS) | pnpm |
| Zips | `zip` crate, deflate only (`docs/decisions/0019-solids-by-the-thousand.md`) |
| Deswik XML | `quick-xml` and `base64` (`docs/decisions/0036-deswik-drawings.md`) |
| Formulas | Starlark (`starlark` crate 0.14.2, patched in `vendor/starlark` to leave serde_json's `arbitrary_precision` off) in `zd-script`: the arithmetic and tests a site edits, kept in the dataflow; what moves material between units stays built in (`docs/decisions/0077-formulas-in-starlark.md`) |

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

- `Polyline` (string): ordered `[f64; 3]` vertices, closed flag, attributes. A strip's number is its `strip` attribute, set
  by a line drawn across the strings, and what a run makes keeps it (`docs/decisions/0029-strip-numbers.md`). A string
  is projected at a batter by `string.project`: up or down, on the side the new string comes out on, to an RL, a height
  above or below it, a grid or a surface, every string picked in one run, each as it's drawn and left as it is, with
  the wall kept as a surface if asked; to an RL or a surface it's a face's one wall, by a height its plan offset lifted
  (`docs/decisions/0079-project-ramps-by-hand-and-measure.md`).
- `Surface`: triangulated mesh (vertices f64, triangles u32), name, layer. One is made between strings by
  `surface.between_strings`: a road between its two edges, or one sheet across several put in order across (a fill's
  toe, the road's edges, a cut's crest), every corner a string's vertex, so a ramp can be made by hand from polylines:
  Set grade, Offset, Project and a surface between, all on the Walls row (0079).
- `Solid`: closed manifold mesh; must always be watertight. The large one strips are cut from is made inside a closed
  string between two surfaces or grids (`solid.between_surfaces`), and every cut is one Cut solids panel: into strips by
  faces, by a surface, grid or RL, or between surfaces (`docs/decisions/0049-strip-solids.md`). A block is a named
  polygon in plan, made from lines (lines across the strips offset every 100 m and lines closing them,
  `string.polygons_from_lines`), and a solid is cut vertically where it sits, the piece taking its name as `block`; the
  polygons must cover every solid in plan, checked before the cut and refused with each gap listed, the polygons being
  what to fix; strips
  are cut by the seam model's horizons in one run, each piece carrying `seam`, `material` and `horizon`
  (`docs/decisions/0062-blocks-are-polygons.md`); the grids must reach over every solid in plan with no hole under it,
  checked before the cut and refused with each place listed (`docs/decisions/0063-seam-grids-are-never-filled.md`);
  flaps thinner than 1 cm (where a strip's surface runs along a seam's roof or floor) are taken off each piece and said,
  as Deswik removes flaps (`docs/decisions/0064-berms-on-their-grid-and-flaps.md`). After the seams and before the
  blocks, pieces are cut at bench levels by bench rules (`solid.cut_by_benches`): a table read top down, the first rule a
  piece's attributes fit saying whether it's benched and from what RL to what RL every so many metres, each bench taking
  `bench`, its floor RL by default or its top, onto the rule's own layer or the run's; a piece no rule benches is carried over whole
  (`docs/decisions/0071-benches.md`).
- `Grid`: regular raster (origin, cell size, rows, cols, z values with NaN for nulls). A Vulcan roof, floor or
  topography reads whole: a node flagged off keeps the height it holds, with a warning if it steps far from the
  live nodes (`docs/decisions/0025-a-grid-stays-in-space.md`).
- `SeamModel`: an explicit, ordered stack of horizons (the Deswik "surface stacking" pattern), defined in a CSV
  the user edits; `seam.suggest_definition` writes a starting one from the grids' geometry, never from names.
  Each horizon has a code, material, parent horizon (for splits), working flag, roof and floor grids, quality
  grids and a density grid. The model links to its grid files in place (fingerprinted, never copied into the
  project); a project keeps each grid folder as given and relative to itself, opening tries both, and `seam.relink` points a
  folder at its new place in one step (`docs/decisions/0058-one-file-a-project.md`); grids used for design are imported as objects. A seam exists where its roof is above its floor,
  whatever a file flags, and its surfaces carry on where it has no thickness. The stack is checked, never fixed: a floor above
  its roof, a crossing in the working section or a daughter outside its parent is flagged with its location,
  and blocks solids and reserves until the source grids are corrected. Grids are never filled: a node with no height the
  no-seam RL doesn't account for is a hole, said on the model, and the geology model is fixed and the grids imported again
  (`docs/decisions/0063-seam-grids-are-never-filled.md`). A definition may name an RL that means no
  seam, for models that mark absence that way (`docs/decisions/0020-no-seam-rl.md`). Existence polygons and a crop surface
  (topography or base of weathering) come later. Faulting applies to structure, never to thickness.
  See `docs/decisions/0007-seam-model.md`.
- `ParameterTable`: assumptions any tool reads (reserves settings now; dragline, haulage, equipment later): filter
  columns (the bench rules' syntax) and value columns with units, read top down, each value from the first row that fits
  and holds it, tables read in order so a case's rows go first; CSV in and out; edited as grids, with the dataflow, in the Parameters tab, a picked solid's
  values traced to their row (`docs/decisions/0075-reserves-in-the-app.md`). `Dataflow`: a reserves dataflow's fields
  (with where each value comes from), steps by name and the parameter tables it reads; a step may be a formula step, Starlark run on the units its filter fits after `initialise`, `join_benches`, `working_sections`, `rom_dilution` or `rom`, checked and its known answers run when saved (`docs/decisions/0077-formulas-in-starlark.md`). `Table`: results, a table a case,
  values as Parquet in the `.zdp`, read by the HTML report and the app's Reserves table tab, fields as rows and a
  node's children across, its solids matched by the levels the table records (`docs/decisions/0075-reserves-in-the-app.md`). `reserves.run` runs a dataflow a column a pit, strip
  and block (the dataflow's levels above the seam), its steps registered by name, and writes a reserves table: a row a
  leaf of the levels, a column a field under its folders (`Insitu\Coal\volume`), coal and waste on one leaf as Spry's
  node (`docs/decisions/0072-dataflow-parameters-tables.md`, `0074-rom-and-the-reserves-table.md`).
- `Rule`: a function from a string to one value per edge. Types: fixed, fixed list, inside polygon,
  inside solid, segment azimuth, by horizon. Later: block model, geotech solid with transition distance.
  Blending between rules may only widen berms and flatten slopes: where a value changes along a straight run, a
  transition bevels the crest across the steeper or narrower side (`docs/decisions/0027-rules.md`).
- `FaceProfile`: one table for one type of string, going one way from its toe, up or down: the grid, surface or RL
  the toe registers to, then ordered steps, each "project to surface B or an RL at a batter" or "berm of a width", with the
  batter or width a number or a rule, and each row saying how its line registers and conditions. A string going both
  ways takes a profile for each, and the top or bottom string is made by hand after the run. Still to come: berm on
  roof and berm on floor per horizon, minimum and maximum bench height, wall type (face, side, end). Side (left or
  right for open highwall strings) is on the line. A face is built oversized, past the pit shell, and the cut does
  the fitting (`docs/decisions/0023-wall-step.md`). A face lies on the grids it's built to: a berm on a grid is split
  along the grid's triangles, not a chord across its cells, and the seams are stacked from the same grids
  (`docs/decisions/0064-berms-on-their-grid-and-flaps.md`). `FaceDesign` is the saved object the Face design panel edits:
  lines, profiles and any shared rules together (`docs/decisions/0024-face-design.md`). A face built by hand is a step
  design: a face design on one string that saves and runs whole as each step is added (`face.step`, one undo a step),
  with ramps placed by clicking. Faces are built from strings, never by moving a surface's points
  (`docs/decisions/0045-step-design.md`). A line a row ends on can be redrawn by hand, kept on the design's line and not
  in the profile: the row re-forms to meet it and the rows above carry on from it, roads and all
  (`docs/decisions/0046-edit-the-line.md`). Join on a berm makes the switchbacks turning on it, on box pads facing each other,
  one pad, the line running straight from the back of one box to the back of the next; left to itself a berm joins only
  boxes within a road's width of each other or overlapping (`docs/decisions/0047-join-switchbacks.md`). The manual
  builder, replacing the step design's hand edits, builds one wall or berm from the line you pick, run once
  (`face.build_step`): its outputs are ordinary strings, roads carry on from the line, turns and Join go on the berm, and a
  wall from a berm's line a step made builds that berm with it from its crest, standing on your line if you've edited it.
  Each line keeps how it was built, and Rebuild from here (`face.rebuild`) builds the steps above an edited line again
  from it, each with its settings, as next revisions of what they made, in one undo. The Builder panel builds the next
  step from the picked line, or rebuilds when steps are built on it, and lists the build's steps, a click on one going
  to its line (`docs/decisions/0048-build-a-step.md`). A face is told apart by its strip number: `face.join` makes one
  face a strip from up and down faces, several runs or a build's steps, welded, stitched and gaps filled, every change
  said; `solid.slice_by_faces` closes each face behind it past the solid, its triangles facing the pit, and slices the
  solid from the pit out, each strip cut from what the strips before it left so they add up to the solid; a face that
  doesn't pass right through the solid (past both ends, above its top, below its floor; one stopping on them doesn't) is
  refused, every shortfall listed, and the Cut solids panel checks before the cut; carrying faces on past the solid is
  the designer's, not automated (`docs/decisions/0049-strip-solids.md`).
- `Ramp`: centreline or start point, direction, start and end bench, width, grade (%, ratio or degrees,
  measured on the inside edge by default, which is the pit side), inside and outside taper, entry type, flat or
  graded switchbacks with radius. It's a named ramp in a `FaceProfile`, set by a string drawn across the strips: each line
  the string crosses gets a road, starting where it crosses the toe of its first wall, or with its end held, coming out
  on its last crest level with that place along the strip, with the run moving the start along the toe to get there
  (`docs/decisions/0031-ramps-on-real-strips.md`). Wall rows name the ramps
  that climb them, and it's cut back as they're projected, so running the design again rebuilds it. A wall can hold several
  roads running the same way, lowest first, each cut into the wall above the one below (`docs/decisions/0032-roads-on-one-wall.md`),
  and roads running opposite ways side by side, as a straight ramp beside a switchback, each cut its own way in its own part of
  the wall, never within a road's width of each other (`docs/decisions/0042-roads-both-ways.md`).
  The toe behind its start moves back by the road for an entrance. Each ramp goes past a berm after it, by default, crossing
  the full berm level to climb on from the next toe, or through it at its grade, the berm narrowing to nothing at the road
  (`docs/decisions/0035-ramps-after-the-berm.md`). After the berm it crosses straight across, in line with its top, as
  Deswik's does; a road wider than the berm starts its next stretch on the top of the last. A wall row can turn a ramp
  back before it: a switchback on a pad of the two roads and the berm between, level on an RL and on the seam floor on a
  grid (no coal left under it), on straight or curved walls. The road coming up ends at its top and turns on a flat box past it,
  in line with it and on the floor on a grid (its width long unless the ramp says otherwise): the road, the berm and the next
  road wide, the next road climbing the other way from its near end, straight across the berm from the top, and the wall above
  going round the box, its far end one straight wall square across, the berm standing out into the wall from the crest
  beside and past the box, never round its end; the ramp's end angle (90° by default) flares that wall out past the box
  from its back corner to the berm's line past it. With no box the toe past the turn joins back a length on set on the ramp
  (50 m by default), whatever vertices the string has; one longer than the line runs to the string's end
  (`docs/decisions/0038-switchbacks.md`). On a profile going down a road comes down from the top line, notching it
  for its entrance, and the walls below it move out towards the pit (`docs/decisions/0037-roads-going-down.md`). A crest
  records each road that came out on it as `road_top` attributes, and a ramp set to carry on from the line takes it on
  from there in a design run from that crest, with no entrance; a crest a run made keeps its heights as a line, so two
  designs give one design's face (`docs/decisions/0041-roads-carry-on.md`). It reports
  the bench where the grade can't be met instead of drifting (`docs/decisions/0028-ramps.md`, `0030-ramp-strings.md`).
- Every object has a stable id (128-bit, generated in-house), a name, a layer, and provenance (the command
  run that made it). Edits write a new revision of the same object; earlier revisions stay for undo
  (see `docs/decisions/0004-phase1-foundations.md`). A project is one `<name>.zdp` file; older `<name>.zdproj/` folders still open and save as a `.zdp` beside them (`docs/decisions/0058-one-file-a-project.md`).
- An object can be hidden on its own, under its layer's tick. That's display state saved with the project, not a
  revision and not undone. Hidden means not drawn, snapped to or cut by a section; a command that names the object
  still takes it (`docs/decisions/0026-one-layer-tree.md`). So is how it looks: surfaces and grids colour by RL in
  bands on round RLs, in the shader, over each one's own range by default, or in the layer's colour; and an opacity per
  object and per layer, which multiply (`docs/decisions/0043-colour-by-rl-and-opacity.md`).

## Scope limits for v1

- Seam dip below 30° and normal faults only, with fault blocks as separate domains. Reverse faults,
  thrusts and overturned seams wait for v2 (the incumbents switch methods there too).
- No scheduling, spoil design, drill and blast, or underground.

## Reserves conventions

- JORC Coal Guidelines 2014: tonnes on an in situ moisture and in situ density basis (store air-dried RD,
  convert with Preston–Sanders at reporting). RD is composited by thickness, raw qualities by mass
  (thickness × RD), clean-coal qualities by mass × yield.
- Mining factors in this order: roof and floor loss and gain, then recovery. Seam rules in Spry's order (Tim,
  15 November 2026, so PIT B's working sections can be matched): in situ quality limits per ply first (coal set aside
  by its in situ ash), then minimum separable parting, minimum product-to-waste ratio as a section builds, and minimum
  mining thickness (partings in) last; the ROM ash cut-off after dilution (`docs/decisions/0073-working-sections.md`).
  Loss and dilution material carries its own density and quality. Edge loss is supported.
- Overburden and interburden attach to the next seam below, with explicit rules for masked or missing seams.
- Output one record per pit / strip / block / bench / seam / material with volume, tonnes, qualities,
  areas, dip and centroid. Block names are unique. Coal is reported in tonnes, waste in BCM.

## Exchange formats

- Import first: Vulcan `.00t` triangulations and grids (the formats sites exchange day to day; see
  `docs/decisions/0006-vulcan-grids.md` and `0011-vulcan-triangulations.md`), DXF (3DFACE, polyface and
  strings), CSV strings and XYZ grids; then Surpac `.str`/`.dtm`, Datamine pt/tr ASCII, OBJ.
- Solids come by the thousand as a zip or a folder (Spry exports a zip; Deswik imports one): `solid.import_many`
  takes either in one run. See `docs/decisions/0019-solids-by-the-thousand.md`. They go out the same way: `solid.export_many` writes a
  zip or a folder of one DXF a solid, named for it, with a list of them and their attributes
  (`docs/decisions/0057-solids-out-a-file-each.md`).
- Then: OMF v1 and v2, and an export to Seequent Evo schemas (triangle mesh, design geometry).
- Spry `.spry` projects are read, not written: block solids by node path with Spry's values and volume, design-layer
  strings and solids with their attributes (`docs/decisions/0039-spry-projects.md`).
- Vulcan formats are read, not written. Deswik `.dcf` drawings are read, not written: a zip of VectorDraw XML layers,
  read by the layers picked, with projection details kept on strings and Deswik's volumes on meshes
  (`docs/decisions/0036-deswik-drawings.md`). Exchange back into Deswik through DXF (R12, confirmed reading at the same
  coordinates) or OMF. Spry `.spry` projects (SQLite) come next.

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

## Interface

The shell is docks round the view (`docs/decisions/0076-docks-and-workflow-tabs.md`, the Build Plan's "UI improvements
plan" tab): the layer tree on the left, one tool dock on the right where every tool opens as a tab beside Properties,
and one results dock under the view where every table opens as a tab (Run log, Block table, Reserves table, Parameters,
Workflow canvas, Face design). A new panel names its dock in `PANELS` (`app/src/workspace/Workspace.tsx`) and opens
through `showPanel`; it never adds a column or a row of its own, and the view keeps its room. The view has no tab strip,
and its own tools (Select, Draw, Measure, Snap, 3D or Plan, the zooms, the field of view, Section and its step and fill)
are on the view bar over it (`app/src/panels/ViewBar.tsx`): a new view tool goes there, through `ViewTools`, never on the
toolbar, and nothing drawn over the view may sit behind the bar. Measure (M) reads the distance, RL difference, grade and
bearing between clicks from the points' world coordinates, and makes nothing
(`docs/decisions/0079-project-ramps-by-hand-and-measure.md`). A panel never says its name under its tab: its head is
one line of what it does (`PanelHead`), and its first control is within 40 px of the tab. Every tool is listed once in
`app/src/tools.ts`, in its stage, with its name, what it does, and the panel it opens or the action App carries out;
the toolbar, its menus, the Panels menu and the workflow canvas are drawn from the list, so a new tool is added there
(and its action to App's table), never as a button of its own. The ribbon (`app/src/workspace/Ribbon.tsx`) is a tab a
stage (Project · Strings · Walls · Solids · Geology · Reserves) over the row of that stage's tools, named in words;
the rows are `ROWS` in the tool list, a row too long folds into More, the tool in use is lit, and switching stage
changes the row only. A new tool goes on its stage's row there. Find a command (Ctrl+K, `FindCommand.tsx`) searches
the list by whole words, so a tool's name and its line are what it's found by: write them in the words a planner would
type. Properties shows only: an object's name, look, attributes, measures and history. A command on an object is a tool
in a panel on the kit, never a row in Properties (the string tools are the Strings panel). All seven workstreams of 0076
are built.

The UI plan (the Build Plan's "UI plan" tab) keeps the app one way of working as it grows. A tool panel is made from the
kit in `app/src/design-system/kit/`: `ToolPanel` (what it does in one line that fits the 400 px dock, with more in a tip, its sections, one button at the
foot named for what it does, Ctrl+Enter), `PanelHead` for a panel that isn't a tool, `NumberInput` with its unit, `NumberOrBlank` for a number that may be left blank, `TargetInput` for an RL, grid or surface, `Choice` (in `columns` where its options don't fit one row), `Field`, the badges,
`RunResult`, and `ToolButton` for a button on a bar of tools. Colours come from the theme's tokens, one meaning each: gold picked, orange `warning`, blue `edited`,
red refused, green valid. Don't hand-roll an input, select, badge or colour a kit part covers; add to the kit instead.

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

**Between phases: Phase 3 is closed, and what comes next is not yet agreed.** The proposed order is on the
"Review 10 Nov" tab of the Build Plan doc; ask Tim before starting any of it. PIT A's seam grids are a load test
only: judge seam work on PIT B, which holds the only real coal seam models.

The Phase 3 gate (walls and ramps) was closed as it stands on 2026-11-10
(`docs/decisions/0053-phase3-closed-as-it-stands.md`): three checks passed, four in part and one not measured.
Carried forward by name: roads on grids (a known limit, 0034), the measured match with Deswik (waits on a surface
compare and a clean real wall), the merge with the topography, and the surface tools and surfaces in section that
weren't built.

The Phase 2 gate passed on 2026-10-31: a two-point section with an overview, RL grid, fills and string editing
(`docs/decisions/0022-section-view.md`, `bench/RESULTS.md`), passed by Tim on look and feel without a side-by-side
in Deswik. The Phase 1 gate passed on 2026-10-31: a toe string digitised and edited on real surfaces, with every
check on the "Phase 1 plan" tab of the Build Plan doc ticked. The Phase 0 gate passed on real data on
2026-10-30 (`bench/RESULTS.md`): all four checks, with 59.5 fps at 25 M real triangles. The viewport bar is tens of millions of triangles: the best incumbent claim
(Datamine MineScape on HOOPS) is at that level, and every incumbent runs OpenGL 3.3–4.6.
