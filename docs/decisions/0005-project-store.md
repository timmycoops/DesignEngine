# 0005 — Project store layout

Date: 2026-10-28 · Status: accepted (refines decision 3 of 0004)

## Decision

A project is a `<name>.zdproj/` folder:

```text
manifest.json                          format "zdproj", version 1, layers, and every live object with its
                                       revision, layer, provenance, attributes and the files holding it
run-log-<save>.json                    the session's run log
strings-<save>.parquet                 all string vertices: object (u32, index into the manifest), x, y, z (f64)
objects/<id>-r<rev>-vertices.parquet   x, y, z (DOUBLE): the OMF2 vertex array (surfaces, solids)
objects/<id>-r<rev>-triangles.parquet  a, b, c (UINT32): the OMF2 triangle array
objects/<id>-r<rev>-grid.parquet       z (DOUBLE, NaN = null), row-major; lattice in the manifest
```

- **Refinement of 0004:** 0004 said one Parquet file per object type. Strings keep that, since they are many and small.
  Surfaces, solids and grids get one file per array per object instead, in OMF2's own array layout. The reasons:
  - A file named by object and revision never changes, so a later save reuses it. A rename or a string edit
    never rewrites a large mesh: an 8 M-triangle surface re-saves in 1 ms instead of 0.6 s.
  - One surface opens without reading the others.
  - OMF2 export (Phase 6) becomes a file copy.
- **Crash safety:** a save writes new files first and swaps the manifest in last (write `manifest.json.tmp`, rename
  over `manifest.json`). A crash leaves the previous save intact. Stale files are deleted after the swap, but only
  files of the shapes above, never anything else in the folder.
- **What is saved:** only each live object's current revision, keeping its id, revision number and provenance,
  so run-log references stay valid. The undo history does not survive closing a project.
- **Validity flags** are recomputed when a project opens, not trusted from disk.
- **Crates:** `parquet` 60 with only the `arrow` and `snap` features, plus `arrow-array` and `arrow-schema`. The
  OMF Rust implementation is not published on crates.io, and OMF is our exchange format, not the store.

## Timings (8 M-triangle surface, release build)

Save 625 ms (129 MB), open 713 ms including the validity flags, incremental re-save 1 ms. For comparison, the same
surface read from 135 MB of XYZ text took 0.56 s to triangulate and encode in Phase 0.
