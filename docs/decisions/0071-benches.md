# 0071 — Benches: solids cut at bench levels by bench rules

Status: agreed 15 November 2026 (Tim: "this is basically the logic - we may need to alter later on but for now this is
good", the Build Plan's "Bench levels plan" tab, all eight decisions with decisions 1 to 4 as revised to his comments
that day). Workstream 1, the cut, is built here.

## Why

Tim, 15 November: "We would normally add bench levels (rl) to the reserves prior to creating the scheduled quantities".
A schedule mines a block a bench at a time, so the reserves have to be reported by bench. Deswik's block solids rule
cuts by benches (a bench height, a lowest and highest RL, a bench attribute named from the RL); Spry's reserving workflow
cuts blocks and benches in one step and carries Bench as the last level of its table, Pit / Strip / Block / Seam /
Bench.

## Decision

1. **Levels are from an RL, to an RL, every so many metres** (Tim: "set a certain rl from and to with a thickness 0-500
   and every 10m"): `from`, `from + every`, … up to `to`, or the last level under it. Ground below the lowest level or
   above the highest is one piece, `below <RL>` or `above <RL>`.
2. **Benches are cut after the seams and before the blocks** (Tim: "bench everything prior to running blocks"):
   strips, seams, benches, blocks. The cut takes any solids. The cuts don't depend on each other, so every order gives
   the same ground; on the demo pit the order agreed is the faster (below).
3. **What's benched is set by bench rules** (Tim: "we should be able to decide what's benched easily … a filtering idea
   will be important … for each specific mining location and type"). A short table read top down, the first rule that
   fits a solid winning. A rule picks solids by what they carry (`horizon`, `material`, `seam`, `strip`, `block` or any
   attribute): each named attribute's value must be in its comma list of words, numbers or ranges (`1-5`; `U-M` is a
   word, as it isn't two numbers), case aside, blank or `*` being any. It says benched or not, and if benched, its own
   from, to, every and whether a bench is named by its floor or its top. A solid no rule benches is carried over whole
   onto the same layer, so the layer holds everything for the block cut, and the run names them. By default one rule:
   everything, RL 0 to 500 every 10 m, by the floor.
   **Each rule may name its own output layer** (Tim, 15 November: "majority of the time it will be one layer but I can
   see people using it with various outputs"): what it takes, benched or carried over whole, goes there; a rule naming
   none, or a solid no rule takes, goes to the run's layer. In a workflow case a rule's layer takes the case's name as
   the run's does (`9 coal benches 50m`), and the next step takes what the step made, on every layer.
4. **The label is `bench`**, the bench's floor RL as a number (or its top RL if the rule says so): the bench from RL 70
   to 80 is 70 by its bottom, 80 by its top. Names are `{solid} {bench}` from the cut, and block names default to
   `{strip}_{block}_{horizon}_{bench}`, the bench left out (with its separator) where a solid has none.
5. **Slivers:** nothing is dropped by size. Flaps thinner than the flap thickness (1 cm) where a level lies along or
   runs into a nearly flat face (a berm, or a seam's roof or floor meeting a level at a low angle, a knife edge) are taken
   off as the seam cut takes them (0064) and their volume said (`bench.flaps_left_out`).
6. **Interrogation and the report:** each coal bench is interrogated as any coal solid is; the column check adds a
   block and seam's benches before it compares; the report gains totals by bench. (Workstream 3.)
7. **A workflow step:** By benches is a mode of the Cut solids panel and a tool on the canvas; the demo pit gets it
   between the seam cut and the blocks. (Workstream 2.)
8. **The reserves dataflow** (changes 0070's decision 8, "no benches in v1"): a unit becomes a block's horizon and
   bench, with Spry's in situ bench shares to join and put benches back. Built on main once 0070's column engine is
   merged. (Workstream 4.)

## Built: workstream 1, the cut

- **`zd_geom::cut_solid_by_levels`**: a closed solid split by each level that passes through it, bottom up, by
  Manifold's split by a plane (no boolean with a closed surface), each piece with its floor and top level (`None` past
  them). Worked about a whole-metre origin near the solid in plan. Each piece's flaps are taken off by the seam cut's own
  flap remover. `bench_levels(from, to, every)` makes the levels, at most 100,000.
- **`solid.cut_by_benches`** (`crates/zd-commands/src/builtin/benches.rs`): `solids`, `layer`, `rules`
  (`when`, `benched`, `from`, `to`, `every`, `named_by`, `layer`), `flap_thickness`, `name`, `attribute`. Solids are cut in
  parallel, with progress and Cancel. A rule asking for an attribute no solid carries is warned of (likely mistyped), and
  one taking no solid is said (`bench.rule_unused`); a rule whose levels can't be made is refused, by its number.
  Pieces that don't add back to their solid within a millionth are warned of (`bench.cut_differs`). The run says each
  rule and how many solids it took, the volume by bench, and the solids carried over whole.
- `solid.cut_by_polygons`'s default name is now `{strip}_{block}_{horizon}_{bench}`; the workflow report counts bench
  flaps with what was left out.

### Known answers (`crates/zd-geom/tests/benches.rs`, `crates/zd-commands/tests/benches.rs`, to 1 part in a million)

- A block 100 × 60 m from RL 0 to 30 every 10 m: three benches of 60,000 m³, floors 0, 10 and 20; the levels at its
  floor and top don't cut it.
- From RL −5 to 37 against levels 0 to 30: `below 0` 30,000 m³, three of 60,000, `above 30` 42,000 m³. Wholly in one
  bench: one piece, labelled.
- 45° walls both sides, 60 m wide at RL 0 and 20 m at 20: 50,000 and 30,000 m³.
- A floor dipping 1 in 10 under a flat top at 30: the bench from 0 to 10 the wedge above it, 30,000 m³ less its knife
  edge under 1 cm cut out 5 mm further, 0.1 × 0.105² / 2 × 60 = 0.033075 m³, taken off and said; with no flap
  thickness, 30,000 m³.
- A top 4 mm over a level: the 24 m³ sheet a flap; with no flap thickness a piece.
- A strip 1,500 m long cut every metre from 0 to 30: 30 benches of 90,000 m³, 12 triangles in, 360 out, 10 ms in a
  debug build.
- Through the commands: the default rule naming `Strip 3 OB 0`, `10`, `20`, 900,000 m³ each, one undo; the plan's
  example rules (interburden whole, coal on strips 1–5 whole, other coal every 5 m, the rest every 10 m by the top)
  each solid taking its first rule, those left whole carried over and named; `below 0` and `above 30` past the levels;
  benches cut into blocks named `3_B1_OB_0`, `3_B1_OB_10`, and `3_B1_U` for coal left whole; a mistyped attribute
  warned of, a rule taking nothing said, levels running downward refused naming the rule; coal benches, waste benches and
  the interburden kept whole each onto its rule's layer, a rule naming a blank layer onto the run's.

### On the made-up demo pit (`testdata/synthetic/bench_levels.py`)

The workflow demo's pit, 6 strips, 76,003,223 m³, everything benched from RL −300 to 500 every 10 m, in a release build:

| Order | Steps | Time |
|---|---|---|
| Seams, then benches, then blocks | 36 seam pieces in 0.48 s, 143 benches in 0.46 s, 1,815 blocks in 0.38 s | 1.3 s |
| Benches, then seams, then blocks | 81 benches in 0.54 s, 143 seam pieces in 0.98 s, 1,815 blocks in 0.48 s | 2.0 s |
| No benches | 533 blocks from the 36 seam pieces in 0.16 s | |

Both orders give the same 1,815 strip / block / horizon / bench units, the most any differs 0.0007 m³. The blocks come
to 51 m³ (0.7 parts per million) less than the unbenched blocks: the flaps at the levels, said by the run. In Manifold a
level is a plane split, so benching costs about what the seam cut does; benching first is slower because the seam cut
then runs on more solids.

## Built: workstream 2, the app

- **By benches**, a mode of the Cut solids panel between By seams and By polygons. The solids are taken as in every mode
  (picked, all on the layer of the first, or on show). Its Benches section:
  - **Levels** (or **Others** once there are more rules): from, to, every and Bottom or Top, the rule for everything
    else, the last in the table.
  - **Horizons**: a tick for each horizon the solids carry (read with the block table's `solids_table`), in the order the
    seam cut made them. Unticked, a horizon is carried over whole; the ticks keep one rule at the top listing those
    horizons. A horizon another rule decides shows that rule's number and can't be ticked there.
  - **Rules, first that fits**: each rule's filter typed as `horizon: U-M, M-L; strip: 1-5`, Bench or not, its levels,
    its own layer (blank the run's), how many of the solids it takes, and up, down and remove. Under them, how many solids
    are benched and how many carried over whole.
  - **CSV** in and out: a column for each attribute filtered on, then `benched`, `from`, `to`, `every`, `named_by` and
    `layer`; a file with mistakes is refused line by line and changes nothing.
  - **Flaps**, and the names (`{solid} {bench}`) and output layer (`benches`) in Output. The button reads *Bench n
    solids*, Ctrl+Enter, and Add to the workflow as every panel's does.
- The matching is the engine's, written again in `app/src/panels/benchRules.ts` so the panel shows what each rule takes
  before the cut; `app/tests/benchRules.test.mjs` checks it against the same cases as the engine's tests.
- **By benches** is a tool on the workflow canvas. The Cut solids panel's By polygons names default to
  `{strip}_{block}_{horizon}_{bench}`.
- **The demo pit** (`testdata/synthetic/workflow_demo.py`) benches its seam pieces as step 16, RL −300 to 500 every 10 m,
  onto `6 benches`; the layers after it move up one (`7 block lines`, `8 block polygons`, `9 blocks`). Base makes 1,815
  blocks and 50m 3,580, every one with its bench, adding up to the benches within 1e-11 and the benches to the seam pieces
  less 51.054 m³ of flaps; the workflow report reconciles each case within 0.0004 m³ with the flaps counted as left out. 26
  of the 143 benches are in more than one part, where a level crosses a seam twice: each stays one solid, said, as a strip
  or a block in parts does.

## Still to come

Workstream 3 (the column check and the report by bench) and 4 (the column engine's bench labels and in situ shares, after
0070's PR merges).
