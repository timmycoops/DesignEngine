---
paths:
  - "crates/zd-reserves/**"
  - "crates/zd-script/**"
  - "crates/zd-geom/src/reserves.rs"
  - "crates/zd-core/src/{dataflow,parameters,table,leaves,workflow}.rs"
  - "crates/zd-commands/src/builtin/{reserves_run,dataflow,formulas,starter,interrogate,compare,measure,workflow}.rs"
  - "crates/zd-commands/src/{reserves_view,table_grid,table_calc,workflow,workflow_run,report,report_html}.rs"
  - "app/src/panels/{Reserves,Dataflow,Parameters,Formula,ColumnSteps,StepPane,BlockTable,Tables,TableGrid,Calculated,UpdateRecords,Workflow}*"
  - "app/src/panels/{reserves,dataflow,parameters,formula,columnSteps,notebook,blockTable,tableGrid,workflow}*"
---

# Reserves: parameter tables, dataflows, tables, formulas

Moved from CLAUDE.md's object model and reserves conventions. The short form of the conventions stays in CLAUDE.md.
Decisions: 0065 to 0075 and 0077 (workflows, reserves dataflow, working sections, ROM, the reserves table, formulas), 0082
(live workings), 0084 to 0086 (dataflow view, run again, fields), 0090 (filter in the view), 0099 (tables and scripts).

- `ParameterTable`: assumptions any tool reads (reserves settings now; dragline, haulage, equipment later): filter
  columns (the bench rules' syntax) and value columns with units, read top down, each value from the first row that fits
  and holds it, tables read in order so a case's rows go first; CSV in and out; edited as grids, with the dataflow, in the Parameters tab, a picked solid's
  values traced to their row (`docs/decisions/0075-reserves-in-the-app.md`). `Dataflow`: a reserves dataflow's fields
  (with where each value comes from), steps by name and the parameter tables it reads; a step may be a formula step, Starlark run on the units its filter fits after `initialise`, `join_benches`, `working_sections`, `rom_dilution` or `rom`, checked and its known answers run when saved, edited in the Parameters tab's formula panel and run there on a picked block line by line (`docs/decisions/0077-formulas-in-starlark.md`); a block's column opened
  step by step gives each step's units, the settings it read, its workings and what it wrote to the reserves table, a
  folder each, for the Dataflow tab's strip, step and Before and After (`docs/decisions/0084-dataflow-view.md`), run live on the
  Parameters tab's unsaved edits, what they moved marked against the saved run (`docs/decisions/0082-live-workings.md`); Run
  again there runs a block, its strip or every block into the table in place, `reserves.run` with `table` and `only`
  (`docs/decisions/0085-run-a-table-again.md`). `Table`: results, a table a case,
  values as Parquet in the `.zdp`, read by the HTML report and the app's Reserves table tab, fields as rows and a
  node's children across, its solids matched by the levels the table records (`docs/decisions/0075-reserves-in-the-app.md`), its leaves filter carried to the view by a tick, coal solids only if asked (`docs/decisions/0090-reserves-filter-in-the-view.md`). `reserves.run` runs a dataflow a column a pit, strip
  and block (the dataflow's levels above the seam), its steps registered by name, and writes a reserves table: a row a
  leaf of the levels, a column a field under its folders (`Insitu\Coal\volume`), coal and waste on one leaf as Spry's
  node (`docs/decisions/0072-dataflow-parameters-tables.md`, `0074-rom-and-the-reserves-table.md`); clean coal under ROM,
  and a Summary: the balance of coal and waste from Imported through Insitu and ROM to Scheduled with what moved between
  them, each check zero for every block, ratios as ratios of sums (`AddUp::Ratio`) and an audit
  (`docs/decisions/0086-reserves-fields.md`).
- One table concept (`docs/decisions/0099-tables-and-scripts.md`): the blocks (a row a solid, `table_grid::objects_table`)
  and a reserves table (a row a leaf) are both a `Table`, seen in the Tables tab's grid, `table_grid::grid`: grouped by
  any word column, every subtotal `add_up_column` over the group's leaves, as the Breakdown and the report add up. The
  Dataflow tab's Every block is a step's units for every column as such a table (`units_at_step`, run on demand from
  what's saved, never kept). A column is written by a run or entered by a person (`ColumnKind`, `made` in JSON):
  `table.enter` and `table.remove_column` touch entered columns only, Run again keeps them by each leaf's key and
  says any whose leaf is gone, and `object.set_attributes` types blocks' attributes; the reserves run behind a table
  is found by `table_run`, as its latest revision may be an entry. `table.update_records` takes a CSV matched on key
  columns into objects' attributes or a table's entered columns, its report (`match_records`) shown first. A table's
  calculated columns (`CalculatedColumn`, `table_calc.rs`) are dataflow formulas on a row, `def formula(row, p):`,
  kept as scripts and worked out by `grid_with`, a group from its inputs or its rows by a rule; `table.calculate` puts
  one on. Its
  workstreams 2 to 9 (entered and calculated columns, update records, table and object scripts, a dataflow from any
  table) build on it.

## Reserves conventions

- JORC Coal Guidelines 2014: tonnes on an in situ moisture and in situ density basis (store air-dried RD,
  convert with Preston–Sanders at reporting). RD is composited by thickness, raw qualities by mass
  (thickness × RD), clean-coal qualities by mass × yield.
- Mining factors in this order: roof and floor loss and gain, then recovery. Seam rules in Spry's order (Tim,
  15 November 2026, so PIT B's working sections can be matched): in situ quality limits per ply first (coal set aside
  by its in situ ash), then minimum separable parting, minimum product-to-waste ratio as a section builds, and minimum
  mining thickness (partings in) last; the ROM ash cut-off after dilution (`docs/decisions/0073-working-sections.md`).
  Loss and dilution material carries its own density and quality. Edge loss is supported.
- Overburden and interburden attach to the next seam below, with explicit rules for masked or missing seams.
- Output one record per pit / strip / block / bench / seam / material with volume, tonnes, qualities,
  areas, dip and centroid. Block names are unique. Coal is reported in tonnes, waste in BCM.

