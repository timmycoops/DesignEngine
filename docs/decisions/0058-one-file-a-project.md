# 0058 — One file a project: `.zdp`, a zip of Parquet tables

Date: 2026-11-11 · Status: accepted; stages 1 to 3 built (Tim, 2026-11-11: "a single file is going to be required", "happy with the zip
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

## Stage 2: one file

`ZdpStore` saves and opens a `.zdp` file, and the session, the app and Python use it. A `.zdproj` folder still opens
(today's reader is kept), and saving it writes a `.zdp` of the same name beside it; nothing writes folders any more.

- **One zip, saved beside then swapped in.** Entries go to `<name>.zdp.saving`, flushed to disk, then renamed over
  the file, so a crash or a dropped connection leaves the last save whole; a leftover is written over by the next save.
  Parquet entries are stored as they are, JSON deflated, every entry with the same fixed time: the same project saves
  to the same bytes. Opening reads the whole file, and every entry is checked against the manifest as before.
- **Only what changed is encoded.** An `EncodeCache` keeps each group's entries with what they held: attribute groups
  by the revisions in them and their first row, meshes and grids by the geometry itself (held so its address can't be
  reused). A rename or an attribute edit keeps the mesh; a large mesh renamed keeps its bytes under the new revision's
  name. Opening fills the cache from the file's own entries when its tables are this build's, so the first save after
  an open encodes only what changed.
- **Nothing is written when nothing changed:** no new run in the log, and the tables' content key as it was. A look or
  a hidden object, which aren't logged, still change the key.
- **Saving doesn't hold up the app.** `Session::begin_save` takes a `Snapshot` (the live revisions, layers, hidden
  objects and looks) in an instant; `SaveJob::run` encodes and writes it off the session's lock; `end_save` notes it in
  the run log. The app's Save does exactly that, so the viewport and commands carry on during a large save.
- **A lock note**, `<name>.zdp.lock` beside the file, says who has it open (user, machine, process, since when). Ours
  is written on open or the first save and removed when the session closes; one left by a crash on the same user and
  machine is taken over. Someone else's is reported when the file is opened (the app shows "Project in use") and
  refuses a save to it, naming who and since when, with Save as or deleting their note as the ways on.
- **The app** opens a `.zdp` (Open a project), or an older folder (Open an older project folder), and saves with a
  `.zdp` file dialog. **Python**: `zd.open("pit.zdp")`, and `save` returns the path, whether it was unchanged, the
  entries and how many were encoded, the size and the time.

Not done as planned: free space isn't checked before a save. A save that runs out of room fails, removes its
part-written file and leaves the last save whole, which is what the check was for. The run log is still one entry,
written whole each save (4.6 MB on the 43k project, deflated); it was to be each save's new runs.

Measured on 11 November (release build, local disk; `targets_on_every_saved_project` and `store_scale`):

| Project | File | First save | Open | Save after one change | Save, nothing changed |
|---|---|---|---|---|---|
| Deswik drawing, 43,181 objects (folder: 714 MB, opened in 5.8 s) | 92.0 MB | 0.63 s | 3.3 s | 152 ms | 74 ms, nothing written |
| Terrace drawing, 101,432 objects (folder: 683 MB) | 234.8 MB | 0.70 s | 1.9 s | 343 ms | 185 ms, nothing written |
| PIT B, 15 strips | 82.7 MB | 0.52 s | 0.61 s | 68 ms | 14 ms, nothing written |
| Made-up: 200,000 solids, 50 M attribute values | 95.0 MB | 1.7 s | 14.3 s | 481 ms | 412 ms, nothing written |

Against the targets: saves meet them (one change under 0.5 s, the 101k project under 1 s, the 200k one under 2 s,
nothing written when nothing changed, one file of about 100 MB). **Open doesn't yet**: 3.3 s against 2 s at 43k, and
14.3 s against 5 s at 200k. Both are mostly building every attribute one by one, which is stage 3's job.

## Stage 3: attribute names held once

`zd_core::Attributes` was a map of its own strings on every object: 11 million name strings on the 43k project, and
50 million on the made-up 200k one. It is now a short list of name and value, kept in name order, whose names are
interned (`zd_core::intern`): each held once for the program and pointed to by every object that has it. It reads,
iterates and saves as the map did (`get`, `insert` with a `String`, indexing, `extend`, collecting, the same JSON, the
same order), so commands, the app and Python see no change beyond `keys()` and `iter()` giving `&'static str`. An
interned name is never freed; attribute names are column names, a few hundred to a project, so that's bounded in
practice.

Reading a `.zdp` builds each attribute group across cores straight from the table's arrays, looking each name up once
a group.

Measured on 11 November (release build, local disk):

| | Before stage 3 | After | Target |
|---|---|---|---|
| Open the 43k project (`.zdp`) | 3.3 s | 0.84 s | under 2 s |
| Open the made-up 200k project (`.zdp`) | 14.3 s | 2.1 s | under 5 s |
| Memory once the 43k project is open | 1.7 GB (folder) | 786 MB (`.zdp`), 651 MB (folder) | under 1 GB |
| Save after one change, 43k / 200k | 152 ms / 481 ms | 144 ms / 407 ms | under 0.5 s |

All 17 projects saved under `testdata` still come back through the tables with no difference. Every target of this
decision is now met. The `.zdp` file takes more memory than the folder because a save's cache holds the file's
entries, so the first save after an open encodes only what changed.

## Still to come

- **Stage 4:** links that survive a move: both locations tried, Relink by folder, fingerprints checked.
- **Stage 4:** links that survive a move (both locations tried, Relink by folder, fingerprints checked).
