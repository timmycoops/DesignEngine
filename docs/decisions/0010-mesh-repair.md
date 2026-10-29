# 0010 — The import repair step and crossings

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29); for closed solids, amended by decision 0014 (thin features, 2026-10-30)

## Decision

Workstream 4, slice 2: the repair step engine rule 8 asks for (weld by tolerance, drop empty and
duplicate faces, orient by flood fill, find crossings), with every change logged. It is what the
Phase 1 gate's "the import log lists every repair made" checks. Closing open meshes into solids and
repairing crossings are not part of it: they change geometry, so they would be a command of their
own, run on purpose.

1. **Where it runs.** DXF meshes are repaired inside `surface.import`, which gains `weld_tolerance`.
   Grids have nothing to weld or orient and cannot cross themselves (one elevation per node on a
   regular plan lattice); they are recorded as checked for that reason. Meshes given inline
   (`surface.from_mesh`, `solid.from_mesh`) are checked, never changed, so a script gets back exactly
   what it sent.
2. **Weld: 1 mm by default, in 3D.** Each vertex merges into the first vertex already kept within the
   tolerance, in the file's order: every kept coordinate is exactly as read, never averaged, and
   merges cannot chain along a line of close points. Triangles the weld collapses are dropped, as are
   those of zero area and duplicates (the same three vertices either way round, the first kept), and
   vertices no triangle uses. Speed does not depend on the tolerance; it decides which points are the
   same point written twice. The worst-case volume change is surface area times tolerance, which is
   why it stays small next to seam thickness (a 0.3 m ply over 100 x 50 m: at most 0.67 % at 1 mm).
3. **Orient by flood fill.** Winding is made consistent across edges with two faces; each connected
   patch is then turned to face out if it is closed (positive volume) or up if it is open
   (area-weighted normal). Edges with three or more faces stop the fill and are listed with where
   they are; a patch that cannot be wound consistently (one-sided) is left as read and counted.
4. **Crossings are found, never fixed.** Two triangles cross when their insides pass through each
   other, or fold back onto each other, by more than the tolerance; touching at a corner, along an edge
   or within the tolerance is not crossing. The result (the number of crossing pairs and where the
   first 100 are) is saved with the surface or solid, persisted in the project, and sets
   `NonSelfIntersecting`. Properties lists the sites, marks them in the viewport and flies to one on a
   click. The object model gains `Crossings` (with how it was checked: a search at a tolerance, or a
   lattice) on `Surface` and `Solid`.
5. **What a crossing blocks.** A surface with crossings imports, to be looked at, draped on and
   snapped to; `solid.cut_by_surface` refuses it, and `solid.from_mesh` refuses a solid that crosses
   itself. A cut whose inputs were never checked (saved before this, or made by an earlier cut) checks
   them first. Crossings are fixed at source, as seam model issues are.

## How crossings are found

Every pair of triangles whose boxes, grown by the tolerance, overlap is a candidate: a parry3d-f64
BVH query per triangle, split across all cores, each pair once. Two triangles in different planes
cross when each has corners more than the tolerance either side of the other's plane and the pieces
of the two on the line where the planes meet overlap by more than the tolerance (the interval test).
Two in one plane (within the tolerance) cross when one, clipped to the other shrunk by the
tolerance, leaves some area; two on one edge cross when they fold back onto each other. The site is
the middle of the piece the two share.

The first version tested an edge of one triangle passing strictly inside the other. Two boxes run
into each other on a 5 m grid showed why that is not enough: every point where an edge meets a face
lay exactly on another triangle's edge, so nothing was found. Grid-aligned coordinates do that often,
and the interval test is indifferent to it.

## Evidence

- A sheet of four 10 m squares written as loose 3DFACEs (one square 0.4 mm off, a triangle wound
  backwards, one written twice 0.3 mm off, a sliver 0.2 mm wide) imports as one sheet: 5 vertices
  welded (largest move 0.4 mm), 1 triangle collapsed and 1 duplicate dropped, 2 unused vertices
  dropped, 1 triangle flipped, 8 triangles facing up, no crossings; the check survives save and open.
  At 0.1 mm the offset square stays apart.
- Merges do not chain: points 0.6 mm apart weld in pairs, not into one.
- A scrambled box comes out facing out at its exact volume; a sheet facing down is turned up; three
  fins on one edge are listed at the edge's midpoint; a Möbius strip is left as read.
- A triangle through another is one crossing, its site the middle of the shared piece; a 0.5 mm
  crossing counts at 0.1 mm and not at 1 mm; corners resting on a face and fins standing on an edge do
  not; a fold and a coplanar overlap do; flat and rolling grids and a box have none; two boxes run
  into each other on a 5 m grid are found, every site on a face of each.
- Through the registry: a crossing design surface imports flagged, and a cut by it is refused; a
  solid of two boxes run together is refused; a grid is recorded as a lattice; a cut of a surface and
  solid saved without checks checks both first.

Timings on a 2,000,000-triangle rolling grid (release build, this machine's 48 threads): repair
239 ms, crossing search 222 ms, 2,000,000 triangles out (641 ms and 616 ms before the speed-ups in
decision 0011, which change no results). On a real surface of 18.5 million triangles: repair 3.9 s,
crossing search 2.3 s.
