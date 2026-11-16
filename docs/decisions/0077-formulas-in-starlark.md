# 0077 — Formulas in Starlark

Status: agreed 16 November 2026. Tim agreed the eight decisions on the Build Plan's "Formulas plan (ws7)" tab ("Agree
all 8, go ahead with 7a"). This is workstream 7 of 0070. It replaces 0070's decision 10 on formulas ("a small language of
our own, built last"). Workstream 7a, the script engine, is built here; 7b to 7e follow.

## Why

Tim, 16 November: "ensuring we are basically ai native here - id want a powerful scripting language but users should
be able to copy and paste the ideas and get help with ai to recreate the scripting logic instead of just learning it
verbatim and the tool helps people check outputs and inputs to give users the ability to know what's happening when
they aren't rust experts. Also have serious scripting power." Until now every formula of the reserves dataflow was in
the engine, so a site changing one needed a new release, and could only read the method in the code.

Read for its method only (Tim's OK, 16 November), Spry's Import to ROM dataflow is 34 structural tasks (sorting,
splitting and relabelling units, the working-section walk, cutting loss and dilution, putting benches back), which our
built-in steps already do, and 13 formula tasks: per-unit arithmetic or a test a site might change. Its Product step is
empty: it has no yield or product.

## Decision

1. **Starlark is the scripting language**: the `starlark` crate (Meta's), pinned to 0.14.2 with its derive crate.
   Python's syntax, which AI models write best; no files, network, clock, randomness or imports; the same answer every
   run; a public specification with implementations in Rust, Go and Java.
2. **The split**: what moves material between units stays built in Rust; the formula pieces become starter scripts with
   known-answer tests, shipped with the starter dataflow and edited like any other.
3. **What a formula reaches**: its unit's fields (setting only fields the dataflow declares), its labels (read only),
   its block's attributes, its parameters, read-only add-ups of its working section and column, and the unit above. It
   never moves material or writes another unit.
4. **Formula steps sit at fixed stages** between the built-in steps, in order within a stage, each with a filter in the
   bench rules' words.
5. **The starter scripts follow Spry's method** where it differs from ours: waste in situ density by the moisture
   factor, coal air-dried tonnes through the in situ density, qualities added up by air-dried tonnes (Mis by in situ,
   Mrom by ROM), the full moisture alignment of dilution, and a first-or-last-ply switch for loss thickness. Each change
   to the demo pit's totals is listed when 7c lands.
6. **AI help**: Copy for AI and Explain copy names and script text, never a block's or a client's values. An assistant
   inside the app that talks to a cloud model waits for Tim's policy on client data.
7. **Added from Spry's method**: waste under the last coal as underburden with the out-of-model netting (built in);
   inverse Preston-Sanders, RD from tonnes, thickness or area filled in, and a section's geometry (scripts); clean coal
   at its section's average (a setting). ROM solids, flipped materials, severities and audit fields come later.
8. **CodeMirror 6** is the formula editor in the app.

The engine never sees Starlark: it hands `zd-script` a column's units and gets back what each formula set. A faster
engine (column-at-a-time, a compiled tier, WebAssembly plugins) can sit behind the same call later, without users
rewriting their formulas.

## Built: 7a, the script engine

`crates/zd-script`:

- **A formula** is a Starlark file defining `def formula(unit, p):` (`p` only where it reads parameters).
  - It reads `unit.ash_ad`, `unit.seam`, `unit.section.volume`, `unit.column.volume`, `unit.block.strip`,
    `unit.above.area` and `p.rom_ash_cutoff`.
  - It sets `unit.rd_is = …`.
  - A field set earlier in the run reads back as set.
  - A declared field the unit has no value for reads `None`.
  - A block attribute or section value that isn't there reads `None`.
- **Parameters**: a parameter no row gives stops the block (0070) with "no parameter row gives `x` for this unit".
  `getattr(p, "x", default)` reads one that may be missing.
- **The library**, in Rust and tested once, all moistures in %:
  - `in_situ_rd` (Preston-Sanders) and `air_dried_rd` (its inverse);
  - `moisture_factor` and `to_basis`;
  - `weighted`;
  - `abs`, `round`, `sqrt` and `clamp`.
- **Checked when compiled**, every problem with its line. Each refuses the formula:
  - a parse error;
  - a name never given a value (Starlark finds it when the file loads);
  - no `formula` function;
  - `unit.x` that isn't a field or label (with "did you mean");
  - a label set;
  - `p.x` that isn't a parameter column;
  - a parameter set;
  - a section or column add-up of a field that isn't a number.

  Comments and quoted text are never taken for names.
- **Run** on a column's units top down (`Formula::run`), or many columns in parallel (`Formula::run_many`).
  - Each column gets a heap of its own, freed when it's done.
  - A value of the wrong kind stops the column, said with the unit and the line: words in a number field, or a
    division by zero.
  - What a formula prints is kept with the unit.
- **Traced** on one unit (`Formula::trace`):
  - the lines that ran, in order, each with the values it gave its names (a line not reached isn't there);
  - every value read and field set;
  - what it printed;
  - where it stopped, if it did.

  The trace uses the evaluator's statement hook. The crate marks that hook as not yet stable, which is why the version
  is pinned exactly.

Known answers (`cargo test -p zd-script`):

- Preston-Sanders on 1.45, 3.5 % and 8 % is 139.925 ÷ 98.525 to 1e-12. Its inverse gives the RD back to 1e-12 over 45
  coals. The moisture factor is 92 ÷ 96.5.
- A made-up column of two coal plies and a waste, worked by hand:
  - the in situ densities and tonnes to 1e-12;
  - the 40 % ash ply discarded against a 35 % cut-off, the 12 % one not;
  - the waste left alone;
  - shares of the section (1,000 ÷ 1,500) and of the column for a unit with no section (300 ÷ 1,800).
- Each refusal above is said with its line. `open`, `load` and `import` are refused.
- The trace of the discarded ply:
  - lines 3 and 5 to 10 run, the early return not taken;
  - line 5 gives `rd` its in situ density;
  - the parameter read is 35 and the field set is `discard = "ash"`.

  The kept ply never reaches lines 9 and 10.
- 64 columns run in parallel give exactly what they give one at a time.

Timing, release build: a million made-up units, 20 to a column, each column its own values, through the in situ formula
(eight reads, a Preston-Sanders call, two or three sets) take 187 ms on 48 threads, 1.17 µs a unit on one. The demo pit's
50m case is 2,847 leaves.

**Starlark is patched** (`vendor/starlark`, used through `[patch.crates-io]`). The published crate asks for
`serde_json`'s `arbitrary_precision` feature, and Cargo turns a dependency's features on for the whole build. That
feature breaks serde's untagged enums and flattened structs holding numbers: 12 places in our types (attribute and
parameter values, face steps, dataflow fields). CI caught it as a note's coordinates failing to read back. Tim chose the
patch on 16 November over changing our types or the language. The copy leaves the feature off and drops Starlark's own
tests; nothing else changes. The only loss is `json.encode` of integers wider than 64 bits, which come out as floats.
`serde_json_reads_numbers_as_plain_floats` fails if the feature is ever back. `vendor/starlark/ZOVLYN.md` says how to
move to a newer Starlark.

## Built: 7b, formula steps in the dataflow

- **A dataflow step may carry a formula** (`DataflowStep.formula`):
  - `when`, a filter in the bench rules' words, matched against the unit's labels (`seam`, `material`, `bench`,
    `became`) and its block's attributes;
  - `script`, the Starlark;
  - `tests`, known answers: a unit's fields, block and parameters, and the fields the formula must leave it with, to
    1e-9 relative.

  Its name is its own, never a built-in step's, and it takes no settings: a formula reads what it needs itself.
- **Where it sits**: after `initialise` (before in situ), `working_sections`, `rom_dilution` or `rom` (before the final
  writes), with other formula steps between. Anywhere else is refused, with where it sits.
- **Checked when saved**: `dataflow.save` compiles each formula against the dataflow's fields, the unit's labels and its
  parameter tables' columns, and runs its tests. Any problem, said with the step and the line, refuses the save; the
  save says how many formulas it checked.
- **Run in place**: `reserves.run` compiles each formula once and runs it on each column, columns in parallel as before.
  - Each unit the filter fits is handed over top down: its fields (a missing value reads `None`) and labels, its block's
    attributes, its parameters from the dataflow's tables as the built-in steps read them, its working section's units
    and the column added up, and the unit above.
  - What it sets is written back.
  - A formula that stops on a unit stops its block, said with the step, the line and the solid; the other blocks go on.
  - Setting the volume, the volume of record, stops the block: moving material is a built-in step's work.
  - What it prints is said, counted by step with the units' volume (`reserves.formula_said`).
  - The run names formula steps among the steps it ran, and the preview shows the column after each.
- **The balance after every run**: a column whose volume out isn't its volume in, to 1e-9 of it, is now a warning
  naming the block and by how much (`reserves.unbalanced`). Before, the run's first line only gave the worst.

Known answers (`crates/zd-commands/tests/formula_steps.rs`, on the made-up strip of `reserves_run.rs`):

- `ash2 = ash × 2 + the ash cut-off` after initialise, on U's coal only, gives 55 on B1's U and both of B2's benches,
  none on L.
- A formula reading the block's strip and the unit above gives 31 for coal under waste and 30 for B2's lower bench
  under its upper. Its print is said as 4 units and 60,000 m³.
- Each refusal on save, said with its line: a misspelt field (with "did you mean"), a step before any built-in or after
  `in_situ`, a built-in's name, and a test whose answer is 55, not 56.
- Reading a parameter no row gives stops B1 alone, said with the step, line 2 and the solid; setting the volume stops
  its blocks.

The workspace's tests are unchanged, with no column out of balance. The app check (`--dataflow`) on the demo pit passes
21 of 21. The app is 40 MB with Starlark in it, 35 MB before.

## Built: 7c, part 1, the in situ formulas

The first of the starter scripts: Spry's Initialise and Insitu arithmetic, after `initialise`, replacing the Rust that
worked it.

- **`fill_size`**:
  - a unit with no thickness takes its volume over its plan area; one with no area, its volume over its thickness;
  - one with no RD but air-dried tonnes (a `tonnes_ad` field) takes the tonnes over the volume;
  - a measured value is never changed.
- **`limits`**: each field with a `<field>_default` parameter takes it where it has none, is outside `_min` to `_max`
  (inclusive), or is forced by a minimum equal to its maximum. Waste takes its grade defaults outright. This replaces
  `initialise`'s Rust, rule for rule. What it changes is said as before, now as what the formula printed:
  `2 units printed by limits: ash: coal outside its limits given its default, 30000.0 m3`.
- **`densities`**, Spry's way (point 5):
  - coal's in situ density by Preston and Sanders, which it must have (`fail("coal with no mis")` stops its block, with
    the line);
  - waste's as its RD over the air-dried-to-in-situ factor where it has both moistures, else its RD;
  - ROM densities as before;
  - `rd_adt`, the air-dried tonnes a cubic metre: the in situ density × (100 − Mis) ÷ (100 − Mad).

  This replaces `in_situ`'s Rust, which now only writes. A coal unit reaching it with no in situ density stops its block,
  said.
- **The starter dataflow** (`dataflow_starter`, `with_starter_formulas`):
  - gets these after `initialise`, written for its own field names;
  - gets `densities` only where it has the moistures;
  - gets an `rd_adt` field, which starts as the block's RD (so Imported, written before any step, is as read) and which
    its qualities and Mad are weighed by;
  - reports air-dried tonnes by `rd_adt`.

  Each formula carries its known answers: Preston-Sanders at RD 1.4, Mad 3 and Mis 8 is 135.8 ÷ 99; waste at RD 2.3,
  Mad 2 and Mis 6 is 2.3 × 98 ÷ 94; a 20 % ash outside 5 to 10 takes 12.
- **Formulas name a field as they run**: `unit["ash"]` reads and sets as `unit.ash` does (Starlark has no `setattr`), so
  `limits` works for any field. `fail("…")` is said in the formula's own words.

What moved on the demo pit (regenerated 16 November against its 15 November tables, by leaf):

| | Base | 50m |
|---|---|---|
| In situ volume and tonnes | unchanged | unchanged |
| In situ coal, air-dried tonnes | 9,358,537.7 to 8,739,118.5 t (−6.62 %) | the same |
| ROM coal volume and tonnes | unchanged | −4,503 m³ (−0.072 %), −7,051 t (−0.076 %) |
| ROM coal ash, sulphur | up to +2.2 %, +0.4 % a leaf | the same, and one section over the cut-off |
| High ROM ash loss | unchanged | 9,586.3 to 14,643.7 m³: one more section (3 leaves) over the cut-off |
| ROM coal, air-dried tonnes | −6.29 % | −6.36 % |

All of it is decision 5 and nothing else. The air-dried tonnes come through the in situ density now, about 6.6 % fewer,
so coal weighs a little less against waste dilution, and diluted ash rises. In the 50m case that tips one section over
its ROM ash cut-off. The demo script's own check, written-against-blocks to 1e-9, holds with air-dried tonnes defined
Spry's way. `reserves.run` on Base takes 58 ms with the formulas.

Known answers: the starter unit tests (the formulas compile and give their answers, with and without moistures, and
aren't added twice). The reserves-run tests are updated where the method changed:

- air-dried tonnes 20,000 × 1.358 ÷ 0.99 × 92 ÷ 97;
- the limits notes as printed;
- the step lists with the formulas;
- a missing Mis stopped at `densities` line 13.

The app check (`--dataflow`) passes 21 of 21 on the regenerated demo.

## Built: 7c, part 2, the ROM side

The rest of Spry's formula pieces, and what decision 7 added. Tim, approving it: "we will expect weight averaging based
on the tonnes for qualities as well". Every quality composite here is by tonnes on the quality's own basis, as before.

- **`discard`**, a new stage after `join_benches`, so each seam is tested whole, as the Rust did. It marks coal in a
  `discarded` field the starter adds: 1 by hand (`discard` yes), 2 by its ash over `insitu_ash_cutoff`, 0 kept.
  `working_sections` sets aside what's marked and no longer tests anything itself. Any other mark a site's own formula
  sets is discarded "by formula".
- **`rom_qualities`**, after `rom`, which builds nothing in now and stays as the stage. On what came into a ROM coal
  section that isn't its coal, against the section's coal added up by tonnes (`unit.rom_coal`, new to the script
  engine):
  - each moisture it has none of takes the coal's;
  - where its own row says `align_dilution_mad` yes, its Mad takes the coal's, Spry's way: qualities × (100 − Mad coal)
    ÷ (100 − Mad own), its RD ÷ the same, so its in situ density holds. The Rust read the section's first ply's row and
    left the RD alone;
  - a negative quality takes the coal's.

  Then each unit in a ROM coal section takes the section's length (its coal's largest: Spry's section geometry), and
  every unit's ROM density and air-dried tonnes a cubic metre are worked again. A dataflow with dip or width fields can
  add them the same way; the starter has neither.
- **Clean coal at its section's average**: where the first ply's row says `clean_coal_average` yes (a new starter
  column), each ply's clean coal carries its section's coal average, by tonnes (Spry's Aggregate WS Clean Coal). It
  still does where `aggregate_sections` is yes.
- **Waste below the last coal isn't mined** (built in, `final`): model waste under a column's last coal isn't ROM waste.
  It's underburden, netted off `Summary\Out of model` (Spry's Final Calculations): the underburden taken into ROM coal
  less this waste. Coal lost into it stays ROM waste, coal lost. It's said as underburden.
- **Reserved words**: a field named one (Vulcan's ash, `as`) is written `unit["as"]` in the starter formulas. The demo
  found this.

Known answers:

- The starter's worked examples:
  - discard: by hand 1; 40 % over 30 % is 2; 12 % is 0;
  - dilution of in situ density 2.3 takes its coal's 3, 8 and 10 %: ROM density 2.3 × 92 ÷ 90, air dried 2.3 × 92 ÷ 97;
  - Mad 2 aligned to 3: 80 % ash × 97 ÷ 98;
  - a negative ash takes the coal's 12 %.
- A new test, clean coal by tonnes: A (RD 1.3, 10 %) and B (RD 1.6, 20 %) carry 14.8 %, not the 14.29 % of a volume
  average.
- The loss column's 20,000 m³ below its last coal:
  - with dilution, ROM waste drops 19,500 m³ and `Summary\Out of model` is −19,500 m³;
  - with floor loss, B's 1,000 m³ stays ROM waste on that leaf.

  ROM coal and waste less the netting are the blocks' 120,000 m³ either way.
- Reserved words written in brackets compile and give their answers.

On the demo pit, against part 1, by leaf:

| | Base | 50m |
|---|---|---|
| ROM coal volume and tonnes | unchanged | unchanged |
| ROM coal ash, sulphur | up to −1.5 %, −0.3 % a leaf | the same |
| ROM coal air-dried tonnes | −0.244 % | −0.244 % |
| Out of model air-dried tonnes | −4.663 % (92 ÷ 96.5) | the same |
| ROM coal length | the section's, 100 m | the same |

Dilution now takes its coal's moistures before its air-dried tonnes are worked again: its air-dried tonnes fall by
92 ÷ 96.5, so it weighs less in the coal's ash. The demo has no waste below its last coal and no Mad alignment, so
neither moves it. `reserves.run` on Base takes 64 ms. The app check passes 21 of 21 on the regenerated demo.

## Built: 7d, the formula panel

In the Parameters tab's dataflow view, until the dataflow canvas (5c) gives steps a panel of their own:

- **The steps list** marks formula steps (`{}`). A click opens one in the formula panel on the right, in place of the
  fields. **Add a formula after…** puts a new one after a stage (after any already there), starting from a few lines
  of template.
- **The formula panel**:
  - its name and its filter, typed in the bench rules' words (`material: coal; seam: U, M`);
  - its Starlark in **a script box**: CodeMirror 6, a kit part (`ScriptEditor`), with Python's syntax, line numbers and
    undo, its colours from the theme's tokens.
- **Checked as it's typed** (`formula_check`): every problem underlined on its line, and listed below with the line
  number. Each worked example is held (green) or not (red, with why). The dataflow's own check (`dataflow_check`) now
  compiles every formula and runs its examples, as `dataflow.save` does, so Save stays off while a formula is wrong.
- **Check on the picked block** (`formula_trace`):
  - the picked solid's block column is run through the dataflow as edited, unsaved, up to the formula, with the blocks
    on its layer and the project's seam model;
  - the formula is then traced on the solid's unit. The lines that ran are tinted in the script box and listed with
    the values each gave, with what it read, what it set, what it printed, and where it stopped, if it did;
  - a unit outside the filter is said;
  - the unit's values before the formula are a click away;
  - nothing is kept.
- **Worked examples** are shown as JSON and edited there, each run on every save.
- **Copy for AI** puts on the clipboard: how formulas are written (`zd-script`'s reference, also under "How formulas
  are written" in the panel), the dataflow's field, label and parameter names with their units, this formula, and a
  request to fill in. **Explain** puts the same with a request to explain it in plain words. Both carry names only,
  never a value: not a block's, not a parameter's, not a worked example's.
- **Saved** with the dataflow (Save, Ctrl+Enter), as its next revision; Discard puts it back.

In the engine:

- `check_formula` and `formula_trace` (`zd-commands`);
- `prepare_flow`, which runs a dataflow given as a value, saved or not;
- `begin_column` and `formula_input`, which the run and the trace share;
- the trace notes a line resumed after a call it made only once.

CodeMirror's packages are pinned exactly, each released at least two weeks before: `@codemirror/lint` at 6.9.7, as
6.9.8 was three days old.

Known answers:

- **The engine.** A formula as edited is reported problem by problem: a misspelt field (line 2), a label set and a
  misspelt parameter (both line 3), with no tests run until it compiles. A worked example expecting 56 fails with
  "ash2 is 55, not 56". Traced on 3_B1_U: line 2 ran, it read `unit.ash` 10 and `p.ash_cutoff` 35, and it set
  `ash2 = 55`; 3_B1_L is outside the filter.
- **The app's text helpers** (`app/tests/formulaText.test.mjs`): a filter typed and read back. Copy for AI carries the
  fields with their units, the parameter columns, the stage and the script, and never a worked example's values.
- **The app check** (`--dataflow`, on the regenerated demo, 26 of 26):
  - the dataflow lists its five starter formulas;
  - `densities` opens checked with its three worked examples holding;
  - Check on a solid of 3_B1 tints the 11 lines that ran, matching those listed, and sets `rd_is`, `rd_rom` and
    `rd_adt`;
  - `unit.rd_si` typed for `unit.rd_is` is shown as "line 21: `unit.rd_si` isn't a field of this dataflow (did you mean
    `rd_is`?)", underlined, with Save off;
  - Discard puts the formula back.

  With `--pictures` it saves `formula-panel.png`.

Not yet: the reference for AI written up with worked examples and tried on two starter formulas (7e).
