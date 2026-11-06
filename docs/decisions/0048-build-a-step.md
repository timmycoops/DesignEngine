# 0048 — The manual builder: one step on a line

Date: 2026-11-07 · Status: accepted (Tim, 2026-11-07, on the Manual builder plan and its five decisions: "Agree go for it").
Workstream 1 of the Manual builder plan tab in the Build Plan doc. After 0041 (roads carry on), 0046 (edit the line) and 0047
(Join).

## Why

Tim, 2026-11-06, after the step design's hand edits kept failing for him: "The automatic version of running a set of toes is
easy, that makes sense. Where we are getting confused is the editing piece while the rule is still open." And: "A manual pit
design builder with the ability to use the ramp, project, berm options we have built but with greater flexibility will be the
ultimate outcome." The plan: pick a line, pick an operation, it runs once and makes the next line; edit any line between steps
with the ordinary tools; nothing is built again unless you ask (Rebuild from here, workstream 2). This record is the engine's
side of one step.

## Decision

1. **`face.build_step` builds one wall or one berm from the line you pick,** run once, as a design of one row on that line.
   - **Project:** a wall to a surface, a grid or an RL at a batter. The roads that came out on the line climb it. `ramps` are
     new ones, each placed by its string across the line or held at its end, as on a design's row.
   - **Berm:** a berm of a width, on what the line lies on unless it names a surface, a grid or an RL. The roads that came
     out on the line cross it, after it or through it as each was built (0035). `turns` are those turning back on it, on a
     box past their tops (0038), and `join` makes their boxes one pad: on, off, or by themselves where they come within a
     road's width of each other, if it doesn't say (0047).
   - **Ramp, Turn and Join** are how a step goes about the roads, not steps of their own: a new road on a wall, a turn and Join
     on the berm before the wall above. `stop` on either ends roads that came out on the line.
2. **What a step makes.** A wall step makes its crest (`<build> crest <n>`), its sheet (`<build> wall <n>`) and its roads'
   edges. A berm step makes only its line (`<build> toe <n>`): its sheet comes with the wall above, as in a design, where the
   wall builds the berm round the roads' pads (0038). Each is an ordinary string or surface, yours to edit. None carries
   `face_design`, so the viewport doesn't refuse edits to it.
3. **A wall from the line of a berm a step made builds that berm with it,** from the crest the berm was built from, as one
   design of two rows. The roads cross the berm and turn on it as they do in one design, and the wall's sheet takes in the
   berm below it. The wall stands on the berm's line as the berm makes it, or on the line as you've edited it, as on a line
   redrawn by hand (0046): your line comes first, Join and all. Each line a step makes keeps the fingerprints of the line it
   was built from and of itself, so the wall knows whether either has been edited since without building the berm again. If
   the crest has gone, the wall stands on the berm's line as it is, says so, and no road carries on over the berm.
4. **A berm that ends a face shows the line the wall above will stand on.** Where roads turn on it, that's its boxes, each
   next road's flat start cut back into the wall above across its box, and the boxes joined as one pad where Join says, or
   where they met. Where a flat start meets the wall above at its far end moves with that wall's batter, by 0.33 m between
   60° and 65° on 30 m roads. The berm's line takes the batter of the wall below it, as benches usually repeat (65° if a step
   didn't make that wall). The wall above, built from the line unedited, cuts its own.
5. **Roads carry on as in 0041,** from the tops on the line, at the width and grade they were built at. A crest now also
   records how each road gets past a berm and turns (`road_top A berm`, `turn_join`, and `turn_flat` and `turn_end` where
   set), so the next step builds it on as it was. Every design run records them too, for carrying on by hand from a design's
   crest.
