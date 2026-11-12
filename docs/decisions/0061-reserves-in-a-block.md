# 0061 — Interrogating a block: a thin reserves slice against the seam model

Status: agreed 12 November 2026 (Tim: "Agree all seven", the Build Plan's "Reserves slice plan" tab), changed the same day
as below. All six workstreams.

## Why

Reserves had nothing underneath them (the review of 10 November, finding 5): a seam model linked its quality and
density grids but nothing read them, nothing sampled inside a solid, and the 2.5D column check existed only for whole
seams. Item 6 of the agreed order runs a thin slice beside Phase 4's block cutting, so reconciliation with Spry starts
early, on real blocks: block solids already in a project and a seam model, each block's geology interrogated from the
model's grids, its volume checked against a 2.5D column calculation, and set beside Spry's own values by name.

## Decision

Agreed with the plan, all as recommended:

1. **The records are a table object in the project**: changed the same day (below), the values go on the solids. One row per block, saved in the `.zdp` as its own
   table, with CSV out (workstream 2; a new object kind).
2. **A block's horizon is found by its geometry**: superseded the same day (below). A block is the coal of one seam.
3. **Tonnes are in situ by Preston–Sanders**: superseded the same day (below). The interrogation gives the geology only.
4. **Variables are sampled at the model's lattice nodes** inside each block: changed the same day (below) to exact partial
   cells, as Vulcan does.
5. **Spry's fields are matched to our codes by a CSV Tim edits**, kept with the project as the seam definition is.
   Field names stay on his machine.
6. **Tolerances:** volume 0.1 %, tonnes 0.5 %, RD 0.005, a quality 0.05 in its own unit; each a parameter of the run.
7. **A small Reserves tool panel** comes in workstream 5, so Tim can run it on PIT B himself.

Changed the same day (Tim): *"the quality should be weight averaged based on volume for density and then coal tonnes for
qualities - this should be left open to easily let people create whatever they desire. The actual interrogation step
will be all about the geology at the block - the steps after can alter things as required like a dataflow in spry or a
workflow in deswik."* So:

- **The interrogation is the geology at the block:** its volume, area and thickness in each horizon, and each of the
  model's variables averaged over it. Nothing else.
- **Each variable says how it's averaged:** by volume, or by volume times other variables at each node. As a rule
  density is by volume and qualities by coal tonnes (volume × density), but each is set in the definition, so anyone
  can average anything by whatever they want.
- **Tonnes, moisture basis (Preston–Sanders), mining factors and seam rules are steps after the interrogation**, as a
  dataflow is in Spry or a workflow in Deswik. They need a plan of their own, and are not in this item.

And again (Tim): *"It should be a coal solid of a certain coal seam - the geological model may have insitu density grids
but all we care about is getting the qualities based on the geology for each grid (or block model) at the individual
solid - it should be easy to bring up a block of coal and say the ash grid and check that this seam has the correct
quality based on the ash grid contours and values."* So:

- **A block is a coal solid of one seam**, and each of that seam's grids (later, a block model's variables) is averaged
  over the solid, by the solid's whole thickness at each node. It isn't split between seams. How much of it lies between
  its seam's roof and floor is a check that it's in the seam, not a split.
- **Whatever the grids hold is taken as it is**: an in-situ density grid is a density like any other, with nothing
  converted.
- **It should be easy to check by eye**: pick a block of coal, choose one of its seam's grids, such as ash, and see the
  grid's contours and values around the block beside the block's value. Proposed as a workstream in the plan.

And (Tim): *"Don't worry about clean coal as a yield component - when we are doing the interrogation it will be clean
coal already - that logic comes in once we add dilution and it becomes ROM coal where we have both coal and waste. That
will have nothing to do with the geological interrogation."* The solid interrogated is seam coal; yield, dilution and
coal mixed with waste belong to the steps that make run-of-mine coal.

And (Tim): a block is cut down to whatever size the reserve calls for; in a coal mine a 60 m wide strip and 100 m long
blocks, cut to the geological grids, is the simple standard most would use, so a block is about 60 × 100 m in plan. The
tests' blocks are that size.

