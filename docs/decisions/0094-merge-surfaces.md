# 0094 — Merge surfaces, grids and solids: bottom or top, as a grid or an exact surface

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20, the Surface tools plan tab, decision 2)

## Why

Phase 3's gate check 7 wanted the face's surface merged with the topography; the merge was carried forward by name
(0053). Tim set how it works on 20 November:

> I'd think the best option is to have the ability to make a grid custom grid size - if it's small data you can make it
> a small grid and if it's very large ... Then have the option of creating a surface perfectly also so if I want to wait
> for a perfect surface I can.

A test the same day on made-up data settled the trade:

- **A 5 m grid:** about 30 ms, the pit volume within 0.023 % of exact, but walls up to 2.9 m off in height.
- **The exact surface:** 0.5 to 1.3 s.

## Decision

1. **`surface.merge`** takes `objects`: two or more surfaces, grids and solids, picked together.
   - `keep: bottom` (the default) keeps the lowest of them at each place: a pit shell or strip solids dug out of the
     topography.
   - `keep: top` keeps the highest: a dump laid on.
   - A solid counts by its floor (its faces facing down) for the bottom, and by its top for the top.
   - Where only one of them covers a place, it stands as it is.
2. **What comes out is a choice.** It's new, on `layer` ('merged' unless set), named `name` ('Merged bottom' or
   'Merged top'). What's read is left as it was.
   - **`output: grid`** (the default) samples each at every node of a lattice `cell` m square (5 m unless set) over them
     all. Each node takes the lowest or highest height there, and has none where nothing is. It's fast at any size. Every
     input a grid on one lattice, with no cell asked for, it's taken node by node.
   - **`output: surface`** is exact. Each input is closed down to a floor below them all, as the solid under it. The
     bottom keeps what's under every one of them where it is (and under the rest outside it), and the top what's under
     any. The new surface is the upper skin of what's kept, every crest, toe and berm where the inputs put them and
     joined exactly where they meet.
   - A grid of more than 100 million nodes is refused, with the cell that would do.
3. **In the Surfaces panel** (0093), Merge is a tool on the Walls row. It asks what to merge (what's picked already
   counts) and runs on a right click or the button, then asks again (0092). Started by the button with two or more
   picked, it runs at once, since the button always runs with what the panel has. Its settings:
   - Keep: Bottom or Top.
   - As: A grid or An exact surface.
   - The grid's cell: blank is 5 m, or the grids' own lattice.
   - Name and layer.
4. **The kit's asking** learns `byButton`: a loop knows the button started it, so it can run with what's picked rather
   than ask.

## Where

- `crates/zd-geom/src/merge_surfaces.rs`: `merge_to_grid`, `merge_to_surface`, `solid_skin`, `Envelope`, `MergeInput`.
- `crates/zd-commands/src/builtin/surface_merge.rs`.
- `app/src/panels/SurfacesPanel.tsx`, `surfaces:merge` in `app/src/tools.ts`, `Asker.byButton` in
  `app/src/design-system/kit/Asking.tsx`.

## Checked

| Test | Known answer | Tolerance |
|---|---|---|
| `a_pit_dug_out_of_the_topography_exactly` (zd-geom) | The bottom of a level topography at RL 100, 400 m square, and the pit design (100 m floor at RL 90, 45° to a 120 m crest). Less the topography, it's the frustum, 121,333.33 m³; it covers the whole 160,000 m²; its floor is at RL 90. | 1e-6 relative |
| `a_higher_topography_cuts_deeper_and_a_lower_one_keeps_the_design_above_it_out` | At RL 102 the bottom is the frustum plus a 2 m slab over the design's 200 m square. At RL 95 the top is 200² × 5 less the pit between 95 and 100 of fill. | 1e-6 relative |
| `solids_count_by_their_floor_and_their_top` | A 20 m block from RL 90 to 110 dug out: 4,000 m³. A 20 m dump from RL 100 to 112 laid on: 4,800 m³. As a 1 m grid, the block's floor in its middle and the topography outside. | 1e-6 m³; 1e-9 m |
| `as_a_grid_the_lowest_at_each_node_and_grids_alike_node_by_node` | At 5 m, 81 by 81 nodes: RL 90 on the floor, 95 half way up the wall, 100 past the design. Two grids on one lattice, node by node, lowest and highest. | 1e-9 m |
| `what_cant_be_merged_is_refused` | One input; a cell of nothing; a cell too fine, with the cell that would do | Exact |
| `the_pit_dug_out_exactly` (zd-commands) | "Merged bottom" on 'merged', a surface. Compared against the topography: cut 121,333.3 m³, fill 0.0, over 160,000.0 m². The two read are at their revisions. | 0.1 m³ |
| `as_a_grid_by_default` | A grid 81 by 81 at 5 m, RL 90 at the floor, 100 past the design, and said so | Exact |
| `a_dump_laid_on_as_the_top` | "Final landform" on its layer; against the topography, fill 4,800.0 m³ | 0.1 m³ |
| `what_cant_be_merged_is_refused` | One thing; the same twice; a string | Exact |
| `zd-app --bench --string-edits` | From the Walls row, the topography and a slope picked, then a right click: their bottom as a 5 m grid, 101 by 61 nodes. Picked again, An exact surface, the button: as a surface. Two Ctrl+Z take both away. | — |

Timing in a release build, made-up data (`timing_merge`, ignored): a 999,698-triangle topography 4 km square and a
100,352-triangle pit shell.

| As | Time | Out |
|---|---|---|
| Exact surface | 1,323 ms | 1,005,748 triangles |
| 2 m grid | 67 ms | 2,001 by 2,001 nodes |
| 5 m grid | 26 ms | 801 by 801 nodes |
| 10 m grid | 16 ms | 401 by 401 nodes |

In the app: `--string-edits` 46 of 46, `--shell` 62 of 62, `--workflow` 54 of 54, `--select` 29 of 29, `--clicks` 24
of 24. `--cut` gives the same as before.

Known limits:

- **PIT B isn't run.** The plan wants its strip faces merged with its topography and compared against it. No PIT B
  project holding both is set up for an opt-in test, and picking its objects means reading its layer names, which stay
  out of the work (client data). It wants a PIT B project with faces and topography together, named by an environment
  variable like the other golden tests.
- **The exact surface takes the whole of every input through Manifold.** A test on 20 November showed it takes a third
  of the time working only on the topography near the pit and carrying the rest over as it is. That's left for when a
  lease-sized topography makes it worth it.
- **A solid's skin must be single-valued in plan.** A solid that overhangs itself gives a skin that folds, and is
  refused where it folds.
- **The grid takes each node's height by sampling.** A crest or toe falls between nodes, as on any grid; that's what
  the exact surface is for.
