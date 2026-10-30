# 0020 — An RL that means no seam

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31)

## Why

A seam exists only where the roof is above the floor. Some geological models mark where a seam is absent
with a flat RL rather than a null: the node is live and holds a number, so a reader cannot tell it from
an elevation. In a client's folder of 15 seams, every roof tops out at exactly one RL, flat over 55 to
100 % of its nodes, and a flat grid at that RL sits beside them. Where such a roof lies over a real floor,
the model read coal more than 400 m thick, and the checks flagged crossings that are not there.

Clipping to the topography or base of weathering does not answer this, though it is still part of the
job. Of the 67,707 such nodes across the 15 seams, 60,205 have their floor under the topography, and at
57,629 of those it is more than 50 m down: clipping would leave the false coal in place.

## Decision

1. **The definition names the RL.** A line beside `folder`, in the file the planner owns:

   ```text
   folder,D:\\models\\pit\\grids
   no_seam,160
   horizon,parent,material,working,roof,floor,rd
   ```

2. **A node with its roof or floor at that RL holds no seam**, and both read as null. RLs are compared in
   single precision, as the grids store them. The files are never changed.
3. **It is on the model**, so the checks, the viewport and later commands all read a horizon alike. A model
   saved before this reads with none.
4. **It is counted.** The run record says how many nodes each horizon lost to it, and Properties shows it.
5. **`seam.suggest_definition` works it out from the grids, never the names:** every roof must top out at
   one RL, flat over at least 1 % of its nodes, with two roofs or more to agree. It writes the line and
   says so, and deleting the line takes the rule away. Horizons are nested and ordered with those nodes
   blank, so a false thickness cannot make a parent.

This does not relax "checked, never fixed": a planner's explicit rule is not a repair. A floor above its
roof still blocks, and clipping stays a separate step: the general cut (decision 0016), by the topography,
the base of weathering or any other surface, once the seam solids exist.

## Evidence

- Two seams, 3 m and 4 m, with both roofs at the mark over real floors on the western 10 of 21 columns:
  the suggestion writes the line, the model has coal at 121 of 231 nodes at 3.00 m, no blocking issue,
  a working section that stops at the real roofs, and the files unchanged. Delete the line and the same
  grids read 43 m of coal and a blocking crossing.
- The 15-seam folder: mean thickness 0.6 to 5.0 m in every seam, where one read 214 m before. Blocking
  issues fall from 18 to 7, all crossings, at 36 nodes in all, each with its place.

## Known limit

The rule cannot tell a seam that is absent from one that subcrops with its roof marked in the air: at 158
of those nodes the floor is within 5 m of the topography, and 630 more within 20 m. Those may be real coal
that the rule blanks. They are a small share, and an edit to the definition takes the rule off, but it
needs checking on any pit where subcrop tonnes matter.
