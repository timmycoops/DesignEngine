# 0026 — One layer tree

Date: 2026-11-01 · Status: accepted (Tim, 2026-11-01)

## Why

Tim, with two seam models on one layer: "Shows two seam models and two surfaces - but i cant see which one is which ?
This feels very weird". Layers and Objects were separate lists, only a whole layer could be hidden, and selecting showed
nothing on screen. The Layers plan on the Build Plan doc fixes that in six workstreams. This records the decisions Tim
agreed and the first four workstreams: the tree, showing and hiding one object, seeing what you pick, and layer commands.

## Decision

1. **One tree replaces Layers and Objects.** Open a layer to see what's on it. Clicking a layer still sets what new
   strings are drawn on, and clicking an object selects it.
2. **A layer's name is its path**, such as `Grids\PIT C`, and folders come from the names (workstream 5).
3. **Hidden objects save with the project as display state**, like a layer's tick. Hiding isn't a revision and doesn't
   undo (workstream 2).
4. **Hidden means not drawn, not snapped to and not cut by a section**, but still usable by a command that names it.
5. **Each seam model goes on its own layer under `Seam models`**, named after its definition file (workstream 4).
6. **Colour stays on the layer**, and line weight stays there as a stopgap until workstream 6.
7. **What comes in numbers shares a row.** Tim, three face runs in: "the tree structure is already out of control". A run
   makes four strings a wall, and a seam's import brings forty quality grids. So two or more strings, solids or quality
   grids on a layer sit behind one shut row with a count and a tick for all of them. It opens to list each one, and a pick
   in the viewport opens it. Surfaces, elevation grids, seam models and face designs still list, as there are few of them
   and which one it is matters. Layers start open. Opening or closing anything by hand holds for the session.
8. **The tree draws only the rows on screen.** Scrolled down a layer, its row stays pinned at the top, so it can be closed
   from anywhere in it, and closing it there lands on that layer.
9. **Hiding one solid rebuilds its layer's mesh without it.** The rebuild was measured first, because a layer's solids
   draw as one mesh (decision 0019). It makes the 1 s check by a wide margin, so hiding triangles in place isn't needed.
   The old mesh stays up until the new one is ready, so the rest of the layer doesn't blink.
10. **Hiding one solid builds no new snap index.** An index takes 1.9 s on a pit, so it keeps every solid and knows which
    solid each triangle came from, and a cast passes over hidden ones. Snapping never pauses, and nothing hidden is ever
    snapped to. The same table will let a click in the viewport name a solid (workstream 3).
11. **A tick says what's on screen.** Tim, trying the first version, where a layer's tick overrode its objects': ticking a
    seam model with its layer off showed nothing. So with a layer off, its objects read as unticked, and ticking one
    brings the layer on with only that object showing. A layer that's partly showing has a dash, and clicking it shows
    the lot. Turned off and on, a layer comes back as it was. A layer shows how many of its objects are showing. A
    property grid or a face design has nothing to show or hide, so it has no tick.
12. **"Visible" means this too.** Export of everything visible, and a cut of what's visible, leave hidden objects out. A
    command that names a hidden object still takes it.
13. **The seam legend follows what's on show.** There's one legend for each seam model showing, under its current name,
    and hiding a model or its layer takes its legend and issue markers away.
14. **Selecting shows everywhere, in one colour.** A surface, grid or seam horizon glows faintly, enough to find it with
    its shading and thickness colours still readable. A solid gets a copy of its own mesh drawn over everything, as a
    selected string is, so one inside the pit still shows. In a section its cut lines draw over the rest.
15. **A click selects what's on top.** Strings come first, then the nearest surface, solid or seam horizon on show. On a
    section it's the surface or seam line within 8 px, else the solid the point is in, or within that reach of straight
    up or down. Clicking nothing clears the selection.
16. **The tree follows a pick.** It opens the object's layer and brings its row into view. The label beside the cursor
    now names a solid too.
17. **Layer changes are runs.** `layer.rename`, `layer.recolour` and `layer.delete` are typed commands that undo and redo
    like any other, and whether a layer is shown stays display state, so undoing a rename leaves its tick alone. An undo
    puts a deleted layer back in its place.
