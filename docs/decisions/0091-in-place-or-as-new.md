# 0091 — In place or as new on a layer: one rule for every tool

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20: "Agree, start ws0", the Surface tools plan tab, decision 6)

## Why

Tim, 20 November, on whether Trim changes things in place or makes a new layer:

> We should be consistent with that behavior for all functions across the tools so happy to make it work as required but
> be consistent.

The tools weren't consistent:

- The string edits and `solid.repair` changed their objects in place.
- Offset, Project and the cuts made new objects.
- Move had a separate Copy beside it, and Rotate and Mirror had a Copies tick of their own.
- Engine rule 4 in CLAUDE.md ("commands never modify their inputs") no longer matched the code. The review of 10
  November raised this as finding 9.

## Decision

1. **Two kinds of tool, one rule each.**
   - **A tool that changes an object** works in place by default. The change is the object's next revision, and one undo
     puts it back. Run as new, the same run makes each changed object anew and leaves the original as it was.
   - **A tool that makes something** always makes new objects on a layer it names: Offset, Project, Contours, Merge,
     Triangulate and the cuts. There's nothing to change in place.
2. **It's done once, in the command layer, for every command.** Any command takes `"result": {"as_new": true, "layer":
   "…"}` beside its own parameters. The session takes `result` out before the command reads its parameters, so no
   command knows about it. A new command gets the choice with no code of its own. As new:
   - each revision becomes a new object, named for what was done ("Toe 3 graded", "Topo trimmed", "Toe 1 copy"), on the
     layer given, or on its original's layer if none is given;
   - each deletion is dropped, so the originals stay as they were (Join as new deletes no pieces);
   - with a layer given, anything else the run makes goes on that layer too (Trim's later parts);
   - a note says what was done: how many copies, on what layer, and what a change in place would have deleted;
   - a run that changes nothing in place says as new made no difference;
   - the run's record keeps `result`, so a workflow step replays it the same way;
   - an unknown setting, or a blank layer, refuses the run.
3. **One control on every panel.** The kit's Result section (In place / As new, and On layer when As new) shows on every
   panel whose tool changes objects: the Strings panel's changing tools and Move. The tool list says which. A workflow
   step added from such a panel keeps the choice. Whatever is made as new is picked once made.
4. **Copy is Move as new.** The Move panel has Move, Rotate and Mirror, each in place or as new. Its Copy tool and the
   Copies tick are gone. As new on the original's layer is exactly what Copy made: "<name> copy" on its own layer.
   `object.copy` stays registered so saved workflows and scripts still run.
5. **Engine rule 4 is reworded** to say this.

## Where

- `crates/zd-commands/src/result_as.rs`: `take_result`, `make_anew` and `done_word`. They are called in `RunJob::run`,
  and a cut's pre-check in `workflow_run.rs` ignores `result` too.
- `app/src/design-system/kit/ToolPanel.tsx`: `useResultAs`, `resultParams`, the Result section and `ToolAction.result`.
- The Strings panel, whose changing tools are listed in `CHANGES`, and the Move panel.

## Checked

| Test | Known answer | Tolerance |
|---|---|---|
| `graded_as_new_leaves_the_toes_and_makes_graded_copies_on_a_layer` | Two flat toes graded 10 % as new on 'graded' give "Toe 1 graded" and "Toe 2 graded" there, each rising 10 m. The toes are at their revisions and RL 100. The record keeps `result`. One undo takes the copies off. In place, the same run revises them. | 1e-9 m |
| `a_trim_as_new_with_no_layer_keeps_each_on_its_originals_layer` | "Toe 1 trimmed" and "Toe 1 part 2" on 'toes', the cut at RL 106, the toe whole | 1e-9 m |
| `a_join_as_new_deletes_nothing` | One joined string of 4 vertices; all three pieces still there, and said | Exact |
| `a_move_as_new_is_a_copy` | "Toe 1 copy" on 'toes', 100 m east; the toe where it was | Exact |
| `what_result_cant_be` | An unknown setting and a blank layer refused with nothing changed; Offset as new says it made no difference | Exact |
| `zd-app --bench --string-edits` | Clip as new on 'clipped' leaves the toe whole and makes "Long toe clipped" there, RL 105 to 115, picked. Mirror as new makes a "Dog-leg copy" across the pit limit, picked, with the dog-leg where it was. Ctrl+Z puts each back. | 1e-6 m |

In the app: `--string-edits` 41 of 41, `--shell` 62 of 62, `--workflow` 54 of 54, `--select` 29 of 29 and `--clicks` 24
of 24.

Known limits:

- Only the Strings and Move panels show the control so far. The Builder's Rebuild, Repair and the Surfaces panel take it
  as they're built or touched, and the command layer already honours it for them.
- A copy's later parts keep their command's own names ("Toe 1 part 2"); only the first changed object is named for
  what was done.
