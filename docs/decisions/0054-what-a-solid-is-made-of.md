# 0054 — Every solid says what it is made of

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: the Solids plan tab's order agreed, then "Yes, start").
Workstream 6 of the Solids plan. After 0018 (one file, one solid).

## Why

A solid can hold several closed pieces (0018) and cavities (0010), and nothing showed it short of a line in an import's
run record. Tim asked that our commands never make solids with small holes in them, and filters small solids by hand:
both need the pieces and cavities to be visible. Nothing is taken off by size (0018 stands).

## Decision

1. **A solid's pieces and cavities are worked out from its mesh when asked**, as its volume is: a piece is a closed
   shell wound outward, a cavity one wound inward. Nothing new is stored, so projects saved before need no change.
   `Solid::pieces()` gives the pieces, the cavities, the smallest and largest piece's volume and the cavities' volume.
2. **A piece's volume is what its own shell encloses**, before any cavity in it is taken away. The shells add up to the
   solid's volume.
3. **Shells are told apart by shared corners.** A solid's pieces that touch along an edge are already taken apart
   (0011), so they count separately; shells that touch only at a corner count as one.
4. **Every run that makes solids says so**, for every command present and to come, in the same place every solid is
   checked (0015): how many of the solids made are in several pieces, the pieces in all, and the smallest piece with the
   solid it is in; and how many hold cavities, with the volume taken out. A run whose solids are each one piece with no
   cavity says nothing.
5. **Properties and Python show them** on a solid: Pieces with the smallest, where there is more than one, and Cavities
   with their volume, where there are any. `Session.object()` gains `pieces`.

## Known answers

- A 10 x 20 x 5 m box, a 2 m cube beside it and a 1 m cube inside it wound inward: three shells of 1,000, 8 and -1 m³,
  adding up to the mesh's 1,007 m³, to 1e-9 m³, at mine grid coordinates.
- A cube with a 2 m cube 20 m off: 2 pieces, the smallest 8 m³. A cube with a 1 m cavity: 1 piece, 1 cavity of 1 m³,
  999 m³ in all. To 1e-9 m³.
- Three solids made in one run, one whole, one in two, one holed: "1 of the 3 solids made are in several pieces, 2
  pieces in all; the smallest piece is 8.000 m3, in 'two'" and "1 of the 3 solids made hold cavities, 1 in all,
  1.000 m3 taken out of their volume". One solid on its own is named. A whole solid says nothing.

## On the pit

The 42,223 relimiting solids, imported with repair: 3,980 are in several pieces, 10,737 pieces in all, the smallest
under 0.001 m³. The import takes 9.3 s, as before.

## What this leaves

- The one cavity the import's repair step counts on the pit doesn't show here as a cavity: its shell shares a corner
  with the shell round it, so the two count as one. Not looked into further.
- No filter by piece size, and no way to take a small piece out of a solid: Phase 4, as 0018 says.
