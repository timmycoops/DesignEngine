# 0105 — Reserves case plan: a case runs several dataflows in order on one table, on one Reserves case screen. All 14 decisions agreed by Tim 2026-11-25.

Date: 2026-11-25 · Status: accepted (Tim, 2026-11-25: "Agree all fourteen", on the Build Plan's "Reserves case plan" tab
as rewritten after his Reserves Flow prototype)

## Why

Tim, 24 November, after the Setup tab (0103 workstream 5): it "doesn't make that much sense initially", "how does it
align to which table", "it just feels confusing at the moment". One case was spread over Tables, Parameters, Dataflow,
Setup, Workflow and the Run log, in engine words, with no way in from the app. His brief, five needs: clear which
dataflow is open; clear what its inputs are; simple to open how it works; step through each piece to see the numbers
change and spot-check a block; the parameters beside it, to change them and rerun a step. Not like Spry.

Agreed in principle on 24 November: a case holds several dataflows run in order on the same table, each with its own
parameter tables and columns, a later one reading an earlier one's results; run one or all; an upstream change marks
later ones out of date; add-ons stay for work inside a run; a dataflow may run over a range only ("you could have a data
flow run over a range only also"). On 25 November Tim picked concept 1 (step through it like a strip log) and built a
prototype, Reserves Flow (`docs/briefs/2026-11-25-reserves-flow-ideas.md`): the material river, the steps by stage,
parameters by seam and ply, a units table with groups and field sets. The plan keeps concept 1's band and dataflow
switcher and puts Reserves Flow's view under them.

## Decision

1. **A case is a reserves table and its setup** (0103 decision 1): no new object. It's 0072's case: Base, Low price and
   Tight loss are each a table. Copy case makes a new table.
2. **A case holds a list of dataflows, run in order.** The setup's `dataflow` is the first, the reserves dataflow: it
   makes the rows, a leaf a row, its add-ons inside it. The rest are **row dataflows**, listed in `row_dataflows`: steps
   on the table's rows, a formula on each (`def formula(row, p):`) or a table script, reading the row's words and numbers,
   its settings and the columns of the dataflows above it. A setup saved before has none: a list of one, the same table.
3. **Each dataflow has its own settings tables**, read in order, the case's rows first; none given, its own list.
4. **Each dataflow has its own range**, a filter in the filter syntax (0088) on the table's columns, e.g. `strip 1-6`.
   A row outside it has the dataflow's columns empty, never 0; its totals add up only the rows it ran.
5. **A column has one writer.** A row dataflow writes only its own fields' columns, under its own folder
   (`Haulage\distance`) unless the field is named with one. Two writing one column, one writing a column the reserves
   run or a person wrote, and one reading a column of a dataflow below it are refused, each named.
6. **Run one, what's out of date, or all; never by itself.** Each run records what it read; out of date is said with
   why; running one never runs those below (rule 4). Each run is the table's next revision, one undo.
7. **Add-ons stay inside a run** (Waste allocation splits ROM units); a row dataflow can't split units.
8. **One Reserves case tab**, laid out as the merged screen: concept 1's band (with the balance check, no ground made or
   lost) and switcher, then the inputs, the flow carrying the case's quantities, steps by stage | the step | the block
   map and strip log, and the units table. It replaces the Setup tab (reverses 0103 decision 5) and the Dataflow tab
   (0084). Parameters and Tables stay.
9. **A change reruns the case from the first step that reads it**, the steps before reused, as you type, if a rerun
   from a step on PIT B takes under a second and the kept columns fit; else the block as you type and the case on Run
   to this step. Workstream 3 measures it.
10. **How it works is the flow**, the steps by stage and the field map, in the order the run puts them; nothing laid
    out by hand.
