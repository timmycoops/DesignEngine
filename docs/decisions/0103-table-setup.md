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

## Known limits

- The setup holds the dataflow's own parameter tables (the table's own list is workstream 4) and no field overrides yet
  (workstream 3).
- An add-on's unsaved edits aren't run live in the notebook (the live run takes the main dataflow as edited): save it
  to see them.
- A dataflow made by Add waste allocation before add-ons (0102, workstream 5) keeps its steps in it; it runs as it did.
- A setup is seen and changed from Python or `table.setup` for now; the Setup tab is workstream 5.
- A setup filled from an older run keeps that run's own rows and model; a table made outside a reserves run (a table
  script's) has none.
