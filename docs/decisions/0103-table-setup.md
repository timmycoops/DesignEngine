# 0103 — Table setup plan: a reserves table holds what fills it, the chain from objects to reserves to schedule one way

Date: 2026-11-23 · Status: accepted (Tim, 2026-11-23: "Agree all ten", on the Build Plan's "Table setup plan" tab)

## Why

Tim, 23 November, on where a script's values come from: "Isn't this really a per table option, where you apply which
table has which dataflows, scripts or parameters?", and "the natural option is to just go for the table per setup".
Then, on how the tables reference the design: the Objects table is the reference data; once the reserves start there's
a fixed set of inputs; dataflows and scripts complete the fields; the schedule links to them by the processes it defines.

What filled a reserves table was spread over three places: the dataflow (fields, steps, parameter tables), the run that
made it (blocks, seam model, the case's own rows, kept only in the log) and the table (entered and calculated columns).
A table wanting one field from elsewhere needed a copy of the whole dataflow; the waste allocation starter was merged
into each dataflow it was added to; nothing showed, from a parameter table, what read it; and the starter's scripts read
`p.dip` from a row, past any setting.

## Decision

The ten decisions of the plan tab, as agreed. The chain goes one way: the Objects table is the live design; a reserves
table's Imported is a snapshot of it by leaf at the run (the fixed inputs); the setup works it through; Scheduled is the
hand-off, a section a process; the schedule links by process. Nothing downstream writes into the objects.

1. **A table holds its setup:** its link to the Objects table (which blocks, by layer and filter so new blocks are taken
   up, and which of their columns give which fields), the seam model or stack, the dataflows, the parameter tables and
   its field source overrides. Run again runs it; every run is still logged. An older table's setup is filled from the
   run that made it.
2. **Add-on dataflows, run in turn** at their stage on the same columns; waste allocation becomes one add-on made once.
3. **Field sources per table; scripts read fields** (`u.dip`, `u.width`), settings alone through `p`; the starter is
   rewritten so, each column choosing its own zones.
4. **Parameter tables per table, and Read by:** what reads each, columns no step reads and those shadowed marked; names
   unique; lookups kept by id under the name the script uses.
5. **A Setup tab** in the results dock.
6. **Python and workflows** use the same setup: `table.setup`, and a run from the table.
7. **Schedule tables** take the same shape when built.
8. **The schedule links by process:** a process names a reserves table and its Scheduled section; tasks are process by
   leaf; it never reaches past the reserves table to the objects. The contract 0100 builds on.
9. **Calculated columns on the Objects table**, worked when shown and never stored, mappable to fields.
10. **Object scripts** set attributes only; **table scripts** read the Objects table as tasks; **no dataflow writes into
    the objects**. 0099's workstreams 8 and 9 are folded in here.

Six workstreams: setup on the table; add-on dataflows; sources per table; parameters per table; the Setup page; the
Objects table's formulas and scripts.

## Built: workstream 1, the setup on the table

- **`zd_core::TableSetup`** on `Table.setup`: the dataflow, the blocks (`BlockPick`: layers and a filter in the bench rules'
  words, or exactly these ids), the seam model or stack, and the table's own parameter rows. The case is the table's own
  `case`. Kept in the `.zdp` with the table's headers (settings, not values) and compared when a project is saved.
- **Every reserves run writes it** onto the tables it makes. Its blocks are picked by their layers where they're every
  solid on them (`BlockPick::of_blocks`), else exactly those ids. Run again keeps it; a table made before setups takes
  the one its run was made from.
- **Run again runs the setup** (`rerun_params`): the dataflow, the blocks it picks now (so a block added to the layer is
  taken up), the model or stack and its own rows; the run's layer and name from the table. Everything that runs a table
  again reads it: the app's Run again, Open its column, Every block at a step, Python.
- **`setup_of`** gives a table's setup, held or filled from the run that made it.
- **`table.setup`** saves a setup as the table's next revision, one undo. Refused where it picks no blocks, names a
  dataflow or seam model that isn't one, orders columns by nothing, or carries rows that don't hold together.
- **Python:** `table_setup(table)` and `run_table(table, only=None)`.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_reserves_table_holds_its_setup_and_run_again_takes_up_a_new_block` (reserves_run.rs) | B1 and B2, every solid on `blocks`: the setup picks the layer, the dataflow and stack; B3's OB added to the layer is taken up by Run again, Imported waste up by its 50,000 m³; the setup kept | 1e-6 m³ |
| `a_setup_is_saved_on_a_table_and_what_it_picks_is_run` (reserves_run.rs) | a run on B1's blocks alone picks those ids; `table.setup` by the layer filtered to block B2 picks B2's 5 blocks; no blocks, a dataflow that isn't one, no model or stack refused, the table as saved | exact |
| `zdp` round trip (zd-io) | a table's setup saved and read back | exact |
| The demo pit (`workflow_demo.py`, built before setups, run by hand) | Base and 50m, their setups filled from their runs (one layer each, the seam model), run again from them: 1,436 and 2,847 rows, 455 columns each, every value the same but `Summary\Audit\run`, the run's time | exact |

## Built: workstream 2, add-on dataflows

- **An add-on** (`Dataflow.add_on`): a dataflow of only fields, words, parameter tables and its own formula, allocation
  and table script steps; no built-in step and no add-ons of its own (`check_add_on`). Saved on its own: its tables
  there, its loads and lookups bound, its allocation and table scripts compiled and its formulas parsed; a formula is
  checked against a main dataflow's names when one listing it is saved or run. Run on its own, it's refused.
- **A main dataflow lists its add-ons** (`Dataflow.add_ons`), in order; a table's setup may give its own list
  (`TableSetup.add_ons`), and so may a run (`reserves.run`'s `add_ons`). Saving a main dataflow checks it with its add-ons
  put in; one listing a dataflow that isn't an add-on, or itself, is refused.
- **Put in at their stage** (`Dataflow::with_add_ons`): formula and allocation steps before `final`, after the main's own
  there, so after `rom`; table scripts last; fields, words, parameter tables, scripts and lookups after the main's. A field
  or word both have must be the same; a step name both have is refused. The run, Open its column, Every block at a step
  and Run again all put them in the same way (`prepare_flow`).
- **Recorded:** the audit names each add-on with its revision (`Reserves r3 + Waste allocation r2`), and the run reads
  them, so a table says when an add-on has changed since. A notebook cell from an add-on carries its id, so Edit opens
  the add-on.
- **Waste allocation is an add-on** (`waste_allocation_add_on`), made once: Add waste allocation saves its tables and
  the Waste allocation add-on where the project hasn't them (a site's edited ones kept), then the dataflow saved listing
  it. Its quantities on the known-answer column and the demo pit are as before.
- **The Parameters tab:** a main dataflow's **Add-ons**, in order, added, moved and taken out; an add-on says what it is,
  hides the levels, and adds allocations before its table scripts.

| Test | Known answer | Tolerance |
|---|---|---|
| `two_tables_share_one_add_on_and_an_edit_to_it_reaches_both` (reserves_run.rs) | the plan's case: one Waste allocation add-on listed by two tables' setups, Topsoil 2,500 m³ on A's leaf in each; its loose ground edited once (topsoil and freedig taken together): both, run again from their setups, Topsoil 25,000 and Freedig 0, Excavator 25,000 and Dragline 150,000 as before; the audit names the add-on; the main dataflow's revision unchanged | 1e-9 |
| `an_add_on_runs_only_with_a_main_dataflow` (reserves_run.rs) | run on its own refused; a built-in step in an add-on refused; a main listing a main refused; a step name clash refused | exact |
| `waste_allocation_is_an_add_on_that_goes_in_where_it_belongs` (zd-commands) | 39 fields, 5 words, 7 steps; put in between rom and final, the check last; twice and without rom refused | exact |
| `add_waste_allocation_fills_each_process_on_a_waste_interval_of_two_horizons` (reserves_run.rs) | as in 0102, through the add-on; the dataflow lists it and its own steps are untouched | 1e-9 |
| The demo pit (by hand) | every leaf's primes add to its scheduled waste (largest difference 3.6e-11 m³), coal mining to its coal, a process on every waste leaf; 245 ms | 1e-9 relative |

## Built: workstream 3, sources per table

- **A table's own sources** (`TableSetup.sources`, and a run's `sources`): where a field or word comes from, by its name,
  in place of the dataflow's (the block's measure, a block attribute, a parameter column, worked by a step), put in when
  the run is prepared. The dataflow isn't copied or changed; a name the dataflow (with its add-ons) hasn't stops the run,
  saying so.
- **The starter reads the model through fields:** the dragline system reads the interval's measured dip and width
  (`column.total("dip", units=…)`), the coal thickness of the section below from the column and the zone thickness from
  the interval, all as keys of the factors grid (dip and width nearest, coal and zone thickness between). Only settings
  come through `p`. Add waste allocation gives the main dataflow `dip` and `width`, each the block attribute
  `solid.measure` writes, where it hasn't them.
- **Each column chooses its zones:** a fifth table, System methods (a row a zone count: its method and the least system
  thickness it takes); from the settings' `max_zones` down, the first a column's system thickness reaches. The method is
  a word on the units (`method`). The settings row no longer holds a dip, a method or a zone count.

| Test | Known answer | Tolerance |
|---|---|---|
| `each_column_reads_its_own_measured_dip_and_a_table_may_take_a_design_dip` (reserves_run.rs) | the plan's case: B1's 40 m at 5°, B2's 60 m at 15°, a 60 m system of two zones, the site's grid (share 0.4 at dip 0, 0.5 at 20, rising 0.3 by 60 m of zone): B1 0.5, Dragline 100,000 m³, rehandle 40,000; B2 0.65, Dragline 195,000, rehandle 60,000; the table set to a design dip of 25°: B1 0.6, Dragline 120,000, B2 as before, the dataflow untouched; a source for a field the dataflow hasn't stops the run | 1e-9 |
| `each_column_chooses_its_zones_by_its_own_thickness` (reserves_run.rs) | the plan's case: 25 m and 60 m of OB against a 40 m system of up to two zones: B1 one zone, Dragline 60,000, method `1 zone`; B2 two, Dragline 150,000, Poststrip 10,000, Excavator 75,000, `2 zone`, `Zone 2; Zone 1` | 1e-9 |
| `zdp` round trip (zd-io) | a setup's sources saved and read back | exact |
| The demo pit (by hand) | every leaf's primes to its scheduled waste (largest difference 4.5e-11 m³), coal mining to its coal, a process on every waste leaf; 252 ms | 1e-9 relative |

## Built: workstream 4, parameters per table

- **A table's own parameter tables** (`TableSetup.tables`, and a run's `tables`): read in their order, in place of the
  dataflow's, the case's first; its add-ons' tables after them. The dataflow is untouched.
- **One name, one parameter table:** `parameters.save` refuses a second table of a name taken, naming the first, and
  `object.rename` refuses renaming one to it. Add waste allocation keeps a table it finds by its name.
- **A lookup keeps the name its script uses** (`Dataflow.lookup_names`, `CalculatedColumn.lookup_names`), bound beside
  each table's id when saved: a table renamed since is still read under that name, and the audit says it by its new one.
- **Read by** (`read_by`, the Parameters tab under each table's grid, `read_by` in Python): every dataflow, add-on,
  reserves table and calculated column that reads the table, through `p` (its place in their list: `p, table 2 of 2`)
  or by name; each parameter, who names it, and the tables read before it that have it too, read first where their rows
  fit. A parameter is named by a built-in step's setting (as its step's settings call it), a field's parameter column or
  own name, a field's `_default`, `_min` or `_max`, `p.name` and `getattr(p, "name")` in a script, or a lookup's column
  in quotes; one looked up by a name the script works out counts as every column. One nothing names is marked.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_table_reads_its_own_parameter_tables_and_read_by_says_who_reads_each_column` (reserves_run.rs) | the table set to read `Case waste` (waste RD 2.5) before the assumptions (2.3): run again, waste RD 2.5, the dataflow untouched; Read by: the assumptions by the dataflow (p, 1 of 1) and the table (p, 2 of 2), `rd` shadowed by Case waste, a built-in setting read; Case waste by the table alone (p, 1 of 2), its note named by nothing | 1e-12, exact |
| `a_looked_up_table_is_read_after_a_rename_and_names_are_unique` (reserves_run.rs) | the plan's cases: a second `Method factors` refused naming the first, a rename to it refused; renamed after saving, the dragline still 187,500 m³, the audit naming it by its new name; Read by lists the dataflow by name, its factors read | 1e-9, exact |
| `the_parameters_a_script_reads_are_read_off_it` (zd-core) | `p.min_burden`, `u.p.dip`, `getattr(p, "rd_default")` read; a lookup's and a curve's columns; a column named as it runs, every column | exact |

## Built: workstream 5, the Setup tab

- **The Setup tab** in the results dock, beside Tables and Dataflow (the Reserves row's Setup), for the table picked
  there or anywhere, else the latest:
  - **Run on:** the blocks' layers (added, moved, taken out) and the filter (an attribute and its words, as the bench
    rules take them), or the blocks named with Pick by their layers instead; how many it picks now; the seam model or
    the stack; the rows of its own.
  - **Dataflows:** the main dataflow, picked from the project's; its add-ons in order, the dataflow's until one is
    changed, then the table's own, with Use the dataflow's add-ons.
  - **Steps** in the order run, each its kind and, where it isn't the main's, the add-on it comes from; a formula or
    allocation opens in the Parameters tab. Table scripts and calculated columns said under them.
  - **Parameter tables** in the order read, the dataflow's until changed then the table's own (Use the dataflow's
    tables), each opening in the Parameters tab, with the columns this table reads from it, those nothing here names
    (orange) and those an earlier table gives first; its add-ons' tables after, as their add-on lists them; tables looked
    up by name.
  - **Fields:** every field and word field with where the dataflow (or its add-on) takes it from; a source of the
    table's own picked beside it (the solid's volume, plan area or thickness, a block attribute, a parameter column,
    worked by a step), marked blue, and the dataflow's put back by picking it again.
  - **Run again** runs every block from the setup into the table.
- **Every edit is `table.setup`**, the table's next revision, one undo. A table made before setups shows the one its run
  gives and takes it as its own on the first edit.
- **`table.setup` checks what the tab can set:** each add-on an add-on, each table a parameter table, every source a
  field or word field the dataflow and its add-ons have, the add-ons put in as a run would; a refusal leaves the table as
  it was. (A source for a field the dataflow hasn't was refused only when the table ran.)
- **`setup_view`** (`setup_view.rs`, the Tauri `table_setup_view`, `setup_view` in Python): the tab's view, read only.
  Anything that stops it putting together is listed as a problem at the tab's top.

| Test | Known answer | Tolerance |
|---|---|---|
| `the_setup_tab_shows_what_fills_a_table_and_saves_each_edit` (reserves_run.rs) | the starter with Waste allocation added, on two blocks' one layer: 6 blocks picked, dataflow Reserves, add-on Waste allocation (the dataflow's); `dragline_system` an allocation from it between `rom` and `final`, its checks last; the dataflow's tables first, then the add-on's; `dip` the block's measure. Its own tables (`Case waste` first, `rd` read, the assumptions' `rd` then given first by it) and a design dip from a parameter column: one revision, the source marked its own. A main dataflow as an add-on, a dataflow as a parameter table and a source for `dipp` refused, the table as it was | exact |
| `each_column_reads_its_own_measured_dip_and_a_table_may_take_a_design_dip` (reserves_run.rs) | as before; a source for `dipp` now refused by `table.setup`, and a run given it still stops | exact |
| The demo pit (by hand, `setup_view` from Python) | Base and 50m, made before setups: filled from their runs, 1,815 and 3,580 blocks picked from one layer, the assumptions 53 columns read, 19 fields, 15 steps, no problems; 6 and 8 ms | exact |

