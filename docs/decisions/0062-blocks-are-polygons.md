# 0062 — Blocks are polygons: strips cut by seams, then by block polygons

Status: agreed 13 November 2026 (Tim: "Agree all eight", the Build Plan's "Block cutting plan" tab). Item 7 of the
agreed order, Phase 4 block cutting. Workstream 1 built.

## Why

On 13 November a made-up pit was taken from grids to interrogated blocks with the commands the app had
(`testdata/synthetic/reserves_workflow.py`). Every step ran, but cutting by the grids took two runs a seam and left
in-between solids and names growing with each cut; nothing set a block's `seam`; there was no block tool, and blocks made
from vertical faces overwrote the strip number; a seam with no thickness (U running into the weathering) refused the cut
on 2 of 6 strips; the 2.5D column check flagged every block with battered walls; and slivers were left where U thinned out.

Tim then said how he makes blocks: draw a line, offset it every 100 m, draw lines to close it, then make polygons from the
closed areas, as Deswik and Spry do. A block is a named polygon; a solid is cut vertically where it sits and inherits the
block's name. Deswik's block solids rule is the same shape: a layer of block polylines, the solids to cut, an optional
surface stacking rule in the same pass, a block attribute and a minimum volume.

## Decision

All eight as recommended:

1. **Polygons run across the strips**: the offset lines and two closing lines past the pit make one polygon per 100 m
   column across every strip. A piece's strip comes from its solid and its block from the polygon, so only the 100 m ends
   cut and the faces keep shaping the blocks' sides. One polygon per strip still works; its long sides then cut through
   the batter vertically.
2. **A stacked piece carries `seam`, `material` and `horizon`**: `seam` is its horizon for coal and, for waste, the seam
   below it (CLAUDE.md attaches overburden and interburden to the next seam down); `material` is coal or waste; `horizon`
   names the layer of the stack (OB, U, U-M, M and so on).
3. **A slab is made only where it's thicker than 0.01 m.** Thinner is left out and its volume said.
4. **Seams then blocks**, Tim's order. The two cuts don't depend on each other, so either order gives the same blocks.
5. **Names are `{strip}_{block}_{seam}`** by default, editable, checked unique.
6. **The column check takes the block's outline at mid-seam**, its section halfway between roof and floor, instead of its
   whole plan: the polygon Minex or Vulcan would use at the seam, so the batter drops out.
7. **No automatic small-piece limit**, as decided in Phase 1: a select-by-volume filter in the app.
8. **In the app**, By seams and By polygons are modes of the Cut solids panel; polygons from lines goes with the string
   tools.

## Workstreams

| # | What | Where |
|---|---|---|
| 1 | `string.polygons_from_lines` | zd-geom, zd-commands |
| 2 | `solid.cut_by_polygons` | zd-geom, zd-commands |
| 3 | `solid.cut_by_seams` | zd-geom, zd-commands |
| 4 | The column check at mid-seam | zd-geom, zd-commands |
| 5 | The app: two Cut solids modes, polygons from lines, select by volume | app |
| 6 | The made-up pit redone, then PIT B by counts and timings only | testdata, Python |

### Workstream 1: polygons from lines

`string.polygons_from_lines` takes strings, open or closed, and makes a closed string of every area they close in plan
(`zd_geom::polygons_from_lines`).

- **Joined where they cross.** Every segment is cut where another meets it, crossing, touching or lying along it. Points
  within 1 µm in plan are one point.
- **Ends that stop short.** An open line's end within the tolerance (1 mm by default) of another line is joined to it by
  a stretch of its own, and the join is said (`string.end_joined`, with the gap). Nothing is moved.
- **What closes nothing is left out.** Stretches with a free end are taken off end by end until every point is on a ring.
  A free end outside every polygon runs on past the last line it crosses, which is how block lines are drawn: those are
  counted, with the length of line left out. A free end inside a polygon was meant to split it and doesn't: a warning
  (`string.end_short`) gives the line, the place, the gap to the nearest line and the polygon. A line that closes nothing
  at all is named (`string.closes_nothing`). A ring lying inside a polygon without touching it is an island
  (`string.island`): it doesn't split the polygon, and its own polygons overlap it. With no area closed at all the run is
  refused, saying why.
- **The polygons** are the faces of the lines with the area on their left, anticlockwise, each vertex at the height of
  the line it lies on. They're named from a pattern (`Polygon #` by default), numbered west to east by centroid, then
  south to north; a line drawn across them (`string.number_across`) numbers them as blocks.

Checked on made-up lines with answers worked by hand (`crates/zd-geom/tests/polygons.rs`, areas to 0.001 m²): a 2 × 2
grid of 100 m squares closes four of 10,000 m², the same with the lines running 20 m past (12 free ends, 240 m left out);
a line 5 mm short joined at 1 cm and listed at 1 mm, the two squares either side then one of 20,000 m²; a line crossing
nothing named; a closed string split by a line across it; a closing line drawn along another adding nothing; an island;
and Tim's layout, two bent lines of 2,000 vertices each and 15 lines across every 100 m, closing 14 blocks of 36,000 m²
(4,013 segments in, 30 crossings, 6.7 ms in release). Through the registry (`crates/zd-commands/tests/blocks.rs`): named
blocks in one undo, a short end joined or listed, lines closing nothing refused or named.

### Workstream 2: cut by polygons

`solid.cut_by_polygons` cuts each solid by each block polygon's vertical prism (`zd_geom::cut_solid_by_polygons`): the
polygon in plan, either way round, extruded past the solid's top and bottom, one Manifold boolean each about a
whole-metre origin near the solid. Only polygons whose plan box meets the solid's are cut with.

- **A block keeps its solid's attributes** (`strip` stays) and takes the polygon's name as `block` (the attribute can be
  named). The polygon's other attributes come with it where the solid hasn't one of the same name: the solid's own win.
- **Names** come from a pattern of attributes in braces, `{strip}_{block}_{seam}` by default, with `{solid}` and
  `{polygon}` for the names. One a block hasn't is left out with the separator before it, so a block with no seam yet is
  `5_B07`. Names repeated across the run are said (`block.name_repeated`), with how many share them.
- **What lies outside every polygon** is one remainder for each solid, on its own layer (`<layer> outside` by default),
  keeping the solid's attributes. A solid wholly outside is kept whole there and said (`solid.outside_polygons`).
- **Overlapping polygons are refused**, naming the pair, the overlap's area and where it is: a block there would be
  counted twice. Overlaps under 0.01 m² are a shared edge drawn twice.
- **The pieces and remainder add back to the solid**; a difference over a millionth of its volume is said
  (`block.volume_differs`). Open strings are taken as closed and said. Each piece is searched for crossings on its own
  thread, and blocks in more than one part are said by the solid rules' check every run goes through.
- Solids are cut in parallel, with progress and Cancel.

Checked with volumes worked by hand (`crates/zd-geom/tests/blocks.rs`, to 1 part in a million): a 1,500 × 60 × 30 m
strip cut by 15 polygons into blocks of 180,000 m³ that add back exactly; a strip with 65° sides cut by polygons wider
than it, each piece keeping the batter (its trapezoid section × 100 m); the east 500 m outside ten polygons as the
remainder; a square turned 30° taking its own area; a clockwise polygon, a U-shaped polygon making one piece of two
parts, a far polygon left out; overlaps found and shared edges not. Through the registry
(`crates/zd-commands/tests/blocks.rs`): polygons from lines then 15 named blocks in one undo; the solid's attributes
winning over the polygon's; the remainder and a solid wholly outside; repeated names said and told apart by the
pattern; overlapping polygons refused.

Timing on the made-up pit (`reserves_workflow.py`): its 32 strip-level seam and waste solids with 65° and 45° faces,
92,510 triangles in, cut by 15 polygons into 471 blocks (99,558 triangles out) in 107 ms, adding back to within 1e-13.
Their names repeat until the solids carry `seam` (workstream 3).

## Still to come

Workstreams 3 to 6, each in its own PR.
