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
| `a_written_column_is_never_typed_over` | entering in `Insitu\Coal\volume` refused; words in a column of numbers refused; row 100 of 6 refused; a written column can't be taken out; an entered one comes out and one undo puts it back | exact |
| `attributes_typed_in_the_grid_revise_their_blocks_only` | three cells on two blocks: "2 values set, 1 taken away on 2 objects", each revised once, mesh and volume as they were; two blocks changed since the run; one undo puts both back; `volume` refused | exact |
| `a_table_holds_together_or_is_refused` (zd-core), the `.zdp` round trip (zd-io) | an entered column comes back entered through JSON and through the project file | exact |
| `tableGrid.test.mjs` | a value read for its column, a pasted block cut to a value a line, what's changed said in a line | exact |
| `zd-app --bench --dataflow` (demo pit) | two blocks' attributes set: two blocks changed since the run, none after one undo; a comment on two leaves stays on them when the whole table runs again, and two undos take it out; B99 typed in block 1_B1_OB_110's `block` cell in Objects is set, and one undo puts B1 back | exact |

## Built: workstream 4, update records

- **The engine** (`builtin/update_records.rs`): `match_records` reads a CSV (a header row, `name (unit)` where a column
  has a unit, then a record a row) and matches it on the key columns picked to objects or to a table's rows. A key is
  compared as words, a number as one (`03`, `3` and `3.0` the same). An object's key column is its name, its layer or
  an attribute; a table's, its column of that name. A record whose key several objects share sets them all (a block's
  every horizon by `block` alone).
- **The report** says, before anything is written: records and how many matched, the objects or rows they matched,
  those matched by none, the keys in the file only, and each value that would change with its old and new (the first
  200, all counted). It refuses, with every reason, a key the file hasn't, a key twice in the file, a solid's own name,
  layer, kind, volume or tonnes, a column the run wrote, a unit that isn't the column's, and words in a column of
  numbers. Nothing is written while it refuses.
- **`table.update_records`** writes what the report showed, one run, one undo: objects' attributes, or a table's entered
  columns, made where they aren't there with the file's unit. **Update** takes the file's values where a record matches,
  a blank cell leaving a value as it is; **Replace** replaces the file's columns, so a blank cell or a record the file
  hasn't is cleared. It says what matched and what's in the file only.
- **The app:** **Update records…** in the Tables tab, for Objects (the rows shown) and for a reserves table. The CSV is
  chosen, its columns ticked to match on (those the table has by name ticked first: a table's levels, a block's cut
  words), Update or Replace, the report shown as it's worked out, and **Update N values** writes it. A refusal shows
  why, the button off.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_csv_updates_blocks_matched_on_their_key` (made-up strip) | by block and horizon: 2 of 3 records matched 2 objects, 7 matched by none, B9 U in the file only; 3_B1_U's rd 1.4 → 1.45 and note → resampled, B2 L's blank row changing nothing; the run says so and one undo puts it back | exact |
| `a_record_s_key_matches_every_object_that_has_it` | `block` alone: 2 records, 9 objects; strip `03` matches strip 3's 9; refused together: no `seam` column, B1 twice, `volume` | exact |
| `a_csv_updates_a_table_s_entered_columns_on_its_leaves` | by block and seam: 2 leaves, 4 by none, B7 U in the file only, 3 values; period in months and comment made entered; Replace clears B2 L's period; days against months refused; a written column refused | exact |
| `tableGrid.test.mjs` | a CSV's columns from its header, units left off, a quoted comma kept; the likely key | exact |
| `zd-app --bench --dataflow` (demo pit) | three Base blocks by name take `checked`, "no such block" in the file only, undone; one leaf by strip, block, seam and bench takes a period of 4 months, undone; in Objects, a CSV chosen shows "1 of 1 record matched 1 solid" and Update 1 value sets zone east, undone | exact |

## Built: workstream 5, calculated columns

- **A table keeps its calculated columns** (`zd_core::CalculatedColumn`): a name, a unit, a script, how a group adds it
  up (`CalcAddUp`: from its inputs, sum, largest, smallest, or weighted by a column) and the parameter tables its `p`
  reads. They're kept as scripts with the table (the `.zdp` keeps them beside its columns' headers), worked out when
  shown, never stored as numbers, and Run again keeps them.
- **The formula is a dataflow formula on a row** (`table_calc.rs`, 0077's engine unchanged): `def formula(row, p):`
  setting the column on the row, `row.strip_ratio = row["Insitu\\Waste\\volume"] / row["Insitu\\Coal\\volume"]`. It
  reads the table's numbers by column (the folder names in brackets), its words (`row.seam`), and its parameter tables,
  a row's values the first row of them that fits its words. It's checked against the table as it's typed, every problem
  on its line, and one that never sets its column is said.
- **How a group adds it up:** from its inputs, the formula is worked again on the group, each number it reads added up
  by that column's own rule and each word kept where every row has the same, so a strip's ratio is its waste over its
  coal. Otherwise each row's value is summed, the largest, the smallest, or averaged weighted by a column.
- **A row it stops on** (dividing by an empty coal, words where a number is) has no value, and the rest go on; the
  grid says how many and the first, with its line. Rows are run 256 at a time, and only a batch the formula stops in a
  row at a time.
- **The grid** (`grid_with`) works out each calculated column it shows or sorts by over the leaves the filter keeps.
  Its head is in italics with ƒ to open it and a cross to take it out.
- **`table.calculate`** puts a calculated column on a table or changes one (renamed through `replaces`), checking it
  first; a name a column of values has, a weight that isn't a column of numbers, and every problem with its line are
  refused. `table.remove_column` takes one out.
- **The app:** **+ Calculated** in a reserves table's Grid opens its editor: name, unit, how a group adds it up, the
  formula in the script box with its problems on their lines as it's typed, **Run on** the picked leaf line by line,
  and **Copy for AI** and **Explain**, which carry the table's names and this formula, never a value. A new column's
  `p` reads the parameter tables its table's dataflow reads.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_ratio_adds_up_from_the_group_s_own_inputs_not_its_rows` (made-up table) | blocks at 100/10 and 300/50: 10 and 6, strip 1 400/60 = 6.67, not 8; the total 450/60; strip 2, no coal, nothing, said with its row and line | 1e-12 |
