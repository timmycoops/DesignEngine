# 0055 — Standing checks on solids, and the stepped weld made cheap on surfaces

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: the Solids plan tab's order agreed, then "Yes, start").
Workstream 7 of the Solids plan. After 0052 (a solid welded only as far as it needs) and 0053 (no edge under a
micrometre from a cut).

## Why

Both faults fixed on 10 November (0052, 0053) would have shown in a test that sent a solid out and read it back, and
neither had a test that drove the real code: 0053's known answers were meshes written by hand. And 0052's check against
Deswik's own stored volumes had not been run again, as the drawings weren't to hand.

## Decision

1. **Every solid a command makes in the tests goes out as DXF and comes back the same solid.** A shared test helper
   writes each solid in a session as loose 3DFACEs and imports it as read: it must come back a solid with no weld and
   no repair, the same triangles, and its volume to a billionth of itself. It runs after the cut by a surface, a grid
   and a level, the slice by faces, and the solid between surfaces. A command that makes solids, when it's added, gets
   the same line.
2. **A real cut is driven to make a nanometre edge.** A design with one corner half a nanometre inside a cube's wall
   makes the boolean leave corners a nanometre apart. The test cuts it through the command: the run says corners were
   joined, no part has an edge under a micrometre, below + above equal the cube, and each part agrees with the same cut
   made with the corner on the wall exactly.
3. **The stepped weld looks before it repairs.** Running the Deswik check showed 0052 had made a large drawing twice as
   slow: an open topography of 18 million triangles was repaired with its corners as written, then at each step up to
   the ceiling, looking for a solid that was never there. Now:
   - a quick look says whether the mesh could be closed with its corners as written (`closed_as_written`: every edge
     used an even number of times, in one pass with no sorting), and only then is it repaired that way;
   - a mesh that isn't held as a solid where it came from (a Deswik drawing's meshes in Auto) is tried as written and
     then at the ceiling only, with no steps between. Files given to `solid.import`, Spry blocks and `solid.repair`
     keep every step;
   - a closed mesh refused as read with its corners as written isn't tried as read again at the ceiling when the
     ceiling joins no more corners.

## Known answers

- The cube and its two parts, twelve cubes and their parts cut by a grid, the pit and its sixteen strips, and the pit
  made between two surfaces with its strips: each reads back from its own DXF as it was.
- The cut by a design a nanometre inside the wall: corners joined and said, no edge under a micrometre, 1,000 m³ to
  1e-6 m³ between the parts, and each part within 1e-6 m³ of the cut with the corner on the wall (joining corners half
  a nanometre apart over tens of square metres moves about 1e-8 m³).
- A cube written as loose triangles is closed as written; with a triangle off, or a corner a micrometre out, it isn't.

## Deswik's own volumes

PIT A's pit designs drawing (`Pit_Designs_v1.dcf`, 93,070 meshes), `deswik.import` with its defaults, release build,
48 threads. Each solid against the volume Deswik stores for it:

| | Before 0052 | After 0052, as merged | With this |
|---|---|---|---|
| Solids | 92,559 | 92,798 | 92,798 |
| With Deswik's volume | 90,501 | 90,736 | 90,736 |
| Differ from it by more than 1e-6 | 256 | 0 | 0 |
| By more than 1e-3 | 1 | 0 | 0 |
| Sum of the differences | 28.1 m³ | 0.000 m³ | 0.000 m³ |
| Kept as surfaces, not a solid | 70 | 56 | 56 |
| Making the solids and surfaces | 50 s | 101 s | 68 s |

Every difference from Deswik was on a mesh our weld had changed, as decision 0010 said at the time; with corners as
written there is none. The golden pit's 42,223 relimiting solids import the same with this as with 0052 as merged, file
by file: the same solids, no volume differing, 9.1 s as read and 9.6 s with repair.

## What this leaves

- **The drawing still takes 18 s longer than before 0052.** Not traced further. 239 more of its meshes are solids now,
  5.4 million triangles more, and a solid costs more to check than a surface; the two 18-million-triangle topographies
  take 22 and 26 s each and set the time the run takes.
- **A Deswik mesh in Auto that is closed only by a weld between a micrometre and the ceiling** is welded at the ceiling,
  as it was before 0052.
- **The round trip isn't run on every solid of every test**, only where the helper is called.
- **The pit shell drawing** (`240209_FP Design Pit Shell v11.dcf`, 1.9 GB) was not run again.
