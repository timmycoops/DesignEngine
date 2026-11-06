# 0046 — Edit the line and carry on

Date: 2026-11-06 · Status: accepted (Tim, 2026-11-06: "Edit by line", then "Go ahead" on the plan on the Step design plan
tab). Workstream 3 of the Step design plan, after 0045.

## Why

Two switchbacks side by side on one berm, as Tim built them on his step template (ramps A and B turning at RL 155), each
make the wall above go round their box, leaving a nose of wall about 70 m wide standing out between them all the way to
the top. Tim: "in a real design you would merge those switchbacks together at RL 155 so you don't have the small nose
on the design up into RL 175", and "in a manual design you will need to be more flexible". No rule should have to
guess that: the planner draws it, and the design keeps it. A "minimum nose" rule on berms, for designs on many strips,
comes later.

## Decision

1. **A line redrawn by hand is kept on the design's line,** not in the profile: `edits: [{ row, string }]` on a
   `FaceLine`, the row from 1 as the panel numbers them. The profile still runs over other strips without it. One edit
   to a row, and only on a line on one string. A design saved before this reads with none.
2. **An edit is the line a row ends on.** For a wall, its crest. For a berm, the toe of the wall above as the steps make
   it, roads and all (entrances, switchback boxes, the toe past a turn), or the next toe where the design ends on the
   berm. That's what the planner sees and copies, so a copy put back unchanged changes nothing.
3. **Your line, vertex by vertex, straight between.** Where it runs on the line the steps would make, it takes that
   line's heights, so a line the steps held off a grid stays held. Where it leaves it, each vertex lies on what the line
   lies on: the berm's floor (an RL, surface or grid), the wall's target, or for a berm that keeps its crest's heights,
   the crest. Draping it again added vertices along it, which put a toe that leaves a crest at a shallow angle a hair
   across it, and the berm folded.
4. **A berm's line redrawn:** the berm re-forms between the crest below and your line, one band, so switchback boxes on
   it join into one pad. The wall above starts from your line. A road on its way up whose start on the steps' toe is
   within 2 m of your line carries on from there as it is, with no entrance, turn or pad to make again: your line has
   them, or left them out on purpose. Its corners go into your line where it reaches them, at your line's heights, as
   they would into the steps' line. A road whose start isn't on your line goes in from the nearest place on it, as it
   would on the steps' line, and the run says how far off it was.
5. **A crest redrawn:** the wall below re-forms straight from its toe to your crest, one sheet, at whatever batter that
   takes. Not yet where a road climbs that wall, whose cut is built with the wall: the run refuses it and says to redraw
   the line after the berm above it instead.
