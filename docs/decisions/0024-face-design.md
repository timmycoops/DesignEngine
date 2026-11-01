# 0024 — The Face design panel and a whole face

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31), one-table layout 2026-11-01

## Why

Tim: a full face needs "a panel (form whatever we call it) with clear rules to make this repeatable - not
possible to do it from the selection of one string either". Stepping one selected string at a time proved the
geometry, and it isn't how a face gets designed. Ramps come once this is tested.

The first panel spread a profile across a dropdown, separate up and down tables and a rules section. Tim laid it
out as one table instead, a row per step, "easier to use", and asked for everything to go in that format.

## Decision

1. **A face design is one saved object**: its lines, the profiles they use and any rules those share, so an edit
   is one revision and one undo. It saves with the project. `Fixed` is the only rule type so far, and ramp rules can
   sit beside them later.
2. **A line is a string or a layer, a profile, a side and a run-on past each end.** A line on a layer takes the strings on
   it at each run, hidden or not, so a string added since comes in on the next run and nothing rebuilds by itself. It
   leaves out what this design made last time, and a string a design reaches twice under one profile runs once. One side
   covers the whole layer, so strings on it want digitising the same way round. Tim, 2026-11-01: "a layer or string inside
   a layer". The layer is held by its id, so renaming it doesn't lose it.
3. **`face.project` runs every line in one run, one undo.** A run makes toe and crest strings for each wall and
   one face surface per line, on the design's layer, and replaces what the last run of that design made.
4. **Each line gets its own oversized face.** Faces that cross are fine, because the cut does the fitting.
5. **Steps at one batter with no berm between them are one wall.** A face projected to the topography and on to
   an RL is the same plane carrying straight through, not a new wall from a rough line. The targets on the way
   only mark where crest lines fall. A change of batter with no berm stands the new wall clear of the crest by
   the tolerance, because a conditioned line weaves either side of its crest.
6. **A profile is one table going one way, up or down**, for one type of string. Tim: keep it in one direction and
   create the bottom or top string by hand after the rules have run. Going down is the same walk with heights
   mirrored, so the same side carries on the same plane below the toe. A string that goes both ways is in twice, with
   a profile for each, and each makes its own sheet. They aren't joined.
7. **A toe off its grid carries on at the grade beside it**: straight across a hole, and at its last grade, read
   over 20 m, past an end.
8. **Where a target has no data, the crest bridges the gap and the stretch is reported.** This changes how engine
   rule 6's fall-back to a level is met. Inside a hole the target is bridged straight between its edges, and past
   its outside edge it's held level with the nearest edge. Holding one level across a hole would leave a step at
   the far side, and a face has to stay one sheet. PIT B's grids are about half null.
9. **A step with no height anywhere is passed over**, and the next step carries on from the same string.
10. **A rule used as both a batter and a berm is refused**, as one is in degrees and the other in metres.
11. **Targets may be wound either way.** Grids wind upward but a `.00t` needn't, and the walk goes by winding.
12. **Everything about a step is on its row**: Type, Surface or RL, Batter or Width, Register and Condition. The toe
    is the first row. A row says where it finishes, because where it starts is where the row above ended.
13. **A batter or width is a number typed on the row, or a rule's name.** A rule is only needed to share a value, or
    once one changes along a line.
14. **Register** is always true on a wall, which stops on its target with a vertex at every triangle it crosses, so
    the tick is locked there and blank for an RL. On a berm, unticked, the line keeps its crest's heights. A berm
    registers on what the row above reached unless its row names another surface or RL.
15. **Condition is a tick and a tolerance on each row**, 2 m at most. On the toe it cleans the line before it
    registers, and the report says so. On a wall it cleans the crest before the next step, and on the last wall it
    cleans only the string, as the sheet keeps its own edge. On a berm it cleans the line after it's moved out. Between
    walls of one batter there's no line to clean (item 5), so the tick is greyed out.
16. **A berm line with no surface under it holds the height of its crest.** It used to fall to RL 0, which PIT B's
    holes reach on a real string.
17. **A berm waits for the wall above it.** That wall's crest can come down onto the berm's line part way along a
    segment, and the berm takes a vertex there. Without it the surfaces meet but the mesh has zero-width slits, and a
    cut can't read the face's edge.
18. **Same batter both sides of a berm, on a target with no cross-fall, is refused.** A line moved square off its
    crest then lies in the plane of the wall below, so neighbouring walls are one plane with no hip between them. Real
    grids have cross-fall. A different batter, or any cross-fall, goes through.

## Evidence

Tim's own 620 m string on PIT B, run on 100 m each end, as one design with a profile each way: up through four seam
grids with 10 m berms, to the real topography and on to RL 500, and down to RL -500, at 70° and 45°, each side:

| Target triangles in | Time for both faces | Walls | Triangles out, up | Triangles out, down |
|---:|---:|---:|---:|---:|
| 289,000 a grid | about 65 ms | 7 | 5,199 to 5,312 | 417 to 441 |

Each face has no crossing triangles, consistent winding, and one boundary loop. Before the berms were triangulated
from their outline, the same faces had 7 to 11 crossing pairs.

Known answers hold to 1 mm: every crest and berm of the sequence on level surfaces, up and down, and the faces' areas.
The slit test fails with 3 boundary loops with item 17 switched off. The table was driven in a browser against a
stand-in engine and what it saved is pinned as a test against the Rust types. The bench runs the same two-way design
through the real app and its bridge (decision 0025), but not the table's own controls.
