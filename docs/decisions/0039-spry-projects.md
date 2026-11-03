# 0039 — Reading Spry projects (`.spry`)

Date: 2026-11-03 · Status: accepted (Tim, 2026-11-03: "Yes, build it", which took the plan, the change to the exchange
formats and the two crates below).

## Why

Spry is where the schedule lives, and its block solids are the volumes the schedule runs on. Tim asked to bring Spry's
solids, strings and attributes in as the Deswik reader (0036) brings in `.dcf` drawings, so a block designed here can be
set beside the block Spry holds, by name, with Spry's own volume to check against.

## Decision

1. **Spry `.spry` projects are read, never written,** as Deswik's and Vulcan's formats are. A project is opened read-only
   and immutable, so SQLite neither writes to it nor locks it while Spry has it open.
2. **What a project holds is listed from its tables alone,** without unpacking any geometry: each Solid field with how
   many nodes have a solid, and each design layer with its size as stored. Only what is picked is read.
3. **A block solid is named by its node path,** its positions down the table's levels joined by `/` (`Pit/Strip/Block/Bench`),
   and goes on a project layer named after its Solid field: the table, the field's folders and the field, joined by `\`
   (`Deposit\Coal\Solid`). It carries its position on each level under the level's name (`Pit`, `Strip`, `Block`,
   `Seam`, `Bench`) and the values of every field in its Solid field's folder under the field's name, spaces either side
   trimmed. A design layer's objects go on a layer named by its folders and name, joined by `\`, and are named by the
   layer's last level and a count, as Deswik's are, with their Spry attributes.
4. **Spry's volume of a block is kept as `spry_volume_m3`,** from the field named by `volume_field` (`Volume` by default,
   matched whatever its case) in the Solid field's folder, and a solid whose volume differs from it by more than a millionth
   is reported in the run record, the largest in m³ first. A folder with no such field, and blocks with no value in it,
   are counted in the run record.
5. **A mesh goes through the repair step (0010, cavities facing in)** and then, by default, becomes a solid if closed after
   the weld and a surface if open, by the same steps as `deswik.import` (`meshes`, `repair`, `fill_limit`,
   `weld_tolerance`). A closed mesh refused as a solid is kept as a surface and named with why.
6. **Spry's own faces are passed over.** A node's solid stores Spry's faces (polygons of three to dozens of corners, not
   flat) beside its triangles; the triangles are the mesh. A node whose solid has faces and no triangles (Spry's flat plot
   polygons) is passed over and counted.
7. **New crates: `rusqlite` (with SQLite built in) and `lz4_flex` (pure Rust),** agreed by Tim. A new command,
   `spry.import`; no change to the object model or the command interface.
8. In the app, **Spry** on the toolbar picks a `.spry` and opens the layer picker of 0036, listing Solid fields (with their
   solid counts) and design layers to filter and tick.

## The layout

There is no public description. This is what Spry writes, worked out from Tim's projects (client data: kept local, never
committed), at database versions 168 and 175.

