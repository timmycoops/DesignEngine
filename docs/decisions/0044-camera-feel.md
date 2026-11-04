# 0044 — A camera that handles like Spry's

Date: 2026-11-05 · Status: plan agreed (Tim, 2026-11-05); accepted once the side by side passes

## Why

Tim uses Spry every day, and next to it our camera feels wrong in two ways: **orbit** and **pan**. Zoom and
the field of view are not the complaint, but he wants zoom to push through a surface, and a perspective
switch.

## What we have now (0021, `app/src/viewport/Viewport.ts`)

| | Today |
|---|---|
| Controller | three.js `OrbitControls`, with its left button taken off and our own turn added on top |
| Orbit | Left drag in 3D. It turns about what is under the cursor when the button goes down: a string within 12 px first, else the nearest surface or solid on the ray (Rust BVH, `snap_ray`, about 1.5 ms). Over empty space it turns about the view centre. A full turn is a drag of one viewport height. Up stays vertical, and it stops 0.6° short of straight up or down. The pick is asynchronous, so the first move can turn about the centre before the pick comes back |
| Pan | Right drag (`OrbitControls`), and left drag in plan and section. It moves at the depth of the view centre, not the point grabbed, so near ground slides faster than the cursor and far ground slower |
| Zoom | Wheel, towards the cursor. Each tick the view centre is first set to the depth under the cursor, then `OrbitControls` dollies a share of the way to it. So it slows near a surface and never reaches it. Middle drag also dollies |
| Projection | Perspective, 45° vertical, in 3D. Orthographic in plan (north up, left pans) and section (square on, RL up). No orthographic 3D |
| Clip planes | Near = distance to the view centre / 5000 (at least 5 cm), far = that distance + 6 × the scene radius, on a reversed depth buffer |
| Motion | Instant, no damping |
| Keys | S snap. Nothing for the camera |
| Ties | Strings switch the controls off while a vertex drags, and use the view centre's RL for the working plane. Click and drag tell apart by a few pixels of slop. The section view slides its camera with the plane. The bench (`--bench --solids`) checks the point under the cursor holds while turning (1e-10 px) and zooming (0.3 px) |

How the others do it:
- **Spry** (26.5 help, Design Window shortcuts): rotate is Z + left drag, or middle + left, "around the mouse
  cursor". Pan is Space + left drag, or middle + right. Zoom is the wheel or I and O. F zooms to extents, T is
  plan view, R resets the view. Left drag is a selection box. Keys can be changed in Keyboard Settings, which
  also has a rotation mode and an invert-wheel option. The help says nothing on perspective, field of view or
  damping.
- **Deswik** (API reference): a perspective switch (`PerspectiveView`) and a fixed rotation centre that can
  be shown (`RotationCentre`, `ShowRotationCentre`). That is the set-a-centre model, not Spry's turn about
  the cursor.
- **Vulcan**: no source here. Tim knows it best; nothing below depends on it.

## What Tim asked for (2026-11-05)

1. **Pivot is surer.** Turn about what is under the cursor. Over empty space, turn about the **nearest
   thing to the cursor**, never the view centre.
2. **Pan sticks.** The point you grab stays under the cursor the whole drag, near or far.
3. **Zoom slows, then pushes through.** Each tick goes a share of the way to what is under the cursor, but
   carries on past it once close, into a cavity or out the far side.
4. **Perspective, with a switch** to orthographic in 3D. Plan and section stay flat. The field of view stays
   45°.
