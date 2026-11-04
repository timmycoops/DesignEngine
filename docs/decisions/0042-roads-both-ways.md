# 0042 — Roads both ways on one wall

Date: 2026-11-05 · Status: accepted (Tim, 2026-11-05, after End angle worked for him: "I want to test when we have
multiple ramps on the same face and how that would work"; then, on the finding below, "Yes, build it"). Changes item 2
of 0032.

## Why

A straight ramp and a switchback on one face is an ordinary layout: a main road up the highwall and a terrace
switchback further along. After a switchback turns, it runs the other way from the straight road on that wall. 0032
held roads on one wall to one way, but checked it only when a road joined a wall, never after a turn, and the wall
cut took the first road's way for all of them. So the switchback was cut running on, never turning, with a jog at
each berm and no warning. On the Grid template with roads 50 m at 10 %, ramp D turning at 2,200 m switched back
2,200, 2,405, 2,112, 2,405, 2,120 m on its own, and with a straight ramp A from 400 m on the same face climbed on east
to 3,125 m. On grids it ran into the strip's end and the run dropped A to recover.

## Decision

1. **Roads on one wall may run both ways.** Running one way they stack as before (0032). Running opposite ways they sit
   side by side along the wall: climbing opposite ways over one stretch they would cross, so they can't share one.
2. **Each road is cut alone first to find the stretch of wall it takes:** from the bottom of its climb to its top, with
   its wall-side edge, and for a road that turned, its flat and the toe round its pad or box. An entrance isn't part of
   it, as it moves back only as far as there's room.
3. **Where two running opposite ways would share a stretch, or come within the wider road's width of each other, the
   later in the list stops** and names the other: "it would cross ramp A running the other way on this wall, or come
   within a road's width of it: move one of them along the strip". Tops facing closer than that leave a sliver of wall
   between them. So does one where the two leave no straight metre of toe between them to split on.
4. **The wall is cut in parts, split between runs of roads one way,** in the middle of the longest straight stretch of
   toe between them, so the walls either side rise on one plane and meet edge to edge. Each part takes its run's roads,
   cut in their own way, which stack within it as on any wall.
5. **Where two runs' entrances face each other** (roads climbing away from each other), each moves the toe back as far as
   the split, so they share the floor between them where their moved toes meet there. Where they don't, as with roads
   of different widths or a switchback's toe that joins back, the toe stays on the line for 0.5 m either side of the
   split, so the walls rise square to it, and each entrance comes across from there at 45 degrees.
6. **A berm the roads come up through steps round each road's top the way that road runs.** It took the first road's
   way for all.
7. **The Face design panel's ramp cell** says a wall can hold roads both ways.

## Results

Known answers on a level toe 2 km long, 45 degree walls 40 m high to RL 140, roads 10 m at 10 % (400 m), all to 1 µm
unless noted:

- Two climbing towards each other, east from 200 m and west from 1200 m: each road the same line as on its own, a
  section 200 m from each start the other's mirrored, the plain wall between their tops (RL 100 + metres out, to the
  crest at 40 m), and the toe behind each start back by the road, 10 cos(asin 0.1) m.
- Two climbing away from each other, west from 700 m and east from 1100 m: the toe 9.95 m back all the way between them,
  and one wall from there to its crest.
- The same with the east road 20 m wide: split at 899.5 m, halfway between the stretches they take (701 and 1098 m, the
  wall-side starts a metre and 2 m along as the roads climb at sin(angle) = 0.1); the toe on the line from 899 to 900 m,
  4.0 m out 5 m west of the split's hold and 5.0 m out 5 m east, and back by each road beyond.
- Opposite ways over one stretch, east from 200 m and west from 700 m: the second stops naming the first, which builds.
  Tops facing 7 m apart, under a road's width, the same; 17 m apart, both build.
- Both ways through one berm, two walls to RL 120 and 140: each the same line as on its own, 400 m.
- A straight ramp east from 100 m after the berm and a switchback from 1200 m: the switchback turns and is the same as on
  its own, 400 m climbing; the other way about, both the same as on their own.
- Every face one sheet with nothing crossing.
- Through the commands: a straight ramp and a switchback 800 m apart on a 1.5 km toe both reach RL 120 and the switchback
  turns; 150 m apart the switchback stops naming ramp A.

The Grid template, ten strips, roads 50 m at 10 % after each berm, straight ramp A from 400 m and switchback D (a string
added for the test) from 2,200 m, turning on every berm:

| Design | Before | After |
|---|---|---|
| To RLs, A and D | D never turns: climbs on east to 3,125 m on every strip | D switches back on all 9 strips it builds on, to RL 140, as alone; 16 of 20 roads to the top, 153 ms |
| Tim's 7 grid rows, A and D | D runs into the strip's end and the run drops A | 18 of 20, 2.3 s; the 2 stops are D's own, as with D alone |

The 4 stops to RLs are all the berm at the strip's west end folding over itself, which ramp A's 50 m entrance does on
its own on 2 strips. With the switchback at 1,000 m, 600 m from A, the two reach for the same stretch of wall above the
first berm: to RLs the switchback now stops naming A on 5 strips, where it used to be built running the wrong way. The
template's own check, all one-way designs, is unchanged at 23 faults.

## What this leaves

- **Shared floor only where the moved toes meet exactly.** On a bent or draped toe two entrances of one width can come
  out a little apart at the split, and then each comes across at 45 degrees, leaving a small wedge of wall between them.
- **Tops facing each other near twice the berm's width apart.** The crest between them is a tongue of plain wall a road
  deep; a berm offset from it leaves a narrow slot, and the wall above closes over it and pinches. With 10 m roads
  through a 30 m berm that's tops 55 to 80 m apart. The run says what's built can't be and drops the later road. Closer
  than a road's width is stopped up front (item 3).
- **The berm folding over itself at a strip's west end** behind a 50 m road's entrance happens with one road too, and
  wants looking at on its own.
- **A wall with roads both ways is cut twice:** once a road alone to find its stretch, once in its part.
