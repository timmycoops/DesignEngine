# 0079 — Project a string, ramps from polylines, and a distance measure

Status: agreed 16 November 2026 (Tim, on the plan and its seven decisions as picked: "Yes, go ahead"). Three pieces,
a PR each. Project is built here; Measure and the surface between strings follow, and are written up here as they land.

## Why

Tim, 16 November: "I want to add more of the standard design expectations in the string command - a project function
will be key here, must work with grids and surfaces, also under the Walls I'd expect the ability to build ramps manually
with just polylines. We also need a distance measure tool."

What was there: Drape was the only string tool that took a grid or a surface, and it only drops a string straight down.
A projection at a batter existed only inside the Builder (`face.build_step`, 0048): one line at a time, up only, with a
build's name, what the line lies on and a record kept for Rebuild. Ramps existed only as rows of a face design or a
Builder step. Nothing measured a distance. Deswik's Project (`PolylineProject`) is the standard this is held to: an
angle, a direction, to an elevation, a relative level, or a limiting surface or grid, with the projection surface kept
if asked.

## Decisions

1. **Project makes the new string, and the wall as a surface when asked** (Deswik's "generate projection surface").
2. **The side is left or right, looking along the string,** and it's where the new string comes out whether it goes up
   or down. The panel can set it from a click in the view. Deswik's Inside and Outside only mean something on a closed
   string.
3. **Every string picked is projected, as one run:** one undo, one line in the log. A string that can't be projected
   refuses the run, each one listed with why, and nothing is made (as 0078 has it for the other string tools).
4. **Each string goes as it's drawn, at its own heights.** Nothing is registered first; Drape does that.
5. **Ramps from polylines are a kit of parts,** as they're made in Deswik: draw and grade the road's edge (Set grade),
   offset it by the road's width (Offset keeps the heights), project batters from both edges (Project), and make the
   road and any sheet by hand with a surface between two strings (to build). Join faces then makes one face a strip for
   the cut. A one-run "road from a string" tool can sit on those parts later.
6. **Measure reads out and keeps nothing:** plan distance, slope distance, RL difference, grade and bearing between
   clicks, on the view bar. It makes no object.
7. **Order:** Project, then Measure, then the surface between strings.

## Built: Project

**`zd_geom::project_string`** (`crates/zd-geom/src/project.rs`): a string, a side, up or down, a batter in degrees from
horizontal measured square to the string in plan, and where to.

- **To an RL, a surface or a grid** it's one wall of a face, `project_wall` (0023): each segment's plane at the batter,
  hips where planes meet, the line walked across the target, a gap in the target bridged and reported (0024). There is
  still one projection path. A grid is its triangulation, made once for the run. An RL is a level sheet, and the line to
  it loses the vertices that sheet's own two triangles gave it (those within a micrometre of the straight line through
  their neighbours), so a two-vertex toe gives a two-vertex line.
- **Going down** is the same wall upside down: heights mirrored, the wall built, and what it made mirrored back, as a
  face going down is (0037). Plan winding doesn't change, so the wall stays wound upward.
- **By a height** there's no target to meet. Every point goes out the height over the tangent of the batter and up or
  down by the height: the string's plan offset with true mitres, each vertex at the height of the nearest point on the
  string, lifted. A vertex on a straight run, where only the grade changes, is kept on the new line square across from
  it, so a graded road edge keeps its grades. On a level string this is the wall to that RL, corner for corner. On a
  string that rises and falls each corner goes out along its plan bisector, as Deswik's relative projection does; the
  hip between two graded walls would lean off it by a little. An offset that pinches off comes back in pieces, each a
  string, said (`string.projected_in_pieces`), and no wall is made for it.

**`string.project`** (`crates/zd-commands/src/builtin/project.rs`): `strings`, `to` (`{kind: rl, rl}`,
`{kind: height, height}` or `{kind: surface, id}`, a grid or a surface), `batter`, `side`, `direction` (up by default),
`wall`, `layer`, `name`.

- A new string is named for its string and where it went (`Strip 3 to RL 120`, `Edge up 10 m`, `Toe to Topography`), goes
  on its string's layer unless the run names one, and keeps its string's `strip` and `pit` (0029). The wall, if kept, is
  `<name> wall`, with the same.