6. **Refused, and nothing else tried:** your line crossing the line below it; standing past it on the pit side by more
   than 0.1 m (the usual conditioning: the steps' own lines stand that far past each other where a wall has no height);
   a berm or wall between the line below and yours that folds, with where; a string that's gone; a row that isn't there.
   Each says where. A new `GeomError::Refused` carries these, which the road pass hands straight back: before, a run
   that couldn't build took the highest road off and tried again, so a refused line stopped a road instead.
7. **The run says what each edit did:** "your line after row 8: the berm below it is 10.0 to 69.3 m wide", "the wall
   below it stands at 38.7°", and any ramp whose start was off your line. Widths and batters are measured square to the
   line below from your line's vertices and the line below's, so a straight line drawn across a pad reads its middle.
8. **The next toe at the end of a design says its row** in its `step` attribute, as wall lines do (0045), for the panel
   to edit it.
9. **A berm that folds says where it folds:** the first place its outline crosses itself, not its first point.

The pit-side check uses the crest of the wall below as that wall left it, before a switchback box rounds it: a crest
going round a box's road strip loops back on itself there, and a box's far end read as standing 25 m on the pit side.

## Results

Known answers, through the commands as the panel saves designs, toe on RL 100, 45 degree walls, 10 m berms:

- **A crest redrawn 5 m further back** than a 20 m wall reaches: that wall one plane at atan(20 / 25) = 38.66° from the
  toe to your line, 1,000 m long and 32.016 m up its slope, to 1 µm; the berm and wall above from your line.
- **A berm's line redrawn 15 m further out:** one flat bench 25 m wide, 25,000 m², and the wall above from your line.
- **A berm's line put back as the run made it changes nothing,** every line the same to 1 µm and the face the same height
  to 1 µm every 2 m: with no road, a road starting above with its entrance either way, crossing the berm after its top,
  going up through it, and turning back on a box or with no box.
- **A road across a berm redrawn 15 m out** goes in from the nearest place on your line, 15.05 m from where it would have
  started (the crest is conditioned to 0.1 m), crosses the wider berm and climbs on to RL 140.
- **Refused:** a line across the crest, one 5 m on the pit side, a crest with a road on its wall, a string gone, a row
  past the end. Nothing is saved.
- **Tim's step template** (bent toe, 30 m roads at 10 %, ramps A and B turning on boxes at RL 155): toe of the wall to
  RL 165 redrawn straight from the back of A's box to the back of B's. The walls to RL 165 and 175 run straight behind the
  boxes with nothing standing out, both roads climb from where they turned to RL 175, nothing crosses. 5 ms a step
  (release), 290 triangles.

Put back unchanged on Tim's template, every berm line comes back the same to 1 µm. The face is the same height but on
road A's surface on the wall to RL 145, where it differs by up to 1.1 cm: that road's surface is triangulated between
its edges, which are the same lines to 1 µm with vertices in other places along them.

The made-up grid pit (0034), strip 5 run as one line, release build:

| Design | Rows | No edit | Toe after row 2 put back | Triangles |
|---|---|---|---|---|
| Grid switchbacks | 7 | 861 ms | 918 ms, the same lines and face (10 m grid) | 18,311 |
| Grid ramps | 10 | 6.5 s | 10.9 s | 18,105 |

An edit costs one more build of the wall above it, to find where its roads start. Grid ramps' ramp B is held at its end,
so with your line in its start is found again and it moves, and the walls below with it: a road held at its end is
placed with your line, not before it. Step designs place ramps held at their start. On Grid switchbacks the same toe
moved 10 m out as a whole is refused where it comes within a few centimetres of a crest that juts out where the wall
below has no height: "between the line below and your line, the berm near 651340.2, 7450442.3 folds over itself".

## In the panel (workstream 3c)

- **✎ on each row of a step design** (Edit from here) copies the line that row ends on, as the last step made it, to a
  string of your own on the "edited lines" layer, "Toe Steps your line 8", puts it on the row and selects it. Nothing
  changes until you change it. Where a row can't be redrawn, the ✎ says why: a road climbs that wall, or there's no line
  yet. On a row with your line, ✎ shows in blue and selects it.
- **Your lines** under the table, one to a row: select it, **Redraw a stretch**, or take it off (its string goes too;
  undo brings it back). Drag, insert or delete its vertices in the viewport as for any string.
- **Redraw a stretch** is a command, `string.redraw_stretch`: click on your line where the new stretch leaves it, the
  new path, and where it comes back (Enter). The first and last points come onto the string within a metre, and the
  points between take the place of everything between those places, drawn either way, in one undo. Tested: a straight
  line with its middle redrawn out round 5 m keeps its ends and its heights there, drawn either way, and a line with the
  same vertex twice, as a toe can have at a box, redraws as well.
- **Your line changing runs the step again,** as the design changing does, 0.4 s after the last change. Rows moved or
  taken out take their lines with them.

The panel's flow on Tim's template, through the commands: Edit from here on row 8 runs in 6 ms and changes nothing;
Redraw a stretch from the back corner of A's box to the back corner of B's, then the step, 4 ms: the berm is 10 to
69.3 m wide, the walls above run straight behind both boxes, both roads reach RL 175, nothing crosses. It's saved as
`testdata/synthetic/Step template joined.zdproj` (git-ignored).

## On grids

Tim, 2026-11-06: "could you try this in some grids as well". Where your line leaves the line the steps made, a stretch
between two of your vertices now follows what it lies on, with a vertex wherever the floor bends (in line with its
neighbours in 3D, it adds nothing), as the steps' own lines on a floor do. Before, it ran straight between your vertices,
above or below a seam floor between clicks. Where it runs on the steps' line it still keeps that line's vertices and
heights. Tested: on a floor rising 1 in 10 to a ridge, a stretch moved out across the ridge gets one vertex on it at the
floor's height, to 1 µm, and the rest of the line keeps the steps' vertices, held 2 m off the floor where they were.

The made-up grid pit, strip 5 as one line, through the commands the panel uses (release build), saved as
`testdata/synthetic/Grid template edits.zdproj` (git-ignored):

| Design | Edit | No edit | Copied | Redrawn | Triangles |
|---|---|---|---|---|---|
| Grid walls | Row 3 (toe of the wall to M roof, on the M floor): a 40 m wider bench from 640 to 1,260 m, 45° ends | 708 ms | 734 ms, the same | 572 ms: berm 30 to 70.1 m | 18,608 → 17,379 |
| Grid switchbacks | Row 4 (toe of the wall to BOW round ramp C's box, on the U floor): the back of the box carried on 150 m, back at 45° | 865 ms | 1,049 ms | 1,057 ms: berm 10 to 110 m | 18,311 → 17,980 |

In section the bench lies on the M floor out to your line and the walls above project from it to M roof, U floor, BOW
and Topo; the box's pad lies on the U floor out to your line and the walls above stand back from it.

Tim's case on grids (Tim, 2026-11-06: "a switchback that needs to join, similar to the RL example"): Grid switchbacks
on strip 5 with a second ramp, D, climbing west from 3,100 m beside ramp C climbing east from 1,000 m, both turning on
boxes on the U floor before the wall to BOW, 50 m roads at 10 %. The wall to BOW goes round each box and stands out
about 170 m wide and 100 m deep between them. Row 4's line redrawn straight from the back corner of C's box to the back
corner of D's: one pad on the U floor, 10 to 110 m wide, the walls to BOW and Topo straight behind both, C to RL 141.1
and D to RL 133.8 as before, nothing crossing. 939 ms with no edit, 1,076 ms copied, 1,045 ms joined; 17,836 triangles
to 17,071. Saved as `testdata/synthetic/Grid template joined.zdproj` (git-ignored). D starting at 2,900 m stopped on
the wall to BOW, as its leg after the turn would have come within a road's width of C's (0042), with or without an edit.

Copied unchanged on Grid switchbacks' row 4, where ramp C turns on its box, the face differs by up to 7.6 cm next to
where the road leaves the box, and road C's wall-side edge starts 0.8 m off: carried on from your line, the road's
wall-side edge starts from your line's corner rather than its own offset. Its pit-side edge is the same: a turned road
carried on from your line keeps the corner its edges take across a pad that isn't level, which it lost at first.

## What this leaves

- **A crest with a road on its wall:** re-forming the wall and its road cut together.
- **Two switchbacks on one berm on a straight toe,** with both roads starting on the first wall: the second road's box
  folds the berm, with or without an edit (found while building the tests here; Tim's bent template builds).
- **A "minimum nose" rule on berms** for designs on many strips, Tim's other option on 2026-11-06.
