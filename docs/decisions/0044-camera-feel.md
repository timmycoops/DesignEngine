# 0044 — A camera that handles like Spry's

Date: 2026-11-05 · Status: plan agreed (Tim, 2026-11-05); accepted once the side by side passes

## Why

Tim uses Spry every day, and next to it our camera feels wrong in two ways: **orbit** and **pan**. Zoom and
the field of view are not the complaint (after PR 1 he called zoom and its speed "amazing"), but he wants a
perspective switch.

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
3. **Zoom stays as it is.** Each tick goes a share of the way to what is under the cursor and slows near it.
   It never goes through a surface: Tim checked Spry, and you can't zoom through one there either.
4. **Perspective, with a switch** to orthographic in 3D. Plan and section stay flat. The field of view is
   **narrow, 6° by default, and a setting** (Tim, 2026-11-05, after PR 1: heights looked flatter than in Spry
   at 45°; Spry's Z scale is 1, so it isn't a stretch).
5. **Stop dead.** No glide.
6. **Mouse layout picked in settings**: Spry's, or ours.
7. **Keys you can change**: hold to turn about the cursor (Spry's Z), zoom to extents (F), plan view (T).

## Decision

### 1. Our own controller replaces `OrbitControls`
A `CameraController` in a new `app/src/viewport/camera.ts` handles every pointer, wheel and key input for
the 3D, plan and section cameras. `OrbitControls` only gets in the way now: we already override its orbit,
and pan needs what is under the point grabbed, which it has no way to use. The
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
viewport height until the side-by-side says otherwise (a constant, tuned in PR 5).

### 4. Pan that sticks
The grabbed point P sets a plane square to the line of sight through P. Each move puts P back under the
cursor on that plane by moving the camera and target together. That is exact in perspective and in
orthographic. In plan and section P is just the point on the view plane.

### 5. Zoom as it is
Each wheel tick moves the camera along the ray through the cursor by 5 % of the distance to what is under
it, so it slows near a surface and stops short of it, as Spry's does. Zooming out uses the same share the
other way. With no hit it uses the last depth. In plan and section the wheel scales the view about the
cursor, and in orthographic 3D it will too. The share is a constant, left alone unless the side by side
says otherwise.

### 5a. A narrow field of view, as a setting
At 45° the camera sits close, so the near side of a pit draws much bigger than the far side and the floor
seems to stretch away: walls read short. A narrow lens from further back draws both sides near the same size,
which is how Spry's view reads. Tim tried 45, 30, 20, 12 and 6° beside Spry and preferred the narrow end, 6°.
- The 3D view opens at **6°**. A toolbar menu offers 2, 4, 6, 10, 15, 20, 30 and 45° (45 is the old view),
  saved in `localStorage` on this machine.
- Changing it moves the camera along its line of sight so the view centre stays the same size on screen.
- Anything that sized itself by distance now scales by the field of view: zoom to extents (1.9 scene radii at
  45°), fly to a point (600 m at 45°) and the bench's polyline handles (now in pixels). Pan, zoom, the plan
  switch and marker sizes already used the field of view.
- The bench always runs at the default, whatever is saved, and checks the scale holds through a change
  (below 1e-9) and that zoom to extents frames the scene the same at 6° as at 45°.
- On PIT B (2026-11-05) at 6°: scale off by 2e-16 through a change; extents 1.11 screens across at 6° and
  1.13 at 45°; turn drift 1.4e-12 px; orbit 60 fps; pick p50 1.2 ms. The grabbed point is 0.0001 px off the
  cursor after a 200 px pan, against 23.9 px at 45°: at a narrow lens every depth pans at nearly the same
  rate, so pan sticks already. PR 2 still makes it exact, for the wider settings.
- Lighting like Spry's (ambient 0.4, diffuse 0.6, straight down) was tried on its own first and didn't fix
  the flat look; it isn't in this change.

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
Damping. A navigation cube. Saved views and R (reset). A fixed rotation centre
(Deswik's). Left-drag selection box in the Spry layout (left drag does nothing there until there is one).

## How it is judged

- **Bench**, on the 11.7 M-triangle scene and the 4.9 M whole pit, through real pointer and wheel events:
  the turned-about point stays under the cursor (below 1e-6 px); a grabbed point stays under the cursor
  through a 500 px pan, near and far (below 0.5 px); the point under the cursor holds through a wheel zoom
  (below 1 px); a press over empty space beside a solid pivots on the solid; the
  orthographic switch keeps the scale at the pivot within 1 %; orbit holds 60 fps. Timings: pick at press
  p50 and p95, the ray fan p50 and p95 (budget 5 ms), triangles in the scene.
- **Rust test** for `snap_rays`: a flat synthetic surface with a square hole; a fan centred over the hole
  returns the hit on the hole's nearest edge, with its known screen distance.
- **Tim, side by side with Spry on the same pit** (PIT B), Spry layout on: turn about a point on the
  highwall, about a string, and over the sky beside the pit; pan near and far; zoom into a ramp and down
  to the pit floor; plan and section; the keys. He says what still feels off; constants are tuned in PR 5.

## Pull requests (each off `main`, small)

1. **Own controller, same feel.** `camera.ts` replaces `OrbitControls` with today's behaviour, plus the
   turn waiting for its pick. 0021's bench checks still pass. No visible change except no start about the
   wrong point.
1a. **Narrow field of view** (6° default) and its setting, stacked on 1.
2. **Pan that sticks** in 3D, plan and section, with its bench check.
3. **The surer pivot:** `snap_rays` in `app/src-tauri` with its Rust test, the fallback to the nearest
   thing then the last pivot, and the bench check.
4. **Orthographic 3D switch**, with the `flatView` split.
5. **Spry layout and the keys:** the View controls dialog, `localStorage`, clash checks. Then the side by
   side with Spry, tuning, and this record goes to accepted.

PRs 2 to 4 each depend only on 1 and touch separate parts of `camera.ts`, but they are small enough to run
one after another.

## Settled with Tim

- Between a string near the cursor and the surface under it, **the one nearer the camera** is the pivot.
- In the Spry layout, **middle drag on its own pans**, as middle + right does.
- **Zoom stays as PR 1 has it** (2026-11-05): no push-through. Spry doesn't zoom through a surface either.
- **The 3D field of view is 6° by default and a setting** (2026-11-05). Spry's heights read taller at 45°.

## T and Z, ahead of PR 5 (2026-11-09)

Tim, after zoom to selection (0050): "I want a shortcut for plan and 3d mode as well". He picked, from four ideas,
**T toggles, Z tilts**. These are the first of section 7's keys, built before the View controls dialog, so they can't be
changed yet.

- Keys: **T**, **Z**, **2** and **E**, beside F and Shift+F (0050).
- **T** flips between plan and 3D over the same spot at the same scale, as the toolbar's button does. On a section it
  closes the section first. It works anywhere but a text box or a list, as F does (0050), so it can be pressed with the
  tree in focus. The toolbar's button now follows the view however it changed.
- **Z held, and a left drag,** turns about the point under the cursor, as Spry's does. In 3D that's what a left drag
  does already. In plan it tilts the view out of plan into 3D: the view first goes to 3D looking straight down on the same
  spot at the same scale, north up, 0.6° short of vertical where a turn stops, then turns from there. In a section it pans
  as before. While Z is held the cursor is a hand, and a press is the camera's: no click selects, no box, no vertex drag.
  Ctrl+Z is still undo.
- **2** is the section key (Tim, 2026-11-09): it starts picking a section's two points, or stops picking if it had
  started, and closes an open section, as the toolbar's Section button does. `[` and `]` still step it.
- **E** zooms to everything (Tim asked for a zoom to extents key): one key for what Shift+F does, so typing Z then E,
  AutoCAD's zoom extents, does it too. F stays zoom to selection (0050).
- **Bench** (`zd-app --bench --select`, release app, WebGPU, 2026-11-09), through key and pointer events: 2 starts and
  stops picking, two points picked open a section and 2 closes it; E from a view moved 361 m off brings the target back to
  the middle of everything, 0 m off. T into plan and
  the toolbar says Plan; T back to 3D; Z and a 170 px drag in plan end at 60° from straight down, in 3D, with the point
  under the cursor at the press 0.26 px from where it was; the selection unchanged; and without Z a left drag in plan pans
  58 m and stays in plan.

## The pivot on a big project (2026-11-21)

Tim, on the made-up large mine (`Large mine 1m.zdp`, 188 M triangles, 140,220 blocks, the toes and pit limit on show):
"I try and snap and it like it orbits from a different position so it doesn't feel smooth." Two causes, both measured
with `zd-app --bench --lod` on that project:

- **The string always won.** Settled here on 2026-11-05 but never built: the pick for a turn or a zoom took a string near
  the cursor before the surface under it. Strings draw over everything, so the toes, draped on S20's floor up to 500 m
  down, showed under the cursor across the whole pit. 2 of 15 presses across the view turned about a toe 443 m behind
  the ground clicked. Now `StringLayer.pivot` takes both and turns about the nearer the camera. A string on the ground,
  or above it, still wins, within 1 m. A share of the distance wasn't used: from across the lease at 6° a hundredth is
  hundreds of metres. On the large mine, 0 of 15 now.
- **The turn waited about 50 ms for its pick.** The page draws about 4,900 chunks a frame there, 13 ms of the main
  thread each frame. The pick's answer from the shell queued behind three frames (a plain invoke: 1.2 ms before the
  versions were on, 50 ms after; the ray itself takes 1.2 ms). Since nothing moves while a turn or wheel tick waits for
  its pick, the view now holds drawing until the pick lands, for 250 ms at most (`Viewport.holdFrames`). Press to pivot:
  3.3 ms p50, 26.6 ms the most, against about 50 ms before.
- **The gold mark shows at the press,** as soon as the pick lands, so you see where it will turn before you move.

`zd-app --bench --select` checks a string 80 m under the topography (turned about the ground above it) and one on the
ground (turned about the string). `--lod` checks the large mine: no pivot behind what's under the cursor, press to pivot
under 20 ms p50. `--clicks`, `--shell`, `--string-edits` and `--section-guide` pass unchanged. The orbit still runs at
59 fps at 1 px.

Still not built from this record: pan that sticks (PR 2), the fan of rays over empty space and the last pivot (the rest
of PR 3), the orthographic switch (PR 4) and the Spry layout (PR 5). And while the page is drawing thousands of chunks,
every other ask of the shell (a snap while drawing a string, say) still waits about 50 ms: that wants fewer draw calls.

## The edge of the topography, and a snap index that comes late (2026-11-21)

Tim, the same day, trying the fix above: "when i go try and Snap and orbit on the edge of the topo surface it
essentially defaults to spinning from the centre and a random rl", then a while later "it has changed and recognises
the surface now". Two causes:

- **The snap index wasn't in yet.** It was built once the project was drawn, every surface and tile of solids on its own
  thread at once, hidden ones too (425 M triangles on the large mine, 188 M of them on show), and none went in until
  all were done: 66 s on this machine's four cores, while a press found nothing and turned about the view's centre,
  wherever its RL happened to be. Parry builds a BVH on one thread: a 1 m lidar's 85 M triangles take 25 s.
  - A big surface's index is now built in pieces by plan position, about 4 M triangles each
    (`zd_geom::PIECE_TRIANGLES`), side by side on every core: the 85 M triangles in 2.9 s, rays as quick. A cast goes
    through the pieces in the order the ray reaches their boxes and stops once the next box is past the best hit; it
    hits what one index hits, naming the same triangle.
  - Only what's on show is indexed, surfaces before solids, a few at a time on the cores, each going in as soon as it's
    built, after every change; a layer shown later is indexed then (0.3 s for a hidden layer of 5.6 M triangles on the
    large mine). Snapping is ready 6.7 s after the large mine is drawn, against 66 s.
  - While it isn't, the status bar says "Snapping getting ready…" (after a quarter of a second, so a quick change
    doesn't flicker it).
- **Over nothing, the view's centre.** This record's fan of rays is now built: with nothing under the cursor, rays in
  rings 8, 16, 32, 64, 128 and 256 px round it (8 a ring, then 16 and 32 in the outer two, so they're no more than
  about 50 px apart), cast in one ask of the shell (`snap_rays`), the first ring with a hit giving its hit nearest the
  camera; with none, the last point turned about while it's in view; else the view's centre. Strings aren't in the fan:
  one within the snap distance is taken first, as before.

`zd-app --bench --lod` on the large mine: 40 presses over nothing within 200 px of something, none turned about the
centre, the worst 256 px from the cursor, 3.9 ms p50 and 50 ms the most; press to pivot 6 ms p50; a hidden layer shown
indexed. `zd-geom` tests: pieces hit what one index hits on 2,000 rays (with and without passing over triangles), a ray
down lands on a plane, and the timing (ignored). `zd-app` test: a fan over a square hole turns about its nearest
edge. Not yet tried by hand.

Then, on `Large mine 2m.zdp`: "when i have just a seam model open the snap to orbit doesn't work". A seam model's
horizon is drawn from its grids on disk, not from an object of the project, so nothing was indexed under it. Its roof,
the surface drawn, is now indexed when the horizon is staged, kept under the model's own key (so it goes when the model
changes and is passed over while hidden). `--lod` checks a press in the middle of a seam model shown alone: it turns
about the roof right under the cursor (0 px). On the 2 m mine, snapping was ready 2.8 s after drawing, and 0 of 40 presses
over nothing near something turned about the centre.

