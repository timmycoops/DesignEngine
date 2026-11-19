---
paths:
  - "crates/zd-geom/src/{wall,road,ramp,face,rule,drape,strips}.rs"
  - "crates/zd-commands/src/builtin/{face,ramp,build,strips}.rs"
  - "crates/zd-core/src/face.rs"
  - "app/src/panels/{FaceDesignPanel,BuilderPanel}.tsx"
---

# Faces, rules and ramps

Moved from CLAUDE.md's object model. Read `docs/decisions/0002-industry-research.md` before designing a command here.
Decisions: 0023 (wall step), 0024 (face design), 0027 (rules), 0028 to 0035, 0037, 0038, 0041 and 0042 (ramps),
0045 to 0049 (step design, builder, strips).

- `Rule`: a function from a string to one value per edge. Types: fixed, fixed list, inside polygon,
  inside solid, segment azimuth, by horizon. Later: block model, geotech solid with transition distance.
  Blending between rules may only widen berms and flatten slopes: where a value changes along a straight run, a
  transition bevels the crest across the steeper or narrower side (`docs/decisions/0027-rules.md`).
- `FaceProfile`: one table for one type of string, going one way from its toe, up or down: the grid, surface or RL
  the toe registers to, then ordered steps, each "project to surface B or an RL at a batter" or "berm of a width", with the
  batter or width a number or a rule, and each row saying how its line registers and conditions. A string going both
  ways takes a profile for each, and the top or bottom string is made by hand after the run. Still to come: berm on
  roof and berm on floor per horizon, minimum and maximum bench height, wall type (face, side, end). Side (left or
  right for open highwall strings) is on the line. A face is built oversized, past the pit shell, and the cut does
  the fitting (`docs/decisions/0023-wall-step.md`). A face lies on the grids it's built to: a berm on a grid is split
  along the grid's triangles, not a chord across its cells, and the seams are stacked from the same grids
  (`docs/decisions/0064-berms-on-their-grid-and-flaps.md`). `FaceDesign` is the saved object the Face design panel edits:
  lines, profiles and any shared rules together (`docs/decisions/0024-face-design.md`). A face built by hand is a step
  design: a face design on one string that saves and runs whole as each step is added (`face.step`, one undo a step),
  with ramps placed by clicking. Faces are built from strings, never by moving a surface's points
  (`docs/decisions/0045-step-design.md`). A line a row ends on can be redrawn by hand, kept on the design's line and not
  in the profile: the row re-forms to meet it and the rows above carry on from it, roads and all
  (`docs/decisions/0046-edit-the-line.md`). Join on a berm makes the switchbacks turning on it, on box pads facing each other,
  one pad, the line running straight from the back of one box to the back of the next; left to itself a berm joins only
  boxes within a road's width of each other or overlapping (`docs/decisions/0047-join-switchbacks.md`). The manual
  builder, replacing the step design's hand edits, builds one wall or berm from the line you pick, run once
  (`face.build_step`): its outputs are ordinary strings, roads carry on from the line, turns and Join go on the berm, and a
  wall from a berm's line a step made builds that berm with it from its crest, standing on your line if you've edited it.
  Each line keeps how it was built, and Rebuild from here (`face.rebuild`) builds the steps above an edited line again
  from it, each with its settings, as next revisions of what they made, in one undo. The Builder panel builds the next
  step from the picked line, or rebuilds when steps are built on it, and lists the build's steps, a click on one going
  to its line (`docs/decisions/0048-build-a-step.md`). A face is told apart by its strip number: `face.join` makes one
  face a strip from up and down faces, several runs or a build's steps, welded, stitched and gaps filled, every change
  said; `solid.slice_by_faces` closes each face behind it past the solid, its triangles facing the pit, and slices the
  solid from the pit out, each strip cut from what the strips before it left so they add up to the solid; a face that
  doesn't pass right through the solid (past both ends, above its top, below its floor; one stopping on them doesn't) is
  refused, every shortfall listed, and the Cut solids panel checks before the cut; carrying faces on past the solid is
  the designer's, not automated (`docs/decisions/0049-strip-solids.md`).
- `Ramp`: centreline or start point, direction, start and end bench, width, grade (%, ratio or degrees,
  measured on the inside edge by default, which is the pit side), inside and outside taper, entry type, flat or
  graded switchbacks with radius. It's a named ramp in a `FaceProfile`, set by a string drawn across the strips: each line
  the string crosses gets a road, starting where it crosses the toe of its first wall, or with its end held, coming out
  on its last crest level with that place along the strip, with the run moving the start along the toe to get there
  (`docs/decisions/0031-ramps-on-real-strips.md`). Wall rows name the ramps
  that climb them, and it's cut back as they're projected, so running the design again rebuilds it. A wall can hold several
  roads running the same way, lowest first, each cut into the wall above the one below (`docs/decisions/0032-roads-on-one-wall.md`),
  and roads running opposite ways side by side, as a straight ramp beside a switchback, each cut its own way in its own part of
  the wall, never within a road's width of each other (`docs/decisions/0042-roads-both-ways.md`).
  The toe behind its start moves back by the road for an entrance. Each ramp goes past a berm after it, by default, crossing
  the full berm level to climb on from the next toe, or through it at its grade, the berm narrowing to nothing at the road
  (`docs/decisions/0035-ramps-after-the-berm.md`). After the berm it crosses straight across, in line with its top, as
  Deswik's does; a road wider than the berm starts its next stretch on the top of the last. A wall row can turn a ramp
  back before it: a switchback on a pad of the two roads and the berm between, level on an RL and on the seam floor on a
  grid (no coal left under it), on straight or curved walls. The road coming up ends at its top and turns on a flat box past it,
  in line with it and on the floor on a grid (its width long unless the ramp says otherwise): the road, the berm and the next
  road wide, the next road climbing the other way from its near end, straight across the berm from the top, and the wall above
  going round the box, its far end one straight wall square across, the berm standing out into the wall from the crest
  beside and past the box, never round its end; the ramp's end angle (90° by default) flares that wall out past the box
  from its back corner to the berm's line past it. With no box the toe past the turn joins back a length on set on the ramp
  (50 m by default), whatever vertices the string has; one longer than the line runs to the string's end
  (`docs/decisions/0038-switchbacks.md`). On a profile going down a road comes down from the top line, notching it
  for its entrance, and the walls below it move out towards the pit (`docs/decisions/0037-roads-going-down.md`). A crest
  records each road that came out on it as `road_top` attributes, and a ramp set to carry on from the line takes it on
  from there in a design run from that crest, with no entrance; a crest a run made keeps its heights as a line, so two
  designs give one design's face (`docs/decisions/0041-roads-carry-on.md`). It reports
  the bench where the grade can't be met instead of drifting (`docs/decisions/0028-ramps.md`, `0030-ramp-strings.md`).
  By hand, a ramp is placed from a start as its polygon (`string.ramp_polygon`, Ramp on the Walls row): its inside edge
  along a line picked or straight the way clicked, at a width and grade, to an RL, a grid or a surface, which says
  whether it climbs or falls; the polygon and nothing else, and placed again it has moved
  (`docs/decisions/0079-project-ramps-by-hand-and-measure.md`).