18. **Deleting a layer takes what's on it.** The menu says how many objects, and one undo brings them all back, so there's
    no second question. On its own, the engine refuses to remove a layer with anything left on it.
19. **Moving objects is a revision of each.** `object.move` takes a list and makes the layer if it's new. An object
    already there is left alone. `object.delete_many` does the same for deletes, so a list is one run and one undo.
20. **Several rows act together.** Ctrl and Shift mark rows in the tree, and a tick or the right-click menu then takes all
    of them. The mark is the tree's own and doesn't change what's selected.
21. **Each seam model built from the app gets its own layer,** `Seam models\<definition file>`. It reads as a flat name
    until workstream 5 makes it a folder.

## Evidence

PIT B's layer is 41,917 solids as read and 42,218 with repair, which is how the app bench imports them. Redrawing it, 3
runs each. The first three rows are the engine at 41,917 solids, 4.7 M triangles and a 58 MB buffer, and the rest are the
real app at 42,218 solids and 4.9 M triangles:

| Step | ms |
|---|---:|
| List the solids | 8 to 14 |
| Join them | 30 to 33 |
| Encode for the viewport | 54 to 57 |
| Those three, through the bridge | 91 to 95 |
| Fetch, upload and first frame | 72 to 95 |
| **On screen** | **about 170** |
| Snap index, on its own thread | 1,850 to 1,960 |

So the screen is well under the 1 s check and the snap index is not, hence item 10.

Hiding in the real app, on that layer with 8.7 M triangles drawn, each to the frame that shows it:

| | ms | Triangles after |
|---|---:|---:|
| Hide one solid | 404 | 8,700,216, the solid's 62 gone |
| Show it again | 385 | 8,700,278 |
| Hide the layer | 152 | 3,808,590 |
| Show the layer | 172 | 8,700,278 |

About 130 ms of each is the reply, as every change sends the whole project summary back. That was already so for a layer.

Clicks in the real app, through the canvas, with only the layer under test showing:

| Click on | Selected | Row found |
|---|---:|---|
| A solid of 42,218 | 32 ms, lit by 51 ms | yes |
| A seam model | 41 ms | yes |
| A surface of about 3.8 M triangles | 19 ms | yes |

The solid's copy adds its own 70 triangles and goes when the selection clears. Orbit stays at 60.0 fps.

Layer commands in the real app on that layer of 42,218 solids, each undone, with the scene back as it was every time:

| Command | ms | Undo, ms |
|---|---:|---:|
| Rename the layer | 140 | 166 |
| Recolour it | 156 | 169 |
| Move one solid to a new layer | 2,268 | 2,201 |
| Delete the layer | 554 | 513 |

Moving a solid changes what two layers' snap indexes hold, so the 1.9 s is those being built, in the background and after
the move is on screen. Snapping to those two layers' solids stops until they're done.

The tree with all 42,218 solids on one layer, in the real app:

| | |
|---|---:|
| Open the layer | 13 ms |
| Rows in the page | 45 |
| Frame while scrolling end to end, p50 | 16.6 ms |
| Frame while scrolling, worst of 120 | 21.1 ms |
| Last solid in the last row at the far end | yes |
| Orbit | 60.0 fps, 0 frames over 33 ms |

## Known limits

- **One tick at a time.** There's no way yet to hide a run of rows, or everything but one. Filtering in workstream 6 is
  where that goes.
- **The Objects table's sorting and columns are gone.** Objects list in the order they were made, which keeps a face's
  toes and crests together. A revision shows beside the name once it's past 1. Filtering comes in workstream 6.
- **One selected object at a time.** Rows can be marked to act together, but Properties and the highlight show one.
- **A moved solid costs a snap index on each layer**, as above. It's not worth a smarter index until moving solids in bulk
  is common.
- **A face design keeps the layer name it was given.** Rename its layer and the next run makes one of the old name.
- **A solid is found straight up and down from a click on a section.** That's right for solids on a section, and one with
  an overhang could name its neighbour.
- **Seam horizons are picked in the page**, triangle by triangle within the tiles under the cursor. It's fine for a click,
  and they aren't snapped to.
- **The panel layout resets once**, as a saved layout names the Objects panel.
