# 0076 — Tools in one dock, tables in another, and a tab a stage

Status: agreed 16 November 2026. Tim picked "panel and then b" of the options put to him (fix how panels open first,
then the workflow tabs), and agreed the six decisions on the Build Plan's "UI improvements plan" tab as recommended
("Agree all 6"). It takes over the open parts of the UI plan tab and replaces 0051's one toolbar row. 0051's minimised
strips and the ` key stay. Built as seven workstreams, a PR each. Workstream 1, the docks, is built here.

## Why

Tim, 16 November: "I'm looking to add some UI improvements to how all the tabs are used and how they load and take up
space inside the app ... the ribbon and tools in particular." Measured on the shell at 1,600 × 900:

| What | Before |
|---|---|
| The toolbar row | needs 1,714 px: Workflow clipped, Panels off the screen |
| Face design open | the view goes from 1,000 px wide to 320 px |
| Face design, Builder and Cut solids open | the view is 250 px wide |
| Every panel open | the view is 166 × 119 px |

Every tool opened as its own column beside the view, and every table as its own row under it. Tools were in four
places (the toolbar, the Panels menu, Properties and the workflow canvas), and each panel's head said its name twice.

## Decision

What the shell becomes:

- **One tool dock.** A tool opens as a tab on the right, beside Properties, at one width (400 px). The view is as wide
  with every tool open as with one.
- **One results dock.** A table opens as a tab under the view: Run log, Block table, Reserves table, Parameters,
  Workflow canvas and Face design. The dock grows to the height a table asks for and never shrinks under one. A button
  on it lets a table fill the window, and puts it back.
- **View tools on the view**: Select, Draw, Snap, 3D or Plan, the zooms, the field of view and Section, on a bar over
  the view. The view loses its own tab strip.
- **A tab a stage**: Project · Strings · Walls · Solids · Geology · Reserves, with a row of the stage's tools named in
  words under it. A row that's too long folds into More ▾, so nothing is clipped.
- **Compact heads**: a tool's name is on its tab only, and what it does is one line, the rest in a tip.
- **One tool list** that the ribbon, the Panels menu and the workflow canvas all read, grown from the canvas's.
- **Find a command** (Ctrl+K).

The six decisions, as agreed:

1. The stages and the tools in each, as the plan tab's table has them.
2. Face design opens under the view, in the results dock: it's a wide table.
3. Switching stage changes the tool row only. The panels stay where they were left.
4. Find a command is in this phase, after the tabs. It needs the `cmdk` package (shadcn's Command), a dependency
   outside the locked stack, agreed here.
5. The string tools come out of Properties in this phase, last, onto the Strings tab. Properties shows only.
6. The view loses its tab strip, and with it the double-click on the Viewport tab. ` still hides every panel.

Workstreams, in order: docks, view bar, compact heads, one tool list, workflow tabs, Find a command, string tools out
of Properties. None touches the engine, the object model or the command interface.

Not in this plan: Face design on the kit, a layout a stage, floating tools, a panel popped out to a second monitor.

## Built: workstream 1, the docks

- **A panel has a dock** (`PANELS` in `app/src/workspace/Workspace.tsx`): `left` for Layers, `tools` for Properties,
  Builder, Cut solids, Reserves and Workflow, `results` for the tables and Face design. `showPanel` opens a closed
  panel as a tab in its dock's group, wherever that group has been dragged to. The viewport's own group is never a
  dock.
- **A dock with nothing in it goes**, and is made again when one of its panels is asked for: the tools down the right
  of the window at 400 px, the tables under the view at the table's height, Layers down the left at 280 px.
- **The view gives and takes the room.** dockview shares a closed dock's room among every panel left, takes a new
  dock's from them all, and scales every panel when the window changes size. The docks' sizes are noted after each
  change to the layout and put back when a dock comes, goes, is minimised or brought back, and when the workspace
  changes size (the window resized, or the strips at its edges coming and going). So the docks stay as they were set,
  to the pixel, and the view takes the difference. This closes 0051's known limit, "a few pixels move".
- **A minimised dock comes back with the tool asked for in front**, whether the tool was open in it or closed.
- **Fill the window**: a button before the – on the results dock. The same button, or `, puts it back. ` with a table
  filling the window gives the window to the view.
- **Layouts saved before the docks aren't read** (`zd.workspace.layout.v3` and its edges): the app opens once on the
  new default, then keeps its layout as before (`v4`).
- The default layout is Layers 280 px, the view, the tool dock 400 px with Properties in it, and the results dock
  200 px with the Run log.

## Evidence

Release app, `zd-app --bench --shell` (16 November 2026), through clicks on the toolbar and the panels' own buttons
and key events, in a 1,600 × 1,000 window, from the default layout, with the layout saved on the machine put back
after. 20 of 20 checks pass: 0051's eight, and twelve for the docks.

| Check | Result |
|---|---|
| The default layout | Layers 280 px, the view 920 px, the tool dock 400 px with Properties in it |
| One tool open (Cut solids) | a tab beside Properties; the view 920 px |
| Two tools, then all four | five tabs in one dock; the view 920 px each time |
| Block table, Reserves table, Parameters, Face design and the canvas open | six tabs in one row under the view; the view 920 px wide |
| The results dock's height | 200 px with the Run log, 320 px once the Block table opens, 400 px with them all |
| Builder closed, the tool dock minimised, Builder asked for from the toolbar | the dock back with Builder in front; no strip; the view 920 px |
| Fill the window | the table 1,600 × 939 px; put back, the view 920 × 509 px as before |
| ` with a table filling the window | the view 1,600 × 909 px; ` again, everything back as it was |
| Every tool closed, then Cut solids opened | the view 1,320 px, then 920 px; Layers 280 px throughout; the dock 400 px |
| No layout from before the docks | neither old key is kept |
| 0051: ` and back | the view 1,172 px before and after (it moved 6 px before this) |
| 0051: Properties minimised and back | 372 px given back, 372 px taken |

`zd-app --bench --workflow` and `--dataflow`, which open panels as they go, still pass.

With `--pictures <folder>` the shell bench draws the whole window in four states (`docks-default`, `docks-solids`,
`docks-walls`, `docks-reserves`), over the project given by `--dataflow`. The picture is made inside the app
(`app/src/bench/windowPicture.ts`): the page's elements laid out again in an SVG with the page's own style sheets, the
view's canvas as the picture it holds. Nothing reads the screen.

## Known limits

- **A layout opened in a window of another size** is scaled by dockview as it's read, docks and all. They hold from
  then on.
- **The window resized while a table fills it**: the docks come back scaled, as dockview leaves them.
- **A dock is told by what's in it.** A table dragged into the tool dock stays there, and the next table still opens
  under the view. With a dock's panels split across two groups, the next one opens in the group holding the most.
- **The toolbar still clips** at 1,600 px. Workstream 5 replaces it.
- **Window pictures** leave out scroll positions and web fonts, and are for the bench only.
