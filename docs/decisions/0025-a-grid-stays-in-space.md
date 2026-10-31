# 0025 — A grid stays in space

Date: 2026-11-01 · Status: accepted (Tim, 2026-11-01)

## Why

Tim: "we want to ensure that the grid nodes exist otherwise everything is really hard to project to and also use to
cut the solids after... The thickness may be zero but the grids can still exist in space."

The reader made any node without the live flag a null, and only counted what it held. On PIT B that punched holes in
45 to 92 % of every roof and floor. Decision 0024 bridged them for walls, and a cut by one of those grids was refused
outright.

## What the files hold

**PIT B**, 10 seams, 266,031 nodes a grid, 20 roofs and floors:

- Every node flagged off holds a height. None is empty.
- Roof equals floor exactly at every one of those nodes, and roof is above floor at every live node. Here the flag says
  the thickness is above zero, and nothing else.
- The thickness grid is live everywhere, and reads 0 at those nodes.
- Those heights run on from the live nodes: a median step of 0.3 to 0.5 m from a live node to a held one, against
  0.4 to 0.5 m between live nodes.
- With every node's height, no seam crosses the next in any of the 9 pairs.
- At 38.9 % of nodes all 20 surfaces are one height. The held top of the stack sits on a live-everywhere grid to 1 mm.

**PIT C**, 179 surfaces on one 533 x 505 lattice, marks zero thickness on live nodes, with one exception. Its `fr4` roof
has 83,778 nodes flagged off, and there the flag doesn't mean zero thickness:

- The roof is above the floor at 56,940 of them, 40.6 M m³ of the seam's 163.4 M m³.
- Roof minus floor equals the thickness grid to 1 mm at 73 % of them, against 87 % of live nodes.
- Those heights are in order with the seams either side, with no crossing.
- They step 35.8 m from the live nodes at the 95th percentile, against 4.8 m between live nodes, and the floor, which is
  live on both sides, steps the same across the same edge: within 10 cm at 2,489 of 3,000 node pairs, up to 144 m.

So that step is a break in the geology, most likely a fault, and the flag there has some other cause. The file's header
says it was made from a triangulation, unlike its neighbours.

PIT A and a third folder have nothing flagged off.

## Decision

1. **A roof, floor or topography reads whole, everywhere.** `VulcanGrid::surface` gives `surface.import`, the elevation
   grids of `grid.import_vulcan_seam` and a seam model's roofs and floors every height the file holds.
2. **A seam exists where its roof is above its floor**, not where a flag says, as Tim first put it in decision 0020. The
   seam view draws the whole roof with no coal in grey. A section stops where the roof meets the floor, so PIT B's
   sections don't change. The stack check covers every node, because a surface that carries on has to be in order.
3. **Qualities and thicknesses stay as they read.** `VulcanGrid::grid` is live nodes only.
4. **A node with nothing in it stays null**: flagged off holding zero or a NaN, or carrying an unknown flag.
5. **A large step is reported, never held back.** The import warns when the 95th percentile step from a live node to a
   held one is more than 3 times the same between live nodes, and over 1 m. The first version of this decision left such
   heights null and called them not a surface, and that was wrong: a grid on its own can't tell a fault from bad data,
   and it held back a good roof. Whether the heights fit is what the stack check shows.
6. **The import says what it did**, nodes kept and any large steps, in the run log.
7. **One solid that can't be cut doesn't lose a batch.** It's left uncut and named, as one whose parts fail their check
   already was. A single solid is still refused, as there's nothing else to go on with.

Decision 0024's bridging stays for gaps that really are gaps, such as the edge of a `.00t`.

## Evidence

Seam models through the engine, with every node checked:

| Model | Horizons | Blocking issues |
|---|---:|---:|
| PIT B | 10 | 0 |
| PIT C, the suggested stack | 92 | 7, each a crossing, up to 279 m |
| PIT C, Tim's stacking order | 92 | 0 |

The suggestion has those 7 neighbouring pairs the wrong way up: it compares median gaps, and where surfaces sit exactly on
each other over most of the area the median is zero. In Tim's order there's no crossing in any of the 178 neighbouring
pairs. `fr4` was also blocking, as an unpaired roof and floor at 83,778 nodes, until this decision.

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

- **A truly leftover height would now be used.** The warning and the stack check are the only guards. At 27 % of `fr4`'s
  flagged nodes roof minus floor doesn't match the thickness grid, against 13 % of its live nodes, and those grids were
  written months apart, so that alone doesn't settle it.
- **Not checked against an export.** What a flag means comes from the files and from Tim, not from Vulcan or Deswik
  reading a grid with nodes flagged off. Decision 0006's open item stands.
- **Import again and rebuild.** A surface already in a project stays as it was imported, and a saved seam model keeps the
  issues it was built with.
- **The suggested stack still has that fault.** A definition from `seam.suggest_definition` wants checking against the
  planner's own order.
- **Whole seam grids sit on each other** where the stack has closed up, so walls between them have no height there, and
  imported as surfaces they draw as one line in a section. The seam model doesn't, as it stops where a roof meets its floor.
