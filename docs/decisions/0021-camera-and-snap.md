# 0021 — A view that works where you are looking, and a snap switch

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31)

## Why

Tim, on real surfaces: when you zoom in you want to work about a certain point, and it felt wrong to zoom
in and rotate on an object. The view turned about a centre left at the middle of the pit, and the wheel
zoomed towards it. Snapping was always on, with no way to switch it off before drawing. Both come before
Phase 2, because sections mean constant zooming in on one spot.

## Decision

1. **The wheel zooms where the cursor is**, in 3D and in plan. In 3D it slows as it nears what is under
   the cursor, and stops short of it.
2. **Left drag in 3D turns about the point under the cursor** when the button goes down: a visible string
   near it first, as strings draw on top, else the surface or solid there, whichever layer is showing. The
   camera and its target swing together about that point, so
   it stays where it is on screen and the view does not jump. Over empty space it turns about the view's
   centre. The turn stops just short of straight up or down.
3. **Right drag still pans**, and left drag still pans in plan view.
4. **Snap is a toolbar button and the S key**, on by default, and works mid-string or mid-drag. With it off,
   every point goes on the working plane: at the dragged vertex's RL while dragging, the last vertex's
   while drawing, else the RL at the centre of the view.
5. **You can see where a point will land before you click.** While drawing, or dragging a vertex, a ring
   sits on the snap point with a label beside the cursor naming it and its RL. Its colour says what it
   is: green a vertex, blue a segment, pink where strings cross, white a surface or solid, amber the
   working plane.
6. **The rotation point shows.** A gold ring sits on it while a drag turns the view.
7. **Picks and strings stay fast with a million vertices.** A string is held as runs of 64 segments, each
   with a sphere round it. A pick tests the spheres against the ray through the cursor and looks closely
   only at the runs it meets, and reaches only as far as the handle distance. A layer's strings draw as one
   set of lines, as a layer's solids draw as one mesh (decision 0019), and the selected string draws over
   them. An edit reloads only its own layer's lines.

These are all in the viewport: no engine change and no new command.

## Evidence

The release app on the whole pit (42,218 solids, 4.9 M triangles), `--bench --solids`, sending real pointer,
wheel and key events to the canvas:

- A 120 px by 60 px drag swung the camera 4,268 m, and the point under the cursor stayed there to 1e-10 px.
- Twelve wheel clicks closed from 5,497 m to 2,696 m, with that point 0.28 px from the cursor.
- In draw mode, a click snapped; S switched snap off; the next click landed on the working plane; S
  switched it on again.
- The hover label named what was under the cursor: a solid and its RL with snap on, the working plane and
  its RL with snap off. The gold ring sat on the point turned about, 0 m off, and cleared on release.
- A string held 50 m above the surface, so a pick that saw only surfaces would land elsewhere: a turn from
  its vertex pivoted on it to 0 m, and from its segment to 4e-10 m. Hovering named `vertex 2` and `segment 1`
  of the string, with their RL.
- Orbiting still holds 60 fps, and snapping takes 0.9 ms at p50.

The first version of string picking looked at every vertex and was far too slow on Tim's project. The scene
that showed it is now part of the check: a real pit's topography, 21,432 solids and 5,724 strings of about
a million vertices, 11.7 M triangles in all.

| On that scene | One draw and pick per string | Now |
|---|---|---|
| Orbit | 35 fps, p95 35 ms | **60 fps**, p95 17 ms |
| A pick, at every press and wheel tick | 92 ms at p50 (reaching 40 px) | **1.5 ms** at p50, 6 ms at worst |

The point under the cursor stayed there through a 27.9 km swing to 3e-12 px, and to 1 px through a zoom from
31.8 km to 15.6 km. Turning from a string's vertex or segment pivoted on it to 8e-10 m at most.

How it feels is checked by hand, not by this. The bench runs in 3D, so zooming in plan view is not covered.