**Geometry blobs.** A magic byte of 10 or 11, then `72 79 b8`; four bytes not read; the unpacked size as a little-endian
`u32`; then one LZ4 block. A long stream (a layer's solids) is split into rows of 100 MB unpacked, numbered by their
`Volume` column, and a solid can run across the split, so rows are joined in order before they are read.

| Table | Holds |
|---|---|
| `SpryProperties` | `DatabaseVersion` |
| `Table_`, `Schema` | the tables (Deposit, Dump, Calendar) and their field schemas |
| `Field`, `FieldFolder` | fields by name and `DataType`, in folders (`ContainerType`, `Container_id`) up to a `Schema` |
| `Level`, `Positions`, `Position`, `Node` | a table's levels, the positions on each, and the node tree (`Parent_id`) |
| `NodeData` | one blob a node for each Solid field (`Column_Id` = the field): the node's solid |
| `FieldNodeData` | one blob a field: a node count, the nodes, then their values in that order |
| `Layer`, `LayerFolder` | design layers in folders; `Layer.Shapes` the strings, `Layer.Texts` text |
| `LayerTriangles` | a layer's solids |
| `LayerAttributes`, `Attribute` | attributes of a layer's strings and solids, by handle; attribute names and types |

| Blob | Read as |
|---|---|
| A node's solid | `i32` vertex count, x, y, z `f64` each; `i32` face count, each an `i32` corner count and the corners; `i32` triangle count, three `i32` corners each; then 92 bytes (an `f64` 1.0 and zeros in every one seen), not read |
| A layer's solids | back to back: `i32` vertex and triangle counts, the vertices, the triangles, then 24 bytes starting with the solid's `i32` handle |
| A layer's strings | `i32` count; each an `i32` vertex count, the vertices, then 30 bytes: the `i32` handle, and a 1 at byte 20 where the string is closed |
| A layer's attributes | records of an `i32` handle, an `i32` count, the attribute ids, then their values: text as a .NET string (UTF-8 length in 7-bit groups), `Double` as `f64` |
| A field's values | `Double` and `Percentage` `f64` (a percentage as a share), `Boolean` one byte, `DateTime` .NET ticks, `String` an `i32` length and UTF-8; points and solids are passed over and counted |

Coordinates come through exactly as stored. Text on layers, triangulations (`TriangulationMeshData`), grids, schedules
and process maps are not read yet.

## Results

Known answers: a blob packs and unpacks byte for byte with either magic byte, and a wrong magic, a wrong size and a blob
cut short are refused; a node's 100 x 50 x 10 m box, with Spry's faces beside its triangles, reads its vertices bit for
bit at 50,000 m³ (to 1e-6 m³), and a triangle past the vertices or a solid cut short is refused; a column reads doubles
(a NaN is no value), text and a .NET date (2025-11-24 20:23:35); a project written as Spry lays one out lists its Solid
field and layers, names a block `PIT A/12/B3` with its Pit, Strip and Block, its folder's fields and Spry's volume,
passes over a flat plot polygon, reads a 30 m tetrahedron on a layer at 4,500 m³ (to 1e-6 m³) and a closed and an open
string with their attributes; a layer's solids split into rows of 100 bytes, mid-vertex, read whole; a text file, an
SQLite file that isn't Spry's, a missing file, a name the project doesn't hold and a layer whose strings run past their
count are refused and say so. Through the registry: two boxed blocks at 50,000 m³ (one with Spry's volume a thousandth
short, reported at +0.1001 %), a box with a 4.5 m³ cavity at 49,995.5 m³, an open sheet as a surface and a closed toe
with its attributes, each on its Spry layer, in one undo step; one layer for all, strings only, and a volume field the
folder lacks.

Tim's projects (opt-in, `ZD_SPRY_FILE`), release build, 48 threads:

| Project | Version | Size | Listed | Read | Block solids | Layer solids | Strings | Triangles |
|---|---|---|---|---|---|---|---|---|
| PIT A base model | 168 | 81 MB | 0.3 s | 0.3 s | 7,138 | 7,124 | 56 | 3.5 M |
| PIT B 90-day plan | 175 | 1.8 GB | 0.4 s | 3.1 s | 42,482 | 0 | 2 | 5.0 M |
| PIT B life of mine | 175 | 2.8 GB | 3.5 s | 4.8 s | 110,470 | 27,725 | 33,617 | 52.9 M |

The 90-day plan's 43,455 and the life of mine's 48,576 plot polygons were passed over.

- **Every block that is closed as read matches Spry's volume to a billionth**, where Spry holds the solid's volume: all
  7,123 on PIT A and all 107,356 on the life of mine. On the 90-day plan's relimiting solids 2,854 of 2,873 coal and
  8,968 of 9,092 waste do; of the rest, 11 coal and 111 waste are over 10 % apart, where Spry's `Volume` is not the
  solid's (1 m³ against a block of 25,445 m³, or 1,950 m³ against 152,896 m³). That checks every vertex, triangle and
  winding read.
- **Through `spry.import`,** the differences left are the repair step's 1 mm weld:

  | Project | Time | Objects | Solids | Surfaces | Differ by > 1e-6 | In all | Closed but kept as surfaces |
  |---|---|---|---|---|---|---|---|
  | PIT A base model | 5.0 s | 14,318 | 14,261 | 1 | 3 | −0.656 m³ (largest −0.008 %) | 0 |
  | PIT B 90-day plan | 13.1 s | 42,484 | 42,175 | 307 | 181 | +975,499 m³ | 6 |
  | PIT B life of mine | 43.7 s | 171,812 | 136,571 | 1,624 | 2,500 | −140.7 m³ of 2.27 billion | 219 |

  The 90-day plan's total is the blocks whose stored volume isn't the solid's, above. The meshes kept as surfaces cross
  themselves or have edges of three faces, and are named with where. The life of mine's import is 6.6 s reading and
  36.9 s of repair, solid checks and crossing search over 51.8 M triangles.

## What this leaves

- **Spry's volumes against its own solids.** On PIT B most blocks agree with the `Volume` beside them to a thousandth,
  but some differ by far more and some waste blocks hold 1 m³; which figure Spry schedules on (`Volume`, `Adjusted
  Volume`, a relimited solid) is Tim's to say.
- **Triangulations and grids** from a project, for topography and the seam surfaces Spry was given.
- **Schedule tasks and periods**, to colour blocks by when they are mined.
