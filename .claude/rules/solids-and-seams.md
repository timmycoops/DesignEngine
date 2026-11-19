---
paths:
  - "crates/zd-geom/src/{cut,slice,between,blocks,benches,layers,seam,stack,grid,halfspace,close,repair,orient,self_union,ray}.rs"
  - "crates/zd-core/src/{mesh,validity,grid,seam}.rs"
  - "crates/zd-commands/src/builtin/{solid,seam,grid,blocks,benches}.rs"
  - "app/src/panels/{CutPanel,RepairDialog,BenchRulesSection}.tsx"
  - "app/src/panels/benchRules.ts"
---

# Solids, grids and seam models

Moved from CLAUDE.md's object model. Judge seam work on PIT B. Decisions: 0007 (seam model), 0020 (no-seam RL),
0025 (a grid stays in space), 0049 (strip solids), 0052 (weld only as far as needed), 0062 to 0064 (blocks, seam
grids never filled, flaps), 0071 (benches).

- `Solid`: closed manifold mesh; must always be watertight. The large one strips are cut from is made inside a closed
  string between two surfaces or grids (`solid.between_surfaces`), and every cut is one Cut solids panel: into strips by
  faces, by a surface, grid or RL, or between surfaces (`docs/decisions/0049-strip-solids.md`). A block is a named
  polygon in plan, made from lines (lines across the strips offset every 100 m and lines closing them,
  `string.polygons_from_lines`), and a solid is cut vertically where it sits, the piece taking its name as `block`; the
  polygons must cover every solid in plan, checked before the cut and refused with each gap listed, the polygons being
  what to fix; strips
  are cut by the seam model's horizons in one run, each piece carrying `seam`, `material` and `horizon`
  (`docs/decisions/0062-blocks-are-polygons.md`); the grids must reach over every solid in plan with no hole under it,
  checked before the cut and refused with each place listed (`docs/decisions/0063-seam-grids-are-never-filled.md`);
  flaps thinner than 1 cm (where a strip's surface runs along a seam's roof or floor) are taken off each piece and said,
  as Deswik removes flaps (`docs/decisions/0064-berms-on-their-grid-and-flaps.md`). After the seams and before the
  blocks, pieces are cut at bench levels by bench rules (`solid.cut_by_benches`): a table read top down, the first rule a
  piece's attributes fit saying whether it's benched and from what RL to what RL every so many metres, each bench taking
  `bench`, its floor RL by default or its top, onto the rule's own layer or the run's; a piece no rule benches is carried over whole
  (`docs/decisions/0071-benches.md`).
- `Grid`: regular raster (origin, cell size, rows, cols, z values with NaN for nulls). A Vulcan roof, floor or
  topography reads whole: a node flagged off keeps the height it holds, with a warning if it steps far from the
  live nodes (`docs/decisions/0025-a-grid-stays-in-space.md`).
- `SeamModel`: an explicit, ordered stack of horizons (the Deswik "surface stacking" pattern), defined in a CSV
  the user edits; `seam.suggest_definition` writes a starting one from the grids' geometry, never from names.
  Each horizon has a code, material, parent horizon (for splits), working flag, roof and floor grids, quality
  grids and a density grid. The model links to its grid files in place (fingerprinted, never copied into the
  project); a project keeps each grid folder as given and relative to itself, opening tries both, and `seam.relink` points a
  folder at its new place in one step (`docs/decisions/0058-one-file-a-project.md`); grids used for design are imported as objects. A seam exists where its roof is above its floor,
  whatever a file flags, and its surfaces carry on where it has no thickness. The stack is checked, never fixed: a floor above
  its roof, a crossing in the working section or a daughter outside its parent is flagged with its location,
  and blocks solids and reserves until the source grids are corrected. Grids are never filled: a node with no height the
  no-seam RL doesn't account for is a hole, said on the model, and the geology model is fixed and the grids imported again
  (`docs/decisions/0063-seam-grids-are-never-filled.md`). A definition may name an RL that means no
  seam, for models that mark absence that way (`docs/decisions/0020-no-seam-rl.md`). Existence polygons and a crop surface
  (topography or base of weathering) come later. Faulting applies to structure, never to thickness.
  See `docs/decisions/0007-seam-model.md`.
