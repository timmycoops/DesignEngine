# 0037 — Roads on a face going down

Date: 2026-11-03 · Status: accepted (Tim, 2026-11-03: "Yes, build them", then "Match Deswik's join" for what follows a
switchback). This is the first of two parts. Switchbacks follow.

## Why

Tim's back strip from Deswik (PIT A, kept local) is a design going down: from a start string at RL 140, 45° walls of 30 m
with 10 m berms to RL -190, and a 40 m road at 10 % coming down them, turning on flat switchbacks. Away from the road our
walls already matched Deswik's to the millimetre. Our roads only went up, so the rest couldn't be compared. Decision 0028
left a road going down open because it moves the walls below it out towards the pit. That's what Deswik does, and what Tim
wants.

## Decision

1. **A road comes down a profile going down** with the same ramp settings as one going up. It starts on the top line where
   its string crosses, runs down each wall that names it at its grade, and gets past each berm after it (crossing level) or
   through it, as set (0035).
2. **It's built the way a face going down already is: upside down.** The face is projected as a face going up with heights
   mirrored and the side swapped, so the road climbs it, and both are turned back. The road's two edges swap as they're
   turned back, so its pit-side edge is still on the side the face goes down to.
3. **Its entrance is a notch in the top line.** Behind the road's start the top line moves out by the road's width, as the
   toe behind an entrance moves back going up.
4. **A wall going down starts from its crest and comes down to its toe,** and the strings a run makes are named that way:
   `crest 1` is the top line, `toe 1` the foot of the first wall. They were named as if the wall went up.
5. **Why a road stopped is told the right way up:** RLs as they are, and "top line", "bottom" and "below" where a road
   climbing says "toe", "top" and "above".

The grade is held on the edge walked down the wall, which is the edge against the wall above. On a straight wall both edges
hold it exactly. On a bend the pit-side edge, the inside edge CLAUDE.md names, differs by the road's width times the bend.
That's left until a design needs it.

## Results

Known answers: a 10 m road at 10 % down a 45° wall 20 m high leaves the top line where it's put, runs 200 m in plan to RL
100 and finishes 198.997 m along and 20 m out on the pit side, both edges at 10 % and 10 m apart, level across. Going down
two walls after a 10 m berm, it runs 400 m at 10 % and crosses the berm on one level landing of 13.484 m, and the berm
keeps its 10 m. Through the commands, a road down a 40 m wall is 400 m from RL 100 to 60, and the strings are named crest
and toe the right way.

Tim's back strip, the start string without Deswik's notch and our road down the first three walls to RL 50, where
Deswik's first turns:

| Leg | Ours: drift out from the line it starts on | Deswik's | Deswik's ramp face setting |
|---|---|---|---|
| 1, RL 140 to 110 | 30.0 m | 39.7 m | 37° |
| 2, RL 110 to 80 | 30.0 m | 30.0 m | 45° |
| 3, RL 80 to 50 | 30.0 m | 30.0 m | 45° |

- **Every leg holds 10 % and its 40 m width,** and drifts out as a road on its wall must: 30 m in a 30 m drop on 45°.
- **Deswik's first leg is set to a 37° face,** and a 30 m drop on 37° is 39.8 m out. So Deswik's road follows its walls as
  ours does. That one leg had a flatter face set on it.
- **Deswik's landing across a berm goes straight across,** 10.0 m. Ours goes a berm width on along the road, 13.5 m (0035).
- **Deswik joins a line it has moved round a road straight to the line's next vertex,** so its walls beside a road are
  flatter than 45° along the join. Matching that comes with switchbacks, as Tim decided.

## What this leaves

- **Switchbacks,** with Deswik's join. The second part.
- **The landing straight across a berm,** and a face angle set on a ramp itself, if Tim wants Deswik's way.
- **The grade on the pit-side edge on bends,** going down.