| `a_test_on_each_row_is_added_up_by_its_rule` | tonnes where ash is under 12, summed: 10; ash weighted by tonnes (10 × 10 + 20 × 50) / 60; the largest 20 | 1e-12 |
| `a_row_s_parameters_are_the_first_row_that_fits_its_words` | a price of $80 for U and $60 otherwise: 800 + 4,000 = 4,800 | exact |
| `a_formula_is_checked_and_traced_as_the_dataflow_s_are` | a name the table hasn't, on line 2; a formula that never sets its column; the names given; traced on B2, line 2 sets 6; on B3 it stops at line 2 | exact |
| `rows_a_formula_stops_on_leave_the_rest_their_values` | 600 rows, every third with no coal: 200 stop, the first said, the other 400 their own value across batches | exact |
| `a_calculated_column_is_put_on_a_table_and_kept_through_run_again` (made-up strip) | B1's strip ratio 55,000 / 50,500; a clashing name and a bad formula refused; kept through Run again; taken out and back with one undo | 1e-9 |
| the `.zdp` round trip | a calculated column comes back with its script and rule | exact |
| `tableGrid.test.mjs` | a new column's starter formula; Copy for AI's names, rule and script, no value | exact |
| `zd-app --bench --dataflow` (demo pit) | a waste-to-coal column on Base: every strip's and the pit's is its own waste over coal (worst 0), the leaves with no coal said; traced on a leaf it sets itself; one undo takes it off | 1e-12 |

## Built: workstream 6, scripts as objects

- **A script is an object of the project** (`zd_core::Script`, `Body::Script`, kind `script`): a name, a line on what
  it's for, and its Starlark, kept in the `.zdp` (and the older folder store) as any object is, revised and undone.
