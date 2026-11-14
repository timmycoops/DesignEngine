# 0065 — Workflows: saved steps run again in one go

Status: agreed 14 November 2026 (Tim: "Go for it", on the stage 1 plan as it stood). Stage 1 (the frame) of the Build
Plan's "Workflow plan" tab, whose eight decisions Tim agreed the same day ("Yep I agree"). Cases (stage 2) and the HTML
report (stage 3) are designed for here but not built. Workstream 1 (the object, rules, capture) built.

## Why

The chain from a toe to interrogated blocks runs end to end (0049, 0062 to 0064), but each pass is a dozen panel clicks,
and a revised toe or a new seam model means doing them all again. A workflow keeps the steps, their settings and Tim's
note on why, and runs them again in one go.

## Decision

1. **A workflow is an object** (`Body::Workflow`, kind `workflow`), saved in the `.zdp` as JSON beside face designs, with
   revisions kept for undo. It holds its steps in order and, per step, what its last run did. Cases go in it in stage 2.
2. **A step** has a stable id, a note, an on/off switch, and is either a command (its name, its settings, and a rule for
   each input) or a pause.
3. **An input is picked by a rule**, worked out each time the step runs:
   - what an earlier step made (still there), optionally only one kind, or only where an attribute has a value;
   - what's on a layer, with the same filters;
   - one object by name and kind (a seam model, a face design);
   - the objects as picked, by id: the fallback, said as a warning when a capture has to use it.
   A rule that finds nothing, or more than one object for a one-object input, refuses the step and says which rule.
   Objects come in the order the earlier step made them, or the project's order for a layer.
4. **Capture** turns runs from the run log (those still applied: not refused, not undone) into steps. Each input id
   becomes the simplest rule that finds exactly the same objects at that point: an earlier captured step's output, whole
   or filtered; else a layer, whole or filtered; else a named object; else the ids. Capture is a session operation that
   saves the workflow with `workflow.save`, so it's logged and undone like any run.
5. **Add to workflow**: a tool panel built on the kit has a mode in which its foot button reads Add to workflow and adds
   the panel's command and settings as a step, its picked objects turned into rules the same way.
6. **Run, run from step N, and stop**: steps run in order, each an ordinary logged command run. Before a cut, its check
   runs first (`check_slice`, `check_cut_by_polygons`, `check_cut_by_seams`) and a refusal stops the run with its notes.
   The first refused step stops the run with that step's notes; nothing after it runs. Run from step N keeps what the
   earlier steps made: their outputs are read from the workflow's last-run record.
7. **A pause** ends the run there. Tim does what he needs by hand, then Continue runs from the step after the pause.
8. **One undo a run**: the steps of one run (to the end, a refusal or a pause) apply as one undo step, with the
   workflow's last-run record. Hand work at a pause is undone on its own, and Continue is another undo step.
9. **A rerun makes new revisions of what the last run made** (agreed decision 6). What a step makes is matched to what it
   made last time by layer, name and kind: a match becomes the next revision of that object (same id), what wasn't made
   again is deleted, and what's new is created; the counts are said in the step's notes. Commands are unchanged: the
   runner turns their creates into revisions before applying them.
10. **Python**: `s.capture_workflow(...)`, `s.run_workflow(workflow, from_step=...)`, `s.workflow(...)`.

## Workstreams

1. **The object, rules and capture**: `Body::Workflow` saved as JSON in the `.zdp`; `workflow.save`, refusing a rule that
   names a step not before it and warning (`workflow.picked`) of inputs kept as picked; `workflow::resolve`;
   `Session::capture_workflow`, working out what was there at each point from the run log (only current revisions
   survive closing a project, so an object's attributes then are read from its revision while the project holds it, and
   as it is now after). An input is any parameter holding an object's id, or a list of them, whether or not the
   command's spec lists it, so `face.save_design`'s design is one.
2. **Running**: run, run from step N, pause, stop at a refusal, checks before cuts, one undo a run, reruns as revisions,
   a `workflow` entry in the run log, Python.
3. **The app**: the Workflow panel, Capture in the Run log, Add to workflow on the kit's tool panels.

## Not here

Cases and the settings grid (stage 2), the HTML report (stage 3), templates across projects, grouping and utility steps.
Nothing reruns by itself (engine rule 4).

## Done when

Tim's toe-to-blocks sequence on the made-up pit (`testdata/synthetic/reserves_workflow.py`, then `_v2.py`) is captured
from its runs and run again to the same blocks, each block's volume and the total within 1e-6 m³; with known-answer
tests in Rust.
