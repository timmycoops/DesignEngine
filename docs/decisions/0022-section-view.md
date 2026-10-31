# 0022 — The section view

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31; the Phase 2 plan tab of the Build Plan)

## Why

Phase 2's gate is a section through the pit that reads as clearly as in Deswik. Tim's standard is a plane
through two points, with a very narrow width: he wants the actual section, not what lies either side of it.
Clipping on the graphics card only hides what is in front, so the engine cuts instead: the lines stay exact
in mine grid coordinates, and can be snapped to and exported.

## Decision

1. **`slice_mesh` cuts a triangulation by a vertical plane into exact lines.** An edge is crossed once,
   whichever triangles share it, so neighbours meet at the same point to the bit and the segments chain into
   whole lines. A vertex on the plane counts as on its positive side, so a closed solid always gives closed
   loops. Lines run the way the triangles are wound, so a hole runs against its outer edge, ready for fills.
2. **Two points make a vertical plane.** Inclined sections wait.
3. **Surfaces, solids and grids are cut with no width.** Strings cross a thin slice rather than lie in it, so
   those within 1 m either side are drawn flat onto it (`string_near_plane`).
4. **A section is view state, like the seam horizon on show:** no command, nothing to undo. The Section
   button picks two points, and the section takes over from the view it was picked in. Plan view, 3D and
   the snap, draw and select tools come back when it closes. Strings cannot be edited in section yet.
5. **What is cut:** every visible layer. Solids cut as one mesh per layer, as they draw, and seam models as
   each working horizon's roof and floor. What takes work to make is kept until its geometry changes, so
   a step costs only the cut. Each mesh kept carries an index, groups of 2,000 triangles with a bounding
   circle each, so a cut looks only at the groups the plane can touch. The list of what to cut is kept against
   `Project::generation`, which moves with every run, undo, redo and layer shown or hidden, because finding
   tens of thousands of solids again at every step took twice as long as cutting them.
6. **Lines have real weights,** drawn with three.js's Line2 on the WebGPU renderer: 2.5 px for surfaces,
   1.5 px for solids and seams, 2 px for strings, and a layer can set its own in the Layers panel while a
   section is open. Weights are display state and are not saved. Keeping them on the layer is a stopgap:
   colouring, filtering and attributes come later, and the weight may move there.
7. **It frames the stretch between the two points,** with the first on the left. `[` and `]` or the toolbar
   step the plane square to itself, 25 m by default, keeping the view.
8. **A plan map stays in the corner while a section is open,** so the section is never lost. It shows the
   surfaces and strings from above, the section line with its A and B ends, a tick for the way it looks, the
   step lines either side when they stand far enough apart to read, and how far it has moved from the start.
   A press or drag on the map moves the section to the pointer in whole steps, so it lands on the same
   sections the keys do. A drag held near the map's edge runs on past what is in view, the faster the closer
   to the edge, and when it ends the map eases back round the section. Stepping keeps the section near the
   map's centre, with the plan ahead in view. The map slides over a picture of the plan three times its width,
   so it follows at every step without drawing the scene, and the picture is taken again when it nears its edge.
9. **An RL grid sits over the section,** with RL lines and distance along the section from A, read from the
   section camera each frame so a line sits exactly where that RL and distance are drawn. The spacing is the
   smallest 1, 2 or 5 times a power of ten that keeps lines 64 px apart, the same both ways so the grid stays
   square on a true-scale section, and labels gain decimals as the spacing drops below a metre. RLs read on
   both sides, and distances along the top with A and B marked, because the plan map takes the bottom left
   and A falls under it. A step moves the plane, not the distances or RLs, so it leaves the grid alone.
10. **Solids fill between their cut lines,** in their layer colour at 30 % so the lines stand out, with a Fill
    button to go back to lines only. A hole goes with the smallest outer edge round it and each outer edge
    fills on its own, so solids that touch or overlap on a layer each fill as themselves. The area filled must
    match the area the loops enclose to one part in a million. A loop that fails, such as one that crosses
    itself, stays unfilled and the status line counts it, so nothing is coloured in that isn't there.
11. **Seams fill between each working horizon's own roof and floor.** Those are surfaces, so they cut into open
    lines, not loops: the fill walks both along the section from breakpoint to breakpoint and takes in what
    lies between them wherever both exist and the roof is above the floor, stopping exactly where one passes
    through the other. A parent does not fill, as its daughters do.

## Evidence

