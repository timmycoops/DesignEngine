# 0023 — A wall step from grid to grid

Date: 2026-10-31 · Status: accepted (Tim, 2026-10-31), with item 5 for Tim to confirm

## Why

Tim: "as a rule in a coal mine it is possible to go to an RL but that's easy, projecting to and from a
complicated grid is the hard part". A real face is a rule sequence: register a toe to Grid A, project to
Grid B and berm there, project to Grid C and berm there, project to the topography and on past it to an RL,
then back down from Grid A to an RL. Nearly every step starts on one undulating line and ends on another
undulating surface. So Phase 3 starts by proving one step between real grids, before rules, profiles or
forms are built on it. The face as a whole is built oversized, RL to RL and past the pit shell, and the cut
does the fitting.

## Decision

1. **Each string segment makes a wall plane**, through the segment and rising at the batter square to it in
   plan. Neighbouring walls meet in a hip, the line their planes share, which keeps walls and mitres exact
   on a string that rises and falls. A plan offset can't: there, the mitre leaves the bisector.
2. **The sweep up the walls is a straight skeleton with planes in place of plan offsets.** Where a wall's
   two hips meet it is squeezed out, and its neighbours meet in a new hip. A hip runs outward in plan, not
   necessarily uphill: where a string climbs along its length, its true hip can run out and down. There is
   no single level all walls can be swept by, so a closing goes ahead once it's the first thing to happen
   along both of its hips.
3. **The crest is walked across the target**, triangle by triangle and wall by wall, on exact edge crossings
   as in a section. The walls are swept past the top of the target first, so a closing above the crest
   changes nothing.
4. **A line is conditioned before the next step.** A line cut from a grid has a vertex at every triangle it
   crosses. Its plan shape is simplified to a tolerance, moved out a berm, and registered back on the grid.
   Walls take their azimuth from the plan alone, so vertices that are collinear in plan add no corners
   however the line rises and falls, and the line stays exactly on the grid. This runs before a step, not
   after it, so it isn't offset-then-repair. Deswik has the same stage (`DefaultLineConditioner`).
5. **A berm is the conditioned crest moved out in plan and registered on the grid**, not a delayed start. A
   step starts from a string on a grid, so there's no one level for a delay to count from.
6. **Where a target is below the string, the step has no height there.** The crest runs along the string and
   the place is reported.
7. **Refused for now, each with its place:** a wall that runs off the target, and a wall that closes against
   a far wall (a split in skeleton terms), which would make the crest cross itself. The first needs the
   fall-back of engine rule 6 and comes with face projection. PIT B's grids are about half null, so oversized
   ends will reach it.
8. **`face.project_step`** is the typed command: string, target surface or grid, batter, side, berm and
   tolerance in; crest, wall surface and next toe out. The string stays as it is. Rules, `FaceProfile` and
   legs down from the toe build on it.

## Evidence

Toes along strike across PIT B's real grids, a 10 m lattice of 289,000 triangles, stepped through four walls
over five seam grids at 70°, to each side, at 0.1, 0.25 and 0.5 m tolerance with 10 and 20 m berms:

| Toe | Target triangles in | Time a step | Crest vertices | Wall triangles out |
|---|---:|---:|---:|---:|
| 600 m | 289,000 | 1.5 ms | 270 to 310 | 410 to 480 |
| 2 km | 289,000 | 5 ms | 980 to 1,010 | 1,480 to 1,530 |

All 52 walls in that run have no crossing triangles, and each wall's only open edges are its toe, its crest
and its two ends. Sections square to each wall read 70.0000°, with both ends within 1 mm of their grids. On
the raw crest, with no conditioning, the next step ran off the target. At 0.1 m the 600 m crest's 309
vertices come down to 18 in plan.

Through the app, with PIT B's 42,000 solids, topography and seam model loaded, four steps up five grids took
1.1 s including the panels (`--walls` on the bench). They read on the section as walls landing on each seam
line with a berm along it, and a stretch where two grids cross came back as having no height.

Known answers hold to 1 mm: a 45° batter from a flat toe, mitres on both sides of a corner, a closed toe's
rectangle and its wall area, a step between two dipping planes, a mitre on a rising string, and a wall
squeezed out.
