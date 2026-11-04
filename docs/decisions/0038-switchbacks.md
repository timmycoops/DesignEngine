# 0038 — Switchbacks on flat pads, and landings in line with the road

Date: 2026-11-03 · Status: accepted (Tim, 2026-11-03: "Yes, build them"; then "flat rls with a flat pad for a
switchback would be a standard terrace design for a coal mine and a metals design (metals may have non straight walls
though)"; on the toe past a turn, "the set joint length make sense from a default position"; and on making that length
a setting on each ramp, "Ok go for it"; on pads on grids, "On the seam floor"; and after trying it in the app, "From the
toe of a ramp we will need the ramp width at least of flat entrance"). The second part after 0037.

## Why

Tim's back strip from Deswik (PIT A, kept local) comes down eleven 45° walls of 30 m with 10 m berms, on a 40 m road at
10 % that turns back six times on flat pads. After 0037 our road came down it but couldn't turn, and crossed each berm a
berm width on along itself (0035), where Deswik's goes straight across. Tim's message makes this the standard terrace
design, in coal and in metals, where walls curve.

## Decision

1. **A wall row can turn a ramp back before it** (`turns` on the row, beside `ramps`). The road climbs to the top of the
   wall below, crosses the berm straight across from its wall-side top, and climbs this wall the other way, from the
   berm's line, as a fresh road with its own entrance. The pad is the two roads and the berm between them, level on an RL
   and on the floor on a grid (item 9). A
   turning ramp must be on this wall and the wall before; a ramp with its end held can't turn yet. A road that turns
   always crosses its berm after it, never through.
2. **A road after the berm crosses it straight across, in line with its top,** as Deswik's does, in place of 0035's
   landing a berm width on along the road. The road's top crossing is carried on out until it meets the berm's line,
   and the next stretch starts there. A road climbs at an angle to its wall, so its top runs a little back: 10 m across
   and a metre back at 10 % on 45°. Deswik's landings measure 10 m out and 1.3 m back.
3. **Where the road is as wide as the berm or wider,** the next stretch starts on the top of the last, the berm's line runs
   down that top, and there's no berm where the two roads meet: the road just carries on, shifted the berm's width. A
   gap under 0.5 m counts as meeting, since a berm that narrow between two walls of no height is no berm. Where the road
   is narrower, the berm closes in to the road's top by the road's width. Away from the road the berm keeps its full width.
4. **The berm's line steps round the road ahead of it, the way the road was going.** With a road as wide as the berm, the
   line behind and the line ahead are about as near the road's top as each other, so nearness can't tell which way to go.
5. **The sheet is split where two pieces meet part way along an edge.** A road wider than the berm starts its next stretch
   part way across the last one's top, so the triangles there are split at that point. Nothing moves.
6. **The Face design panel** cycles a ramp's cell off, on, then turning (a U-turn mark) where the wall before holds the ramp.
   A berm under a turn says so.
7. **Past a turn the toe of the wall above joins back a set length on.** It comes out to the wall-side corner of the next
   road's flat entrance (item 10), the pushed corner, and runs straight from there back to the line the ramp's join on
   from the turn, 50 m unless the ramp says otherwise, measured along the line from where the road comes onto it, then
   follows the line. Deswik joins the pushed corner to the line's next
   vertex instead, so its join runs wherever the string happens to have one: 9 to 94 m on Tim's back strip, which he says
   he moved by hand, and one grid cell on a line from a grid. A set length gives the same pad whatever vertices the string
   has. On its way the join can pass over a bend of the line out into the pad, as where the berm's line goes round the
   first road's top, if the stretch beyond the line is shorter than the road is wide; a longer one would cut the berm, so
   the join stops where it first meets the line. A join that only touches the line at a vertex goes out and straight back
   in, so it passes over too. This replaces pushing the whole line behind out by the road, as an entrance does, which
   carried the pad to the string's end.
8. **The join is a setting on each ramp** (`turn_join`, in metres, more than zero), for every turn that ramp makes. A
   design saved before it takes 50 m. The Face design panel shows it on a ramp that turns somewhere. A join longer than
   the line goes on past the turn runs straight from the corner to the end of the string, which is how to taper the toe
   to the strip's end as Tim's back strip does.
9. **On a berm registered to a grid the pad lies on the floor** (Tim, 2026-11-03: "On the seam floor", over a level pad
   with coal left under it). The road tops out on the crest as anywhere and crosses its own top level, a road width, to
   its wall-side corner; from there the pad is the berm, lying on its grid, and the next road starts on the berm's line,
   on the floor. Round a turning road's top the crest stands above the grid only because the road's top is level
   across, so the berm's line there isn't held up to the crest's height as it is elsewhere a crest stands above its
   target; it lies on the grid. On a seam floor dipping into the wall the pad falls across the berm, and the next road
   starts lower and climbs further. On an RL the floor is level, so the pad is too, as before.
10. **After a turn the road runs flat before it climbs** (Tim, 2026-11-03: "From the toe of a ramp we will need the ramp
    width at least of flat entrance - trucks cant turn that tight", then "Yes, a setting"). It comes onto the berm's line
    across the pad, runs along the line as far as the ramp's `turn_flat` says, its width unless it says otherwise, and
    climbs from there; 0 climbs straight away, as Deswik's does. The join past the turn is measured along the line from
    where the road came onto it, however long the flat stretch. Beside that stretch the toe of the wall above moves
    back as far as the road's wall-side edge starts from the line, so the flat road is cut into the wall at the pad's
    level. Its pit side follows the line, on the floor, which runs near enough along strike; its wall side stands level
    across the road with it, as the road's own ends do. The first stretch of a ramp needs none: trucks come along the
    floor behind its entrance and drive straight on.
11. **Where a wall crosses a target on its way, its line goes round the roads in it.** Steps at one batter with no berm
    between them are one wall, and the targets on the way only mark where it crosses them, as at a seam's roof. That line
    was drawn from the wall's plain toe, as if there were no road, so round a road's entrance and past a turn it crossed
    the toe of the wall above it. Now the roads that went into the wall are cut into it up to each target too, so the
    line goes round their entrances and flat stretches and across them where they pass that height, and stands on the
    same toe as the wall it's part of. A road that can't be cut to the target leaves the plain line, as before.

Where the berm's line can't be read this way, the road falls back to 0035's landing, as before.

## Results

Known answers, all exact to 1 µm unless noted:

- Two 45° walls with a 10 m berm and a 10 m road at 10 % after the berm: 400 m of climb, one landing across the first
  road's top, 10 m across and 1 m back, and the berm 10 m wide everywhere but the roads' own corners. Roads of 4, 9.8,
  10.2, 25 and 40 m on the same walls each give one clean sheet with the berm whole. The landing is 10.05 m except where
  road and berm are within 0.5 m of each other's width.
- The same going down, mirrored.
- A switchback: 400 m of climb, one pad, the second road starting a road width and the berm out from the first road's
  top, square to the crest, and running back west to RL 140. The pad is level at RL 120.
- A wall curving round a centre 600 m away, as a metals pit's does, with a road turning back twice up three walls: 600 m
  of climb, two pads, RL 160 at the top, one clean sheet with nothing crossing.

Tim's back strip, our road with Deswik's six turns, where each leg's wall-side edge starts and ends against Deswik's:

| | Before (45° landings) | Now |
|---|---|---|
| Leg starts and ends off Deswik's | 2.7 to 24 m | 0 to 16.3 m (starts 0 to 13.7 m) |
| Landing across a berm | 13.5 m along the road | 9.9 m out, 1.1 m back (Deswik: 9.9 m out, 1.3 m back) |

The face is one sheet with nothing crossing, road 3,639 m (561 triangles out before the join below). What's left is Deswik's
first leg, set to a 37° face, which puts it 10 m further out, carried down every leg below. On top of that, Deswik's legs
heading west are about 1.5 m shorter along the strip than ours, so the along-strip gap grows by a metre or so per turn.

The made-up grid pit (0034), against the commit before this one:

| Design | Faults before | Now | Roads to the top before | Now |
|---|---|---|---|---|
| Grid ramps | 9 roads stopped | 7 | 11 | 13 |
| Flat toe to grids | 2 | 2 | 18 | 18 |
| Grid toe to RLs | 3 | 3 | 17 | 17 |
| Whole pit | 30 faults | 28 | | |

Every road still stopped stopped before too, at the old "a wall reaches past the target with no crest on it". Grid
ramps runs in 10.9 s against 9.7 s. The ramp template's five strips are clean, each road 1,200 m up and 40 m across its
two berms.

The toe past a turn, known answers exact to 1 µm: with a 10 m road and with a 25 m road, the join meets the berm's line
50 m on from the turn along the road, at the same point whether the string has vertices only at its ends, one 9 m past
the turn, or one every 25 m; past it the toe is the berm's line, the pad inside is level, and the face is one sheet. A
line that jogs 15 m back into the wall just past the turn is joined 50 m on as a straight one is; one that jogs 15 m out
towards the pit stops the join where it first meets the line, before the jog, so the berm keeps its width.

On the back strip the face stays one sheet with nothing crossing: 11 walls, 509 triangles, 29 ms, the road unchanged at
3,639 m. Each turn's corner stands about 80 m back from the line, a road width for each road, so each join climbs 80 m in
50 m along; below a turn the walls carry the join down, so the lines past the lower turns step back in a stair of joins.
Tim's own lines past the east turns taper out to the strip's end instead, some 700 m, so there our walls below the turns
sit up to 80 m behind his.

The join as a setting, known answers exact to 1 µm: joins of 20, 50 and 90 m, with 10 m and 25 m roads, each meet the
berm's line as far on from the turn as they say, at the same point whatever vertices the string has. At 20 m with a 10 m
road the join passes exactly through the vertex where the berm's line steps round the first road's top, only touching
the line there; it goes on to 20 m. A 5,000 m join on a 2 km toe runs straight from the corner to the string's end, with
the pad under it level. Through the commands, a ramp saved without a join and one saved with 50 m give the same toe, and
one saved with 30 m meets the line 20 / cos(5.74°) = 20.10 m nearer the turn.

On the back strip with the join set past the strip's end, the face is one sheet with nothing crossing: 11 walls, 333
triangles, 29 ms, the road unchanged at 3,639 m. The line past each turn now tapers straight from the corner to the end
of the string, as Tim's does: some 800 m past the west turn at RL 50, 640 m past the east turn at RL -40. Where the
tapers end is what's left: ours ends on the berm's line, so the berm is 10 m again at the strip's end, and Tim's ends
50 to 60 m further out, a berm some 60 m wide at the end at RL 50 and 100 m at RL -40. The walls below carry that down,
so below the west turn our lines sit up to 60 m behind his at the strip's end, against 78 m with a 50 m join.

A pad on a seam floor, known answers exact to 1 µm: two 45° walls and a 10 m berm on a floor dipping 5 % into the wall,
with a 10 m road turning back on it. The first road tops out at RL 120 on the crest, crosses its top level to its corner
a road width across, and the second road starts on the berm's line on the floor about a metre lower, climbing at 10 %
from there to RL 140. Every vertex of the berm's line lies on the floor but the second road's own corner, which is level
with its start across the road. Holding the line up as before starts the second road at RL 119.59 instead of 119.005.

The made-up grid pit (0034) has two designs for this, with a ramp C turning back on every berm. RL switchbacks: 9 of 10
roads reach RL 140 with three turns each on level pads at RL 50, 80 and 110, every face one sheet, 0.11 s for ten strips;
Strip 5's road stops on its first wall, before any turn, at fault 3 of 0034. Grid switchbacks, with the walls finishing
on the seam grids and the pads on the M floor, the U floor and the base of weathering: every road holds 10 %. Held up
level, the pads sat across the M seam up to 5.5 m above its floor, the berm's line climbing 3.6 m in 26 m beside them
and the M roof wall with no height under each second road's start; all 10 roads reached the topography. On the floor,
each pad is crossed level over the first road's top and then down the berm at 12 to 16 %, 6 to 8 % over the 60 m, on the
M and U floors; on the base of weathering, which is nearly flat there, within 1.3 %. The walls of no height at the turns
are gone. Strip 2's road now stops after its second turn on the BOW wall, at fault 3 of 0034, so 9 of 10 reach the
topography. Strips 1 to 3 aren't one sheet, as in Grid walls with no roads. Tim's back strip, on RLs, is unchanged: 509
triangles with a 50 m join, 333 with the join past the strip's end, the road 3,639 m. The test pit's check now counts a
road that climbs some walls and then stops as stopped; before, one that climbed more than one wall counted as reaching
the top. No other design's count changed.

The flat entrance and the lines on the way, known answers exact to 1 µm: on the level pad the second road comes onto the
berm's line, runs exactly a road width (10 m) along it at RL 120, and climbs its 200 m from there; the flat stretch's
wall-side corner stands a road width across from where it came on, and the join is 50 m on from there along the line.
Joins of 20, 50 and 90 m with 10 m and 25 m roads still land where they say, whatever vertices the string has. On the
floor dipping 5 % the flat stretch is 10 m along the line, within 1 % of level, its wall side level across with it.
Walls carried straight on through RL 110 and RL 130, with a road and a turn: the line at each RL is at that RL
everywhere, stands on the same toe as the wall above, and crosses neither that toe nor the crest above; drawn from the
plain toe it doesn't stand on it. Through the commands a 30 m join meets the line exactly 20 m nearer the turn than a
50 m one, and the second road runs 10 m flat at RL 110.

On the grid pit, as Tim ran it in the app, a 50 m road and a 75 m join: Strips 2, 4 and 6 stopped before; now 9 of 10
roads reach the topography, Strip 2's stopping at fault 3 of 0034. With the template's 30 m road and 50 m join, 9 of 10,
Strip 4's stopping at fault 3. Every RL switchbacks face has no lines crossing. In Grid switchbacks the lines that
crossed round each road start and turn are gone; what's left are walls of no height: at the washout and where U runs
into the weathering, as Grid walls has with no roads at all (9 pairs), and on Strips 1 and 8 with a 50 m road, where the
flat stretch's wall side stands some 2.5 m above a floor dipping into the wall, more than the U seam is thick, so the U
roof line runs onto the toe and touches it. One face is wrong: with a 30 m road and a 75 m join, Strip 1's last road, on
a Topo wall 21 m across with a 30 m road after a turn where the minimum bench drops the berm, has its wall-side edge bend
back to its pit side at the top, and the run-out from that bent end goes 57 m the wrong way: 6 pairs of triangles cross,
and the face says so. With a 50 m join, or a 50 m road, or no flat stretch, it doesn't happen.

Tim's back strip with the flat entrance: one sheet, nothing crossing, 555 triangles with a 50 m join and 433 with the
join past the strip's end, the road 3,854 m against 3,639 m, each of its six turns adding a 40 m flat stretch. Tim's
Deswik turns have none, so the legs after the turns heading east now start 31 to 40 m further along than his, where
before every leg started within 14 m. With the ramp's flat set to 0 it's as it was: the road 3,639.3 m, every leg
starting within 0 to 13.7 m of Deswik's, one sheet, 504 triangles.

The flat as a setting, known answers exact to 1 µm: on a 10 m road turning on a level pad, flats of 0, 10 and 25 m each
run that far along the line at RL 120, then the road climbs its 200 m, finishing that much further on, and the toe past
the turn joins back 50 m on from where the road came onto the line each time. A ramp that doesn't say runs flat its
width, one saved before it could say does too, and less than zero is refused. Through the commands a 25 m flat runs
25 m at RL 110 and leaves the join where it was.

## What this leaves

- **A road's wall-side edge running out from a bent end,** as on Strip 1 of Grid switchbacks with a 30 m road and a 75 m
  join.
- **Fault 3 of 0034 after a turn on a seam floor,** as on Strip 2 or 4 of Grid switchbacks.
- **Where a taper ends,** if the planner wants it off the line at the string's end, as Tim's back strip has it (probably
  moved by hand, he says).
- **A join per turn,** rather than one for every turn a ramp makes.
- **A face angle set on a leg,** as Deswik's first leg has at 37°.
- **A turn with the end held,** and graded switchbacks with a radius.
- **The grade on the pit-side edge on bends,** going down.

## Fixed since: a top crossing at a skew (2026-11-04)

Tim found a flap past the top crest of the made-up grid pit's Grid ramps, Strip 1, where road A comes out. On a seam
rising into the wall, a road's wall-side edge runs on past the end of its inside edge to reach the seam, so its top
crosses the road at a skew, the wall-side corner ahead. The crest just behind that corner was then as near the inside
corner as the corner itself, and of the two crest segments at the corner the crossing was taken for the one behind the
road (item 4). The road's heading came out across the berm, so the step in the berm's line was taken at its top, not its
foot, and the next toe zigzagged back over the step. The next stretch followed it, going 37 m in toward the wall and
29 m back, and its wall-side edge folded out about 90 m, carrying the wall above and the crest with it. A segment lying
along the crossing is now never taken for the crest behind.

Known answer: on a seam rising 5 % into the wall, roads of 10 m and 20 m after a 20 m berm climb on along the next
wall, their inside edges never turning back toward the pit and their wall-side edges never more than a road width off,
one sheet with nothing crossing; before, the inside edge turned back. On the grid pit, folds go from 2 to 0 in Grid
ramps and from 1 to 0 in Flat toe to grids, with roads of 20, 25 and 30 m, after the berm or through it; the pit's own
check stays at 33 faults, and the other five designs and Tim's back strip are unchanged.

## Fixed since: the face on a pad on a seam floor (2026-11-04)

Tim couldn't tell in the app whether the face followed the road strings through Grid switchbacks' turns. It didn't. On
every turn on a seam floor the road's edges were off the face: with the template's 30 m road, by up to 0.7 m on the pit
side and 2.4 m on the wall side; with a 50 m road and a 100 m flat, by 2.4 m and 3.7 m. Three things did it:

- **The berm under a pad ran as one band** from the crest to the toe of the wall above, and beside a flat stretch that
  toe is the road's wall side. The berm's line, which the flat stretch's pit side runs along on the floor, wasn't in it,
  so the level road wasn't there: the face sloped from the last road's top past the road and down toward the floor.
  Now the berm runs out to the line beside the flat stretch, from the last road's wall-side top straight across to where
  the next road came onto the line, and the flat stretch is a piece of its own between the line and its wall side. Past
  the turn the berm runs out to the toe as before.
- **The toe past the turn lay on the floor right up to the flat stretch's wall-side corner,** which is level with the
  road's start and so stands above a floor dipping into the wall: the toe stepped up 2 m in under a metre there, and the
  walls above it bent round the step. Now it lies on the floor to a road width short of the corner and leads straight up
  to it, as an entrance's toe does. Where the corner is on the floor, as on any level pad, nothing changes. This leaves
  a wedge above the floor past the turn, no thicker than the road's wall-side corner stands above it, for Tim to confirm
  against "no coal left under the pad".
- **The wall-side edge crossed the end of the pad in one straight line** from the last road's wall-side top to the next
  road's wall-side corner, up to 2 m above the pad where it bends at the berm's line. It now passes where the next road
  came onto the line, as the pit-side edge does.

Known answers, two switchbacks on two seam floors dipping 5 % into the wall, one turning each way: both road edges lie on
the face everywhere to 1 µm, the berm beside each flat stretch lies on the floor to 1 µm, the toe past each turn reaches
the corner straight from the floor a road width short of it, exact to 1 µm, and the wall-side edge passes where each
road came onto the line. Across the flat stretch the face keeps within the heights the line has along it, here within
21 mm, where it bends off strike near the turn; where the line runs along strike the road stands 0.125, 0.25 and
0.375 m above the floor 2.5, 5 and 7.5 m into the wall. Each of the three changes, switched off, fails it.

On the grid pit, Grid switchbacks with the template's settings, a 50 m road with no flat, and a 50 m road with a 100 m
flat: every road edge on every strip is on the face. The faces change only round the turns, where the walls above each
pad corner now meet on a straight hip from it, moving the crests above by up to 7.5 m in plan; no triangle faces down
and nothing crosses. RL switchbacks and Grid ramps keep the same surface to 0.01 m³ under every face (RL switchbacks has
72 more triangles, the pads in pieces). The pit's own check stays at 33 faults, with the same strips stopping as before
(fault 3 of 0034 and, with a 100 m flat, fault 1), and Grid switchbacks still takes 2.2 s for ten strips. Tim's back
strip, on RLs: the same roads, 3,853.7, 3,639.3 and 3,865.3 m, nothing crossing, 567, 504 and 449 triangles against
555, 504 and 433.

## Changed since: the flat is on the road coming up (2026-11-04)

Tim looked at the top of a ramp in the app and found it weird: the flat was on the next road, running back along the
berm's line above the road below before it climbed. "What I'm expecting is the ramp flat pad would actually be in front
of the ramp start, not move the flat to the other side." Asked where, he chose the flat on the road coming up, and on a
coal seam: "we will have to match the switchback to the floor grid. Otherwise we will end up with coal in the wrong strip
because of the dip." Item 10 changes:

- **The road coming up runs flat past its top** before it turns, on along the crest the way it came, `turn_flat` metres,
  a road wide on the wall side of the crest: its pit side is the crest itself and its wall side the crest moved back by
  the road. It lies on what the berm lies on, so on a seam floor it's on the floor, dipping into the wall with the seam,
  and no coal is left under it. The berm stands its full width out beyond the flat, round it as round a road's top.
- **The next road climbs straight away,** from straight across the berm from the flat's end. Nothing sits back over the
  road below.
- **The join past the turn** is measured from where the next road comes onto the line, as before, so it moves on with the
  flat.
- **A flat on a floor steeper along than the road's grade** is reported on the ramp's line, with where it ends and how
  steep it goes: under Tim's single turn on the Topo wall the base of weathering falls 10.5 to 12.5 % along it.
- **With no berm since its last wall** a road turns on the crest, as before, with no flat: there's no berm for one to lie
  on.

The next road's climb stays level across, as every road is, so on a floor dipping into the wall its wall-side start stands
above the floor by the dip over its width (0.5 m for a 10 m road at 5 %), and the toe past the turn leads up to it over a
road width, as #74 has it. Tim was told and didn't ask for that start on the floor.

Known answers, all exact to 1 µm unless they say:
- **Level pad, 10 m road and flat:** the flat ends 10 m on along the crest at RL 120. Its wall side is 10 m into the
  wall, and the next road comes onto the line 20 m in from the crest, straight across from the flat's end, and climbs
  from there 200 m to RL 140. The join is 50 m on from there along the line.
- **Flats of 0, 10 and 25 m:** each ends that far on. With none, the next road is in line with the road below's top
  crossing.
- **On a floor dipping 5 %:** the flat's pit side is on the crest at RL 120 and its wall side on the floor 0.5 m lower. The
  face across the flat and the berm beyond is the floor. The next road starts on the floor at RL 119 and climbs 210 m.
- **Two switchbacks on two dipping floors, one turning each way:** both road edges are on the face everywhere, and each
  flat and berm is on its floor.
- **A floor falling 12 % along the strip:** the flat follows it, 11.91 % along a crest running off square by 12 in 100,
  and the road reports it.
- **Joins:** an inward jog 16 m past the flat's end gets its join on the jogged line, and a bump into the wall makes a
  150 m join stop where it meets the line round the bump.
- **Through the commands:** a flat of 25 m moves the join 15 m on from a 10 m one.

On the grid pit, Grid switchbacks with the template's settings now gets all 10 roads to the topography, against 9, and
the pit's own check goes from 33 faults to 32. Every road edge is on the face with a 30 m road and its flat, a 50 m road
and no flat, and a 50 m road and a 100 m flat. With a 100 m flat, Strips 1, 2, 4 and 9 stop, against Strip 3 before.
Tim's seven-row design with one turn on the Topo wall and a 50 m flat gets 7 of 10 to the top, against 3. Turning on
every berm with a 50 m flat, it gets 9 of 10.

Tim's back strip on RLs is the same road with no flat (3,639.3 m, 504 triangles). With a flat its width the road is
3,904.9 m against 3,853.7 m, each of its six turns now running flat on the road coming up, 900 triangles, nothing
crossing. Grid switchbacks takes 2.1 s for ten strips.

## Changed since: the switchback is a box (2026-11-04)

Tim looked at a 100 m flat in the app and asked what it meant: the road ran flat 100 m along the crest and only then
turned. "The ramp should be the 10% up to the surface and the flat section should be after that (for trucks to turn
around), then for a switchback it would need to align to the ramp going up to the next seam in the opposite direction."
He drew his standard switchback as a white box on a design: "the ramp comes to the top, a flat box is built, offset is
added after the ramp and then the other ramp is heading opposite direction up." Then: "there may also be a berm in that as
well, so the new ramp would be after the berm as well, which makes it wider (if the berm is in the rule)." Item 10
changes again:

- **The road ends at its top.** Its edges no longer run on along the flat.
- **The box runs on past the top,** in line with the road, `turn_flat` metres (the road's width unless the ramp says), on
  what the berm lies on, so on a seam floor it's on the floor. It's three strips wide: the road's width along the crest,
  the berm beyond it, and the next road's width beyond that.
- **The next road starts at the box's near end,** straight across the berm from the top, on the line nearest the road's
  wall-side top, and climbs the other way at once. Its start is the same whatever the box's length.
- **The wall above goes round the box.** Its toe runs from the next road's wall-side corner along the box's back to the
  far end, square across from the end of the box on the crest, then down to the berm's line, which goes on from there.
  The berm stands out round the box's first two strips as round a road's top.
- **The join** is used only with no box (`turn_flat` 0): then the road turns at its top, the next road starts in line with
  the top crossing, and the toe past the turn joins back the ramp's join on, 50 m by default, as before.
- **A box on a floor steeper along than the road's grade** is still reported on the ramp's line.

Known answers, all exact to 1 µm unless they say:
- **Level pad, 10 m road, berm and box:** the road climbs 200 m from RL 100 to 120 to its top. The next road comes onto the
  line straight across the berm from the top, 10 m from the road's wall-side top to 0.01 m, and climbs back 200 m to RL
  140. The box is flat at RL 120. Its far end is 10 m on past the top, and its back corner 10 m out from the line.
- **Boxes of 0, 10 and 25 m:** each far end is that far on. With a box the next road starts in the same place whatever
  its length. With none it starts in line with the top crossing, and the join is 50 m on.
- **On a floor dipping 5 %:** the box's pit side is on the crest at RL 120, its wall side on the floor 0.5 m lower, and the
  berm beyond lower again. The next road starts on the floor at RL 119 and climbs 210 m to RL 140. Every vertex of the line
  the second wall stands on is on the floor, but the next road's own wall-side start, level with its start across the
  road. The box's back leads up to it from the floor over the last road width.
- **A skewed top on a seam rising 5 % into the wall:** the box runs on the way the road came, on the seam 5 m on past the
  top, and square across from the end of it on the crest 10 m on, not back over the road below.
- **Through the commands:** with no box, joins of 50 m (and none) and 30 m meet the line 20 m apart. With boxes of 10 and
  25 m the toe comes down to the line beside the box, a road width further out, at the box's far end that far on past the
  road's top.

### Fixed on the way: a bevel with no width

On the grid pit the box moved Strip 2's third road to start 10 m nearer the turn, and its wall would not build: "a wall
reaches past the target with no crest on it". The road's wall edge ends in two stretches all but in line, climbing at 10 %,
with batters a millionth apart as the road leaves them. The bevel between them has no width, and the crest walk steps
straight over it, as it has since #61, but the wall's mesh still asked it for a crest. Now a transition with no width that
the walk stepped over builds nothing. Known answer: those five vertices and batters as the run had them, with a target
rising 2.25 % east to meet the string's end: every crest point is on the target to 1 mm, the crest ends on the string's
end, and the wall is one sheet with nothing crossing.

The same fault stopped ten other roads on the grid pit. Grid ramps now gets 20 of its roads to the top against 13, Grid
toe to RLs 19 against 17, and Grid switchbacks all 10. The pit's check goes from 32 faults to 23 (from 32 to 23 with #72
to #75 as well). Three new ones show where roads now get further:
- Grid ramps on Strip 1 and Grid toe to RLs on Strip 8 have triangles crossing where a road as wide as its berm crosses it
  straight and starts its next stretch on its last one's wall-side top. There the berm's crest and its next line touch at
  a point.
- Ramp A on Grid toe to RLs, Strip 7, stops later than it did, at a berm fold at the strip's west end.

Tim's seven-row design (turns on the U floor and Topo walls, a 50 m road, a 100 m box, a 10 m minimum bench) gets 8 of 10
roads to the top. Strips 1 and 2 stop at the second box. The minimum bench drops the berm west of the box, so along the
box's far end its line has no width, but beyond the box the berm is back to full width: the line runs through the box's
back corner and the band folds there. #75 doesn't reach this, since the change of width is at a corner. Whether the berm
should wrap round the far end of a box at all, or the far end be one straight wall across the road, the berm and the next
road, is for Tim's strings to settle.

Tim's back strip on RLs gets its road to RL -190 through all six turns with a box its width: 3,635.1 m, 848 triangles, one
sheet, nothing crossing, 23 ms. With no box it's as before, 3,639.3 m and 504 triangles. Grid switchbacks takes 2.1 s for
ten strips and 177,185 triangles.

## Changed since: the box squared off at its far end (2026-11-04)

Tim looked at the box in the app and asked where the flat is measured from. It's measured from the road's top, where its
edges reach the crest, along the crest. The box's road and berm strips then had the berm standing 30 m on round their far
end, because the box was put into the crest and the berm went out from all of it. Tim: "why is it 30 m berm but in the wrong
direction?" Asked whether the far end should be one straight wall across all three strips, he said "Yes, square it off".

- **The berm stands out into the wall from the crest, beside the box and past it, never round its end.** The berm's line
  beside the box runs only as far as the box's far end carried on square across. From there it comes straight down the
  far end to the berm's line past the box, the one there'd be with no box, and goes on along that
  (`Flat::squared_end`). Where the two run along the crest's own far end, the line takes the crest's points, so the berm
  splits there with no slit.
- **The wall above** goes round the box's back and straight down its far end, one straight wall across the next road,
  the berm and as much of the road strip as stands past the berm's line.
- **Where the minimum bench drops the berm** along the far end only, the line beside the box comes down onto the far end
  and runs down it. Where it drops it all along the box too, the box is the two roads with nothing between. Either way the
  far end is square. This is what folded Strips 1 and 2 of Tim's seven-row design.
- **The box's far end is laid on the floor** with its sides, so on a seam floor the box, its far end and the berm past it
  are all on the floor.

### Fixed on the way: a wall that closes just on the target

The westbound known answer below left a hole in the wall above: a stretch of wall closing exactly at RL 140 had its crest
only touching it at that point, and was dropped. A wall whose crest only touches it where it closes is now built whole,
as one closing lower is, with the crest's own point there so it welds to its neighbours.

Known answers, exact to 1 µm unless they say:
- **Level pad, 10 m road and berm, 10 m box:** the toe comes down the far end from the line beside the box, 20 m out from
  the crest, to the berm's line past it, 10 m out. 5 m past the end and 15 m out the wall above is at RL 125, not berm.
- **20 m road, 10 m berm, 25 m box, climbing east and climbing west:** along the far end the toe has the back corner 50 m
  out, the line beside the box 30 m, the box's wall-side corner 20 m and the berm's line past it 10 m, and nothing past the
  far end above that. Flat at RL 120 in the box and on the berm past it. 2 m past the end it's RL 122, 15 m out and 40 m
  out. One sheet, nothing crossing.
- **A berm the minimum bench drops everywhere (30 m least over a 20 m wall), 25 m box:** the toe along the far end has the
  back corner 20 m out, the wall-side corner 10 m out and the crest, and runs along the crest past the box. Flat in the box,
  RL 122 2 m past its end. Nothing crosses. That face isn't one sheet, as it isn't with no box or turn at all: with the
  berm dropped all along on an RL, the crest and the toe on it have their vertices in different places. That's still open.
- **Through the commands:** with boxes of 10 and 25 m the toe comes down the far end to the berm's line past the box, and
  nothing stands past the far end above it.
- **A wall closing just on the target:** a toe turning north at (100, 30) with vertices at 40 and 50 m up the far end, a
  45 degree wall 20 m high. The nose's hip meets the target just where the stretch from 40 to 50 closes. The wall's area is
  its plan area √2 times over, to 1 µm², and it's one sheet. Without the fix it was 150 √2 m² short.

Results:
- **Tim's seven-row design** (turns on the U floor and Topo walls, 50 m road, 100 m box, 10 m minimum bench): all 10
  roads reach the top, against 8. No face has triangles crossing. Every face has the same holes as before the square-off
  (2 to 9 small slits a strip, which predate #76), but Strip 2, whose third road now gets to the top.
- **His single turn on the Topo wall with a 50 m box:** 3 of 10 reach the top, as before #76. The flat on the road coming
  up had moved the next road 50 m along, which got 7. The box starts it straight above the top, and on Strips 1 to 7 the
  floor under the Topo berm rises 10.5 to 12.5 % the way it climbs, faster than its grade: it says so.
- **Grid pit:** still 23 faults. Grid switchbacks and RL switchbacks get all 10 roads to the top. Grid switchbacks takes 2.9
  s for ten strips (179,172 triangles); the berm's line with no box is offset once more at each berm with a box.
- **Tim's back strip on RLs:** the road reaches RL -190 through all six turns with a box its width, 3,635.1 m, 694
  triangles, one sheet, nothing crossing, 23 ms. With no box it's as before, 3,639.3 m and 504 triangles.
