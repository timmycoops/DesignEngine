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

Not yet: formula steps in the dataflow and `reserves.run` (7b), the starter scripts (7c), the panel (7d) and the
reference for AI (7e).
