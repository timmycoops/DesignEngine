# 0073 — Small volumes to working sections: the dataflow's first four steps

Status: agreed 15 November 2026. Tim said "Agree all nine" to the decisions on the Build Plan's "Working sections plan"
tab, all as recommended, including Spry's order of the seam rules (decision 6), which changes CLAUDE.md's. This is
workstream 3 of 0070, run in the runner of 0072, and is built as three PRs: 3a (small volumes and initialise), 3b (in
situ) and 3c (working sections).

## Why

Spry's Import to ROM (template 1.1.4) does these four steps in an order and grouping we keep. Some of its rules are
hidden or hard-coded, and those we write out instead:

- small volumes vanish;
- the walk that forms sections is a built-in no one can read;
- the underburden is 10 m, hard-coded;
- quality limits are exclusive, so a value on a limit is replaced;
- discarded coal can come back as a parting;
- a missing parameter is skipped, asserted or crashes, depending on the step.

## Decision

1. **Small volumes are set aside, not dropped.** A unit under `min_volume` (no minimum by default) is labelled a small
   volume. It stays in the column's balance, is written at a `small_volume` write point and is left out of everything
   after.
2. **Thickness and area are the solid's, checked and not changed.** `max_thickness` and `max_area` only warn, naming the
   blocks.
3. **Quality limits and defaults** are parameter columns beside each quality and the density: `<field>_min`,
   `<field>_max` and `<field>_default`.
   - Limits are inclusive.
   - Coal outside its limits, or with no value, takes the default.
   - Waste takes its default outright, as it has no grids.
   - Each replacement is counted by field and said.
4. **Underburden is a setting, outside the model.** Where a column's last unit is coal, underburden is added beneath it:
   - it is waste, `underburden_thickness` thick (10 m where no row gives one, 0 for none), over the column's largest
     area;
   - its values come from its parameter rows as a waste block's would;
   - it is never written and never in the column's balance to its blocks;
   - where real waste lies below, none is added.
5. **Moisture and Preston-Sanders** (3b).
   - Mad and Mis come from grids, else from parameter rows. Mrom comes from parameter rows.
   - Dis = RD × (1 − Mad) ÷ (1 + RD × (Mis − Mad) − Mis).
   - Qualities are stored air dried and put on another basis at the write.
   - Waste RD is in situ as given.
   - A coal unit with no Mad or Mis stops its block.
6. **The seam rules in Spry's order** (3c). In situ quality limits come first, per ply. Then come the minimum separable
   parting and the product-to-waste ratio as a section builds, and the minimum mining thickness (partings in) last. The
   ROM ash cut-off comes after dilution (0070, workstream 4). CLAUDE.md now says so.
7. **The walk, written out** (3c).
   - A parting thicker than `min_parting` breaks the column, and so does discarded coal.
   - A parting and the ply below it join a section if that ply's coal × `ws_strip_ratio` covers the parting, bcm for
     bcm.
   - A section counts only if its thickness, partings in, exceeds `min_coal_thickness`.
   - Forced splits by `ws_split` put the parting with the ply above.
8. **Steps carry settings**: the names of the parameter columns and fields a step reads, where a site's differ from the
   usual ones.
9. **A starter dataflow** from the seam model, so no dataflow is built field by field.

## Built: 3a, small volumes and initialise

- **Step settings** (`DataflowStep::settings`, `setting(key)`): a step's parameter column by its usual name unless the
  step names another, as `{"min_volume": "smallest"}`.
- **The runner** gives each step a column with:
  - each input unit's block attributes, which match its parameter rows (a joined unit takes its top bench's, as Spry);
  - the parameter tables;
  - the dataflow;
  - what it has said.
  Those notes are summed over the columns: `reserves.small_volume`, `reserves.underburden` and `reserves.defaulted`
  (counts and volumes) and `reserves.out_of_range` (the blocks named).
- **`small_volumes`**, the first step, does decision 1. The in situ write leaves the set-aside units out.
- **`initialise`** sorts as before, then checks size (decision 2), adds underburden (decision 4, a unit whose origin is
  `out of model`, its volume kept out of the balance) and gives defaults (decision 3, the underburden taking waste's,
  as Spry).