- Stretches with no height (the target on or past the string) and stretches bridged where the target has no data are
  warned of with their length and place (`wall.no_height`, `wall.no_target`). A kept wall is checked for crossing
  triangles as a face's is.
- The run says how many strings, the batter, where to, the side, what it made and how long it took.

**In the app:** Project is a tool on the Strings row after Drape, opening the Strings panel set to it. It works on
every string picked. To: a height, an RL, or a grid or surface from the project's own; Going: up or down; the batter;
the side, with **Click it** setting left or right from a click in the view, read against the string picked last (the
stretch of it the click is beside); and whether to keep the wall. The one button says what it will do ("Project 3
strings up to Topography"). After a run the new strings are picked, ready for the next step from them: a berm's
offset, or the next project. Their walls aren't.

## Results

Known answers, at mine grid coordinates, from a toe 200 m long at RL 100:

- up at 45° to RL 120 stands 20 m out on the side asked for at RL 120, the wall 200 × 20√2 m²; down at 45° to RL 80 the
  same on the same side at RL 80: every vertex to 1 µm, the area to 1e-6 m²;
- to a surface rising 1 in 10 away from the toe, RL 120 over it, the wall meets it where 100 + y = 120 + y / 10: 22.222 m
  out at RL 122.222, and going down to one falling 1 in 10 from RL 80 is the same upside down, to 1 µm;
- on an L-shaped level toe, inside and outside the corner, up and down, at 60°: 15 m by a height is the wall to that RL,
  every vertex of each line on the other to 1 µm and the walls' areas to 1e-6 m²;
- a road's edge straight in plan, level for 100 m then up at 10 %: 10 m up at 45° is the same line 10 m out and 10 m
  higher with its grade break kept, three vertices to 1 µm, and the wall the two planes' area to 1e-6 m²;
- a seam's floor and roof as Vulcan grids at RL 100 and 120 and a surface at RL 140, a string at RL 110: up to the roof
  grid 10 m out at RL 120, up to the surface 30 m out at RL 140, down to the floor grid 10 m out at RL 100, to 1 µm;
- three toes numbered by a line across, in one run: three strings and three walls, each keeping its strip number, the
  toes left as they were, and one undo taking all six back;
- of two toes, one already above the RL: the run is refused naming it, and nothing is made;
- a closed dumbbell projected 8 m inwards closes its neck: two pieces, said, and no wall.

Timing, release build: fifteen toes 1.5 km long, 80 m apart, under a grid of 709 × 709 nodes at 25 m (1.0 M triangles),
up at 45° in one run: 45 ms without walls, 50 ms with them (1,839 vertices out, 1,839 wall triangles), the grid's
triangulation included. Every vertex of each new string is on its toe's batter to 1 µm.

The app's own check (`zd-app --bench --project`), through the ribbon, the panel's controls and clicks in the view, 18
of 18:

- **Opening:** Project is on the Strings row and opens the panel set to it; with nothing picked it says so and its
  button is off.
- **The side:** with a toe running east picked, Click it and a click south of the toe set Right, and north of it Left;
  the toe stays picked.
- **Several at once:** three toes picked by a box, projected up 20 m at 45° in one run: three strings, each 20 m out on
  the left at RL 120, picked ready for the next step, and the run's line on the panel.
- **To a surface, walls kept:** from those to a topography at RL 140: three strings 40 m out at RL 140 and a wall each,
  the strings picked and not their walls.
- **A refusal:** up to RL 90 from RL 140 is refused, naming all three, and makes nothing; going down it runs, 50 m
  further out at RL 90. One undo takes that run back.

## What this leaves

- **Rules** for the batter (by azimuth, inside a polygon or solid, by horizon): a number for now, as the Builder's.
- **A limiting surface** on a projection to an RL or by a height (Deswik's "limit to surface"), and projecting to a
  solid.
- **Cleaning the line first.** A string goes as it's drawn. One with a vertex at every triangle it was draped across is
  fine where those lie on straight runs in plan; one that wanders needs Filter first, as a profile's condition does.
- **Measure, and the surface between strings:** the next two PRs.
