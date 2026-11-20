---
paths:
  - "app/src/**"
---

# Interface

Moved from CLAUDE.md. Decisions: 0026 (one layer tree), 0043 (colour by RL), 0076 (docks), 0080 (clicks, Esc,
shortcuts), 0087 (layers only), 0089 (lists in the app's look), 0091 (in place or as new), 0092 (tools ask in turn).

The shell is docks round the view (`docs/decisions/0076-docks-and-workflow-tabs.md`, the Build Plan's "UI improvements
plan" tab): the layer tree on the left, a row a layer saying what's on it by kind and never a row an object, what isn't
drawn (face designs, workflows, parameter tables, dataflows, reserves tables) found in its own tab's list with Rename and
Delete there, an object hidden or shown from the view's right-click in Select, and found by name in the Objects table
(`docs/decisions/0087-layers-only.md`); one tool dock on the right where every tool opens as a tab beside Properties,
and one results dock under the view where every table opens as a tab (Run log, Block table, Reserves table, Parameters,
Workflow canvas, Face design). A new panel names its dock in `PANELS` (`app/src/workspace/Workspace.tsx`) and opens
through `showPanel`; it never adds a column or a row of its own, and the view keeps its room. The view has no tab strip,
and its own tools (Select, Draw, Measure, Snap, 3D or Plan, the zooms, the field of view, Section and its step and fill)
are on the view bar over it (`app/src/panels/ViewBar.tsx`): a new view tool goes there, through `ViewTools`, never on the
toolbar, and nothing drawn over the view may sit behind the bar. Measure (M) reads the distance, RL difference, grade and
bearing between clicks from the points' world coordinates, and makes nothing
(`docs/decisions/0079-project-ramps-by-hand-and-measure.md`). In the view, left clicks add points and a right click
finishes (a right drag pans); Esc takes back one thing a press, nearest first, from a tool panel's box too, and says
what it did; Delete deletes what's selected in one undo (a vertex only under the cursor on the selected string); a
tool's shortcut may be two letters typed in turn (NS, a new string), never starting with a key that's a
shortcut on its own (`docs/decisions/0080-right-click-esc-and-two-letter-shortcuts.md`). A panel never says its name under its tab: its head is
one line of what it does (`PanelHead`), and its first control is within 40 px of the tab. Every tool is listed once in
`app/src/tools.ts`, in its stage, with its name, what it does, and the panel it opens or the action App carries out;
the toolbar, its menus, the Panels menu and the workflow canvas are drawn from the list, so a new tool is added there
(and its action to App's table), never as a button of its own. The ribbon (`app/src/workspace/Ribbon.tsx`) is a tab a
stage (Project · Strings · Walls · Solids · Geology · Reserves) over the row of that stage's tools, named in words;
the rows are `ROWS` in the tool list, a row too long folds into More, the tool in use is lit, and switching stage
changes the row only. A new tool goes on its stage's row there. Find a command (Ctrl+K, `FindCommand.tsx`) searches
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
a new tool that takes inputs asks the same way. Colours come from the theme's tokens, one meaning each: gold picked, orange `warning`, blue `edited`,
red refused, green valid. Don't hand-roll an input, select, badge or colour a kit part covers; add to the kit instead.


## Hidden objects and how they look

- An object can be hidden on its own, under its layer's tick. That's display state saved with the project, not a
  revision and not undone. Hidden means not drawn, snapped to or cut by a section; a command that names the object
  still takes it (`docs/decisions/0026-one-layer-tree.md`). So is how it looks: surfaces and grids colour by RL in
  bands on round RLs, in the shader, over each one's own range by default, or in the layer's colour; and an opacity per
  object and per layer, which multiply (`docs/decisions/0043-colour-by-rl-and-opacity.md`).
- A layer's solids are drawn a hair nearer than they are (a depth offset), so where a solid lies on a surface, as the
  overburden's top on the topography or a block's floor on its seam grid, the solid shows and not patches of both
  (`docs/decisions/0060-the-app-at-scale.md`).
