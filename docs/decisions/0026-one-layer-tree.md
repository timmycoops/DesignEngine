# 0026 — One layer tree

Date: 2026-11-01 · Status: accepted (Tim, 2026-11-01)

## Why

Tim, with two seam models on one layer: "Shows two seam models and two surfaces - but i cant see which one is which ?
This feels very weird". Layers and Objects were separate lists, only a whole layer could be hidden, and selecting showed
nothing on screen. The Layers plan on the Build Plan doc fixes that in six workstreams. This records the decisions Tim
agreed and the first workstream, the tree.

## Decision

1. **One tree replaces Layers and Objects.** Open a layer to see what's on it. Clicking a layer still sets what new
   strings are drawn on, and clicking an object selects it.
2. **A layer's name is its path**, such as `Grids\PIT C`, and folders come from the names (workstream 5).
3. **Hidden objects save with the project as display state**, like a layer's tick. Hiding isn't a revision and doesn't
   undo (workstream 2).
4. **Hidden means not drawn, not snapped to and not cut by a section**, but still usable by a command that names it.
5. **Each seam model goes on its own layer under `Seam models`**, named after its definition file (workstream 4).
6. **Colour stays on the layer**, and line weight stays there as a stopgap until workstream 6.
7. **A layer of more than 50 objects starts closed**, and the rest start open, so a face run shows its strings and a pit's
   solids don't fill the tree. Opening or closing one by hand holds for the session.
8. **The tree draws only the rows on screen.** Scrolled down a layer, its row stays pinned at the top, so it can be closed
   from anywhere in it, and closing it there lands on that layer.
9. **Hiding one solid rebuilds its layer's mesh without it.** The rebuild was measured first, because a layer's solids
   draw as one mesh (decision 0019). It makes the 1 s check by a wide margin, so hiding triangles in place isn't needed.

## Evidence

Redrawing PIT B's layer of 41,917 solids, 4.7 M triangles and a 58 MB buffer, in the real app, 3 runs each:

| Step | ms |
|---|---:|
| List the solids | 8 to 14 |
| Join them | 30 to 33 |
| Encode for the viewport | 54 to 57 |
| Those three, through the bridge | 91 to 95 |
| Fetch, upload and first frame | 72 to 95 |
| **On screen** | **about 170** |
| Snap index, on its own thread | 1,850 to 1,960 |

So the screen is well under the 1 s check, and the snap index is not. It already runs in the background, so hiding doesn't
wait for it, and workstream 2 has to keep a hidden solid from being snapped to during those 2 s.

The tree with all 41,917 solids on one layer, in the real app with 8.7 M triangles drawn:

| | |
|---|---:|
| Open the layer | 13 ms |
| Rows in the page | 45 |
| Frame while scrolling end to end, p50 | 16.7 ms |
| Frame while scrolling, worst of 120 | 18.1 ms |
| Last solid in the last row at the far end | yes |
| Orbit | 60.0 fps, 0 frames over 33 ms |

## Known limits

- **Object rows have no tick yet.** It arrives with workstream 2, which makes it work. A tick that did nothing would mislead.
- **The Objects table's sorting and columns are gone.** Objects list in the order they were made, which keeps a face's
  toes and crests together. A revision shows beside the name once it's past 1. Filtering comes in workstream 6.
- **The tree doesn't move to an object picked in the viewport.** That comes with workstream 3.
- **The panel layout resets once**, as a saved layout names the Objects panel.