- **`load("name", "function")` among the project's scripts** (`zd-script`'s loader): a formula, a calculated column
  or a script loads another by its name, which may load others in turn; each is loaded once a run, with the same library
  as a formula. A name the project hasn't is refused (`can't load x: the project has no script of that name`), as is a
  script that loads itself, its chain said (`helpers loads itself, through helpers → helpers2 → helpers`). Still no
  files, network, clock or imports.
- **What loads one keeps its id**: a dataflow's and a calculated column's `scripts`, bound by name from their `load`
  lines when they're saved (`bind_loads`, through the scripts those load too). A run reads them by id, so the run records
  each script's revision among its inputs; a script revised, the next run reads it as it is. A dataflow or column edited
  but not saved is bound the same way, so the Dataflow tab's live run, a formula's check and its trace all load it.
- **`script.save`** makes a script or its next revision, checked first as a library (`check_library`: it parses, and
  what it loads is there and loads cleanly), every problem on its line; two scripts of one name are refused, as a load
  couldn't tell them apart.
- **The app:** a **Scripts** list in the Parameters tab, under Parameter tables and Dataflows, with **+ New script**.
  Its editor: the name a load asks for, what it's for, the Starlark checked against the project's other scripts as it's
  typed, Copy for AI, and Save. Properties shows a script's Starlark.

Decision 11's "a script runs three ways" lands with the table scripts it runs (workstream 7, below).

| Test | Known answer | Tolerance |
|---|---|---|
| `a_formula_loads_its_functions_from_the_project_s_scripts` (zd-script) | `twice_ash` through `helpers` and `maths` doubles 12 to 24, traced through the loaded function; a missing script and a self-load refused with their words; a script's problem on line 3 | exact |
| `the_app_s_starter_script_checks_clean` (zd-script) | the app's starter script checks clean, and `by_tonnes` adds 10 % and 20 % over 1 t and 3 t to 17.5 % | exact |
| `a_script_s_loads_are_read_off_its_source` (zd-core) | `qualities` and `limits` read from its `load` lines, a commented one passed over | exact |
| `a_formula_step_loads_a_project_script_and_the_run_records_its_revision` (formula_steps.rs) | ash2 = 10 × 2 + 35 = 55 by a loaded function; the dataflow keeps the script's id; the run names revision 1; revised, the next run gives 56 and names revision 2 | 1e-9 |
| `a_script_and_what_loads_it_are_checked` | a load of a missing script refuses the dataflow; a script's problem on line 3; two of one name refused | exact |
| `a_calculated_column_loads_a_project_script` (reserves_run.rs) | the strip ratio by a loaded `ratio`: B1 55,000 / 50,500, the column keeping the script's id | 1e-9 |
| `zd-app --bench --dataflow` (demo pit) | a script loaded by a calculated column on Base gives every strip its own waste over coal; + New script in the Parameters tab opens one that checks clean | 1e-12 |

## Built: workstream 7, table scripts

- **A table script** is Starlark defining `def run(t, p):`, given a table and giving one back (`zd_script::TableScript`,
  compiled once, checked with every problem on its line). `t` is a read-only handle; each call makes a new table in
  Rust: `t.where(...)` in the grid's filter words (0088), `t.group([...])` in the grid's order with each number added up
  by its own rule (`add_up_column`, as every subtotal is), `t.total(column)`, `t.join(other, on=..., prefix=...,
  how="left"|"outer")`, `t.with_column(name, values, unit=..., adds=...)`, `t.select`, `t.drop`, `t.rename`, `t.sort`,
  `t.row(i)`, `t[column]` as a list. `table("name")` reads another table of the project; `new_table({...})` makes one
  from lists. `adds` is in words: `sum`, `max`, `min`, `volume`, `mass of rd`, `ratio of <column> per <column>`. A
  number with no rule is empty in a group, and the run says so; a join's unmatched rows and repeated keys are said too.
  The engine's work comes in through `zd_script::TableWork` (`GridWork` in zd-commands: the grid's add-up and order).
- **As a task, `table.script`**: a script of the project by id, or one written in, on a table, with the parameter tables
  `p` reads (the first row that fits no words in particular). `into: new` (the default) makes a table of its own on the
  `tables` layer, named for the table and the script; `into: columns` puts its new columns on the table it ran on as
  entered columns, one undo, and is refused if it sorted, filtered or grouped the rows, or changed a column the run
  wrote or a person entered (decision 3). The run records the table, the script and what it loads, the parameter tables
  and every table it read by name.
- **As a dataflow step**: a step with a `table` script (`zd_core::TableStep`) sits after `final`, nothing but table
  steps after it, never with a built-in's name; `dataflow.save` compiles it and binds what it loads. `reserves.run` runs
  each one on every table it makes or runs again into, on the whole table (a block run again is worked over every row),
  its new columns the run's own (written, so the next run writes them again). One that stops, or gives back other rows,
  is said (`reserves.table_script_stopped`) and its columns left off; the table still stands.
- **As a workflow step**: any command is one (0065), so `table.script` after the reserves run that makes its table.
- **The app**: a script of the project that defines `run` has **Run on** (a table), **New table** or **Columns on it**,
  and **Run**, as saved; the result opens in the Tables tab. The Parameters tab's dataflow steps list table steps with
  their own mark, **+ Add a table script after final**, and an editor beside them checked as it's typed. Copy for AI
  hands over the reference, now with a **Writing a table script** section, and the script.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_table_script_groups_and_works_a_ratio_from_the_sums` (zd-script) | strip 1's ratio 400 / 60 from its sums, not the average 8; strip 2 with no coal None | 1e-12 |
