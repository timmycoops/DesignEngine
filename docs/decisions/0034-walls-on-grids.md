# 0034 — Walls on grids

Date: 2026-11-02 · Status: in progress: the test pit and the faults it shows, then items 1 to 4, which keep walls from losing their way. On a level toe up level benches project, berm and
ramp are settled (0033), and every wall fault left on PIT C is in rows that finish on grids. Tim said "Lets do the grids next - but lets
create our own environment to test this in", with made-up grids at the coordinates of the RL template.

## The test pit

`testdata/synthetic/grid_template.py` writes eight grids as XYZ files in `testdata/synthetic/grids/`, and `Grid template.zdproj`
with them imported as surfaces. Neither is in git; the script makes them again in under a minute. The grids are a small coal model
invented for testing, 5.5 by 3 km at 25 m:

| Grid | Over the strips | What it's there for |
|---|---|---|
| L floor, L roof | RL -23 to 47, 4.3 to 6.7 m of coal | The working seam. The toe registers to its floor. |
| M floor, M roof | RL 6 to 81, 17 to 33 m above L | 0 to 3.2 m of coal. A washout, 760 by 520 m, where its roof lies on its floor. |
| U floor, U roof | RL 53 to 124, 32 to 56 m above M | 0 to 2.1 m of coal. It runs into the weathering at the south end and under the creek, where it lies on BOW. |
| BOW | RL 100 to 127 | 14 to 22 m below the topography. |
| Topo | RL 121 to 143 | A creek 14 m deep crosses the strips. The steepest slope is 14 %. |

Every seam dips north at 3 degrees and rolls a few metres every 100 to 200 m, as real seam grids do. That's the wrinkle that's
broken walls on real grids. Nothing crosses anything; grids only lie on each other where a seam has no coal or meets the weathering.

The strips are the bent base toe of the RL template, offset 80 m north ten times and draped on the L floor, with ramp strings A and B
across them. There are five designs, each with faces on its own layer:

| Design | Toe | Rows | Ramps |
|---|---|---|---|
| Grid walls | L floor | Walls at 65 degrees to the L roof and M floor, M roof and U floor, U roof and BOW, with 30 m berms (3 m minimum bench), then 45 degrees to the topography | None, so a fault is the wall's own |
| Grid ramps | L floor | As Grid walls | A held at its start, B at its end, on every wall |
| Flat toe to grids | RL -20 | As Grid walls | As Grid ramps |
| Grid toe to RLs | L floor | RL 50, 80 and 110 at 65 degrees, RL 140 at 45, 30 m berms | As Grid ramps |
| Every horizon | L floor | A wall to every grid, with 10 m berms and a 3 m minimum bench | None |

The script runs each design once without saving the runs, and prints what each strip didn't build. In the app, open the project and run
the designs from the Face design panel.

## Where it stands

The baseline, on the code of 0033:

| Design | Faces built | Roads to the top | Not right | Time for 10 strips |
|---|---|---|---|---|
| Grid walls | 8 of 10 | – | Strips 2 and 3 refused | 0.8 s |
| Grid ramps | 8 of 10 | 9 of 20 | Strips 2 and 3 refused, 7 roads stopped | 28.8 s |
| Flat toe to grids | 8 of 10 | 15 of 20 | Strips 1 and 2 refused, 1 road stopped | 7.3 s |
| Grid toe to RLs | 10 of 10 | 17 of 20 | 3 roads stopped | 0.3 s |
| Every horizon | 0 of 10 | – | The run is refused | – |

No face built has triangles crossing. The faults come in four kinds:

1. **The crest walk loses its way:** "the wall runs off the target", at a point hundreds of metres from where the wall could be. This is
   `wrinkled_strings_below_their_target_all_build`, the known fault of 0032. It refuses 2 strips of Grid walls, Grid ramps and Flat toe to
   grids, and 8 of the 10 in Every horizon.
2. **"The crest ran out of the walls: the string climbs too steeply for this batter there":** Strip 3 of Grid walls and Grid ramps, Strips 4
   and 9 of Every horizon. It also stops both roads on Strip 1 of Grid ramps, as the wall above them can't be built.
3. **A road's wall reaches past the target with no crest on it,** and the road stops: 7 roads in Grid ramps, 1 in Flat toe to grids, 3 in
   Grid toe to RLs.
4. **Speed:** ramps held at their end on grids take 2.9 s a strip, against the 1 s gate.

Walls of no height are reported where M and U have no coal, as they should be. They aren't faults.

## Why walls lost their way

Every refused strip came down to a hip running the wrong way. A hip is the line two neighbouring walls share, and the sweep takes it
outward from the string. Three things sent one backwards, along the string or behind a foot, where it ran out hundreds of metres across
the walls beside it. The crest then walked off along the wrong plane, or out of the walls altogether. These were faults 1 and 2.

- **Beside a transition.** A transition faces halfway between its neighbours, and the code took that as the way out. Where a berm narrows
  to nothing under a minimum bench, the line is moved out by walls of 1.9 and 89.9 degrees, and the hip between the transition and the
  near-vertical wall leans out from that wall's foot by less than the line turns. So halfway was the wrong way. This refused 11 of the
  16 strips in the baseline.
- **Two walls of one batter that fold over each other.** On a string that turns as it climbs, two segments can lie all but in one plane
  while turning and steepening by degrees and percent. The line their planes share then runs along the string, or just behind one foot.
  A transition was only ever put between walls of different batters.