- **`dataflow_starter`**, also `Session::dataflow_starter` and Python's `Session.dataflow_starter`, builds:
  - the fields volume, thickness, area, the model's density and every other grid of its working section;
  - every step built;
  - a parameter table with filters material and seam, every column the steps read, a waste row, a coal row and a
    default row holding the 10 m.
  It's saved as two ordinary runs, `parameters.save` then `dataflow.save`, because one run can't make an object that
  points at another it makes.

Known answers, from `crates/zd-commands/tests/reserves_run.rs` (two blocks):

- **Small volumes.** Waste under 6,000 m³, in a column the step's setting names `smallest`, sets each block's U-L
  (5,000 m³) aside. It's written as a small volume and never in situ, and all 180,000 m³ is written once.
- **Underburden.**
  - Under L it is 10 m × 5,000 m² = 50,000 m³ a block, at the waste row's RD of 2.3, and never written.
  - The column still balances to its blocks to 0.
  - With real waste below L in B1, it's added in B2 only.
- **Ash limits** of 5 to 10 % with defaults of 12 % for coal and 60 % for waste:
  - L's 20 % takes 12;
  - U's 10 %, on the limit, stays;
  - the four waste units and the two underburden units take 60.
- **Size check.** Coal thicker than 3.5 m names 3_B1_U.

On the demo pit (`testdata/synthetic/workflow_demo.py`), the dataflow is now the starter's, with 2.3 typed into its
waste row. Both cases' in situ tables still come to 76,003,157.442 m³ and 169,304,790.747 t, 0 from their blocks.
The run said:

- 1,430 waste units took their RD default, 80,756,207.6 m³;
- 90 columns got underburden, 11,214,358.4 m³, out of model;
- 1,340 waste rows have no ash or sulphur.

The run took 9 ms for 90 columns.

## Built: 3b, in situ

- **A moisture field kind** (`FieldKind::Moisture`, in %). Quality limits and defaults (decision 3) take moistures too.
- **`in_situ` works each unit's densities, then writes**, as before.
  - Coal: the in situ density by Preston-Sanders from its RD, Mad and Mis. Its ROM density is that × (1 − Mis) ÷
    (1 − Mrom) where it has an Mrom.
  - Waste: its RD is in situ as given, and its ROM density the same unless it has moistures.
  - Coal with no RD, Mad or Mis stops its block, said with what's missing.
  - A dataflow with no `rd_is` field is written on its RD's own basis, as before.
  - The step's settings name the fields where a site's differ: `rd`, `mad`, `mis`, `mrom`, `rd_is`, `rd_rom`.
  - `preston_sanders(rd, mad, mis)` is public, with moistures as fractions.
