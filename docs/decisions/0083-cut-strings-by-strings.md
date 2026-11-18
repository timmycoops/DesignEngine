# 0083 — Cutting strings by strings: Trim, Extend, Split at a point, Join finished

Status: agreed 17 November 2026 (Tim: "Agree all nine", the String tools plan tab of the Build Plan). This record is
that plan's workstream 1; the later workstreams add to it as they merge: workstream 2, round corners, below.

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
