# 0086 — The reserves table's fields: clean coal under ROM, a balance of coal and waste, ratios and an audit

Status: agreed 18 November 2026. Tim agreed the seven decisions on the Build Plan's "Reserves fields plan" tab ("Agree
all 7"), the balance as he refined it. All three workstreams are built here: 1 (the folders and the summary), 2 (the Reserves
table tab: thickness, area and length shown, the Balance view) and 3 (dip, width and centroid in the starter
dataflow).

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
  piece on the unit's own leaves (`Ran.moves`), except the underburden's: it has no record of its own, so what came in
  from outside is said on the coal it went into, the lowest coal's record for floor dilution (Tim, 22 November 2026:
  "just the lowest record but with ROM waste component. ROM coal is both coal and rock"). Before, it made a leaf with no
  seam that held only the from-outside volume.
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

## Built: workstream 2, the Reserves table tab

- **Thickness, area and length shown** (`reserves_view::add_up_geometry`). They're written with no add-up rule, and the
  tab added every value up by its rule, so they never showed. Now a column of one of them with no rule adds up by
  block, the leaves' words for the levels above the seam (as `Dataflow::column_levels` splits them): a leaf its own;
  thickness added down a block (its benches and seams stacked) and averaged over blocks by each block's largest area;
  area each block's largest, added over blocks; length a block's own, none above it. Thickness is each block's own true
  thickness added up, not volume over area, so a leaf and its parents are on one measure; the decision's own words
  (benches stacked add up, blocks beside each other an area-weighted mean) are what it does.
- **The Balance view** (`ReservesBalanceView.tsx`, `reservesBalance.ts`): Fields or Balance in the tab's header. Balance
  shows the picked node's `Summary\Balance` a row a stage (Imported, Insitu, ROM, Scheduled; coal, waste and total
  across), the moves between them signed as they change each part (small volume set aside, coal to waste, waste to
  coal, from outside, not mined; a move none made is left out), and after each stage its check, green where coal, waste
  and the total each hold within 1e-6 of the imported volume and red where one doesn't. A table made before this says to
  run it again.
- **The Dataflow strip's ROM tag** no longer adds ROM's clean coal, a part of its coal, to ROM's volume again; the tip
  still lists it.

| Test | Known answer |
|---|---|
| `thickness_area_and_length_add_up_by_block` (reserves_view) | a bench its own, 2 m; U in B10, benches 3 m and 2 m stacked, 5 m over 1,000 m²; B10, every seam stacked, 6.5 m over 1,200 m², length 100 m; the strip (6.5 × 1,200 + 4 × 3,000) ÷ 4,200 = 4.714 m over 4,200 m², no length |
| `reservesBalance.test.mjs` (2) | the 4c column's dilution balance a row a stage: to coal +1,760 coal and −1,760 waste, not mined −19,500 waste and total, Imported plus the moves each part's ROM, every check holding; 5 m³ off of 120,000 said; a table without a balance |
| `dataflowView.test.mjs` | ROM 100 + 50 m³ with clean coal 90 m³ in the tip: the tag 150 m³ |
| `zd-app --bench --dataflow` (16 Nov demo) | every block run again: the Reserves table tab's Balance shows Imported, Insitu, ROM and Scheduled with its three checks holding; Fields shows the coal's thickness (11.84 m on the node shown) |

## Built: workstream 3, dip, width and centroid

solid.measure already wrote each block's width, dip (degrees) and centroid onto it; the starter dataflow
(`dataflow_starter`) now reads them as fields beside volume, thickness, area and length:

- `width`, a length, and `dip`, each a column's largest within a run (a seam's benches share one dip). In the table
  they're written with no add-up rule, as thickness is, and the Reserves table tab takes a block's own and, above a
  block, the mean weighted by each block's area (`reserves_view`, as thickness).
- `centroid_x`, `centroid_y` and `centroid_z`, by volume: the centroid of the pieces together.
- Units: dip in degrees (shown °), the centroid in metres. They're with the detail fields (Every field).

A dataflow made before this keeps its fields: add them in the Parameters tab's dataflow (from the block attributes
`width`, `dip`, `centroid_x`, `centroid_y`, `centroid_z`), or make a new starter.

| Test | Known answer |
|---|---|
| `the_starter_reads_each_block_s_width_dip_and_centroid` | width and dip by largest, the centroid by volume, each from the block attribute of its name |
| `a_run_writes_each_block_s_width_dip_and_centroid` (the 4c column, 100 m by 50 m, flat) | OB 50 m wide, dip 0°, centroid at RL 45; A's centroid at RL 39; dip and width with no add-up rule, the centroid by volume; units deg and m |
| `width_and_dip_are_a_block_s_own_area_weighted_above` (reserves_view) | B10 50 m and 4° its own; the strip (50 × 1,200 + 30 × 3,000) ÷ 4,200 = 35.71 m and (4 × 1,200 + 8 × 3,000) ÷ 4,200 = 6.857° |
| `reservesTable.test.mjs` | `deg` shown °; width, dip and centroid detail fields, thickness not |
