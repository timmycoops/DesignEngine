# 0063: Seam grids are never filled; holes are said, and a cut over one is refused

Status: agreed 14 November 2026 (Tim). Follows 0062's polygon cover rule, the same way round for the seam model's grids.

## Why

A cut by seams (0062) splits a solid by its seam model's grids and makes nothing outside the solid. Before this, two
things could still go wrong without a word:

- **Past the grids' edge.** The part of a solid past the edge of the grids was kept whole on a layer of its own
  (`<layer> off the grids`), as ground the model doesn't describe.
- **Over a hole.** A node with no height in any working horizon was taken as below everything, so the whole column
  there went into the overburden. A node with no height in one seam, with the others live, was taken as that seam absent.

Holes reach the grids in three ways:

- a Vulcan node flagged off holding nothing (0 or blank), an odd flag, or a live node holding no number (0025 keeps the
  height a flagged-off node holds, so a grid exported whole from Vulcan has none);
- an XYZ or CSV grid with no point near a node;
- the no-seam RL (0020), which blanks a seam where it's marked: that's a seam absent, not a hole.

Tim asked whether holes should be filled when grids are imported. His answer: the grids are inputs; don't fill them.
Flag that they have holes, and if a solid to be cut sits over one, say so; the geology model is fixed and the grids are
imported again. A grid fixing tool may come later. The grids must reach over the whole solid for the cut to be right.

## Decision

- **Nothing fills a grid.** Roofs, floors and quality grids are read as they are.
- **A hole is a node whose roof or floor holds no height as read**, unless the model's no-seam RL marks the seam absent
  there (`zd_core::grid_holes`). A seam marked absent is part of the model and is laid on the floor above for the cut,
  as before.
- **The seam model says its holes.** `seam.build_model` adds a warning for each horizon with holes (`seam.holes`,
  `IssueKind::Holes`): how many nodes, and the largest hole's place, nodes joined side to side or corner to corner
  (`zd_geom::grid_hole_patches`). It doesn't block the model: a solid clear of the holes can still be cut.
- **A cut by seams checks first and refuses.** Before anything is cut, each solid's footprint in plan is checked against
  the grids (`zd_geom::solid_off_the_grids`):
  - any part past the grids' edge (`solid.off_grids`);
  - any part over a grid cell with a hole at a corner in any working horizon (`solid.over_grid_holes`). The whole cell
    counts, because the layers can't be placed anywhere in it.

  Every place is listed with its area and where it is, and nothing is cut. Slivers narrower than 1 cm are left out of the
  check, as with the polygons (0062); any volume such a sliver leaves past the edge is said, in no piece.
- **The off-grids layer and its parameter are gone.**
- **The Cut solids panel runs the same check** (`check_cut_by_seams`) as the solids or the model change, lists what it
  finds with Show, and holds the cut ("Fix the grids or the solids first").
- Unpaired nodes (a roof live where its floor isn't) still block the whole model, as before.

## Known answers

- `zd-geom/tests/layers.rs`:
  - A strip on the grids with no hole under it is covered, and so is one with a hole well clear of it.
  - A strip starting 50 m west of the grids: 50 × 60 m past their edge, about its middle.
  - A hole at one node inside a strip: the four 10 m cells round it, 400 m², about the node. At the strip's edge, half
    of that (200 m²); 10 m outside, none, its cells only touching. Two holes side by side: one part of six cells, 600 m².
  - Holes told apart, largest first: three nodes joined corner to corner, then one alone.
- `zd-commands/tests/seam.rs`: eleven nodes with nothing in roof or floor, a 3 × 3 block and two at the far corner: one
  `seam.holes` warning on 11 nodes, at the block's middle node; the model stays usable, and the grids read with 11 holes
  flagged.
- `zd-commands/tests/cut_by_seams.rs`:
  - A strip 50 m past the grids: the check gives one `solid.off_grids` warning (3,000 m² about its middle), the cut is
    refused with nothing made, and the strip kept to the grids is checked and cut (180,000 m³).
  - A hole along a whole grid column under a strip: said on the model, 9 nodes. The strip over it gives one
    `solid.over_grid_holes` warning, 50 × 60 m. Its cut is refused; a strip clear of it cuts; both together are refused,
    nothing made.
