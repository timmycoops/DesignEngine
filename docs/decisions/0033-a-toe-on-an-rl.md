# 0033 — A toe on an RL

Date: 2026-11-02 · Status: accepted. Tim asked how to register a toe to an RL, and it couldn't be done: the Register row took a grid or
surface only. He said "Yep we need that". This changes `FaceProfile.register`, which was a surface's id. Item 5 is a fault the first
real run showed up, and item 6 one the template to try it in the app showed up.

## Why

Rows could step up to RLs, but a toe always lay on a grid, so a run "to RLs" still started from a line that rises and falls with the
ground. A level toe up level benches is the plainest case of project, berm and ramp there is, and settling that on one strip is what
Tim asked for in 0032.

## Decision

1. **A toe registers to a surface, a grid or an RL.** `register` takes the same target a row does, and the Register row takes `–` and
   an RL the way a row does. The toe lies flat at that RL, whatever heights its string had.
2. **A design saved with a surface's id still opens,** and runs as it did.
3. **An RL goes down the one path.** The toe registers on a level floor, a triangulation like any other target, reaching 1 km past
   the toe. So walls, berms, entrances and faces going down need no code of their own for it.
4. **A design no longer needs a surface.** A new one starts on RL 0 when the project has none, and new rows after a toe on an RL go
   to RLs until a row names a surface.
5. **A road that can't go in no longer stops the others finding their held ends.** The run kept the face whose worst road was closest,
   so one road that never built, or one stuck at the end of the string, froze every other road on the strip at its first guess, out
   by 2 to 70 m on a bent strip. Now it keeps the face with the most roads at the top, and of those the one with the least distance
   left between them.
6. **The top end of a road is no gap.** A road comes out level with its crest, so there's no wall across its top end, and
   that's where it was meant to finish. Rounding decided whether the run saw it: on two strips of the template's five it
   said a wall had no height there. It's no longer reported, and the next toe is built as it is where rounding went the
   other way.

## Trying it

`testdata/synthetic/ramp_template.py` writes `Ramp template.zdproj` at made-up coordinates: five strips 80 m apart from a
bent base toe, numbered by a line across, two ramp strings across them, and the design Ramps on RLs. It runs every strip
on the toes layer: the toe flat on RL -100, walls to RL -60 and -20 at 70 degrees and RL 20 at 65, 20 m berms, and two
30 m roads at 10 % up all three walls, A held at its start and B at its end. The script runs the design once without
saving it and prints what each strip made.

## Results

Known answers, with no surface in the project: a toe on RL 100 under walls at 45 degrees to RL 140 and RL 60 has its crest 40 m out
each way, to 1 µm; a berm as the first row lies on RL 100; a road up the wall at 10 % is 400 m to 1 mm, its entrance stays on RL 100,
and the toe moves back 9.95 m, a 10 m road square to itself. A design holding the grid's id builds the same as one naming it.
On the template's five strips every road is 1,200.0 m from RL -100 to 20, every face is one sheet with nothing crossing, and
no wall is said to have no height. One thing differs where the strips are the same shape: on Strips 1 to 3, road A's wall
edge has a kink 1.7 m long where it leaves the second berm, rising 0.3 m, and on Strips 4 and 5 it doesn't.

PIT C, each of 11 strips on its own, all three ramps on all four walls, a 50 m road at 10 %, 70 degrees and 30 m berms. Roads reaching
the top of 33, with those that came out away from where they were held in brackets:

| Toe | Walls to | Start along | Start back | End along | End back | Stopped by a wall fault |
|---|---|---|---|---|---|---|
| RL -150 | RLs -50, 50, 150, 250 | 11 | 11 | 22 (11) | 22 (11) | 0 of 132 |
| Tim's grid | RLs -50, 50, 150, 250 | 10 | 15 | 23 (18) | 30 (9) | 3 of 132 |
| RL -150 | Tim's grids | 8 | 15 | 23 (18) | 21 (8) | 19 of 132 |
| Tim's grid | Tim's grids | 5 | 17 | 21 (14) | 25 (13) | 22 of 132 |

- **A flat toe to RLs is clean.** Every one of the 110 roads built is its rise over the grade: 1,000.0, 3,000.0 or 4,000.0 m. All 44
  faces are one sheet with nothing crossing. Every stop is the strip's length, as a 4 km road doesn't fit everywhere on a 6.5 km
  strip, or two roads wanting the same start at the end of one. A strip takes 3 to 4 ms from the toe and 8 to 12 ms held at the end.
- **Wall faults come from rows that finish on grids, not from the toe.** A toe on Tim's grid going to RLs loses 3, a flat toe going
  to his grids loses 19. Of those 19, 12 are Strip 1, refused outright at step 5 with "the crest walk didn't finish", which builds
  from his grid toe. One face, Strip 9 with its ends held, has triangles crossing.
- **Held ends land more often.** With a toe on his grid going to RLs, 21 of 30 roads at the top land where they were held, against 9
  of 30 before item 5. The one face in 88 with triangles crossing in 0032 was a first guess, and it's gone.
- **PIT B,** the run of 0030: 8 runs, 44 faces, all one sheet with nothing crossing, 315 to 405 ms held at the start and 1,544 to
  2,113 ms held at the end. Run again the same evening, with and without item 6, it made the same triangles to the one and took 400
  to 610 ms and 2.2 to 3.3 s, so the first timings were low, not item 6 slow.
- Toes on grids are no faster: to grids, 473 to 1,247 ms from the toe and 2,551 to 4,658 ms held at the end.

## What this leaves

- **Lines draped on grids.** That's where every wall fault left is, including the new one on Strip 1. It's the next piece of work.
- **Held ends to grids are slow,** up to 5 s for three roads on one strip.
- **The kink in road A's wall edge** on three of the template's strips, which isn't chased yet.
