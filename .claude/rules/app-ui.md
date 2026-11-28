---
paths:
  - "app/src/**"
---

# Interface

## The two sides: the map of places (decision 0108)

The app is two sides, Design and Tables, joined by the Objects table and a prepared input (the Build Plan's "Two
sides plan"). This is where things live. Workstreams 2 to 4 of 0108 brought it in; 0076's docks below hold for
Design. A plan names where its feature goes on this map; a feature that needs a new place
changes the map here first, in its own decision.

- **The top row, on both sides:** the project menu (New, Open, Save as, Import, Export, Licence, View controls), the
  Design or Tables switch, Undo and Redo, Find a command. On Tables, also the case and **Run**, the one place to run a
  case.
- **Design:** the ribbon of five groups (Strings, Walls, Surfaces, Solids, Geology), each tool in one group, with
  Move, Rotate, Mirror and Delete at the end of every row. On the left, the layers, face designs, seam models and
  workflows. In the middle, the view, its view bar, what tools work on at its foot, and documents opened over it (Face
  design, a workflow). On the right, the one tool slot, chosen by the ribbon with no tool list of its own, and
  Properties under it. Under the view, the Objects table and the Run log.
- **Tables:** no row of tools; one panel filling the window (`TablesSidePanel`, `tables` in `PANELS`). Down its left
  the list (`listOf` in `workspace/tablesSide.ts`), in the order the work flows: 1 Inputs (the Objects table, then any
  other table, such as one imported), 2 Cases, 3 Dataflows (and the scripts they load), 4 Parameters, 5 Results (each
  case's reserves table), 6 Schedule (its place, not built); it folds to its six numbers for room. In the middle, the
  item picked: the case a step at a time, the Parameters editor with no list of its own, a table's grid. Each section
  is an item in `PANELS` (`dock: 'item'`), so Find a command and `showPanel` open it there; what's open and the case
  the top row runs live in `tablesSide.ts`. The 3D view only on demand: Show in 3D on the case's picked block. The
  picked unit opens under its row in the units table (0107).
- **Both at once, by the pop-out** (0109): the switch reads Design · Tables, with no Both (taken out on Tim's word, 28
  Nov). To see the table work and the design together, the Tables side pops out into a window of its own, from the
  ribbon's button beside the switch or its list's head (dockview's pop-out into an app window the shell makes,
  `on_new_window`); the main window stays on Design with the case and Run on its ribbon, and a block picked on the
  case's map is picked in the view. Brought back by either button or Tables on the switch, it's on Tables in the main
  window; closed by its X, Design stays. A menu, list or tip opens in the window it's used in (`PortalRoot`, or the
  kit list's own box's page), so a new one drawn into a portal must do the same; keys there reach the app's shortcuts.
- **One home each:**
  - What a tool works on (Picked, Its layer, On show): the foot of the view (`WorksOnBar`, `panels/worksOn.ts`); Cut
    solids and Interrogate read it, and say what it takes where their own choice was. A new tool that takes solids
    reads it too.
  - Running a case: Run in the top row (`CaseRun`), on the case picked there or opened in the list.
  - Editing a settings table: Parameters. Elsewhere it's shown and links there.
  - A workflow: one document.
  - Shaping design outputs for a dataflow: a prepared input, under Inputs.
  - The Objects table: under the view on Design; read by Inputs on Tables.
  - Design scenarios: layers and workflow cases on Design. A case on Tables names the input it reads.
- **The look:** light by default, dark as a setting saved on the machine (View controls, Look). Panels take their
  colours from the tokens in `app.css` (`:root` light, `.dark` dark); the view's colours are read from `view` in
  `viewport/look.ts` when drawn, never from a constant. A part that draws differently in each look reads `useLook`.

House rules:

1. **Features add to places, not new places.** No new dock tab, band or inner tab strip without a change to the map.
2. **One works-on.** Tools read it from the foot of the view; none draws its own.
3. **Sketch the whole side, then plan.** A plan that changes a screen starts with a picture of the whole side with
   the change in it.
4. **Used before it's built on.** A screen gets no follow-up plan until Tim has used it by hand, on PIT B or the demo.
5. **One UI plan open per side.**

## Today's docks (0076, until 0108 moves them)

Moved from CLAUDE.md. Decisions: 0026 (one layer tree), 0043 (colour by RL), 0076 (docks), 0080 (clicks, Esc,
shortcuts), 0087 (layers only), 0089 (lists in the app's look), 0091 (in place or as new), 0092 (tools ask in turn).

The shell is docks round the view (`docs/decisions/0076-docks-and-workflow-tabs.md`, the Build Plan's "UI improvements
plan" tab): the layer tree on the left, a row a layer saying what's on it by kind and never a row an object, what isn't
drawn (face designs, workflows, parameter tables, dataflows, reserves tables) found in its own tab's list with Rename and
Delete there, an object hidden or shown from the view's right-click in Select, and found by name in the Objects table
(`docs/decisions/0087-layers-only.md`); on the right one tool slot, a tool at a time, over Properties (`dock: 'inspector'`;
a tool asked for closes the one before it, and its tab is named for the tool, decision 0108),
one results dock under the view (the Run log, and Tables: the Objects table, or any table picked at its head;
Face design and the workflow, its canvas beside its steps, open over the view as documents, `dock: 'document'`; the Dataflow and Setup tabs are gone into the Reserves case tab, 0105 workstream 7, and a
saved layout's copy closes as it opens, `RETIRED`; so do the Reserves case and Parameters tabs, items of the Tables side's list since 0108). The case view (0105), Cases on the Tables side, edits settings through the same unsaved edits
(`drafts`) as the Parameters tab, which lays another tab's edits on its grid (`withTable`); a tab that edits a parameter
table goes through `setDraft`, never a store of its own. Its layout is 0106's: one header line (+ for New case, Copy case and Add a dataflow,
the case's dataflows as a chain with Setup; the cases are the list's and Run the top row's, 0108, the open case's figures and the balance), the flow across the width
as the step list, then the step, the units table and the block map side by side to the foot of the dock (folding under
each other under 1,400 and 1,000 px, by the tab's own width); each part said once, help in tips, not lines. Run (the top row's `CaseRun`) lists the case's dataflows (`case_state`) and runs them by
`reserves.run` and `table.run_dataflow`, as Python's `run_case` does, each its own undo; a row dataflow opens in the same tab from the chain. Its cases (`CasesBar`) make New case, Copy case and Add a dataflow
from existing commands, each its own undo, and its Setup (`CaseSetup`) is the old Setup tab fixed to the case open, its own view in place of the working area with Done (0107): it has no Run, a change putting the case out of date for Run to run. Tables holds every table, picked from its Table list: Objects, then each reserves table, one grid
for both (`docs/decisions/0099-tables-and-scripts.md`). A new panel names its dock in `PANELS` (`app/src/workspace/Workspace.tsx`) and opens
through `showPanel`; it never adds a column or a row of its own, and the view keeps its room. The view has no tab strip,
and its own tools (Select, Draw, Measure, Snap, 3D or Plan, perspective or orthographic (P), the zooms, the field of view, Section and its step and fill)
are on the view bar over it (`app/src/panels/ViewBar.tsx`): a new view tool goes there, through `ViewTools`, never on the
toolbar, and nothing drawn over the view may sit behind the bar. A turn or wheel zoom works about `StringLayer.pivot`: a string near the cursor or the surface under it, whichever is nearer the camera (a string within 1 m of the surface wins); over nothing, the nearest thing on screen by a fan of rays (`snap_rays`, `viewport/pivot.ts`), else the last pivot while it's in view, else the view's centre; and the view holds drawing while that pick comes back (`docs/decisions/0044-camera-feel.md`). A pan in perspective picks the same way and pans at that point's depth, so the point grabbed stays under the cursor. Orthographic 3D draws with a camera that copies the 3D one after every move (`Viewport.rig`, `syncCamera`): moves work on the rig, and code that needs a pixel the same size everywhere asks `noPerspective`, not `flatView` (plan or section). Which button turns, pans or boxes, and the view's six keys (hold to turn Z, hold to pan Space, F, E, T, P), are View controls (`viewport/viewControls.ts`, Spry's layout or ours, saved on this machine): read keys through `isViewKey`, never a letter written in, and a new shortcut must not take a view key. Only what's on show is indexed for snapping, after every change, a big surface in pieces across the cores, and the status bar says so while it's getting ready. Strings, polygons and roads are hidden behind the solids and surfaces in front of them by default (`Viewport.stringLines`, a hair nearer than they are), with Over everything in the field-of-view menu; a click doesn't pick a string it can't see. The view's own aids (the picked string, handles, the string being drawn, the section guide) stay over everything. Licence (decision 0098) is a tool on the Project row, its panel saying who the copy is licensed to, its modules and dates and the machine's code; without a licence the app is the Viewer and a command outside the licence is refused naming its module, and the status bar says when a licence is ending, in grace or missing. Measure (M) reads the distance, RL difference, grade and
bearing between clicks from the points' world coordinates, and makes nothing
(`docs/decisions/0079-project-ramps-by-hand-and-measure.md`). In the view, left clicks add points and a right click
finishes (a right drag pans); Esc takes back one thing a press, nearest first, from a tool panel's box too, and says
what it did; Delete deletes what's selected in one undo (a vertex only under the cursor on the selected string); a
tool's shortcut may be two letters typed in turn (NS, a new string), never starting with a key that's a
shortcut on its own (`docs/decisions/0080-right-click-esc-and-two-letter-shortcuts.md`). A panel never says its name under its tab: its head is
one line of what it does (`PanelHead`), and its first control is within 40 px of the tab. Every tool is listed once in
`app/src/tools.ts`, in its stage, with its name, what it does, and the panel it opens or the action App carries out;
the toolbar, its menus, the Panels menu and the workflow canvas are drawn from the list, so a new tool is added there
(and its action to App's table), never as a button of its own. The ribbon (`app/src/workspace/Ribbon.tsx`, decision 0108)
has along its top the project menu (`PROJECT_MENU`), the Design or Tables switch (`showSide` in the workspace, the side
remembered a project), and on Design a tab a group (Strings · Walls · Surfaces · Solids · Geology) over the row of that
group's tools, named in words, with `EDIT_ROW` (Move, Rotate, Mirror, Delete) at its end. The rows are `ROWS` in the
tool list, a row too long folds into More, the tool in use is lit, and switching group changes the row only. A tool is
on one row; a new tool goes on its group's row there. A panel names its side in `PANELS` (`side`: `tables`, or
Design by default): asking for one on the other side switches side. Find a command (Ctrl+K, `FindCommand.tsx`) searches
the list by whole words, so a tool's name and its line are what it's found by: write them in the words a planner would
type. Properties shows only: an object's name, look, attributes, measures and history. A command on an object is a tool
in a panel on the kit, never a row in Properties (the string tools are the Strings panel, which works on every string
picked as one run, `docs/decisions/0078-strings-as-one-run.md`). All seven workstreams of 0076
are built.

The UI plan (the Build Plan's "UI plan" tab) keeps the app one way of working as it grows. A tool panel is made from the
kit in `app/src/design-system/kit/`: `ToolPanel` (what it does in one line that fits the 400 px dock, with more in a tip, its sections, one button at the
foot named for what it does, Ctrl+Enter), `PanelHead` for a panel that isn't a tool, `NumberInput` with its unit, `NumberOrBlank` for a number that may be left blank, `TargetInput` for an RL, grid or surface, `Choice` (in `columns` where its options don't fit one row), `Select` (its choices in the app's own list, never the system's, `docs/decisions/0089-lists-in-the-apps-look.md`), `FilterInput`, `Field`, the badges,
`RunResult`, and `ToolButton` for a button on a bar of tools. A tool that changes objects sets `result` on its `ToolAction` (`useResultAs`) and adds
`resultParams` to what it runs: the panel then shows the one Result control, In place or As new on a layer
(`docs/decisions/0091-in-place-or-as-new.md`); a tool that makes something doesn't. A tool that takes inputs asks for them in
turn (`useAsking` and `AskSteps`, `docs/decisions/0092-tools-ask-in-turn.md`): what to work on, then what with, then it
runs and asks again; Esc steps back a prompt a press, and it asks only while its panel is in front. The button always
runs with what the panel has. A click on a string at a prompt picks it too, so a tool takes what it works on as
the prompt opens, and the project as it stands when the click lands. Clip, Trim, Extend, Split at a click, Where surfaces meet, every Cut solids mode, and Compare, Merge, Trim in plan and Triangulate do;
a new tool that takes inputs asks the same way. Colours come from the theme's tokens, light and dark alike, one meaning each: gold picked, orange `warning`, blue `edited`,
red refused, green valid. Don't hand-roll an input, select, badge or colour a kit part covers; add to the kit instead.

Since 0108 workstream 6 every panel and dialog is on the kit, not only the tool panels, and `app/tests/kitEverywhere.test.mjs`
fails a panel that isn't: no shadcn `Input` or `Checkbox`, no plain `<input>` but a file, colour or range, no system
`<select>`, no look-alike of a kit part of its own (`NumberBox`, `SmallButton`, `Choice`, `Section`, `Field`…), no
`Dialog.Root` of its own. The parts it added: `TextInput` (words, `sm` for a row; with no `value` it holds what's typed
till it's left), `Check` (a tick, its words beside it as children), `PanelButton` (a button in a panel, `row` at 24 px),
`TextButton` (words as a button: Open, + Add a step), `Disclosure` (▸ a line that opens what's under it),
`InlineField` (Width, m [50] in a line), `SectionHead` (1. Lines, a long panel's part), `ShownSection`, `ShownRow`,
`FlagBadge` and `PercentSlider` (what a panel shows and doesn't edit, as Properties), and `KitDialog` with `DialogList`
(every dialog's frame: title, a line, buttons at the foot). **What a run, save or edit did is said one way**: a `Said`
(`ok`, `warning`, `refused`, `quiet`, or `saidOf(reply)`), drawn by `SaidLine`, beside the button, never a colour a
panel picks; a run's notes are `RunResult`'s. Every panel has its head line, Layers, Properties and the Run log
included; the Tables panel's head is its bar, the table picked first.


## Hidden objects and how they look

- An object can be hidden on its own, under its layer's tick. That's display state saved with the project, not a
  revision and not undone. Hidden means not drawn, snapped to or cut by a section; a command that names the object
  still takes it (`docs/decisions/0026-one-layer-tree.md`). So is how it looks: surfaces and grids colour by RL in
  bands on round RLs, in the shader, over each one's own range by default, or in the layer's colour; and an opacity per
  object and per layer, which multiply (`docs/decisions/0043-colour-by-rl-and-opacity.md`). So are the Reserves case
  tab's own field sets (`Display.field_sets`, `set_field_sets`, 0105): saved with the next Save, never a revision.
- Surfaces and grids are drawn a hair farther than they are (a depth offset), so where a solid lies on a surface, as
  the overburden's top on the topography or a block's floor on its seam grid, the solid shows and not patches of both.
  Solids are drawn where they are: pulled nearer, one block's floor came through the next one's wall
  (`docs/decisions/0060-the-app-at-scale.md`).
- Far off, a big surface's chunks are drawn with coarser versions, each the coarsest out by no more than the Detail in
  pixels (the field-of-view menu: Full, 1 px default, 2, 4); close in, full detail. Versions are extra meshes beside
  their chunk, sharing its vertices. Anything that reads drawn triangles (as `distanceTo` does) passes over
  `userData.level` meshes; picks, snaps and sections are engine-side and never see them
  (`docs/decisions/0097-fewer-triangles-drawn.md`). A tile of solids sends the walls two of them share as chunks marked
  `shared`, drawn only while one of the tile's solids is hidden or the layer is seen through; its versions lock every
  block edge, and a picked block's highlight uses the full chunks and the shared walls.
