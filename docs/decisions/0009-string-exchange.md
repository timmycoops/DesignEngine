# 0009 — String exchange: DXF and CSV

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29)

## Decision

Workstream 4, slice 1 covers the Phase 1 gate's "export the string to DXF and read it into Deswik at
the same coordinates" and the toe string import. The logged mesh repair step is slice 2.

1. **DXF is written as R12 (AC1009)**: a header with `$EXTMIN` and `$EXTMAX`, a layer table, strings as
   3D POLYLINEs (flag 8, plus 1 when closed) with 3D vertices, and surfaces and solids as 3DFACEs.
   R12 is the most widely read version and the one mining packages have long exchanged. Deswik's
   own export writes AutoCAD 2000 to 2018 (`ExportSimpleDxf.FileVersionConstants`); if Deswik will not
   read R12, the fallback is an R2000 writer (handles, block records, an objects section).
   Coordinates use Rust's shortest round-trip formatting, so every f64 reads back exactly.
2. **Layers carry across.** On export each object goes on a DXF layer named after its project layer.
   On import each string goes on a project layer named after its DXF layer (or its CSV `layer` cell),
   unless one layer is given; strings are named `<layer> 1`, `<layer> 2`, and so on.
3. **What the reader converts, all counted in the run record:**
   - LWPOLYLINE and 2D POLYLINE take their elevation.
   - Arcs (bulges) become chords no farther than 0.01 m from the arc; the original vertices stay
     exactly.
   - Entities drawn in a tilted plane (extrusion other than +Z) move to world coordinates by the DXF
     reference's arbitrary axis algorithm.
   - A LINE becomes a two-vertex string.
   - Other entities (points, text, blocks, MESH, polygon meshes) are passed over and counted by type.
   - Handles, owners, subclass markers, reactors and extended data are read past, so files from
     AutoCAD 2000 and later (and Deswik's exports) read; extended-data points (1010/1020/1030) are
     never taken for vertices.
   - A string with one vertex is passed over; a closed string with two vertices is read as open.
4. **CSV strings**: a header row, columns found by name in any order and case: `x` (`easting`,
   `east`), `y` (`northing`, `north`), `z` (`rl`, `elevation`), and optionally `string`, `name`,
   `layer` and `closed`. A new string starts where `string` changes; without a `string` column a blank
   row ends a string. A string whose last row repeats its first is read as closed and the repeat
   dropped, unless `closed` says no. Export writes `string,name,layer,closed,x,y,z`, with a byte-order
   mark for Excel, and does not repeat a closed string's first vertex.
5. **Export is a command.** `export.dxf` and `export.csv` write a file and change nothing. The run is
   logged with the exact object revisions written, but adds no undo step. Neither replaces a file
   unless told to (the app's save dialog asks first). Grids and seam models are not exported, and CSV
   takes strings only. The command interface gains one field type, `objects` (a list of ids).

`surface.import` also reads polyface meshes (POLYLINE flag 64): positions, then face records of three
or four 1-based vertex numbers (negative for hidden edges); a number past the positions is an error
with its line.

## Commands

| Command | What it does |
|---|---|
| `string.import` | strings from DXF (POLYLINE, LWPOLYLINE, LINE) or CSV, each on its source layer or one given layer |
| `export.dxf` | strings, surfaces and solids to one DXF R12 file, each on a layer named after its project layer |
| `export.csv` | strings to CSV, one row per vertex |

In the app: a Strings import button beside Surface, and an Export menu: the selected object to DXF,
the selected string to CSV, everything visible to DXF, visible strings to CSV.

## Evidence

- A graded toe and its closed crest leave as DXF R12 and come back through `string.import` with the
  same layers, closed flags and every coordinate's bits; the same through CSV with their names. Coordinates with long binary fractions (650000.1, 1/3) survive both.
- The export's run record lists the two revisions written and no outputs; undo after it takes off the
  edit before it. An existing file is refused unless overwrite is set.
- A 10 x 20 x 5 m box solid goes out as 3DFACEs and comes back as a watertight surface with the same
  eight corners, 1,000 m³ to 1e-6 m³.
- A polyface cube reads as 12 triangles, watertight, 1,000 m³.
- A circle drawn as a closed two-vertex LWPOLYLINE with two bulges of 1 reads as chords all 50 m from
  the centre to 1e-9 m, at the polyline's elevation, its area within 0.01 m times the circumference
  below the circle's.
- A 90° ramp arc of radius 20 m reads with every chord vertex 20 m from its centre.
- Extrusion -Z mirrors x and z; an east-facing plane maps its x to north and y to up.
- A polyline written as AutoCAD 2000 writes it (handles, subclass markers, reactors, extended data
  with 1010/1020/1030 points) reads with exactly its three vertices.