6. **What a step leaves,** for the next step and for Rebuild: on every piece, `build` (the build's name), `build_step` (its
   number in the build, from 1) and `build_id` (the step's own id). On its line, `build_on` (what the line lies on) and
   `build_record`: the line it was built from, what that lies on and its revision then, the side, the step, and the
   fingerprints. So the step after it needs no side or what the line lies on.
7. **The first step.** A line drawn by hand needs `lies_on` and `side`, and is registered to what it lies on. `run_on` runs
   it on past its ends, so the face is built oversized and the cut does the fitting, and `condition` cleans it, as a profile
   does. A line a step or a design made keeps its heights, with only its run-on registered (0041). What's made goes on the
   `builder` layer unless the step names one; later steps go on their line's layer.
8. **Engine:** a berm row can carry the wall to come (`to_come`: the roads turning on it and that wall's likely batter)
   where no wall follows it in the face, and a face with no wall at all carries every road that came out on its line over
   its berms. Neither changes a design's run: a design never has a berm with nothing after it that roads turn on.

## Results

Known answers, from a toe at RL 100, 200 m long:

- a wall to RL 120 at 45° stands 20 m out, a sheet of 200 × 20√2 m², to 1e-6 m²;
- a 10 m berm puts its line 30 m out;
- a wall from it to RL 140 at 45° puts its crest 50 m out, and its sheet takes in the berm, 200 × (10 + 20√2) m².

Built a step at a time against the same design run whole (toe at RL 100, walls to RL 110 and 120 at 45° with a 5 m berm, a
road 10 m wide at 10 %), going after the berm, up through it, and turning back on a box on it: the crests, the berm's line
and every vertex of the road's edges agree to 1 µm, the two walls' sheets make the whole design's sheet to 1 cm², and the
top crest records the same top to 1 µm.

Tim's step template, from his bent toe up to RL 175, roads A and B 30 m wide at 10 % placed on the wall to RL 130, both
turning on boxes on the berm at RL 155 with Join on, built in 11 steps: every crest, the joined pad at RL 155 and both roads'
tops at RL 175 agree with the design run whole to 1 µm, and no wall's triangles cross. Each step takes 0 to 1 ms; the design
run whole, 7 ms.

To grids and surfaces: a seam's floor and roof as Vulcan grids at RL 100 and 120, and a surface at RL 140. A toe drawn at RL 0
lying on the floor grid is registered to RL 100. A wall at 45° to the roof grid stands 20 m out at RL 120, a 10 m berm lies
on the roof grid 30 m out, and a wall at 45° to the surface stands 50 m out at RL 140, every vertex to 1 µm. The sheets are
150 × 20√2 m² and 150 × (10 + 20√2) m², to 1e-6 m².

A berm's line edited by hand, its far end dragged 3 m further out: the wall above stands on it, says the berm below is 5.0 to
8.0 m wide, its crest follows 10 m out square to the slanting end, and the road carries on.

The made-up grid pit (0034), Strip 5 of Grid switchbacks built a step at a time, run on 100 m each way, ramp C turning on
every berm. Every step builds, nothing crosses, and C comes out on the top crest. Release build:

| Step | Time |
|---|---|
| Walls to the L roof, M floor, U floor and BOW | 11 to 49 ms |
| Berms with C turning on the M floor, U floor and BOW | 0.2, 0.4 and 0.85 to 1.02 s |
| Walls over those berms, to the M roof, U roof and topography | 0.2, 0.4 and 0.9 to 1.07 s |
| The design run whole, ten strips side by side | 1.0 s |

The slow steps are the berm's 30 m offset of a crest of about 2,000 vertices, done twice where boxes stand on it, at 0.3 to
0.4 s each. A design pays the same on each strip; it runs strips side by side. The offset is the place to make grid steps
faster.

## The Builder panel

Workstream 3, ahead of Rebuild (Tim, 2026-11-07: "yes build the panel"), so a face can be built and edited between steps in
the app. The Builder button beside Face opens it.

- **Click a line** in the viewport, and the panel sets up the next step from it: a berm after a wall, a wall after a berm or
  from a toe. A wall goes up to an RL, a grid or a surface, and a berm lies on one: the lists give the project's grids and
  surfaces under their own headings, and say so where there are none yet (Tim, 2026-11-07: "We need to ensure that the
  manual face builder allows project to grids and surfaces"; his step template has neither). A wall goes to the grid after the last wall's, or 10 m above its RL, at its batter; a berm is as wide as the
  last. Picking a step's wall or road picks its line.
- **The roads on the line:** going up a wall, each climbs unless you untick it, which ends it there. At a berm each crosses
  (or climbs through), turns back, or ends here. With two turning, Join: when close, one pad, or apart. **+ Road** places a
  new road on the wall: a click where it starts on the line and one the way it climbs, as in the step design.
- **From a line drawn by hand** the panel asks for the build's name, what the line lies on (its RL if it's flat), the side
  the walls go up on, and how far to run it on past its ends.
- **Build wall** or **Build berm** runs the step, one undo, and picks the new line, ready for the next.
- **The steps of the build** are listed in order with what each did. A click on one picks its line and frames it, and
  picking a line, wall or road lights up its step. A step whose line you've edited says so. A step built from a line
  edited since, as by the revision kept in its record, says it was built before the edit and shows dimmed, waiting for
  Rebuild. The bin takes a step out with every step built on it, in one undo.
- **Your edits are ordinary string edits.** Builder lines aren't a design's, so the viewport lets you drag, insert and
  delete their vertices.

The app's own check (`zd-app --bench --builder`), through the panel's buttons:

- **Building:** from a toe at RL 100, Build wall to RL 115, Build berm and Build wall each built and picked the new line.
- **Picking from the list:** a click on step 1 picked crest 1, lit its row and framed it to the millimetre.
- **Editing:** with the berm's line edited 3 m out at one end, step 2 said edited and step 3 built before an edit. Step 3
  taken out with its bin left two steps.
- **Building again:** the wall built from the edited line stood on it, saying so and how wide the berm below it now is.
- **To a surface:** with a surface at RL 140 imported, the To list showed it under Surfaces; picked there, a berm and a wall
  up to it built, the crest all at RL 140.

## What this leaves

- **Rebuild from here** (workstream 2).
- **Rules** for a step's batter and width: it takes numbers for now.
- **Going down,** turning on a crest with no berm, a minimum bench on a berm built on its own (it has no wall below it in its
  own run, as in 0041), and carrying on a road held at its end (0041).
- **The berm offset on grids,** for steps under a second.
