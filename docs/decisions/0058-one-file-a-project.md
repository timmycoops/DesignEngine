# 0058 — One file a project: `.zdp`, a zip of Parquet tables

Date: 2026-11-11 · Status: accepted (Tim, 2026-11-11: "a single file is going to be required", "happy with the zip
logic like a Deswik file", "lock in the .zdp file format name", "Yep agree" to the targets). Item 3 of the Review
10 Nov order; the plan and its measurements are on the Build Plan's Store plan tab. Replaces the folder layout of 0004
once stage 2 lands.

## Why

The store was built in Phase 1 for tens of objects. A 43,181-object project saved 86,364 files and a 588 MB JSON
manifest, three quarters of it attributes; a save with nothing changed rewrote 593 MB in 8.8 s (4.7 s inside the
engine), and open took 5.5 s, 4.1 s of it parsing attributes. Phase 4 makes hundreds of thousands of blocks, each with
hundreds of attributes. And a project has to move as one file, as a Deswik `.dcf` or a Spry `.spry` does.

Measured on 11 November with a prototype on the 43k and 101k projects: one zip of Parquet tables is 90 MB and 232 MB
(714 MB and 683 MB as folders), and a whole-file rewrite of the larger, flushed to disk, takes 0.31 s. SQLite holding
the same Parquet blocks saves only what changed (0.002 s) but is weaker on network drives and synced folders, and a C
library in the store would stand in the way of a browser build; plain SQLite rows took 334 MB, nine times the space
of the attributes as Parquet.

## Decision

1. **A project is one `.zdp` file**: a zip of Parquet tables with a small JSON manifest, copied and moved like a
   Deswik or Spry file. `.zdproj` folders keep opening, and saving one writes a `.zdp` beside it.
2. **Projects are normally on a local drive**; a network drive works, slower (the whole file goes each way).
3. **Geology stays outside.** Seam grids, and block models when they come, are referenced by location and fingerprint,
   never packed in. Surfaces and grids imported as objects for design are inside the file.
4. **Rules that keep later phases open:** everything keyed by object and revision; a table per kind of record, each
   with its own version, older versions opening for good and a newer one refused by name; what's stored separate from
   the container (zip now, SQLite possible later); meshes in the OMF2 layout; the same project encodes to the same
   bytes; every entry checked against the manifest's size and checksum, a mismatch refusing the file by name.
5. **Targets** (43k project unless said): a save after one change under 0.5 s, in the background; nothing written when
   nothing changed; open under 2 s; one file of about 100 MB; under 1 GB in memory after stage 3; the 101k project
   saved under 1 s; a made-up 200k-solid project opened under 5 s and saved under 2 s.

## Stage 1 (this record's first PR): the tables

`zd_io::zdp`: `encode_project` and `write_project` turn a project into its entries, `read_project` reads them back,
through a `Container` (a `FolderContainer` for now). `project_differences` says where two projects differ, bit for bit.

```text
manifest.json                    format and version, each table's version, layers, hidden objects, looks,
                                 and every other entry with its size and 64-bit FNV-1a checksum
objects.parquet                  one row per live object: id, revision, name, layer, provenance, kind, and where
                                 its body is (string vertex count, mesh group and rows, grid entry and lattice,
                                 crossings, and a seam model or face design as JSON)
attributes/g00001.parquet        object row, name, number | text | flag; 1,000 objects a group
meshes/g00001-vertices.parquet   x, y, z (f64), the group's meshes end to end; 1,000 meshes a group
meshes/g00001-triangles.parquet  a, b, c (u32), counted from each mesh's own first vertex
meshes/<id>-r<rev>-....parquet   a mesh over a million triangles, on its own
grids/<id>-r<rev>.parquet        z (f64, NaN = null)
strings.parquet                  object row, x, y, z
runlog.json                      the run log as the session passes it
```

A mesh or grid two objects share is written once. Seam models and face designs sit in their object's row rather than
the manifest, as planned, so everything about an object is in one place. The run log is one entry for now; stage 2
adds each save's new runs as their own entry.

Checked: every one of the 17 projects saved under `testdata` (the 43k Deswik drawing, the 101k terrace drawing,
PIT A, B and C, the templates) opens through today's reader, goes through the tables and comes back with no
difference. The made-up 200,000-solid project (21.6 M triangles, 50 M attribute values) is 95 MB, encodes in 1.4 s
and reads back in 9.8 s with no difference; the 43k project reads in 3.1 s. Both reads are mostly building attributes
one by one, which stage 3 (attribute names held once) is for.

`bytes` joins zd-io's dependencies: it is Parquet's own buffer type, already in the build through `parquet`, and the
only way Parquet reads from memory.

## Still to come

- **Stage 2, one file:** the zip container; save beside then swap, in the background; a lock note; the app and Python
  open and save `.zdp`; CLAUDE.md's "Data at rest" line.
- **Stage 3:** attribute names held once in memory.
- **Stage 4:** links that survive a move (both locations tried, Relink by folder, fingerprints checked).
