# 0076 — Tools in one dock, tables in another, and a tab a stage

Status: agreed 16 November 2026. Tim picked "panel and then b" of the options put to him (fix how panels open first,
then the workflow tabs), and agreed the six decisions on the Build Plan's "UI improvements plan" tab as recommended
("Agree all 6"). It takes over the open parts of the UI plan tab and replaces 0051's one toolbar row. 0051's minimised
strips and the ` key stay. Built as seven workstreams, a PR each. Workstreams 1, the docks, 2, the view bar, and 3,
compact heads, are built.

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

## Built: workstream 2, the view bar

- **The view's tools are on the view** (`app/src/panels/ViewBar.tsx`): a bar in its top left corner with Select, Draw
  and Snap as icons, 3D or Plan, zoom to what's selected, zoom to everything, the field of view and Section. With a
  section open its step back, step, step on and Fill come onto the bar beside Section. Each says what it does and its
  key in a tip, and a tool that's on is lit in the picked gold. They are off the toolbar. A new view tool goes on the
  bar, through `ViewTools`; App owns the state, as it does the keys.
- **The view has no tab strip.** Its group's header is hidden, which gives the view 30 px, and the group takes no panel
  dropped on its middle: a panel there would cover the view with no tab to come back by. Panels still dock against its
  edges. A layout from before with a panel sharing the view's group shows the strip until that panel is closed or
  dragged out. The double-click on the Viewport tab is gone; ` and the Panels menu still hide every panel.
- **Every key works as it did**: T, F, Shift+F, E, 2, [, ], S and `.
- **The bar never covers the section's overview.** The overview is in the bottom left corner; in a view too short for
  both (under 340 px) the bar sits beside the overview instead of over it (a container query on the view).
- **The section's distances sit under the bar**: the band along the top that holds them starts 44 px down
  (`BAR_CLEAR` in `sectionGrid.ts`), so none is behind the bar. The legends stop short of the bar too.
- `ToolButton` (a bar's button with its tip, lit while on) is in the kit, for the toolbar and the view bar.

Release app, 16 November 2026, a 1,600 × 1,000 window. `--shell` passes 33 of 33: the 20 above and 13 for the view bar,
over a made-up topography and block.

| Check | Result |
|---|---|
| The view's height in the default layout | 739 px, its group's (709 px with the tab strip) |
| A panel dropped on the middle of the view | not taken: the group is locked |
| The view tools | all eight on the bar, none on the toolbar |
| The toolbar with the view tools off it | needs 1,280 px (1,714 px before): nothing clipped at 1,600 px |
| 3D or Plan | the button and T both go into plan and back |
| Select, Draw, Snap | each says when it's on |
| E and its button | the view back on everything, from 361 m away |
| Section and 2 | start the picking and stop it; two points open a section, and step and Fill come onto the bar |
| ] and [, and their buttons | the section 25 m on and back |
| The bar and the overview, the view 739 px high | the bar over it, 446 px clear |
| The same, the view 300 px high | the bar beside it, 12 px clear |
| The section's distances | from 44 px down; the bar ends at 41 px |
| 2 with a section open | closes it; step and Fill leave the bar |

`--select` (29 of 29) covers F, Shift+F, T, hold Z, 2 and E through key events, reading the view bar where it read the
toolbar; `--section-guide` (11 of 11), `--workflow` (43 of 43) and `--dataflow` (21 of 21) still pass. The window
pictures gain `view-bar-section`, a section open with its step and fill on the bar.

## Built: workstream 3, compact heads

- **A panel's head is one line** (`PanelHead` in the kit, 28 px): what the panel does, never its name, which is on its
  tab. A line too long for the panel is cut short; the whole of it, and anything more to say, is in a tip on the line and
  on the mark at its end.
- **`ToolPanel` takes `summary`, one line, and `more`, for the tip**, and no longer shows its title, which names the
  panel to a screen reader. Builder, Cut solids, Reserves and Workflow each have a line written to fit the 400 px dock.
- **Cut solids' and Reserves' lines follow what they're set to do**: a line a choice ("Cuts solids by the seam model's
  horizons, in one run.").
- **Cut solids' six choices are three to a row** (`Choice` takes `columns`), each shown whole. At 400 px they ran off the
  dock in one row, their words wrapped and cut.
- **Face design** opens on the same head. With a design open it starts with its controls, as the tables do: none of them
  said its name twice or opened with a paragraph, so they are as they were.
- The Workflow panel's first section is headed Saved, not Workflow again.

Release app, 16 November 2026, a 1,600 × 1,000 window, every panel open in its dock. `--shell` passes 38 of 38: the 33
above and five for the heads.

| Check | Result |
|---|---|
| Builder, Cut solids, Reserves, Workflow | each a 28 px head, its line whole at 400 px, its name nowhere under its tab |
| The rest of what a tool does | in a tip at the end of the line, and on the line |
| Cut solids' line | six choices, six lines, each whole |
| Cut solids' six choices | two rows of three inside the dock, none cut short |
| First control under the tab: Cut solids, Reserves | 37 px (about 90 px and 75 px before, read off the earlier pictures) |
| The same: Builder, Workflow, Face design | 36 px |
| The same: Run log, Block table, Layers | 32 px, 14 px, 6 px |
| First line under the tab: Reserves table, Parameters, the canvas with nothing to show, Properties | at the top |

`--select` (29 of 29), `--section-guide` (11 of 11), `--workflow` (43 of 43) and `--dataflow` (21 of 21) still pass;
`--builder` and `--cut` run with no failed check.

## Known limits

- **A layout opened in a window of another size** is scaled by dockview as it's read, docks and all. They hold from
  then on.
- **The window resized while a table fills it**: the docks come back scaled, as dockview leaves them.
- **A dock is told by what's in it.** A table dragged into the tool dock stays there, and the next table still opens
  under the view. With a dock's panels split across two groups, the next one opens in the group holding the most.
- **The toolbar needs 1,280 px**, so it clips in a window narrower than that. Workstream 5 replaces it.
- **The view bar wraps to a second row** in a view narrower than its tools (about 540 px with a section open), and then
  covers the section's distances.
- **The view stays where it is.** With no tab there's nothing to drag it by; the docks move round it.
- **Window pictures** leave out scroll positions and web fonts, and are for the bench only.
