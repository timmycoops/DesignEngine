# 0016 — The general cut: keep below, above or both

Date: 2026-10-30 · Status: accepted (Tim, 2026-10-30)

## Why

Cropping a seam model by the topography was first planned as a seam-model feature. Tim's view: it is
just a cut, keeping what lies below or above a surface, and the surface need not be a topography. The
same cut runs again and again: a large solid stacked into seam solids, those cut by the topography,
then blocked; and any design can be cut by a surface, a grid or a level. So one command does it, and
the seam-model crop is dropped.

## Decision

`solid.cut_by_surface` (Phase 0, engine rule 5) becomes the general cut:

1. **Cut by a surface, a grid or a level.** `surface` takes a surface or a grid object (a grid cuts as
   its triangulation, engine rule 6); `rl` cuts at a level instead, made as a flat surface over each
   solid. One of the two, never both.
2. **Keep below, above or both** (`keep`, both by default). A side with nothing on it is left out and
   said so.
3. **One solid or many in a run** (`solid`, or `solids`), each cut on its own, across the machine's
   cores. Over ten solids the run record sums the cut up rather than giving a line per solid.
4. **Each part keeps its solid's attributes**, plus `cut_side` (below or above) and `cut_by` (the
   surface's name or the level). Parts are named "<solid> <side> <surface or RL>"; `below_name` and
   `above_name` name one solid's parts.
5. **Nothing invalid is saved** (decision 0015). Each part is checked by the solid rules before it is
   kept; a part that would cross itself leaves its solid uncut and says why, so one bad solid does not
   lose a batch. Every cut must keep its volume: the run record gives the largest difference between a
   solid and its parts.
6. As before, inputs are never changed, a surface or solid that crosses itself is refused, and the part
   of a solid the surface does not cover in plan is counted as above, with a warning.

Stacking a solid by the seam model and blocking (Phase 4) build on this cut.

## Evidence

- A 10 m cube cut at RL 105: two parts of 500 m³ (to 1e-6 m³), named and carrying the cube's
  attributes with their side; keeping below at RL 102.5 makes one part of 250 m³; keeping above at
  RL 120 makes none, and says nothing lies above.
- Twelve cubes cut in one run by a flat grid at RL 104: 24 parts, 4,800 m³ below and 7,200 m³ above,
  every part checked, the inputs unchanged.
- Refused: no solid; no cutter, or both a surface and a level; a solid given twice; part names with
  several solids; an unknown `keep`.
- The golden pit: all 44,175 relimiting solids (the 42,223 files, imported with repair) cut by the
  mined-out topography (3.28 M triangles), keeping both sides, in nine runs of up to 5,000 solids:
  **51,783 parts in 187 s** (4.2 ms a solid, the booleans about 23 s of it), every part checked by
  the solid rules and none crossing itself. Each solid and its parts agree to 0.15 m³ at most (1.4e-5
  of that solid's 10,996 m³; most to 1e-12). 25 solids were cut again with the whole topography after
  a part of the cropped cut failed its check; one was left uncut, its part above crossing itself either
  way, and named. Using the whole topography for every cut took 343 ms a solid, about four hours for
  the pit.
