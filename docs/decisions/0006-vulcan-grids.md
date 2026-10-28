# 0006 — Reading Vulcan grids

Date: 2026-10-28 · Status: accepted; the format reading stays **unverified** until checked against an
independent export (below)

## Decision

Seam models come from Vulcan grids (decision 5 of the Phase 1 plan). Files are named
`<proj><seam>.<variable>g`: `.srg` roof, `.sfg` floor, `.tkg` thickness, and quality codes such as `.rag`
raw ash, `.rdg` relative density, `.rvg` volatile matter and `.tsg` total sulphur. `zd-io` reads the
plain binary layout directly (`read_vulcan_grid`, `read_vulcan_seam`), and the
`grid.import_vulcan_seam` command brings a seam's grids into a project.

There is no public specification. The layout was worked out from a client's folder of about 3,000
grids (client data: kept local, never committed, and not quoted here). Big-endian:

| Bytes | Content |
|---|---|
| 0..32 | free text: name and timestamp, or how a derived grid was made (e.g. `…:TK:SR-SF:…`) |
| 32..64 | f64 x0, x1, y0, y1: extents of the node lattice |
| 64..68 | i32 node count (cols × rows) |
| 68..72 | i32, zero in every sample |
| 72..80 | f32 dx, dy |
| 80..88 | i32 cols, rows |
| 88.. | per node: f32 value, i32 flag (1 = live, else null; null nodes can hold leftover values) |

Nodes run row by row, x fastest, from the south-west corner. Files may end in zero padding. Deswik
writes the same layout (its files carry `_dw_VulcanGrid` in the header).

The reader refuses anything that does not check out: a node count that disagrees with cols × rows,
extents that disagree with the cell size, a truncated file, or non-zero bytes after the last node.
Null nodes, nodes with leftover values and unknown flags are counted in a log, never dropped silently.

## Evidence so far

- **Orientation.** Read with row 0 as the south edge, the sample topography grid matches a topography
  DXF of the same area at a median of 2 m; read the other way, 74 m.
- **Consistency.** Two grids written separately from one model run (thickness and structural
  thickness) agree at every shared node, over 160,000 in each seam checked, to within 1 mm.
- **Model revision.** Where an older thickness grid differs from a newer roof minus floor, the
  differences fall in contiguous revised areas, not scattered.
- **Plausibility.** Every seam with a roof and floor in the sample reads (about 2,950 grids), the floor is
  never above the roof, and qualities fall in physically plausible ranges.

## Still open

- **Node-for-node check** against an export of the same grids from Deswik or Vulcan (XYZ or ESRI
  ASCII). Tim is exporting a seam next. The opt-in tests take `ZD_VULCAN_GRIDS`, `ZD_VULCAN_PREFIX` and
  `ZD_VULCAN_SEAM`: `cargo test --release -p zd-io --test vulcan_golden -- --ignored --nocapture`.
- **The compressed layout (`vulZ`, about 5 % of the sample)** is not read. Its header is
  little-endian, records the uncompressed size and chains 2 KB blocks, and it is not zlib. Those grids
  are skipped and listed; export them instead.
- **Variable codes.** Many quality codes (e.g. `s1`–`s8`, `t1`–`t8`, which look like washability yields and
  ash by density) need the project's code key before the seam model names them.
