# 0001 — Stack

Date: 2026-10-27 · Status: locked, pending Phase 0

## Decision

Tauri 2 desktop app (Windows first). Rust geometry engine as a separate workspace of crates with no UI code.
Manifold (via the `manifold-csg` / `manifold3d` Rust bindings, f64) for solid booleans; parry3d for ray casting;
spade or Manifold for triangulation; Clipper2 or Manifold CrossSection for planar offsets. React + TypeScript
interface with three.js `WebGPURenderer` for the viewport. Parquet/Arrow and binary meshes at rest.

## Why

- Speed, feel and look are the priorities. The engine must sit in the same process as the renderer: no server
  round trips.
- Rust gives C++-class speed with memory safety, and one engine can run native, in a browser (via Emscripten,
  which manifold-csg supports) or from Python (PyO3).
- three.js is the most used web 3D library; its WebGPU renderer is the recommended default with automatic
  WebGL2 fallback.
- React has the best ecosystem for forms, tables and panels.
- Rejected: Python + Qt/VTK (lower ceiling on feel and look, GIL stutter); web front end + Python server
  (latency on every engine call); C++/CGAL (slow to build in); Blender as host (not a mining tool,
  precision issues); QGIS (weak 3D).

## Phase 0 targets (all must pass to keep the stack)

| Check | Target |
|---|---|
| Viewport with largest real surfaces, scaled to tens of millions of triangles | Smooth orbit and zoom, ~60 fps |
| Manifold cut of seam solids by a design surface | Watertight output; seconds, not minutes, per cut |
| Multi-million-triangle mesh, Rust → viewport | Well under 1 second, binary transfer |
| Vertex drag with surface snapping | Feels instant (no visible lag) |

A failure points to one layer to fix (tuning, transfer format, or a library swap inside `zd-geom`), not to
a stack change.
