# 0082 — The notebook calculates live: built-in workings, rerun as you edit

Status: agreed 17 November 2026. Tim agreed the seven decisions on the Build Plan's "Live workings plan" tab ("Agree all
7, start ws1"). Workstream 1 (the engine's workings) is built here; 2 (the Workings row) was built in the Dataflow
view's step pane (0084, workstream 3); 3 (live) is built here, on the Dataflow tab, after Tim asked on 18 November how to
run the dataflow again once a parameter was edited; 4 (the rest of the steps) is built here too, as is point 7 (a
setting clicked goes to its cell in the Parameters tab). The plan is done.

## Why

Tim, 17 November, with the notebook (0081) open: "I was thinking each step would be calculated live so you can see it."
He meant both the arithmetic of the built-in steps shown, and the notebook running again as he edits, before a save. A
formula cell already showed its lines with their values; a built-in cell showed its rule in words and the settings it
read, but not what it did with them.

## Decision

1. A built-in step writes its workings for each unit as it runs, only when the column is opened as a notebook (as the
   settings it reads are recorded, 0081); a reserves run writes none and runs as before. The cell shows the unit in focus.
2. The steps that move material write theirs first: working_sections, rom_loss, rom_dilution and rom_ash_cutoff; then
   in_situ, join_benches, rom, rom_qualities and final. Steps that only copy or set fields keep their rule in words.
3. A working is a line: what was worked out or tested, the numbers that went in with their names and units, what it came
   to, and what followed (`roof dilution = roof_dilution 0.05 m × area 5,000 m² = 250 m³, off the bottom of section 3
   waste B, into section 4 coal B`).
4. The open notebook runs again about a third of a second after typing stops in a formula or a parameter cell, on the
   unsaved version. Nothing is kept; an edit that doesn't check leaves the last good run showing, with why.
5. An unsaved parameter table stands in for the saved one in the notebook's run only, marked edited; saving stays in the
   Parameters tab.
6. What moved shows in blue, old → new, against the saved run.
7. A setting in In or Workings goes to its cell in the Parameters tab when clicked.

## Built: workstream 1, the engine's workings

`StepView.workings` (`crates/zd-commands/src/builtin/reserves_run.rs`): a `Working` a line, with the unit it's for (its
id as the step found it), `what`, `sum`, `value` and its `measure`, and `then`. `ColumnRun::work` writes a line on the
units it names when the column runs as a notebook or preview, and does nothing in a reserves run; the line is made only
then. Numbers are written as a planner reads them: thousands marked, settings to what they hold, volumes to a tenth.

- `working_sections`: coal discarded and why; each run of waste between plies, its thickness against `min_parting`
  (its own row's, or the larger of the plies' either side) and whether it breaks; each strip-ratio test (the ply's
  volume × `ws_strip_ratio` against the parting's), on the parting and the ply; each section's thickness, partings in,
  against `min_coal_thickness`; a forced split by `ws_split`; and each unit's section number, with a parting in a coal
  section said as parting dilution.
- `rom_loss`: each coal section's roof and floor loss (setting × the section's area) and where it went, or why none; the
  section against its roof and floor loss; each ply's edge loss (setting × length × thickness) and other loss (% of its
  volume) and where it went.
- `rom_dilution`: each waste interval's floor, roof, edge and other dilution, with the sums and which end of the waste
  it's cut from into which coal section, on the waste's units and the coal section's; a waste short of what's asked,
  with each share.
- `rom_ash_cutoff`: each ROM coal section's ash (its coal, partings and dilution, by the field's rule) against its
  cut-off, rejected or kept; or why it wasn't tested.

Every unit's workings come with the column, so the notebook shows the focus unit's without a second call, and the live
rerun (workstream 3) is one call. The plan had `column_trace` give a built-in step's workings for a unit; it isn't
needed, and stays for formula steps.

Checked in `crates/zd-commands/tests/reserves_run.rs` on the hand-worked columns: rom_dilution's four sums equal the
pieces they cut (1e-9 relative) with their words exact; a waste short of its dilution gives each share (13,636.4 m³ of
15,000); working_sections' parting, strip ratio, thickness, discard and section lines exact; rom_loss's roof, floor,
edge and other lines (500, 1,000, 10 and 200 m³); the ROM ash cut-off's test (16.32 % > 15 %) and an untested section.
The reserves tests that check the tables are unchanged and pass: the workings change nothing a run writes.

## Built: workstream 3, live

Tim, 18 November, editing `min_volume` in the Parameters tab with the Dataflow tab open beside it: "i dont see how i
re-run the dataflow once the parameter was edited". The tab ran its column only when one was opened.

- **The engine runs a column on what isn't saved.** `Unsaved` (`crates/zd-commands/src/builtin/reserves_run.rs`): the
  dataflow as edited, and parameter tables as edited, each standing in for the saved one with its id; `table_column` and
  `column_trace` take it. An edited table is checked first and refused with its name. `ColumnSteps.unsaved` names what
  the run stood in for: the dataflow, and the edited tables among those it reads. Nothing is kept.
- **The Parameters tab hands its edits on.** A table being edited, or a dataflow (its formulas with it), is a draft in
  the workspace (`drafts`, `setDraft`) while it differs from what's saved, and goes when it's saved, discarded or closed.
  A table whose grid doesn't check hands on why instead of the table. A dataflow is always handed on: a worked example
  that fails is flagged and run anyway (0081), and what can't run comes back from the engine with why.
