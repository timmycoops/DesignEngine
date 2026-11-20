# 0095 — Trim in plan: strings, surfaces, solids and grids cut by a closed string

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20, the Surface tools plan tab, decision 3)

## Why

Trimming a surface to a boundary is something a planner does every day in Deswik, and Phase 3 left trim unbuilt by
name (0053). Tim widened it on 20 November:

> Can trim just cut everything in plan view (solids, surfaces, lines, points) or is that a much bigger job for later?

It isn't a bigger job. Each kind already has a cut that suits it: Clip for strings (0083, workstream 6), the polygon cut
for solids (0062), and Merge's closing of a surface into the solid under it (0094).

## Decision

1. **`object.trim`** takes `objects`, any mix of strings, surfaces, solids and grids, and `by`, a closed string.
   `keep` is `inside` (the default) or `outside`. It runs as one run with one undo. Each kind is cut by its own rule:
   - **A string** as Clip does it, cut at its own heights. Its first part kept is its next revision. Any further parts
     are new strings on its layer, named "<name> part 2" and on, carrying its attributes.
   - **A surface** is cut exactly along the string at its own heights. It's closed down to a floor below it as the
     solid under it, that solid is cut by the string's prism in Manifold, and the upper skin of what's left is the
     surface. Each new vertex where the string crosses a triangle lies on that triangle. Keeping the inside, only the
     triangles near the string go through the cut.
   - **A solid** is cut vertically along the string by `cut_solid_by_polygons` with the one polygon, with no coverage
     check. It stays closed, and its pieces add up to it.
   - **A grid** has its nodes on the side dropped set to no value. Nothing is filled (0063). A node within 1 µm of the
     string counts as on it and is kept whichever side is kept. What's left is cropped to the nodes that still have a
     height, on the same lattice.
2. **What happens to things a cut doesn't divide:**
   - A thing wholly on the side kept is left as it was, and the run says so.
   - A thing wholly on the side dropped refuses the run, named, with nothing changed, as Clip does (0078).
   - Something that isn't a string, surface, solid or grid refuses the run, as does a string that isn't closed or
     closes round no area, or the string trimmed by being among those picked.
3. **In place or as new** works as every changing tool does (0091). In place, each thing's next revision. As new, each
   kept is a copy on the layer named, with its own name and attributes, and the original is left as it was.
4. **In the Surfaces panel** (0093), Trim in plan is a mode, and a tool on the Walls row ("Trim plan"). It asks in turn
   (0092):
   - first, what to trim, where what's picked already counts;
   - then a click on the closed string;
   - then it runs, and asks again.

   The click on the string picks it as well, so the panel takes what to trim as the prompt opens, and the project as it
   stands when the click lands. Its settings are Keep (Inside or Outside) and the Result control. A workflow step
   added from it asks for the click as a line across, as Clip's does.
5. **Points** join when the point object exists, in a plan of their own. Trimming by another surface (the part above or
   below it) is for later.

## Where

- `crates/zd-geom/src/trim_in_plan.rs`: `trim_string_in_plan`, `trim_surface_in_plan`, `trim_solid_in_plan`,
  `trim_grid_in_plan`, `trim_ring`, `InPlan`, `mesh_plan_area`.
  - `upper_skin` is shared with Merge, in `merge_surfaces.rs`.
  - Clip's two "lies wholly" refusals are named constants in `string_cut.rs`.
- `crates/zd-commands/src/builtin/trim_in_plan.rs`.
- `app/src/panels/SurfacesPanel.tsx`; `surfaces:trim` in `app/src/tools.ts`.

## Checked

