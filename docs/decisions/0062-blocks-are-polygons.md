# 0062 — Blocks are polygons: strips cut by seams, then by block polygons

Status: agreed 13 November 2026 (Tim: "Agree all eight", the Build Plan's "Block cutting plan" tab). Item 7 of the
agreed order, Phase 4 block cutting. Workstreams 1 to 5 built.

## Why

On 13 November a made-up pit was taken from grids to interrogated blocks with the commands the app had
(`testdata/synthetic/reserves_workflow.py`). Every step ran, but cutting by the grids took two runs a seam and left
in-between solids and names growing with each cut; nothing set a block's `seam`; there was no block tool, and blocks made
from vertical faces overwrote the strip number; a seam with no thickness (U running into the weathering) refused the cut
on 2 of 6 strips; the 2.5D column check flagged every block with battered walls; and slivers were left where U thinned out.

Tim then said how he makes blocks: draw a line, offset it every 100 m, draw lines to close it, then make polygons from the
closed areas, as Deswik and Spry do. A block is a named polygon; a solid is cut vertically where it sits and inherits the
block's name. Deswik's block solids rule is the same shape: a layer of block polylines, the solids to cut, an optional
surface stacking rule in the same pass, a block attribute and a minimum volume.

## Decision

All eight as recommended:

1. **Polygons run across the strips**: the offset lines and two closing lines past the pit make one polygon per 100 m
   column across every strip. A piece's strip comes from its solid and its block from the polygon, so only the 100 m ends
   cut and the faces keep shaping the blocks' sides. One polygon per strip still works; its long sides then cut through
   the batter vertically.
2. **A stacked piece carries `seam`, `material` and `horizon`**: `seam` is its horizon for coal and, for waste, the seam
   below it (CLAUDE.md attaches overburden and interburden to the next seam down); `material` is coal or waste; `horizon`
   names the layer of the stack (OB, U, U-M, M and so on).
3. **A slab is made only where it's thicker than 0.01 m.** Thinner is left out and its volume said.
4. **Seams then blocks**, Tim's order. The two cuts don't depend on each other, so either order gives the same blocks.
5. **Names are `{strip}_{block}_{seam}`** by default, editable, checked unique. (Made `{strip}_{block}_{horizon}` with
   workstream 3, so waste isn't named like the coal of its seam: see below.)
6. **The column check takes the block's outline at mid-seam**, its section halfway between roof and floor, instead of its
   whole plan: the polygon Minex or Vulcan would use at the seam, so the batter drops out.
7. **No automatic small-piece limit**, as decided in Phase 1: a select-by-volume filter in the app.
8. **In the app**, By seams and By polygons are modes of the Cut solids panel; polygons from lines goes with the string
   tools.

## Workstreams

| # | What | Where |
|---|---|---|
| 1 | `string.polygons_from_lines` | zd-geom, zd-commands |
| 2 | `solid.cut_by_polygons` | zd-geom, zd-commands |
| 3 | `solid.cut_by_seams` | zd-geom, zd-commands |
| 4 | The column check at mid-seam | zd-geom, zd-commands |
| 5 | The app: two Cut solids modes, polygons from lines, select by volume | app |
| 6 | The made-up pit redone, then PIT B by counts and timings only | testdata, Python |

### Workstream 1: polygons from lines

`string.polygons_from_lines` takes strings, open or closed, and makes a closed string of every area they close in plan
(`zd_geom::polygons_from_lines`).

- **Joined where they cross.** Every segment is cut where another meets it, crossing, touching or lying along it. Points
  within 1 µm in plan are one point.
- **Ends that stop short.** An open line's end within the tolerance (1 mm by default) of another line is joined to it by
  a stretch of its own, and the join is said (`string.end_joined`, with the gap). Nothing is moved.
- **What closes nothing is left out.** Stretches with a free end are taken off end by end until every point is on a ring.
  A free end outside every polygon runs on past the last line it crosses, which is how block lines are drawn: those are
  counted, with the length of line left out. A free end inside a polygon was meant to split it and doesn't: a warning
  (`string.end_short`) gives the line, the place, the gap to the nearest line and the polygon. A line that closes nothing
  at all is named (`string.closes_nothing`). A ring lying inside a polygon without touching it is an island
  (`string.island`): it doesn't split the polygon, and its own polygons overlap it. With no area closed at all the run is
  refused, saying why.
- **The polygons** are the faces of the lines with the area on their left, anticlockwise, each vertex at the height of
  the line it lies on. They're named from a pattern (`Polygon #` by default), numbered west to east by centroid, then
  south to north; a line drawn across them (`string.number_across`) numbers them as blocks.

Checked on made-up lines with answers worked by hand (`crates/zd-geom/tests/polygons.rs`, areas to 0.001 m²): a 2 × 2
grid of 100 m squares closes four of 10,000 m², the same with the lines running 20 m past (12 free ends, 240 m left out);
a line 5 mm short joined at 1 cm and listed at 1 mm, the two squares either side then one of 20,000 m²; a line crossing
nothing named; a closed string split by a line across it; a closing line drawn along another adding nothing; an island;
and Tim's layout, two bent lines of 2,000 vertices each and 15 lines across every 100 m, closing 14 blocks of 36,000 m²
(4,013 segments in, 30 crossings, 6.7 ms in release). Through the registry (`crates/zd-commands/tests/blocks.rs`): named
blocks in one undo, a short end joined or listed, lines closing nothing refused or named.

### Workstream 2: cut by polygons

`solid.cut_by_polygons` cuts each solid by each block polygon's vertical prism (`zd_geom::cut_solid_by_polygons`): the
polygon in plan, either way round, extruded past the solid's top and bottom, one Manifold boolean each about a
whole-metre origin near the solid. Only polygons whose plan box meets the solid's are cut with.

- **A block keeps its solid's attributes** (`strip` stays) and takes the polygon's name as `block` (the attribute can be
  named). The polygon's other attributes come with it where the solid hasn't one of the same name: the solid's own win.
- **Names** come from a pattern of attributes in braces, `{strip}_{block}_{seam}` by default, with `{solid}` and
  `{polygon}` for the names. One a block hasn't is left out with the separator before it, so a block with no seam yet is
  `5_B07`. Names repeated across the run are said (`block.name_repeated`), with how many share them.
- **The polygons must cover every solid in plan** (Tim, 13 Nov 2026): a block polygon set that doesn't reach over the
  whole of a solid is the polygons to fix, not ground to leave out. Before anything is cut, each solid's footprint in plan
  is taken less the polygons (`zd_geom::polygons_uncovered`), and a solid reaching outside them is refused, every gap
  listed with its area and where it is; nothing is cut. A solid wholly outside every polygon is refused the same way.
  Gaps narrower than 1 cm, where lines drawn on the same place part by rounding, are left out of the check; the volume of
  any such sliver the cut leaves is said, in no block. The Cut solids panel runs the same check
  (`check_cut_by_polygons`, `solid.outside_polygons` warnings) as the solids or polygons change, lists what's uncovered
  with Show, and holds the cut ("Fix the polygons first"). Until then, what lay outside was kept as a remainder on a
  layer of its own (`<layer> outside`); that layer and its parameter are gone.
- **Overlapping polygons are refused**, naming the pair, the overlap's area and where it is: a block there would be
  counted twice. Overlaps under 0.01 m² are a shared edge drawn twice.
- **The blocks add back to the solid**; a difference over a millionth of its volume is said
  (`block.volume_differs`). Open strings are taken as closed and said. Each piece is searched for crossings on its own
  thread, and blocks in more than one part are said by the solid rules' check every run goes through.
- Solids are cut in parallel, with progress and Cancel.

Checked with volumes worked by hand (`crates/zd-geom/tests/blocks.rs`, to 1 part in a million): a 1,500 × 60 × 30 m
strip cut by 15 polygons into blocks of 180,000 m³ that add back exactly; a strip with 65° sides cut by polygons wider
than it, each piece keeping the batter (its trapezoid section × 100 m); the east 500 m outside ten polygons as the
remainder, and found uncovered beforehand as 30,000 m² about its middle; polygons drawn to the toe of a 30 m wall at 65°
leaving the crest's lean uncovered, 300 × 14.0 m each side; polygons a millimetre apart or on the solid's own edge
covering it, a metre apart leaving 60 m², and a solid away from them all uncovered whole; a square turned 30° taking its
own area; a clockwise polygon, a U-shaped polygon making one piece of two
parts, a far polygon left out; overlaps found and shared edges not. Through the registry
(`crates/zd-commands/tests/blocks.rs`): polygons from lines then 15 named blocks in one undo; the solid's attributes
winning over the polygon's; a strip reaching 500 m past the polygons and a strip away from them all, found by the check
on their solids and refused by the cut, nothing made, then cut when the polygons cover it; repeated names said and told
apart by the pattern; overlapping polygons refused.

Timing on the made-up pit (`reserves_workflow.py`): its 32 strip-level seam and waste solids with 65° and 45° faces,
92,510 triangles in, cut by 15 polygons into 471 blocks (99,558 triangles out) in 107 ms, adding back to within 1e-13.
Their names repeat until the solids carry `seam` (workstream 3).

### Workstream 3: cut by seams

`solid.cut_by_seams` cuts each solid by a seam model's working section in one run (`zd_geom::layer_stack`,
`zd_geom::cut_solid_by_layers`). The roofs and floors are read where the model links them, checked by fingerprint, and
blanked where the model marks a seam absent; nothing needs importing as an object.

- **The layers**, top down: the overburden above the first roof, each seam, each interburden between a floor and the next
  roof, and what's below the last floor.
- **Each layer is a slab between its two grids**, made where it's thicker than the minimum (0.01 m, decision 3). Inside each
  grid triangle the thickness is a plane, so the slab is clipped exactly where it passes the minimum, with a wall that
  high there. A seam with no thickness makes no slab, so nothing coincides and nothing is refused. A solid's piece of a
  layer is one Manifold intersection with its slab, over the cells under the solid only. What the slabs leave out is said
  by its volume (`seam.thin_left_out`).
- **A seam with no height at a node** (null, or at the model's no-seam RL) is laid on the floor of the seam above there,
  so the waste between goes with the seam below it; with no seam above, on the roof of the one below, so the overburden
  reaches down to it. A surface rising above the one over it, within the model's tolerance (crossings beyond it block the
  model), is lowered onto it, and both are counted.
- **A piece keeps its solid's attributes** and carries `horizon` (OB, a seam's code, `U-M`, `below L`), `material` (the
  horizon's for a seam, `waste` otherwise) and `seam`: its own for coal, the seam below it for waste, none below the last
  floor (decision 2). Named `{solid} {horizon}` by default.
- **What lies off the grids** in plan is kept whole on its own layer and said (`solid.off_grids`). The pieces, what's off
  the grids and what's too thin add back to the solid; a difference over a millionth is said (`seam.cut_differs`).
- Solids are cut in parallel, with progress and Cancel.

**Names, changed from decision 5.** Waste carries the seam below it, so a block's overburden and its first seam's coal both
had `seam` U, and `{strip}_{block}_{seam}` named them alike. The default for `solid.cut_by_polygons` is now
`{strip}_{block}_{horizon}`: coal names are as agreed (a seam's horizon is its code), and waste is told apart (`5_B07_OB`,
`5_B07_U-M`). Tim agreed on 13 November ("Keep it").

Checked with volumes worked by hand (`crates/zd-geom/tests/layers.rs`, to 0.001 m³): three flat seams cut a strip into its
seven layers exactly; a 65° battered strip keeping its batter in every layer; a seam thinning to nothing across one cell
and lying on its floor beyond, its coal ending where it's 0.01 m thick and the 0.015 m³ wedge past it left out and said; a
seam absent from x = 300 m, its waste going with the seam below; a top seam absent, the overburden reaching the next; a
strip reaching off the grids; a strip standing on the last floor, nothing below it; surfaces crossing by 1 mm lowered and
counted. Through the registry (`crates/zd-commands/tests/cut_by_seams.rs`): five named layers with their attributes in one
undo; a seam running out; off the grids; seams then blocks, every name unique and the coal interrogated by its `seam`.

Timing on the made-up pit (`reserves_workflow.py`): its 7 strip solids, 41,178 triangles, cut by three seams into 42 pieces
(21 coal, 21 waste; 123,768 triangles in the slabs, 128,612 out) in 319 ms, adding back to within 3e-13, with 7.975 m³
thinner than 0.01 m left out where U runs out under the weathering. Nothing refused: cutting at each roof and floor in turn
had refused 2 of 6 strips there. Then cut by 15 block polygons into 616 blocks in 129 ms, no name repeated.

### Workstream 4: the column check at mid-seam

`solid.interrogate`'s 2.5D column check (decision 0061, workstream 3) now takes each block's plan at mid-seam
(`zd_geom::mid_seam_cells`): where the surface halfway between its seam's roof and floor lies inside it, by exact partial
cells, the polygon Minex or Vulcan would take for it at the seam. Its column is the seam's thickness at each node times
that plan in the node's cell. A battered wall widens a block's whole plan but not its section at mid-seam, so it drops out.

- **How:** the block is cut at the mid-seam surface (the roof and floor grids averaged, triangulated over the cells under
  it) into the part below and the part above, one Manifold intersection each. A face of the block the cut doesn't touch
  looks up from one part or the other, and the section is the top of the part below, so in each cell the section's plan
  is the two parts' upward plan less the block's.
- Where the roof or floor is null the mid-seam surface is taken below the block, so there's no section there, as there's no
  seam. A block whose plan at mid-seam can't be worked out takes its whole plan, as before, and is named
  (`interrogate.column_whole_plan`).
- The column volume written onto each block (`column_volume`) is this one.

Checked with volumes worked by hand (`crates/zd-geom/tests/reserves.rs`, to 1e-6 m³): a block with a 65° east side through
a 4 m seam, its whole plan 4 cot 65° wider than at its floor, its section at mid-seam 2 cot 65° wider, its column its own
volume on 25 m and 5 m cells, where its whole plan gave 100 × 4 × 4 cot 65° m³ more; a strip between two parallel 65° faces,
its column its own 24,000 m³; a seam dipping 1 in 10, the section on the dipping mid-seam; a block short of mid-seam having
none, and a null roof having none. Through the registry: a battered block passes the check, its column written.

On the made-up pit (`reserves_workflow.py`, after cutting by seams and then by 15 polygons): 269 of 301 coal blocks are
within 0.1 % of their column and the pit is +0.002 % in all, where the whole plan had every block over (U +1.6 %, M +2.4 %,
L +4.9 %). Every L and M block is within it (L +0.001 %, M 0.000 %). The 32 outside are U blocks along its subcrop, where U
thins to nothing inside a 25 m cell and the column holds each node's thickness over its whole cell, as Vulcan and Minex do;
most are under 1,500 m³. Interrogating them took 59 ms. The 43,000 made-up blocks (ignored test) interrogate in 4.4 s on
every core, against 4.1 s before.

### Workstream 5: the app

- **Cut solids panel**, two more modes (decision 8):
  - **By seams.** The solids taken as in every mode (picked, their layer, or on show), the seam model (the last built
    unless chosen), the thinnest layer made (0.01 m) and the names (`{solid} {horizon}`). It runs `solid.cut_by_seams`.
  - **By polygons.** The block polygons are the closed strings on a layer, the layer with the most of them offered first.
    The names default to `{strip}_{block}_{horizon}`. It runs `solid.cut_by_polygons`.
- **Polygons from lines**, with the string tools: select the lines (the toe's offsets across the strips and the lines
  closing them) and Polygons in Properties, beside Number across. It gives the name (`B#`), the layer
  (`block polygons`) and the tolerance in mm, and the polygons made are selected.
- **Select by volume** (decision 7): with several selected, Solids, Under *n* m³, Keep those selected narrows the selection to
  the solids holding less than that. Select a layer's solids from the tree first (Select all on it); the selection's own
  Delete then takes them out, in one undo. The app asks the engine for the volumes (`solids_under`), as it holds none itself.

Checked in the release app with `--bench --cut --seam testdata/synthetic/reserves/stack.csv`, through the panel's own
buttons. A 1,000 × 500 m pit between two surfaces, cut by the made-up pit's seam model with By seams, gave 7 pieces named
`PIT OB` to `PIT below L` in 120 ms, adding back to within 1.5e-16. Lines across it every 100 m, closed into 10 polygons
and offered by the panel, cut the 7 pieces into 69 blocks with no name repeated (`B1_OB`…) and 7 remainders past the
polygons in 33 ms. Those polygons stopped 50 m short of the pit's east end; with the polygons required to cover it
(13 Nov 2026), the lines run from 50 m before the pit to 50 m past it: the panel's check passes, and 11 polygons cut the
7 pieces into 76 blocks adding back to the 50,000,000 m³ pit within 1.1e-15, in 34 ms. The volume filter found the 2 blocks under 1,000 m³. The Polygons and Under rows in Properties weren't
driven by the bench.

## Still to come

Workstream 6: the made-up pit redone with these commands, then PIT B by counts and timings only.
