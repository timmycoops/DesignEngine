# 0021 — A view that works where you are looking, and a snap switch

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31)

## Why

Tim, on real surfaces: when you zoom in you want to work about a certain point, and it felt wrong to zoom
in and rotate on an object. The view turned about a centre left at the middle of the pit, and the wheel
zoomed towards it. Snapping was always on, with no way to switch it off before drawing. Both come before
Phase 2, because sections mean constant zooming in on one spot.

## Decision

1. **The wheel zooms where the cursor is**, in 3D and in plan. In 3D it slows as it nears the surface under
   the cursor, and stops short of it.
2. **Left drag in 3D turns about the point under the cursor** when the button goes down: the surface or
   solid there, whichever layer is showing. The camera and its target swing together about that point, so
   it stays where it is on screen and the view does not jump. Over empty space it turns about the view's
   centre. The turn stops just short of straight up or down.
3. **Right drag still pans**, and left drag still pans in plan view.
4. **Snap is a toolbar button and the S key**, on by default, and works mid-string or mid-drag. With it off,
   every point goes on the working plane: at the dragged vertex's RL while dragging, the last vertex's
   while drawing, else the RL at the centre of the view.

These are all in the viewport: no engine change and no new command.

## Evidence

The release app on the whole pit (42,218 solids, 4.9 M triangles), `--bench --solids`, sending real pointer,
wheel and key events to the canvas:

- A 120 px by 60 px drag swung the camera 4,268 m, and the point under the cursor stayed there to 1e-10 px.
- Twelve wheel clicks closed from 5,497 m to 2,696 m, with that point 0.28 px from the cursor.
- In draw mode, a click snapped; S switched snap off; the next click landed on the working plane; S
  switched it on again.
- Orbiting still holds 60 fps, and snapping takes 0.9 ms at p50.

How it feels is checked by hand, not by this. The bench runs in 3D, so zooming in plan view is not covered.
