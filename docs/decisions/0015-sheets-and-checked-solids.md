# 0015 — Hanging sheets, touching edges, and every solid checked

Date: 2026-10-30 · Status: accepted (Tim, 2026-10-30)

## Why

After decision 0014, 109 of the golden pit's 42,223 relimiting solids were still refused, even with
repair. Looked at one by one:

- **97 were open, and every one carried loose upward-facing sheets** (2 to 1,400 m² each), lying wholly
  outside a closed body: scraps of the cutting surface left by Spry's topo relimit. 95 of them hung off
  the body along an edge, or sat beside it touching nothing; 2 were nothing but a sheet. The sheets'
  open edges are what read as "open edges" and "holes over 10 %". Taking the sheets off by hand, 46
  bodies were valid solids as they stood, with no change in volume.
- **10 closed solids were refused at an edge of four faces** that the repair should take apart: the faces
  round it alternate like two parts touching. Each was one piece touching itself, with a single fan of
  faces round each end of the edge, so copying the ends (decision 0011) could not separate it.
- **2 large waste solids still crossed themselves after the cut** (430,000 m³, 93 % of the refused
  volume).
- Some solids are written twice by Spry with the same geometry, once as coal and once as waste.

Deswik's Close and Repair Solids has the same tools for the same defects ("remove hanging sheets",
"remove flaps", "remove interior facets"). Tim also asked that our own commands never make a solid
like these.

## Decision

1. **Hanging sheets come off whole**, before fins, in "Import and repair". A hanging sheet is a patch
   (triangles joined across edges of two faces) with an open edge that hangs off the rest along edges of
   three or more faces, one face of its own on each, the rest keeping two or more faces there. A sheet
   encloses nothing, so no volume changes; each is logged with its area and where. Peeling a sheet a
   fin at a time (decision 0013) left its far side loose, and its loop was then filled as a hole.
2. **Loose sheets come off too**: a patch with open edges that touches nothing, when it is a sheet
   rather than a shell with a hole. Its area vector (the sum of its triangles' area times normal, zero
   for a closed shell) must be at least half its area: a box missing a face scores 0.2, a scrap of
   topography about 1. Something must be left; a file that is only a sheet is still refused.
3. **One piece touching itself along an edge** is taken apart by splitting one side's two faces at a
   vertex added at the edge's midpoint. The vertex lies on the edge, so no coordinate moves and no
   volume changes; the solid stays one piece, which touches itself there.
4. **Every solid a command makes is checked before anything is saved**, in the session, for every
   command present and to come (cuts now; stacking and blocking later). A solid is already closed,
   manifold and consistently wound, or it cannot be made; one not yet searched by the solid rules (a
   cut's parts) is searched now and keeps the result. One that would cross itself stops the run, with
   where, and nothing is saved.

## Still refused

- **The 332,000 m³ waste solid.** Its self-union is exact (closed, manifold, volume unchanged, ray check
  passed), but the crossing check still finds three pairs in it: a 0.9 mm overhanging lip, where two
  faces facing the same way lie within 1 mm with the lip's underside between them (a false positive of
  decision 0014's rule for faces facing the same way), and one fold where two faces lie flat on each
  other. Both are in Spry's file. It stays refused until the rule tells a lip from an overlap.
- **Three small coal solids with interior walls**: closed parts with flat faces between them. The
  union does not yet close them.
- **Two files that are only a sheet**: refused, correctly.

## Evidence

- Synthetic, each with its volume exact to 1e-9 m³:
  - a sheet of three triangles hanging off a cube's edge comes off whole, nothing filled, 1,000 m³;
  - a flap round a corner is one sheet; two sheets on one edge both come off;
  - a loose sheet beside a cube comes off, and alone it is left for the caller to refuse;
  - a box missing its top beside a cube is a shell with a hole: it stays and is filled (2,000 m³);
  - two cubes touching along an edge, one with a triangle missing: held, not a sheet, filled;
  - a slab with two pockets meeting along an edge (one piece touching itself): split at the edge's
    midpoint, one piece, 22,000 m³, no vertex moved;
  - two overlapping cubes made as one solid are refused before saving; a cut's parts are saved with
    their check.
- The golden pit: see below.
