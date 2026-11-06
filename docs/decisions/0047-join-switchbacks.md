# 0047 — Join switchbacks on a berm

Date: 2026-11-06 · Status: accepted (Tim, 2026-11-06: "Yes, build Join"). After 0046 (edit the line), 0038 (switchbacks).

## Why

Two switchbacks side by side on one berm, each on its own box pad, leave a nose of wall standing out between them all
the way up (0046). Tim wants them as one pad, and 0046 let him draw that: ✎ on the berm's row, then take the line's
vertices out between the two boxes. In the app it didn't work for him, three times over on 2026-11-06:

- His Deletes went to the design's own line under his copy, then to a copy left from an earlier try (fixed in #86 and
  #87).
- His copy was made before the ramps turned on the wall above, so it had no boxes in it, and it stayed that way when he
  turned them.
- Dragging vertices onto one spot to shape the pad ran the wall above off its target.

A line of your own is a frozen copy: it has to be made after the rows and turns are settled, and joining needs the two
back corners exactly. For the job Tim wants, that's too fiddly. The planner's intent, "these two are one pad", is a
setting on the berm, not a drawing.

## Decision

1. **Join is a setting on a berm row** (`join` on `ProfileStep::Berm`, saved only when on; a design saved before it
   reads without it). The panel shows a Join switch on a berm where two or more ramps turn on box pads, and on one
   where it's on.
2. **What it does:** the line the berm ends on, the toe of the wall above as the steps make it, boxes and all, runs
   straight from the back of one box's far end to the back of the next one's, leaving out the far ends and the nose of
   wall between them. That's the line Tim drew by hand on his template on 2026-11-06.
3. **Each box is found from what the run knows of it,** not from the shape of the line: its far end, square off the
   crest (0038), and its road's top. Its back corner is the line's vertex on the far end furthest out from the crest.
   Neighbouring boxes along the line join when they face each other: the first runs on from its road's top towards the
   second, and the second back towards it.
4. **From there it's a line redrawn by hand** (0046), in the same code path: the berm re-forms between the crest below
   and the joined line; the roads on their way carry on from their boxes, which are on it; the walls above stand on it;
   and the same refusals hold. On a seam floor the straight stretch lies on the floor, as a stretch you draw does.
5. **A line of yours after the row comes first.** Join leaves the design as it is, and says why, where there's nothing
   to join:
   - only one ramp turning on the berm;
   - neighbours that face away from each other;
   - Join on a berm with no wall above it yet.
6. **The run says what it did:** "row 8 joins the switchbacks of A and B into one pad: the berm below it is 10.0 to 69.3 m
   wide".

And for lines of your own: where a ramp turns on the wall above a berm whose line you copied before it turned there, the
run now says so in place of "started 30.0 m from it": "ramp A turns on row 9 but its box isn't in your line, copied
before it turned there: take your line off and ✎ again, or switch Join on".

## Results

Tim's step template (bent toe on RL 100, 30 m roads at 10 %, A and B turning on row 9, boxes on row 8's berm at RL 155),
through the commands:

- **Join on row 8 gives the line drawn by hand, to 1 µm.** That line is the plain run's toe after row 8 with everything
  between the back of A's box and the back of B's taken out. Berm 10 to 69.3 m wide; both roads climb from their boxes
  to RL 175; no crossings in the face; 28 ms for 11 rows (debug build).
- **Only A turning:** "Join on row 8: only ramp A turns on it, so there's nothing to join it to", and the toe as the
  steps make it.
- **Join on the last row:** "Join on row 8: the ramps turn on the wall above a berm, so add that wall to join them".
- **Your line wins:** with a line of yours on row 8 and Join on, the run takes your line. Your line copied before A and
  B turned says, for each, that its box isn't in your line.
- **Saved:** `join` reads and writes as set, and a berm without it saves as before.

The made-up grid pit (0034), Grid switchbacks on strip 5 with ramp D beside ramp C, both turning on boxes on the U floor
before the wall to BOW (the 0046 demo), release build:

| Run | Time | Berm | Triangles | Crossings |
|---|---|---|---|---|
| Plain | 942 ms | | 17,836 | 0 |
| Row 4 redrawn by hand (0046) | 1,041 ms | 10.0 to 110.1 m | 17,071 | 0 |
| Join on row 4 | 977 ms | 10.0 to 110.0 m | 17,097 | 0 |

Both roads reach the same RLs as without the join, RL 141.1 and 133.8. Join's line is within 0.34 m of the line drawn
by hand there, whose ends were picked by eye at points 11 m and 18 m along the boxes' backs short of their far-end
corners; Join's ends are the corners.

**In the app** (`--bench --design-lines`): the template built with the commands and opened in the Face design panel
shows one Join switch, on row 8, titled "Join the switchbacks of A and B on this berm into one pad". A click runs the step
once: "row 8 joins the switchbacks of A and B into one pad: the berm below it is 10.0 to 69.3 m wide", both roads to RL
165, and the switch shows on.

## What this leaves

- **Join on a straight toe** meets the fold where two boxes on one berm on a straight toe fold the berm (0046), with or
  without Join. Tim's bent template and the grid pit build.
- **A minimum nose rule** for designs on many strips: Join is set by hand, row by row.