| `a_join_lists_each_difference_beyond_tolerance` (zd-script) | against a made-up Vulcan export with three planted differences: B2 by 2 t, B3 only ours, B4 only theirs, exactly those; B1 agrees | exact |
| `filters_sorts_totals_and_rows_are_rust_s`, `a_table_script_s_mistakes_are_said` (zd-script) | filter, sort, total 60, a row; each mistake in words with its line | exact |
| `every_table_script_in_the_reference_compiles_and_the_shares_add_to_one` (zd-script) | each reference example compiles; shares 0.25, 0.75 and 1 | exact |
| `a_table_script_grouped_by_block_equals_the_breakdown_s_blocks` | every number of each block's row equals the Breakdown's block node (volume, RD by volume, ash by tonnes) | 1e-9 relative |
| `a_table_script_puts_columns_on_its_own_table_or_is_refused` | leaf shares of the coal add to 1, entered, one undo; sorted rows and a run's column refused, nothing written | 1e-12 |
| `a_project_script_reads_another_table_and_a_parameter` | a reconciliation of two runs of the same blocks finds no block over 0.5 m³; the run lists both tables, the parameter table and the script | exact |
| `a_dataflow_s_table_script_step_writes_its_columns_after_final` | the step's ROM shares add to 1, written by the run; before final, not compiling, or a built-in's name refused at save; one that stops said, the table made | 1e-12 |
| `a_table_script_step_runs_on_the_whole_table_when_a_block_runs_again` | after B1 runs again, the shares over the whole table still add to 1 | 1e-12 |
| `zd-app --bench --dataflow` (demo pit) | a table script grouping Base by pit and strip gives each strip the grid's subtotals, and undoes | 1e-9 relative |

## Built: help while a script is typed

Tim, 22 November, asked for autocomplete or help writing scripts. Every script box (a dataflow's formula, a calculated
column, a script of the project, a table script step) now offers, as it's typed or on Ctrl+Space:

- after `t.`, a table's calls; after `unit.` or `row.`, the fields, labels and parts; after `p.`, the parameter columns;
- inside `t["`, the table's columns (the one picked to run on, or the newest), each `\` doubled as a string needs; inside
  `load("`, the project's scripts;
- by name, the library, Starlark's own functions, a table script's `table` and `new_table`, and the script's own names.

A call under the pointer says how it's called and what it gives, and the script and table step editors have the guide,
opened at their kind's section. The calls come from the engine (`zd_script::help`, served by the shell's `script_help`),
tested against what runs them: `dir(t)` names exactly the table calls offered, and every function offered compiles. It
uses `@codemirror/autocomplete` 6.20.4 (CodeMirror 6, the locked editor; Tim agreed, 22 November).

| Test | Known answer | Tolerance |
|---|---|---|
| `the_table_calls_offered_are_the_table_s_own`, `the_functions_offered_are_there` (zd-script) | the calls offered are exactly the table's, and every function offered is there | exact |
| `zd-app --bench --dataflow` (demo pit) | in a new script made a table script, `t.` offers group, where, join, total and with_column; `t["ROM` offers the table's `ROM\\…` columns; `in_si` offers in_situ_rd; the guide is the table scripts' | exact |

## Built: workstream 9, a dataflow from any table

Tim agreed four choices on 2026-11-24 ("Yes build them out - I agree all 4"): the mapping on the reserves table's setup
(0103), not the dataflow; a row is one block piece, as a solid is; imported said in the run, the audit and the report;
and `table.import` for CSVs.

- **`table.import`**: a CSV read into a table of the project (`table_of_csv`), the first line naming the columns (a unit
  in brackets), a column numbers where every filled cell is one, the rest words. Its columns are entered; one in m³,
  BCM or tonnes adds up by summing. In the app, Import › A table (CSV)… opens it in the Tables tab.
