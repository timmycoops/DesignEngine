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
