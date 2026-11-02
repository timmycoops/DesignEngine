# 0035 — Ramps after the berm

Date: 2026-11-02 · Status: accepted. Tim asked how the berm and ramp line up: "Is the ramp starting after the berm or is it on the
projection lines? ... Generally it's neater after the berm". It was on the projection lines (0028 item 9). Of the three ideas put to him
he chose a choice on each ramp, with after the berm the default. This changes `Ramp`.

## Why

A road went up through each berm at its grade. The berm narrowed to nothing where it met the road, so there was no way off the road onto
it, and the road drifted in a berm width over ten berm widths ahead while the berm opened out again below it. Every package that designs
ramps offers both ways:

| Package | Through the berm | After the berm |
|---|---|---|
| Vulcan | Rock access off: the berm narrows and is cut off at each bench | Rock access on: the berm stays open to the road |
| Datamine Studio OP | Toe-Ramp: no berm offset at the top of the ramp, a continuous ramp | Toe-Ramp-Crest: the next toe offset by the berm, ramps with access to the berms |
| Deswik | Taper berm to zero width | Do not alter berm contours (setting names only, from the API) |
| Minex | | The road widens to its width and the berm's over a taper |

## Decision

1. **A ramp says how it gets past a berm: after it or through it.** After is the default, and a design saved before this takes it.
2. **After the berm, the berm keeps its full width.** It's laid round the top of the road as the crest is, so its line steps round that
   end a berm width on along the road. The road crosses the berm level, from its top to the foot of the step, and climbs the next wall
   from there, its wall-side edge at the top of the step, where the line behind runs on. The berm opens onto the top of the road.
3. **The foot of each stretch of road is a straight edge,** so the berm's line runs straight between its two ends. On a grid the
   step's corner can stand off that edge by most of a metre, and left a sliver of a hole between the berm and the road.
4. **Through the berm is as it was,** 0028 item 9.
5. **The landing is a berm width along the road.** The road climbs at sin(angle) = grade / slope off the strike, so across a berm of
   width B the landing is B·√(((1 − sin)/cos)² + 1): 13.48 m for a 10 m berm at 45 degrees, 27.8 m for 20 m at 70. Its length isn't a
   setting yet.
6. **In the Face design panel** each ramp's row has After berm or Through berm.

## Results

Known answers, a 10 m road at 10 % up two 45-degree walls of 20 m with a 10 m berm between: 200 m of climb on each wall, a level landing
of 13.484 m at RL 120 to 1 µm, the berm 10 m wide everywhere to 1 µm, where through it the berm comes in to nothing at the road. Roads
together on three walls, close together, and stacked running back with the wall on the right each climb their 20 m a wall and cross each
berm on one landing, every face one sheet with nothing crossing. Through the commands, as the panel saves it: 206.7 m after the 5 m berm,
200.0 m through it.

The RL template, five strips, roads A and B up three walls: every road climbs 1,200.0 m and crosses the two 20 m berms on landings of
27.8 m, 1,255.6 m in all. Every face is one sheet with nothing crossing, either way, in 5 ms.

The grid test pit, after the berm against through it:

| Design | Faces built | Roads to the top | Faces with triangles crossing | Faces not one sheet |
|---|---|---|---|---|
| Grid ramps | 10 → 10 | 13 → 11 of 20 | 0 → 0 | 3 → 3 |
| Flat toe to grids | 10 → 10 | 19 → 18 of 20 | 0 → 0 | 2 → 2 |
| Grid toe to RLs | 10 → 10 | 17 → 17 of 20 | 0 → 0 | 0 → 0 |

Every road stopped either way is fault 3 of 0034, a wall above it reaching past the target with no crest on it. Going after the berm
moves which roads it stops.

PIT C, each of 11 strips on its own, all three ramps on all four walls with 30 m berms, as in 0033, through the berm against after it:

| Toe | Walls to | Roads to the top | Stopped by a wall fault | Faces not one clean sheet |
|---|---|---|---|---|
| RL -150 | RLs | 66 → 66 | 0 → 1 | 0 → 0 |
| Tim's grid | RLs | 78 → 73 | 3 → 2 | 0 → 0 |
| RL -150 | Tim's grids | 75 → 69 | 6 → 9 | 1 → 3 |
| Tim's grid | Tim's grids | 70 → 65 | 18 → 16 | 1 → 0 |

- **Fewer roads reach the top, because each landing makes a road longer** (about 42 m across a 30 m berm at 70 degrees) and moves
  where it starts on the walls above. With the toe on his grid going to RLs, through against after: off the end of the wall 22 against
  26, two roads wanting one start 8 against 12, dropping back to the toe 15 against 12, can't leave the toe 6 against 7, wall faults 3
  against 2.
- **The three faces not clean with a flat toe to his grids** are each a road that now reaches the top wall, with triangles crossing at
  its top end. That's the road's top end across an oblique crest, found in 0034. Going through, those roads stopped a wall lower. The
  one face grid to grid that wasn't clean, a lone triangle of the same kind, is clean now. Two roads with a flat toe to his grids stop
  on the top wall with "the road can't be built", which is new, and of the same place.

## What this leaves

- **A road's top end across an oblique crest,** which more roads reach going after the berm: three faces on PIT C.
- **The landing's length** as a ramp setting, and other ways onto a berm: a flat pad, or the road widening over a taper as Minex has it.
- **Fault 3,** a road's wall with no crest on it, which going after the berm moves about.
- **Switchbacks,** which will sit on a berm.
