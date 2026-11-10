# 0057 — Solids go out a file each, in a zip

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: "Zip, a file each", then "Yes, build it" on the plan).
Workstream 3 of the Solids plan. After 0009 (string and mesh exchange) and 0019 (solids by the thousand).

## Why

`export.dxf` writes every solid given as loose 3DFACEs on a DXF layer named for its project layer, in one file. Fifteen
strips on one layer arrive in Deswik as one pile of triangles with no names. Solids come in by the thousand as a zip of
one file a solid (0019: Spry exports one, Deswik imports one), and Deswik puts each file's mesh on a layer of the
file's name, as its own file showed on 10 November (`Test_Solids.duf`).

The other form weighed was one DXF with a polyface mesh a solid. DXF R12 holds 32,767 corners to a mesh, which the
larger pit solids exceed, and attributes have nowhere to go.

## Decision

1. **`solid.export_many`** takes solids and a path, and writes a zip where the path ends in `.zip`, a folder
   otherwise. It changes nothing in the project.
2. **A DXF R12 file a solid**, as 3DFACEs on a layer named for the solid's project layer: the form Tim validated in
   Deswik on 10 November (0053).
3. **Each file is named for its solid.** What a file name can't hold (`\ / : * ? " < > |`, control characters) becomes
   an underscore, in the file's name only. A name already taken, whatever its case, as Windows takes names, gets a
   number: `Twin (2).dxf`. Nothing is dropped, and the run says how many names were shared.
4. **`solids.csv` goes with them**, a row a solid: its file, its name as it is, its layer, volume, triangles and
   pieces (0054), then a column for every attribute any of the solids carries, in the attributes' order.
5. **Each file is read back as written before it goes in**: with its corners as written and no repair it must come
   back the same solid, its triangles and its volume to a billionth (the check of 0055). One that doesn't is left out
   and named; if none does, the run is refused.
6. **Refused:** no solids given; an object that isn't a solid; a zip or folder already there, unless `overwrite`. In a
   folder, each file replaces one of its name and others are left.
7. **In the app,** the Export menu gains Selected solids to a zip and Visible solids to a zip.

Vulcan `.00t` is not written (CLAUDE.md: Vulcan formats are read, not written). Surfaces and strings don't go out this
way.

## Known answers

- A cube named `Pit/N: 1` cut at RL 105, and a 10 m and a 2 m cube named `twin` and `Twin`: four DXFs
  (`Pit_N_ 1 below RL 105.000.dxf`, its `above`, `twin.dxf`, `Twin (2).dxf`) and `solids.csv` with `cut_by` and
  `cut_side` as columns, the names as they are and volumes of 500, 500, 1,000 and 8.
- A zip holds the same files as a folder, byte for byte, and reads back through `solid.import_many` as four solids
  with no corner moved, each volume to 1e-9 m³.
- Refused: nothing given, a string among the solids, and a zip or a folder already there without `overwrite`.

## On the pit

Release build, 48 threads. The 49,776 parts of the golden pit's relimiting solids cut by the mined-out topography
(8.34 M triangles, 205,809,442 m³):

- **Out:** 29.2 s, a zip of 250.7 MB, every file read back the same solid before it went in.
- **Back in:** `solid.import_many` reads the zip as 49,776 solids in 10.2 s, none refused, no corner moved, each
  within 5e-15 of its volume.

## In Deswik

`bench/out/deswik-check/strips.zip` holds the sixteen strips of the made-up pit (7,500,000 m³, fourteen of 2,500,000 m³,
7,500,000 m³) with their list. Tim imported it on 2026-11-10: "All arrived and all valid". Sixteen layers come in, each
named for its file (`PIT STRIP 1_DXF` to `PIT STRIP 15_DXF` and `PIT PAST STRIP 15_DXF`), each holding its solid on a
layer under it named for the DXF's own layer (`STRIPS`, the project layer), and all sixteen pass Validate Solids.

## What this leaves

- **The zip is written on one core.** Making and checking the files uses every core; packing 2 GB of DXF text into the
  zip doesn't, and is most of the 29 s.
- **Deswik reads the list only as a file beside the solids.** Putting the attributes on the solids in Deswik is done
  there, by joining on the file's name.
- **Names Windows keeps for itself** (`CON`, `NUL`) are not changed.
