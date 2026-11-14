# 0064: Berms lie on the grid they're built to, and a seam cut takes off flaps

Status: agreed 14 November 2026 (Tim). Follows 0062 (cut by seams) and 0063 (grids never filled).

## Why

PIT B's strips, cut by seams, gave pieces that crossed themselves (61 of 190), so the cut by polygons refused them, and two
strips didn't add back (+19.2 and −3.6 m³). Measured by counts only:

- the grids don't cross, the slabs are clean, and the pit, strips and faces going in are clean;
- every crossing lay on a strip's own surface, almost all on its berms;
- a berm registered on a grid touches it at its corners, but its triangles reach across the grid's cells, so between
  corners it's a chord, millimetres to 10 cm above or below the grid, weaving through it;
- cut along that grid as a seam's roof, the pieces either side get films of coal or rock a few millimetres thick,
  tapering to knife edges, where Manifold leaves hairline touches: no volume counted twice, but crossing by the check.

Tim: faces are designed on the same grids the seams are stacked from; using different grids is the designer's choice, and
slivers along the berms are then to be expected. Deswik removes flaps (thin parts of a solid between faces close
together) before cutting with surfaces, for the same reason.

Draping alone made the berms lie on the roof exactly, and the cut added up, but the berm and the roof were then one
surface on two sets of triangles, agreeing to rounding, and Manifold left sheets of no thickness there. Flaps taken off
alone left the chords. Together the cut is clean.

## Decision

- **A face lies on the grids it's built to** (`zd_geom::drape_on_grids`, run on every face sheet, so for `face.project`,
  the Builder and Rebuild). Where a triangle's corners all lie on the grid the toe registers on, or one a wall goes to or a
  berm registers on, it's cut into the pieces of the grid's triangles under it, each on its grid triangle. An edge it
  shares with a triangle off the grid (the wall up from a crest) gains the same corners, so the face stays one sheet. A
  triangle already on the grid, one the grid doesn't wholly cover, or one more than 0.5 m off it between its corners
  (not following it, as a berm on a real seam grid does to within centimetres) is left as it is. Nothing else moves.
- **A cut by seams takes off flaps** (`flap_thickness`, 0.01 m by default, 0 for none; the Cut solids panel's Flaps).
  A flap is where a piece's top and bottom, faces flatter than 60°, one facing up over one facing down, come within the
  flap thickness of each other: a knife edge where a strip's surface runs into a seam's roof or floor at a low angle, or a
  sheet with no thickness where it lies on it. It's cut out straight down where the two faces are that close, and 5 mm
  further in plan so the cut doesn't stand on the slab's own edge (where a layer is the minimum thickness thick). A steep
  wall meeting a seam is no flap.
- **Flaps are left out and said**, as the thin layers are: `seam.flaps_left_out`, its volume, and how many pieces had
  them. The pieces, the thin layers and the flaps add up to the solid.

## Known answers

- `zd-geom/tests/drape.rs`:
  - a 40 × 10 m band, two triangles with corners on grid nodes, over RL 100 + 0.0005 x²: draped, every piece and corner
    lies on the grid's triangles, the plan area stays 400 m², it's one sheet, and it moved 0.2 m at most (the chord at
    x = 20 m: 0.0005 × 1600 / 2 − 0.0005 × 400);
  - on a plane grid it's already on, nothing changes;
  - over RL 100 + 0.002 x² (0.8 m off) it's left as it is and counted.
- `zd-geom/tests/layers.rs`:
  - a strip's top 4 mm over U's roof: the overburden film, 4 mm × 500 × 60 = 120 m³, is a flap, taken off and said; with
    no flap thickness it's a piece;
  - a top rising 1 in 100 through U's roof: the wedge thinner than 1 cm, cut 5 mm further, 500 × 0.005 × 1.005² =
    2.5250625 m³, comes off, and 2,250 − 2.5250625 m³ is the overburden piece;
  - battered walls (about 72°) through three seams: no flap.
- `zd-commands/tests/cut_by_seams.rs`: a box's top 4 mm over cu's roof: `seam.flaps_left_out` says 24.000 m³, the
  pieces are the box less it; with `flap_thickness` 0 the film is a piece.
- PIT B (counts only), the bent toe's 15 strips: faces built again in 220 ms, 153,149 of 153,150 berm triangles on a seam
  surface exactly; sliced in 1.1 s; cut by its seam model into 188 pieces in 2.4 s, none crossing, 297 m³ of flaps and
  1,182 m³ of thin layers left out and said, adding up exactly; 5 polygons cut them into 622 blocks in 1.2 s, none
  crossing, names unique, adding up exactly; 323 coal blocks interrogated. Without the drape, the cut still refused, and
  without the flaps, so it takes both.
