# 0066 — Workflow cases: one workflow, several sets of settings side by side

Status: agreed 14 November 2026 (Tim: "Tag them makes the most sense", on the stage 2 plan as it stood). Stage 2 of the
Build Plan's "Workflow plan" tab, on the frame of 0065.

## Why

Varied reserves sets come from one workflow with a few settings changed: 100 m against 50 m blocks, another minimum
thickness, another batter. Copying a workflow and keeping the copies in step by hand is how they drift apart.

## Decision

1. **A case holds only its differences.** Base is the workflow's steps as they are. A case (`50m`) keeps, per step, the
   settings it changes; everything else reads as Base. Cases are kept in the workflow object, so they save, revise and
   undo with it. A case's name is unique, not "Base", and names no step that isn't there.
2. **A case writes to its own layers and tags what it makes.** In a case's run, every layer a step writes to (any
   setting named `layer` holding a name) becomes `<layer> <case>`: `8 blocks` is `8 blocks 50m`. Everything a step makes
   carries `case = <case>`, Base's `case = Base` (Tim's choice), so the report and the block table group Base like any
   other case.
3. **What's drawn and the geology are shared; what a case's steps make is its own.** A case's rules see the objects it
   made and the objects no workflow step made (the toe, pit limit and block lines drawn by hand, the grids, the seam
   model), never another case's. A rule by layer looks on the layer and on its case layer. A step that changes a shared
   input (numbering the drawn toe, draping it) does so in every case alike; a case that changes such a step's settings
   would change the shared input for all, so that's said, not done quietly: the toe and pit limit per case wait (below).
4. **Each case keeps its own last run per step,** so a case's rerun makes its own objects again as revisions, and the
   panel's badges, a click on a step and its eye show the case picked.
5. **Run this case, or Run all cases**: Base, then each case in order, as one undo. A pause or a refusal stops Run all
   cases at that case and step.
6. **The settings grid**: a row a setting (the plain values: numbers, text, true or false), a column a case; Base's
   column edits the step's own settings, a case's blank cell reads as Base. This is also option C of the demo guide:
   settings changed in the panel, with no tool rerun and capture.

## Built

- **Engine**: `Case` in the workflow (`zd-core`), `Workflow::as_case` and `keep_case_runs`; `Session::run_case`,
  `begin_case` and `run_all_cases` (one undo, joined under Base's run id); `CaseScope` for rules (`resolve_in`), case
  layers in a step's parameters (`case_layers`, never a layer held by its id), the `case` tag on what a step creates or
  makes again (`tag_case`); `workflow.save` refuses a case named Base, two of one name, a step that isn't there, and an
  input changed as a setting. Python: `run_workflow(..., case=)` and `run_all_cases`.
- **App**: the Workflow panel's Cases section (a case picked, new, taken out; Run all cases) and the settings grid; the
  steps show the picked case's last runs; Tidy after a run and the eye work per case; the shell's `run_all_cases`.
- **Checks**: Rust known answers on the box pit (Base cut at RL 40, case 60 at RL 60: 240,000 and 360,000 m³ each way,
  on `cut` and `cut 60`, tagged, one undo); `zd-app --bench --workflow` 31 of 31; on the made-up pit
  (`workflow_demo.py`), Base 526 blocks and 50m 1,050, each case's blocks within 4.5e-8 m³ of its seam pieces and the two
  totals 1.5e-8 m³ apart.

Known limit: a face design's lines are held by a layer's id, so in a case they run on Base's toe layer, not the case's
own offsets of the toe: the same toes unless a case changes the strip width. Moving a design's lines to the case's
layer comes with inputs per case.

## Not here

A different toe, pit limit or seam model per case (overrides of inputs, not settings); comparing cases (the report,
stage 3).

## Done when

On the made-up pit, Base (100 m blocks) and `50m` (the block line offset 50 m, 30 times) run side by side on their own
layers, each case's blocks adding up to its seam pieces, and the two cases' totals the same, to 1e-6 m³; with
known-answer tests.

## Settings under their step (14 November 2026)

Tim: settings belong with the step they drive, so they're seen at the time, beside the inputs that step takes. The
settings grid (a row a setting, a column a case) is gone. Each command step's sliders open its plain settings under it,
as the case picked has them: Base's are the step's own; in a case an empty box reads as Base, its value greyed in it,
what the case changes shows in blue, and the step carries a blue badge saying how many it changes. One case at a time;
a side-by-side compare of cases is the report's (stage 3). Lists and tables (points, profiles) are still set in the
step's tool. Nothing changes in the engine or the saved workflow.
