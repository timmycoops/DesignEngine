# 0024 — The Face design panel and a whole face

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31)

## Why

Tim: a full face needs "a panel (form whatever we call it) with clear rules to make this repeatable - not
possible to do it from the selection of one string either". Stepping one selected string at a time proved the
geometry, and it isn't how a face gets designed. Ramps come once this is tested.

## Decision

1. **A face design is one saved object**: its lines, the profiles they use and the named rules those use, so an
   edit is one revision and one undo. It saves with the project. Rules are named and reused as Deswik's are, and
   ramp rules can sit beside them later. `Fixed` is the only rule type so far.
2. **A line is a string, a profile, a side and a run-on past each end.** Lines come from the selection, a list,
   or a whole layer, as selection is one object at a time.
3. **`face.project` runs every line in one run, one undo.** A run makes toe and crest strings for each wall and
   one face surface per line, on the design's layer, and replaces what the last run of that design made.
4. **Each line gets its own oversized face.** Faces that cross are fine, because the cut does the fitting.
5. **Steps at one batter with no berm between them are one wall.** A face projected to the topography and on to
   an RL is the same plane carrying straight through, not a new wall from a rough line. The targets on the way
   only mark where crest lines fall. A change of batter with no berm stands the new wall clear of the crest by
   the tolerance, because a conditioned line weaves either side of its crest.
6. **The leg down is the same walk with heights mirrored**, so the sheet carries on below the toe on the far side.
7. **A toe off its grid carries on at the grade beside it**: straight across a hole, and at its last grade, read
   over 20 m, past an end.
8. **Where a target has no data, the crest bridges the gap and the stretch is reported.** This changes how engine
   rule 6's fall-back to a level is met. Inside a hole the target is bridged straight between its edges, and past
   its outside edge it's held level with the nearest edge. Holding one level across a hole would leave a step at
   the far side, and a face has to stay one sheet. PIT B's grids are about half null.
9. **A step with no height anywhere is passed over**, and the next step carries on from the same string.
10. **A rule used as both a batter and a berm is refused**, as one is in degrees and the other in metres.
11. **Targets may be wound either way.** Grids wind upward but a `.00t` needn't, and the walk goes by winding.

## Evidence

Tim's own 620 m string on PIT B, run on 100 m each end, up through four seam grids with 10 m berms, to the real
topography and on to RL 500, and down to RL -500, at 70° and 45°, each side:

| Target triangles in | Time for the line | Walls | Face triangles out |
|---:|---:|---:|---:|
| 289,000 a grid | about 55 ms | 7 | 6,300 to 6,900 |

Each face has no crossing triangles, consistent winding, and one boundary loop, so it's one sheet. Before the
berms were triangulated from their outline, the same faces had 7 to 11 crossing pairs.

Known answers hold to 1 mm: every crest and berm of the sequence on level surfaces, and the face's area. The panel
was driven in a browser against a stand-in engine, and the real commands through the bench.
