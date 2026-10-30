# 0017 — Lips and flat folds in solids

Date: 2026-10-30 · Status: accepted (Tim, 2026-10-30)

## Why

Decision 0014 left two kinds of pair crossing in a solid: faces that coincide, and faces overlapping the
same way. On the golden pit's 332,000 m³ waste solid, the self-union was exact (closed, manifold, volume
unchanged, ray check passed), yet the check found three pairs in the result, all in Spry's own geometry:

- **A lip.** Two tops within 0.9 mm of one plane, overlapping and facing the same way, with the lip's
  underside between them, pointing the other way. Going through all three runs solid, gap, solid, gap,
  which is sound. Two faces facing the same way with nothing between them would mean the solid is
  there twice; the rule did not tell the two apart.
- **A flat fold.** Two faces on one edge, folded exactly flat on each other, facing opposite ways: a knife
  edge of no thickness, enclosing nothing. Decision 0014's coincidence test was written for faces that
  are not joined, such as a block sitting on another.

## Decision

Both are solid rules; surfaces keep the surface rules, where a fold is a defect worth flagging.

1. **Two faces overlapping the same way are thin when a face between them points the other way.** A short
   line from one to the other, through a point in the overlap, crosses more faces pointing against it
   than with it. With nothing between them they still cross.
2. **Two faces folded on a shared edge, facing opposite ways, are thin even when they coincide.** Faces that
   coincide without sharing an edge still cross.
3. Thin pairs are counted and shown as before, and never refuse a solid. The word now covers knife edges,
   hairline gaps and thin lips.

## Evidence

- A prism with a 0.8 mm lip over a gap: closed, crossing by the surface rules, no crossing by the solid
  rules, with its pairs thin. Two blocks overlapping with their tops 0.5 mm apart and nothing between
  them still cross; so do a block sitting on another and two blocks with their tops in one plane.
- Two faces folded flat on an edge: one crossing by the surface rules; thin, no crossing, by the solid
  rules.
- The golden pit's 42,223 relimiting solids, file by file against decision 0015's build:
  - **41,917 import as read** (was 41,891) and 301 with repair; **5 are refused** (was 6): the three coal
    solids with interior walls and the two files that are only a sheet. The pass takes 143 s.
  - **The 332,000 m³ waste solid imports**, volume unchanged, through the self-union and its ray check.
  - Nothing that imported before is refused.
  - 34 solids no longer go through the union, leaving 7 that do, the 332,000 m³ solid among them. Each of
    the 34 keeps the volume the union gave it, to the 0.001 m³ the run reports, which is the evidence
    these were false alarms: a real overlap would have lost volume in the union.
  - 26 of the 34 now import as read, untouched. **14 of the 34 come in as several pieces instead of one**:
    108 pieces, the same total volume, because their touching edges are taken apart where the union
    used to rebuild them. 65 of those pieces are under 0.001 m³.
- The whole-pit cut again, now 44,270 solids, by the mined-out topography: 51,875 parts in 112 s (2.5 ms
  a solid), every part checked. No solid needs the whole-surface retry now (25 did under decision 0016's
  check). Two are left uncut and named (one before; not the same one). Each solid and its parts agree to 0.27 m³ at most (3.2e-5 of 8,407 m³); on the near-zero pieces
  the difference can be a large share of a small number, 0.019 m³ of a 0.059 m³ piece at worst.