- **The Dataflow tab runs on them.** It runs its column on what's saved, and on the drafts it reads (its own dataflow,
  the tables it reads) 330 ms after they stop changing. An edit that doesn't check, or a run that fails, leaves the last
  good run showing and says why. Saving a parameter table or dataflow runs the column again (their revisions are part of
  what's asked), so what's saved is what's shown after a Save.
- **What moved** (point 6): the units of the live run are matched to the saved run's by what they are (horizon, bench,
  material, what it became) and their order, as an edit that makes or takes a unit moves the ids. A value the edits
  moved is washed blue in Before and After with the saved run's struck through beside it; a unit the saved run hasn't is
  marked edit; a write in the strip they moved shows the saved volume struck through; the table read is marked edited in
  Inputs, and a step whose formula, filter or rule is edited is marked edited in the step pane. A banner says what the
  run stood in for. Whole column runs on the same edits.
- **Run in Dataflow is Show in Dataflow**: the tab runs on a formula as it's typed, so the button only brings the tab
  forward on its step. Back to the saved formula is Discard in the Parameters tab.

| Test | Known answer |
|---|---|
| `a_column_runs_on_a_parameter_table_as_edited_and_the_project_keeps_the_saved_one` (engine) | roof dilution 0.10 m for 0.05 m: 1,000 m³ of roof dilution, not 500 m³ (1e-9 relative), the table named as unsaved; the saved run still 500 m³ and the table's revision unchanged; a table edited past its own check refused with its name |
| `dataflowView.test.mjs` (3 more) | units matched by what they are when an edit moves their ids; OB cut 49,000 m³ against the saved 49,500 m³ and the dilution 1,000 against 500 marked, U's benches not; the strip's Dilution 1,000 against 500; a unit the saved run hasn't marked made by the edit |
| `zd-app --bench --dataflow` (16 Nov demo, 3_B1) | roof dilution doubled in the Parameters tab beside it: rom_dilution's write and 12 values moved within about 0.4 s, the table edited in Inputs; a word typed keeps the last run and says why; Discard brings the saved run back. rd_is doubled in densities: picked by Show in Dataflow, marked edited, its failing worked example flagged and run, the densities it moved marked; Discard runs the saved formula again |

## Built: point 7, a setting to its cell

A setting in the Dataflow tab's step pane opens the cell it was read from in the Parameters tab: in In, each setting a
built-in step read from a parameter table is a button; in Workings, each setting the step read for the unit followed is
a link where its name stands in the line. The Parameters tab shows the table, scrolls the cell into sight, focuses it
and rings it gold for a moment, so it's typed into at once (and with the Dataflow tab beside, the column runs on the
edit live).

- **Which table.** `ColumnSteps.table_ids`: each table read, by id, beside `tables`; none for the run's own rows or a
  table since deleted, whose settings stay words.
- **Which cell.** A read's row is the table's row as saved; the Parameters tab finds the grid column that holds it
  (rows can be added before it while editing), and the parameter's cell in it. Asked through the tool ask
  `cell:<table>:<row>:<parameter>`, as a formula's Edit is `formula:<dataflow>:<step>`.
- A formula's reads are in its trace by name and not lit (0084), so they open nothing.

| Test | Known answer |
|---|---|
| reserves_run engine test, the live column | the column's one table, assumptions, by its id |
| `zd-app --bench --dataflow` (16 Nov demo, 3_B1, rom_dilution, U followed) | roof_dilution clicked in In opens column 2's roof_dilution cell, focused and ringed; floor_dilution clicked in a Workings line opens column 2's floor_dilution cell |

## Built: workstream 4, the rest of the steps

Point 2 named in_situ, join_benches, rom, rom_qualities and final after the steps that move material. Since 0077,
`rom` does nothing built in and `rom_qualities` is a starter formula, whose own lines are its trace; so the three left
are built in, each writing its lines as the others do (`ColumnRun::work`, only when the column is opened):

- `in_situ`: each unit's volume × its in situ density, its in situ tonnes, and the folder it's written to (on its
  benches by their in situ shares, for a joined unit); its volume and folder where the dataflow has no in situ density;
  a unit set aside, or the underburden outside the model, not written, said.
- `join_benches`: each join on its benches and the unit they make: the benches' volumes added up to the unit's, and
  each bench's in situ share, kept to write it back.
- `final`: where each unit goes. ROM coal: a section's unit on its own leaves, roof dilution on its first ply's,
  floor dilution and losses on its first or last, edge and other dilution shared over its plies by their volumes (each
  ply's part said); with `aggregate_sections` yes, the section's qualities every piece carries. Clean coal: each ply,
  at the section's average where `clean_coal_average` (or `aggregate_sections`) says yes. ROM waste: on its own leaves,
  or a loss on the waste it went into. Model waste below the last coal netted off out of model; small volumes and the
  underburden left never in ROM.

| Test | Known answer |
|---|---|
| `final_writes_where_each_unit_goes_for_the_notebook` (the 4c column, dilution only) | A's ROM coal 10,000 m³ on its own leaves; the roof dilution cut for it, 250 m³, on A's; every ROM coal line adding up to the table's 10,854 + 15,906 m³; A's clean coal 10,000 m³; OB's ROM waste 50,000 − 250 − 4 − 100 = 49,646 m³; below B's 19,500 m³ netted off out of model. 1e-9 relative |
| `join_benches_and_in_situ_write_their_workings_for_the_notebook` (B2) | U's benches 18 and 16, 10,000 m³ each, joined into 20,000 m³, shares 0.5 and 0.5, said on both benches and the unit; each unit's in situ volume and folder; the underburden not written |
| `in_situ_writes_each_unit_s_tonnes_for_the_notebook` (starter densities) | each coal unit's tonnes its volume × its rd_is; U's rd_is 1.358 ÷ 0.99, said 1.3717 t/m³ |
| `zd-app --bench --dataflow` (16 Nov demo, 3_B1, U followed) | in_situ shows in situ tonnes, join_benches the join, final ROM coal and clean coal |
