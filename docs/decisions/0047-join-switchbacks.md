# 0047 — Join switchbacks on a berm

Date: 2026-11-06 · Status: accepted (Tim, 2026-11-06: "Yes, build Join"; joining by themselves within a road's width: "go for it"). After 0046 (edit the line), 0038 (switchbacks).

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

## Joining by themselves (2026-11-06)

Tim, on how it should feel: "They would only join by default if they overlap or are close to overlapping so the nose is
removed from the face. I feel it's a manual decision to make it work the way you need most of the time", and "one road
width is a good trigger by default".

Moving B's ramp towards A's on his template showed that close boxes didn't build at all before this. Join off, a box
gap of about 17 m or less folded the berm, or left "nothing" of it between them, and one road gave up. That's the case to
join by themselves.

1. **Join is one of three on a berm:** left out, the usual, the berm joins only facing boxes that would come within the
   wider road's width of each other along the crest, or overlap; on, every facing pair wherever they are (above); off,
   none, as before Join. A design saved with Join on reads as on; one saved before Join reads as the usual.
2. **Close boxes are shortened, then joined:** each box is shortened (or lengthened), sharing the difference, so the two
   stand a road's width and a metre apart along the crest, where the berm between them builds. Then the wall above joins
   them from the back of one to the back of the other, exactly as Join on does. Both boxes keep a turn: none is shortened
   below a third of a road's width. The gap moves along the crest, the nearest way, to keep both box ends 5 m clear of
   the crest's bends where it can.
3. **Tops too close to share a berm:** where boxes that short still couldn't stand a road's width apart, the later ramp
   in the list stops at its top on that berm and says how much further apart the tops need to be: two thirds of a road's
   width for the two boxes, the road's width between them and a metre. The other ramp turns there as it would alone.
4. **The run says so:** "row 8: the boxes of A and B would have been 10.3 m apart, under a road's width, so they're one
   pad" or "would have overlapped by 2.8 m", beside the join's own line with the berm's width; and for tops too close,
   "its top is only 43.1 m along the crest from ramp A's, too close for both to turn back on one berm: move one of them
   7.9 m further along the strip".
5. **The panel:** the Join switch shows the usual as it did off, with a tip saying the boxes join by themselves within a
   road's width (30 m on the template); a click turns Join on, another leaves it to the berm again. A berm saved off shows
   "Apart".

Meeting half way between the tops as one continuous pad was tried first and dropped. The road leaving each box shapes the
toe of the wall above round the box's far end, and two of them leaving one pad left a notch in it where the boxes met,
which folded the berm two rows up. Shortening and joining reuses the join that builds.

### Results

Tim's template, ramp B's string moved west by the distance shown, Join left to the berm, 11 rows (debug build, about
25 ms each):

| B moved west | Before (Join off) | Left to the berm |
|---|---|---|
| 0, 60 m | builds, nose between | the same: far enough apart |
| 75 m (20 m gap) | a road gives up: the berm folds | one pad, both roads to RL 175 |
| 80, 85, 88, 90, 92 m (15 to 3 m) | a road gives up | one pad, both to RL 175 |
| 96, 98 m (overlap 0.8, 2.8 m) | a road gives up | one pad, both to RL 175 |
| 94, 100, 102, 104 m | a road gives up | still: the berm folds by the bend in the crest near x 590 |
| 106, 112 m | a road gives up | B stops at its top: "move one of them 1.9 m / 7.9 m" |
| 130 m | B crosses A | the same (B's road would cross A's) |

Tests:
- 80, 85 and 98 m: both roads to RL 175, joined, no crossings; the gap reported shrinks one for one as B moves (5 m
  further west, 5.0 m closer, to 0.1 m).
- Boxes 102 m apart stand as they are, left to the berm or off.
- 85 m with Join off gives a road up as before.
- At 112 m B stops at its top, and the tops' distance plus how much further is 51.0 m, to 0.1 m.

In the app (`--bench --design-lines`) the switch's tip says the boxes join by themselves within a road's width (30 m),
and a click joins A and B 102 m apart.

## What this leaves

- **Join on a straight toe** meets the fold where two boxes on one berm on a straight toe fold the berm (0046), with or
  without Join. Tim's bent template and the grid pit build.
- **A box ending by a bend in the crest** can fold the berm, as at 94 to 104 m on the template, with or without Join (at
  70 m it did with no Join at all). Close boxes are kept 5 m clear of bends where they can be; the fold itself is in how
  a box's berm meets a bend, not in Join.
- **Boxes overlapping by more than a few metres** don't build: the tops are then too close and the later road stops,
  saying how far to move one.
- **A minimum nose rule** for designs on many strips: Join is set by hand, row by row.
