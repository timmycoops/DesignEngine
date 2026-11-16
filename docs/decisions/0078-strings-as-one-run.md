# 0078 — String tools on several strings, as one run

Status: agreed 16 November 2026 (Tim: "Agree all 7", the Build Plan's "Strings on many strips plan" tab). Workstream 1,
the commands, is built here. Workstreams 2 to 4 (the panel, Join on the pieces picked, Add to workflow) are to build.

## Why

Strips come as a toe offset 10 to 15 times. The Strings panel ran a tool on one string, the last picked, so draping
fifteen toes on a grid, setting them to an RL or smoothing them was a pick and a press each: 30 in all, with 15 undo
steps, 15 lines in the run log and 15 steps in a captured workflow.

It isn't speed. Measured before this from Python on made-up data, fifteen toes of 40 vertices draped a run each took
3 ms on a grid of 97 x 65 nodes and 91 ms on a surface of 1.0 M triangles.

## Decision

1. **Eight commands take several strings**: `string.offset`, `string.set_grade`, `string.set_elevation`,
   `string.drape`, `string.smooth`, `string.filter`, `string.reverse` and `string.set_closed`. `string.split` stays on
   one string, as it works at a vertex. `string.number_across` and `string.polygons_from_lines` already take several.
2. **One run for them all.** Each of the eight takes `strings`, a list, beside `string`: one run, one undo, one record,
   one step in a workflow. `string` still runs exactly as it did, with the same words, so saved workflows, scripts and
   the templates' run logs carry on. Giving both, or neither, or a string twice in the list, is refused. A workflow
   finds a list's strings by a rule (0065), so a step captured on the toes on a layer runs on however many are there.
3. **A string that can't be done refuses the run.** Nothing changes, and each such string is listed with why
   ("1 of 5 strings can't be done, so nothing was changed. 'Short': a closed string needs at least 3 vertices"). A
   string that comes out as it was (already closed, already at the RL, nothing to filter out) is not revised, and is
   said; where every string does, the run changes nothing and says so, and is not a refusal, so a workflow run again
   doesn't stop there. One string given as `string` and already closed is still refused, as it was.
4. **Offset's side is each string's own** left or right, looking along it. A string running the other way to the first
   (open strings: more than a right angle apart from first vertex to last; closed ones: turning the other way) went to
   the other side, and the run warns of it by name (`string.runs_other_way`).
5. **The panel works on every string picked** (workstream 2), not the last one, and doesn't fall back to the layer.
6. **Join takes the pieces picked** and joins them end to end into one (workstream 3).
7. **The Strings panel adds a step to a workflow** (workstream 4).

Settings are the same for every string in a run. One that names a vertex means one string, so with several it's
refused, not guessed at: Set grade's `from` and `to` (several are each graded from their first vertex to their last,
from their own RL there or from `rl`), Set elevation's `from` and `to`, and Offset's `name`.

What a run of several says: a line of what it did to how many ("draped on Topo: 15 of 15 strings"), then each string's
own line on that string, under its name, so a click goes to it. Warnings keep their codes and measures.

## Built: workstream 1, the commands

`crates/zd-commands/src/builtin/string.rs`: `picked` reads `string` or `strings`; `edit_each` runs an edit over them
with the rules above; Offset has its own loop, since it makes strings. Drape triangulates a grid once a run. The
refusal for a closed string of two vertices read "an closed string"; it reads "a closed string" now.

Tests, `crates/zd-commands/tests/strings_many.rs`, with their known answers:

| Case | Known answer | Tolerance |
|---|---|---|
| Fifteen toes set to RL 120 | Every vertex at RL 120; 15 revised in one record; one undo puts all back | Exact |
| Three lines running east offset 40 m right, twice | Each new line 40 m and 80 m south of its own | 1e-9 m |
| A line running west among them | Goes north, and is warned of by name; so is a square drawn the other way round | 1e-9 m |
| Five toes draped on a plane z = 50 + 0.1x + 0.05y | Every vertex on the plane | 1e-9 m |
| A string half off the surface | Draped where it can be, named, 1 vertex off | Count |
| Five strings, one of two vertices, closed | Refused; all five open, at revision 1 | Exact |
| An offset that leaves nothing of one string of three | Refused; no string made | Count |
| One string as `string`, then as `strings` | The same 21 vertices | Exact |
| Three toes graded at 2 % | Each ends 0.02 x its 402.237 m plan length above its own start | 1e-9 m |
| A workflow step captured on three toes on a layer | Runs on five | Count |

`crates/zd-py/tests/test_session.py` runs three toes as `strings` from Python, with the refusal and the one undo.

Timings, release build, 16 November 2026, strings of 40 vertices, one run for them all:

| One run of | 15 strings | Vertices in, out | 150 strings | Vertices in, out |
|---|---|---|---|---|
| Drape on a grid of 97 x 65 nodes | 2.6 ms | 600, 3,300 | 24.6 ms | 6,000, 33,040 |
| Drape on a surface of 999,698 triangles | 98.7 ms | 3,300, 23,328 | 977.7 ms | 33,040, 233,304 |
| Set elevation | 0.2 ms | 23,328, 23,328 | 2.5 ms | 233,304, 233,304 |
| Filter at 0.5 m | 0.6 ms | 23,328, 375 | 5.8 ms | 233,304, 3,750 |
| Smooth at 5 m | 0.2 ms | 375, 6,840 | 1.6 ms | 3,750, 68,400 |
| Reverse | under 0.1 ms | 6,840, 6,840 | 0.8 ms | 68,400, 68,400 |

Each run is the one after the row above, on the strings as that left them. The timing test is
`timings_for_many_strings_in_one_run`, ignored unless asked for
(`cargo test --release -p zd-commands --test strings_many -- --ignored --nocapture`).

## Known limits

- **The app still sends one string**: the Strings panel is workstream 2. Until then several strings as one run are
  for scripts and workflows.
- **Every string takes the same settings.** A different distance or RL a string is not here; nor is a list of strip
  widths, which Offset's `times` does only for equal ones.
- **Offset's side isn't asked for by a click.** Strings running against the first are warned of, not turned.
- **An open string and a closed one aren't compared** for which way they run, and an open string whose ends are over
  each other in plan is compared with nothing.
- **A run of several that changes nothing** is in the run log but is no step to undo.