And (Tim): the weighting depends on how large the grid's cells are against the block, and geological models are
generally 25 m but vary; he is happy to follow Vulcan's logic to start with and tweak it later. At 25 m a 60 × 100 m
block takes in 2 or 3 nodes across, so counting each node inside it with its whole cell is off by up to a quarter at its
edges, and a block narrower than a cell can miss every node. So **each node's value holds over the cell centred on it,
weighted by the block's exact volume inside that cell (exact partial cells)**: the shares add up to the block's volume
wherever it sits and whatever the spacing, and a block smaller than a cell takes its node's value.

And (Tim), on how a block's seam is known: *"it has to come from an attribute - you would say this seam is this seam type
filter option to filter. The geometry should also match as well in theory but the quality grids are going to be at an rl
that is the quality variable and you just intersect the grid nodes into the seam structure - if the roof and floors
don't match perfectly I don't think it will matter."* So **a block's seam is the value of an attribute the user picks,
matched to the model's horizon codes (a filter: blocks whose attribute says this seam take this seam's grids)**. A
quality grid's value at a node holds over its cell in plan, so the averaging uses only where the block lies in plan and
its own volume; the seam's roof and floor never enter it. How much of a block lies between its seam's roof and floor is a
warning only, for a block whose attribute may be wrong (under 95 % in its seam), and changes no value.

And (Tim): *"I have numbers we can compare in Deswik and in Spry. Checking by eye is really just a contour of a grid and a
colour of a block for the same seam."* So:

- **Workstream 4 sets each block beside Deswik's numbers and Spry's**, whichever carries values, each through a match
  file Tim edits (source field to our code), reporting counts and differences only.
- **Workstream 6, the check by eye, is agreed:** a seam's grid drawn as contours, and the blocks of that seam coloured by
  their own value on the same bands, so a block whose colour doesn't match the contours round it stands out.

And (Tim): *"We would just [put] the values on the solids no?"* Yes: **each block's averaged values go on the block solid
as attributes, one a variable, named by its code**, as Deswik and Spry keep them. No table object and no change to the
object model. The run is an edit of the blocks, as a rename or a rebuild is: a new revision of each block it interrogates,
undone in one step, and run again it writes them afresh with the earlier revisions kept. The geometry doesn't change, so
the view keeps its meshes; CSV out is the solids' attributes, which the export already lists; and the check by eye
colours the blocks from the same attributes.

## Workstream 1 (this record's first PR): cells and averages

`zd-geom::reserves`, pure geometry and sums:

- **`cells_in_solid`**: the block's volume in each node's cell, the cell centred on the node, and the plan area of its top
  there. Each face is clipped to each cell it lies over (Sutherland and Hodgman), and the integral of its height over the
  piece is added for a face that looks up and taken away for one that looks down, as a closed solid's volume is; the
  cell's walls stand straight up and add nothing. Heights are taken from the solid's lowest point, so the sums aren't of
  large RLs.
- **`composite`**: each variable averaged by its own weighting, the block's volume in each node's cell times the
  variables it names (`Variable::weight_by`), with the block's volume and plan area in its cells. A node is left out of a
  variable where it has no value for it or for one of its weights, and counted. Every cell still counts in the volume.
- **`columns_in_solid`** and **`horizon_samples`**: where a vertical line through each node is inside the block, and how
  much of that lies between a seam's floor and roof, each node standing for its whole cell: a check that a block lies in
  its seam, coarse where the grid is coarse against the block. A node on the edge between two faces in plan is counted
  once, by the top-left rule.
- **`in_situ_rd`**: `RD_is = RD_ad × (100 − M_ad) / (100 + RD_ad × (M_is − M_ad) − M_is)`, kept for a step after the
  interrogation.

The block's volume of record stays its mesh's enclosed volume, which its cells' shares add up to.

Checked on made-up blocks at mine-grid coordinates, against answers worked by hand. Each block is 60 m across its strip
and 100 m along it, the size a coal mine usually cuts a block to:

- **A block 3 m thick on 5 m cells half over its sides:** 13 × 21 cells, the side ones halved and the corners quartered,
  adding to 18,000 m³ and 6,000 m², as its mesh. On cells whose edges run along its sides, 240 whole cells of 75 m³.
