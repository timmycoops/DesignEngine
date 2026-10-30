# 0019 — Importing solids by the thousand

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31)

## Why

A pit's relimiting solids are tens of thousands of files. Picking them in a file dialog takes too long
on its own: Spry exports them as a zip for that reason, and Deswik imports the zip. In the app, importing
them was slower still, and got slower with every solid, because it was built for a handful of files:

- one run per file, each rebuilding and sending the whole object list;
- one mesh per solid, each fetched on its own and waiting for a rendered frame, which alone is 11 minutes
  at 60 fps for 40,000 solids;
- one thread per solid to build the snap index;
- a row in the page for every object, and every output named in the run log.

## Decision

1. **`solid.import_many`** takes any mix of files, folders (with their subfolders) and zips, and makes one
   solid per `.00t` or `.dxf` file, through the same steps as `solid.import`, across the machine's cores.
   `solid.import` stays as it is for one file. The app imports through the new command, a selection
   included, and a folder can be picked as well as a zip.
2. **One run, one undo.** Over ten files the run record sums up what was done to them all.
3. **A refused file is named with why and the rest carry on.** A panic in the geometry refuses that file,
   not the run. Two files with one name are counted in the record; their solids share it.
4. **A zip is read into memory once**, and nothing is unpacked to disk. Entries at any depth are read;
   other files are counted and left out.
5. **A layer's solids draw as one mesh, and snap as one index.** Layers are already what a user shows and
   hides, so nothing is lost: the viewport has never picked a solid by clicking, only strings. A change to
   any solid reloads its layer's mesh.
6. **The Objects table draws only the rows on screen**, and the run log names a run's first few outputs.
7. **A new dependency:** the `zip` crate, deflate only, agreed by Tim (2026-10-31).

Small meshes also stay on one thread for the crossing search and the adjacency, where starting a thread
for each core cost more than the search. Undo takes a run's objects off in one pass rather than one at a
time. A project's meshes are read across the cores when it opens.

## Evidence

- Four 10 m cubes from a zip and a folder, with an open mesh and a text file among them: 4,000 m³ (to
  1e-6 m³), the open mesh named and refused, the text file left out, and one undo takes all four off. A
  zip that is not one, and a selection with nothing to import, are refused and say so. Two solids on a
  layer join as one mesh of 2,000 m³ under a key that changes when one is deleted and comes back on undo.
- The golden pit's 42,223 relimiting solids against the file by file pass, on a 48-thread machine:

  | From | Repair | Solids | Time |
  |---|---|---|---|
  | Zip (72 MB) | off | 41,917 | 7.7 s |
  | Zip | on | 42,218 | 10.6 s |
  | Folder (276 MB) | off | 41,917 | 8.9 s |
  | Folder | on | 42,218 | 11.9 s |

  The same files are refused (306 with repair off, 5 on), and no solid's volume changes (to 1e-6 m³),
  206,009,846 m³ in all. Undoing the run takes 0.01 s.
- The release app on the same zip with repair on, panels showing (`--bench --solids`): 4,891,688 triangles
  from the zip to drawn with the snap index built in **12.6 s**: 10.5 s for the import and 2.1 s to draw
  and index. Orbiting holds **60 fps**, no frame over 33 ms. Snapping takes 0.9 ms at p50 and 1.7 ms at p95.
  31 table rows are in the page and the last solid is a scroll away. Undo takes 24 ms.
- A project of that pit: **37 s to save** (84,439 files, 210 MB, two files a solid), 5 s to save again
  with nothing changed, and **17 s to open**, down from 335 s reading the files one at a time.
