# 0070 — The reserves dataflow: Spry's grouping, a block column at a time

Status: agreed 15 November 2026 (Tim: "Yep I agree", all ten as recommended, on the Build Plan's "Reserves dataflow
plan" tab). Stage 5 of the Workflow plan, on 0061 (interrogation), 0062 (blocks are polygons), 0065 and 0066
(workflows and cases). Workstream 1, the column engine, is built here; the rest follow.

## Why

Interrogation puts in situ values on each block (0061). What Spry's Import to ROM does next, working sections, roof
and floor loss, dilution, the ROM ash cut-off and the moisture bases, is what a reserve is reported on, and it
stays in Spry until we do it. Tim asked for the grouping and logic first: "the formulas are easy to recreate but the
grouping and logic of how you do this are vital to it working". So the plan was reworked after reading his reference
Import to ROM (template v1.1.4), and keeps Spry's shape where it works and says where ours does better.

How Spry does it, in short: one block column at a time; a stack order sorted by seam, then material, then bench;
units carrying four layers of labels at once (in situ, working section, ROM, what it became), split and relabelled
rather than moved; every field declaring how it adds up, with settings copied onto the units as fields; only
air-dried values and moisture factors stored, the other bases worked out at write points along the way. What it does
badly: the walk that forms working sections is inside a function no one can read, the underburden is a fixed 10 m,
a dilution shortfall undoes losses without saying, and losses are written out before dilution can undo them.

## Decision

1. **The unit of work is one block column** (case, strip, block), blocks run in parallel.
2. **The stack order is the seam model's**, stored on each unit, not a sort by name or position. Benches inside a
   seam come when they come.
3. **A unit carries every layer at once** (in situ, working section, ROM, what it became) and the unit it came from.
   Pieces are split and relabelled, never moved between tables, and nothing is dropped without a label.
4. **Every field declares how it adds up** (sum, by volume, by mass, maximum, minimum), and settings are copied onto
   the units with their own rule (thresholds by maximum, the ash cut-off by minimum), as Spry does.
5. **The working-section walk is ours, written out as rules:** top down, the three tests (a parting thicker than the
   minimum breaks a section; a parting joins only if the coal covers it at the strip ratio threshold; a section counts
   only over the minimum thickness), then forced splits. Matching Spry's sections on PIT B is the check.
6. **No shortfall is silent:** dilution short of waste is said with how much, and no loss is undone without saying.
7. **The ROM ash cut-off is tested once after dilution**, as Spry does, so PIT B can be compared; each rejected
   section is said. Iterating can come later.
8. **No benches in v1:** a unit is a block's horizon. Benches and their shares come when the cuts make benches.
   *Changed by 0071 (decision 8):* a unit is a block's horizon and bench; a horizon's benches sort top down, are joined
   with their in situ shares for the working sections, loss and dilution, and are put back by share at each write
   (`join_benches`, `by_bench`).
9. **Stored on the air-dried basis with the moisture factors;** in situ and ROM bases are worked out at the write
   points, each written row listing the units it came from.
10. **The object and the stack:** a dataflow is a new object in the `.zdp`, its results tables kept as Parquet
    there; it runs as a workflow step and on its own; Rust on Arrow (Arrow for settings and results; a column in
    work is plain rows, tens of units); formulas in a small language of our own, built last; settings by range
    (seam, material, strip, block) with a default and CSV in and out, a missing one stopping that block.
    *Changed by 0077:* formulas are Starlark (`zd-script`), not a language of our own.

## Workstreams

1. The column engine (here). 2. The dataflow object, settings by range, the runner, results as tables. 3. Steps 1 to
4: small volumes, initialise, in situ, working sections. 4. Steps 5 to 9: ROM loss, dilution and the ash cut-off, ROM
calculations, final, aggregate and clean coal. 5. The app: steps on the canvas, settings, the per-block before and
after, results in the block table, `reserves.run` as a workflow step. 6. Checks: a hand-worked column, the demo pit,
PIT B against Spry by counts and differences only. 7. Formulas.

## Done when

A made-up column matches its hand-worked tables to 1e-9 relative, its shortfall and rejection said; the demo pit's
Base and 50m give the same ROM totals; PIT B's working sections and ROM coal by seam match Spry's within the reserves
tolerances (tonnes 0.5%, 0061), said by counts and differences only.

Not in stage 5: yield, product and washability; waste allocation (Spry's); Python steps (stage 6);
reporting the results and comparing cases (stage 7); ROM solids.

## Built: workstream 1, the column engine

A new engine crate, `crates/zd-reserves` (no UI, f64, depending only on serde):

- `Fields`: the fields in order, each with a kind and how it adds up, checked once: names unique, exactly one volume
  and one thickness, both summed, and every mass average naming a density field that isn't itself a mass average.
- `Column`: one block's units with its key and origins (the solid each input unit was cut as). A `Unit` has its id,
  origin, stack place, the four layers of labels and a value a field.
- What a step can do: `sort` (stable by stack place, so a unit's pieces keep their order); `split` a piece off the top
  or bottom by volume, thickness or fraction, the piece above or below the rest, summed fields shared out so the two
  add up to the unit, the rest kept, more than the unit holds refused as a shortfall with what was asked and what there
  was (beyond 1e-12 relative, which is rounding); `label_became` (loss, dilution, thin loss, high-ash loss, small volume,
  discarded), the set-aside kinds staying in the column; `runs` of neighbours; `add_up` by each field's rule, a
  missing value among units that weigh giving a missing answer, never passed over.
- There is no way to take a unit out, so `balance` (every unit added up) always equals the inputs' summed fields.

Tests, `crates/zd-reserves/tests/column.rs`, on a made-up two-seam block (overburden, cu, a parting, cl, waste below)
with known answers: the stack order; a section's volume, thickness, RD by volume (840/550) and ash by tonnes
(20,600/840) to 1e-15; settings by maximum and minimum; missing values; splits from the top and bottom and where the
pieces sit; the balance to 1e-12 through four splits and a set-aside; a shortfall said in full; runs by seam, the waste
above a seam with it, as Spry groups them; and the field checks.
