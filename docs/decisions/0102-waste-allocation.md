# 0102 — Waste allocation: Spry's Scheduled folder filled in the reserves run, a section a process, words on units, every setting from a parameter row

Date: 2026-11-23 · Status: accepted (Tim, 2026-11-23: "Agree all eight", on the Build Plan's "Waste allocation plan"
tab, a sub-tab of the Schedule quantities plan; decision 8 his: "the parameters drive the majority of these decisions
... The use of the parameters is the way to make this easy to audit and check")

## Why

Workstream 4 of the Schedule quantities plan (0101) was a starter formula after `rom` filling a few schedule fields.
Tim, 23 November, on where the process codes live: "each process having a section and inside that would be Prime
Volume, Total Volume, Rehandle Volume (etc). This field would be called something like Scheduled/Excavator/PrimeVolume
... you would create what processes you want to schedule and ensure the dataflow is filling them out correctly." He sent
his default Spry template, the table and dataflows a schedule is built from.

Its Import to ROM dataflow is what the reserves run already does. Its Waste Allocation dataflow runs after it on the ROM
results, a column (pit, strip, block) at a time: topsoil and freedig split off the top by thickness; blasting horizons,
blasts grouped, thin waste ripped; the dragline system's method chosen from 5 zones down to 1, each zone split by the
method's factors into cast, pre-doze excavator, dozer push, dragline, poststrip and wedge, with rehandle; what's left
to excavator, thin excavator waste to partings, a wedge over each coal; drill and blast production, rehandle and swell
by process, the coal exposed horizon; each process's quantities written to its own section under `Scheduled`, the
drill and blast and coal exposed onto the horizon's leaf. Our run could hold none of it: every field was a number, a
formula couldn't split a unit or see the column, and fields were written only by stage and material.

## Decision

The eight decisions of the plan tab, as agreed:

1. **Allocation runs in the reserves dataflow, after `rom`,** before `final`, on the run's own units a column at a time:
   one run gives ROM and Scheduled together, and Run again, the notebook, the trace and the balance all work on it.
2. **A field may hold words** (Process, Method, Zone, Drill Type, Blasting Seam and Bench), set by a script, read by
   later steps, written to the table as a words column; a leaf whose units differ shows them joined. It doesn't add up.
3. **An allocation script may split a unit**, by thickness from the top or by volume shares; the pieces add back to the
   unit exactly, each split traced and counted. A formula step still can't (0077, decision 2, stands for formulas). The
   solids stay the volume of record: a split shares a unit's volume, it never makes one.
4. **An allocation script works on the column**, `def allocate(column, p):`, the grouping and splitting done in Rust.
5. **A field named with a folder is a schedule field**, `Waste\Dragline\Prime Volume`, written under `Scheduled` only,
   on the unit's leaf or placed onto another leaf the unit names.
6. **`lookup` and `interpolate` on several number keys**, each nearest or between two rows, for a dragline factors
   table.
7. **Add waste allocation** adds the template's fields, parameter tables, steps and checks in one action.
8. **Parameters drive every allocation**: every number and choice a step uses comes from a parameter row read by the
   unit's or column's keys, never written into a script; each read is traced to its table, row and revision. The
   starter scripts hold the order and the logic only.

Five workstreams, in that order: words, schedule fields, allocation steps, several keys, the starter. Workstream 5 of
0101 (splits by a surface) follows them.

## Built: workstream 1, words

- **Word fields on a dataflow** (`zd_core::WordField`, `Dataflow.words`): a name and where its words come from, a block
  attribute (else the parameter tables' column of its name), a parameter column, or worked by a step. Checked when the
  dataflow is saved: each named once, never a number field's name or a unit's own label (`seam`, `material`, `bench`,
  `became` and the rest, `zd_core::UNIT_LABELS`), never from a solid's measure. Saved only where a dataflow has some.
- **Carried by each unit** (`zd_reserves::Unit.words`): a piece split off keeps its unit's words; a join keeps a word its
  benches agree on and joins those that differ, each once, in order (`joined_words`).
- **Read and set by formulas**: declared to every script as a field of words (`declare_words`), so `unit.process =
  "Dragline"` sets it, `None` clears it, and a number on it is refused naming the field.
- **Written by leaf**: wherever a unit's numbers are written (Imported, Insitu, loss and dilution, ROM, Scheduled, clean
  coal), its words go beside them as words columns under the same folder (`ROM\Waste\process`), a leaf's units' words
  joined; the balance's moves and the Summary take none. A word field no leaf has a word for has no column.
- **Shown**: the notebook's units carry their words (in a unit's description), the Dataflow tab's Every block table
  gives each word field a column after the labels, and the Parameters tab's Fields lists **Word fields** under the number
  fields, added, named and taken out there, with where each comes from. Properties lists them.

