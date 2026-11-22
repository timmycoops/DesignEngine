# 0099 — Tables and scripts: one table concept, editing, update records, scripts beyond the dataflow

Date: 2026-11-22 · Status: accepted (Tim, 2026-11-22: "Agree all 12", on the Build Plan's "Tables and scripts plan" tab)

## Why

Tim, 22 November:

> The Reserves table we have built is a great start but at the moment i find it a little clunky with how the data is in
> the dataflow and how you have to load up the column to see it. I also thinking the scripting capability of the
> starlark language shoudl allow us to do more without the dataflows themselves. Im thinking we shoudl be able to edit
> fields in the table, import data into it - update records, calculated fields / Starlark with scripts etc on top of the
> dataflows.

And, on the plan as first drafted: "At the moment we have a Object table and a reserves table as well. Will we combine
this into a table thats the same logical concept?"

Before this, every view of the reserves data was one node or one block at a time: the Reserves table broke down a node
(0075), the Dataflow tab opened a block's column (0084). The Objects table (0069) showed blocks' attributes and edited
nothing; no command set an attribute; a rerun replaced a results table whole; and Starlark ran only as a dataflow step at
four fixed stages (0077).

## Decision

The twelve decisions of the plan tab, as agreed:

1. **One table concept, one grid.** A table is rows with a key and columns. The blocks table (a row a solid, its
   attributes a live view, nothing copied) and a reserves table (a row a leaf, kept as Parquet) are the same concept,
   seen in one grid: grouped by any word column, a subtotal a group added up by each column's own rule, filtered in
   0088's words. A Tables tab lists every table; a reserves table keeps Breakdown and Balance beside its Grid.
2. **Three kinds of column:** written by a run (read only), entered (typed or imported, kept by the row's key across
   reruns), and calculated (Starlark on the row, worked when shown).
3. **A number a run wrote is never typed over.** A reserve is changed through its inputs, a parameter row, a calculated
   column or a formula step, so every tonne traces to a rule.
4. **Blocks' attributes are edited in the grid**, each change an `object.set_attributes` run, one undo, mesh and volume
   untouched; a results table whose blocks changed says so, with Run again (0085). This changes 0069's "never by typing
   into a cell": typing becomes a command run.
5. **Update records from a CSV** by key, with a match report before anything is written.
6. **Calculated columns**, added up from a subtotal's own inputs (a ratio) or per leaf then by a rule.
7. **Table scripts** on whole tables (`table.script`), grouping, joins and add-ups in Rust.
8. **Object scripts** (`object.script`) that set attributes only.
9. **Scripts as project objects**, `load()` among the project's own scripts.
10. **Every block at a step** in the Dataflow tab, run on demand.
11. **A script runs three ways**: as a task, as a dataflow step (a new stage after the final writes for table scripts),
    or as a workflow step. What must be the same every case goes in the dataflow; a one-off runs as a task.
12. **A dataflow reads any table** through a field mapping; with no solids, its volumes are said to be imported, not of
    record (CLAUDE.md rule 9).

Nine workstreams, in the plan tab's order: 1 one table, one grid; 2 every block at a step; 3 entered columns and
attributes; 4 update records; 5 calculated columns; 6 scripts as objects; 7 table scripts; 8 object scripts; 9 a
dataflow from any table.

## Built: workstream 1, one table, one grid

Tim chose one Tables tab over keeping the two tabs (22 November).

- **The engine** (`crates/zd-commands/src/table_grid.rs`):
  - `grid(table, ask)`: the total, a subtotal a group of each word column grouped by (a seam in the stack's order, a
    bench top down by RL, anything else in natural order, B2 before B10), and the leaves under each innermost group
    opened, past 5,000 counted, not given. Every number subtotal is `add_up_column` over the group's leaves, the add-up
    the Breakdown and the report use; a word column shows its word where every leaf agrees. Siblings sort by any
    column, groups by their subtotals. A leaf is labelled by its levels' words that aren't grouped by ("U 130" under a
    strip and block), or its name.
  - `objects_table(project, ids, density)`: objects as a table, a row an object: `name`, `layer`, `kind`, a solid's own
    `volume` (summed), then every attribute, numbers where every value is one. With a density, it adds up by volume and
    `tonnes` (volume × it, summed) follows `volume`.
- **The shell:** `table_grid`, given a table object or a list of objects and a density.
- **The app:**
  - The Reserves table tab is now **Tables** (the same tab, so a saved layout keeps it), with a **Table** list at its
    head: Objects, then each reserves table. The toolbar's Objects table opens it on Objects (key
    `restable:objects`); its Tables tool on a reserves table. A layout saved with the old Objects table tab drops that tab
    as it opens.
  - Objects keeps everything the Objects table did and gains **Group by**: grouped, the rows shown come from the
    engine's objects table, volume and tonnes summed and the density by volume.
  - A reserves table gains **Grid** beside Breakdown and Balance, remembered on the machine. Group by sits in the side
    column in place of the tree, the table's levels but the last at first. Its columns are the fields Breakdown shows
    (Main fields or Every field, folded folders left out). A leaf or a group picked is the node shown, so Select and
    Isolate its solids and the Dataflow tab follow it. CSV saves the grid as shown.
  - The kit gains `GroupBy`; the grid is `TableGridView.tsx`, its logic in `tableGrid.ts`.

