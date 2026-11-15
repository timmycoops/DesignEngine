# 0072 — The dataflow, parameter tables and results tables

Status: agreed 15 November 2026 (Tim: "Yep looks good", the Build Plan's "Dataflow plan" tab, all eight decisions as
recommended, with three notes folded in as decisions 3, 4 and 9). Workstream 2 of the reserves dataflow (0070), built as
two PRs: the objects (2a, here), then the runner and the report (2b).

## Why

The column engine (0070, workstream 1) works one block column at a time, but nothing turned a project's blocks into
columns, held the settings a run reads, or kept what a run makes. Tim's notes on the plan: build it to take more as we
go; parameter tables matter in their own right, since "lots of mine planning assumptions will generally be parameter
table type data (dragline, haulage parameters, equipment)", for flexibility and optionality; and the outcomes should
give metrics in the HTML report.

## Decision

1. **A dataflow is an object** (`Dataflow`): its fields (name, kind, how each adds up, and where its value comes from),
   its steps by name in Spry's order, each on or off, and the parameter tables it reads, in order. JSON in its object's
   row of the `.zdp`; an edit is a new revision, one undo. Settings live in parameter tables, not Arrow (0070's decision
   10 had Arrow for settings; Arrow stays for results, where the size is).
2. **Where a field's value comes from**: the solid's volume; its plan area (its footprint, from its upward-facing
   triangles); thickness, volume over plan area; a block attribute (an interrogated RD or quality), falling back to the
   parameter tables' column of the field's name; a parameter column; or worked out by a step.
3. **Parameter tables are objects of their own** (`ParameterTable`), for any tool. Filter columns (any attribute, each
   cell a comma list of words, numbers or ranges, the bench rules' syntax) and value columns, each with its unit. Read top
   down: each value comes from the first row that fits and holds it, so a row can change one value for one seam; the last
   row, filters blank, is the default. Tables are read in order, so a case's or a step's own rows go first. CSV in and
   out: a filter column's header is `when <attribute>`, a value column's `<name> (<unit>)`. One matcher for these and
   the bench rules (`zd_core::filter_fits`).
4. **Results are `Table` objects**, one a case a run, on the run's layer: columns of numbers or words with their units,
   an empty cell a value that isn't there. Their values are a Parquet entry of their own in the `.zdp`
   (`tables/<id>-r<rev>.parquet`), their columns' names, units and kinds the object's body. The workflow's HTML report
   reads them (2b), so a metric a step adds reaches the report as a column.
5. **`reserves.run` is a command** (2b), so it's a workflow step; a step's own parameter rows are read first, so a case
   holds only the setting it changes.
6. **The runner** (2b): a column a strip and block, blocks across the cores, a block that can't run left out with why.
7. **Preview** (2b): one block's column after every step, for the canvas.
8. **What ships first** (2b): the structural steps only (sort, join benches, the in situ write by bench, the put-back).
9. **Built to take more**: steps are named, fields are data, parameter tables carry any columns, results and the report
   follow the fields.

## Built: 2a, the objects

- `zd_core::ParameterTable` (`parameters.rs`): `check`, `lookup`, `lookup_in` (tables in order), `to_csv`, `from_csv`
  (quoted cells, a column of any word holds words, units in brackets).
- `zd_core::Dataflow` (`dataflow.rs`): fields as the column engine's `Field` plus a `FieldSource`, steps, parameter table
  ids; `check` runs the engine's field checks and refuses a step listed twice. zd-core now depends on zd-reserves, which
  depends only on serde.
- `zd_core::Table` (`table.rs`): columns of `Option<f64>` or `Option<String>` with units; `check`, `headers`.
- Filters moved from the bench rules into `zd_core::filters`, unchanged.
- Object kinds `parameter_table`, `dataflow` and `table` in the `.zdp` (the manifest lists a `tables` table, version 1,
  so an older build refuses a file holding one by name), the older folder store, Python's `object()` (a table's CSV; a
  results table's columns by name) and the app (labels, icons, Properties' measures).
- Commands: `parameters.save` (rows, CSV text or a CSV file; new or the next revision), `parameters.export_csv`,
  `dataflow.save` (warns of a parameter field none of its tables has a column for, `dataflow.no_column`).

Known answers: a value from the first row that fits and holds it (a row with no waste RD passes to the next); a case's
table read first; CSV out reads back the same; tables that don't hold together refused by name; a results table with a
NaN, minus zero, nulls and empty words, and one with no columns, back through the `.zdp` bit for bit; a parameter table
revised and undone; a dataflow's missing column warned of; a solid named as a parameter table refused.
