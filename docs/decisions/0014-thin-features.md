# 0014 — Thin features in solids

Date: 2026-10-30 · Status: accepted (Tim, 2026-10-30); lips and flat folds are thin too, by decision 0017 (2026-10-30)

## Why

With close-and-repair slices A and B (decision 0013), 247 of the golden pit's 638 refused relimiting
solids imported. The relimiting solids come from Spry's topo relimiting, which builds each solid
again by its own booleans. Looking at the 391 still refused:

- **About 290 of the 314 refused for crossings had no crossing at all** when searched at 1e-9 m.
  Where the topography meets a bench or seam surface at a grazing angle, the solid tapers to a knife
  edge thinner than the 1 mm tolerance; its top and bottom faces there met decision 0010's fold and
  coplanar-overlap rules. On a surface that is a real defect (a sheet folded back on itself); on a
  closed solid it is sound geometry. Along 160 x 160 vertical rays, every one of these solids counts
  exactly one layer of solid, to 6e-14 m.
- **The union cut only crossings deeper than the tolerance.** Where a real crossing tapered below
  1 mm, the line it follows stopped short, the regions either side ran together, and the union could
  not close (125 solids) or judged a region wrongly (61 failed the ray check).

## Decision

1. **Solid rules.** On a closed solid, two faces within the tolerance of one plane that face opposite
   ways and lie apart (more than `COINCIDENT`, 1e-9 m) are **thin**: a knife edge, or a hairline gap
   between two parts of the solid. They are counted with where the first 100 are, reported in the
   run record and in Properties (a "Thin" section, marked in the viewport) and never refuse a solid.
   Faces that coincide, or overlap facing the same way, still cross; a pass-through is unchanged (by
   more than the tolerance).
2. **Where.** Every check of a solid uses them: `solid.import` with and without repair,
   `solid.from_mesh`, and a cut's solid input. A solid saved with a check by the surface rules (before
   this decision) is searched again when it is cut. Surfaces keep the surface rules: a topography
   that folds back on itself is a defect worth flagging.
3. **The object model.** `CrossingCheck` gains `SolidSearch { tolerance }`; `Crossings` gains `thin`
   and `thin_at`, read as none from projects saved before.
4. **The union cuts along every crossing**, however shallow (at `COINCIDENT`), so the lines it follows
   close into loops; thin features are not crossings at that tolerance and are left as they are. If
   that fails, the cut at the tolerance (slice B as first built) is tried, so nothing that imported
   before is refused now. Either result must leave no crossing by the solid rules and pass the ray
   check, and the run record says which cut was used.

Found while building it, on the pit, and fixed the same way:

5. **The piece two triangles share is measured exactly.** The interval test took a corner within the
   tolerance of the other's plane as lying in it, which stretched the shared piece past where the
   triangles meet: 21 pairs were triangles touching near their corners, called crossings. A corner
   now counts as in the plane only when it coincides with it; the tolerance still applies to how far
   each passes through the other's plane and how long the shared piece is. This holds for surfaces
   too.
6. **The union needs a closed surface, facing out.** What an open surface encloses is not defined: one
   solid still open after closing read as -160.554 m³ and came out of the union at 1.771 m³, its open
   sheets dropped and missed by the rays. The union refuses a surface with open edges, saying where.
   It also refuses one with parts wound inside out (winding -1 along the rays, more than 0.01 % of the
   volume), which it would drop rather than resolve.
7. **A closed mesh tangled with edges of three or more faces**, which do not come apart as closed
   pieces touching, goes through the union too; if the union cannot rebuild it, it is refused as not
   closed. One solid that slice B rebuilt by chance (its folds were crossings then) imports this way.

## Evidence

- A wedge rising from a knife edge to 0.5 mm: a fold by the surface rules, thin (no crossing) by the
  solid rules, and nothing either way at 0.1 mm. Two cubes 0.5 mm apart: thin. A cube sitting offset on
  another (faces coinciding) and two cubes with their tops in one plane facing the same way still
  cross; so does a pass-through.
- Two triangles touching near their corners, each through the other's plane by more than 1 mm: no
  crossing at 1 mm, 0.1 mm or 1e-9 m (the old test called it one at 1 mm).
- A box whose bottom dips into a cube's top by 0.5 mm along one side and 5 mm along the other: cut at
  1 mm the union cannot close; cut along every crossing it is exact, 1000 + 600.165 - 0.0825 m³ to
  1e-6 m³. Two overlapping cubes beside a knife-edge lens: 1,875.025 m³, the lens kept with its thin
  pairs. An open surface is refused with where it is open; a cube crossed by one wound inside out is
  refused with the 875 m³ inside out (measured within 1 %).
- Through the registry: a knife-edge lens (0.025 m³) imports with and without repair, its thin pairs
  in the run record and saved as `SolidSearch`; as a surface the same faces are flagged. A wedge saved
  with a surface-rules check is searched again when cut and cut at RL 0.25 mm into 0.01875 and 0.00625
  m³ (Manifold takes the knife edge). The overlapping-cubes import says it cut along every crossing.
- The golden pit's 42,223 relimiting solids: **41,882 import as read** (was 41,585), 276 with thin
  pairs, in 161 s (159 s before). Of the 638 refused before, **529 now import** (297 without repair,
  232 with it; was 247); the 34 with crossings resolved changed by at most 0.0055 % of their volume,
  33 cut along every crossing and 1 at the tolerance. Nothing that imported before is refused. Still
  refused, 109: 86 with open edges or edges of three or more faces, 16 with holes over 10 % of the
  surface, 4 that cross themselves on a surface still open after closing, 2 still crossing after the
  cut, 1 partly inside out.
