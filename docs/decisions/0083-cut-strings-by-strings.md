# 0083 — Cutting strings by strings: Trim, Extend, Split at a point, Join finished

Status: agreed 17 November 2026 (Tim: "Agree all nine", the String tools plan tab of the Build Plan). This record is
that plan's workstream 1; the later workstreams add to it as they merge: workstream 2, round corners, and workstream 3,
move, copy, rotate and mirror, and workstream 4, vertices, below.

## Why

Tim asked to add string and point manipulation to the engine. The string tools changed a string as a whole; nothing
cut one string by another, which a planner does all day in Deswik: trim the toes back to a pit limit, run a line on
to another, split a string where it's clicked. Join had three limits left from 0078: no tolerance, a ring of pieces
left open, and the pieces' attributes dropped.

## Decision

1. **`string.trim`**: every string picked (`string`, or `strings` as one run, 0078) is cut wherever another string, open
   or closed, crosses it in plan. With `keep` side, the parts on the side of the cutting string that `side` (a point) is
   on are kept: left or right of an open one, inside or outside a closed one, the one side for every string. With
   `keep` both, every part is kept, split at the crossings. The first part kept is the string's next revision; any
   others are new strings on its layer, named `<name> part 2` and on, carrying its attributes. Parts kept either side of
   a place the cutting string only touches stay one. A crossing within a nanometre of a vertex is at the vertex, which
   is kept exactly.
2. **Heights are the string's own** (plan decision 2): a cut takes the RL along the segment it falls on. The cutting
   string's heights are never used.
3. **`string.extend`**: every string picked runs on from its nearer end (the one that meets the other string sooner)
   along its last segment, at that segment's grade, no further than `reach` (100 m unless set). The end vertex moves
   there, so the line is the same line, longer, with no vertex added.
4. **`string.split`** takes `point` in place of `at`: the place on the string nearest the point in plan, which must be
   within a metre of it. A vertex goes in there at the string's height, unless the place is within a millimetre of a
   vertex, which it splits at.
5. **Join, finished**: `tolerance` (1 cm unless set): ends within it in 3D share one vertex, the piece before's as the
   string runs, and aren't warned of. Pieces that make a ring, the joined string's own ends within the tolerance, close.
   An attribute the kept string hasn't got is taken from the other pieces where they agree; where they differ, or one
   differs from the kept string's own, the kept string's stands and it's said (`string.attributes_differ`).
6. **What can't be done** follows 0078: a string lying wholly on the side dropped, a point to keep that lies on the
   cutting string, or an end meeting nothing within the reach refuses the run, each string listed, nothing changed. A
   string the cutting string doesn't cross, or one already ending on the string to run on to, is left as it was and
   said; on its own, it's refused, since there's nothing to do.
7. **In the app**: Trim and Extend are on the Strings row after Split. The strings are picked first, then the button,
   then a click on the cutting string (the nearest string to the snapped click, within 2 m, other than those picked),
   and for Trim a click on the side to keep. Esc while it waits lets it go. Split splits at a click by default, or at a
   vertex by number. Join has Ends within, in millimetres. Each adds a workflow step from the panel, its clicks taken
   when it's added (Split at a click excepted).
8. **The ribbon's fold** measures More once it's drawn: a row that folds (Strings now does at 1,280 px) folds again if
   More is wider than allowed for, so nothing before it is clipped.

## Where

`crates/zd-geom/src/string_cut.rs` (`trim_string`, `extend_string`, `split_string_at`), `join_pieces` in
`crates/zd-geom/src/string.rs`, the commands in `crates/zd-commands/src/builtin/string.rs`, `app/src/panels/StringsPanel.tsx`,
`nearestString` in `app/src/panels/stringSide.ts`, `app/src/tools.ts`, `app/src/workspace/Ribbon.tsx`.

## Checked

Known answers, at mine grid coordinates (`crates/zd-geom/src/string_cut.rs`, `crates/zd-commands/tests/strings_cut.rs`):

