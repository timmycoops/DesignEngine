# 0003 — Phase 0 implementation choices

Date: 2026-10-28 · Status: accepted (inside the locked stack)

## Decisions

- **Manifold binding: `manifold-csg` 0.4.** `manifold3d` on crates.io is now only a facade that
  re-exports it. It builds Manifold 3.5 from source (git clone + CMake, TBB on) and compiles
  cleanly on Windows/MSVC in about 1.5 minutes cold. Mesh exchange uses `MeshGL64` (f64).
- **Spatial queries: `parry3d-f64` 0.31** (the f64 build of parry3d; it now uses glam types).
- **DXF: our own `3DFACE` reader and writer in `zd-io`** instead of a DXF crate. Faces in BLOCKS are
  skipped; shared corners weld by exact f64 equality; quads, degenerate and duplicate faces are
  counted in the import log. Binary DXF is refused with a clear error.
- **Timing harness: `std::time::Instant`**, no criterion.
- **Booleans run about a local origin.** `cut_solid_by_surface` moves both meshes to an origin near
  the solid (a round number, so the shift is exact) and back after. Manifold's own `volume()` sums
  at raw mine-grid coordinates and drifts by ~1e-9 m³ on a 10 m³ test solid; volumes we report come
  from our own centred divergence sum.
- **Closing a design surface into a half-space:** orient by flood fill, walls down from the
  boundary loops, and a flat cap below the solid's bounding box. The cap is triangulated from the
  boundary loops with Manifold's constrained Delaunay; if that fails or the result is not watertight,
  the surface's own triangles are flattened onto the cap plane instead (always topologically valid
  for a single-valued surface). The method used is reported.
- **Seam solids (Phase 0 only):** roof and floor on one lattice, thickness clamped at zero, cells
  thinner than a minimum left out, diagonal corner-pinch cells removed so the solid stays manifold,
  vertical walls on the footprint boundary. Every exclusion is logged. Pinch-outs lose the wedge
  inside the last cell; Phase 1 replaces this with proper horizon stacking.
- **Render buffers (v2, after the first Check 3 run):** arrays first (4-byte aligned from offset 0),
  JSON header and its `u32` length last, so the encoder can time itself without copying. There are
  no normals: three shades flat from screen-space derivatives when a geometry has none. Two layouts:
  - *Triangulation:* spatially compact chunks of at most 65,535 vertices (plan bins of about 100 k
    triangles) with `u16` indices and f32 positions offset from the scene origin (engine rule 2).
    About 12 bytes per triangle, down from 24.
  - *Grid:* only elevations, as `u16` steps of (surface z range / 65,534), in 255 x 255-node tiles.
    One step for the whole surface means tiles quantise their shared edges identically, so no
    cracks. The viewport rebuilds x and y from the lattice and the triangles by the same rule as
    `triangulate_grid` (a Rust test proves the rule matches; the viewport checks the triangle count
    on every decode). About 1 byte per triangle. The worst-case elevation error is half a step:
    0.8 mm on a 100 m range, 3.1 mm on a 400 m one. That's display only; the engine keeps f64.
  - Index buffers are marked `normalized` so three's WebGPU backend uploads them as native
    `uint16` instead of widening them to `u32`.
- **Transport: WebView2 shared buffers** (`ICoreWebView2SharedBuffer`, via `webview2-com` 0.39
  and `windows-core` 0.62, the versions Tauri's wry already uses). Rust copies the buffer once
  into shared memory and posts it; the page gets an `ArrayBuffer` over the same memory. Raw bytes
  over Tauri IPC (`ipc::Response`, ~210 MB/s on WebView2) remain the fallback.
- **Rust minimum 1.90** for the workspace: Tauri 2.12 needs it, and Cargo's MSRV-aware resolver
  otherwise holds Tauri at 2.11 while pnpm installs the 2.12 JS packages.
