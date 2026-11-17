# 0081 — A block's column as a notebook

Status: agreed 17 November 2026. Tim agreed the seven decisions on the Build Plan's "Column notebook plan" tab ("Agree
all 7, go ahead with workstream 1"). Workstream 1, the engine, is built here; the notebook tab (2) and running on the
formula as edited (3) follow.

## Why

Tim, 17 November, on Open its column (0075, 5b) as merged: stepping across makes sense, but he expected to see what
causes each change, "essentially like a jupyter notebook - whats gone in and the resulting output which the script has
caused". Open its column showed what changed (a value in blue with what it was), not why.

## Decision

1. The notebook is the way into a column: Open its column opens it, and today's grid stays as Whole column, the full
   state at one step.
2. It's its own tab in the results dock, Notebook, opened from the Reserves table and from the formula panel.
3. One unit is in focus for In and Code: the picked seam or bench's top unit, else the column's first coal unit; a click
   in Out moves it.
4. A built-in step shows its rule in words and the parameters it read, each with its value and the table and row it came
   from. Its Rust stays hidden (0077).
5. Out shows only what the step changed, with the column's volume in and out.
6. Steps that changed nothing fold to one line.
7. From the formula panel, Run in notebook reruns the column on the dataflow as edited, not saved. Nothing is kept.

A notebook cell is the step panel of 0075's decision 4, built once and used by the notebook and by the dataflow canvas
(5c), whose own work narrows to the canvas layout, switching a step off, and Run reserves. Copy for AI from a cell
carries names and formula text only, never a block's values.

## Built: workstream 1, what each step read

- **Each step of a column as a cell** (`StepView`, as `table_column` and `reserves_preview` give it): its `kind` (the
  column as input, built in, or a formula), a built-in step's `rule` in words (its line in `STEPS`), a formula's filter
  (`when`) and script (`source`), the units the step took (`fits`: a formula's filter as the column stood before it;
  every unit for a built-in step), and the parameters a built-in step read (`reads`).
- **Every parameter a step reads goes through one lookup** on the column (`ColumnRun::lookup`), numbers and words
  alike. While a column is opened as a notebook it records each read: the unit, the column, what it found, and the
  table (its place in the list read) and row that gave it, or nothing where no row gives it. A run records nothing.
  The tables read are named on the column (`ColumnSteps::tables`), the run's own rows first where a case gave some.
- **`column_trace`** traces a formula step on any unit of a table's column, by its place in the notebook and the unit's
  id: the column run up to the step, then the formula traced on the unit as it stands there. A unit the step's filter
  leaves out comes back with no lines, as no line ran for it; a built-in step is refused, having no lines.
- **On the dataflow as edited**: `table_column` and `column_trace` take a dataflow as given, unsaved, and run on it with
  the table's run's settings (`ColumnSteps::edited` says so). Nothing is kept. A dataflow given is checked as one being
  saved is, so a formula whose worked examples fail is refused before it runs, as the formula panel's trace already is.
- **Tauri**: `reserves_column` takes the dataflow as edited, and `reserves_column_trace` is new.

Known answers (`crates/zd-commands/tests/reserves_run.rs`):

- `a_notebook_says_which_row_each_built_in_setting_came_from`: `rom_dilution` on the loss column reads A's four
  dilution settings (0.05, 0.1, 0.02 and 1) from the assumptions' third row (any coal) and B's from its second (coal, B),
  table 0; `small_volumes` with no `min_volume` column reads none for every unit, with no table or row; the column as
  input reads nothing.
- `a_notebook_traces_a_formula_cell_saved_or_as_edited`: `densities` traced on B1's U (RD 1.4, Mad 3 %, Mis 8 %) sets
  rd_is to Preston–Sanders' 1.358 ÷ 0.99, on a line that ran; `initialise` is refused as built in. Edited and not saved
  (rd_is doubled, the filter coal only, its worked examples taken off), U's rd_is after the cell is doubled, the cell
  takes U and L only, and OB, left out, has no lines; the saved dataflow still gives 1.358 ÷ 0.99.

On the 16 Nov demo, block 3_B1 (31 units after its 15 steps, 114 parameter reads): the notebook opens in 5 to 9 ms and
`densities` traces on a coal unit in 3 to 4 ms, its 11 lines run (release build).
