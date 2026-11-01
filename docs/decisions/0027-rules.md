# 0027 — Rules: one value per edge

Date: 2026-11-01 · Status: proposed. Tim asked for the rule types on 2026-11-01, and the choices below are his to check

## Why

Walls and berms took one number for a whole step. Geotechnical design doesn't: batters and berms change by domain, by the
way a wall faces and by horizon, and the Phase 3 gate asks for "batters and berms by horizon and azimuth". CLAUDE.md lists
the rule types and says "Blending between rules may only widen berms and flatten slopes".

## Decision

1. **A rule gives one value per edge, and it's asked again for each line.** A step works from a different line each time:
   the string, a conditioned crest, a berm's line. A wall takes `project_wall_by_edge`, with a plane for each edge at its own
   batter, so the method in decision 0023 holds.
2. **At a corner, different batters meet in a hip.** Planes at different slopes meet in a line like any others.
3. **On a straight run they can't.** Two planes through one line share only that line, so the crest would step sideways with
   a vertical wall to carry it. So a transition stands at the vertex: one plane with no foot, through the fall line of the
   flatter wall and a hip back across the steeper one. The crest bevels at 45 degrees in plan, so a 20 m rise going from 45
   degrees to 2 in 1 steps 10 m out over 10 m along.
4. **The flatter side always wins.** The transition sits on the steeper wall, so the flatter batter reaches into the steeper
   stretch and never the other way. Nothing is steeper than its rule asks for.
5. **A berm does the same.** With widths that differ, its line is read off a wall one metre high with a slope for each edge,
   so the hips and transitions are the wall's own and the wider berm reaches into the narrower one. Widths that are all one
   number take the plain offset as before.
6. **Rules that go by place cut the line.** An inside polygon or inside solid rule adds a vertex wherever the line crosses
   the boundary, so the change falls on the boundary and not at the nearest vertex. These go by where the line a step
   starts from really is, including its RL for a solid.
7. **Rules that go by the string look at the string.** By segment and by azimuth take the string segment nearest each edge.
   A conditioned crest has short segments that wander, and values read from them would flicker along the wall.
8. **The types.**

| Rule | Gives |
|---|---|
| Fixed | One number |
| By segment | A value for each segment of the string, in order. The count has to match, or the run says so |
| Inside polygon | One value inside a closed string in plan and another outside |
| Inside solid | One value inside a solid and another outside |
| By azimuth | A value by sectors of the way the wall faces, with one for anything else |
| By horizon | A value by the surface a row finishes on, with one for anything else |

9. **Azimuth is the way the wall faces:** the direction it dips, clockwise from grid north, whichever way the string was
   digitised. A sector runs clockwise from its first number to its second, and can wrap past north. A face going down takes
   the same value as one going up on that side, as it's the same plane.
10. **By horizon goes row by row.** A wall takes the value for what it finishes on. A berm takes the value for the surface it
    names, or else what the wall below it reached. It's one number for the row, so it never makes a transition.
11. **Rows naming one rule are one wall.** Steps with no berm between them carry straight on, as they do at one batter. By
    horizon is the exception, as its rows can differ.
12. **A berm can have a minimum bench.** Tim: where grids sit on each other "you will end up berming all this flat distance
    and it will look weird". A berm row takes a Min bench, and along any stretch where the wall below is lower than that,
    the berm goes to nothing and tapers in at 45 degrees beside it. It's measured from the crest to the toe below it, so it
    covers grids that touch as well as grids a little apart. A stretch shorter than the berm is wide doesn't count, either
    way, so a berm doesn't stutter on an undulating seam. With the row conditioned, the line stands clear of the crest by the
    tolerance, as it does anywhere there's no berm. A new berm row starts at 1 m, and a saved row stays off until set.

## Evidence

Known answers through `face.project`, all to 1e-5 m: a straight toe up 20 m, 45 degrees then 2 in 1, gives a crest 20 m out
then 10 m out, with the bevel where it should be and the wall's area right to 1e-6 m². It's the same wall run the other way
on the other side, and the change can fall on the join of a closed toe. Polygon and solid changes fall on their boundaries
where the string has no vertex. A berm going from 5 m to 10 m at a polygon's edge starts widening 5 m before it.

On PIT B's real grids through the real app, a 600 m toe up through four seam grids to RL 500, with the batter going from 70
to 60 and the berm from 10 to 15 m part way along: 5 walls and 6,021 triangles in 454 ms, with no crossings reported, against
5,310 triangles in about the same time with neither changing.

## Known limits

- **A change is one step.** The bevel is fixed at 45 degrees in plan, and there's no gradual change over a distance.
  Deswik has a smoothing rate, and the geotech solid rule with a transition distance comes later.
- **One polygon or solid to a rule.** Deswik takes a layer of them with values in an attribute, which fits once attributes
  land (Layers workstream 6). Nested rules can't be written yet.
- **Inside solid goes by the line a step starts from,** so a wall whose toe is outside a solid doesn't change as it rises
  through it. Rows between horizons keep this small.
- **By segment needs the string's own count,** so it suits one string, and a line on a layer with strings of other counts is
  refused.
- **A berm with mixed widths can't lose a piece.** The plain offset drops a piece that pinches out. Mixed widths use the wall
  method, which refuses a wall closing against a far wall.
- **Not compared with Deswik.** That waits for a real toe and its design.