| Test | Known answer | Tolerance |
|---|---|---|
| `table_grid` unit tests, a made-up table of two strips and three blocks | Strip 1: 600 m³, RD by volume 920 ÷ 600, ash by tonnes 15,500 ÷ 920; its strip word kept, its two blocks' not; an open strip's leaves labelled "B1 U", "B1 L", "B10 U"; groups sorted by volume (ties in natural order); 2 of 4 leaves given past a limit of 2, 2 counted; a filter of seam U to 700 m³; a number column or a missing column refused to group by | 1e-12 |
| `the_grid_s_subtotals_are_the_breakdown_s_nodes` (`reserves_run.rs`, the made-up strip run to `join_benches`) | Grouped by pit, strip, block and seam, every group open: each group's every field equals the Breakdown's node at the same words, over 100 subtotals; B2's U leaves labelled "row 3", "18", "16" | exact |
| `the_blocks_as_a_table_add_up_their_volumes_and_tonnes` | The strip's 9 blocks: 180,000 m³ and 103,000 t by `rd`; coal 70,000 m³, 103,000 t, RD 103,000 ÷ 70,000; waste 110,000 m³ with no RD or tonnes | 1e-9 relative |
| `tableGrid.test.mjs` | Opening and closing groups (a group closed closes those under it), sorting up, down and back, cells to their unit, CSV exact | exact |
| `zd-app --bench --dataflow` on the demo pit | The 50m table by pit and strip: every strip's every field the Breakdown's strip node; Base's blocks by material: coal's volume its solids' own volumes added; the Tables tab draws the grid grouped and lists Objects | 1e-12 relative |

Timing, release build, on the demo pit's 50m table (3,027 leaves, every field): grouped by pit and strip with Pit open,
56 ms from the app, against 1,788 subtotals checked equal to the Breakdown's.

## Built: workstream 2, every block at a step

- **The engine** (`reserves_run.rs`): `units_at_step(project, log, table, step, unsaved)` runs every column of the
  table's case through its dataflow to the step (0 for the columns as input), with the settings of the run that made
  the table, read from the log as Open its column reads them, the columns across the cores. The units after it become a
  table: a row a unit, its column's words (the levels above the seam), a name, its labels (seam, horizon, material,
  bench, what it became, its working section and ROM section), then every field with its unit and the dataflow's own
  rule, so the grid adds them up as the run would. A column that stops on the way is said, and the rest go on. Nothing
  is kept in the project.
- **The shell:** `reserves_step_grid`, the units grouped as the Tables tab's grid. It keeps the last step's units while
  the project and the step stay the same, so opening or sorting a group doesn't run every block again.
- **The app:** the Dataflow tab's head has **This block | Every block**. Every block puts the grid in Before and
  After's place beside the step pane: the picked step's units for every block, grouped by the levels above the seam
  at first, the fields After shows (those the step reads and changes), All fields the rest. It runs on what's saved,
  and says so. A unit picked opens its block's column, back on This block.

