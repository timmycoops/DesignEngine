# 0013 — Close and repair solids

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29); slices A and B built (B below)

## Why

The golden pit's 42,223 relimiting solids (decision 0011's reader) imported at 98.5 %; the 638
refused are defects in the source solids, left by the boolean steps that made them. Measured:

- **Fins** (zero-volume flaps hanging off an edge of three or more faces): taking them off closes 93.
- **Cracks** (a vertex on another triangle's edge, unjoined): in 64, mostly beside holes.
- **Holes** (missing faces): in 157, large (median 30 m², a tenth over 266 m², the largest 1,277 m²).
- **Crossings**: 327, all within single pieces; 136 still cross at a 10 cm tolerance, so they are
  real, and a looser weld only makes the open cases worse.
- Manifold does not resolve a self-crossing input: two overlapping boxes keep 2,000 m³ through a
  union, not the 1,875 m³ of their combined shape. Crossings need a self-union of our own.

## Decision

1. **Repair runs only when asked and is never silent.** `solid.import` gains `repair` (off by
   default) and the app's Solids menu an "Import and repair" item. The run record lists every fix,
   and the solid carries a `repair` note (and `repair_filled_m2` when holes were filled), seen in
   Properties and from Python. Pieces split from one file share its note.
2. **Slice A**, in order (`close_solid` in `zd-geom`):
   - Fins come off, until none is left. No volume changes.
   - Cracks are joined: the triangle on the open edge is split at the vertex lying on it (within
     the weld tolerance, more than it from either end). No coordinate moves.
   - Holes are filled across the loop's own vertices (no new vertices), by Manifold's constrained
     Delaunay triangulation of the loop projected onto its best-fit plane; a loop that folds over
     itself there is left open and named.
   - Closed patches are oriented again to face out (an open box was turned to face up by the repair
     step).
   - Touching edges are taken apart as before (decision 0011).
3. **Fill limit: 10 % of the solid's surface** (`fill_limit`, a share). Over it, the file is refused
   with the holes' total area, its share and where the largest is. Tim's choice for a start.
4. **Slice B: crossings**, by a self-union of our own: triangles cut where the surface passes through
   itself (per-triangle constrained triangulation), then the outer shell kept, where solid lies on
   one side and empty space on the other. The volume change is reported.

## Evidence

- `close_solid`: a fin comes off and the cube is whole at 1,000 m³ with its eight vertices; a crack
  at an edge's midpoint is joined with the vertex unmoved; a missing triangle (50 m²) and a missing
  top (100 m², from an inward-facing open box, turned out after) are filled exactly; a hole whose edge
  is not flat is filled with its area counted.
- Through `solid.import`: a cube with a fin, a crack and a hole imports with `repair` at 1,000 m³ and
  the note "1 fin triangles taken off; 1 cracks joined; 1 holes filled (50.0 m², 8.3 % of the
  surface)"; without it, it is refused. A lidless cube (16.7 %) is refused at 10 % with where the
  hole is and imported at 20 %. A clean solid has nothing to do and carries no note.
- The golden pit's 638 refused solids, again with repair: **159 import** (146 with fins off, 43 with
  holes filled, 43 with touching pieces taken apart, 3 with cracks joined; the median fill is 1.8 % of
  the surface). Still refused: 364 crossings (some open solids cross once closed), 99 still open or
  with edges of three or more faces (likely internal walls), 16 with holes over 10 %.

## Slice B: crossings (the self-union)

`self_union` in `zd-geom` reduces a closed mesh whose surface passes through itself to the surface of
the solid it encloses: the region where its winding number is 1 or more.

1. Every crossing pair's segment comes from the crossing search's interval test. Faces overlapping in
   one plane (or folded flat on an edge) give segments too: each one's edges clipped to the other.
   On the pit, these were most of the crossings: zero-thickness sheets where a surface folds flat on
   itself, every sampled case with the two faces facing opposite ways.
2. The segments' ends, and points where two segments in one triangle cross, are shared by every
   triangle they lie on (merged within 1 µm); a point on an edge splits that edge for both triangles.
3. Each affected triangle is triangulated again in its own plane by **spade's constrained Delaunay
   triangulation** (the locked stack names spade; Manifold's triangulator takes no constraint
   edges), with its edges and segments as constraints. A panic inside spade on near-degenerate input
   is caught and refuses the mesh, saying where.
4. Regions between the crossing lines are judged by the original surface's generalized winding
   number just behind and just in front of their largest face, "just" being a quarter of the distance
   to the next surface (at most 1 mm). The first version probed a fixed 1 mm and emptied relimiting
   slivers about 1 mm thick. Faces overlapping in one plane and facing the same way are kept once;
   facing opposite ways, they drop out (no thickness, the same winding on both sides).
5. **The result is checked before it is accepted**: along 160 x 160 vertical rays the length where
   the original's winding is 1 or more must equal the length inside the result, ray by ray (exact
   along each ray, whatever the spacing). More than 0.01 % of the volume apart refuses the mesh with
   the disagreement. This caught the first version's misjudged regions (a 3.46 m³ solid returned
   as 0.05 m³) that a volume comparison alone would not have explained.

`solid.import` with `repair` runs it when the closed mesh crosses itself, then checks crossings
again. The run record and the solid's `repair` note give the volume as read (an overlap counted
twice), the volume enclosed and the change (`repair_volume_change_m3`).

Evidence:

- Two cubes overlapping by 5 m each way: 2,000 m³ as read, 1,875 m³ enclosed, to 1e-6 m³.
- A cube and a 6 m box turned 45° about its centre: 1000 + 600 - 1200 (√2 - 1) m³, to 1e-6 m³.
- A cube nested in another is swallowed (1,000 m³); two cubes sharing their top and bottom planes
  give the 1,750 m³ L-shape; a cube sitting offset on another gives 2,000 m³ with no wall between; a
  0.4 mm slab crossed by a cube keeps its 0.16 m³.
- Through `solid.import`: the overlapping cubes import at 1,875 m³ with the change noted.
- The pit's 638 refused solids, with slices A and B: **247 import**, 88 of them with crossings resolved
  and verified (the volume change a median of 0.000 % and at most -0.009 %). Still refused: 125
  whose cut mesh could not be closed and 124 that still cross after it (the cutting is fragile on
  near-degenerate geometry), about 60 whose result the ray check rejected, 61 still open or with edges
  of three or more faces, 16 with holes over 10 %, 3 the triangulator could not cut. Every accepted
  union passed the ray check; none is accepted unchecked.
