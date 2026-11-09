# 0050 — Several selected, and zoom to them

Date: 2026-11-09 · Status: proposed (Tim asked for all three on 2026-11-09; the plan below was sent to him and built
without a change to how commands take their inputs, which already take lists). After 0026 (one layer tree), 0043 (colour
by RL) and 0044 (camera).

## Why

Tim, 2026-11-09: "we need to have a zoom to entitiues, we need to have the contour lines not on by default for the surface
coloring, we need the ability to multi select elements". PIT B is fifteen faces and sixteen strip solids, so picking
several at once and going to them is everyday work. 0026 had one selected object plus rows "marked" to act together:
marks were faint in the tree, Ctrl-click marked only strings in the viewport, and Properties and the highlight showed one.
There was no way to go to an object but to fly there by hand, and zoom to extents ran only when a project opened.

## Decision

1. **One selection, of any number of objects, of any kind.** It replaces "selected" and "marked" (0026, item 20). The last
   one picked is the one a panel that takes one takes: the Builder's line, Face design's Use selected, a section's cut
   lines over the rest, and the string with handles to drag.
2. **In the tree** a click selects one row alone; Ctrl-click adds a row or takes it off; Shift-click selects the rows from
   the last one clicked to this one, and Ctrl+Shift adds them. Every selected row is gold, the last picked of several in
   bold. Right-click on a selected row acts on all of them; on a group or layer, **Select** takes everything in it.
3. **In the viewport** a click selects what's on top alone, and on nothing clears the selection, as before. Ctrl-click (or
   Shift-click) adds a string, surface, grid, solid or seam model, or takes it off; on nothing it leaves the selection as
   it is. Esc clears it.
4. **A box: Shift and a left drag**, in 3D or plan, takes everything on show that lies **wholly inside it**, and Ctrl+Shift
   adds that to the selection. The view doesn't turn or pan while it's drawn. Wholly inside, so a box over blocks in plan
   doesn't catch the topography or pit shell they stand on. It's worked out in Rust (`objects_in_box`, a bridge call, not a
   command): each object's box first, its points only when the box straddles an edge, nothing behind the camera.
5. **Everything selected lights up**: a surface, grid or seam horizon glows in place, as before; the solids draw as one
   copy over their layers, joined in Rust (`stage_solids`), so a box over a pit's blocks is one fetch and one draw; strings
   draw gold over their layers.
6. **Properties with several** says how many of each kind and on which layers, where they are together (easting, northing,
   RL), sets Colour and Opacity for all those that take them, numbers the strings among them by a line drawn across
   (`string.number_across`, which numbered marked strings before), and lists them: a click on one selects it alone. Zoom
   to and Delete in its header take all of them (`object.delete_many`, one undo).
7. **Tools that take several take the selection:** Face design's Apply to selected, the Cut solids panel's Picked, the
   tree's Show, Hide, Colour, Opacity, Move and Delete, and Export's Selected to DXF and Selected strings to CSV.
8. **Zoom to selection: F**, or the toolbar's Zoom to selection, or **Zoom to** on the tree's right-click menu (an object,
   a group, the selected rows, or a layer: what's on show on it, or all of it if none is). It frames the objects together
   from the way you're looking, as a line from a panel is framed; in plan, straight above them. The box round them is
   worked out in Rust (`objects_bounds`). F with nothing selected zooms to everything, as Spry's F does; **Shift+F**, or the
   toolbar's Zoom to everything, always does. F works anywhere but a text box. On a section it says to close it first.
9. **Contour lines are off by default** when a surface or grid is coloured by RL (0043, item 1): bands alone. The Contour
   lines tick in the RL legend turns them on. See 0043's note of 2026-11-09 for how projects saved before open.

## Evidence

**Rust** (`app/src-tauri/src/selection.rs`, `cargo test -p zd-app selection`): two 10 m cubes 60 m apart and a 30 m
string. The box round a cube and the string, and round both cubes, to the metre; none for nothing or a deleted object. A
box on a plan view takes a cube wholly inside and not one half inside, nor half the string; a hidden cube isn't taken. A
cube in front of a perspective camera is taken; with the camera north of it, looking away, it isn't, though its points land
in the box by the sums alone. The selected solids on show join to 2,000 m³; hidden, one is left out.

**Contour lines** (`crates/zd-io/tests/store.rs`): a project saved with `"lines": true` in its looks opens with them off and
its interval, shared range and opacities as saved; a new project's are off.

**PIT B** (`cargo test -p zd-app --release pit_b -- --ignored --nocapture`, the golden blocks as the app bench imports
them, 2026-11-09): 42,218 solids, 4,891,688 triangles.

| Call | ms | Out |
|---|---:|---|
| `objects_bounds`, all of them | 25.8 | the pit's box |
| `objects_in_box`, a plan box round the pit | 25.9 | 42,218 |
| `objects_in_box`, its west half | 25.1 | 23,066 |
| `stage_solids`' join, all of them | 32.0 | 4,891,688 triangles |

**The release app** (`zd-app --bench --select`, WebGPU, canvas 2400 × 1363, 6° field of view, 2026-11-09), through
pointer events on the canvas and key events on the window, on a topography, six 20 m blocks 100 m apart and two strings.
Every check passed:

| Check | Result |
|---|---|
| A new project has no contour lines | `edge_lines: false` |
| A Shift-drag box round two blocks in 3D | those two alone, 17 ms to the selection |
| Ctrl+Shift box round a third | all three |
| Ctrl-click a fourth, then the first again | blocks 2, 3 and 4 |
| Ctrl-click a string | added |
| Properties | "4 selected" |
| The selected blocks' tops on screen | (221, 184, 59), gold lit; the others (73, 86, 101) |
| F | the camera's target at the middle of the selection, 0 m off; every corner on screen (within ±0.72) |
| Shift+F | the target at the middle of everything, 0 m off |
| Esc | nothing selected |
| A box in plan round two blocks | those two alone |
| F in plan | centred on them, 0 m off |
| A box in plan round all six | the six, not the topography under them, 9 ms |

## Known limits

- **Wholly inside only.** There's no box that takes what it crosses. Zoom out until it's all on screen, or Ctrl-click.
- **No box on a section.** What's drawn there is the cut, and a box would take what's behind the plane.
- **A section shows the last picked's cut lines** over the rest, not all of the selection's.
- **Only the last picked string has handles** to drag, insert or delete vertices.
- **A hidden object can be selected** from the tree, and Zoom to takes it, but a box or a click doesn't, as it isn't drawn.
- **F was to be zoom to extents** in 0044's key list. It's zoom to selection now, falling back to everything; Shift+F is
  always everything. The View controls dialog (0044, PR 5) should offer both keys when it's built.
