# 0079 — Project a string, ramps by hand, and a distance measure

Status: agreed 16 November 2026 (Tim, on the plan and its seven decisions as picked: "Yes, go ahead"). Three pieces,
a PR each, all three built here. A fourth, Ramp from a start, followed the same day once Tim had seen the first three
(decision 8).

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
   road and any sheet by hand with a surface between the strings. Join faces then makes one face a strip for
   the cut. A one-run "road from a string" tool can sit on those parts later. **The grade is set on a string once it's
   drawn, not digitised as it's clicked** (Tim, 16 November, asked which: "I think it would be set grade as a rule like
   you did. These are more manual tools so you would move the ramp in theory"): the edge is drawn and moved by hand, and
   Set grade is run on it again.
6. **Measure reads out and keeps nothing:** plan distance, slope distance, RL difference, grade and bearing between
   clicks, on the view bar. It makes no object.
7. **Order:** Project, then Measure, then the surface between strings.
8. **A ramp is placed from a start, as its polygon** (Tim, 16 November, on seeing the kit of parts: "I expected to draw a
   ramp from a start position - you place a start, set the width and grade and draws the ramp polygon essentially").
   Decision 5 had read "ramps manually with just polylines" as the parts alone; this is the tool he meant, and the parts
   stay for what's made from it. His answers to what it does: it follows "a line you pick or a direction", it ends at
   "a rl or a surface", and "it would only make the polygon". Moving the ramp is placing it again.

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

## Built: Measure

A view tool, so it's on the view bar (0076) beside Draw, with the M key, and in Find a command as Measure distance. It
is a mode of the view like Draw and Section, and makes no object and runs no command: there's nothing to undo or log.

- **Clicks snap as Draw's do:** to string vertices and segments, then surfaces, then the working plane; on an open
  section, to the cut. The points show as handles with a line through them.
- **The leg to the cursor reads out beside it** as the cursor moves: plan distance, slope distance, RL difference, grade
  and bearing.
- **Each click writes the leg to the status line** in full: `plan 500.00 m · slope 502.49 m · RL +50.00 m · 10.0 % up
  (1 in 10.0, 5.7°) · bearing 036.9°`. From the third point on it adds the total over the legs, in plan and on the
  slope, and the RL difference from the first point to the last.
- **Backspace takes the last point off, Esc clears, M or the button ends.**
- **The numbers come from the points' world coordinates as the engine gave them** (f64 eastings, northings and RLs),
  never from what the screen draws (engine rule 2). Grade is rise over plan distance, up positive; a leg straight up or
  down is `vertical`, and one within a hundredth of a per cent is `level`. Bearing is the grid bearing, clockwise from
  north, three figures and a tenth.

Known answers (`app/tests/measure.test.mjs`): a 3-4-5 leg of 500 m in plan rising 50 m is 502.494 m on the slope at
10 %, 1 in 10, on a bearing of atan(3/4), to 1e-9; a millimetre apart at mine grid eastings and northings reads a
millimetre; bearings north, east, south, west and north-west read 0, 90, 180, 270 and 315; just west of north rounds to
000.0° and not 360.0°; three legs along a bench, up a ramp and back add to 500 m in plan and RL +10.

The app's own check (`zd-app --bench --measure`), by the M key, the bar's button and clicks in the view, 13 of 13: a
string from one corner of a 3-4-5 triangle to the other, 50 m higher, its two ends clicked, reads the leg above exactly,
from the points as the engine has them; moving on reads a leg out beside the cursor; a click on gives the total over two
legs; Backspace, Esc and M do what they say; and no object is made. The select (29) and Project (18) checks pass on the
same build.

The shell check (`--shell`) doesn't pass every run on this machine, with or without this work: 53 of 53 and 52 of 53 on
two runs of this branch, 51 of 53 on main at bf6ddb3, a different check failing each time (the panels' sizes after the
` key, a tool's tip). It needs looking at on its own.

## Built: ramps from polylines

**`surface.between_strings`** (`crates/zd-commands/src/builtin/sheet.rs`, `crates/zd-geom/src/sheet.rs`): `strings`
(two or more), `layer`, `name`. One surface between the strings picked, left as they are.

- **Between two strings** it's the band a face builds between a crest and the next toe (`band`): their outline
  triangulated in plan, so it can't reach across either line, wound upward. Every corner is a vertex of a string; nothing
  moves. So it's a sheet single-valued in plan, as every road and every wall under 90° is.
- **Several strings make one sheet across them all,** as the toe of a fill, a road's two edges and the crest of a cut.
  They're put in order across whatever order they're given in (the two furthest apart are the ends of the row, the rest
  fall between), each is taken the same way along as the one before, a band is made between each and the next, and the
  bands are welded where they share a string. The run says the order, and which strings it took the other way along.
- **Closed strings** make a ring, as a pit's toe and crest: each is started from the vertex nearest the other's first.
  Open and closed strings together are refused.
- **Refused, naming the two strings:** strings that cross in plan or double back past each other, where a surface would
  fold over itself, and strings that run along each other with nothing between.
- The surface is named for the strings at either side unless the run names it, goes on the first string's layer, keeps
  the `strip` and `pit` its strings share, and is checked for crossing triangles as a face is.

**A fault found on the way, and fixed.** `band` between two closed strings left one triangle out of the ring: its outline
dropped the last vertex of the second string and doubled the one after the slit, so that string's first segment wasn't
in it. A berm on a closed toe had the same hole. No test covered a closed pair; one does now (a 100 m square and a 60 m
square inside it, 10 m lower: four trapezia, 7,155.4 m², to 1e-6 m²).

**In the app:** Surface between strings is a tool of the Strings panel on the Walls row, with the string tools a ramp is
made from beside it: Set grade, Offset, Project, Surface between, then Join faces. It works on every string picked, two
or more, and the surface it makes is picked.

Known answers:

- a road 30 m wide and 200 m long at 10 % between its two edges: 30 × 200 × √1.01 m², to 1e-6 m², four vertices; given
  running opposite ways it's the same sheet, and said;
- four strings across a road cut into a slope, given out of order and one backwards (fill toe at RL 90, edges at RL 100
  30 m apart, cut crest at RL 115): one sheet, 200 × (10√2 + 30 + 15√2) m², eight vertices, one boundary;
- a pit's toe and crest as closed strings drawn opposite ways round from different corners: the ring above, two
  boundaries;
- a road round a quarter circle, edges at 100 and 130 m radius in 16 chords: the chords' own area to 1e-6 m², within 1 %
  of the quarter annulus;
- **a ramp by hand through the commands** (`crates/zd-commands/tests/sheet.rs`): an inside edge 200 m long graded at 10 %
  from RL 100, its outside edge by Offset 30 m right at the same heights, the cut's crest by Project 15 m up at 45° on the
  left, the fill's toe by Project 10 m down at 45° on the right, and one surface between the four: 13,118.6 m², the three
  parallelograms' areas to 1e-6 m², 11,000 m² in plan, six triangles on eight vertices, one sheet. One undo takes it back.

The app's own check (`zd-app --bench --ramp`), from the Walls row through the panel, 11 of 11: the same ramp, made as
above by Set grade, Offset, Project twice and Surface between strings on four strings picked by a box, with each line
where it should be to the millimetre and the surface's area to 1e-6 m². The Project (18) and Measure (13) checks pass on
the same build, and `cargo test -p zd-geom -p zd-commands` (664).

## Built: a ramp from a start

**`string.ramp_polygon`** (`crates/zd-commands/src/builtin/ramp.rs`, `crates/zd-geom/src/ramp.rs`): `start`,
`towards`, `line`, `width`, `grade`, `side`, `to` (an RL, or a surface or grid), `replaces`, `layer`, `name`. It makes
the ramp's polygon, a closed string, and nothing else.

- **The inside edge is the ramp's own line.** Along a string picked, it runs from the place on the string nearest the
  start, the way the second click says (round a closed string, the shorter way to it). With no string it runs straight
  from the start towards that click. It takes its plan shape from there and its heights from the grade alone, measured
  along the inside edge in plan, from the RL of the start: the string's own heights don't come into it.
- **What it runs to says whether it climbs or falls.** To an RL above the start it climbs, to one below it falls; to a
  surface it heads for the surface from the side of it the start is on. The grade is its size.
- **It ends where the inside edge reaches the RL or meets the surface,** squared off. The surface is read along the
  edge with a vertex wherever it crosses one of its triangles, so where the two meet is exact.
- **The grade is never eased to get there.** A string that ends first, or a surface never met, ends the ramp there at
  its grade, and it says how far short it is (`ramp.short_of_target`), as a ramp reports where a grade can't be met
  instead of drifting (0028).
- **The outside edge is the width out, square across:** each vertex along its plan bisector at its own height, so the
  road has no crossfall and a bend's corner is at one RL from edge to edge. A bend too sharp for the width on that
  side, where the edge would turn back on itself, is refused.
- **`replaces` moves a ramp:** the polygon named takes its next revision where the new one is placed, keeping its id
  and its name, and one undo puts it back. Only a ramp's polygon can be replaced.
- The polygon carries `ramp_width`, `ramp_grade` (signed, the way it runs), `ramp_length`, `ramp_from_rl` and
  `ramp_to_rl`, and the `strip` and `pit` of the string it runs along. It's named `Ramp RL 100 to 120` unless the run
  names it, and goes on its string's layer, or `ramps` with no string. A start more than a millimetre off its string is
  put on it, and said.

**In the app:** Ramp is first of the hand tools on the Walls row, in the Strings panel. Follows: a line (the string
picked) or a direction. Width (50 m to start with), grade, the side the road lies on looking the way it runs, the RL it
starts at (blank: the RL of where you click, on the line or a surface), and what it runs to. Its button asks for two
clicks in the view: where it starts, and the way it runs. The line stays picked for the next one. The ramp placed last is
named on the panel with **Move it**, which places it again from two new clicks with the settings as they now are.

Known answers, at mine grid coordinates, to 1 µm:

- straight east from RL 100 at 10 %, 30 m wide on the right: to RL 120 it's 200 m long, its four corners where they
  should be; to RL 90 it falls, 100 m long;
- along a toe 150 m east then north, from 50 m along it: 100 m to the corner and 100 m on, the corner at RL 110 from
  edge to edge, the outside edge 30 m short each way on the inside of the bend and 30 m long each way on the outside,
  whatever the toe's own heights; the other way along it, from the north leg back, the same going back;
- under a surface falling 1 in 20 from RL 130, 100 + x / 10 meets 130 - x / 20 at 200 m, RL 120; down to a floor rising
  1 in 20 from RL 80 it meets at 133.333 m, RL 86.667; round the toe's corner to the first, 225 m along at RL 122.5; to
  a level Vulcan grid at RL 115, 150 m out;
- a toe 150 m long to RL 120 at 10 % ends at RL 115 and says 50 m short; a hairpin 20 m across has no room for a road
  30 m wide on its inside, and is refused;
- placed along a toe, then placed again 50 m on with `replaces`: the same object at its next revision, and one undo puts
  it back.

The app's own check (`zd-app --bench --ramp-start`), from the Walls row through the panel and clicks in the view, 12
of 12: a ramp placed along a toe by two clicks with its polygon to the millimetre, Move it placing the same polygon 50 m
on, one undo putting it back, and one straight from a typed RL to a sloping topography. The `--ramp` (11) and
`--project` (18) checks pass on the same build, and `cargo test -p zd-geom -p zd-commands` (673).

## What this leaves

- **Rules** for the batter (by azimuth, inside a polygon or solid, by horizon): a number for now, as the Builder's.
- **A limiting surface** on a projection to an RL or by a height (Deswik's "limit to surface"), and projecting to a
  solid.
- **Cleaning the line first.** A string goes as it's drawn. One with a vertex at every triangle it was draped across is
  fine where those lie on straight runs in plan; one that wanders needs Filter first, as a profile's condition does.
- **Measure:** a dimension left in the view, angles between legs, and areas.
- **Ramps by hand:** the ramp's batters and road made with it (Project and Surface between do them from its polygon
  now), a ramp through several benches with flats or switchbacks, a ramp dragged in the view rather than placed again,
  crossfall, and a ramp that meets its target on its outside edge too where the ground is skew to it. A sheet from
  strings with breaklines inside it, and a vertical face. Moving a whole string by a distance: today a string moves a
  vertex at a time.
