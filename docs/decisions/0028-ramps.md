# 0028 — Ramps: a road through the wall

Date: 2026-11-01 · Status: accepted for one road leg. Tim agreed the seven decisions on the Ramps plan tab of the Build Plan on 2026-11-01

## Why

A ramp is the hardest thing left in Phase 3, because it changes the wall it runs through. It goes in as each wall is
projected, so it came ahead of surface build and cut. Phase 3 decision 7 set it by start point and direction, so running the
design again rebuilds it. This record covers workstream 1 of the plan, one road up one wall, and what later workstreams build on.

## Decision

1. **A ramp is on a line of a face design.** `FaceLine.ramps`: a start point in plan, a way along the string, the step it
   starts from and the step it finishes on, a width and a grade. On a line that names a layer it goes on the string nearest
   its start. Road width and grade are typed for now, and a named ramp rule follows with tapers.
2. **A road cuts back into the wall.** The stretch of wall splits in three: the lower wall stays where it was and stops at
   the road's pit-side edge, the road climbs at its grade at full width, and the upper wall rises from the road's wall-side
   edge. The other way builds the road out and moves the toe.
3. **The inside edge is the pit side.** It's the shorter edge on a road spiralling round a pit, and it's the edge that holds
   the grade. Deswik's reference doesn't define the word: `OffsetOutsideRight` suggests outside means away from the pit.
   The two edges only differ round corners.
4. **One grade, as rise over plan run, whatever the grids do.** The pit-side edge is walked up the plain wall facet by facet.
   On a plane there's one way forward that rises at the grade, so a toe that rises and falls changes the edge's line and
   never its grade, and nothing drifts. Each step is exact to the facet.
5. **The wall above is parallel to the wall below.** A road climbs a wall at an angle to its strike, sin(angle) = grade /
   slope. Measured square to the road, the batter would lean the wall above: about 10 cm over 200 m at 45 degrees and 10 %.
   So its batter is set from the slope left after the road's rise: `sqrt(slope² - grade²)`. Width is square to the road, so
   the two walls stand `width × cos(angle)` apart.
6. **The start of a road is a step back across it.** The upper wall's string runs along the toe to the road's start, back
   across the road and up the wall-side edge, so the end of the cut is an ordinary wall at the same batter, with hips. It's
   the plainest start, and tapers replace it in workstream 3.
7. **A road that stops doesn't stop the face.** Where it can't reach the top of its wall (off the end of the string, a toe
   rising faster than the road, a wall flatter than the grade), or the wall above it can't be built, that road is left out,
   and the run names the step, the place and the reason. If a step above can't be built on it, the road comes out, highest
   first, and the face is run again.
8. **A road comes out on its surface across its full width.** Where the surface rises behind the wall, the wall-side edge
   carries straight on at the grade until it gets there. If it hasn't within 200 m it stops and says so.
9. **The wall-side edge is a true offset.** A wall's facets kink a road's edge a little at each one, and a mitre at every
   vertex folds over on real grids at 30 m wide. Then it's tidied to 1 cm, as walls follow a line's plan shape.
10. **A road meets the wall above with no slit.** The road's inner line takes a vertex wherever the crest comes down to it,
    as a berm does.

Two things changed in Min bench (0027 item 12), found by running roads:

11. **A stretch with no berm stands clear by both tolerances.** The crest is simplified by its row and the line by the berm's
    row, so each can weave. Standing clear by one let them cross, and the berm folded, on a plain wall with no road.
12. **A change of width with no room to bevel moves to the corner.** It's tried only if the plain offset is refused, so nothing
    that worked before changes. A road's top is two sharp corners, and a change 1.5 m short of one has nowhere to go.

## Results

Synthetic, known answers: a 10 % road up a 20 m wall at 45 degrees is 200 m long, finishes 198.997 m along and 20 m out,
holds its width to 1e-6 m and its grade to 1e-9. The sheet covers the plan between toe and crest once, to 1 part in a million.

PIT B through `face.project`, a 30 m road at 10 % up each of five walls of a real 600 m toe, both sides, both ways, 20 runs,
130 to 250 ms, about 9,000 triangles: 19 roads built, up to 261 m long, each one sheet with nothing crossing. One came out and
said why, and its face built.

A stressed sweep of 960 cases with walls wandering round a 5 m minimum bench: 892 roads built, each one sheet with nothing
crossing, 40 stopped at the end of the string and said so, 28 came out and said why, none refused.

## What this leaves

- **One wall.** A road finishes at the top of the wall it started on. Workstream 2 takes it across berms and up the next walls.
- **A road only goes on a profile going up.** Going down from a fixed toe it couldn't cut back, so the wall below would move
  out from the pit side. That needs its own decision.
- **Open strings, and one road to a wall, for now.**
- **A row carrying on as one wall into the next** can't finish a road. Those rows are reported.
- **Grade is in percent for now.** Ratio and degrees are a matter of the box.
- **A start point, not a digitised centreline,** as Phase 3 decision 7 agreed. CLAUDE.md still lists a centreline.
- **Not compared with Deswik.** That waits for a real ramp.
