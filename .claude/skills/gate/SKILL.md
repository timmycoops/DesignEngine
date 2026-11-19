---
name: gate
description: Run the real-pit gate locally - the opt-in golden tests on PIT B and the other local data, then the synthetic workflow demo against its recorded figures - and report counts, totals, faults and timings only.
argument-hint: "[full]"
disable-model-invocation: true
---
The golden data is client data and stays local. **Nothing from it reaches this conversation but test names, pass or
fail, counts, totals and timings**: no panic text, coordinates, object dicts, note or refusal text, and no file
contents. Pipe every golden run through the filter below so only those lines come back.

## 1. Golden tests

The paths come from the `ZD_*` variables in `.claude/settings.local.json` (main checkout, not committed). First list
which of these are set and whether each path exists; a test whose data is missing is a **failure, never a pass**.

Run in release, ignored tests only, each package once, filtered:

```
cargo test --release -p zd-commands --test walls --test strips --test seam --test vulcan_tri --test deswik -- --ignored --exact \
  one_string_on_one_real_grid sweep_one_string_on_real_grids a_whole_face_on_real_grids a_road_on_real_grids \
  ramps_across_real_strips fifteen_strips_on_real_grids a_seam_model_on_real_grids a_real_folder_suggests_and_builds \
  timing_a_real_surface_import a_real_pit_slices_exactly_and_fast timing_a_real_drawing_import \
  2>&1 | grep -E '^test |^test result|^error(\[|:)|^\s+Running '
cargo test --release -p zd-io --test deswik_golden --test vulcan_golden --test zdp --test zdp_file -- --ignored --exact \
  a_real_drawing_reads_whole held_heights_in_a_folder grid_matches_vulcan_export_node_for_node \
  every_seam_with_roof_and_floor_reads every_grid_reads_including_compressed \
  every_saved_project_comes_back_the_same_through_the_tables targets_on_every_saved_project \
  2>&1 | grep -E '^test |^test result|^error(\[|:)|^\s+Running '
```

With `$ARGUMENTS` = `full`, also run `every_solid_in_a_folder` (`--test vulcan_tri`, every solid in `Data/relimits`,
tens of thousands of files: long).

Not run, because their data isn't under `testdata/golden`: the Spry tests, `one_seam_reads_and_agrees_with_itself`,
the Vulcan triangulation against its Deswik DXF, `a_real_drawing_matches_its_dxf_export`, `a_cut_by_real_grids` and
the strip-ramp tests that need `ZD_PROJECT`/`ZD_DESIGN`. List them as not run.

For a failure, give the test name and run it again alone through the same filter; never rerun with `--nocapture`. To
see why, write a script that prints counts and booleans only.

## 2. Workflow demo (synthetic, numbers allowed)

Build the module if needed (`VIRTUAL_ENV="$PWD/.venv" .venv/Scripts/maturin develop --release -m crates/zd-py/Cargo.toml`),
then run `.venv/Scripts/python testdata/synthetic/workflow_demo.py bench/out/gate/demo` (a scratch folder, so Tim's
own demo isn't touched).

Compare what it prints for each case (Base and 50m: blocks, coal blocks, total m3, benches, leaves, working sections,
ROM coal and waste m3 and t) with `bench/gate-figures.json`. If that file doesn't exist, show the figures and ask Tim
before writing it. A figure that moved beyond 1e-6 relative is a finding: name the commits since the figures were
recorded that touched `crates/` (`git log --oneline <recorded commit>..HEAD -- crates`), and ask before updating the
file.

## 3. Report

One table: check, result (pass, fail, not run, missing data), count or total, time (the golden tests' time is each
test binary's `finished in`, since their own output stays captured). Then any figure that moved, with the commits
that could have moved it.
