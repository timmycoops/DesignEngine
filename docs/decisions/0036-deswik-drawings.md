# 0036 — Reading Deswik drawings (`.dcf`)

Date: 2026-11-03 · Status: accepted (Tim, 2026-11-03: "Yes, Deswik first", which also took the change to the exchange
formats and the two crates below). Awaits a vertex-for-vertex check against Deswik's DXF export of the same layers.

## Why

Tim asked to import a Deswik file easily and pull examples of walls, ramps and berms out of it, with Spry data to follow.
His Deswik files open: a `.dcf` is a zip of plain XML, and every string Deswik projects records how it was made. That
gives real designs to set beside ours, and Deswik's own volumes to check against. Spry (`.spry`, SQLite with
compressed geometry) comes after this, by the plan Tim agreed.

## Decision

1. **Deswik `.dcf` drawings are read, never written,** as Vulcan's formats are (0006, 0011). DXF R12 stays the way back
   into Deswik (0009).
2. **The layers of a drawing are listed from the zip's directory alone,** so a file of gigabytes lists at once, and only
   the layers picked are read. They're read across the machine's cores, largest first.
3. **A project layer is named as Deswik shows the layer:** levels joined by `\`, as `3.0 PIT\DESIGN\STRIP 12`. So a
   string's "Projection Surface" names its target's layer as it is here. With `layer`, everything goes on one layer and
   each object keeps its Deswik layer as `deswik_layer`.
4. **Strings keep every attribute Deswik gave them,** projection details among them. Empty values are left out.
5. **A mesh goes through the repair step (0010),** then, by default, becomes a solid if closed after the weld (by the
   steps of `solid.import`) and a surface if open (by those of `surface.import`). A closed mesh refused as a solid is kept
   as a surface and named with why, so nothing is lost. `meshes` can make every mesh a solid (and `repair` close them),
   every mesh a surface, or none (strings only).
6. **Deswik's own volume of a mesh is kept as `deswik_volume_m3`,** and a solid whose volume differs from it by more
   than a millionth is reported in the run record, the largest in m³ first.
7. **A layer whose zip checksum doesn't match what it holds is read anyway if its XML runs whole to its end,** and named
   in the run record. One layer of a real file had a wrong checksum over contents that unzip whole; a layer cut short is
   refused.
8. **New crates: `quick-xml` and `base64`,** both already in the app's build through Tauri. A new parameter type,
   `texts`, carries the list of layers.
9. In the app, **Deswik** on the toolbar picks a `.dcf` and opens a list of its layers to filter and tick.
10. `bench/deswik_examples.py` lists every string with projection settings in a drawing, by layer, with the settings in
    our terms: side, batter, up or down, and to an RL, a distance or a surface. The report goes under `testdata/golden`.

## The layout

There is no public description. This is what Deswik writes, worked out from Tim's files (client data: kept local, never
committed). Their layers were saved by VectorDraw 7.7, 8.8 and 10.1, and one file holds layers of all three.

| Part | Content |
|---|---|
| `<name>.dcf` | the document: the layer table (names with `\` between levels), blocks, settings; `<name>` is the name the file was first saved under |
| `<name>.dcf_<path>` | one layer's entities; the path's levels joined by `^^`, and a `.` in a name written `@@` |
| `<name>.dcf__DW_…` | settings, hidden graphics, annotations |

Each entry is VectorDraw XML: `O` elements for objects and `P` elements for properties, each with its field (`F`), type
(`T`) and value (`V`). A long array is `streamed`: base64 of a little-endian `i32` count, then the items, padded with
zeros to a power-of-two capacity. A short one lists each item as an `_Item`.

| Entity | Read as |
|---|---|
| `dwPolyline` | a string: x, y, z and a bulge for each vertex, in world coordinates whatever its extrusion vector; `Flag` 1 closes it |
| `dwPolyface` | a mesh: x, y, z for each vertex, then five numbers to a face, four 1-based corners (a triangle repeats its first) and a colour, `-1` in every face seen; `VolumeInternal` is Deswik's volume, `-1` where it worked out none |
| `XProperties` | the entity's attributes, typed (`Int32`, `Double`, `Boolean`, text) |
| `dwPoints`, `vdText`, images and others | passed over and counted by type |

Entities inside blocks are definitions and are not read. A bulged segment would become chords within 0.01 m, as in DXF,
but none of 8.9 million vertices in Tim's files has a bulge.

## Results

Known answers: a listed square string and a streamed one read bit for bit with their attributes, escapes undone; a
tetrahedron of 30 m legs, five numbers to a face, is 4,500 m³ and Deswik's volume of it is kept; a four-cornered face
splits in two and a face of two corners drops; a semicircle of bulge 1 becomes chords on its arc within the sagitta,
the RL running straight; a face past the vertices, a face list not five to a face, an array shorter than its count, and
a layer cut short are refused; entities in a block are not read. A `.dcf` written as Deswik lays one out lists its
layers by name and size and reads only those asked for. One with a checksum changed reads the same mesh and names the
layer. Through the registry, a box of four-cornered faces wound either way comes in as a solid of 50,000 m³, an open
square as a surface, and the string with its projection details, each on its Deswik layer, in one undo step.

Tim's files (opt-in, `ZD_DESWIK_DCF`), release build, 48 threads, the file in memory:

| Drawing | Layers | Unzipped | Strings | Meshes | Triangles | Read |
|---|---|---|---|---|---|---|
| PIT A pit shell | 778 | 8.3 GB | 123,831 | 90,167 | 84.2 M | 2.9 s |
| PIT A pit designs | 198 | 9.0 GB | 18,374 | 93,070 | 95.5 M | 3.9 s |
| PIT B reserves (older) | 91 | 0.9 GB | 6,216 | 177 | 5.2 M | 0.7 s |

Read cold from Tim's USB drive the first two take about three minutes, which is the drive.

- **Every closed mesh matches Deswik's volume as read:** 73,883 on the pit shell and 89,909 on the pit designs, none more
  than a millionth apart. That checks every face and its winding. The older PIT B file stores no volumes.
- **Through the repair step, 278 and 256 differ.** Most are under a hundred-thousandth, where the 1 mm weld collapses a
  sliver. The largest, about 1 %, are solids with a cavity: a closed piece inside another, wound inward. The repair step
  turns every closed piece outward, so it adds the cavity where Deswik takes it away. A box of 50,000 m³ with a 4.5 m³
  tetrahedral cavity comes in at 50,004.5 m³ against Deswik's 49,995.5 m³, reported in the run record. That's a fault
  of the repair step, for every format. Fixed by decision 0010's amendment of 2026-11-03 (cavities face in): the box
  comes in at Deswik's 49,995.5 m³, and 278 and 250 differ, every one a mesh the weld changed.
- **The whole pit shell through `deswik.import`:** 214,000 objects (87,372 solids, 2,795 surfaces, 123,831 strings) in
  55 s, the crossing check the most of it. 382 closed meshes kept as surfaces, mostly with edges of three faces or
  crossings.
- **Projection details:** 5,647 strings on 214 layers of the pit shell carry them, and every wording reads: dig walls
  at one batter up and down to five bench RLs, dump walls at another, some "up by" a distance, two "up by" a distance
  "to surface".

## What this leaves

- **Deswik's DXF export of the same layers,** to check strings vertex for vertex and triangles as sets of corners
  (`ZD_DESWIK_DXF`, ready). That needs Tim to export one.
- **A real wall beside Deswik's.** The design layers hold hand-edited fragments, and settings are recorded only on the
  last projection of each piece, so a toe and its result can't be paired by machine. That needs Tim to name one clean
  case: a toe, its profile and Deswik's face.
- **Spry,** next.