| Test | Known answer | Tolerance |
|---|---|---|
| `word_fields_hold_together` (zd-core) | saved and read back; twice, a number field's name, a label, a solid's measure, spaces at the ends refused | exact |
| `words_follow_a_split_and_a_join` (zd-reserves) | OB's benches Dragline and Poststrip, both Zone 1, join to `Dragline; Poststrip` and `Zone 1`; a split piece keeps its unit's words | exact |
| `word_fields_are_set_by_formulas_and_written_by_leaf` (reserves_run.rs) | B2's two U benches (10,000 m³ each) on one leaf set Dragline and Poststrip: the leaf reads `Dragline; Poststrip`, its dragline volume 10,000 m³ worked from each unit's own word; a parameter column's `Strip` and an attribute's `U` carried, from Imported on; a worked word has no Imported column; the notebook's units show their own; a number on the word field refused | 1e-9 |
| `a word field takes its attribute or parameter column...` (app) | the Fields editor's word field helpers | exact |

## Built: workstream 2, schedule fields

- **A field named with a folder is a schedule field** (`zd_core::is_schedule_field`): `Waste\Dragline\Prime Volume`, a
  number or a word field. Checked when the dataflow is saved (`check_schedule_fields`): every folder words with no spaces
  at its ends, not starting at `Scheduled` (it's written there already), and never the unit's own volume or thickness,
  which every stage writes. A schedule number field is kind `other`; its table unit comes from its last name where its
  kind has none (a volume m³, a thickness m, tonnes t).
- **Written under Scheduled only**: as `Scheduled\Waste\Dragline\Prime Volume`, a leaf's Scheduled pieces, coal and
  waste, added up by the field's rule. Imported, Insitu, loss and dilution, ROM, clean coal and the Summary take none,
  and the balance is the stages' volumes as before. A word schedule field goes beside it the same way.
- **Set by a formula** by its name in square brackets, `unit["Waste\\Dragline\\Prime Volume"] = prime` (the handle took
  names so already).
- **Placed onto another leaf**: `unit.place_onto("Waste\\Drill and Blast\\Volume", x, seam=…, bench=…)` writes the value
  on the leaf of the block those levels name (a level not named is the unit's own), not the unit's; it sets nothing on
  the unit. Only a field named with a folder that adds up by sum is placed; a level that isn't one below the column stops
  the block, naming the levels. `final` writes each where the column has something scheduled on that leaf, keeping the
  leaf's own spelling; one onto a leaf with nothing scheduled lands nowhere, said with its block and never written, so
  no leaf is made for it. A joined unit's value is shared over its benches by their in situ shares. The trace shows each
  placing (`Event::Placed`) and `final`'s workings say where it went.
- **Shown**: the Reserves table tab's Breakdown has each section under Scheduled as `Waste › Dragline`; the reference
  and the editor's suggestions have `place_onto`.

| Test | Known answer | Tolerance |
|---|---|---|
| `schedule_fields_hold_together` (zd-core) | a number and a word schedule field save; an empty folder, spaces at a folder's end, a name from `Scheduled`, the unit's volume named so refused | exact |
| `a_schedule_field_is_placed_onto_another_leaf` (zd-script) | 300 m³ × 0.5 placed as 150 onto seam L, bench `110`; traced; a plain field, a word field, an unknown field and words refused | exact |
| `schedule_fields_are_written_under_scheduled_by_process_and_placed_onto_a_leaf` (reserves_run.rs) | the loss column's OB, 50,000 m³ of ROM waste on seam A's leaf, 0.8 dragline with rehandle 0.2 and swell 0.3, all from an allocation row: Prime 40,000 m³, Rehandle 8,000, Total 48,000, Swelled 52,000 under `Scheduled\Waste\Dragline` on A's leaf, Method `Strip`, none of them outside Scheduled; the other 10,000 m³ placed onto seam B's leaf as drill and blast, none on A's, 10,000 in all; the balance unchanged; `final`'s workings name the leaf; onto seam Z said and not written; `zone=` stops the block | 1e-9 |
| `schedule fields show under Scheduled, a section a process` (app) | the Breakdown's rows and values | exact |

## Built: workstream 3, allocation steps

- **An allocation step** (`DataflowStep.allocation`, `zd_core::AllocationStep`): a step of its own name holding
  Starlark that defines `def allocate(column, p):`, run once on each block column. Checked when the dataflow is saved
  (`check_allocation_steps`): it sits after `rom` and before `final` (a formula step may follow it there), never takes a
  built-in step's name, has no settings, and compiles. A step that is a formula or table script too is refused.
- **The column, in Rust** (`zd_script::Allocation`, the `column` and `unit` handles in `alloc.rs`): the script works
  on the run's own column, so a split is the column's split. `column.units` top down; `column.waste()` and
  `column.coal()`, the units that end in ROM waste and in a ROM coal section, as `final` decides (small volumes, the
  underburden left and waste below the last coal end in neither); `column.after(u)` and `column.before(u)`;
  `column.group_while("process", "zone", units=…)`, runs of neighbours with the same values; `column.total(field,
  units=…)`, added up by the field's rule; `column.name` and `column.block`.
- **Splits** (decision 3): `u.split_thickness(0.5, 4.5)` takes pieces of those thicknesses off the top and gives the
  rest; `u.split_shares([0.2, 0.3, 0.5])` splits by volume shares adding to 1, the last piece what's left. A piece of
  nothing is None. Each piece keeps the unit's labels, words and ending, its summed fields shared and the rest kept, so
  the pieces add back to the unit exactly and the balance holds. More than the unit holds, shares not adding to 1, or a
  negative share stop the column, said with the unit. A split puts its pieces in the unit's place, so the plan's
  `column.replace` isn't needed. Each split is counted (`split by <step> into n pieces`) and worked in the notebook on
  its pieces (`OB waste by thickness 0.5, 4.5: 2,500 m³ + 22,500 m³ + …`).
- **Sets and placings**: a unit's fields and words are set as a formula's (`u.process = "Wedge"`,
  `u["Waste\\Wedge\\Prime Volume"] = u.volume`); the volume is of record and never set, and a label isn't set.
  `u.place_onto(...)` works as a formula's (workstream 2).
- **Every setting from a row** (decision 8): `p` is the column's parameters, read by its levels above the seam; `u.p` a
  unit's, by its block's attributes; `lookup(...)` any table by its keys. Each parameter read through `p` or `u.p` is
  traced in the notebook to its table and row, as a built-in step's are; `lookup` reads are in the trace.
- **Shown**: the Parameters tab's Steps has **+ Add an allocation before final**, each allocation step a row opened in
  its own editor on the right (checked as it's typed, completions for `column.` and a unit's calls, Copy for AI, the
  reference's allocation part). The Dataflow tab marks the step ⇶, shows its script with the lines that ran (traced on
  the whole column, whichever unit is followed), every setting it read with its table and row, and its splits under
  Workings; the graph wires the tables it read. The reference has a section, "Writing an allocation script", whose
  examples are compiled by a test.

| Test | Known answer | Tolerance |
|---|---|---|
| `loose_ground_and_a_wedge_are_split_off_by_thickness_and_add_back_exactly` (zd-script) | the plan's 30 m waste unit of 3,000 m² over coal: topsoil 0.5 m, freedig 4.5 m, wedge 3 m from the column's row give Topsoil 1,500 m³, Freedig 13,500, Excavator 66,000, Wedge 9,000; OB's pieces add to 90,000; the splits and the reads noted | 1e-9 |
| `a_changed_parameter_row_changes_the_split_and_not_the_script` (zd-script) | the same unit with 1 m of topsoil: Topsoil 3,000, Freedig 13,500, Excavator 64,500, Wedge 9,000 | 1e-9 |
| `shares_split_a_unit_in_order_and_the_last_piece_is_the_rest` (zd-script) | 90,000 m³ by 0.2, 0, 0.3, 0.5: Cast 18,000, None, Dozer Push 27,000, Dragline 45,000, adding to 90,000 exactly; runs grouped by process; a total of two pieces 45,000 | 1e-9, exact sum |
| `what_an_allocation_script_may_not_do_is_refused_with_why` (zd-script) | too thick a split, shares not adding to 1, a negative share, the volume set, a label set, words as a number, an unknown name grouped by, `after` given a number, a script without `allocate` | exact |
| `an_allocation_traced_shows_its_lines_reads_sets_and_the_column_it_left` (zd-script) | the split line ran, `p.topsoil` read, the topsoil piece's process set | exact |
| `the_allocation_calls_offered_are_there`, `every_allocation_script_in_the_reference_compiles` (zd-script) | every call the editor offers exists; every reference example compiles | exact |
| `an_allocation_step_holds_together` (zd-core) | saved and read back; no script, settings, or a formula too refused | exact |
| `an_allocation_step_splits_the_column_and_fills_each_process_from_the_parameter_rows` (reserves_run.rs) | 30 m of OB over A coal, 5,000 m²: Topsoil 2,500 m³, Freedig 22,500, Wedge 15,000, Excavator 110,000 under `Scheduled\Waste\<process>\Prime Volume` on A's leaf, adding to its 150,000 of ROM waste; the leaf's process `Topsoil; Freedig; Excavator; Wedge`; the waste below A not taken; the notebook cell an allocation with OB in four pieces, the split worked, `topsoil_thickness` traced to the settings table's row 1; traced, the split line ran; the row changed to 1 m, Topsoil 5,000 and Excavator 107,500 | 1e-9 |
| `a_dragline_system_splits_its_zones_by_method_factors_looked_up_by_zone` (reserves_run.rs) | the plan's dragline case on 60 m of waste (300,000 m³): a 50 m system in two 25 m zones; Excavator 50,000; zone 1 Cast 25,000, Dozer Push 37,500, Dragline 62,500 with rehandle 62,500 × 0.7 ÷ 0.5 − 62,500 = 25,000; zone 2 Dragline 125,000 with rehandle 62,500; on A's leaf Dragline 187,500, rehandle 87,500, zones `Zone 2; Zone 1`; the primes add to the 300,000 of ROM waste | 1e-9 |
| `an_allocation_step_sits_between_rom_and_final_and_a_bad_split_stops_the_block` (reserves_run.rs) | an allocation after initialise refused, one without `allocate` refused, a split of 40 m of a 30 m unit stops the block naming it | exact |
| app: `an allocation step's tables are wired…`, `an allocation step's guide…` | the graph wires the tables an allocation read; the editor's guide is the reference's allocation part | exact |

Not timed: an allocation runs one Starlark call a column, as a formula step does a unit; it hasn't been run on the
large test pits.

## Built: workstream 4, several keys

- **A key read nearest or between** (`zd_core::Graded`, `GradedKey`, `ParameterTable::read_graded`): `lookup` and
  `interpolate` take keys marked `nearest(x)` or `between(x)` beside their plain ones, so a table of rows reads as a grid
  (the template's Dragline Factors: method, dip, width, coal thickness, zone thicknesses). The plain keys filter the rows
  as any lookup does; then each marked key, in the order written, is read off its own column of numbers. `nearest` takes
  the rows at the value nearest it (the smaller of two as near, the end's outside them); `between` the rows at the two
  values either side, each weighted by how near, so two `between` keys read a value off four rows. Where rows are left
  after every key, the first is read, as a lookup reads.
- **Outside**: a `between` key outside its rows is refused, saying the value and the range; `interpolate`'s
  `ends="hold"` holds the nearer end's rows. `interpolate` reads its own `x` between, then the marked keys. `p.at` takes
  plain keys only, and refuses a marked one.
- **Traced**: a traced lookup names its rows and their weights, `lookup Dragline factors rows 3 (0.6), 4 (0.4): dragline
  (method 2 zone, dip nearest 12, zone_thickness between 24)`.
- **Saved**: a curve read with a marked key isn't checked as a curve on save (it has many rows at each `x`).
- **Shown**: the reference has "Reading a grid of rows"; `nearest` and `between` are in the editors' suggestions.
- **Found on the way**: after a split, the unit split is its last piece (the rest keeps its id), so a script reads what
  it needs of the whole unit before splitting it. Said in the reference.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_factor_is_read_between_two_rows_on_one_key_and_nearest_on_another` (zd-core) | the plan's case: a 2-zone method's dragline factor at dip 12 (nearest, 15) and zone thickness 24 m (between 20 and 30): 0.4 + 0.4 × 0.15 = 0.46 off rows 3 and 4, weights 0.6 and 0.4; both between, 0.3 × 0.54 + 0.7 × 0.46 = 0.484 off four rows whose weights add to 1; on a row's own values its value; two as near, the smaller | 1e-12 |
| `a_graded_key_outside_its_rows_is_refused_or_held` (zd-core) | 35 m outside 20 to 30 refused, held 0.6; a key or column the table hasn't, one of words, and no row for the keys said | exact |
| `a_factor_is_looked_up_nearest_on_one_key_and_between_on_another` (zd-script) | 0.46 by `lookup` and by `interpolate`; the trace's rows and weights; outside refused with the keys; `p.at` and a key in words refused | 1e-12 |
| `curves_and_lookups_are_read_off_the_source` (zd-core) | a grid read by `interpolate` isn't checked as a curve | exact |
| `a_dragline_system_reads_its_factors_nearest_on_dip_and_between_on_zone_thickness` (reserves_run.rs) | 60 m of waste over A coal, a 50 m one-zone system at dip 12°: share 0.5 and total 0.7 off the grid, Dragline 125,000 m³ with rehandle 50,000, Dozer Push 125,000, Excavator 50,000, the primes adding to the 300,000 of ROM waste | 1e-9 |

## Built: workstream 5, Add waste allocation, with the waste interval

Tim, 23 November, on workstreams 3 and 4: the scripts had a unit's thickness standing in for the waste interval's (the
working section's), the tests hiding it with one OB unit a column. Folded in here, as he asked. And on the aim: "people
may adjust these concepts using a script ... we should have the ability at the end of this to make many various options
and ideas of how to piece together a mine plan and get it ready for scheduling." So the engine's calls stay general and
the template is one way of using them.

- **The waste interval** (general calls, in Rust): `column.intervals()`, the runs of neighbouring units that end in ROM
  waste, top down, each a list: the waste between coal sections, a unit or several (two waste horizons, a coal rejected
  or discarded between them, a loss mined with the waste); `ends="coal"` gives the coal sections.
  `column.take_from_top(units, t)` and `column.take_from_bottom(units, t)` take a thickness across as many units as it
  needs, splitting the one it ends in, each split noted and worked as a split is. `u.place_onto(field, x, unit=top)`
  places a value on the leaves another unit is written on, shared by its benches.
- **Add waste allocation** (`zd_commands::builtin::with_waste_allocation`, `Session::add_waste_allocation`, the
  Parameters tab's **+ Add waste allocation (the template)**, `add_waste_allocation` in Python): four parameter tables,
  each kept where the project has one of that name (Waste allocation settings, read as `p` by the column's levels;
  Dragline factors, a grid by method and zone, dip and zone thickness; Waste processes, rehandle and swell by process;
  Short term overrides, empty); 36 schedule fields, a section a process (Topsoil, Freedig, Excavator, Partings, Wedge,
  Cast, Dozer Push, Dragline, Poststrip; Prime, Rehandle, Total and Swelled Volume) and drill and blast, coal mining and
  coal exposed; the words `process`, `zone`, `drill_type`, `blasting_seam` and `blasting_bench`; six allocation steps
  after `rom` (`loose_ground`, `short_term_overrides`, `dragline_system`, `excavator_partings_wedge`, `blasting`,
  `waste_quantities`) and `waste_checks` after `final`. Refused where the dataflow lacks `rom` and `final` or has the
  steps already. The template's method rebuilt as plain scripts on the general calls, every number from a row; none of
  its code copied.
- **The checks** (`waste_checks`, a table script): on every leaf the processes' prime volumes add to
  `Scheduled\Waste\volume` and coal mining to `Scheduled\Coal\volume`, within the settings row's tolerance; every
  waste leaf has a process.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_thickness_is_taken_across_the_units_of_a_waste_interval` (zd-script) | WEATH 8 m over OB 42 m over A coal, 3,000 m²: the interval is both; 45 m off its bottom takes OB whole and WEATH's bottom 3 m (9,000 m³); 1 m off what's left's top (3,000 m³), 4 m left; the pieces add back; 60 m of 50 refused | 1e-9 |
| `every_starter_script_reads_its_numbers_from_parameter_rows` (zd-commands) | no number in any starter script but 0 and 1 | exact |
| `waste_allocation_is_added_once_where_it_belongs` (zd-commands) | 39 schedule fields, 5 words, the steps between rom and final and the check last; a second time and a dataflow without rom refused; the tables parse | exact |
| `add_waste_allocation_fills_each_process_on_a_waste_interval_of_two_horizons` (reserves_run.rs) | WEATH 8 m and OB 42 m over A coal 2 m, a 2 m parting, B coal 3 m, 5,000 m², the default rows: Topsoil 2,500 m³, Freedig 22,500; a 40 m system in two 20 m zones from the bottom across OB: Cast 20,000, Dozer Push 20,000, Dragline 150,000 with rehandle 50,000 (total 200,000, swelled 250,000), Poststrip 10,000; Excavator 25,000 (swelled 31,250); drill and blast 225,000 on A's leaf; AB Partings 10,000, ripped; coal mining and exposed A 10,000, B 15,000; every check 0 | 1e-9 |
| The demo pit (`testdata/synthetic/workflow_demo.py`, Base, run by hand) | 1,815 blocks, 90 columns, 1,436 leaves: the primes add to the scheduled waste on every leaf (largest difference 3.6e-11 m³), coal mining to the coal (0), a process on every waste leaf; no drill and blast landing nowhere | 1e-9 relative |

Timing: the demo pit's Base run, 1,815 blocks, 241 ms with the starter added against 87 ms without.

Found on the demo pit: a blast's drill and blast placed by seam onto the leaf of a block whose top unit was a rejected
coal landed nowhere (the interval's units have other benches): placed now on the top unit's own leaves (`unit=`).

## Known limits

- A formula's worked examples (its known answers) expect numbers only; a word it sets is checked by running it.
- The Dataflow tab's Before and After show a unit's words in its description, not as columns of their own.
- `p` doesn't match parameter rows on a unit's words yet; `lookup("…", key=unit.process)` does.
- The editor suggests plain field names after `unit.`; a schedule field is typed in square brackets.
- An allocation step's `p` is read by the column's levels above the seam only: a row filtered on a seam or bench
  doesn't fit it; read those by `u.p` or `lookup`. A `lookup` read is in the trace, not traced to its row.
- An allocation's Dataflow tab cell shows the column's lines; Before and After show the pieces as units, their words in
  each unit's description.
- The Balance view checks the stages; the sections are checked by the starter's `waste_checks`, a mine's own check
  (0101).
- The starter leaves out the template's additional rehandle (bridge, ramps, endwall), its extra blasting horizons
  (first bench in a seam, first seam in a bench), its system method overrides and exclusions, and drill and blast's
  metres, explosives and times: each a step a site adds from the same calls. Its blast is a waste interval below the
  loose ground, drilled from its top unit.