| Case | Known answer | Tolerance |
|---|---|---|
| A 100 m line at 10 % from RL 100 trimmed at a line across 60 m along, west kept | Ends 60 m along at RL 106; the vertices kept, exactly | 1e-9 m |
| Fifteen such toes in one run | Each ends there at RL 106; one undo puts all fifteen back | 1e-9 m |
| Keep both | Two strings, 60 m and 40 m, sharing the cut; the new part carries the strip number | 1e-9 m |
| Cut through a vertex | One cut, the vertex kept exactly | Exact |
| A closed limit 20 to 80 m along | Inside: from RL 102 to RL 108; outside: two strings | 1e-9 m |
| A closed 100 m square cut 60 m east, west kept | One open string of 4 vertices, 220 m in plan | 1e-9 m |
| A 50 m line at 5 % from RL 100 run on to a line 80 m along | Ends 80 m along at RL 104, no vertex added | 1e-9 m |
| The same with a 20 m reach | Refused, every string as it was | Exact |
| Split at a point 0.4 m off a 10 % line, 30 m along | A vertex at 30 m, RL 103 | 1e-9 m |
| Split at a point half a millimetre past a vertex | Splits at the vertex, none added | Exact |
| Two pieces 5 mm apart joined at 1 cm, then 2 cm apart | One vertex and no warning; then bridged and warned | Exact |
| Four sides of a square as pieces | One closed string of its four corners | Exact |
| Pieces agreeing on a strip number, then differing | Taken; then left off and said | Exact |

Timings in a release build, made-up data: 15 toes of 40 vertices trimmed by a 60-vertex pit limit in one run in
0.4 ms and run on to a line in 0.1 ms; 150 toes in 3.3 ms and 0.3 ms.

In the app, `zd-app --bench --string-edits` passes 8 of 8: three toes picked by a click and two Ctrl-clicks, trimmed by
the button, a click on the limit and one on the side to keep, each ending at RL 106; one Ctrl+Z puts them back; Esc while
Trim waits lets it go; Extend by one click runs them on to RL 115; Keep both makes a part each; Split at a click puts
a vertex in at the string's height; Join puts the two back. The shell check passes 59 of 61; its two failures (the
menus' words and a panel's first control) fail the same way on main's exe.

## Built: workstream 2, round corners

Decision 3 of the plan as picked: a radius and an angle on every string picked.

- **The geometry** (`crates/zd-geom/src/fillet.rs`, `fillet_string`). Every corner turning more than the angle in plan
  (10° unless given) becomes a circular arc of the radius in plan, tangent to both legs, a vertex every so many metres
  along it (2 m unless given). Where the arc meets each leg it takes the leg's own height there, and the heights are
  graded evenly along the arc between. A leg between two corners being rounded lends each half its length; a leg to an
  open string's end lends all of it; a corner whose legs can't hold the radius takes the biggest they can
  (radius = what they lend ÷ tan(turn ÷ 2)). Corners turning less, a reversal, and an open string's ends stay as they
  are; a closed string's every corner is a corner.
- **The command** (`string.fillet`): `string` or `strings`, `radius`, `angle`, `spacing`; one run and one undo for every
  string picked, a string with no corner past the angle left as it was and said, and each string's corners said with
  any that took a smaller radius ("vertex 2 at 3.00 m").
- **The tool**: Round corners on the Strings row, after Join; its panel takes the radius, the angle and the spacing;
  it adds a workflow step from the panel, as the other Strings tools.
- **More lights.** With Round corners the Strings row is a tool longer, and at the bench's width Reverse now folds into
  More. A tool in use folded into More left nothing lit; More is now lit and says which ("More: Reverse"), carrying the
  tool's key as its button would.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_right_angle_rounded_at_20_m` (zd-geom) | a right-angled corner with 100 m legs at mine coordinates: every arc vertex 20 m from the centre (80 m along each leg, 20 m in); plan length 2 × 80 + 10π = 191.42 m from 200 m; the arc starting at RL 108 on the leg climbing 10 %, ending at RL 110 on the flat, graded up between | 1e-9 m on the circle and heights; 2 cm on the length |
| `a_corner_too_tight_takes_the_biggest_radius_that_fits` | the same corner with 10 m legs asked for 20 m: 10 m, said | 1e-9 m |
| `a_closed_square_and_corners_under_the_angle` | a 100 m square, closed, at 10 m: four corners each 10 m; plan length 4 × (80 + 16 × 2 × 10 × sin(π/64)) = 382.81 m, its arcs' chords at a metre; a 5° corner left at 10°; two vertices as they were | 1e-9 m |
| `round_corners_on_fifteen_toes_in_one_run` (zd-commands) | fifteen toes' 8.5° corners left at 10°, rounded at 5° in one run (each arc starting 50 × tan(4.27°) = 3.73 m back), one undo for all; a 3 m dog-leg asked for 20 m given 3 m and said | 1e-9 m |
| `zd-app --bench --string-edits` | the dog-leg picked by a click, 20 m typed, the button pressed: 19 vertices, every arc vertex 20 m from the centre, the ends kept; one Ctrl+Z puts it back | 1e-6 m |
| `zd-app --bench --shell` | Reverse chosen in the Strings panel lights More as "More: Reverse" | |

Timings (release, made-up toes of 40 vertices, every corner over 1° rounded at 20 m): 15 strings 0.1 ms, 690 vertices
after; 150 strings 0.9 ms, 6,900 vertices after.

