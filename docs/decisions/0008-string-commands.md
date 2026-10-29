# 0008 — String commands

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29)

## Decision

Workstream 6, slice 1 covers the Phase 1 gate's toe string: digitise in plan view snapped to the
topography, edit vertices, set a grade, offset.

1. **Offsets use Manifold's 2D booleans** (Clipper2 underneath), already in the stack, so no new
   dependency. The weighted straight skeleton stays where it is locked: wall projection in Phase 3.
2. **Offset makes a new string**, because commands never modify their inputs. Grade, drape, set
   elevation, reverse and close write a new revision of the same string, as vertex edits do.
3. **Offset elevations:** each new vertex takes the elevation of the nearest point on the source
   string, so a graded toe stays graded. A fixed RL can be given instead.
4. **Grade** is in %, positive uphill in the direction graded, measured along plan length.
5. **Plan view** is a true top-down orthographic view (north up, pans with the left button), not the
   3D camera tipped over.

## How offsets work

Each segment sweeps a band of the offset width on the chosen side. Each corner that turns away from
that side adds a mitre or a round piece; a mitre that would reach farther than the limit (4 times the
distance by default) is bevelled. The union of the bands and pieces is taken with Clipper2, and the
part of its boundary lying at the offset distance from the string is the new string. Vertices left in
the middle of straight runs are dropped.

This differs from the method first described (closing an open string into a region on the offset side
beyond its bounding box). That breaks on strings that curl back on themselves, where the closing edges
cross the string. The band union does not, and it serves open and closed strings alike. Tight inside
corners and insides narrower than twice the distance are resolved by the union, never by trimming
crossed lines afterwards. Open strings end square to their end segments. An offset that pinches off
comes back as several strings; one that leaves nothing is refused. Coordinates are worked about a
whole-metre origin and Clipper2 keeps 8 decimals, so offsets are exact to 1e-8 m.

## Commands

| Command | What it does |
|---|---|
| `string.set_grade` | elevations from a vertex at a grade, to another vertex (the end by default), from its RL or a given one |
| `string.offset` | a new string a horizontal distance left or right, mitred or round corners |
| `string.drape` | vertices onto a surface or elevation grid; optionally a vertex at every triangle-edge crossing, at the edge's exact elevation; vertices off the surface keep their RL and are counted |
| `string.set_elevation` | all vertices, or a range, to one RL |
| `string.reverse` | the string's direction reversed |
| `string.set_closed` | the string closed or opened |

In the app, drawing and dragging snap to a string vertex (its exact coordinates), then the nearest
point on a string segment (the closest point on the 3D segment to the pick ray), then the surface. An
Alt-click insert lands on the segment itself. Properties has the tools for the selected string, and
the vertex table shows the grade to the next vertex.

## Evidence

- A 100 m square offset out 10 m: 14,400 m² mitred, and 10,000 + 4,000 + pi x 100 m² rounded (to
  1 m² with 64 chords per circle); in, 6,400 m². A clockwise square swaps the sides.
- An L offset to its inside and outside gives the expected vertices to the micrometre; rounded
  corners keep every vertex 10 m from the string.
- A 170-degree corner is bevelled at the default limit and reaches 1 / sin(5 degrees) times the
  distance when allowed.
- A U 15 m wide has no inside offset at 10 m; 50 m wide, the inside offset is exact.
- A 10 % grade along a toe, then a 10 m offset: the new string's vertices take RL 100, 109 and 120 from
  the points beside them.
- A string draped on a plane lies on it at every vertex and crossing to 1e-9 m.
- Through the registry: grade, offset, drape, set elevation, reverse and close; save and reopen bit
  for bit; undo back to the empty project.

## Not yet (slice 2)

Split, join, smooth and filter; snapping to intersections; typed coordinates (E N RL) and bearing and
distance while drawing.