5. **Stop dead.** No glide.
6. **Mouse layout picked in settings**: Spry's, or ours.
7. **Keys you can change**: hold to turn about the cursor (Spry's Z), zoom to extents (F), plan view (T).

## Decision

### 1. Our own controller replaces `OrbitControls`
A `CameraController` in a new `app/src/viewport/camera.ts` handles every pointer, wheel and key input for
the 3D, plan and section cameras. `OrbitControls` only gets in the way now: we already override its orbit,
and pan and push-through zoom both need what is under the cursor, which it has no way to use. The
controller keeps the two things other code uses, `enabled` and `target`, so strings, sections and the bench
change little. `Viewport.ts` loses its turn and wheel code, and only wires the controller in. That keeps
merges with the RL colour work, which edits `addMesh` there, small.

### 2. What the camera works on: one pick at the press
- On a press (turn or pan) and on each wheel tick, the controller asks for the point under the cursor
  through the existing path: strings in the viewport, then `snap_ray` in Rust (parry3d BVH, f64). It never
  ray casts the mesh in JS, and it never picks per frame.
- **A turn or pan waits for its pick** before it moves (about 1.5 ms, at most one pointer move), so it never
  starts about the wrong point.
- **Over empty space: the nearest thing.** A new bridge command, `snap_rays`, casts a fan of rays in rings
  round the cursor (8, 16, 32, 64, 128, 256 px, 8 rays each) in one call, and returns the hit nearest the
  cursor on screen. Strings are checked the same way in the viewport. Between a string near the cursor and
  the surface under it, the one nearer the camera wins (today the string always wins). With nothing within 256 px it falls
  back to the last pivot. This is a bridge command in `app/src-tauri`, not a typed command: no change to
  `zd-commands`.
- The gold ring shows on the point, as now.

### 3. Orbit
Same as now about the picked point: camera and target swing together so the point stays put on screen,
up stays vertical, and it stops just short of straight up and down. The turn rate stays one full turn per
viewport height until the side-by-side says otherwise (a constant, tuned in PR 6).

### 4. Pan that sticks
The grabbed point P sets a plane square to the line of sight through P. Each move puts P back under the
cursor on that plane by moving the camera and target together. That is exact in perspective and in
orthographic. In plan and section P is just the point on the view plane.

### 5. Zoom that pushes through
Each wheel tick moves the camera along the ray through the cursor by
`step = max(share × distance to the hit, floor)`, with `share` about 15 % and the floor a few pixels'
worth at the hit. Far off it slows as now. Close in the floor takes over, the camera goes past the surface,
and the next tick picks whatever is beyond. Zooming out uses the same share the other way. With no hit it
uses the last depth. The near plane follows the distance to the hit, not to the view centre, so a surface
just in front doesn't clip. In plan, section and orthographic 3D the wheel scales the view about the cursor.

### 6. Orthographic in 3D
A toolbar switch and a key (P by default). Switching keeps the scale at the pivot depth (half height =
distance × tan 22.5°), so the view doesn't jump. Code that now asks `flatView` to mean "no perspective" and
code that means "plan or section" are split, so snapping, markers and handle sizes stay right in
orthographic 3D.

### 7. Mouse layouts and keys (a setting, saved per machine)
| | Spry layout | Our layout (default today) |
|---|---|---|
| Turn about the cursor | Middle + left, or hold Z + left | Left drag |
| Pan (sticks) | Middle drag, middle + right, or hold Space + left | Right drag |
| Select, draw, drag a vertex | Left click / drag | Left click |
| Zoom | Wheel | Wheel |

Keys, changed in a small View controls dialog: hold to turn (Z), hold to pan (Space), zoom to extents (F),
plan view (T), orthographic (P). A new key is refused if it clashes with S, Enter, Backspace, Escape, the
section's [ and ], or another camera key. Keys work only while the cursor is over the viewport and no text
box has the focus. The settings live in `localStorage`, like the panel layout, not in the project.

### 8. Not now
Damping. A navigation cube. Saved views and R (reset). A field-of-view setting. A fixed rotation centre
(Deswik's). Left-drag selection box in the Spry layout (left drag does nothing there until there is one).

## How it is judged

- **Bench**, on the 11.7 M-triangle scene and the 4.9 M whole pit, through real pointer and wheel events:
  the turned-about point stays under the cursor (below 1e-6 px); a grabbed point stays under the cursor
  through a 500 px pan, near and far (below 0.5 px); 40 wheel ticks over a surface pass through it and the
  next pick is the surface beyond; a press over empty space beside a solid pivots on the solid; the
  orthographic switch keeps the scale at the pivot within 1 %; orbit holds 60 fps. Timings: pick at press
  p50 and p95, the ray fan p50 and p95 (budget 5 ms), triangles in the scene.
- **Rust test** for `snap_rays`: a flat synthetic surface with a square hole; a fan centred over the hole
  returns the hit on the hole's nearest edge, with its known screen distance.
- **Tim, side by side with Spry on the same pit** (PIT B), Spry layout on: turn about a point on the
  highwall, about a string, and over the sky beside the pit; pan near and far; zoom into a ramp and out
  through the pit floor; plan and section; the keys. He says what still feels off; constants are tuned in PR 6.

## Pull requests (each off `main`, small)

1. **Own controller, same feel.** `camera.ts` replaces `OrbitControls` with today's behaviour, plus the
   turn waiting for its pick. 0021's bench checks still pass. No visible change except no start about the
   wrong point.
2. **Pan that sticks** in 3D, plan and section, with its bench check.
3. **The surer pivot:** `snap_rays` in `app/src-tauri` with its Rust test, the fallback to the nearest
   thing then the last pivot, and the bench check.
4. **Zoom that pushes through**, and the near plane following the hit.
5. **Orthographic 3D switch**, with the `flatView` split.
6. **Spry layout and the keys:** the View controls dialog, `localStorage`, clash checks. Then the side by
   side with Spry, tuning, and this record goes to accepted.

PRs 2 to 5 each depend only on 1 and touch separate parts of `camera.ts`, but they are small enough to run
one after another.

## Settled with Tim

- Between a string near the cursor and the surface under it, **the one nearer the camera** is the pivot.
- In the Spry layout, **middle drag on its own pans**, as middle + right does.
