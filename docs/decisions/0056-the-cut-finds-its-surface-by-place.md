# 0056 — The cut finds its surface by place, and work goes to the next free core

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: "Yes, time it", then "Yes, build them" on the timings and
steps 1 and 2). Workstream 4 of the Solids plan. After 0016 (the general cut).

## Why

The cut was timed before anything was changed, on 2,938 of the golden pit's relimiting solids (every fourteenth) against
the mined-out topography (3.28 M triangles), on one thread:

| Stage | A solid | Share of the cut |
|---|---|---|
| Finding the topography near the solid | about 5 ms | 75 % |
| Handing meshes to Manifold | 0.9 ms | 13 % |
| The boolean | 0.4 ms | 6 % |
| Closing the surface under the solid | 0.4 ms | 5 % |
| Reading the result back | under 0.1 ms | 0.2 % |
| The cut | 7.0 ms | |
| The crossing check of its parts, after | 0.7 ms | on top |

Every solid looked through all of the topography for the few hundred triangles over it, and worked out the
topography's extent again each time. And of a whole-pit run of 44 s, 21 s was the cuts, on every core, and 23 s the
work after them, a solid at a time: each part's crossing check and the making of its solid.

The strip slice was timed too: 0.3 to 1.9 s for a pit of 9 or 10 faces on the made-up grid pit, about half closing
the faces and half cutting the strips. It is left as it is.

## Decision

1. **A cutting surface's triangles are found by place, once a run.** `DesignIndex` lays a plan grid over the surface
   (about two triangles to a cell, 4 million cells at most), each triangle in every cell its plan extent reaches, and
   keeps the surface's extent. `cut_solid_by_indexed_surface` cuts by it. It gives the triangles a look through the
   whole surface gives, triangle for triangle and in the same order, so a cut is the same to the bit.
   `solid.cut_by_surface` builds the index once and every solid uses it; a cut at a level has two triangles and needs
   none.
2. **A solid's parts are checked and made solids on the core that cut it.** The crossing check of each part, the second
   cut with the whole surface where a part fails its check (0016), and the making of each part's solid all run with the
   cut. What is left for one core is the run record and the list of changes.
3. **Work goes to the next free core.** `in_parallel` gave each core a fixed run of the items, so a run of heavy ones
   set the pace while other cores stood idle. Each core now takes the next item not yet taken, and the results go back
   in the items' order. Every command that works across cores uses it: the cut, the imports and the repair.
4. **Nothing changes in what a command takes or gives**, or in what it says.

## Known answers

- A 400 m square design of 320,000 triangles, with boxes under its middle, off-centre, across its edge, at its corner
  and off it: the index gives the same triangles as a look through the whole design at three reaches, and the cut is
  the same to the bit, parts and half-space. Under the middle: 500 m³ each side, to 1e-6 m³.
- A design of one triangle indexes; one with none is refused as before.

## On the pit

Release build, 48 threads, the 42,223 relimiting solids cut by the mined-out topography in one run.

| | Before | After |
|---|---|---|
| The run | 44.2 s | 11.0 s |
| Of it, the cuts with their checks | 21.2 s, checks after | 9.8 s, checks in |
| Parts | 49,776 | 49,776 |
| Sample of 3,000 solids | 2.6 s | 0.95 s |

Every one of the 49,776 parts is the same as before: its volume to the last digit, its triangles, its vertices and its
thin pairs. The same one solid is left uncut.

The other runs that use every core, with only the change to how work is handed out:

| | Before | After |
|---|---|---|
| Import of the 42,223 solids, as read / with repair | 9.1 s / 9.6 s | 9.1 s / 9.4 s |
| PIT A pit designs from Deswik, 93,070 meshes | 68 s | 50 s |

The Deswik drawing is back to what it took before 0052, with every solid still equal to Deswik's own volume (0055).

## What this leaves

- **What the 9.8 s is now was not measured.** Likely the one solid cut again with the whole topography, and the 13 %
  spent handing small meshes to Manifold, which costs twice the boolean and isn't understood.
- **A solid with nothing of the surface near it** still has the whole surface closed under it.
- **The strip slice** does each face's boolean twice and slices solids one after another. It waits for Phase 4's
  blocking, where many solids are sliced in a run.
