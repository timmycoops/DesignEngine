# 0031 — Ramps on real strips

Date: 2026-11-02 · Status: accepted. Tim ran ramps across eleven strips of PIT C and got almost nothing back, and this is what his
own run log showed. Item 2 changes item 2 of 0030. The rest are faults in how a road was built or reported

## Why

Eleven strips 100 m apart, 6.4 to 6.8 km long, with toes on seven vertices. Walls climb 170 to 420 m, so a road at 10 % is 1.7 to
4.2 km. Three ramps sat across them, about 1.5, 3.3 and 5 km along. No strip got a finished road on his first run, and ten got no
road at all. He changed batters, berms, widths and sides, then cut it down to one strip, and it still wouldn't go. Every test had
passed, as each toe was straight and the real-data test's ramp string ran on 1,500 m past the crests.

## Decision

1. **A berm opens out along the crest, not across a corner.** Ahead of a road it opened out in one straight line, ten berm widths
   long. A crest turning a corner inside that stretch stood in its way, so the berm crossed it and the road stopped at "the berm
   folds over itself". Now each crest vertex over the taper moves towards the berm line, by a share that grows with the metres from
   the road, so the berm turns every corner the crest does.
2. **A held end goes by where the string crosses the toe.** It came from where the string crossed the top crest, and that's two or
   three strips along from the strip's own point when strips are 100 m apart and walls are 250 m wide in plan. So a string with a
   point on each strip missed the crests of the last three, and moving a point moved another strip's road. Now it's the toe either
   way, and the road comes out on its last crest level with that place along the strip. Nothing about a string needs to run past a
   strip, and a strip's point sets that strip's road.
3. **A road up onto ground with no wall left above it has finished.** It said "there's no wall at its start", in red, on its top
   wall, when it had come out of the pit. If there's more wall above that gap it still says so.
4. **"It runs out of wall" is now "it reaches the end of the wall",** which is what it means: the road ran off the end of the string
   before it got to the top. That's the case switchbacks are for.
5. **A strip that can't be built leaves the others standing.** One strip's wall refused all eleven. It now says why in red and
   nothing is built on it. A run with nothing to show is refused as before.
6. **The crosshair on a ramp won't take a strip.** Tim's Ramp A took Strip 1 that way. Its string then crossed its own line 293 times,
   and as a ramp string isn't a strip, Strip 1 quietly left the run.

## Results

The same strips and walls, in memory, 70 degrees with 50 m berms and a 50 m road at 10 %, one ramp at a time. Strips out of eleven
whose road reached the top, with those that came out away from where they were held in brackets:

| Ramp | Ends at, along | Ends at, back | Starts at, along | Starts at, back |
|---|---|---|---|---|
| A, about 1.5 km along | 0 | 11 (1) | 7 | 0 |
| B, about 3.3 km along | 11 (4) | 10 (2) | 0 | 10 |
| C, about 5 km along | 2 (2) | 0 | 0 | 10 |

Of the five settings run before and after: B ended along, and B and C started each way. 3 of 55 strips reached the top before, and 31
after. Some of that is item 3, which calls a road finished that had already come out. The rest stopped because of where it was and
which way it was heading: the toe rising faster than the road, or the end of the strip.

Before item 1, 20 of 22 strips with the road started back along the toe stopped at a folded berm. After it, none of the 132 did.
All 132 faces are one sheet with nothing crossing, 172,860 to 222,044 triangles a run. Held at the start 620 to 1,000 ms for eleven
strips, held at the end 4,500 to 5,400 ms. Over the plan's 1 s, and 15 strips will be worse.

PIT B, the run of 0030: 8 runs, 44 faces, all one sheet with nothing crossing, 420 to 590 ms held at the start and 2,160 to 3,080 ms
held at the end. Its string no longer runs past the crests.

Known answer: a 30 m berm above a road, 40 m from road to line, with a 30 degree corner in its taper. Past the corner the gap grows with
the metres along the crest to a micrometre, and it's a full berm past the taper.

## What this leaves

- **A wall takes one road a strip,** as in 0030, and Tim's three ramps all want all four walls. With every wall ticked the first ramp
  takes them, and the others build nothing. This is the next limit he'll hit.
- **Roads too long for the strip,** which need switchbacks. C has 1.4 to 1.8 km left along its strip and 2.3 to 3.3 km of road to
  build.
- **The start can still be wrong for the ground.** B started along the toe fails on all eleven, as the toe rises faster than 10 % there.
  It's reported, not fixed, and the planner turns the road or moves the string.
- **At 65 degrees with 30 m berms Strip 1's second wall runs off its target,** with no ramp on it. It reports in red now and the others
  build, but why it runs off hasn't been found. A gap should bridge and report, as 0024 says.
