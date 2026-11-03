# 0041 — Roads carry on from the crest they came out on

Date: 2026-11-04 · Status: accepted (Tim, 2026-11-04: "either attributes for the ramp crests", then on the plan, "Okay
start the plan", and on a crest a run made keeping its heights, "Yes, keep them"). After 0038.

## Why

Tim designs bottom up: one design to a crest, then a new design from that crest as its line, with its own berms, turns
and ramps. A road didn't carry on into the next design. He had to draw a new ramp string where the road should start, and
it started with an entrance as if from the floor. The road's top, its heading and its width were in the design below and
nowhere else.

## Decision

1. **A crest records each road that came out on it,** as attributes on that crest string:
   - `road_top A pit` and `road_top A wall`: the road's top corners, as "x y z" written exactly.
   - `road_top A heading`: `along` or `back` against the string, the way it was going at its top after any turns.
   - `road_top A width` and `road_top A grade`: as built (m, %).

   It goes on the crest of the last wall the road climbed. A road that stopped short records nothing. Nor does a design
   going down, for now.
2. **What a run makes from a string doesn't carry that string's road tops on.** Everything else on the string still
   carries on (0029). The design above has carried those roads on or left them, and its own crests record its own.
3. **A ramp can carry on from the line** (`fixed: line` in the profile, "From line" in the Face design panel). It has no
   string. On each line it starts from the top that its name left there, heading the way that road was going, at the
   ramp's own width and grade. It doesn't make an entrance, because the road is already on its way. From there it does
   what the road would have done in one design: it goes after or through the berm, turns where a row says, and stops and
   reports where it can't go on. The run says so:
   - where a line has no top for it;
   - where the top is more than 2 m (the most conditioning moves a line) off the line, in which case it carries on from
     the nearest place;
   - where the ramp isn't on the profile's first wall, in which case it starts afresh on its own first wall from the
     nearest place to its top, with an entrance.
4. **The toe takes the road's top back in, level across the road,** at the corners' own heights. A road's top is level
   across, so on a seam dipping across the road its wall-side corner stands above the floor: 1.6 m across a 30 m road
   at 3°. Laying the toe on the floor again dropped that corner, and the whole road with it.
5. **A crest another design's run made keeps its heights** when it's a line, and only its run-on lies on the grid the
   profile registers to. That crest is already where its design left it: on its grid, above the grid where the wall
   below had no height, and level across a road's top. Registered again, it dropped off what's below it by up to 2 m
   on the made-up grid pit, and the berm beside it with it. The register row still sets what the run-on and the first
   berm lie on. This applies to every design run from a crest, with or without roads: it's what makes two designs give
   the same face as one (Tim, 2026-11-04: "Yes, keep them").
6. **A berm with no wall below it in its own design keeps its full width,** whatever its minimum bench. As the first row
   of a design from a crest, it had always been dropped, because the toe under it had no height.
7. **A ramp from the line may turn back on the profile's first wall,** as the wall before it was in the design below.
8. **In the Face design panel,** a crest another design made can be picked as a line with Apply to selected; before,
   anything a run made was kept out. "From the line" adds a ramp for each road top on the profile's lines that the
   profile has no ramp for yet, up every wall, with the width and grade the road was built at.

## Results

Known answers, a design split in two at a crest against the same design run whole, toe on RL 100, 45° walls of 20 m,
a 10 m berm, a 10 m road at 10 %. Every vertex of both road edges, every toe and crest, and the sheet's height at
over 10,000 points agree to 1 µm. Tested:

- going up through the berm and after it;
- running along the string and back;
- turning back on a flat pad on the berm before the second design's first wall;
- with no berm between, up a 50° wall.

Each fails with the carrying on switched off. A ramp from the line that isn't on the first wall gives the road a ramp
starting there would, from RL 130. A design starting with a berm with a 25 m minimum bench, over a 20 m wall in the
design below, keeps the berm 10 m wide; run as one design the berm goes.

Through the commands, as the panel saves designs, the same to 1 µm:

- on level benches;
- on a seam floor dipping 5 % into the wall, the road's wall-side top standing half a metre above it;
- with a seam floor falling away below the toe east of 175 m, where the crest stays up on the toe. The second design
  keeps it there and its berm at RL 100, as one design does. Laid on the floor again, that berm fell as far as RL 52, and
  the test fails with decision 5 switched off.

The crest records the road's top exactly. A line with no road on it says so and makes no road.

The made-up grid pit (0034), ramp C at 30 m and 10 %, split after the first wall and run as one, ten strips:

| Design | Roads compared | Pit-side edge | Wall-side edge | Upper design / whole |
|---|---|---|---|---|
| RL switchbacks, split at RL 50 | 9 | within 1 µm | within 1 µm | 66 / 113 ms |
| Grid switchbacks, split at the M floor | 9 | within 1 µm | within 28 mm | 756 / 1,531 ms |
| Grid ramp, split at the M floor | 9 | within 1 µm | within 56 mm | 761 / 1,566 ms |

The road that stops in one design stops in two, at the same place and for the same reason (fault 3 of 0034), and its
crest records no top. On Grid ramp every toe and crest agrees to 1 µm up to the toe of the top wall. The wall-side
differences are heights only, at bends of the top wall to the topography. There a vertex of the wall-side edge is as
near two stretches of the pit-side edge as each other and takes its height from either, so a micrometre either way
flips it. No upper face has triangles crossing. Strips 1 to 3 aren't one sheet, as with no roads at all.

The grid pit's own check is unchanged: 33 faults on main and on this branch, with the same triangles and the same roads
to the top in all seven designs.

## What this leaves

- **Going down:** a design down from the bottom line of another, and recording where a road comes out at the bottom.
- **Joining the two designs' sheets,** which waits for surface build (Phase 3 workstream 5).
- **Running the design below again** makes new crests with new ids, and the design above names its lines by id, so
  they're picked again.
- **Picking the top crests of many strips** is Ctrl-click for now. Filtering by attribute (the Layers plan) would make it
  a few clicks.
- **The wall-side edge's height at a bend,** which can come from either side.
- **A ramp from the line with its end held.**
