# 0076 — Tools in one dock, tables in another, and a tab a stage

Status: agreed 16 November 2026. Tim picked "panel and then b" of the options put to him (fix how panels open first,
then the workflow tabs), and agreed the six decisions on the Build Plan's "UI improvements plan" tab as recommended
("Agree all 6"). It takes over the open parts of the UI plan tab and replaces 0051's one toolbar row. 0051's minimised
strips and the ` key stay. Built as seven workstreams, a PR each. Workstreams 1, the docks, 2, the view bar, 3,
compact heads, 4, the one tool list, 5, the workflow tabs, and 6, Find a command, are built.

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

## Built: workstream 4, one tool list

- **Every tool is listed once**, in `app/src/tools.ts`: 64 tools, each in one stage (Project 12, Strings 12, Walls 5,
  Solids 15, Geology 4, Reserves 6, and 10 that are always there). A tool has a name, a line saying what it does, and
  what it does: a panel it opens (set to one of what the panel does, where it does several) or an action App carries
  out. It may name its engine command, its keys, its icon, what it needs before it can be used (something to undo,
  something selected), its place in a menu, and that it can add a step to a workflow.
- **The toolbar is drawn from it.** The project menu, Save, Undo and Redo, the Import and Export menus with their
  rules, headings and the Solids menu within, the row of panels, and the Panels menu all read the list
  (`app/src/workspace/ToolMenu.tsx`). App holds one table of actions, a line a tool; an action in the list with nothing
  in the table doesn't compile, nor does an icon with no picture or a panel with nothing to draw.
- **The canvas's tools come from it**: the tools that add a step, as the list has them.
- **Cut solids' and Reserves' choices take their names and their head lines from it**, so a row of tools, a panel's
  choice and the line at its head say the same thing.
- **The panel list is on its own** (`app/src/workspace/panels.ts`), free of React, for the list and the tests.
- **No change to the look.** The toolbar needs the same 1,280 px and reads the same, button for button and item for
  item. Every menu item now has a tip; some had none.
- The stages are as agreed (decision 1), with each tool in one: Relink geology is in Geology, and the Import and Export
  menus gather their tools from every stage. Today's one row is named in the list (`ONE_ROW`) until workstream 5 draws
  a row a stage.
- **Repair** is in the list, and App can carry it out (every closed mesh kept as a surface for not being a solid, into
  the repair dialog), but nothing on screen asks for it until the Solids row does.

`pnpm test` passes 76, nine of them new (`app/tests/tools.test.mjs`): every tool once and in one stage; every tool
opens a panel in the panel list or names an action; every command a tool names is one the engine has, read from
`const NAME` in `crates/zd-commands`; every panel has one tool that opens it, under its own name; the menus top to
bottom as they were; the canvas's tools as the list has them.

Release app, 16 November 2026: `--shell` passes 41 of 41, three new (the toolbar's buttons in order, eleven of them
from the list; the Export menu, with what needs a selection off without one; the Panels menu). `--select` (29 of 29),
`--section-guide` (11 of 11), `--workflow` (43 of 43, which adds steps from the canvas's tools) and `--dataflow`
(21 of 21) still pass; `--builder` and `--cut` run with no failed check.

Not in the list yet: the view bar's tools, which Find a command will want, and the tools inside panels that aren't
choices (Block table's Select shown, the Layers tree's commands).

## Built: workstream 5, the workflow tabs

- **The ribbon** (`app/src/workspace/Ribbon.tsx`) replaces the one row: a tab a stage (Project · Strings · Walls ·
  Solids · Geology · Reserves) and under it the row of that stage's tools, each named in words with its picture. It is
  68 px high against 37 px; the view's tab strip going paid 30 px of that, so the view is 708 px high in the default
  layout against 709 px with the old row and the view's own tab strip.
- **Before the tabs**: the project's menu on its name, Save, Undo, Redo. **At the end of every row**: Workflow and the
  Panels menu.
- **The rows are in the tool list** (`ROWS` in `app/src/tools.ts`), as agreed (decision 1):

  | Stage | Its row |
  |---|---|
  | Project | New, Open, Save as · Import ▾, Export ▾ · Relink geology |
  | Strings | Offset, Number across, Polygons from lines · Set grade, Set elevation, Drape · Split, Join, Smooth, Filter, Reverse, Close |
  | Walls | Face design, Builder, Rebuild from here, Join faces |
  | Solids | Between surfaces, Strips by faces, By surface or RL, By seams, By benches, By blocks · Block table, Repair · Import solids ▾, Export solids ▾ |
  | Geology | Import seam grids, Suggest definition, Build seam model, Relink geology |
  | Reserves | Reserves, Parameters, Reserves table, Compare, Report |

- **A tool opens its panel set to it**: By seams opens Cut solids set to By seams, Compare opens Reserves set to
  Compare. Join faces opens Cut solids at Into strips, where its button is; Rebuild from here opens the Builder; Report
  opens the Workflow panel, where Write report is.
- **The tool in use is lit** in the picked gold: its panel in front in its dock, set to it. A panel that does several
  things says which it's set to (`toolMode`), so the light follows the panel's own choice.
- **More ▾**: a row too long for the window folds what doesn't fit into a menu at its end, in order, a row's menu as a
  menu within. Each tool is measured once and the fold follows the window, so nothing is clipped at any width.
- **Switching stage changes the row only** (decision 3). The stage is remembered for each project on this machine, by
  its path, beside the layout; it's how the window looks, so it stays out of the project file.
- **The Strings tools open Properties at their row**, lit for a moment, on the string picked; with none picked the
  status line says to pick one. They run in Properties until workstream 7 gives them panels of their own.
- **Repair** has its button: every closed mesh kept as a surface for not being a solid goes to the repair dialog, or
  the status line says there is nothing to repair.
- The names on a row are the plan's (Strips by faces, By surface or RL, By blocks). The Cut solids panel's own choice
  and the workflow canvas kept theirs (Into strips, By a surface, By polygons) until workstream 6 gave all three the
  row's names (Tim, 16 November: "I'm happy with those names for now").

`pnpm test` passes 80: every stage has a row of tools that exist, each with a picture and a name of 22 letters or
fewer; a stage's tools are on its row but for four reached another way (the older project folder in the project's
menu, Cut solids as it stands in the Panels menu, and the two the canvas lists that Builder and Reserves open on);
every tool on the old one row is one click from its stage, and every import and export from Project; which tool is in
use; the stage remembered.

Release app, 16 November 2026, a 1,600 × 1,000 window. `--shell` passes 46 of 46, eight of them for the ribbon; the
checks on the docks now ask for their tools from the ribbon (a stage's tab, then the tool).

| Check | Result |
|---|---|
| The ribbon | six tabs over a row, 68 px high |
| Every stage's row at 1,600 px | its tools in order, none in More, nothing clipped or off the screen |
| At 1,280 px | the same; Strings shows 10 and folds 2, Solids shows 8 and folds 2 |
| At 900 px | the same; Strings shows 5 and folds 7, Solids shows 5 and folds 5 |
| A tool folded into More | does what it does on the row |
| Switching through every stage | the layout and the view's size unchanged; the stage remembered |
| The project, Import, Export and Panels menus | read as they did; what needs a selection is off without one |
| The Solids row's own menus | the four imports of solids, the two exports |
| The tool in use | By seams lit; By benches once picked in the panel; Block table lit until its dock is minimised |
| A Strings tool | with no string picked says so; with one picked, its row in Properties is brought into view and lit |
| Repair, with nothing to repair | says so |

The narrower widths are the app laid out in 1,280 px and 900 px of the 1,600 px window, not the window resized.
`--select` (29 of 29), `--section-guide` (11 of 11), `--workflow` (43 of 43) and `--dataflow` (21 of 21) still pass;
`--builder` and `--cut`, which now open their panels from the ribbon, run with no failed check.

## Built: workstream 6, Find a command

- **Ctrl+K opens Find a command** from anywhere, a box being typed in included, as does its box at the end of the tab
  row (`app/src/workspace/FindCommand.tsx`). It lists every tool in the tool list, a stage at a time, each with its
  picture, its name, what it does and its keys. Type, and Enter or a click carries the tool out as its button would:
  its panel opens set to it, or its action runs. Esc closes it. The stage on the ribbon stays where it was.
- **A tool is found by whole words typed**, in its name, its other names (its menu's wording), its stage, its engine
  command or what it does (`findScore` in the tool list). Every word typed must be found; a name that starts with a
  word comes first, then a name holding it, then a stage or command, then what the tool does. Nothing is guessed from
  scattered letters: cmdk's own matching found 26 tools for "seams", this finds one.
- **A tool that needs something it hasn't got** (a selection, something to undo) is listed but off.
- **The view's tools are in the list** (Select, Draw string, Snap, Plan or 3D, the two zooms, Section), so Find reaches
  them; the view bar still draws its own buttons. The list is 71 tools.
- **`cmdk`** (1.1.1) is the one new dependency, agreed in decision 4. It gives the list, the keys and the dialog; the
  matching is ours.
- **One set of names for Cut solids' tools**: Strips by faces, By surface or RL and By blocks on the row, in the panel's
  choice and on the canvas.

`pnpm test` passes 83, two new: Find lists every tool once under its own name, no two tools sharing one; and what is
found, in order, for a name, part of a name, a command, a stage, a key, two words, and nothing.

Release app, 16 November 2026. `--shell` passes 50 of 50, four of them for Find a command.

| Check | Result |
|---|---|
| Ctrl+K, and the box on the ribbon | open it on all 71 tools, the typing in its box; Esc closes it |
| "seams", then Enter, from the Project stage | By seams alone found; Cut solids opens set to it; the stage stays Project, and By seams is lit on the Solids row |
| "polygons" | Polygons from lines, then By blocks (by what it does) |
| "solid.cut_by_benches" | By benches |
| "geology" | the four Geology tools |
| "selected solids" | Export selected solids, off with no solid selected |
| Something no tool has | nothing, and it says so |
| Ctrl+K from a box being typed in | opens |
| "plan or 3d", Enter, twice | the view into plan, and back |

`--select` (29 of 29), `--section-guide` (11 of 11), `--workflow` (43 of 43) and `--dataflow` (21 of 21) still pass;
`--builder` and `--cut` run with no failed check, under the new names.

## Known limits

- **A layout opened in a window of another size** is scaled by dockview as it's read, docks and all. They hold from
  then on.
- **The window resized while a table fills it**: the docks come back scaled, as dockview leaves them.
- **A dock is told by what's in it.** A table dragged into the tool dock stays there, and the next table still opens
  under the view. With a dock's panels split across two groups, the next one opens in the group holding the most.
- **The tab row doesn't fold**: in a window too narrow for the six tabs they are clipped. The tool row folds at any
  width.
- **A Strings tool with several strings picked** finds its row only if it's Number across or Polygons from lines, which
  are the two Properties shows for several; the others need one string picked.
- **Find a command reaches a tool, not what's inside a panel**: the Block table's Select shown or the Layers tree's
  commands aren't tools yet. The view bar draws its own buttons, so its names are written twice.
- **Repair's dialog** was not opened by a check: the bench has no closed mesh that isn't a solid.
- **The view bar wraps to a second row** in a view narrower than its tools (about 540 px with a section open), and then
  covers the section's distances.
- **The view stays where it is.** With no tab there's nothing to drag it by; the docks move round it.
- **Window pictures** leave out scroll positions and web fonts, and are for the bench only.