- **After a short wall closes.** The new hip between its neighbours was turned to run away from the closed wall's foot, which is meant for
  walls back to back. A wall a few centimetres long closes at the end of its foot, and away from there is back along the string.

And "the crest walk didn't finish", on PIT C's Strip 1 and the test pit's, came from a target that only just lifts off the string at a
vertex, where grids lie on each other. The crest left the string there onto neither wall beside it and came straight back down, for ever.

## Decision

1. **A hip beside a transition goes out by the wall standing on a foot,** not by the transition's facing.
2. **Two walls of one batter whose shared line doesn't lean out from both feet by 1 % get a fold:** a transition through the fall line of
   the one lower out from the corner, and a line across the other leaning back along its foot at 45 degrees in plan. The two all but lie in
   one plane, so the fold does too, it only ever flattens, and its own hips are well apart. Walls in one plane to 1e-9 need none, as a
   wall above a road is parallel to the wall behind it on purpose.
3. **After a closing the new hip goes out by the walls either side,** as at the string. Only walls back to back, with no way out between
   them, go by the closed wall's foot.
4. **A crest that comes straight back down where it left the string goes on along the string** to where the target stands clear of it,
   ten times further each time round, from 1 µm to 1 cm. It used to look for where the target was clear, and then start at where the gap
   was nothing, which was the same vertex.

## Results

The test pit before and after items 1 to 4. The script now also checks each face is one sheet, with nothing missing from it:

| Design | Faces built | Roads to the top | Faces not one sheet | Time for 10 strips |
|---|---|---|---|---|
| Grid walls | 8 → 10 | – | 1 → 3 | 0.8 s |
| Grid ramps | 8 → 10 | 9 → 13 of 20 | 1 → 3 | 27.7 → 10.6 s |
| Flat toe to grids | 8 → 10 | 15 → 19 of 20 | 0 → 2 | 7.6 → 6.7 s |
| Grid toe to RLs | 10 → 10 | 17 → 17 of 20 | 0 | 0.3 s |
| Every horizon | 0 → 9 | – | – → 7 | 3.9 s |

- **Walls build on 49 strips of 50, against 34.** Every horizon's Strip 2 is still refused where the U roof lies on the U floor: a crest
  doesn't close on a wall.
- **No face has triangles crossing,** before or after.
- **Every road still stopped says the same thing:** a wall above it reaching past the target with no crest on it (fault 3), 11 of them.
- **15 faces have small holes,** each a sliver a metre or two long and a few centimetres wide, where a wall has no height: U lying on the
  BOW at the south end and under the creek, and the M washout. Grid walls' Strip 1 had them before. They come with the strips that now
  build, which are the ones where grids lie on each other, and they're fault 5, next.

PIT C, each of 11 strips on its own, all three ramps on all four walls, as in 0033, before and after:

| Toe | Walls to | Faces built | Roads to the top | Roads stopped by a wall fault | Faces not one clean sheet |
|---|---|---|---|---|---|
| RL -150 | RLs | 44 → 44 | 66 → 66 | 0 → 0 | 0 → 0 |
| Tim's grid | RLs | 44 → 44 | 78 → 78 | 3 → 3 | 0 → 0 |
| RL -150 | Tim's grids | 40 → 44 | 67 → 75 | 19 → 6 | 1 → 1 |
| Tim's grid | Tim's grids | 44 → 44 | 68 → 70 | 22 → 18 | 0 → 1 |

Strip 1 with a flat toe to his grids builds now: it was "the crest walk didn't finish", item 4. The face counted not clean with a flat toe
is Strip 9's, with triangles crossing, as in 0033. The new one grid to grid is a lone triangle where road A, stopped before, now comes out on
its last crest across 91 m of an oblique crest, and the toe above hooks back along that end. Roads held at the end on grids take up to
13.6 s a strip, where they took 8.8.

`wrinkled_strings_below_their_target_all_build`, the known fault of 0032, now builds all 64 strings with every crest on its target, and
runs with the other tests. 16 were refused before: 11 were item 2 and 5 item 3. `walls_of_one_batter_that_meet_along_the_string_fold`,
`a_berm_narrowing_to_nothing_on_a_wavering_line_moves_out` and `a_target_lifting_off_the_string_at_a_vertex_starts_a_crest` fail on the old
code and pass on the new. The last is a line and grid cut from the test pit, `testdata/synthetic/lift_off.txt`, as numbers, so it doesn't
hang on how one machine's sine rounds.

One road test changed. `a_road_stops_a_wall_lower_if_whats_built_on_it_cant_be` relied on a wall above a road that couldn't be built,
and now it can: the road is the one it was, 200 m from RL 100 to 120, and stops because the wall above needs 400 m and has 300 m left. It's
`a_road_stops_below_a_wall_too_short_for_it` now, and nothing else makes a road stop that way to test it.

## What this leaves

- **Fault 3,** a road's wall reaching past the target with no crest on it: 11 roads in the test pit and most of the 18 on PIT C.
- **Fault 5,** slivers missing from a face where a wall has no height: 15 faces in the test pit.
- **A road's top end across an oblique crest** leaving a lone triangle: one face on PIT C.
- **Every horizon's Strip 2:** a crest doesn't close on a wall where the U roof lies on the U floor.
- **Speed:** roads held at the end on grids take up to 13.6 s a strip on PIT C.