- **The results table.** Where the dataflow works an in situ density:
  - `tonnes` is on the in situ basis (JORC's), with `tonnes_ad` (volume × RD) beside it;
  - each quality also comes on the in situ basis, `<quality>_is` = air dried × (1 − Mis) ÷ (1 − Mad), added up by in
    situ tonnes.
  Qualities stay air dried in the column, as Spry stores them. Raw qualities are still averaged by volume × RD, as
  CLAUDE.md has it; Spry's air-dried tonnes on the in situ volume differ only by a factor in each ply's moistures, left
  for PIT B's comparison to show.
- **The starter** adds `mad` and `mis` (from a grid of that code where the model has one, else a parameter row), `mrom`
  (a parameter row), and `rd_is` and `rd_rom` (worked). Each moisture is averaged by the tonnes on its own basis. The
  parameter table gains `mad`, `mis`, `mrom` and their limits.

Known answers (`crates/zd-commands/tests/reserves_run.rs`):

- **Preston-Sanders by hand.** RD 1.45, Mad 3 %, Mis 8 % gives 1.4065 ÷ 0.9925 = 1.4171.
- **The two blocks, at Mad 3 %, Mis 8 %, Mrom 10 %.**
  - B1's U (RD 1.4) has an in situ density of 1.358 ÷ 0.99, so 20,000 m³ is 27,434.3 t in situ against 28,000 t air
    dried.
  - Its ash of 10 % air dried is 10 × 0.92 ÷ 0.97 in situ, and its ROM density is the in situ × 0.92 ÷ 0.90.
  - B2's U benches each use their own RD.
  - OB, 50,000 m³ at 2.3, is 115,000 t either way.
- **Missing moisture.** U with no Mis stops both blocks: "3_B1_U is coal with no mis".

On the demo pit, the starter's coal row is given Mad 3.5 %, Mis 8 % and Mrom 10 %:

- Base is 169,112,828.499 t in situ against 169,304,790.747 t air dried.
- Both match their blocks to 0 t, with coal by Preston-Sanders on each block's interrogated RD.
- 50m is 0.107 t different in situ, as Preston-Sanders isn't linear in RD and the blocks average it differently. Its
  air-dried tonnes are the same.

## Built: 3c, working sections

`working_sections` runs after `join_benches`. It applies decisions 6 and 7, in Spry's order:

1. **Coal is discarded** by hand (`discard` yes) or by its in situ ash over `insitu_ash_cutoff`. It is carried as waste
   and always breaks a section. The step's `ash` setting names the field, and the starter sets it to the model's ash
   code.
2. **A run of waste between plies** breaks the column when it is thicker than `min_parting`. The run's own setting is
   used, else the larger of the two plies'.
3. **Top down within each group**, the next parting and the ply below it join a section if that ply's coal volume ×
   `ws_strip_ratio` covers the parting's volume, bcm for bcm. The ratio is the largest of the section's plies' and the
   ply's own. Otherwise the section closes, and the next one starts at that ply.
4. **A section counts** if its thickness, partings in, exceeds `min_coal_thickness`, the largest of its plies'. The coal
   of a section that doesn't is thin loss, set aside, and its partings stay waste.
5. **A section splits** where its plies' `ws_split` names change, and each parting goes with the ply above.

Then the labels and the write:

- Each section's units carry its working section label: its number top down, counting the waste between sections, coal
  or waste, and its name (roof seam, or roof-floor). Thin coal is set aside, so the waste either side of it is one run,
  as Spry puts thin loss in its waste.
- A parting in a section is parting dilution.
- The preview shows each unit's section.
- A coal ply no row gives a rule stops its block, said with the rule.
- The `sections` write gives a row for each section (its units added up by their rules, partings in), then each thin and
  each discarded ply.

The starter adds `min_parting`, `ws_strip_ratio`, `min_coal_thickness`, `insitu_ash_cutoff`, `discard` and `ws_split`
to its parameter table.

Known answers (`crates/zd-commands/tests/reserves_run.rs`): one 5,000 m² block, with the rules min parting 1 m, ratio
0.5, minimum 0.45 m and in situ ash cut-off 30 %.

- **A-B.** A (2 m) and B (1.5 m) form section A-B, 3.8 m and 19,000 m³. The 0.3 m parting A-B joins because B's
  7,500 m³ × 0.5 covers its 1,500 m³, and it becomes parting dilution.
- **B-C.** At 5 m it breaks.
- **C.** C (0.4 m) is alone and becomes thin loss.
- **D.** D's 40 % ash discards it, a break.
- **E and F.** E (2 m) is a section. The 0.9 m parting E-F stays waste, as F's 2,500 m³ × 0.5 doesn't cover 4,500 m³.
  So F (0.5 m) is a section of its own.
- **The numbering**, top down, is waste 1, A-B 2, waste 3 (B-C to D-E, around thin C), E 4, waste 5, F 6 and the waste
  below 7.
- **A forced split** (A1, B1) gives A at 11,500 m³ with the parting, and B at 7,500 m³.
- **A missing rule.** With no `min_coal_thickness`, the block stops: "1_B1_A has no min_coal_thickness".

The demo pit sets coal's rules to min parting 0.5 m, ratio 1, minimum 0.3 m and cut-off 35 %:

| case | sections | L | M | U | thin loss |
|---|---|---|---|---|---|
| Base | 256 | 90 (3,682,315.617 m³) | 90 (1,956,673.751 m³) | 76 (820,516.419 m³) | 7 (1,802.5 m³) |
| 50m | 509 | 180 (3,682,315.617 m³) | 180 (1,956,673.751 m³) | 149 (820,067.356 m³) | 15 (2,251.6 m³) |

- L and M are the same in both cases.
- U's subcrop leaves 449 m³ more as thin loss in 50m's smaller blocks, as block size changes where a thinning seam is
  cut.
- The in situ tables still match their blocks to 0.
- The run took 9 ms.

Still to come: PIT B's working sections against Spry's, by counts and differences only (0070, workstream 6).
