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
4. **Variables are sampled at the model's lattice nodes** inside each block, the block's volume at a node being its
   thickness there times the cell's area. Exact partial cells are for the volume check.
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

Still open: whether PIT B's Spry project carries tonnes, RD and qualities for each block (it shapes only workstream 4),
and how a block's seam is known.

## Workstream 1 (this record's first PR): columns and averages

`zd-geom::reserves`, pure geometry and sums:

- **`columns_in_solid`**: every lattice node a solid lies over, with where a vertical line through the node is inside
  it, bottom first. Each face's plan is tested against the nodes in its box, its height at a node found from where the
  node sits in it, and a face whose outward normal points down is entered going up. A node on the edge between two faces
  in plan is counted once, by the top-left rule. A block whose sides stand on grid lines takes the nodes on its west and
  north sides, not its east and south, so its nodes times the cell's area is its plan area. A column through a solid in
  two pieces, one over the other, has two stretches. Faces standing straight up have no plan and are passed over.
- **`solid_samples`**: the block's whole thickness at each node, which its seam's grids are averaged by.
- **`horizon_samples`**: the columns between one horizon's floor and roof, node by node on the same lattice; a null roof
  or floor holds none. Set against `solid_samples`, it says how much of a block lies in its seam.
- **`composite`**: each variable averaged by its own weighting, the block's volume at each node times the variables it
  names (`Variable::weight_by`); with the block's sampled volume and plan area. A
  node is left out of a variable where it has no value for it or for one of its weights, and counted. Every node still
  counts in the volume.
- **`in_situ_rd`**: `RD_is = RD_ad × (100 − M_ad) / (100 + RD_ad × (M_is − M_ad) − M_is)`, kept for a step after the
  interrogation.

The block's volume of record stays its mesh's enclosed volume. These samples set the weights.

Checked on made-up blocks at mine-grid coordinates, against answers worked by hand. Each block is 60 m across its
strip and 100 m along it, the size a coal mine usually cuts a block to:

- **A block 3 m thick, its sides on grid lines:** 240 nodes at 5 m, each with one stretch from RL 100 to 103, on its west
  and north sides and not its east and south. 6,000 m² and 18,000 m³, as its mesh.
- **Ash rising 0.1 % a metre east across it, by coal tonnes:** 13 %, its value at the middle, to a millionth.
- **A wedge 3 to 8 m thick, RD 1.40 on its west half and 1.50 on its east, ash 10 % and 20 %:** RD by volume
  16.075 / 11, and ash by coal tonnes 262 / 16.075.
- **RD 1.4 and 1.6, CV 25 and 21 MJ/kg:** CV by coal tonnes 68.6 / 3, by volume alone 23, and by the RD and a weight
  grid together 103.6 / 4.4.
- **Preston–Sanders:** RD 1.45 at 3 % air-dried and 8 % in situ gives 140.65 / 99.25 = 1.4171285. It is unchanged when
  the moistures are equal.
- **How much of a block lies in a seam:** a block from RL 100 to 110 against seams meeting at 107 holds 18,000 m³ of
  one and 42,000 m³ of the other. Where a seam's roof is null, it holds none.
- **The wedge:** 33,000 m³, as its mesh. Two 3 m pieces one over the other as one mesh: two stretches at every node,
  36,000 m³.
- **Nodes with no RD or no value:** left out and counted (a quality by tonnes is left out where the RD is missing too),
  the volume whole.
- **43,000 made-up blocks on a 10 m lattice:** 2,580,000 node columns in 285 ms on one thread (an ignored test).

## Still to come

Workstream 2: the interrogation command, the table object, each variable's weighting in the definition, grids loaded
for the run and CSV out. Workstream 3: the 2.5D column
check by block outline with exact partial cells. Workstream 4: Spry beside ours. Workstream 5: the Reserves panel.
