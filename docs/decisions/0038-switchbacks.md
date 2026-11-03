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
10. **After a turn the road runs flat a road width before it climbs** (Tim, 2026-11-03: "From the toe of a ramp we will
    need the ramp width at least of flat entrance - trucks cant turn that tight"). It comes onto the berm's line across the
    pad, runs along the line a road width, and climbs from there. Beside that stretch the toe of the wall above moves
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
before every leg started within 14 m.

## What this leaves

- **The flat entrance's length as a setting,** if a road width isn't always what's wanted; Tim's Deswik back strip has
  none.
- **A road's wall-side edge running out from a bent end,** as on Strip 1 of Grid switchbacks with a 30 m road and a 75 m
  join.
- **Fault 3 of 0034 after a turn on a seam floor,** as on Strip 2 or 4 of Grid switchbacks.
- **Where a taper ends,** if the planner wants it off the line at the string's end, as Tim's back strip has it (probably
  moved by hand, he says).
- **A join per turn,** rather than one for every turn a ramp makes.
- **A face angle set on a leg,** as Deswik's first leg has at 37°.
- **A turn with the end held,** and graded switchbacks with a radius.
- **The grade on the pit-side edge on bends,** going down.
