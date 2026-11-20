---
paths:
  - "crates/zd-geom/src/{string,string_cut,vertices,transform,crossings,polygons,polygon_ops,fillet,project,sheet,from_surfaces,compare_surfaces,merge_surfaces}.rs"
  - "crates/zd-commands/src/builtin/{string,object,project,sheet,surface,from_surfaces,surface_compare,surface_merge}.rs"
  - "app/src/panels/{StringsPanel,MovePanel,FromSurfacesPanel,SurfacesPanel}.tsx"
  - "app/src/panels/{stringSide,moveWords,strips}.ts"
---

# Strings and surfaces between them

Moved from CLAUDE.md's object model. Decisions: 0029 (strip numbers), 0078 (strings as one run), 0079 (project,
ramps by hand, measure), 0083 (cut strings by strings).

- `Polyline` (string): ordered `[f64; 3]` vertices, closed flag, attributes. A strip's number is its `strip` attribute, set
  by a line drawn across the strings, and what a run makes keeps it (`docs/decisions/0029-strip-numbers.md`). The string
  commands that offset or edit a string take one (`string`) or several as one run (`strings`): one undo, one record, a
  string that can't be done refusing the run with each listed, one that comes out as it was left and said; Join takes
  the open pieces picked and joins them end to end into the first, nearest ends first, ends within a tolerance as one
  vertex, a ring closed, each gap bridged and said; Trim cuts the strings picked where another string crosses them,
  keeping the side clicked or splitting there, Extend runs them on to one at their own grade, and Split splits at a
  point clicked, each cut at the string's own height (`docs/decisions/0083-cut-strings-by-strings.md`); Alt and a box
  picks the vertices of the strings picked, a drag on one moving them all and Delete deleting them, and Set elevation,
  Set grade and Drape then act on them alone (`vertices`), Densify adds a vertex every so many metres and Vertices at
  crossings one wherever strings cross (0083, workstream 4); and
  each tool but Ramp from a start adds a workflow step from the panel, its strings found by rule, a layer's by the layer
  (`docs/decisions/0078-strings-as-one-run.md`). A string
  is projected at a batter by `string.project`: up or down, on the side the new string comes out on, to an RL, a height
  above or below it, a grid or a surface, every string picked in one run, each as it's drawn and left as it is, with
  the wall kept as a surface if asked; to an RL or a surface it's a face's one wall, by a height its plan offset lifted
  (`docs/decisions/0079-project-ramps-by-hand-and-measure.md`).
- `Surface`: triangulated mesh (vertices f64, triangles u32), name, layer. One is made between strings by
  `surface.between_strings`: a road between its two edges, or one sheet across several put in order across (a fill's
  toe, the road's edges, a cut's crest), every corner a string's vertex, so a ramp can be made by hand from polylines:
  Set grade, Offset, Project and a surface between, all on the Walls row (0079).
- Strings from surfaces, grids and solids (0083, workstream 5), in a panel of their own from the Strings row: where two
  surfaces or grids meet, at their own RLs; contours every so many metres, each exactly at its RL with an `rl`
  attribute; a surface's edge with each hole its own string, or a solid's outline in plan at its lowest RL; and what a
  section cuts. Lines are exact, as sections are, and whole: open or closed as they run.
- Polygons in plan (0083, workstream 6), on the Strings row: Clip keeps each string's parts inside a closed string, or
  outside, cut at its own heights as Trim does; Union, overlap or difference makes new closed strings of closed strings
  in plan, on Manifold's CrossSection about a local origin, each vertex that was there coming back exactly with its
  height and each new one taking the height along the first set's edge.
- Surfaces against surfaces (Surface tools plan, 0093 on), in the Surfaces panel, a panel of its own: Compare gives cut
  (the second below the first) and fill, exact as the solids between them each way, with a 2.5D column check said
  beside them and a warning past 0.1 %, inside a closed string or wherever both are; a thickness grid only when
  switched on (`role: thickness`: drawn coloured by value, never cut in a section). Merge (0094) keeps the bottom (dug out) or top (laid on) of surfaces, grids and solids, a solid by its
  floor or top, as a grid at a cell size (fast) or an exact surface (every crest and toe, joined where they meet).
  Trim in plan (`object.trim`, 0095) cuts strings, surfaces, solids and grids by a closed string, inside or outside,
  each by its own rule: a string as Clip, a surface exactly along the string at its own heights, a solid vertically and
  closed, a grid's nodes on the side dropped emptied (never filled); anything wholly on the side dropped refuses the
  run, as Clip does. Triangulate (`surface.triangulate`, 0096) makes one new surface through the strings picked on
  spade's constrained Delaunay, every vertex at its own RL and every segment an edge, no point added but where strings
  cross at one height; bounded by the outermost closed strings (one alone is a pad) or the hull peeled back past a
  longest edge (200 m); strings crossing or touching in plan at heights over 1 mm apart are refused, each place listed.
