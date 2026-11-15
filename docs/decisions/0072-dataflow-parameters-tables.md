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

## Built: 2b, the runner and the report

- **`reserves.run`** (`crates/zd-commands/src/builtin/reserves_run.rs`): a dataflow, the block solids, and the seam
  model or the horizons top down; an output layer; and the step's own parameter rows, read before the dataflow's
  tables, so a case changes a setting there. Blocks are grouped into columns by case (`case`, Base where none), strip
  and block. Each block is a unit: its stack place from its `horizon`, its labels, its bench (`bench`, `bench_floor`)
  and a value a field. Columns run across the cores. A block that can't run (a horizon not in the stack, no material, a
  bench with no floor) leaves its column out, said (`reserves.block_stopped`), and the rest go on. Rows written with a
  missing value are counted (`reserves.value_missing`): a setting no row gives is missing until a step needs it, and
  that step stops the block.
- **The steps**, named and registered, in Spry's order:
  - `initialise` puts the stack order in place, benches top down.
  - `in_situ` writes each horizon's benches as they are, before anything is joined, as Spry writes in situ by bench.
  - `join_benches` joins each horizon's benches with their in situ shares, as Spry's working sections start.
  A name this build doesn't have is refused, with the list it has.
- **Results**: one `Table` a case, named `{dataflow} {case}`. Its columns:
  - case, strip, block, write point, horizon, seam, material, bench, became, units and the blocks a row came from;
  - a column a field, with its unit and rule;
  - tonnes, volume × the density field on its own basis (air dried until the in situ step of 0070's workstream 3).
  Thickness and area are written on each row but carry no rule, as neither adds up across blocks.
- **The report**: a Reserves section a case, after By bench. Each results table is shown by write point: by horizon and
  material top down, then coal and waste, then by bench with each bench's coal and waste apart. Each column is added up
  by its own rule (summed, by volume, by tonnes, the largest or the smallest), so a field a dataflow adds shows as a
  column of its own.
- **Preview**: `reserves_preview` (and Python's `Session.reserves_preview`) gives one block's column as input and after
  each step, with each unit's values and, once joined, its benches and shares.

Known answers (`crates/zd-commands/tests/reserves_run.rs`, `report.rs`, to 1e-9):

- Two 100 × 50 m blocks:
  - B1's four horizons are written top down. U is 20,000 m³, 4 m and 5,000 m², at RD 1.4, so 28,000 t.
  - B2's U benches are written as they are: 10,000 m³ each at RD 1.4 and 1.6, so 14,000 t and 16,000 t. Bench 18 comes
    before bench 16.
  - Waste RD comes from a parameter row: OB is 115,000 t, and waste ash is missing on four rows, said once.
  - Everything adds to the 180,000 m³ of blocks.
- A step's own row gives U's cut-off as 30 and the dataflow's table gives L's as 35. Base and 50m get a table each.
- A block with a horizon the stack doesn't have is left out with why, and the other block runs. A step this build lacks,
  or no stack order, is refused.
- The preview shows sorted, written and joined: U's benches as one unit with shares 0.5 and 0.5, at RD 1.5.
- The report adds a results table by each column's rule: U 20,000 m³ at RD 1.5 by volume and ash 332,000 / 30,000 by
  tonnes; a column of nothing is nothing, not zero.

The demo pit (`testdata/synthetic/workflow_demo.py`, step 21) runs `reserves.run` as a workflow step in both cases.
- Base writes 1,815 in situ rows from 1,815 blocks and 50m writes 3,580 from 3,580.
- Both cases come to 76,003,157.442 m³ and 169,304,790.747 t, matching their blocks' volumes and tonnes to 0 m³ and
  0 t.
- Every column balances to 3.3e-16 of its volume.
- `reserves.run` alone on Base's 90 block columns takes 8 ms.