| Test | Known answer | Tolerance |
|---|---|---|
| `every_block_at_a_step_is_each_column_s_units_at_it` (`reserves_run.rs`, the made-up strip) | As input, 9 units and B1, B2 90,000 m³ each; after `join_benches`, 10 units (B2's U benches one unit, an underburden a column), 280,000 m³, B2's U 70,000 m³; each block's units, by horizon and bench, the same as its own column opened at that step; past the last step refused | 1e-9 relative |
| `zd-app --bench --dataflow` (demo pit, Base) | As input, 1,815 units of 90 blocks, 76,003,157.44 m³, the blocks' own volumes, in 47 ms; at `working_sections`, blocks 3 B1 and 4 B2 have the same units and volume as their own columns (7 each), in 39 ms; Every block in the tab shows "After rom_dilution: 2,671 units of 90 blocks", and This block brings Before and After back | 1e-9 relative |

## Built: workstream 3, entered columns and blocks' attributes

- **A column says what made it** (`zd_core::ColumnKind`): written by a run, or entered by a person. It's kept in the
  `.zdp` with the column (as `made` in JSON, where `kind` is the values' own tag), and an entered column of words is
  no level of the table.
- **`table.enter`** enters values in a table's own column, made if it isn't there (numbers where every value given is
  one, or words when asked). A column the run wrote is refused, so is words in a column of numbers and a row past the
  table's. **`table.remove_column`** takes an entered column out; a written one is refused.
- **Run again keeps them** (`merge_rerun`): an entered column's values on the leaves run again are found by each
  leaf's key (its case and its levels' words). A value whose leaf the run no longer makes is said with its value
  (`reserves.entered_lost`), never dropped silently.
- **`object.set_attributes`** sets attributes on objects, every object revised once in one run, one undo; geometry,
  name, layer and a solid's volume as they were. Nothing takes an attribute away; the object's own name, layer, kind,
  volume and tonnes aren't attributes to type.
- **Changed since this run** (`changed_since`): the blocks revised since the reserves run that last wrote the table,
  and anything else it read, by name. A table's latest revision may now be an entry, not a run, so Open its column,
  Every block and Run again find that reserves run by walking back through the log (`table_run`).
- **The app:**
  - The kit gains `EditableCell`: a double-click (or Enter) types, Enter or Tab keeps, Esc puts it back, and a column
    pasted from a spreadsheet goes down the rows shown from there, in one run.
  - **Objects:** every attribute's cells are typed or pasted, in the list and the grouped grid. **+ Attribute** adds
    a column to type into. A value is read for its attribute: numbers for a column of numbers, a number where a new
    one reads as one.
  - **A reserves table's Grid:** entered columns follow the run's fields, their heads blue with a cross to take one
    out, and their leaves' cells are typed or pasted. **+ Column** makes one, of numbers or words. A line says what has
    changed since the table's run, with **Run again** (every block, on what's saved).

| Test | Known answer | Tolerance |
|---|---|---|
| `entered_values_are_kept_by_their_leaf_when_the_table_runs_again` (made-up strip) | a comment on B1 U and B2 L and a period on B1 L stay on those leaves when B1 runs again; Open its column still opens after an entry; with B1's L and U-L blocks deleted, "1 entered value on leaves this run no longer makes, not kept: Base Pit 3 B1 L: period = 2", the rest kept | exact |
| `a_written_column_is_never_typed_over` | entering in `Insitu\Coalolume` refused; words in a column of numbers refused; row 100 of 6 refused; a written column can't be taken out; an entered one comes out and one undo puts it back | exact |
| `attributes_typed_in_the_grid_revise_their_blocks_only` | three cells on two blocks: "2 values set, 1 taken away on 2 objects", each revised once, mesh and volume as they were; two blocks changed since the run; one undo puts both back; `volume` refused | exact |
| `a_table_holds_together_or_is_refused` (zd-core), the `.zdp` round trip (zd-io) | an entered column comes back entered through JSON and through the project file | exact |
| `tableGrid.test.mjs` | a value read for its column, a pasted block cut to a value a line, what's changed said in a line | exact |
| `zd-app --bench --dataflow` (demo pit) | two blocks' attributes set: two blocks changed since the run, none after one undo; a comment on two leaves stays on them when the whole table runs again, and two undos take it out; B99 typed in block 1_B1_OB_110's `block` cell in Objects is set, and one undo puts B1 back | exact |

## Known limits

- Entered values carry through Run again into the same table; a fresh reserves run makes a new table without them.
- A paste fills one column, down from the cell pasted into.
- Every block runs on what's saved, not on the Parameters tab's unsaved edits, as the block shown does (0082).
- Objects' numeric attributes other than the density have no rule, so a group shows none for them: qualities on blocks
  add up only once a reserves run has given them one.
- Past 5,000 leaf rows under open groups, the rest are counted, not shown.
