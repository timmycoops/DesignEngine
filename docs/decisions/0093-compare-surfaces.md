# 0093 — Compare surfaces: cut and fill, exact, with a column check

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20, the Surface tools plan tab, decisions 1 and 5: "The cut and fill
volumes makes sense", "Have a toggle for the thickness grid - don't need that by default", "These should be a new panel")

## Why

Phase 3 closed without a compare (0053). The Review of 10 November put surface compare in its order (item 8): it
measures a design against the topography or against Deswik's, and Phase 5 reconciliation needs it. It's the first
workstream of the Surface tools plan, after workstream 0 (0091, 0092).

## Decision

1. **`surface.compare`** takes `surface` (what's compared against: the topography, say) and `with` (what's compared:
   the design). Either may be a surface or a grid; a grid is triangulated, as everywhere.
   - **Cut** is where `with` lies below `surface`; **fill** is where it lies above. Each is in BCM.
   - It works inside a closed `string`, or with none, wherever both surfaces are.
   - It says the plan area both cover inside the string, worked from each surface's edge with its holes left out.
2. **Exact, as solids.** Cut is the solid below the first surface and above the second (`solid_between`, 0049). Fill is
   the same the other way round. It's the volume of record. Where either surface doesn't cover, there's nothing.
3. **A column check alongside** (engine rule 9). A 2.5D volume samples both surfaces at a node in the middle of each
   cell, `cell` m square (2 m unless set), inside the string. Its cut and fill are said beside the exact ones. If either
   is more than 0.1 % from the exact one, it's said as a warning (`surface.columns_differ`). A cell giving more than 50
   million columns is refused, with the cell that would do.
4. **The thickness grid is a toggle, off by default.** Switched on, `thickness: true` makes a grid of `with` less
   `surface` at each column: negative in cut, positive in fill, no value where either surface isn't or outside the
   string.
   - It's named "<with> less <surface>", on `layer` ('thickness' unless set).
   - It's drawn as a grid, so colour by RL shows it as an isopach in plan.
   - It's the one place a run makes a grid, as the plan said.
5. **The Surfaces panel,** a panel of its own, not inside Face design. Its first tool is Compare. Compare is on the Walls
   row, and on the Reserves row too. It asks in turn (0092):
   - the surface or grid to compare against;
   - the one compared;
   - a closed string to compare inside, or a right click for wherever both are;
   - then it runs, and asks again.

   Two surfaces picked when it opens fill the first two steps. Settings: the column check's cell, and the thickness
   grid (off, or Make it, with its layer).

## Where

- `crates/zd-geom/src/compare_surfaces.rs`: `compare_surfaces`, `SurfaceCompare`, `COLUMN_CHECK`, `MOST_COLUMN_NODES`.
- `crates/zd-commands/src/builtin/surface_compare.rs`.
- `app/src/panels/SurfacesPanel.tsx`; the `surfaces` panel in `app/src/workspace/panels.ts`; `surfaces` and
  `surfaces:compare` in `app/src/tools.ts`.

## Checked

| Test | Known answer | Tolerance |
|---|---|---|
| `a_pit_under_a_level_topography_is_a_frustum_of_cut` (zd-geom) | A pit with a 100 m floor at RL 90 and 45° walls to a 120 m crest, under a level topography at RL 100: cut 10/3 × (100² + 120² + 100 × 120) = 121,333.33 m³, fill 0, over 40,000 m² wherever both are. Inside the crest the same, over 14,400 m². The columns at 1 m are within 0.1 %. | 1e-6 relative; 1e-6 m² |
| `a_topography_2_m_higher_adds_the_slab_and_one_at_rl_95_cuts_and_fills` | At RL 102: the frustum and a 2 m slab 120 m square. At RL 95: cut 55,166.67 m³ (the pit below 95) and fill 5,833.33 m³ (120² × 5 less the pit between 95 and 100), with the columns within 0.1 %. | 1e-6 relative |
| `its_thickness_grid_is_the_second_less_the_first` | -10 m on the floor, 0 outside the crest, -5.5 m on the wall 4.5 m in from the floor's edge | 1e-9 m |
| `what_cant_be_compared_is_refused` | Surfaces apart in plan; a cell of nothing; a cell too fine for the area, with the cell that would do | Exact |
| `a_pit_against_the_topography_is_its_cut` (zd-commands) | "'Pit design' against 'Topography' wherever both are: cut 121,333.3 m³, fill 0.0 m³, net cut 121,333.3 m³, over 40,000.0 m² both cover". The column check is said, with no warning; nothing made; the surfaces at their revisions. | 0.1 m³ |
| `inside_a_string_with_the_thickness_grid` | Inside 'Crest' against RL 95: cut 55,166.7, fill 5,833.3, net cut 49,333.3 m³ over 14,400.0 m². The grid "Pit design less Topography" is on 'isopach': 120 by 120 at 1 m from the crest's corner, its floor nodes 5 m down. | 0.1 m³; 1e-9 m |
| `what_cant_be_compared_is_refused` | A surface with itself; surfaces apart; a cell of nothing | Exact |
| `zd-app --bench --string-edits` | From the Walls row, the topography and a slope picked: it asks for the string; a right click gives cut 25,000.0 m³ and fill 25,000.0 m³, the slope 5 m under the topography to 5 m over it across the 100 m they share | 0.1 m³ |

Timing in a release build, made-up data (`timing_compare`, ignored): a rolling topography of 999,698 triangles, 4 km
square, against a 100,352-triangle pit design 1.2 km square, wherever both are, with the thickness grid. Exact: 664
ms. Columns at 5 m: 11 ms for 57,600 columns, 0.006 % from the exact.

In the app: `--string-edits` 44 of 44, `--shell` 62 of 62 (the Panels menu lists Surfaces), `--workflow` 54 of 54,
`--select` 29 of 29, `--clicks` 24 of 24.

Known limits:

- PIT B isn't compared yet. The plan runs it with the merge (workstream 2): the strip faces merged with the topography,
  and the merged surface compared against it.
- The column check samples cell middles. Along a string that cuts cells it isn't exact by partial cells, so a small
  area inside a coarse cell can show more difference than the 0.1 % the warning allows; a finer cell closes it.
- The thickness grid is drawn as a grid at its values' RLs (near 0). Seen in plan, coloured by its own range, it reads as
  an isopach.
