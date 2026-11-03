# 0040 — Fixing what an import couldn't make a solid

Date: 2026-11-03 · Status: accepted (Tim, 2026-11-03: "throw up a warning and ask to fix those as a default option, the
invalids only", then "Replace it" for what a fix does with the surface).

## Why

A Deswik or Spry import makes a closed mesh a solid only if it passes the solid checks; one that crosses itself or has
edges of three faces is kept as a surface and named in the run record (0036, 0039). On PIT B's life of mine that is 219
of 138,000 meshes. Tim liked that they are kept apart, and wants to be asked to fix them, and only them, rather than
tick repair for the whole import or find them by hand in a list of names.

## Decision

1. **A closed mesh an import keeps as a surface carries `not_a_solid`,** the reason it was refused as a solid, by both
   `deswik.import` and `spry.import`. An open mesh kept as a surface, as asked, isn't flagged.
2. **A new command, `solid.repair`,** takes surfaces and runs the repair of 0013 on each: fins off, cracks joined, holes
   filled up to `fill_limit` (10 % by default), and a mesh that crosses itself, or is tangled with edges of three or more
   faces, rebuilt by the union to the solid it encloses, accepted only if it passes the check along rays.
3. **A fixed solid replaces its surface in the same run:** the surface is deleted and the solid made with its name,
   layer and attributes, less `not_a_solid`, with the repair's note and volume change. Commands otherwise leave their
   inputs alone; here Tim asked for the replacement, so a layer never holds a block twice, and one undo brings the
   surfaces back. A run where nothing can be fixed is refused.
4. **What repair can't fix stays a surface,** its `not_a_solid` revised to say why now, and is listed in the run record.
5. **The repaired solid is checked against the source's own volume** (`spry_volume_m3` or `deswik_volume_m3`) and any
   more than a millionth apart is listed, largest first.
6. **In the app,** a Deswik or Spry import that flags any mesh opens a warning: how many, counted by reason (cross
   themselves, edges of three or more faces, holes), the first few by name with why, and **Fix them** as the default
   button beside **Leave as surfaces**. Fixing runs `solid.repair` on those alone.

The import's own **Repair solids** tick stays, for repairing every mesh as it comes in.

## Results

Known answers: a Spry block of two 10 m cubes overlapping by 5 m each way imports as a surface flagged "crosses itself";
`solid.repair` replaces it with a solid of 1,875 m³ (to 1e-6 m³; 2,000 as read), its name, layer, levels and Spry's
volume kept, `repair_volume_change_m3` -125 m³, and no line for Spry's volume, which agrees; undo brings the surface
back with its flag. An open 100 m sheet given with it stays a surface, flagged as over the 10 % fill limit; a surface
named twice is taken once; a run with nothing to fix, a solid given as a surface, and no surfaces are refused.

PIT B's projects read whole, then the flagged meshes repaired (opt-in, `ZD_SPRY_FILE`), release build, 48 threads:

| Project | Flagged | Triangles in | Repair | Fixed | Volume change | Still not solids |
|---|---|---|---|---|---|---|
| 90-day plan | 6 | 45,914 | 0.36 s | 5 (790,448 m³) | 0.000 m³ | 1, edges of three faces |
| Life of mine | 219 | 123,726 | 1.9 s | 131 (201,732 m³) | −3,037 m³ by the union | 88 |

On the life of mine, 79 of the 131 differ from Spry's volume by more than a millionth, the largest −0.34 % on blocks of
about 100 m³. The 88 left have edges of three or more faces that are not closed pieces touching (likely an inner wall),
or cross themselves where the union's cut couldn't be closed; two of those stopped spade's triangulator, which is caught
as that mesh's refusal. They stay surfaces, each saying why.

## What this leaves

- **Edges of three faces the union can't rebuild.** An inner wall between two halves of a block might be dropped and the
  halves joined, or split into two solids; which one is Tim's call, block by block.
