# 0086 — The reserves table's fields: clean coal under ROM, a balance of coal and waste, ratios and an audit

Status: agreed 18 November 2026. Tim agreed the seven decisions on the Build Plan's "Reserves fields plan" tab ("Agree
all 7"), the balance as he refined it. Workstream 1 (the folders and the summary) is built here; 2 (the Reserves table
tab: thickness, area and length shown, the Balance view) and 3 (dip, width and centroid in the starter dataflow) are to
come.

## Why

Tim, 18 November: "Lets review the fields in the Reserve Table and ensure we have all the fields required. Id expect to
see Clean coal under the rom section also. Also a summary that compares the volumes (Prime volumes) against each stage
to ensure volume is moving correctly through the system." Then: "The Summary should compare the Import > Insitu > ROM >
Scheduled and make sure coal and waste add up and anything that's been changed from coal - waste is not lost and vice
versa." Spry's field tree (ROM with Waste, Coal and Clean Coal; a Summary of roll-ups by stage and an audit) was a method
reference only.

## Decision

1. Clean coal moves to `ROM\Clean coal`; the top folder goes.
2. A balance in the table, coal and waste apart: `Summary\Balance\Coal`, `\Waste` and `\Total`, each with its volume at
   every stage (imported, small volume, insitu, ROM, scheduled), what moved between them (to waste, to coal, from
   outside, not mined) and each stage's check against the one before, zero for every block.
3. A Balance view in the Reserves table tab (workstream 2).
4. Thickness, area and length shown in the Reserves table tab (workstream 2): a leaf its own; above it, thickness as
   volume over plan area with each block's area counted once, area as each block's largest added over blocks, length per
   block only.
5. Dip, width and centroid in the starter dataflow (workstream 3).
6. ROM and Scheduled waste keep what waste has: no qualities or moistures.
7. `Summary\Ratios` (strip ratio, in situ strip ratio, clean coal yield, as ratios of sums) and `Summary\Audit`.

## Built: workstream 1, the folders and the summary

In `crates/zd-commands/src/builtin/reserves_run.rs`:

- **Clean coal** is written to `ROM\Clean coal`, on the ROM basis as before.
- **The moves.** `final` sets each unit's in situ material against where it ends: coal ending in ROM waste moved to
  waste (losses, thin and high ROM ash coal, discarded coal), waste ending in ROM coal moved to coal (dilution, partings
  in a section), the underburden in ROM coal came from outside, and waste below the last coal isn't mined. Each is a
  piece on the unit's own leaves (`Ran.moves`).
- **The balance** (`balance_columns`): each stage's volume per leaf from the pieces that stage wrote (`Imported`, `Small
  volume`, `Insitu`, `ROM`, `Scheduled`; coal, and every other material as waste), the moves beside them, and three
  checks a part:
  - `unaccounted_insitu` = imported − small volume − insitu;
  - `unaccounted_rom` = insitu − to waste + to coal + from outside − ROM for coal; insitu + to waste − to coal − not
    mined − ROM for waste; insitu + from outside − not mined − ROM for the total;
  - `unaccounted_scheduled` = ROM − scheduled.
  The stage volumes and the moves are worked out apart (the pieces each stage wrote, and the units at the end), so the
  checks are real. Summed down a block they're zero; a leaf can be off where material moves between seams. A dataflow
  with no in situ step writes no Insitu, so ROM is checked against imported less small volume and the in situ columns
  are left out. Volumes, which are conserved; tonnes change with density and basis.
- **Waste trimmed**: ROM and Scheduled waste write no quality, moisture or moisture factor, nor the ROM-basis qualities.
- **Ratios** (`ratio_columns`): `strip_ratio` (ROM waste bcm per ROM coal tonne), `insitu_strip_ratio` and
  `clean_coal_yield` (ROM clean coal tonnes per ROM coal tonne), each with the sums it's made of beside it and a new
  add-up rule, `AddUp::Ratio { of, per }` (`zd-reserves`): the sum of one column over the sum of the other, so it adds
  up right over any group. A dataflow field can't take it (`ColumnError::RatioField`).
- **Audit**: `Summary\Audit\run` (UTC; "run again" for a block run again, 0085), `\dataflow` (name and revision, or "as
  edited") and `\parameters` (each table and its revision).

| Test | Known answer |
|---|---|
| `the_balance_follows_coal_and_waste_through_dilution` (4c column, dilution only) | coal 25,000 m³ and waste 95,000 m³ imported and in situ; to coal 854 + 906 = 1,760 m³; ROM coal 26,760 m³ (the table's 10,854 + 15,906); not mined 19,500 m³; ROM waste 73,740 m³; total ROM 100,500 m³; every check zero. 1e-9 relative |
| `the_balance_follows_coal_lost_to_waste` (roof 0.1 m, floor 0.2 m loss) | to waste 3,000 m³ on both sides, every loss; ROM coal 22,000 m³; ROM waste 95,000 + 3,000 − 20,000 = 78,000 m³; checks zero |
| `the_balance_without_an_in_situ_step_checks_rom_against_what_came_in` | no in situ columns; ROM coal 26,760 m³; checks zero |
| `clean_coal_is_under_rom_and_the_summary_has_ratios_and_an_audit` | `ROM\Clean coal` and no `Clean coal\`; no qualities on ROM or Scheduled waste; the strip ratio over the column its ROM waste over its ROM coal tonnes, the yield its clean coal over its ROM coal tonnes; the audit's run in UTC, "Reserves r1", "assumptions r1" |
| `zd-app --bench --dataflow` (16 Nov demo) | every block run again into the Base table: 90 blocks, each with its checks, the worst 2e-16 of its volume; the balance's ROM coal each block's ROM coal |
