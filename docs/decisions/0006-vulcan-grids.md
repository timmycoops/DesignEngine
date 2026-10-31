# 0006 — Reading Vulcan grids

Date: 2026-10-28 · Status: accepted; the plain layout **verified** node for node against Vulcan's own
export on 2026-10-29 (below)

## Decision

Seam models come from Vulcan grids (decision 5 of the Phase 1 plan). Files are named
`<proj><seam>.<variable>g`: `.srg` roof, `.sfg` floor, `.tkg` thickness, and quality codes such as `.rag`
raw ash, `.rdg` relative density, `.rvg` volatile matter and `.tsg` total sulphur. `zd-io` reads the
plain binary layout directly (`read_vulcan_grid`, `read_vulcan_seam`), and the
`grid.import_vulcan_seam` command brings a seam's grids into a project. The compressed layout
(`vulZ`) reads too (below, added 2026-10-29).

There is no public specification. The layout was worked out from a client's folder of about 3,000
grids (client data: kept local, never committed, and not quoted here). Big-endian:

| Bytes | Content |
|---|---|
| 0..32 | free text: name and timestamp, or how a derived grid was made (e.g. `…:TK:SR-SF:…`) |
| 32..64 | f64 x0, x1, y0, y1: extents of the node lattice |
| 64..68 | i32 node count (cols × rows) |
| 68..72 | zero, or (Deswik) the f32 cell size again; not used |
| 72..80 | f32 dx, dy |
| 80..88 | i32 cols, rows |
| 88.. | per node: f32 value, i32 flag (1 = live, else null; a null node can still hold a height, decision 0025) |

Nodes run row by row, x fastest, from the south-west corner. Files may end in zero padding. Deswik
writes the same layout (its files carry `_dw_VulcanGrid` in the header), and may end a file with copies of
the first node's f64 x and y: two to four in the 31 grids of one client's folder (2026-10-31). They are
passed over and counted; nothing else is allowed after the last node.

The reader refuses anything that does not check out: a node count that disagrees with cols × rows,
extents that disagree with the cell size, a truncated file, or anything else after the last node.
Null nodes, nodes holding a value and unknown flags are counted in a log, never dropped silently.

### Compressed layout (`vulZ`)

Some grids are stored compressed: about 5 % of the first folder and a fifth of a second client's folder,
where they included roof and floor grids. A compressed file holds the plain layout's bytes, cut into
fixed-size blocks, each compressed with LZF (the public liblzf format). Little-endian:

| Bytes | Content |
|---|---|
| 0..8 | magic `ea fb a7 8a` then `vulZ` |
| 8..12 | i32, 2 in every sample |
| 12..16 | i32 index depth: levels of index pages, the root included (4 in every sample) |
| 16..20 | i32, 1 in every sample |
| 20..24 | i32 block size, in plain bytes (25,600 in one folder, 40,960 in the other) |
| 24..28 | i32 offset of the root index page (60 in every sample) |
| 32..40 | u64 size of the plain layout |

An index page is 2,048 bytes: 256 u64 file offsets, zero for unused. Each level's entries point to the
pages of the next level down, and the deepest level's entries point to the blocks in order. A block is a
u32 compressed length, a u32 slot length (the space reserved for it), then the LZF stream. The last
block may unpack past the end of the grid; those bytes are zero. The decoder is written in-house (LZF is
about 40 lines), so there is no new dependency.

The reader refuses a compressed file whose header does not fit the file, whose index lists too few
blocks, whose blocks run past the end of the file or fail to unpack, or whose last block holds data past
the stated size. A grid read from it reports how many blocks were unpacked.

**Evidence.** All 182 compressed grids in the two folders unpack to exactly the stated size and parse as
the plain layout, on the same lattice as their plain neighbours. In the second folder, eight seams have a
compressed roof, floor or thickness. In each, roof minus floor equals the thickness grid exactly (to the
last bit of single precision) at all 266,031 nodes, including one seam where the floor is plain. In the
first folder, a compressed thickness equals a compressed `tk` (written separately, from roof minus floor)
at every node in each seam checked. The plain roof and floor there were rewritten two months after the
thickness (the header text carries the dates). Roof minus floor still equals the compressed thickness
exactly at 99 % or more of nodes in three seams. Six others differ more, as expected where the roof and
floor were remodelled. A wrong decoder could not match exactly at hundreds of thousands of nodes.
Unpacking takes about 4 ms for a 2 MB grid.

## Evidence so far

- **Node for node against Vulcan** (2026-10-29). A floor grid that Vulcan wrote in the plain layout
  (501 × 321 nodes at 10 m, every node live) and Vulcan's own CSV export of it agree at all 160,821
  nodes: same origin, same cell size, every value equal to the last bit of single precision. This is the
  independent check decision 0004 asked for before a binary reader is used. A compressed grid unpacks to
  the plain layout, so its parsing rests on this check and its unpacking on the evidence above.
- **Orientation.** Read with row 0 as the south edge, the sample topography grid matches a topography
  DXF of the same area at a median of 2 m; read the other way, 74 m.
- **Consistency.** Two grids written separately from one model run (thickness and structural
  thickness) agree at every shared node, over 160,000 in each seam checked, to within 1 mm.
- **Model revision.** Where an older thickness grid differs from a newer roof minus floor, the
  differences fall in contiguous revised areas, not scattered.
- **Plausibility.** Every seam with a roof and floor in the sample reads (about 2,950 grids), the floor is
  never above the roof, and qualities fall in physically plausible ranges.

## Still open

- **Nulls and compressed files against an export.** The grid checked was plain and had no null nodes.
  Exports of a grid with nulls and of a compressed grid would close both. Set `ZD_VULCAN_EXPORT` (the
  text export) and `ZD_VULCAN_EXPORT_GRID` (the grid file) and run
  `cargo test --release -p zd-io --test vulcan_golden grid_matches_vulcan_export_node_for_node -- --ignored --nocapture`.
  The other opt-in tests take `ZD_VULCAN_GRIDS`, `ZD_VULCAN_PREFIX` and `ZD_VULCAN_SEAM`.
- **Variable codes.** Many quality codes (e.g. `s1`–`s8`, `t1`–`t8`, which look like washability yields and
  ash by density) need the project's code key before the seam model names them.