11. **Settings by seam and ply are a view of the rows as read** (0072's lookup unchanged): rows in reading order with
    what each fits, blanks passing on, the cell that supplied each value for the picked unit outlined, the audit naming
    the row.
12. **The units table**, on the Tables grid (0099): a row a piece of ground; groups you change with subtotals before and
    after; a piece that leaves a group stays listed in it, in red; field sets including your own, saved in the project;
    changed only; the picked piece with every field and its journey; the field map.
13. **New case, Copy case and Add a dataflow in the app**, each case's headline figures against Base.
14. **Python and workflows** use the same case: `table.setup` takes the list; a run names the dataflow it runs, or runs
    them all in order.

Not in this plan: sort order and named hierarchies (a plan of their own, Tim's three questions open), a margin ledger,
a company library of field sets, pieces as solids, the scheduler's tables, running by itself, writing Spry.

## Built: workstream 1, several dataflows a case

- **The object model** (`zd-core`): `TableSetup.row_dataflows`, a list of `RowDataflow { dataflow, tables, range }`,
  written only where there are any, so a setup saved before reads and writes as it did. `Dataflow.rows` marks a row
  dataflow and `Dataflow::check_rows` holds it together: fields and words named once, none the unit's own volume or
  thickness, none read from a block (a row dataflow's fields are worked by its steps); steps named once, each a formula
  or a table script, the table scripts last, none built in, none an allocation, none with settings; no add-ons; at least
  one step. `Dataflow.units` gives a field's unit by name where its kind gives none (`distance` in km), for any dataflow.
  `Dataflow::row_columns` and `row_column` name the columns (`Haulage\distance`; `Costs\haul` as named).
- **`table.run_dataflow`** (`builtin/row_dataflows.rs`): one row dataflow of a case run on its table in place, the
  table's next revision. Its columns are made again empty, then its steps run in order on the rows its range fits: a
  formula on those its `when` fits too, compiled against the table's columns (full names, `row["Insitu\\Coal\\tonnes"]`)
  and its own fields (short names, `row.distance`), each row's parameters the first row of its tables that fits the
  row's words and numbers; a table script on the whole table, only its own columns taken, on its range. A formula
  setting a column not its own is refused; so is a table script changing one or giving one back it doesn't list. A row
  a formula stops on (`fail("…")`, a division by nothing) keeps its columns empty, said once with the first row and line
  (`table.rows_stopped`); the rest are worked, a batch of 256 at a time as calculated columns are. The run records the
  table, the dataflow, its tables, scripts and lookups; the case's other dataflows are read to check the case, not
  recorded, so a change to one below isn't this run's input.
- **The checks** (`row_flows`, `check_case`), at `table.setup` and at each run: every listed dataflow a row dataflow,
  listed once, not the reserves dataflow again; no column with two writers, named with both; no row dataflow writing a
  column the reserves run wrote, a person entered or the table works out (only columns the case's row dataflows as
  saved write may be there already); none reading a column only a dataflow below it writes, found by the column's name
  quoted in its scripts' source, as written or with its backslashes doubled. `reserves.run` refuses a row dataflow.
- **Run again keeps them** (decision 6): a reserves Run again keeps the case's row dataflows' columns by each leaf's key,
  as it keeps entered values (`merge_rerun`'s `by_key`), until those dataflows run again; it never runs them. The setup
  is kept, list and all.
- **`dataflow.save`** saves a row dataflow as it saves an add-on: its scripts checked on their own and their loads and
  lookups bound; checked against a table's columns only when it runs, as the table isn't known until then.
- **Python:** `session.run_case(table, dataflow=None, only=None)`: with `dataflow` that one alone, the reserves dataflow
  run again from the setup (with `only` a block or strip) or a row dataflow on the rows; without, all in order, the
  reserves dataflow first. A list of the runs' records, one a dataflow. `run_table` still runs the reserves dataflow
  alone. `table.setup` takes the list as the setup's JSON does.
- **The app** carries the list: its `TableSetup` and `Dataflow` types have the new fields, and the Setup tab saves the
  setup it was given, list and all. Nothing shows it yet (workstreams 4 to 7).
- **Tests** (`crates/zd-commands/tests/case_dataflows.rs`, a made-up pit of strips 1 to 8, a block each, 22,500 t of
  coal): a setup saved before reads as a list of one and Run again gives the same table, every column but the audit's
  time (exact); Haulage on strips 1 to 6, 2 km on 1 to 3 and 3 km on 4 to 6 from its own table, 45,000 and 67,500 t.km a
  block, 337,500 in all, strips 7 and 8 empty, said as 6 of 8 rows (1e-9); two writing one column, a read from below, a
  field named as the reserves run's column, the reserves dataflow listed again and a main dataflow listed, each refused
  and named (exact); a formula setting another's column refused, and one stopping on strips 7 and 8 leaving them empty,
  said once with the first and its line (exact); Run again keeping Haulage's values by leaf and not running it (exact);
  a table script's coal share, 1/8 a block, on strips 1 to 4 only, and one giving back a column it doesn't list refused
  (exact). `zd-core`: a row dataflow saved and read back, its columns named, and seven ways it's refused. Python
  (`test_session.py`): two strips, Haulage on strip 1, `run_case` runs both in order, 45,000 t.km on strip 1, none on
  strip 2.

## Built: workstream 2, out of date

- **`case_state(project, log, table)`** (`builtin/case_state.rs`): each of a case's dataflows in run order, the reserves
  dataflow first, with its range, its last run and when, whether it's up to date and each reason it isn't. Read from
  the run log each time, never kept; a run undone (and not redone) doesn't count. A dataflow is out of date where:
  - it hasn't run on this table;
  - something its last run read has changed or gone since: for the reserves dataflow, blocks revised (counted) and its
    dataflow, add-ons, seam model and settings tables (named), as `changed_since` (0099) reads them; for a row dataflow,
    its dataflow, settings tables, scripts and lookups (named);
  - for the reserves dataflow, blocks added to what the setup picks since it ran (counted), or its setup changed (its
    dataflow, seam model or stack, own rows, add-ons, sources, tables or input);
  - for a row dataflow, its range or settings tables in the setup changed since it ran, read against the setup the last
    `table.setup` before its run saved;
  - a dataflow above it ran after it ("Reserves ran after it"), or is out of date itself ("Reserves is out of date").
- **Run what's out of date**: Python's `run_case(table, out_of_date=True)` runs those out of date in order, each
  judged after the one before it ran, so a row dataflow below a reserves dataflow just run is run too. `case_state(table)`
  gives the state as a dict. Nothing runs by itself.
- **Tests** (`case_dataflows.rs`, the made-up pit): through a case's life, the exact words each time: Haulage never
  run; both up to date; Haulage's distances revised, Haulage alone out of date ("haul distances changed"); the reserves
  run again after it ("Reserves ran after it"); a block's ash changed ("1 block changed", Haulage "Reserves is out of
  date"), and undone, both up to date again; Haulage's range changed in the setup ("its range changed since it ran"),
  run, and that run undone, out of date again; a block added to the layer ("1 block added to what the setup picks").
  Python: nothing to run when up to date; a revised table runs Haulage alone; the reserves run again runs Haulage after.

## Built: workstream 3, the case at a step

- **`CaseAtStep`** (`builtin/reserves_run/at_step.rs`): a reserves table's case opened at a step, every block column of
  its case run from its blocks with the settings of the run that made the table, on what's saved or with edits not yet
  saved (`Unsaved`: parameter tables, the dataflow). It keeps each column at the step and as it stood before it, as
  decision 9 asks; each step runs over every column across the cores and is timed. `go_to` moves on a step at a time,
  or back by running again from the blocks.
- **Totals at the step** (decision 8): the units as they stand, by material and what became of them (`coal`, `coal,
  loss (roof)`); what the steps so far have written to the table, by folder (`Loss\Roof`, `ROM\Coal`), each field added up
  by its own rule and its tonnes worked leaf by leaf as the table works them (ROM and Scheduled by the ROM density, the
  rest by the in situ density, else the RD); what the step said over the case (`roof loss`, 2 units, 1,000 m³); and the
  balance, no ground made or lost: the volume in from the blocks against what the columns hold now less what came from
  outside the model, by `reserves.unbalanced`'s tolerance, the columns off named.
- **Run again from the first step that reads an edit** (decision 9): what each built-in step looked up is noted as it
  runs (`ColumnRun.asked`), a script step's reads are read off its source (`p.name`, `getattr(p, "name")`, a name built
  as it runs taken as a field's own or its `_default`, `_min` or `_max`, `dir(p)` as which settings a unit has, `p`
  handed on as every setting), and an edit is compared column by column (a row's filters or the rows changed: every
  column; a cell filled or cleared: which settings a unit has too). An edit the open step reads first runs that step
  again from the columns kept before it; one read earlier, or a change to the blocks, the stack, the levels, the fields
  or the run's setup, runs the case again from its blocks; one no step up to here reads runs nothing.
- **`table`**: the table the case would make from the step, its columns run on to the last step and written as a
  reserves run writes a new table, table scripts and all. Nothing is kept or saved.
- **Measured, decision 9's switch.** The demo pit (90 columns, 1,647 units at `rom_loss`): opened at `rom_loss` in 50 ms;
  coal roof loss 0.1 to 0.15 m reruns `rom_loss` alone, 12 to 19 ms over five edits, roof loss 166,169 to 249,253 m³;
  on to `final` 35 to 45 ms, ROM coal 9,106,368.7 t, the plan's 9,106,369. PIT B (622 blocks, the starter and the
  demo's made-up settings: 47 columns ran, 28 stopped, 274 units): opened in 41 ms, a rerun from `rom_loss` 7 ms median,
  8 ms worst; the kept columns about 26 MB beside the project's 354 MB. Both well under a second and the memory small,
  so **the whole case reruns as you type** (decision 9's pick), not the block alone. Script:
  `testdata/synthetic/case_at_step.py` (`--counts-only` on client data).
- **PIT B needs the held unjoined-benches fix.** On `main`, the starter's `working_sections` stops PIT B's run with a
  panic ("plies and partings alternate") where two plies meet with no parting between them. The fix held uncommitted on
  `reserves/allocate-by-bench` (an empty parting between them) was laid over the build for the measurement only and
  taken off again; nothing of it is in this workstream.
- **Python:** `session.step_case(table, step, tables=None)` opens or moves the case kept for the table (a step by name
  or number) with `tables` as edited (`{name or id: CSV}`), each call running again from the first step a change
  reaches, and gives the totals with what ran and from where; `step_units(table)` the units there as a table;
  `step_table(table)` the table it would make. `units_at_step` builds its table with the same code.
- **Tests** (`crates/zd-commands/tests/case_at_step.rs`, one block of 5,000 m² with two coal sections, the starter's
  formulas in): at `rom_loss` roof loss 1,000 m³, floor 2,000, edge 25, other 500, 21,475 m³ of coal left, written
  `Loss\Roof` 1,000 m³, balanced, 13 units (1e-9); roof loss 0.15 m runs `rom_loss` alone, 1,500 m³ and 20,975 m³ of
  coal left; a dilution setting runs nothing; a parting rule runs from the blocks; the dataflow's `rom_loss` told to read
  `floor_loss` runs `rom_loss` alone, 2,000 m³; a settings column added runs from the blocks (the starter's `dir(p)`); the
  table from `rom_loss` at 0.15 m is the table a full run makes once 0.15 m is saved, every number column (exact), and
  the totals at `final` are its columns added up, ROM coal tonnes too (1e-9); balanced at every step; a step back runs
  from the blocks; a step past the last refused. Python (`test_session.py`): two strips at `small_volumes`, the minimum
  edited to 20,000 m³ runs that step alone and sets both coal blocks aside, 30,000 m³; on to `final` runs the two steps
  after it; the table from there has the table's rows; nothing saved.

## Built: workstream 4, the step view

- **The Reserves case tab** (`app/src/panels/ReservesCasePanel.tsx`, a tab in the results dock, a tool on the Reserves
  row), a reserves table's case a step at a time, opened at `rom_loss` where the dataflow has it:
  - **The band**: the dataflow open and its add-ons, the case (a table picked in it, following the one picked in the
    project or opened in the Dataflow tab), and the balance at the step, green while no ground is made or lost, red with
    the columns off; its steps and block columns, and how many stopped.
  - **The inputs**: the block columns, the seam order and each settings table as a card, those the step reads lit gold,
    one run as edited blue.
  - **The steps by stage** (decision 10, `stagesOf`): a built-in step in its own stage; a formula in the next built-in
    step's (the discard formula with working sections), after `rom` in ROM; an add-on's steps a stage named for it.
    ↑ and ↓ step through them; after an edit the steps that ran again show their time.
  - **The step**: its rule on one line (whole on hover) or its script opened in the formula panel; the fields and
    settings it reads (settings gold) and the folders it writes; the whole case at the step against the last run (what
    the step said it moved, then in situ coal and ROM coal in tonnes and ROM waste in BCM once written, with the change
    and its share); then its settings by seam and ply (decision 11, `settingsGrid`): every row of each table that holds
    any of them, in the order read, what each fits, a blank passing on, the cell that gave the followed unit its value
    ringed gold. A cell typed is an edit, cleared passes on; Save and run the case saves the tables typed in and runs
    the case again from its setup; Discard puts them back.
  - **The block map and strip log** (decision 8): a row a strip, a square a block shaded by the volume the step moved
    there (else wrote), a stopped block red, the picked block ringed; under it that block's units top down before and
    after the step, as tall as each is thick, coal dark, waste brown, coal lost red, rock let in orange, what changed
    named; a click follows a unit.
- **One store of edits.** The tab's cells are the Parameters tab's unsaved edits (`drafts`, decision 0082): typed in one,
  they show in the other (`withTable` lays them on the Parameters grid as edited), and a table saved anew is read again.
- **Kept in the app shell** (`reserves_case_step`): the case as saved and, while there are edits, as edited, each a
  `CaseAtStep`, run again on each ask from the first step a change reaches; the first edit starts from a copy of the case
  as saved (`CaseAtStep::edited`), so it too runs only the step it reaches. `CaseAtStep::columns` gives each block
  column's moved and written volume and its coal before and after, for the map.
- **Tested.** App: the stages, the map's rows and shades, the whole case's rows and changes, a step's settings read off
  a built-in step's reads or a script's source, the settings grid in reading order, a cell typed, cleared and added
  (`tests/reservesCase.test.mjs`); an edit from the case tab laid on the Parameters grid as edited
  (`tests/parametersGrid.test.mjs`). Engine: what `rom_loss` did to the made-up block (3,525 m³ moved and written, its coal
  25,000 to 21,475 m³), and the first edit from a copy running `rom_loss` alone with the saved case left as it was
  (`case_at_step.rs`). In the app (`zd-app --bench --case <demo.zdp> --pictures <folder>`, `bench/caseCheck.ts`): the
  demo's Base table opens at `rom_loss` in about half a second, balanced, steps by stage, 90 blocks on the map, its
  settings table lit; the coal roof loss typed 0.1 to 0.15 m reruns `rom_loss` alone and shows roof loss 166,169 to
  249,253 m³ (+50.0 %) within 0.7 s of the last key (a debug build, the 330 ms wait included); discarded, as saved; at
  `final` ROM coal 9,234,489 t.

## Built: workstream 5, the units table

- **The units table** (`app/src/panels/UnitsTable.tsx`, its working parts in `caseUnits.ts`), under the step view in
  the Reserves case tab: the picked block's units before and after the step, a row a piece of ground (decision 12).
  - **Pieces.** A unit that kept its id is one row, before and after. A piece the step split off is a row under the unit
    it came from, named so. So is a unit joined from benches. A unit taken away or joined into another is a row with no
    after. A piece finds its unit by its ground, the same horizon and bench (a joined unit's pieces keep its joins), or a
    joined unit by one of its benches.
  - **Groups.** By working section (where a unit goes in ROM once a step sends it there; the default), seam, horizon,
    material, bench, any word field, or none. Each group's units before and after add up by each field's own rule, as the
    engine adds them (`addUp`, the field's `add_up` now sent with it), so the groups' subtotals together are the
    block's. A piece that leaves a group is listed there too, in red, with where it went.
  - **Field sets.** This step (what it reads and what it changed on the block, with thickness and volume; the default),
    Quantities, Qualities, Class and audit, All, and a planner's own. Find a field picks from every field. Changed only.
    A change shown as "before → after" or "after, was".
  - **The picked piece.** Every field before and after, the last step to write it, and its journey through the steps:
    where it was made, each volume, class or destination change, where it went.
  - **The field map.** Every step by the fields shown, which reads (R) and which writes (W) each on this block.
- **Field sets saved in the project** (decision 12): `Display.field_sets` (`zd-core`), display state like a look or a
  filter, saved with the next Save and never a revision or undone; `Project::set_field_sets` names each once, none a
  built-in set's, its fields trimmed and each once; `field_sets` and `set_field_sets` in the app shell.
- **Tested.** App (`tests/caseUnits.test.mjs`): B5-like U, 6,507 m³, kept its id less its losses and four pieces split
  off it, adding back to 6,507 m³; a join's benches into the joined unit; a joined unit's pieces its own; the groups'
  subtotals the block's before and after, the loss pieces red in the section they left, ash by mass none where waste
  weighs with none; the field sets; a piece's journey, the last step to write a field, and the field map. `zd-core`: field
  sets named once and kept as display state; `zd-io`: they survive a save and open. In the app (`--bench --case`, on the
  demo): B5 at `rom_loss`, its U's five pieces adding to the 1,292.5 m³ it was (exact), the groups' volumes adding to the
  block's before and after (1e-9), 12 pieces red where they left, a field set saved and deleted, a loss piece's journey
  "made here" at `rom_loss`, the field map's writes.

## Built: workstream 6, the flow and Open another dataflow

- **The flow** (decision 10, `app/src/panels/CaseFlow.tsx`, its sums in `flowStages.ts`), across the tab under the
  inputs: the case carried through the dataflow, a stage a column, the blocks first. The coal lane in tonnes (m³ until
  the units weigh) and the waste lane in BCM, each as wide as it is against the most it holds; under the coal what the
  stage took, red, by what became of it (thin, loss, high ash); under the waste the rock it let into ROM, amber. What a
  stage takes is counted by what left the coal lane, so the densities step putting coal on its in situ basis moves the
  lane's tonnes but takes nothing. A stage's name or a step's tick goes there; the stage open is lit; a figure an edit
  moved is blue, with what it was. It runs again with the case as you type.
- **The case through every step** (`CaseAtStep::flow`, engine): the units by material and what became of them after
  each step, kept as each step runs up to the one open, and after it run on from that step's columns, not kept, until
  something runs again. `reserves_case_step` sends it as saved and as edited. Python: `step_flow(table)`.
- **Open another dataflow** (decisions 4 and 6, `CaseSwitcher.tsx`), from the band: the case's dataflows in run order,
  each with its range in words (`strip 1-3`, every row, the case's blocks), what it reads (the blocks and seam order, or
  the columns above it, and its settings tables by name, now in `case_state`), up to date or why not, Open and Run; then
  Run what's out of date (each judged just before it would run, so a row dataflow below a reserves run just made runs
  too) and Run all, in order, each run its own undo, as Python's `run_case`. The band says the open dataflow's state.
- **A row dataflow opened** (`RowDataflowView.tsx`): its inputs (the rows in range of all, the columns above it the step
  reads, its settings tables lit where read); its steps, Formula or Table script; the step's script, what it reads off
  its source and the fields it set; its columns added up over its range by their rules, before the step, after it and
  as its last run left them; its settings by row, typed in as edits and run as you type, Save and run saving the tables
  and running it alone; and the rows in range before and after the step, a changed value lit, the first 400 shown.
  Engine: `row_dataflow_at_step` runs it to the step and the step before it on what's saved or as edited, nothing kept;
  `row_dataflow_step` in the app shell; `row_step(table, dataflow, step, tables=None)` in Python.
- **Fixed: a row formula reading a column in brackets beside a field by name.** A row formula was handed only the
  columns it read by name (`row.distance`) once it read any, so `row["ROM\\Coal\\tonnes"]` beside it came in empty and
  the formula stopped on every row. The columns named in brackets, as written or with their backslashes doubled, are
  now read too.
- **Tested.** Engine (`case_at_step.rs`): the made-up block's flow through every step, the blocks bringing 25,000 m³ of
  coal and 95,000 m³ of waste, `rom_loss` taking 1,000 m³ roof, 2,000 floor, 25 edge and 500 other, 21,475 m³ left,
  every step adding back to 120,000 m³; asked again, nothing runs; roof 0.15 m, `rom_loss` on 1,500 m³ and 20,975 left,
  the steps before it as they were; opened at the blocks or the end, the same flow (1e-9). `case_dataflows.rs`: Haulage
  in two steps on strips 1 to 6, its columns empty before the first, the distances (3 km the largest) and no
  tonne-kilometres after it, 337,500 t.km after the second, nothing written until it runs and then the last run the
  same; strips 1 to 3 at 4 km unsaved, 472,500 t.km; each dataflow's settings tables in the case state. App
  (`tests/flowStages.test.mjs`): the lanes, what each stage took and let in, a density change taking nothing, ranges in
  words. Python: Haulage's step as saved and as edited (67,500 and 90,000 t.km, nothing saved), the flow adding back at
  every step. In the app (`--bench --case`, the demo): the flow's stages (Blocks, In situ, Working sections, ROM,
  Final), ROM taking coal and letting rock in, each stage's coal what came in less what it took (m³, 1e-9), the blocks
  and in situ taking none; a step's tick goes there; a row dataflow added (Check haulage on strips 1 to 3) listed out
  of date, "it hasn't run on this table", with its range and settings; Run what's out of date ran it alone, then both
  up to date; opened at `tkm`, its tonne-kilometres after the step what its last run wrote (10,994,607 t.km); strips 1
  and 2 typed from 2 to 4 km, 17,533,430 t.km as you type; discarded, as saved.
- **Timed** (the demo, 90 block columns, release build): the flow first asked at `rom_loss` 42 ms (the steps after it
  run on), asked again 0.2 ms; an edit and the flow 54 to 56 ms over five edits, inside the 330 ms the tab waits after a
  key. A row dataflow at a step (698 of 1,436 rows, two formula steps) 290 to 300 ms an edit: one run to its end, its
  columns kept at the two steps shown (three runs took 420 ms), about what `table.run_dataflow` takes for the same rows
  (255 ms). A row's words for its filters are now read only from the columns a filter can name (the range's, a step's
  and its settings tables'), not all 455.

## Known limits

- **Not the command decision 14 named.** A row dataflow runs by `table.run_dataflow`, not `reserves.run`: a reserves
  table's column view, Run again and changed-since read the last `reserves.run` that wrote the table, and a row run kept
  apart never stands in for it. "All in order" is `run_case` in Python, a run a dataflow, each its own undo, as decision
  6 says; the app's Run all (workstream 6) does the same.
- **Reads from below are found in the source.** A script that builds a column's name (`"Haulage\\" + x`) isn't caught
  when saved; the run then reads the column as it stands, from that dataflow's last run.
- **A row dataflow's columns are named by its own name.** Renaming the dataflow writes new columns next run; the old
  ones stay until removed.
- **A dataflow taken off the list leaves its columns** on the table; a new dataflow wanting one of their names is
  refused as if the reserves run wrote it.
- **Out of date reads the log.** A project saved and opened carries its log, so the state comes back with it; a table
  whose runs aren't in the log (made before logs were kept) says its dataflows haven't run. A block revised and then put
  back as it was by hand, not by undo, still counts as changed.
- **No way in yet** (workstream 7): New case, Copy case and Add a dataflow, the cases along the foot with their headline
  figures, and the Setup and Dataflow tabs taken out. A row dataflow is added to a case in the Setup tab or Python.
- **The flow is the case's units.** It shows coal and waste in play and what became of them, not what's written to the
  table: ROM coal is the coal lane at `final` with the rock let in, in situ tonnes, not the ROM density's. The flow
  after the open step runs again with each edit any step reads, the whole of it; on a much larger mine that is the
  steps after the one open run again as you type.
- **A row dataflow opened** shows its rows in range, the first 400; its step's reads are a formula's off its source
  (a table script's reads aren't listed), and its settings by row don't ring the cell a row used. Its own dataflow as
  edited in the Parameters tab runs here too; Save and run saves the tables, not the dataflow. A row dataflow runs in
  about a third of a second on the demo's 700 rows, the formula's own run most of it; not yet tuned, nor measured on
  PIT B.
- **The units table is the picked block's** (workstream 5), drawn as a table of its own in the Tables grid's look, not
  the grid itself: the grid holds one value a cell, and a row here holds before and after. A piece's journey follows its
  id; a piece split off starts where it was made. Field sets save with the project's next Save, as looks do. The Tables tab still says what changed since the reserves run, as before.
- **Settings typed are only the settings tables'.** A step's own settings in the dataflow (a built-in step reading
  `roof_loss` from another column) and its script are edited in the Parameters tab, and run here as edited; Save and run
  the case saves the tables, not the dataflow. The run's own rows (a setup's `parameters`) show, not typed in.
- **The strip log draws by thickness.** A unit with no thickness (an area-only piece) is a hairline; its volume is in
  its tip.
- **Edits to a table the Parameters tab has open elsewhere.** The Parameters tab keeps the edits of the table it shows;
  switching it to another table drops that table's edits, wherever they were typed, as it always has.
- **Only the step before the one open is kept.** An edit read by an earlier step runs the case again from its blocks,
  as decision 9 has it; on PIT B and the demo that is still well under a second. A much larger mine (the 140,000-block
  test mine) isn't measured.
- **A script's reads are read off its source.** A name built as it runs is taken as a field's own or its limits, as
  Read by takes it; anything else built (`getattr(p, "roof" + x)`) isn't seen, and an edit to it reruns from the step
  that first reads something seen.
- **Units' tonnes are in situ.** The units' groups weigh by the in situ density, as they stand mid-flow; what's written
  under ROM and Scheduled has its tonnes on the ROM basis, as the table.
- **The app's dataflow editor** opens a row dataflow as it opens any, with the reserves names; its own editor comes with
  the case tab.
