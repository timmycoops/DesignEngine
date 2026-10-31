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
   their working horizons' roofs and floors. What takes work to make is kept until its geometry changes, so
   a step costs only the cut.
6. **Lines have real weights,** drawn with three.js's Line2 on the WebGPU renderer: 2.5 px for surfaces,
   1.5 px for solids and seams, 2 px for strings, and a layer can set its own in the Layers panel while a
   section is open. Weights are display state and are not saved.
7. **It frames the stretch between the two points,** with the first on the left. `[` and `]` or the toolbar
   step the plane square to itself, 25 m by default, keeping the view.

## Evidence

- A 10 m cube: one closed square of 100 m² and 40 m perimeter (to 1e-9), through its middle, along its
  diagonal through two vertical edges (141.42 m²), and along a face. A plane that misses gives nothing.
  A hollow runs against its outer edge, and the two loops net 64 m². A sloping grid gives one open line, every
  point on the surface to 1e-9 m. A string crossing at 45° shows 2 m of a 2 m width, flat on the plane.
- The app's own layer: two cubes on a layer cut to 80 m of line, a hidden layer is not cut, and only the
  near string shows. Stepping reuses what was made, until a solid is added.
- PIT A, 41 sections 25 m apart:

  | Mesh | Triangles | Section | Result |
  |---|---|---|---|
  | Topography | 8.06 M | 8.6 ms p50, 11 ms worst | one line |
  | Solids | 3.59 M | 8.2 ms p50, 10 ms worst | 485 lines, all closed |

  Every point lies within 6e-11 m of the plane.
- The release app on the full scene, through the real toolbar and canvas: 11.7 M triangles, 21,469 solids and
  5,724 strings of about a million vertices. A step takes 46 ms at p50 from request to frame submitted, 34 ms of
  that in the engine, and 63 ms at worst, so the gate's 50 ms holds typically and not every time. The section
  holds 60 fps.

Not covered: seam models in a section have only been tested through their parts, not on screen, and the
line weight input is untested by the bench.
