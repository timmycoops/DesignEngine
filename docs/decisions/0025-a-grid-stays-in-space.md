# 0025 — A grid stays in space where a seam has no thickness

Date: 2026-11-01 · Status: accepted (Tim, 2026-11-01)

## Why

Tim: "we want to ensure that the grid nodes exist otherwise everything is really hard to project to and also use to
cut the solids after... The thickness may be zero but the grids can still exist in space."

The reader made any node without the live flag a null, and only counted what it held. On PIT B that punched holes in
45 to 92 % of every roof and floor. Decision 0024 bridged them for walls, and a cut by one of those grids was refused
outright.

## What the files hold

PIT B, 10 seams, 266,031 nodes a grid, 20 roofs and floors:

- Every node flagged off holds a height. None is empty.
- Roof equals floor exactly at every one of those nodes, and roof is above floor at every live node. The flag says
  the thickness is above zero, and nothing else.
- The thickness grid is live everywhere, and reads 0 at those nodes.
- Those heights run on from the live nodes: a median step of 0.3 to 0.5 m from a live node to a held one, against
  0.4 to 0.5 m between live nodes.
- With every node's height, no seam crosses the next in any of the 9 pairs, to 1 mm.
- At 38.9 % of nodes all 20 surfaces are one height. The held top of the stack sits on a live-everywhere grid to 1 mm.

This isn't every folder. PIT A and a third folder have nothing flagged off: they mark zero thickness on live nodes. In
a fourth, 1 of 198 surfaces has nodes flagged off, and what those hold steps 35.8 m from the live nodes against 4.8 m
between them, so it isn't a surface. One other PIT B grid is the same: its live nodes are the model's own area, and the
65,520 flagged off are a margin outside it.

## Decision

1. **A roof, floor or topography imports whole.** `surface.import` and the elevation grids of
   `grid.import_vulcan_seam` read `VulcanGrid::surface`, where a node flagged off keeps the height it holds.
2. **A seam model reads the flag as before.** It's where the seam is, and it agrees with thickness at every PIT B node,
   so sections, fills and the stack checks don't change. `VulcanGrid::grid` stays live nodes only.
3. **Qualities and thicknesses stay as they read.** A quality at a node with no seam isn't a value to fill in.
4. **A node with nothing in it stays null**: flagged off holding zero or a NaN, or carrying an unknown flag.
5. **Held heights that don't run on from the live nodes stay null, with a warning.** The test is the 95th percentile
   step from a live node to a held one beside it, against the same between live nodes: more than 3 times, and over
   1 m so flat live nodes don't make every step rough. PIT B's seams reach 1.9 times, and the two files it catches
   6.6 and 7.4. A grid with no live nodes has nothing to judge by, and keeps what it holds.
6. **The import says what it did**, nodes kept or left null, in the run log.
7. **One solid that can't be cut doesn't lose a batch.** It's left uncut and named, as one whose parts fail their
   check already was. A single solid is still refused, as there's nothing else to go on with.

Decision 0024's bridging stays for gaps that really are gaps, such as the edge of a `.00t`.

## Evidence

Tim's 620 m string on PIT B, up and down, on five seam grids of 530,000 triangles and the real topography:

| | Live nodes only | Whole grids |
|---|---:|---:|
| Stretches bridged over a hole | 3 | 0 |
| Berm vertices with no surface under them | 10 to 12 | 0 |
| Toe vertices off their grid | 1 | 0 |
| Steps with stretches of no height | 1 to 3 of 10 | 1 of 10 |
| Time for both faces | 65 ms | 76 to 80 ms |
| Triangles out, up | 5,199 to 5,312 | 5,840 to 5,914 |
| Triangles out, down | 417 to 441 | 449 to 473 |

Every face has one boundary loop, no crossing triangles and consistent winding, either way.

All 41,917 PIT B solids, 205.0 M m³, cut by a grid:

| Grid | Live nodes only | Whole grid |
|---|---:|---:|
| A lower seam floor, 50.5 % flagged off | Refused | 44,727 parts, 22 s |
| The sparsest seam floor, 92.3 % flagged off | Refused | 42,089 parts, 19 s |

Parts add up to the solids within 0.44 m³ in both, and none of the design falls short of a solid in plan. Cut by a seam
roof, 59.1 % flagged off, it's 46,232 parts in 23 s, and one solid of 1,950 m³ is left uncut and named, as its lower part
would cross itself (decision 0016). That and the parts add up the same.

## Known limits

- **Not checked against an export.** What a flag means comes from the files and from Tim, not from Vulcan or Deswik
  reading a grid with nodes flagged off. Decision 0006's open item stands.
- **A surface already in a project stays as it was imported.** Import it again to get the whole grid.
- **Whole seam grids sit on each other** where the stack has closed up, so walls between them have no height there,
  and imported as surfaces they draw as one line in a section. The seam model doesn't, as it stops at the flag.