| Test | Known answer | Tolerance |
|---|---|---|
| `a_plane_trimmed_to_a_square_inside_and_outside` (zd-geom) | A plane rising 1 in 10, 400 m square, trimmed to a 100 m square set off its triangles' corners. Inside: 10,000 m² in plan, every vertex on the plane and inside the square. Outside: 150,000 m² of 160,000. | 1e-6 m²; 1e-9 m |
| `the_pit_trimmed_keeps_its_volume_inside_and_the_rest_outside` | The pit design trimmed to its own crest. Compared against the level topography, inside it's the frustum, 121,333.33 m³, over 14,400 m². Outside it's nothing, over 25,600 m². | 1e-6 relative |
| `a_surface_wholly_on_one_side_is_untouched_or_dropped` | A square round it: inside untouched, outside dropped. A square away from it: the other way round. | Exact |
| `a_solid_cut_vertically_closed_and_exact` | A 100 m cube from RL 50 to 150 by a 50 m square over its corner: 250,000 m³ inside and 750,000 outside, each closed, adding up to the cube | 1e-6 m³ |
| `a_grid_loses_its_nodes_on_the_side_dropped_and_keeps_those_on_the_line` | A 1 m grid, 101 by 101 nodes, and a square on nodes 20 to 60. Inside: 41 by 41 nodes, cropped to them, the heights as they were. Outside: all but the 39 by 39 strictly inside, on the lattice as it was. | Exact; 1e-9 m |
| `a_string_as_clip_cuts_it` | A line from RL 100 to 120 across a square: its 100 m inside kept, from RL 105 to 115. Wholly outside: dropped, or untouched kept outside. | 1e-9 m |
| `a_ring_with_no_area_is_refused` | A closed string with no area, and an open string | Exact |
| `every_kind_trimmed_inside_in_one_run_and_one_undo` (zd-commands) | The topography, the pit design, their bottom as a 5 m grid, a block half over the crest's edge, and a line across, all trimmed inside the crest in one run. The design against the topography is 121,333.3 m³ over 14,400 m². The grid keeps 625 of 6,561 nodes, 25 by 25. The block keeps 4,000 of 8,000 m³. The line runs RL 104 to 116. One undo puts every revision back. | 0.1 m³; 1e-9 m |
| `outside_as_new_leaves_the_originals` | As new on 'trimmed', outside: three copies named as they were, the originals at their revisions. The topography keeps 145,600 m², and the design compared against it gives nothing over 25,600 m². | 0.1 m³ |
| `left_as_it_was_and_what_is_refused` | A block wholly inside is left and said. One wholly outside refuses the run, named, with nothing changed. Refused too: all wholly inside, an open string, the string among those picked, nothing picked. | Exact |
| `zd-app --bench --string-edits` | From the Walls row, the topography and the slope picked, then a click on an 80 by 100 m square across the topography's east edge: 5,000 m² and 8,000 m² kept. One Ctrl+Z puts both back. | — |

Timing in a release build, made-up data (`timing_trim`, ignored): a 999,698-triangle topography 4 km square, trimmed
to a 1 km square.

| What | Time | Out |
|---|---|---|
| Surface, inside | 103 ms | 62,661 triangles |
| Surface, outside | 738 ms | 937,046 triangles |
| 2 m grid, 2,001 by 2,001, inside | 12 ms | 251,001 nodes |
| 2 m grid, outside | 16 ms | 3,557,400 nodes |

In the app: `--string-edits` 48 of 48, `--shell` 62 of 62, `--workflow` 54 of 54, `--select` 29 of 29, `--clicks` 24
of 24. `--cut` gives the same as before. The Merge check now undoes its grid before picking again, because the grid
lay over the topography and the pick could land on it.

Known limits:

- **PIT B isn't run**, for the same reason as 0094: no PIT B project with its topography and faces together is set up
  for an opt-in test.
- **Keeping the outside takes the whole surface through Manifold**, so its time follows the surface's size. Carrying
  over as it is what lies well clear of the string is left for when a lease-sized surface makes it worth it.
- **A surface's skin must be single-valued in plan**, as for Merge. One that overhangs itself is refused where the
  closing fails.
