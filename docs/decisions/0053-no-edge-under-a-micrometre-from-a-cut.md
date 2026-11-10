# 0053 — A cut leaves no edge under a micrometre

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: "Yes, go ahead"). After 0016 (the general cut), 0049 (strip
solids) and 0052 (a solid welded only as far as it needs).

## Why

A review of the cut asked whether the solids it makes are valid in Deswik. 65 parts cut from PIT B relimit solids by the
mined-out topography went to Deswik as DXF: 20 with an edge under a millimetre that the cut made, 25 with such an edge
from their source file, and 20 ordinary ones.

- **Deswik keeps a DXF solid exactly as written.** Its file (`.duf`) holds all 65 with every corner the same to the bit
  and four points to a face, unwelded. So detail under a millimetre is not opened there, and cleaning the cut to a
  millimetre, proposed first, was dropped.
- **Validate Solids passed 64 of the 65**, with no boundaries or trifurcations on any. One of the 20 the cut made had one
  self-intersection (`BAC.EAST.25.10.4LM.Coal ... below`, 20.6 m³).
- **That solid has two edges about a nanometre long** (0.47 nm and 1.05 nm) with four triangles on them up to 2.4 m
  long. The solid it was cut from has none. A triangle metres long on a base a nanometre wide has no facing to speak of.
  Our own check finds no crossing in it at any tolerance from 0.1 mm to 0.1 nm.
- **This is a lead, not proof**: two other parts in the set have an edge of 0.35 nm and Deswik passed them.
- **How common.** Of 3,519 parts cut from 3,000 solids, 53 had an edge under a micrometre and 46 under a nanometre. Of the
  42,223 source solids, 50 have one.
- **Manifold's own cleaning doesn't do it.** Setting its tolerance to a micrometre before the cut, or simplifying each
  part after, left 31 to 42 of the 46, took 42,000 ordinary triangles off (corners in the middle of flat faces), and
  loosened below + above = the solid from 3e-12 to 8e-6 of a solid.

## Decision

1. **Every solid a boolean makes has the two ends of each edge under a micrometre joined into one corner**
   (`join_tiny_edges`, `TINY_EDGE` = 1e-6 m): the parts of `solid.cut_by_surface`, the strips of
   `solid.slice_by_faces` and the solid of `solid.between_surfaces`.
2. **The corner kept is the first of the two in the mesh's order, exactly as it is.** No corner that stays moves. The
   triangles on the edge go, having no width left, and so does a pair left folded flat on each other, which encloses
   nothing.
3. **Only where it leaves the solid sound.** The result must be closed with two faces on every edge, wound as it was,
   and its volume within what moving corners a micrometre can change (surface area times a micrometre). The edges are
   joined all at once; if that isn't sound they are tried one after another, each kept only if the solid is still sound,
   and the rest are left and counted. A solid with no such edge comes back untouched, to the bit and in its order.
4. **It is said.** The run record gives the corners joined, in how many solids, the triangles dropped, and any edges
   left.
5. **A micrometre, not a millimetre.** The finest detail in real solids is hundredths of a millimetre (0052), and the
   cut's own balance of volume is kept only because nothing bigger is touched.
6. **Imports are not touched.** A solid read from a file keeps its detail, edges under a micrometre included (0052).

## Known answers

- A 10 m cube whose top has a corner written twice, 0.5 nm apart: 1 corner joined, 2 triangles dropped, every corner
  kept exactly as it was, 1,000 m³ to 1e-9 m³. A plain solid comes back as it was.
- A cube beside a closed wedge 0.5 nm high (3e-9 m³): the wedge folds flat and goes whole, 4 triangles, leaving the cube
  as it was. The wedge alone would leave nothing: it comes back as it was, 1 edge left.

## On the pit

Release build, 48 threads.

| | Before | After |
|---|---|---|
| Sample: 3,000 solids into 3,519 parts, parts with an edge under a micrometre | 53 | 0 |
| Sample: below + above against the solid, at worst | 3.0e-12 of it | 3.0e-12 of it |
| Sample: total volume of the parts | 14,571,807.995176 m³ | 14,571,807.995176 m³ |
| Whole pit: 42,223 solids into 49,776 parts | 42.1 s | 42.2 s |
| Whole pit: below + above against the solid, at worst | 0.266 m³ | 0.266 m³ |

On the whole pit 2,305 corners are joined and 4,696 triangles dropped. 25 edges are left, on 8 parts, and every one of
those parts was cut from a source solid that has such edges itself: none the cut made is left. The same one solid is
left uncut as before.

## In Deswik

The two source solids behind the three parts with a nanometre edge, cut again with this in, four parts
(`bench/out/deswik-check/cut-parts-2`): Tim ran Validate Solids on them on 2026-11-10 and all four are valid, the one
Deswik called bad among them (20.602 m³ as before, 380 triangles, shortest edge 0.08 mm). One solid is thin evidence,
but it failed with the edges and passes without them.

## What this leaves

- **A triangle a nanometre high with no short edge** (a corner all but on the edge opposite) isn't joined by this.
- **No test drives a real cut into making such an edge**: the known answers are on meshes written by hand, and the pit
  is the evidence that the cut's own are caught.
