# 0082 — The notebook calculates live: built-in workings, rerun as you edit

Status: agreed 17 November 2026. Tim agreed the seven decisions on the Build Plan's "Live workings plan" tab ("Agree all
7, start ws1"). Workstream 1 (the engine's workings) is built here; 2 (the cell's Workings row), 3 (live) and 4 (the
rest of the steps) are to come.

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
