# 0045 — Step design: a face built by hand, a step at a time

Date: 2026-11-05 · Status: accepted (Tim, 2026-11-05: "how we can manually edit a face as well ... create these from a
certain position and do these in steps", then on doing it with strings and steps rather than moving surface points,
"Go for it"). The plan is the "Step design plan" tab of the Build Plan doc; this is workstreams 1 and 2.

## Why

Some faces are too fiddly for one profile run over every strip: one detailed pit wall, an endwall round a corner, a ramp
that has to go where it fits. Surpac, Vulcan and Deswik all let a planner build those by hand: project a string, berm,
put a ramp in, look, carry on. The tools to do each step were here, but only as a whole table run over strips, or as the
Properties panel's Wall row, which goes to a surface or grid, not an RL, and can't take a ramp.

Moving a surface's points by hand was the other way to do it. It doesn't keep batters, berm widths or road grades, and
validity has to be checked again after every move, so it isn't built.

## Decision

1. **A step design is a face design on one string,** with the same profile, rules and ramps as the Face design panel. It
   starts from the string selected in the viewport (Step design in the panel): lying flat on its own RL if it's level
   and not at 0, or on the first surface otherwise, with no run-on.
2. **Each step runs as it's added or changed.** With Run each step ticked, the panel saves the design and runs it a
   moment (0.4 s) after each change, in one command, `face.step`, so each step is one undo. It's ticked for a design
   started as a step design, and for any design opened with one line on one string. A step that can't run says why and
   isn't saved, and what the last step built stays.
3. **Each step runs the whole design again,** which takes milliseconds on one string, so a change to an early step flows
   up to the top, and a profile built by hand runs the same over other strips from the Face design panel.
4. **A face ending on a berm hands on the berm's line** as its `next toe` (going down, `next crest`): where the next
   wall will start. Any design ending on a berm does, not just a step design.
5. **A ramp is placed by clicking** (Place ramp): where it starts on the line the next step starts from, then along the
   line the way it runs. It's a ramp string 5 m either side of the line, square to it, held at its start, 30 m wide at
   10 % until changed. The next wall added climbs it.
6. **In a step design a new wall carries on the roads of the wall below,** and any ramp placed since, so a road climbs
   every wall from where it was placed until it's clicked off a wall. Turning it back is a second click on its cell,
   as before. In a design with Run each step off, a new wall has no ramps, as before.
7. **A new wall to an RL starts a bench (10 m) past the last RL,** or past the toe's own RL, up or down the way the
   profile goes. It used to start at RL 0, which on a toe flat on RL 100 is below the toe: the first wall of a step
   design built nothing ("step 1 has no height"), which is what Tim met on his first try.

## Results

Through the commands, a toe 1 km long on RL 100, a 45 degree wall to RL 120 with ramp A (10 m at 10 % from 100 m), a 10 m
berm and a wall to RL 140, added a step at a time:

- after the first step the crest is 20 m out at RL 120, and the design is saved with it;
- after the berm the next toe is at RL 120 all along, 30 m out clear of the road;
- after the third, ramp A climbs steps 1 to 3 to RL 140, and there's no next toe;
- every face, toe, crest and road edge is the same as the same design run whole, to 1 µm;
- ramp B placed on the berm's line at 600 m, as a click places it, starts there at RL 120 and climbs 200 m of road east to
  RL 140 (198.997 m in plan), with A turning back on the berm beside it;
- undo takes a step off, and a step naming a rule that isn't there saves nothing and leaves what was built.

On the Grid template, strip 5 with 100 m run-on, Tim's 7 rows added a step at a time (65 degrees to the M floor, U floor and
BOW grids with a 30 m berm after each, then 45 degrees to the topography), with ramp A (50 m at 10 %) turning back on
the second berm, and the same to RLs 50, 80, 110 and 140 with 3 m berms. Each step is `face.step` timed through Python,
release build. Built a step at a time, the face is the same as the design run whole, vertex for vertex, every time.

| Design | Each step, ms | Triangles |
|---|---|---|
| To grids, ramp A | 12, 117, 185, 556, 590, 1,415, 1,515 | 18,066 |
| To grids, no ramp | 6, 57, 62, 264, 270, 710, 730 | 18,608 |
| To RLs, ramp A | 6, 31, 44, 68, 76, 99, 100 | 4,079 |
| To RLs, no ramp | 3, 18, 18, 32, 35, 46, 48 | 4,200 |

So to RLs every step shows in a tenth of a second. To grids the walls themselves cost most (the topography wall alone is
440 ms), and a ramp doubles it, as the face is built once with no roads to place the road. By the 7th row a step on grids
takes 0.7 to 1.5 s, over the plan's 0.5 s. A wall on grids being slow is the same on any design, so it's taken on as its
own job. The panel stays usable while a step runs.

`testdata/synthetic/step_template.py` writes `Step template.zdproj`: a bent 1.5 km toe flat on RL 100 with the
design Steps from Toe built the way the panel builds it, one `face.step` a row (a 70 degree wall to RL 115, a 10 m berm,
ramp A placed on the berm's line 150 m along as Place ramp places it, a wall to RL 130 climbing it, a berm and a 65
degree wall to RL 145 carrying it on), and a second toe with no design to start one on. Each step takes 1 ms; the
face is clean, road A starts where it was placed and climbs from RL 115 to 145, and the design run whole gives the same
face, vertex for vertex.

## What this leaves

- **Editing a line by hand and carrying on** (workstream 3): the line the next step starts from copied as a string of
  your own, the wall or berm below re-formed to meet it, and any road on it carried on.
- **One face** (workstream 4): every step's face, and any walls between strings, welded into one sheet.
- **A ramp placed on a line other than where the next step starts** is a ramp string like any other: its column in the
  table sets the walls it climbs, and it only makes a road if its string crosses the toe of the first.
- **Hand edits on a rule design** (Deswik's Fine Tune) come after this plan.
