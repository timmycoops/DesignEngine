# 0084 — The Dataflow view: a flow strip, the step, the block before and after

Status: agreed 17 November 2026. Tim picked layout B of three drawn as pictures and agreed the seven decisions on the
Build Plan's "Dataflow view plan" tab ("Yep layout b looks good", then "Go for it"). Workstream 1 (what each step wrote)
is built here; 2 (the Inputs row and the strip) and 3 (the step pane, Before and After) are to come.

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