## Strings hidden behind what's in front, by default (2026-11-21)

Tim: "By default lets make objects the same with a toggle to alter. It makes it slightly confusing for me to see the
strings/Polygons through the solid." Strings, polygons and roads were drawn with no depth test, over everything. Now
they're hidden behind the solids and surfaces in front of them, like any other object, drawn a hair nearer than they
are (a twentieth of a percent of their distance from the camera) so one lying on a solid or surface shows on it. The
field-of-view menu has the toggle, Strings and polygons: Hidden behind solids and surfaces (the default) or Over
everything, saved on this machine. What the view draws to work with stays over everything: the picked string, its
handles, the string being drawn, the section guide.

A click picks what it can see: a string behind the solid or surface under the cursor, by more than a metre, isn't
picked, and the click goes to what's in front (a select click, a Ctrl-click and the right-click menu). The turn already
went about the nearer of the two.

`--select` checks a toe 80 m under the ground and a line on it: by default the toe doesn't show and the line does, a
click over the toe doesn't pick it; over everything, the toe shows. `--string-edits` clicks lines under its ground to
pick them, so it runs with strings over everything. `--clicks`, `--section-guide`, `--measure` and `--shell` pass.

## Pan that sticks (PR 2, 2026-11-22)

Section 4 built, more simply than the plane through the grabbed point: a pan only slides the camera, so every point keeps
its depth, and a pixel measured at the grabbed point's depth (not the target's) moves that point exactly with the cursor.
A right drag in perspective picks what's under the cursor at the press (`StringLayer.pivot`, the fan of rays over
nothing) and waits for it as a turn does, drawing held meanwhile; with no pick it pans at the target's depth as before.
Plan, section and, later, orthographic 3D need no pick: a pixel is the same at every depth there.

