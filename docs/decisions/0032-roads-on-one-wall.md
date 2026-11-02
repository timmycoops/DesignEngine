# 0032 — Roads on one wall

Date: 2026-11-02 · Status: accepted. Tim wants several ramps on one strip, and asked to get that right on one strip, to RLs if need
be, "to embed the foundation logic of how the project, berm and ramp all work together". This changes item 6 of 0030, and the rest are
faults one real strip showed up

## Why

Tim's three ramps sit about 1.5, 3.3 and 5 km along strips 6.5 km long, all on all four walls. A wall took one road a strip, so the
first took them and the others built nothing. Earlier advice that three full-height roads couldn't fit was wrong: running the same way
at one grade they keep the same height apart and never meet.

## Decision

1. **Roads on a wall stack.** At any place along a wall the road that started furthest ahead is the lowest, and what it cuts back moves
   everything above it. So roads go in lowest first, each into the wall above the one below. A section through two reads wall, road,
   wall, road, wall, with each wall parallel to the plain one and a road width further back than the one below it.
2. **Roads on one wall run the same way for now.** One that doesn't says so and the others build. Roads running away from each other
   need their entrances to share a floor, which is its own work.
3. **A berm comes in to the top of each road on it.** Where two roads are too close for it to open out in full, it opens as far as it can.
4. **An entrance moves the toe back only as far as a road coming up through that toe,** and comes across at 45 degrees.
5. **A wall with no height doesn't stop a road.** A road starts on the first wall with height at its place, and one carrying on goes on
   to the next wall from the same place. With no wall left above it, it has come out of the pit.
6. **A held end stays on its string.** A road too long for the strip starts at the end of the string, builds, and says how far off it
   came out. It used to give nothing, or a start over 10 km past the end.
7. **A wall above a road takes its slope from the wall under it, rise and all,** so walls above a rising road stay parallel however
   many roads are in the stack.
8. **Two faults in a face.** A toe held above a target took its height from the nearest crest vertex, not the nearest point. A berm
   touching its crest either side of a corner left the corner open, a hole of up to 170 by 140 m.
9. **Three faults in a wall.** Small walls closed in order of how far they were from their own foot, not by height, so on a toe that
   rises and falls a wall could be left open and the crest walked off along the wrong plane. That was "the wall runs off the target" on
   Strip 1, in the middle of the grid with every node live. A short step off a wall's corner could miss the next edge, as the slack was in
   parts of a step, not metres. A transition with no width had a plane the crest couldn't walk, and it now steps over it.

## Results

Known answers, on a level toe at 10 %. Two roads 300 m apart on a 40 m wall at 45 degrees: each 400 m to 1 mm, grade to 1e-9, and a
section through both on its analytic heights to 1 mm, with the crest two road widths back. Three roads up three walls across two
berms: each 600 m to 1 mm, grade to 1e-9, one sheet with nothing crossing. Also tested: roads apart, roads close together, running back,
sharing some walls and not others, starting above a road, starting where the first wall has no height, and a gap part way up.

PIT C, each of 11 strips on its own, all three ramps on all four walls, a 50 m road at 10 %, 70 degrees and 30 m berms. 132 roads each way:

| Walls to | Start along | Start back | End along | End back | Stopped by a wall fault |
|---|---|---|---|---|---|
| RLs -50, 50, 150, 250 | 10 | 15 | 21 (20) | 30 (21) | 3 of 132 |
| Tim's grids | 5 | 17 | 17 (16) | 25 (22) | 23 of 132 |

Roads reaching the top of 33, with those that came out away from where they were held in brackets. Every face is one sheet, and one in 88
has triangles crossing. The other stops are the ground (the toe rising faster than the road), the strip's length, or two roads wanting
one start. To RLs a strip takes 45 to 90 ms from the toe and 70 to 300 ms held at the end. To grids it takes 530 to 1,660 ms and 3,200
to 6,400 ms, far over the gate's 1 s.

PIT B, the run of 0030: 8 runs, 44 faces, all one sheet with nothing crossing, 380 to 610 ms held at the start and 2,060 to 3,120 ms
held at the end.

## What this leaves

- **Wrinkled strings.** A line draped on a wavy grid wrinkles along and up and down, and that can still lose a wall. 16 of 64 made-up
  wrinkled strings fail with or without these fixes, kept as an ignored test, `wrinkled_strings_below_their_target_all_build`. It's the 23
  above, and why RLs are so much cleaner. It wants looking at before anything else is built on grids.
- **Roads running opposite ways on one wall.**
- **Switchbacks.** The road reaching the end of the strip is the commonest stop.
- **Held ends on grids are slow,** three roads on one strip taking up to 8 runs of the face.
- **One face in 88 has triangles crossing,** on Strip 3 to grids with its ends held.
- **The wall fixes were proven on the real strip and by small tests of each piece.** Nothing made up reproduces the closing order
  fault end to end.