## Built: workstream 3, move, copy, rotate, mirror

Decisions 4 and 5 of the plan as picked: strings, surfaces, solids and grids; a copy on its original's layer, named for
it, picked once made.

- **The geometry** (`crates/zd-geom/src/transform.rs`). A move by an easting, northing and RL; a rotation in plan about
  a point, anticlockwise seen from above, exact at quarter turns; a mirror across a line through two points in plan.
  Rotate and mirror leave heights as they are. Points are worked from the point or line they turn about, so mine grid
  coordinates keep their precision. A mirrored mesh has each triangle wound the other way, so one that faced out still
  does. A grid moves by its origin and every live height (a null stays null), and doesn't rotate or mirror: its rows run
  east and north.
- **`object.transform`**: `objects` and `transform` (`{kind: move, by: [E, N, RL]}`, `{kind: rotate, about: [E, N],
  angle}` or `{kind: mirror, from, to}`). The next revision of each object, in one run and one undo; its name, layer and
  attributes stay. A surface's or solid's validity flags are worked out again from the moved mesh, and its crossing
  check is moved with it rather than run again, since a rigid move makes no crossing and takes none away. Refused, with
  nothing changed: a run with a grid rotated or mirrored, or with anything not drawn (each listed, as 0078); a mirror
  line with no length; a move by nothing or a whole number of turns.
- **`object.copy`**: the same `objects` and an optional `transform` (none copies in place). A new object for each, on
  its layer, named `<name> copy`, carrying its attributes; the objects picked are untouched. Attributes are copied as
  they are, so a copied block solid has its original's `block`, `seam` and the rest: rename it or set them before
  reserves run, since a reserves table finds a leaf's solids by those.