`zd-app --bench --solids` on the store bench's 43,000 made-up blocks (4.6 M triangles, release app, WebGPU), a 500 px
right drag with its press and first move sent at once: the grabbed point 2e-11 px off the cursor at 6° and 7e-12 px at
45° (against 23.9 px for 200 px at 45° on PIT B before). Turn drift 9e-13 px and zoom drift 0.94 px, unchanged; orbit
60 fps. The bench's snap switch and string checks now find the view bar's Draw button by its name (it lost its words in
0076), so they draw again: all four snap switch checks pass.

## Orthographic 3D (PR 4, 2026-11-22)

Section 6 built. **Ortho** on the view bar beside 3D/Plan (Persp while off), and **P**, which no other shortcut uses or
starts with; saved on this machine (`zd.view.orthographic`); off in plan and a section, which are flat anyway, and kept
for when the view goes back to 3D.

- **One rig.** The 3D (perspective) camera stays the one every move works on: `Viewport.rig`. In orthographic 3D the
  camera drawn with, `orthographic3d`, copies it after every move (`Viewport.syncCamera`, called from
  `CameraController.update`): the same way, at the same target, its half height the rig's distance to the target times
  tan of half the field of view, so the target is one size on screen either way, and it stands well back (the distance
  plus four scene radii), so nothing is behind it. Fit, zoom to selection, focus, the field of view, T and Z keep working
  on the rig unchanged. The field of view still sets how far back the rig stands; in orthographic it changes nothing on
  screen.
