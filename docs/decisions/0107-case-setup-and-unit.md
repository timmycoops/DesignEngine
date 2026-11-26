# 0107 — Case setup and unit plan: the case's Setup as its own view with one place to run, and the picked unit said plainly. All 8 decisions agreed by Tim 2026-11-26.

Date: 2026-11-26 · Status: accepted (Tim, 2026-11-26: "Agree all eight", on the Build Plan's "Case setup and unit plan" tab)

## Why

Tim, 26 November, after the working area of 0106 was built, sending a picture of the Setup open on Reserves 50m and one
of a picked unit (M-L, 4 benches at `dragline_sys`): "The setup is weird and is hard to even know you are in it, also
appears to conflict with the run. Then it's quite small with a lot of rows of data and no real context about the header
or what's going on."

The Setup opened as a band under the header, marked only by a bold "Setup" and a thin line, with the flow and the working
area still under it. It had a Run again of its own, one of four ways to run on the tab (Run in the header, the Setup's
Run again, the block map's this block / its strip / every block, and Save and run), and its Run again did what the map's
every block did. Its two columns spread across a wide screen, and a settings table's Reads line named about fifty settings
in one run of text.

The picked unit sat in an 18 rem pane beside the units table, titled with the unit alone: no block, no step, so Before
and After said nothing of what they were before and after. For a unit the step made, Before was empty on every row and
Last written named the open step on every row; the waste fields' path names wrapped to two lines, and about twenty rows
read 0.00.

## Decision

App only, but for one check the case's out-of-date state learns (decision 2). No change to the object model or the stack,
no new dependency.

1. **The Setup is its own view.** Opened, it takes the working area under the header (the flow, the step, units and blocks,
   or an open row dataflow), framed by a title line saying whose setup it is and what it does, with Done at the right.
   Done, Esc or Setup again brings the case back as it was. The header stays, so the case, its figures and Run stay in
   sight.
2. **One place to run the case.** The Setup has no Run of its own: each change saves as the table's next revision (as
   before) and puts the case out of date on its dataflow chain, with why. The whole case runs from Run in the header, the
   reserves dataflow's Run there being every block again; the block map keeps this block and its strip, since those run
   part of the case. Save and run in the step stays: it saves the settings typed, then runs. The out-of-date check now
   also says when blocks it ran on are no longer picked by the setup (a layer taken out, a filter narrowed); it already
   said blocks added, and a changed dataflow, add-ons, seam model, settings tables, sources and range.
3. **The Setup reads top down in run order**, one column at most 70 rem wide, its sections numbered as a run uses them:
   1 Blocks it runs on, 2 The dataflow and its add-ons, 3 Dataflows after it, 4 Settings tables in the order read,
   5 Steps, 6 Fields and where each comes from. Each heading has a line under it saying what the section decides: lines,
   not tips, since the Setup is opened seldom and needs the words.
4. **Long lists fold.** A list of more than 12 names shows as a count ("Reads 53 settings"), opening to the names in
   columns; the fields show the table's own until Every field is clicked or one is searched for.
5. **The picked unit says what it is:** its title names the unit, its block and the step, and the column heads say
   Before and After that step. A unit the step made says so.
6. **Only the rows worth reading:** no Before column for a unit the step made; Last written says "this step", dimmed, for
   the open step, and names the step otherwise; fields the step changed first, lit; empty or nought fields folded.
7. **Waste fields grouped by their path** under a heading for the path, each row its short name, so no name wraps.
8. **The picked unit opens under its row** across the units table's full width, as the unit-by-unit workings do.

## Built

### Workstream 1: the Setup (decisions 1 to 4)

- `CaseSetup.tsx`: the frame (`setup-title`, Done as `setup-done`), the Run again and its call gone, the sections numbered
  (`data-section` 1 to 6) with their lines (`setup-line`), one column at most 70 rem, filling the height and scrolling in
  itself; `Names` folds a list past 12 to its count (`setup-names`, opened as `setup-names-open`), used for a settings
  table's reads and its columns not named, the tables looked up by name and the calculated columns; the fields show the
  table's own until Every field (`setup-fields-all`) or a search.
- `ReservesCasePanel.tsx`: with the Setup open, the flow, the working area and an open row dataflow are out of sight, their
  state kept, so Done or Esc brings back the same step, block or row dataflow; opening a dataflow from the chain or + closes
  the Setup. The block map's every block goes.
- `CaseSwitcher.tsx`: the reserves dataflow's Run in Run says Every block again.
- `case_state` (zd-commands): "N blocks no longer picked by the setup" when blocks it ran on are still there but no longer
  picked; a deleted one is said as gone, as before.
- Tests: `blocks_taken_out_of_the_setup_put_the_case_out_of_date` (strips 7 and 8 taken out of the setup's blocks: 4 blocks,
  an OB and an L each, said; run again on what's picked, up to date); the fold's boundary and count words in
  `reservesCase.test.mjs`. The case bench (`zd-app --bench --case`, the workflow demo) adds: the Setup its own view with the
  flow, the working area and the row dataflow out of sight; no Run in it; sections 1 to 6 each with its line; a settings
  table read for more than 12 settings folded to its count, its names on a click; Run's reserves dataflow is every block
  again; Done and Esc back to the case as it was. 63 of 63 pass, none dropped (a settings table read for 53 settings, 53
  listed on the click).

### Workstream 2: the picked unit (decisions 5 to 8)

- `caseUnits.ts`: `pieceSheet` lays out the picked unit's fields: whether the step made it, the fields it changed (moved
  on a unit there before it), the rest, and those empty or at nought and unchanged set aside; each field's Last written
  is "this step", the step's number and name, or "as read" (`lastWriterAt`, which `lastWriter` now reads); `fieldPath`
  splits a name written as a path (`Waste\Topsoil\Prime Volume`) into its heading (Waste › Topsoil) and short name.
- `UnitsTable.tsx`: the picked unit opens in a row of its own under the first row of it (`picked-piece-row`), across the
  table, held at the left as the table scrolls sideways. Its title names the unit, the block and the step ("U 110 ·
  Pit / 3 / B5 · step 10 rom_loss"); the heads say Before and After that step, Before left out for a unit the step made
  ("Made by this step: nothing before it"); the changed fields first, lit, under "Changed by this step", then the rest;
  path fields under their heading by short name, no name wrapping; "N more, empty or nought" folded; Last written dimmed
  for this step; the fields run in columns 30 rem wide as the width allows, its journey beside them. × or Esc closes it.
  The 18 rem pane beside the table is gone.
- Tests: `fieldPath` and `pieceSheet` in `caseUnits.test.mjs` on the made-up block at rom_loss (U coal kept: volume and
  thickness changed, 6,507 → 5,046 m³ by this step, rd as read, two empty waste fields folded under Waste › Topsoil; the
  roof loss made by the step: no Before, every field by this step). The case bench adds: the picked unit under its row
  and across the table; its title naming unit, block and step; a unit the step made with no Before and every field by
  this step; no field name wrapping; Esc closing it. 68 of 68 pass, none dropped. The demo's fields hold no paths, so
  the heading is checked by the unit test alone.

## Known limits

- Not tried by hand.
- The fold is by count alone: a list of 13 short names folds as one of 50 does.
- The picked unit's columns of fields have one head row, over the first.
