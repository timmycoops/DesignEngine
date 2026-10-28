# First task for Claude Code — Phase 0 stress test

Paste this into Claude Code (run locally on the Windows PC, from the repo root):

---

Read CLAUDE.md. We are starting Phase 0: proving the stack on real data before any feature work.

Build the minimum needed to run four timed checks, and nothing more:

1. **Workspace skeleton.** Create the Rust workspace (`crates/zd-core`, `crates/zd-geom`, `crates/zd-io`,
   `bench`) and a Tauri 2 + React + TypeScript app in `app/` using pnpm. Viewport uses three.js
   `WebGPURenderer`. No UI beyond a full-window viewport, an FPS counter, and a file-open button.
2. **Import.** In `zd-io`, read a triangulation from DXF (3DFACE) and a grid from XYZ/CSV; convert grids to
   triangulations. Store coordinates as f64.
3. **Check 1 — viewport.** Load the largest topography and seam surfaces, shift to a local origin, render.
   Report triangle count and frames per second while orbiting. Then duplicate or densify the surfaces until
   the scene holds tens of millions of triangles (the incumbent bar) and report again.
4. **Check 2 — booleans.** Build seam solids between roof and floor surfaces, then cut them with a design
   surface (closed into a half-space beyond the solid's bounds) using Manifold. Report time per cut,
   triangle counts, and confirm every output is watertight with a non-zero volume.
5. **Check 3 — transfer.** Send a multi-million-triangle mesh from Rust to the viewport as binary typed
   arrays. Report time from request to first frame.
6. **Check 4 — editing.** Draw a polyline on a surface; drag a vertex with snapping to the surface.
   Report the time per drag update.

Write results to `bench/RESULTS.md` (machine specs, data sizes, timings, pass/fail against the targets in
docs/decisions/0001-stack.md). Plan first, show me the plan, then build on a branch.

Test data is in `testdata/golden/<pit-name>/` (not committed if confidential).
