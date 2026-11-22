# 0101 — Schedule quantities: lookups by any value, curves, each mine's checks scripted, schedule fields, splits by a surface

Date: 2026-11-22 · Status: accepted (Tim, 2026-11-22: "I agree", on the Build Plan's "Schedule quantities plan" tab,
decision 3 as he rewrote it: "this should be in the data flow or scripted based on which processes you determine you
need. Each mine may have slightly different processes that need to sum back together")

## Why

Tim, 22 November: is there anything the dataflows and scripts still need "to help ensure we have the ability to populate
out all the possible schedule quantities? Lookups on parameters and so forth is where I was thinking."

A formula's `p` read each parameter from the first row fitting the source block's attributes only. No row could be keyed
on a worked value (a seam's thickness once its benches are joined, its ash, a strip ratio), on the unit's own labels
(dilution's `became`, its working section), or on keys a script builds (equipment, process). There was no interpolation,
no way to say a schedule quantity was missing or that a mine's process splits added back to ROM, no starter set of
schedule fields, and bench rules cut by RL only.

## Decision

The five decisions of the plan tab, as agreed:

1. **Look up a parameter by any value.** `lookup("Rates", "bcm_per_hour", equipment="DL1",
   bench_height=unit.thickness, default=0)` reads a parameter table by its name: the first row fitting every key given,
   matched as rows are matched everywhere. `p.at(seam="U", thickness=x)` does the same over the script's own tables in
   their order. The tables looked up are bound by name when what looks them up is saved and kept by id, so a run records
   their revisions. `p` itself is unchanged, so no existing result moves.
2. **Curves.** `interpolate(table, x, x=, y=, **keys)`, linear between the rows that fit the keys; outside the curve it
   refuses unless the ends are held; a curve with two rows at one `x` isn't saved.
3. **Each mine's own checks, scripted.** Imported, Insitu, ROM and Scheduled stay built in, checked by the balance
   (0086). The process splits differ by mine, so each mine checks them in a table script step after `final`, with two
   helpers so every site's checks read the same: `check(name, parts…, equals=, tolerance=)` writing
   `Summary\Checks\<name>`, and `required(column, material=)`.
4. **A starter set of schedule fields**, added in one go and edited to the site's Spry processes, with a starter
   formula after `rom` and a starter check step.
5. **Splits by a surface**: a bench rule may name a surface or grid in place of RLs, the pieces the solids' own volumes.

Five workstreams, in that order.

## Built: workstream 1, lookups

- **`lookup` and `p.at`** (`zd-script`'s `lookups.rs`, `handles.rs`): every script has `lookup`; a formula's and a table
  script's `p` has `at`. Keys are words or numbers (a yes or no as words); `None` leaves a key out. A row filtering on
  something not given doesn't fit; a key the rows don't filter on is no hindrance. Where no row fits, the block stops
  saying the table and the keys (`no row of Haulage gives km for seam L`), or `default=` is given back. A column the
  table hasn't is said with "did you mean". A traced unit's reads show `lookup Rates row 2: bcm_per_hour (bench_height
  12, equipment DL1)` with the value.
- **Bound by name, read by id** (`zd_core::lookups_in`, `bind_lookups`, `lookups_by_id`): `Dataflow.lookups` and
  `CalculatedColumn.lookups` keep the ids of the parameter tables named in `lookup("…")`, bound when the dataflow or the
  column is saved; a name the project hasn't is refused then. A run reads them by id, so its record lists each table's
  revision, and a reserves table's audit says `Rates r2 (looked up)`. `table.script` binds them as it runs. The shell
  binds an unsaved dataflow's and column's the same way, so the Dataflow tab's live run and a formula's worked examples
  look up as a run does.
- **Where the tables come from**: `Declared.lookups` (`zd_script::Lookups`: the script's own tables in order, and the
  named ones), carried to a run as loaded scripts are; a table script's `TableInput.lookups`.
- **The editors**: inside `lookup("` the project's parameter tables are offered, then that table's columns, then the
  keys its rows filter on and `default=`. The reference gains "Looking up a parameter by any value".

| Test | Known answer | Tolerance |
|---|---|---|
| `a_rate_is_looked_up_by_a_worked_value` (zd-script) | DL1 at 0-10 m 1,800 BCM/h and 10-15 m 2,400: a 12 m unit of 48,000 BCM takes 2,400 and 20 hours, a 6 m unit 1,800; the trace names Rates row 2 | exact |
| `a_lookup_no_row_gives_is_said_or_takes_its_default` (zd-script) | a 20 m bench said with the table and keys, or 0 with `default=0`; a misspelt column and a table not looked up said; a key the rows don't filter on no hindrance (D11 at 900) | exact |
| `p_at_reads_the_own_tables_for_the_keys_given` (zd-script) | the first own table's 2,000 before the second's 2,400 | exact |
| `a_formula_step_looks_up_a_table_by_the_unit_s_own_values` (formula_steps.rs) | an ash factor by seam thickness: B1's U (4 m) 10 × 1.1 = 11, its L (3 m) 20 × 0.9 = 18; the dataflow keeps the table's id; the run names revision 1 and the audit `Factors r1 (looked up)`; revised, 15 | 1e-9 |
| `a_lookup_is_bound_by_name_or_refused` (formula_steps.rs) | a table the project hasn't refuses the dataflow; a seam no row gives stops its columns, said; a calculated column keeps the table's id; a table script by seam gives U 2.5 km and L 4 | exact |
| `zd-app --bench --dataflow` (demo pit) | inside `lookup("` the demo's parameter tables are offered | exact |

## Built: workstream 2, curves

- **`interpolate("Yield curve", unit.ash, x="ash", y="yield", seam=unit.seam, ends="hold")`** (`zd-script`'s
  `lookups.rs`, the maths in `ParameterTable::interpolate`): the rows fitting the keys that give both columns, in order
  of `x`, the `y` worked linearly between the two either side; on a row, that row's `y`. Outside the rows it's refused
  (`Yield curve: ash 25 is outside the curve's 10 to 20`), or with `ends="hold"` the nearer end's `y`. A value that isn't
  a number, a column the table hasn't, a column of words, and a curve no row of which fits are said. A traced unit shows
  `interpolate Yield curve rows 2 to 1: yield at ash 15 (seam U)` and the value.
- **Bound as lookups are**: `interpolate("…")` names a table as `lookup("…")` does (`zd_core::lookups_in`), so it's kept
  by id on the dataflow or column and recorded by revision.
- **One row a point, checked when saved** (`ParameterTable::check_curve`, `zd_core::curves_in`, `check_curves`,
  `curve_readers`): two rows with the same filters at one `x` are refused when a dataflow, calculated column or table
  script reading the table as a curve is saved, and when the table itself is saved while anything reads it as a curve,
  naming what reads it. The same `x` under different filters is two curves, and fine. A table is known as a curve by
  what reads it: a parameter table has no mark of its own.
- **The editors** offer the project's parameter tables inside `interpolate("`, the table's columns inside `x="` and
  `y="`, and `x=`, `y=`, `ends=` and its keys after a comma. The reference gains "Reading a curve".

| Test | Known answer | Tolerance |
|---|---|---|
| `a_curve_is_read_linearly_between_its_rows` (zd-core) | U: ash 10 % at 80 %, 20 % at 60 %, given out of order: ash 15 reads 70 between rows 2 and 1; 25 refused naming 10 to 20, held 60; 5 held 80; 10 is row 2's 80; L's own curve at 25 reads 55 | 1e-12 |
| `two_rows_at_one_point_are_refused` (zd-core) | two U rows at ash 10 refused by the check and when read; L at ash 10 under its own filter fine | exact |
| `curves_and_lookups_are_read_off_the_source` (zd-core) | the tables and the curve's `x` read off a formula, a comment and `my_lookup` passed over | exact |
| `a_formula_step_reads_a_curve` (formula_steps.rs) | ash 5 % at 90 %, 15 % at 70 %: B1's U (ash 10) reads 80; its L (ash 20) held at 70, or its block stopped and said; the table refused with two rows at ash 5 while Reserves reads it, naming Reserves; a dataflow reading such a table as a curve refused | 1e-9 |

## Known limits

- A table named by a name worked out as the script runs (`lookup(name, …)`) isn't bound when it's saved, so it isn't
  found: name it in quotes.
- `p.at` reads the keys given only, not the block's attributes as `p` does: give every key its rows filter on.
- A lookup's row isn't yet listed in the notebook's settings beside a built-in step's; it's in the formula's trace.
- A curve's `x` written as a name worked out as the script runs (`x=column`) isn't checked when saved, only when read.
- A curve is linear between its rows; no smoothing.