- A 10 m cube: one closed square of 100 m² and 40 m perimeter (to 1e-9), through its middle, along its
  diagonal through two vertical edges (141.42 m²), and along a face. A plane that misses gives nothing.
  A hollow runs against its outer edge, and the two loops net 64 m². A sloping grid gives one open line, every
  point on the surface to 1e-9 m. A string crossing at 45° shows 2 m of a 2 m width, flat on the plane.
- The app's own layer: two cubes on a layer cut to 80 m of line, a hidden layer is not cut, and only the
  near string shows. Stepping reuses what was made, until a solid is added.
- PIT A, 41 sections 25 m apart:

  | Mesh | Triangles | Index built | Section | Result |
  |---|---|---|---|---|
  | Topography | 8.06 M | 97 ms | 2.4 ms p50, 3.6 ms worst | one line |
  | Solids | 3.59 M | 59 ms | 5.7 ms p50, 8.4 ms worst | 485 lines, all closed |

  Every point lies within 6e-11 m of the plane. Without the index these took 8.6 and 8.2 ms at p50. On a
  test mesh the index finds every crossing a full scan finds.
- The release app on the full scene, through the real toolbar and canvas: 11.7 M triangles, 21,469 solids and
  5,724 strings of about a million vertices. A step takes 29 ms at p50 from request to frame submitted, about
  20 ms of that in the engine, and 47 ms at worst across twelve runs, so the gate's 50 ms holds on every step.
  Before the index it was 46 ms at p50 and 63 ms at worst. The section holds 60 fps.
- The plan map in the same run, by pointer events on the canvas: a press moved the section in whole 25 m
  steps, and moving the pointer at the same place left it there, so it goes to the pointer and the map holds
  still. Held at the map's edge for 30 frames it ran on 2.8 km on the solids alone and 6.3 km on the full scene
  and ended on the map, and both ends of the section stayed on the map through five 2 km steps. The saved
  pictures show the topography, contours and pit outlines under the section, step lines at 200 m, and the
  plan moved on after those steps. A new picture costs under 10 ms of script and the next frame follows as
  usual, so what looked like a slow map was the old map waiting to move. Its place on screen is checked as well, as
  those pictures can't show it: 240 px in the corner, the middle of the viewport still the section, and gone
  before a section opens and after it closes.

- The RL grid on the full scene, by the real wheel and pan controls. Read off the labels as a planner would,
  the ends of 9,708 cut points match the engine's own mine grid RLs and distances to 0.12 mm. Through 350 wheel
  ticks and a pan, every line sits within 4e-6 px of where the camera draws that RL and distance, the spacing
  runs from 0.05 m to 2 km with lines 64 to 158 px apart, and a drawing takes at most 1.4 ms.

- Seam fills, to 1e-7 m2: a flat seam fills 1,000 m2, a roof dipping through its floor stops at the crossing
  at 250 m2, and a gap in the roof leaves 800 m2. Breakpoints that don't match, and lines running either way,
  match the trapezium rule. A floor above its roof fills nothing. Cut from two sloping grids, a seam fills its
  exact 1,200 m2, and a model of a parent and two daughters fills 3,000 m2, with none of the parent's.
- With seam models loaded, in the release app:

  | Pit | Scene | First section | Step p50 | Step worst | Engine p50 |
  |---|---|---|---|---|---|
  | PIT A | 11.7 M triangles, 21,469 solids, 5,724 strings, 15 horizons | 245 ms | 28 ms | 30 ms | 7.8 ms |
  | PIT B | 8.4 M triangles, about 42,000 solids, 10 horizons | 316 ms | 46 ms | 47 ms | 12.5 ms |

  Before the list of what to cut was kept, PIT B stepped in 62 ms at p50 and 78 ms at worst, with 23 ms of its
  35 ms in the engine going on walking the objects. One loop in PIT B's section stays unfilled and is counted.
- Fills, to 1e-7 m2: a 10 m cube fills 100 m2, a hollow 64 m2, an L 75 m2 with nothing in its notch, and an
  island in a hole 68 m2. Touching and overlapping solids each fill whole, a hole goes with the smallest edge
  round it, and a 4,000-point loop matches its area. A bow tie and a hole with no edge round it are counted,
  not filled. PIT A's solids, 41 sections: all 485 loops a section fill, 6,741 triangles, 2.9 ms at p50 and
  3.9 ms worst. With fills on in the release app, a step is 30 ms at p50 and 37 ms at worst at 60 fps.

Tim tried the map by hand on PIT A's solids: the drag, the run past the data and the step size.

Not covered: seam models in a section have only been tested through their parts, not on screen, and the line
weight input is untested by the bench.
