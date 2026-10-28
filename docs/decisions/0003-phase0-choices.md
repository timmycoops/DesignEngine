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
- **Render buffer:** positions, normals and indices first (4-byte aligned from offset 0), JSON
  header and its `u32` length last, so the encoder can time itself without copying. Positions are
  f32 offsets from a scene origin set by the first surface loaded (engine rule 2).
- **Rust minimum 1.90** for the workspace: Tauri 2.12 needs it, and Cargo's MSRV-aware resolver
  otherwise holds Tauri at 2.11 while pnpm installs the 2.12 JS packages.
