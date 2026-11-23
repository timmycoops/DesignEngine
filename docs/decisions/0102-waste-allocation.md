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

## Known limits

- A formula's worked examples (its known answers) expect numbers only; a word it sets is checked by running it.
- The Dataflow tab's Before and After show a unit's words in its description, not as columns of their own.
- `p` doesn't match parameter rows on a unit's words yet; `lookup("…", key=unit.process)` does.
- The editor suggests plain field names after `unit.`; a schedule field is typed in square brackets.
- The Balance view checks the stages, not the sections: that each process's prime volumes add to Scheduled waste is a
  mine's check (0101), which the starter (workstream 5) adds.
