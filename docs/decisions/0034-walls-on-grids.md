# 0034 — Walls on grids

Date: 2026-11-02 · Status: in progress, with the test pit and the faults it shows. On a level toe up level benches project, berm and
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
