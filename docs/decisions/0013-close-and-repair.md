# 0013 — Close and repair solids

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29); slice A built, slice B (crossings) next

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
