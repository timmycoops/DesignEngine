# 0096 — Triangulate: one surface through strings, each segment an edge

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20, the Surface tools plan tab, decision 4)

## Why

Making a surface from strings drawn by hand is something a planner does every day in Deswik. Phase 3 left the
triangulation with breaklines unbuilt by name (0053). Tim named the tool on 20 November ("Triangulate", as Deswik and
Vulcan call it): draw a polygon, and it makes a surface of it.

## Decision

1. **`surface.triangulate`** takes `strings` and makes one new surface through them, on `layer` ('triangulated' unless
   set). It's named `name`, or the one string's name and "surface", or "Triangulated". The strings are left as they were.
   - Every vertex of the strings is a vertex of the surface, at its own RL.
   - Every segment is kept as an edge (a breakline), so crests and toes stay sharp. It's spade's constrained Delaunay,
     already a dependency, worked in plan about a whole-metre origin.
   - No vertex is added, except where strings cross at one height. There the crossing becomes a vertex at that
     height, and the run says how many.
2. **What bounds it:**
   - The outermost closed strings picked, those not inside a larger one, each filling what's inside it. One closed
     string alone is a pad at its vertices' RLs: drawn at RL 100, a flat pad. Closed strings inside them are breaklines,
     not holes.
   - With none closed, the hull. Wherever an edge along the hull is longer than `longest` (200 m unless set) and isn't
     a string's segment, its triangle is peeled back, and on inwards.
3. **What's refused**, naming the strings, with nothing made:
   - Strings that cross or touch in plan at heights more than 1 mm apart. Each place is given with both RLs, the first
     ten of however many. Lines along each other count too.
   - A string partly outside every bounding closed string.
   - Anything that isn't a string, or too few vertices to make a surface.
4. **In the Surfaces panel** (0093), Triangulate is a mode, and a tool on the Walls row. It asks for the strings (what's
   picked already counts) and runs on a right click or the button, then asks again (0092). Its settings:
   - the longest hull edge, said to be unused when a closed string bounds it;
   - the name and layer.
5. **Holes** (an inner closed string left open) aren't in this. Trim in plan's outside (0095) cuts one after.

## Where

- `crates/zd-geom/src/tin.rs`: `triangulate_strings`, `TinBound`, `TinRefusal`, `TinCrossing`, `LONGEST_HULL_EDGE`,
  `SAME_HEIGHT`. Crossings at different heights are found on a grid of buckets in plan before spade sees the strings.
- `crates/zd-commands/src/builtin/surface_triangulate.rs`.
- `app/src/panels/SurfacesPanel.tsx`; `surfaces:triangulate` in `app/src/tools.ts`.

## Checked

| Test | Known answer | Tolerance |
|---|---|---|
| `a_toe_and_crest_make_the_batter_and_the_floor` (zd-geom) | A 100 m toe at RL 90 and a 120 m crest at RL 100, closed squares. The crest bounds it, with 8 vertices and 14,400 m². Every triangle is on the floor or the 45° batter. Against a level topography at RL 100, it's the frustum, 121,333.33 m³. | 1e-9 m; 1e-6 relative |
| `a_closed_string_alone_is_a_pad` | A five-sided polygon at RL 100: 3,600 m², its own area, every vertex at RL 100, 3 triangles | 1e-6 m² |
| `open_strings_make_the_hull_and_long_edges_are_peeled` | Three open lines 100 m long, 20 m apart: the hull, 4,000 m², nothing peeled. With a fourth line 300 m off: still 4,000 m², the long edges peeled. | 1e-6 m² |
| `strings_crossing_at_different_heights_are_refused_and_at_one_height_joined` | Two lines crossing at RL 100 and RL 90: refused, one place, at its coordinates with both RLs. Crossing at RL 100 both: one vertex made there at RL 100. | 1e-9 m |
| `a_string_outside_the_bound_is_refused` | A line 300 m past a closed square: refused, naming it | Exact |
| `a_toe_and_crest_make_the_pit` (zd-commands) | "Triangulated" on 'triangulated', 10 triangles, 14,400 m², bounded by 'Crest'. Against the topography, cut 121,333.3 m³ over 14,400 m². The strings are at their revisions. | 0.1 m³ |
| `a_polygon_alone_is_a_pad` | "Pad surface" on 'pads', 2,400 m², every vertex at RL 100 | Exact |
| `what_cant_be_triangulated_is_refused` | 'Crest A' and 'Toe B' crossing at RL 100 and 90, the place given; a string partly outside 'Limit'; a surface; nothing | Exact |
| `zd-app --bench --string-edits` | From the Walls row, a closed 80 by 30 m string at RL 100 picked, then a right click: 'Pad outline surface', 2,400 m². One Ctrl+Z takes it away. | — |

Timing in a release build, made-up data (`timing_triangulate`, ignored): 150 toes and crests, nested rings of 200
vertices each, 5 m apart in plan and 2 m in height. That's 30,000 vertices into 59,798 triangles in 53 ms.

In the app: `--string-edits` 50 of 50, `--shell` 62 of 62 (the tooltip check passed on a rerun, as is usual), `--workflow`
54 of 54, `--select` 29 of 29, `--clicks` 24 of 24. `--cut` gives the same as before.

Known limits:

- **No holes.** An inner closed string is a breakline. Trim in plan keeping the outside cuts the hole after.
- **spade can assert on strings that nearly overlap in plan.** That's caught and refused in words, not a crash.
- **PIT B isn't run.** No golden strings were picked to triangulate. The plan's check is on made-up toes and crests.