- **Zoom** with no perspective scales the view about the cursor: the rig closes in along its line of sight by the wheel's
  share, then rig and target slide across the screen so what's under the cursor stays. No pick, so no depth to find.
  A turn picks and turns as in perspective; a pan needs no pick.
- **`flatView` split.** It still means plan or section (left drag pans, no turn). `noPerspective` (plan, section or
  orthographic 3D) is what level of detail, `pixelSize`, string reach and clip planes now ask.

`zd-app --bench --solids` on the 43,000 made-up blocks, by the real P key and canvas: scale at the target off by 0 through
the switch, into plan and back, and back to perspective; in orthographic a turn's drift 1e-12 px, a 500 px pan's 2e-13
px, a wheel zoom's 0.37 px; 12.74 % of the view's pixels drawn against 12.76 % in perspective (at 6° the two nearly
match); orbit 60 fps.

## Spry's layout and the keys (PR 5, 2026-11-22)

Section 7 built, with three changes from the plan:

- **A panel, not a dialog.** View controls is a panel in the tool dock (`viewcontrols`, decision 0076's way, as Licence
  is), opened from the field-of-view menu (View controls: mouse layout and keys…) or Find a command (mouse, Spry). The
  kit has no dialog, and adding one would be a new dependency.
- **The keys are six, not five:** hold to turn (Z), hold to pan (Space), zoom to what's selected (F, Shift+F zooms to
  everything as before), zoom to everything (E), plan or 3D (T) and orthographic or perspective (P). They work anywhere
  but a text box or a list, as T and F did from 2026-11-09, not only over the view.
