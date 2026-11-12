# 0061 — Reserves in a block: a thin slice against the seam model

Status: agreed 12 November 2026 (Tim: "Agree all seven", the Build Plan's "Reserves slice plan" tab). Workstream 1 of 5.

## Why

Reserves had nothing underneath them (the review of 10 November, finding 5): a seam model linked its quality and
density grids but nothing read them, nothing sampled inside a solid, and the 2.5D column check existed only for whole
seams. Item 6 of the agreed order runs a thin slice beside Phase 4's block cutting, so reconciliation with Spry starts
early, on real blocks: block solids already in a project and a seam model, each block's volume, tonnes and qualities,
checked against a 2.5D column calculation and set beside Spry's own values by name. No mining factors, seam rules or
waste yet.

## Decision

Agreed with the plan, all as recommended:

1. **The records are a table object in the project**, one row per block and horizon, saved in the `.zdp` as its own
   table, with CSV out (workstream 2; a new object kind).
2. **A block's horizon is found by its geometry.** Each node's column through the block is split by the working
   horizons' roofs and floors, so a block across two gets a record for each, with a note when it takes in more than 1 %
   of another horizon.
3. **Tonnes are in situ by Preston–Sanders**, with the in-situ and air-dried moisture grids the definition names (two
   new columns in the definition CSV). A horizon with no moisture grids gives air-dried tonnes, and the run warns.
4. **Qualities are sampled at the model's lattice nodes** inside each block, each weighted by the block's thickness at
   the node times the cell's area. Exact partial cells are for the volume check.
5. **Spry's fields are matched to our codes by a CSV Tim edits**, kept with the project as the seam definition is.
   Field names stay on his machine.
6. **Tolerances:** volume 0.1 %, tonnes 0.5 %, RD 0.005, a quality 0.05 in its own unit; each a parameter of the run.
7. **A small Reserves tool panel** comes in workstream 5, so Tim can run it on PIT B himself.

Still open: whether PIT B's Spry project carries tonnes, RD and qualities for each block. It shapes only workstream 4.

## Workstream 1 (this record's first PR): columns and composites

`zd-geom::reserves`, pure geometry and sums:

- **`columns_in_solid`**: every lattice node a solid lies over, with where a vertical line through the node is inside
  it, bottom first. Each face's plan is tested against the nodes in its box, its height at a node found from where the
  node sits in it, and a face whose outward normal points down is entered going up. A node on the edge between two faces
  in plan is counted once, by the top-left rule. A block whose sides stand on grid lines takes the nodes on its west and
  north sides, not its east and south, so its nodes times the cell's area is its plan area. A column through a solid in
  two pieces, one over the other, has two stretches. Faces standing straight up have no plan and are passed over.
- **`horizon_samples`**: the columns split by one horizon's floor and roof, node by node on the same lattice. A null
  roof or floor holds none.
- **`composite`**:
  - air-dried RD by thickness, and raw qualities by mass (thickness × air-dried RD);
  - in-situ RD worked out node by node by Preston–Sanders, then by thickness;
  - the block's sampled volume and plan area, which set its share in each horizon.

  A node with no RD is left out of the RD and qualities and counted, and a node with no value for a quality is left out
  of that one and counted. Every node still counts in the volume.
- **`in_situ_rd`**: `RD_is = RD_ad × (100 − M_ad) / (100 + RD_ad × (M_is − M_ad) − M_is)`.

The block's volume of record stays its mesh's enclosed volume. These samples set the weights and each horizon's share.

Checked on made-up blocks at mine-grid coordinates, against answers worked by hand:

- **A 50 × 40 m block, 3 m thick, its sides on grid lines:** 80 nodes at 5 m, each with one stretch from RL 100 to 103,
  on its west and north sides and not its east and south. 2,000 m² and 6,000 m³, as its mesh.
- **Ash rising 0.1 % a metre east across it:** 12.5 %, its value at the middle, to a millionth.
- **RD 1.40 on the west half and 1.50 on the east, ash 10 % and 20 %:** RD 1.45, and ash 44 / 2.9 % by mass.
- **Preston–Sanders:** RD 1.45 at 3 % air-dried and 8 % in situ gives 140.65 / 99.25 = 1.4171285. It is unchanged when
  the moistures are equal. 6,000 m³ is 8,502.8 t in situ.
- **A block from RL 100 to 110 across two horizons meeting at 107:** 6,000 and 14,000 m³, the block between them.
  Where one horizon's roof is null, it holds none.
- **A wedge 3 to 8 m thick:** 11,000 m³, as its mesh. Two 3 m pieces one over the other as one mesh: two stretches at
  every node, 12,000 m³.
- **Nodes with no RD or no value:** left out and counted, the volume whole.
- **43,000 made-up blocks on a 10 m lattice:** 860,000 node columns in 94 ms on one thread (an ignored test).

## Still to come

Workstream 2: `reserves.blocks`, the table object, grids loaded for the run and CSV out. Workstream 3: the 2.5D column
check by block outline with exact partial cells. Workstream 4: Spry beside ours. Workstream 5: the Reserves panel.