## Built: workstream 6, the Objects table's formulas and scripts

Tim agreed the four choices on 2026-11-24 ("Agree all four"): the calculated columns kept in a table of their own; an
object script written as a calculated column is; a calculated column taken as a field by name; table scripts reading
`table("Objects")`.

- **The Objects table's own** (`Table.objects`, `objects_own.rs`): one table object per project of no rows, named
  Objects on the `tables` layer, holding only the calculated columns the Objects table shows. It's made by the first
  column put on, and adding, changing or taking one out is its next revision, one undo. It's saved in the `.zdp` with
  its mark, and never listed as a table (`objects_own` on the app's summary).
- **Calculated columns on the Objects table** (0099's `CalculatedColumn` and engine unchanged): `def formula(row, p):`
  on each solid's row, its attributes, `volume` and `tonnes` by `rd`, worked out when shown and never stored.
  `table.calculate` with `objects` puts one on, checked against every solid's row; a name the Objects table has is
  refused, as on a reserves table. The Objects grid works them out over the rows shown (`objects_table_with`), and the
  flat list shows them read-only in italics with ƒ.
- **Taken as a field by name:** a field whose source is the block attribute of a calculated column's name takes its
  value where a block hasn't that attribute (`blocks_with_calculated`), so Imported takes it as it takes an attribute.
  The run reads the Objects table's own and names it in the audit (`Objects r2 (calculated columns)`). Open its column,
  Every block and the trace read the blocks the same way. A table's own source (workstream 3) maps one to any field.
- **Object scripts** (`object.script`): the same formula on each block's row, what it sets becoming the block's
  attributes. New attributes are words where the line gives words (a quoted value, `str(...)`, another word column),
  else numbers. It runs on blocks named, or every solid of some layers that fits a filter in the bench rules' words. It
  never sets name, layer, kind, volume or tonnes. Each block it changes is revised once, in one undo, and a reserves
  table made before says its blocks have changed. A block it stops on stops the run, said with its line, and nothing is
  written.
- **Table scripts and the Objects table:** `table("Objects")` in a table script or a dataflow's table step reads every
  solid's row with the calculated columns worked out as numbers (only for scripts that name it). `table.script` with
  `objects` runs on it, giving a table of its own; columns onto the Objects table are refused (an object script sets
  attributes).
- **The app:** the Tables tab on Objects has **+ Calculated** (the reserves tables' editor, run on the picked solid line
  by line) and **Run a script on blocks…**, run on the solids shown. Each calculated column that stops on some solids
  says how many and the first. A project script that defines `run` can run on Objects.
- **The reference** handed to an AI gains `table("Objects")` and a section on object scripts.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_calculated_column_on_the_objects_table_is_worked_when_shown_and_taken_as_a_field` (reserves_run.rs) | the plan's case: `coal_tonnes`, volume by RD: A's 100 × 50 × 2 m at 1.4 is 14,000 t, the OB none (4 solids without RD said); the grid shows it; `ash_twice` mapped to `ash` by the table's own source: each coal leaf's Imported ash 20 (its 10 doubled), nothing stored on the blocks, the audit naming `Objects r2`; `volume` as a name refused; undone, the Objects table's own goes | 1e-9, 1e-12 |
| `an_object_script_sets_attributes_on_blocks_in_one_undo` (reserves_run.rs) | the plan's case on four blocks by layer and filter: `process` from the horizon (words), `discard` from ash (0 at ash 10); each revised once; the earlier reserves table says its 4 blocks changed; one undo; `volume` set, and a division by 0, refused with nothing written; `table("Objects")` grouped by block gives the coal's volume; `table.script` on Objects gives B1 and B2; columns onto it refused | exact, 1e-6 m³ |
| `the_names_a_script_sets_are_read_off_it_words_where_its_line_gives_words` (object_script.rs) | `case` and `material` (from a word column) and `tag` (`str`) words, `discard` a number; a comparison passed over | exact |
| `zdp` round trip (zd-io) | the Objects table's own comes back marked, with its column | exact |
| The demo pit (by hand, from Python, 5,767 solids) | a calculated column put on 31 ms; an object script setting `process` on every solid 76 ms (5,767 changed); a table script on Objects 110 ms; Run again reading the calculated columns 129 ms | — |

## Known limits

- Read by can't see a parameter a script names as it runs through `p` (`getattr(p, name)` with a worked name): the
  starter's `_default`, `_min` and `_max` are counted for every field for that reason.
- A project saved before names were unique may hold two parameter tables of one name; a lookup binds the first, as it did.
- The starter's default factors are one set a zone, the same at every dip, width and thickness: a site fills its grid.
- An add-on's unsaved edits aren't run live in the notebook (the live run takes the main dataflow as edited): save it
  to see them.
- A dataflow made by Add waste allocation before add-ons (0102, workstream 5) keeps its steps in it; it runs as it did.
- The Setup tab hasn't an editor for the table's own parameter rows (the case's): they're shown as a count, set from
  the run or `table.setup`.
- The Setup tab and the Objects table's formulas are checked by their tests and from Python on the demo pit; not yet
  tried in the app by hand.
- A calculated column on the Objects table gives numbers only: a class in words is set by an object script. In a
  table script's `table("Objects")` it has no rule to add up by, as the blocks' other numbers haven't.
- An object script's new attribute is words or a number by the line that sets it; a line that gives words some other
  way (a function's result) makes a number, refused when it runs. Set an existing word attribute, or use `str(...)`.
- A calculated column can't read another, on the Objects table as on a reserves table.
- A setup filled from an older run keeps that run's own rows and model; a table made outside a reserves run (a table
  script's) has none.