- **Spry's left drag is the selection box,** as Shift and a left drag is in ours: 0050's box came after this record's
  "left drag does nothing there". A left press let go without a drag is the click it always was.

| | Spry's layout | Ours (the default) |
|---|---|---|
| Turn about the cursor | Middle, then left as well; or hold the turn key and left | Left drag; or hold the turn key and left |
| Pan (sticks) | Middle drag, middle and right, right drag; or hold the pan key and left | Right drag (left in plan and section); or hold the pan key and left |
| Select | Left click, left drag a box | Left click, Shift and a left drag a box |
| Zoom | Wheel | Wheel, middle drag |

Right drag still pans in Spry's layout, so a right click still finishes (0080). Letting go of the left with the middle
still held goes back to panning. The hold keys work in both layouts. A new key is refused, saying why, if it's another
view key, Enter, Esc, Backspace, Delete, Tab, [ or ], any tool's shortcut, or the first letter of a two-letter one (N, for
NS). The view bar's tips, Find a command and the menus name the keys as set. Saved on this machine
(`zd.view.controls`); a saved set that now clashes falls back to the default keys.

`zd-app --bench --solids` on the 43,000 made-up blocks, by the real canvas and keys, in Spry's layout: a 360 px middle
drag leaves the point grabbed 2e-11 px off the cursor; middle then left turns (the camera swung 31 km round) with the
point 6e-10 px from where it was; a left drag shows the box and moves the view 0 m; with the turn key moved to X, X and a
left drag turn the same; S, N, F and Enter are refused, each saying what has it, and J is taken; the controls are put
back. `--select`, `--clicks`, `--string-edits`, `--measure` and `--section-guide` pass every check (118); `--shell`
passes 59 of 60, the one left a tip opened by focus, which can't open while the window isn't in front. Still to do: Tim's side by side
with Spry on PIT B, the constants tuned from it, and this record to accepted.
