# 0028 — Ramps: a road through the walls

Date: 2026-11-01 · Status: accepted for a road through one set of walls. Tim agreed the Ramps plan's decisions on 2026-11-01, then changed two after trying the first road: ramps moved into the profile grid, and the entry became the moved toe

## Why

A ramp is the hardest thing left in Phase 3, because it changes the walls it runs through. It goes in as each wall is
projected, so it came ahead of surface build and cut. Phase 3 decision 7 set it by start point and direction, so running the
design again rebuilds it. This record covers workstreams 1 and 2 of the plan and the entrance, and what later ones build on.

## Decision

1. **A ramp is in the profile grid.** A Ramp start row and a Ramp stop row sit among the steps, and the walls between them carry
   the road. Tim tried a separate table first and expected "a start a ramp at this profile and stop one here". The start row
   holds a start point in plan, a way along the string, a width and a grade. A ramp goes on the string nearest its start, of
   those its profile runs on. A row between two walls of one batter makes them two walls, so a road can stop part way up a
   tall one. Road width and grade are typed for now, and a named ramp rule can follow.
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
   the two walls stand `width × cos(angle)` apart. Slopes that close are one batter where a string turns at all, so they meet
   in a hip: a transition is for a real change of batter.
6. **The entrance is the toe moved back.** Tim: trucks need a way on, and "the simplest is actually offsetting the whole toe
   across the width of the ramp on the entrance and the opposite side of the ramp direction". The toe behind the road's start
   moves back as far as the road's wall-side edge starts from it, so the wall there and the wall above the road are one
   plane, and the floor runs straight onto the road. On an upper bench that widens the berm below behind the start. It's
   draped on what the toe lies on. A flat pad can follow as a second entrance.
7. **The road is the bench where it crosses a berm.** A level bench can't hold a grade, so a road can't climb across one. The
   berm narrows to nothing where the road comes up through it, at 45 degrees behind the road, and the road carries on up
   the next wall from the same edge. Ahead of the road the berm opens out over ten times its width, so the road bends gently
   across the berm's width with no step in it. The walls below the road keep their berms. Min bench doesn't apply to a road.
8. **A road that stops doesn't stop the face.** Where it can't reach the top of a wall (off the end of the string, a toe
   rising faster than the road, a wall flatter than the grade), or the wall above it can't be built, it stops at the last
   bench it made and the run names the step, the place and the reason. If what's built on a road can't be, the highest road
   stops a wall lower and the face is run again.
9. **A road comes out on its surface across its full width.** Where the surface rises behind the wall, the wall-side edge
   carries straight on at the grade until it gets there. If it hasn't within 200 m it stops and says so. The next stretch of
   road takes up its offset once that's past where the edge came in.
10. **The wall-side edge is a true offset.** A wall's facets kink a road's edge a little at each one, and a mitre at every
    vertex folds over on real grids at 30 m wide. Then it's tidied to 1 cm, as walls follow a line's plan shape.
11. **A road meets the wall above with no slit.** The road's inner line takes a vertex wherever the crest comes down to it,
    as a berm does.

Two things changed in Min bench (0027 item 12), found by running roads:

12. **A stretch with no berm stands clear by both tolerances.** The crest is simplified by its row and the line by the berm's
    row, so each can weave. Standing clear by one let them cross, and the berm folded, on a plain wall with no road.
13. **A change of width with no room to bevel moves to the corner.** It's tried only if the plain offset is refused, so nothing
    that worked before changes.

## Results

Synthetic, known answers: a 10 % road up a 20 m wall at 45 degrees is 200 m long, finishes 198.997 m along and 20 m out,
holds its width to 1e-6 m and its grade to 1e-9. Across a berm and up a second wall it's 400 m in one grade, turning no more
than the taper's angle. The toe behind it and the crest above it run dead straight, and the sheet covers the plan between
toe and crest once, to 1 part in a million.

PIT B through `face.project`, a 30 m road at 10 % from each of five walls of a real 600 m toe to the top of the last, both
sides, both ways, 20 runs, 130 to 230 ms, about 9,400 triangles: 17 reached the top, up to 301 m through five walls. 3 stopped
at the last bench they could make and said why. Each was one sheet with nothing crossing.

A stressed sweep of 960 cases with a wall wandering round a 5 m minimum bench and the road on one wall: 920 built, 40 stopped
at the string's end, none left out, none refused. With the road through to a second wall: 892 roads, 28 of them stopped by
what was built on them, none refused. Every road built was one sheet with nothing crossing.

## What this leaves

- **Switchbacks.** A road that runs out of string stops and says so. That's workstream 5.
- **A road only goes on a profile going up.** Going down from a fixed toe it couldn't cut back, so the wall below would move
  out from the pit side. That needs its own decision.
- **Open strings for now.**
- **The start of a road is level across.** Where the floor isn't level at the road's start, the moved toe leads up to the
  road's edge over a road width, and the road's bottom edge can stand a little off the floor.
- **The taper is fixed at ten times the berm.** It belongs in a ramp rule, with the road's own tapers.
- **Grade is in percent for now.** Ratio and degrees are a matter of the box.
- **A start point, not a digitised centreline,** as Phase 3 decision 7 agreed. CLAUDE.md still lists a centreline.
- **Not compared with Deswik.** That waits for a real ramp.
