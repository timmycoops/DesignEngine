# 0052 — A solid is welded only as far as it needs

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: "Yes plan it", then "go" on the plan). Amends decision 0010's
weld (1 mm, always) for solids; surfaces keep it.

## Why

A review of the golden pit's 42,223 relimiting solids, as written (corners exactly as read, nothing moved):

- **Every one is closed.** 41,080 have two faces on every edge; 1,143 have edges of three or more faces (closed pieces
  touching along an edge). None has an open edge.
- **Our own weld opened them.** Imported at 1 mm, 301 were "not closed" and 5 crossed themselves. The same files at
  0.1 mm: 88 not closed; at 0.01 mm: 35; at 0.001 mm: 25. These solids carry real detail finer than a millimetre: a
  slab a few tenths of a millimetre thick folds flat under a 1 mm weld, its top and bottom become one triangle written
  twice, one is dropped as a duplicate, and the other is left open.
- **The repair then patched our damage.** 293 of the 301 solids "closed by repair" had "hanging sheets taken off, no
  volume". The sheets were what the weld left of thin closed parts of the solid, and they did hold volume: up to
  2.2 m³ on one solid.
- **Three files looked open as written** only because the repair step dropped a triangle of zero area from each. A
  triangle with its corners in a line still joins its neighbours edge to edge.

The same thing shows on the cut (2026-11-10): 23 of 3,517 parts cut from these solids no longer close after a 1 mm weld.
That is a separate change.

## Decision

1. **A mesh on its way to a solid is tried with its corners as written first.** If that makes a solid as read (closed,
   or closed pieces touching, and not crossing itself), it is kept: no coordinate moved, nothing dropped.
2. **If not, the weld steps up:** 0.001 mm, 0.01 mm, 0.1 mm, and it is taken at the first that makes it a solid as read.
   A step that joins no more corners than the one before is passed over, and if the ceiling joins no more than the
   corners as written, nothing between is tried.
3. **`weld_tolerance` is the ceiling**, 1 mm by default: the most a corner may move. At the ceiling the mesh goes on as
   before, to be refused or, with `repair`, closed (0013). No parameter is added or renamed.
4. **Crossings are judged at the ceiling**, whatever weld was taken, so a fine weld doesn't make touching faces cross.
5. **A triangle of zero area stays where the mesh is closed with it and open without.** On an open mesh it is dropped
   as before. This holds for surfaces too, where it changes nothing: a sheet is open either way.
6. **Each solid says what it took.** The run record's line for it reads "corners as written" or "repair at 0.01 mm";
   a solid whose corners moved carries `weld_mm`; the tally of a run counts solids with no corner moved and at each weld.
7. **Where.** `solid.import`, `solid.import_many`, `solid.repair`, `deswik.import` and `spry.import` (meshes that may be
   solids). `surface.import` and meshes read as surfaces weld at the tolerance, as 0010 says.

## Known answers

- A 10 m cube with a closed wedge beside it 0.3 mm high (0.0018 m³): a 1 mm weld leaves it open; it imports with its
  corners as written at 1,000.0018 m³ to 1e-9 m³, every coordinate as written, no `weld_mm`.
- The cube written as loose triangles with its top 0.004 mm off: closed at 0.01 mm, 28 corners welded, 1,000 m³ to
  1e-6 m³. Written 0.4 mm off: closed at 1 mm. With a ceiling of 0.1 mm it is refused as not closed.
- The three in one run: "1 with no corner moved, 1 at 0.01 mm, 1 at 1 mm".
- A cube whose top is joined across its diagonal by a triangle of no area: closed with it at 1,000 m³ to 1e-9 m³; the
  same triangle in an open sheet is dropped.
- A clean solid imported with `repair` is taken as written and never reaches the closing step.

## The golden pit, file by file against the build before

Release build, 48 threads, the 42,223 relimiting solids.

| | Before | After |
|---|---|---|
| Import as read | 41,917 solids, 306 refused, 13.5 s | 42,217 solids, 6 refused, 8.8 s |
| Import with repair | 42,218 solids, 5 refused, 12.1 s | 42,223 solids, none refused, 9.2 s |
| Carrying a repair note | 301 | 6 |
| Corners moved by the weld | 8,369 | 5 |
| Triangles dropped | 17,720 | 11 |

- **Nothing that imported before is refused**, as read or with repair.
- **As read**, 300 more import (193,580 m³). Of the 41,917 in both, 363 change volume by more than 1e-6 m³, 59 by more
  than 0.001 m³ and none by 1 m³: +3.1 m³ in all, the most +0.34 m³ on a 143 m³ coal solid. Each is detail the weld used
  to collapse.
- **With repair**, 647 change by more than 1e-6 m³, 298 by more than 0.001 m³ and one by more than 1 m³: +31.6 m³ in
  all. The largest is a coal solid that was 0.361 m³ with "1 hanging sheet taken off (2,605.9 m², no volume)" and is
  2.582 m³ as written.
- **The five refused before all import as written.** The three coal solids taken for inner walls are 9.588, 2.325 and
  11.182 m³. The two taken for sheets are closed slabs of 0.0029 and 0.0069 m³.
- **Six still need repair**, all for crossings in the file itself (the 332,000 m³ and 350,000 m³ waste solids among
  them), resolved by the union with no change in volume at three decimals.
- **Thin pairs** go from 1,993 to 4,513: the detail is kept, so it is counted.
- **The whole-pit cut** by the mined-out topography: 42,223 solids into 49,776 parts in 42.1 s (42,218 into 49,764 in
  43.4 s before), the same one solid left uncut and named.

Two Deswik drawings, `deswik.import` with its defaults:

| Drawing | Meshes | Solids before | Solids after | Time before → after |
|---|---|---|---|---|
| SPRY IMPORT BON | 43,181 | 43,138 | 43,178 | 19.6 → 19.2 s |
| Terrace Option 1 | 122,367 | 122,324 | 122,366 | 39.0 → 37.5 s |

The meshes gained were closed as written and came in as surfaces before, opened by the weld. On BON, one mesh kept as
a surface for edges of three faces is now a solid; two still cross themselves.

## What this leaves

- **Neither drawing above stores Deswik's volumes**, so the check against them (0010's table, on PIT A's drawings) was
  not run again. Those drawings aren't in `testdata/golden`.
- **A mesh that crosses itself as written** goes to repair at the ceiling, not at the least weld.
- **Solids kept as written carry detail under a millimetre.** They match their source here, and a package that welds on
  import may open them as ours did. That belongs with an export made for Deswik.
- **The cut** still makes edges under a millimetre.