- **The tools**: Move and copy, a panel in the tool dock with Move, Copy, Rotate and Mirror, listed under `always` and on
  the Strings and Solids rows (the Strings row folds them into More at the bench's width). Move and Copy go by two clicks
  (a point, then where it goes, both snapped; RLs kept unless Heights is set to move by the clicks) or by distances
  typed. Rotate takes an angle and a click on the point to turn about; Mirror two clicks on the line. Rotate and Mirror
  can make copies instead (Copies). Copies are picked once made. Each adds a workflow step from the panel, its clicks
  taken when it's added.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_quarter_turn_about_a_point_takes_east_to_north_exactly` (zd-geom) | a vertex 100 m east of the point lands 100 m north of it, its RL kept; −270° the same | Exact |
| `a_mirror_across_a_line_north_puts_a_point_as_far_the_other_side` | 20 m east of the line, 20 m west of it; across a 45° line, east becomes north | Exact; 1e-9 m |
| `a_string_moved_by_distances_typed_moves_every_vertex_by_exactly_that` (zd-commands) | 100.5 m east, 200.25 m south and 3 m up, every vertex; name and layer kept; one undo | Exact |
| `fifteen_toes_rotated_a_quarter_turn_about_a_point_in_one_undo` | each toe's end 100 m east of the point lands 100 m north of it; one undo for all fifteen | 1e-9 m |
| `a_cube_mirrored_moved_or_turned_keeps_its_volume_and_faces_out` | a 10 m cube mirrored: 1,000 m³, Closed, Manifold and ConsistentWinding, 40 to 50 m west of the line as it was east; then moved, then turned 30°: 1,000 m³ | 1e-9 relative |
| `a_copy_is_a_new_object_and_the_original_stays_as_it_was` | the original at the same revision; the copy a new object with the same vertices, `Strip 4 copy` on `toes` with `strip` 4; copied 50 m north, the original where it was | Exact |
| `a_grid_moves_but_a_rotation_with_one_in_it_is_refused_whole` | the grid's origin moved, a live node 104 to 107, the null still null; rotated with a string, refused naming the grid, both as they were | Exact |
| `a_mirror_line_with_no_length_and_a_move_by_nothing_are_refused` | refused, the string untouched | Exact |

| `zd-app --bench --string-edits` | Toe 1 moved by two clicks snapped to its vertex and the pit limit's end: 60 m east and 50 m south, RLs kept; Toe 2 by 100.5, −200.25 and 3 typed; Toe 3 a quarter turn about its first vertex clicked, its end as far north as it was east; the dog-leg mirrored across the pit limit with Copies ticked, the copy picked and the dog-leg where it was; each put back by one Ctrl+Z | 1e-6 m |
| `zd-app --bench --shell` | the Panels menu lists Move and copy; the Solids row keeps its Import and Export solids menus, the four tools after them | |

Timings (release, made-up): 150 toes of 41 vertices and 1,000 10 m cubes (12,000 triangles) in one run: moved in 2.8 ms,
turned 30° in 2.2 ms, copied mirrored (1,150 objects made) in 2.4 ms.

In the app, `--string-edits` passes 23 of 23, `--shell` 62 of 62, and `--select`, `--clicks` and `--workflow` pass whole.

## Built: workstream 4, vertices

Decision 6 of the plan as picked: Alt and a box picks the vertices of the strings picked.

- **Picking** (`app/src/viewport/strings.ts`). With strings picked, Alt and a left drag draw a dashed box; let go, the
  vertices of the strings picked that lie in it are picked, or with Ctrl added to those picked. In 3D only what's in
  front of the camera counts; on a section it's off, as Shift and a box is. Alt+click without a drag still inserts a
  vertex. Picked vertices show gold, every one, however close together. They stay picked through a move and an undo,
  and go when their string is no longer picked or shown, when they're deleted, or with Esc, which lets them go before
  it lets the strings go (0080's one thing a press).
- **A drag on one moves them all**: `string.move_vertices` (`strings`, `vertices` a list a string of its vertex numbers
  from 0, and `by`), the move being from the dragged vertex to where it snaps, to a micrometre. The rest stay exactly
  where they are. One run and one undo for every string.
- **Delete deletes them**: `string.delete_vertices`. A string that would be left with fewer than two vertices (three
  closed) refuses the run, named, and nothing changes (0078).
- **Set elevation, Set grade and Drape act on them alone** through a `vertices` input, the button saying how many ("Set
  6 vertices to RL 90"). Set grade measures from the first vertex picked on each string, in plan along the string
  through the vertices between, picked or not, and sets only those picked. Drape adds no vertices at the surface's
  triangle edges with vertices picked. With vertices picked these tools add no workflow step, since a workflow finds
  strings by rule, not vertices. `vertices` with `from` or `to` is refused.
- **Densify** (`string.densify`): each segment split into equal parts no longer than the spacing in plan, RLs graded
  evenly along it, so the line is the same line; unlike Smooth, nothing curves. On the Strings row after Filter.
- **Vertices at crossings** (`string.vertices_at_crossings`): a vertex on each string picked wherever another of them,
  or a string in `with`, crosses it in plan, at the string's own RL there (decision 2); none where the crossing is on a
  vertex already. On the Strings row after Densify.

The geometry is `crates/zd-geom/src/vertices.rs`.

| Test | Known answer | Tolerance |
|---|---|---|
| `three_of_ten_moved_five_metres_north_and_the_rest_exactly_where_they_were` (zd-geom) | vertices 3, 5 and 7 of ten 5 m north, a number twice taken once; the other seven unchanged | Exact |
| `a_grade_on_the_vertices_picked_alone_measured_along_the_string` | vertices 3, 4 and 8 at 10 % from RL 50: 50, 51 and 55 (0, 10 and 50 m along); the others as they were | 1e-9 m |
| `two_lines_crossing_each_gain_a_vertex_at_the_crossing` | a line east at 10 % crossed 30 m along by one north at RL 50: a vertex on each there, RL 103 and 50; again, none | 1e-9 m |
| `a_100_m_straight_every_2_m_is_51_vertices_all_on_the_line` | 51 vertices, the ends exactly, each on the line; a closed 10 m square every 5 m, 8 | 1e-9 m |
| `three_of_ten_moved_five_metres_north_in_one_run_and_one_undo` (zd-commands) | three of one string's and two of another's, one run: those moved, the rest unchanged; one undo for both | Exact |
| `vertices_deleted_together_and_never_too_few` | three of ten deleted; a run leaving a string one vertex refused, named, nothing changed | Exact |
| `set_elevation_grade_and_drape_on_the_vertices_picked_alone` | RL 90 on two; a grade on three; two draped on a plane at RL 120, no vertex added; the rest as they were | 1e-9 m |
| `a_100_m_straight_densified_every_2_m_is_51_vertices_on_the_line` | 49 added, 51 in all, every one on the line | 1e-9 m |
| `zd-app --bench --string-edits` | two lines picked, Alt and a box round three vertices of each: the panel says 6 picked on 2 strings; Set elevation sets them alone to RL 90; a drag of one onto the vertex 10 m north moves all six 10 m north; Ctrl+Z puts them back; Delete deletes them; Esc lets them go, then the strings; Densify every 5 m makes 19; Vertices at crossings adds one to each line and two to the line across | 1e-6 m |

The Strings row now folds Densify and Vertices at crossings into More at the bench's width, with Move and the rest after
them; the bench reaches them there.

In the app, `--string-edits` passes 31 of 31, `--shell` 62 of 62, and `--select`, `--clicks` and `--workflow` pass whole.
