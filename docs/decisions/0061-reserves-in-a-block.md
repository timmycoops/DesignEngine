# 0061 — Interrogating a block: a thin reserves slice against the seam model

Status: agreed 12 November 2026 (Tim: "Agree all seven", the Build Plan's "Reserves slice plan" tab). Workstream 1 of 5.

## Why

Reserves had nothing underneath them (the review of 10 November, finding 5): a seam model linked its quality and
density grids but nothing read them, nothing sampled inside a solid, and the 2.5D column check existed only for whole
seams. Item 6 of the agreed order runs a thin slice beside Phase 4's block cutting, so reconciliation with Spry starts
early, on real blocks: block solids already in a project and a seam model, each block's geology interrogated from the
model's grids, its volume checked against a 2.5D column calculation, and set beside Spry's own values by name.

## Decision

Agreed with the plan, all as recommended:

1. **The records are a table object in the project**, one row per block, saved in the `.zdp` as its own
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

## Still to come

Workstream 2: the interrogation command, the table object, each variable's weighting in the definition, grids loaded
for the run and CSV out. Workstream 3: the 2.5D column
check by block outline with exact partial cells. Workstream 4: Spry beside ours. Workstream 5: the Reserves panel.