- **A table as the input** (`zd_core::TableInput`, `InputMap`; a run's `input`, a setup's `input`): a row a piece, its
  columns its attributes under their own names, each mapped one under the name the run reads (`seam` from
  `Seam_Name`). The volume comes from its column (always mapped); the thickness from a column, or the volume over a
  plan area column; the stack from the seam model, the typed stack, or an order column (smallest on top). A piece's id
  is the table's with its row, the same every run. The steps run as on solids: the pieces go in where the blocks did
  (`pieces_of_table` in `prepare_flow`), so Open its column, Every block and the trace read them the same way.
- **Refused before it runs, naming what's missing:** no volume column; a mapped column the table hasn't, or one of words
  for the volume; a row with no volume; a horizon the stack hasn't; a piece with no material. `table.setup` keeps a
  mapping not yet whole, so the Setup tab can finish it a column at a time, says what it lacks, and the run refuses it.
- **Imported is said:** the run's note `reserves.imported_volumes` ("volumes imported from Deswik export r1, not
  measured from solids"), the table's `Summary\Audit\input`, and the report's mark before its figures. A table read
  from solids carries no such column: solids are the volume of record (rule 9).
- **The Setup tab:** Run on, block solids or a table; with a table, the table read, its rows, and the mapping: the
  volume, thickness, plan area and order columns, and each attribute the run reads (the levels, `horizon`, `seam`,
  `material`, `bench`, `bench_floor`, `case`, each field's block attribute), its own name by default.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_dataflow_runs_on_a_table_through_a_mapping_and_says_its_volumes_are_imported` (reserves_run.rs) | the plan's case on made-up blocks: the same blocks as a CSV with a site's own names, imported and mapped: every Imported, Insitu and ROM total the solids' own; the run's note, the audit and the report say imported; by the file's order column with no stack, the same; kept as the setup, Run again reads the table and the Setup tab shows its 6 rows | 1e-9 relative |
| `a_mapping_that_cant_hold_is_refused_before_it_runs` (reserves_run.rs) | the plan's case: no volume column, a column the table hasn't, words for the volume, a row with no volume, a horizon not in the stack, no material: each refused naming it; an unfinished mapping kept by `table.setup`, said in the tab, refused by Run again; a dataflow as the table refused | exact |
| `a_csv_s_columns_are_numbers_where_every_filled_cell_is_one` (table_import.rs) | numbers and words told apart; m³ sums; a header with no rows refused | exact |
| `zdp` round trip (zd-io) | a setup's input and mapping saved and read back | exact |
| The demo pit (by hand, from Python) | Base's 1,815 blocks as a CSV of their attributes, imported (10 ms) and run by a mapping of volume and plan area (103 ms): Imported, Insitu and ROM coal and waste volumes and tonnes all the solids' own, largest difference 0; the run says imported | exact |

## Known limits

- Entered values carry through Run again into the same table; a fresh reserves run makes a new table without them.
- A paste fills one column, down from the cell pasted into.
- Update records reads CSV only; an Excel file would need a new dependency.
- A file can't add a record the target hasn't: a row, or an object.
- A calculated column can't read another. (The Objects table's own came with 0103, workstream 6; so did object
  scripts, `object.script`.)
- A calculated column has no worked examples of its own yet, as a dataflow formula's are.
- A load names a script by its name: renaming a script means editing what loads it, and saving that again.
- Nothing lists what loads a script yet; deleting one that's loaded is said when what loads it next runs.
- Every block runs on what's saved, not on the Parameters tab's unsaved edits, as the block shown does (0082).
- Objects' numeric attributes other than the density have no rule, so a group shows none for them: qualities on blocks
  add up only once a reserves run has given them one.
- Past 5,000 leaf rows under open groups, the rest are counted, not shown.
- A table script reads `p` for the table as a whole; a parameter by each group's words is worked in the script.
- A table script isn't traced line by line yet, as a formula is; it prints, and each problem has its line.
- A table a script made has no levels of its own, so Select its solids and Run again don't apply to it.
- `into: columns` from a task makes entered columns, kept by key across Run again but not worked again; a column that
  must follow every run goes in the dataflow as a table script step.
- A table read as blocks has no solids: Show and Isolate its solids find none, and a leaf's centroid, dip and width are
  what its columns give.
- Only a row a piece: a table already at leaves (coal and waste side by side) goes through a table script first.
