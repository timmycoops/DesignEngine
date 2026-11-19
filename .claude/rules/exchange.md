---
paths:
  - "crates/zd-io/**"
  - "crates/zd-py/**"
  - "crates/zd-core/src/project.rs"
  - "crates/zd-commands/src/builtin/{exchange,deswik,spry}.rs"
  - "app/src/panels/{DeswikImportDialog,ExportSolidsDialog}.tsx"
---

# Exchange formats

Moved from CLAUDE.md. The `.zdp` store is in the stack table there (`docs/decisions/0058-one-file-a-project.md`).

- Import first: Vulcan `.00t` triangulations and grids (the formats sites exchange day to day; see
  `docs/decisions/0006-vulcan-grids.md` and `0011-vulcan-triangulations.md`), DXF (3DFACE, polyface and
  strings), CSV strings and XYZ grids; then Surpac `.str`/`.dtm`, Datamine pt/tr ASCII, OBJ.
- Solids come by the thousand as a zip or a folder (Spry exports a zip; Deswik imports one): `solid.import_many`
  takes either in one run. See `docs/decisions/0019-solids-by-the-thousand.md`. They go out the same way: `solid.export_many` writes a
  zip or a folder of one DXF a solid, named for it, with a list of them and their attributes
  (`docs/decisions/0057-solids-out-a-file-each.md`).
- Then: OMF v1 and v2, and an export to Seequent Evo schemas (triangle mesh, design geometry).
- Spry `.spry` projects are read, not written: block solids by node path with Spry's values and volume, design-layer
  strings and solids with their attributes (`docs/decisions/0039-spry-projects.md`).
- Vulcan formats are read, not written. Deswik `.dcf` drawings are read, not written: a zip of VectorDraw XML layers,
  read by the layers picked, with projection details kept on strings and Deswik's volumes on meshes
  (`docs/decisions/0036-deswik-drawings.md`). Exchange back into Deswik through DXF (R12, confirmed reading at the same
  coordinates) or OMF. Spry `.spry` projects (SQLite) come next.

