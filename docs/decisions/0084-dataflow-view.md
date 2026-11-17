# 0084 — The Dataflow view: a flow strip, the step, the block before and after

Status: agreed 17 November 2026. Tim picked layout B of three drawn as pictures and agreed the seven decisions on the
Build Plan's "Dataflow view plan" tab ("Yep layout b looks good", then "Go for it"). Workstreams 1 (what each step
wrote) and 2 (the Inputs row, the strip and Show as graph) are built here; 3 (the step pane, Before and After, the tab
renamed) is to come.

## Why

Tim, 17 November, with a screenshot of a Spry dataflow open on a snippet: "We dont need to copy it but i think the layout
is much neater - allows for cleaner understanding and shows the flow of data as well with the node connections." He had
said the same day that he wasn't convinced by the notebook's layout (0081): a list of cells, one a step, each showing one
unit, with the flow nowhere drawn. Only the layout was taken from the screenshot; nothing of the project in it.

## Decision

1. Layout B: one tab in the results dock, the Notebook's place, renamed Dataflow. An Inputs row and a flow strip across
   the top, and three columns under them: the picked step, the block's units Before it and After it. The list of cells
   goes.
2. The inputs head the strip: the block's solids, the seam model and the parameter tables. What the picked step reads
   is lit gold: the table and row in Inputs, its settings in the step pane, the fields it reads in Before and After. The
   fields it writes are underlined blue.
3. The strip carries the flow: each step's write to the reserves table under it, with the volume written, and the units
   carried on each arrow.
4. Before and After show every unit of the block, a unit a row and a field a column (the Reserves table keeps fields as
   rows).
5. The columns: the labels, then the fields the step reads and writes; All fields shows the rest.
6. Show as graph swaps the strip for the full graph on React Flow, laid out by run order, the tables wired to the steps
   that read them; zoom and pan only, nothing dragged or saved.
7. The strip wraps onto a second line when the steps don't fit.

The Live workings plan's Workings row (0082, workstream 2) goes in the step pane; its live rerun and the rest of the
steps' workings follow on this layout. Editing the dataflow from the strip or graph stays with 5c (0075).

## Built: workstream 1, what each step wrote

`StepView.writes` (`crates/zd-commands/src/builtin/reserves_run.rs`): a `StepWrite` a folder the step wrote to for this
column, in the order first written: the folder (`Dilution\Roof`, `ROM\Coal`), the units its pieces were written from
(or onto, where a step shares material out over a section's plies, as `final` does), and the volume written, its pieces
added up. The column as input carries Imported. Each piece of the table now knows its unit; the writes are read off the
pieces a step added when the column is opened (`table_column`, `reserves_preview`), so a reserves run is unchanged.

| Test | Known answer | Tolerance |
|---|---|---|
| `each_step_s_writes_add_up_to_the_reserves_table_folder_by_folder` (the 4c column, dilution only, run to `final`) | Imported 95,000 m³ waste (3 units) and 25,000 m³ coal (2); rom_dilution roof 500, floor 1,000, edge 10, other 250 m³, two units each; `final` ROM coal 10,854 + 15,906 m³; every folder's writes over the steps equal the table's total for it, and no volume in the table that no step wrote | 1e-9 relative |

## Built: workstream 2, the Inputs row, the strip and Show as graph

Over the Notebook's cells for now; workstream 3 puts the step pane and Before and After under them in place of the
cells.

- **The column's inputs from the engine.** `ColumnSteps` gives `blocks` (the column's block solids by name, top down),
  `model` (the seam model the run named, by name; none where it gave its own stack) and `horizons` (how many the stack
  holds).
- **The logic** (`app/src/panels/dataflowView.ts`): `stripSteps` (a step a chip, the column as input named Imported, its
  units passed on, its writes under their top folder, `Dilution` from `Dilution\Roof` and `Dilution\Floor`, volumes added
  up); `inputsOf` (the blocks, the model or the run's own stack, and each parameter table; the blocks and stack lit when
  the column as input is picked, a table lit with the columns a built-in step read from it, for the unit followed where
  it read any, else for every unit; a formula's reads are in its trace by name and light nothing); `tablesRead` (what the
  graph wires); `volumeShort` (412.3k, 1.1k, 350).
- **The strip** (`DataflowStrip.tsx`): Inputs, then the steps wrapping as they need to; a formula marked ƒ and dashed, a
  step that changed nothing quieter; under each step its writes with their volumes (the folders in full in the tip); the
  units passed on under each arrow. A step clicked is picked, lit gold, its cell opened, marked gold down its left and
  brought into sight.
- **Show as graph** (`DataflowGraph.tsx`, React Flow): the inputs across the top, the steps below in rows of six in run
  order, each write a node under its step, the blocks wired to the column as input and each table wired, dashed, to the
  steps that read it, the picked step's wires gold. Not dragged; zoom and pan. A step clicked picks it.

| Test | Known answer |
|---|---|
| `a_table_s_bench_opens_its_block_s_column_step_by_step` (engine) | B2's five block solids top down, no seam model, as many horizons as the run's own stack |
| `dataflowView.test.mjs` (6) | writes under their top folder (Dilution 1,500 m³ from roof 500 and floor 1,000); Imported first with its units; short volumes; nothing lit with no step picked; the blocks and stack lit for the column as input; a built-in step's table and columns lit for the unit followed, else every unit; a formula lighting nothing |
| `zd-app --bench --dataflow` (16 Nov demo, 3_B1) | a chip a step in run order, Imported writing Imported, rom_dilution Dilution, final ROM; units on every arrow; rom_dilution clicked picked, its cell marked, Reserves assumptions and its column lit; Show as graph a node a step, the writes, the tables wired; a step clicked in the graph picked |