- **On 25 m cells sitting anyhow against it:** 18,000 m³ and 6,000 m², and ash rising across it averaged by each cell's
  share as worked from the cells' edges alone. A metre east, the same volume.
- **A 10 m square within one 25 m cell:** one node, 300 m³, and that node's value.
- **Ash rising 0.1 % a metre east, by coal tonnes:** 13 %, its value at the middle, whether the cells sit wholly in the
  block or half over its sides.
- **A wedge 3 to 8 m thick, RD 1.40 on its west half and 1.50 on its east, ash 10 % and 20 %:** 33,000 m³ in its cells, RD
  by volume 16.075 / 11, and ash by coal tonnes 262 / 16.075.
- **RD 1.4 and 1.6, CV 25 and 21 MJ/kg:** CV by coal tonnes 68.6 / 3, by volume alone 23, and by the RD and a weight
  grid together 103.6 / 4.4.
- **Preston–Sanders:** RD 1.45 at 3 % air-dried and 8 % in situ gives 140.65 / 99.25 = 1.4171285. It is unchanged when
  the moistures are equal.
- **How much of a block lies in a seam:** a block from RL 100 to 110 against seams meeting at 107 holds 18,000 m³ of
  one and 42,000 m³ of the other. Where a seam's roof is null, it holds none.
- **Two 3 m pieces one over the other as one mesh:** two stretches at every node, 36,000 m³ in its cells.
- **Nodes with no RD or no value:** left out and counted (a quality by tonnes is left out where the RD is missing too),
  the volume whole.
- **43,000 made-up blocks (ignored test), on one thread:** 731,000 cell shares in 534 ms on 25 m cells, and 3,311,000 in
  2.3 s on 10 m cells, every block's volume whole.

## Workstream 2: `solid.interrogate`

The geology at each block, written onto it:

- **Its seam by an attribute.** `seam_attribute` names the attribute, `seam` unless given; its value is matched to the
  model's horizon codes, case aside. Those are the codes the seam's grids are named by (`cu` for `pitcu.asg`), and the
  solids the surface stack makes will carry them (Tim: *"the grid names contain the seam name and the solids will have
  the seam name based on the surface stack - matching the seam name of the grids and the solids post surface stack is
  what we need"*). So **item 7's stacking writes each solid's horizon code as its `seam`**, and the interrogation needs
  nothing set; a block from elsewhere, such as Spry's, names its seam by another attribute. The seam name is always at
  the front of a grid's name, after any mine code (Tim: `seam1.asg`, or `grnseam1.asg` with the mine code `grn`); the
  definition takes the mine code off to give the horizon code. A block whose seam comes with the mine code still in front
  (`grnseam1`) matches too, by the seam's roof grid's name. A block with no such attribute, or one naming no horizon of the model, is left as it was and said, by
  count and the first few names.
- **The seam's grids, read for the run.** Each grid a seam needs is read once, in parallel, and checked against its
  fingerprint from when the model was built: a changed file is read as it is now and said (`link.changed`). A grid that
  can't be read, or isn't on the model's lattice, refuses the run with nothing written. A model with blocking issues in
  its stack is refused, as CLAUDE.md says.
- **What's averaged.** `variables` lists the codes to write; with none, every grid of each block's seam. `weights` says
  what a variable is weighted by besides volume. Left out, the model's density is by volume and every other variable by
  coal tonnes (volume × density). A weight that isn't written is averaged too, to weigh by. The weights are the run's
  parameters for now, kept in its record; they can move into the definition later.
- **Onto the block.** Each value is written as an attribute named by its variable code: as Vulcan names a grid, its
  file's extension is always three letters ending in `g` (Tim), and the code is the two before it (`as` for `.asg`, `rd`
  for `.rdg`), with `prefix` in front if one is given, and `interrogated` naming the model. A variable with no value over a block (no grid for its seam, or none of its nodes
  under it) is written as nothing and said. An attribute of the same name from elsewhere, one on a block never
  interrogated (Spry's own `as`, say), is never written over: the run is refused, asking for a prefix. Run again, it
  writes its own values afresh.
- **An edit of the blocks.** A new revision of each, undone in one step, with the earlier revisions kept: engine rule 4's
  new objects don't apply, as a rename's don't. The geometry is unchanged, so the view keeps its meshes.
- **The in-seam check.** At each node it covers, the share of the block's column between its seam's roof and floor,
  over all its nodes. A block under `in_seam` (95 % unless given) is warned of, placed at its middle, the five worst by
  name and the rest counted. It changes no value.
- **Across the cores**, with progress and Cancel (0060), over the grids and then the blocks.

Checked on a made-up seam model of two seams, `cu` (RL 96 to 100, RD 1.40, ash rising 0.1 % a metre east) and `cl` (RL
90 to 94, RD 1.50, ash 20 %), on a 25 m grid, with 60 × 100 m blocks sitting anyhow on its cells:

- **Each block takes its own seam's grids by its attribute**, the grids named with a mine code in front (`grncu.asg`), one
  block saying `CU` and the other `grncl`: `cu`'s RD, and its ash by the cells' shares
  as worked from the cells' edges alone; `cl`'s RD and ash. One revision of each block, undone in one step.
- **A 10 m square within one 25 m cell** takes that node's ash.
- **A block with no seam attribute and one naming `xx`** are left as they were, each said by its code. A block naming its
  seam by `Seam`, as a Spry block might, is interrogated when the run says so.
- **A block saying `cu` that lies in `cl`** is warned of, placed and named, and given `cu`'s values as its attribute says.
- **A block carrying Spry's own `as`**: the run is refused; with the prefix `g_` ours is written beside it and Spry's
  kept; run again, written afresh.
- **Weights:** ash by volume alone reads no RD grid; by tonnes, with the RD changed to 1.6 east of 100 m, it leans east
  and the changed RD grid is said.
- **43,000 made-up blocks against a 25 m model over 24 by 11 km** (an ignored test): 43,000 interrogated and written in
  4.1 s on every core.

## Workstream 3: the 2.5D column check

`zd_geom::column_volume`: a block's seam's thickness at each node, roof less floor, times the block's plan in the node's
cell (exact partial cells, the plan of its top in each cell from `cells_in_solid`), summed. It's how Vulcan and Minex work
out a seam's volume inside a polygon. A null roof or floor, or a floor above its roof, holds none.

`solid.interrogate` works it out for every block from the roof and floor it already reads, writes it onto the block as
`column_volume` (after any prefix), and sets it against the block's own volume, the volume of record. A block differing
by more than `volume_tolerance` (0.1 % unless given) is warned of (`interrogate.column_differs`), placed at its middle
with the difference in %, the five largest by name and the rest counted, and a summary says how many blocks are within
it and the totals either way. It changes no value. A block of the whole seam's thickness matches its column; a block
cut short of the roof or floor, as a bench is, holds less than its seam's column there and is warned of, as it should
be until benches are summed by outline.

Checked:

- **A block turned 30° across 25 m cells, the full 4 m of a flat seam:** its plan in the cells 6,000 m², exact, and its
  column 24,000 m³, its own volume. Where the roof is null over half of it, the column holds none there.
- **A seam whose roof rises 0.05 m a metre east, the block's top on it:** its column, cell by cell, 27,000 m³, its own.
  The same block cut 1 m short of the roof holds 6,000 m³ less than its column.
- **In the command:** blocks of the whole seam get `column_volume` 24,000 m³ and no warning; a block cut 1 m short of
  `cu`'s roof is warned of at +33.3 %, placed and named, with its column written all the same, and a 50 % tolerance lets
  it by. The summary says 1 of 2 blocks within 0.1 %.

## Workstream 4: beside Deswik's and Spry's numbers

`solid.compare` sets each block's values beside another package's, as they came in on the blocks: Spry's fields beside
its Solid field, Deswik's on its meshes. It's a report: nothing in the project changes, and it's kept in the run log.

- **A match file Tim edits** (`zd_io::read_value_match`), kept on his machine: `source,ours,tolerance`, a row a value.
  `source` is their attribute; `ours` is an attribute the interrogation wrote, or `volume`, the block's own. A tolerance
  ending in `%` is a share of their value, a plain number is in the value's own unit, and blank is decision 6's: volume
  0.1 %, density (`rd` unless the run names another) 0.005, any other 0.05. Columns in any order; comments start `#`; a
  file with mistakes is refused, each by its line.
- **For each row**, one line: how many blocks were compared, how many are within tolerance, theirs and ours summed, the
  mean difference, and how many had no value either way. Where any are beyond tolerance, a warning
  (`compare.beyond`) counts them and names the furthest, placed at its middle. A value Spry keeps as text reads as a
  number.

Checked on three made-up blocks carrying Spry-style `Volume`, `RD` (as text) and `Ash` beside our `rd` and `as`: volume
2 of 3 within 0.1 %, the block 100 m³ off named and placed; RD 3 of 3 within 0.005; ash 1 of 2 compared within 0.05, the
block with no `Ash` counted; nothing changed and nothing to undo. A match file with a word for a tolerance is refused by
its line.

On real data the run gives counts and differences; Tim runs it on PIT B with his own match file, as the field names stay
on his machine.

## Workstream 5: the Reserves panel

**Reserves** on the toolbar opens a tool panel made from the kit, as every tool panel is:

- **Interrogate**: the blocks picked, all on the layer of the first, or all on show; the seam model (the last built
  unless another is chosen); the attribute naming each block's seam (`seam`); the variables to write (every grid of each
  block's seam when blank); a prefix. One button, *Interrogate n blocks*, Ctrl+Enter, one undo.
- **Compare**: the same blocks, whose numbers (Spry's or Deswik's), and the match file, chosen from disk. *Compare n
  blocks* is a report: nothing changes.

What a run says shows in the panel's run result, coloured by severity, with Show beside a note that has a place, as
0059 gives every panel. The values go out with the solids: *Export solids* lists every attribute in its `solids.csv`.

Checked: the app's typecheck and tests. Not checked by hand in the app.

## Workstream 6: the check by eye

Tim: *"a contour of a grid and a colour of a block for the same seam."*

- **A horizon coloured by one of its grids.** The seam legend in the viewport has **Roof coloured by**: the thickness, as
  before, or any of the horizon's grids. A grid is drawn where the horizon holds coal, in round bands in its own units
  (`valueBands`: 1, 2 or 5 times a power of ten, at most 12 bands, so ash from 10.2 to 15.9 % is 0.5 % a band and RD
  0.02) with a contour line at each edge, in the shader that colours by RL, and grey where there's no value. The legend
  shows the bands and the step.
- **The seam's blocks on the same bands.** *Colour cu's blocks by their as* takes each block whose `seam` names the
  horizon (`block_values`, matched as the interrogation matches them) and colours it by its own interrogated value, in the
  band the grid's contours put that value in. The band goes in the block's tile's table beside hidden and selected
  (0060), so nothing is sent but a byte a block; the shader takes the band's colour from the same ramp. Choosing another
  horizon or grid puts the blocks back in their layers' colours.
- A block's number is rounded where the shader reads its table per pixel: arriving interpolated, a solid's number could
  read as the one before it.

Checked:

- **Bands:** ash from 10.2 to 15.9 % gives 12 bands of 0.5 % from 10; a value on an edge is in the band above; RD from
  1.35 to 1.55 gives 0.02, and 1.45 lands in its band despite floating point (app tests).
- **Blocks on bands, in the release app's select check:** two of six cubes put on bands 0 and 1 of 2 through their
  tile's table take the two bands' colours, the rest keep their own, and they come back; on WebGPU and WebGL2. This
  caught the interpolated number above.
- **A grid in bands, in the release app with `--solids --seam`:** a made-up seam over the 43,000 made-up blocks, its ash
  rising from 10 % in the west to 30 % in the east, drawn in plan by its ash: west, middle and east read as three
  colours, its range 10 to 30.15.

Not checked by hand: the legend's picker and its toggle, and blocks coloured from a real interrogation.

## Still to come

Nothing in this item. After it: the steps after the interrogation (tonnes, moisture basis, dilution and run-of-mine
coal, factors and rules) as a dataflow, which need their own plan; and item 7's block cutting, whose stacking writes each
solid's horizon code as its `seam`.
