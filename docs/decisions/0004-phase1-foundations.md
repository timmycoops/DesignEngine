# 0004 — Phase 1 foundations

Date: 2026-10-28 · Status: locked (agreed by Tim). Full plan: the "Phase 1 plan" tab of the build plan doc.

## Decisions

- **Edits are immutable object revisions.** An edit command writes a new revision of the same object
  (same id); earlier revisions stay in the project's history, and undo and redo step through them via
  the run log. This keeps engine rule 4 (commands never modify inputs) without minting a new object for
  every edit. During an interactive drag the vertex moves as a preview; letting go commits one command.
- **Object ids are 128-bit, generated in-house** (time plus a counter). No `uuid` crate.
- **A project is a folder, `<name>.zdproj/`:** one Parquet file per object type (OMF2 mesh layout, f64
  vertices), a JSON manifest and the run log. Easy to open from Python, and already git-ignored.
- **Tooling:** the `arrow` and `parquet` crates (the locked Parquet/Arrow layer), PyO3 for the bindings,
  and `maturin` to build the Python package.

## Still open

- Import formats beyond DXF, CSV and XYZ. Each one is added only when the golden pit needs it.
- The seam grid source (Minex or Vulcan exports; roof and floor per seam, or a reference surface plus
  thickness), so the seam model builder reads it directly.

## Unchanged

Phase 1 starts once the Phase 0 gate passes on real pit data (the roadmap's rule: no phase starts until
the previous gate passes).
