# Brief: fixes from a hands-on review of the app, 17 November 2026

From Tim, via a review session in claude.ai. Claude used the release build (`target/release/zd-app.exe`, built
17 Nov 11:08, main at `3eedfde`) on a copy of the 16 Nov workflow demo
(`bench/out/ui-phase/demo16/Workflow demo - Claude review.zdp`). It ran the Base case from the toe to the reserves table,
read the table and the run log, and checked one seam (B1 U) by hand. Nothing in the engine was found wrong; the six items
below are in the app, the starter dataflow's naming and the demo's recorded figures.

Read CLAUDE.md first. Plan first, as usual: item 2 renames a field in the starter dataflow and touches saved projects,
so agree a short plan for it with Tim before building. The rest are app changes. Each fix ships with a test or an app
bench check, as CLAUDE.md asks.

## What held up (no change wanted)

- Base, toe to reserves, in about 2.3 s for 1,815 blocks: 13 steps in 0.5 s to the pause, 8 more in 1.8 s. The cuts by
  seams (526 ms), benches (524 ms) and polygons (496 ms) are the slowest steps.
- Imported waste plus coal is 76,003,157.5 m³, as 0072 records, and ROM coal plus ROM waste adds back to it.
- B1 U by hand: RD 1.444 at Mad 3.5 % and Mis 8 % gives 1.415 in situ by Preston–Sanders; 1,267.8 m³ × 1.415 is the
  1,794.1 t shown; ash 16.05 air dried gives 15.31 in situ (× 0.92 ÷ 0.965).

## 1. Closing the window loses work without asking (most important)

**Seen:** after a full workflow run (rows 96 to 120 of the run log, nothing saved), the window's close button closed the app at
once. No prompt; the run was gone. The project's `.zdp` was unchanged on disk and its `.zdp.lock` was left behind.

**Where:** no close handler was found in `app/src` or `app/src-tauri/src` (no `onCloseRequested`). The lock is written
in `crates/zd-io/src/zdp.rs` (around line 1226) and not taken away when the window closes.

**Wanted:**
- Closing with unsaved changes asks: Save, Don't save, Cancel. Cancel keeps the window open. The same for New and Open
  over an unsaved project, if they don't already ask.
- The project's lock is taken away whenever the app closes, however it closes.

**Check:** an app bench check that runs a command, asks to close, and finds the prompt; Don't save closes and leaves no
lock file; Save writes the file first.

Done (19 Nov, branch `app/review-fixes`): the shell keeps the window when there's work since the last save or open (a run,
undo or redo; showing and hiding don't count) or a run going, and the app asks Save, Don't save or Cancel; New and Open
ask the same. The lock note is taken away on the app's exit, however it closes. Tests: `zd-commands`
`unsaved_work_is_known_and_the_lock_goes_when_the_file_is_let_go`, `zd-app`
`closing_asks_only_with_work_to_lose_and_takes_the_lock_away`. Not an app bench check: the prompt is the system's own
dialog, which the bench can't answer, so the close itself wants a check by hand.

## 2. `rd_adt` means two things, and Main fields shows the wrong density beside tonnes

**Seen** (Reserves table, Base, block B1, Main fields):
- under **Imported\Coal**, `rd_adt` is the lab RD: 1.444 for U, and tonnes are volume × 1.444 (1,831.3 t);
- under **Insitu\Coal**, `rd_adt` is 1.349 for U, beside `tonnes` 1,794.1, which are on the in situ RD of 1.415.

The 1.349 is the air-dried tonnes per in situ m³ (1,710.5 t air dried ÷ 1,267.8 m³), as Spry works it, so the figure
is right. But one name holds two meanings, and the row a planner reads next to in situ tonnes is not the density that
made them. Volume × the RD shown doesn't give the tonnes shown. Every field shows all three (`rd, in situ`, `rd_adt`,
`rd, ROM basis`); Main fields doesn't.

**Where:** the field names come from the starter dataflow (`crates/zd-commands/src/builtin/starter.rs`); `rd_adt` also
appears in `crates/zd-script/src/reference.md`, the AI-written test scripts under `crates/zd-commands/tests/ai_written/`
and `app/src/bench/dataflowCheck.ts`. Main fields' choice of rows is in the app's reserves table.

**Wanted (agree the plan with Tim first):**
- Main fields shows, under each folder, the density its `tonnes` are on: `rd, in situ` under Insitu and Working
  section, `rd, ROM basis` under ROM and Scheduled, the lab RD under Imported. Volume × the density shown = the tonnes
  shown, wherever both are shown.
- The air-dried-tonnes-per-in-situ-m³ field gets a name that says so (for example `rd, air dried on in situ volume`),
  shown beside `tonnes, air dried`, not beside `tonnes`. Imported's lab RD stays `rd`.
- Saved projects with the old name still open: say how in the plan (a rename on open, or both names read).

**Check:** a test that, for every folder in Main fields, volume × the density shown equals tonnes to 1e-9; the demo
pit's figures otherwise unchanged.

## 3. Run all cases stops at the pause and never runs 50m

**Seen:** Run all 2 cases ran Base to its pause at step 14. Continue from step 15 finished Base ("8 steps run from step
15 in 1.8 s, finished") and stopped. 50m never ran, and nothing said so; the panel showed Run 21 steps as if done.

**Where:** `runAllCases` in `app/src/App.tsx` (around line 346) and the Workflow panel (`app/src/panels/WorkflowPanel.tsx`).

**Wanted:** after a pause in Run all cases, Continue carries on through Base and then the cases after it, as Run all
cases would have. If that isn't wanted, the panel says plainly which cases are still to run and offers to run them.
0066's decision 5 (Run all cases stops at a pause or refusal) stays true; this is about what Continue does next.

**Check:** the workflow bench check (`app/src/bench/workflowCheck.ts`) runs all cases through the pause and finds both
cases' tables written in this run.

Done (19 Nov, branch `app/review-fixes`): after Run all cases stops at a pause, the panel says which cases are still to run,
and Continue reads "Continue from step N, then 50m": it runs that case on and then the cases after it, as Run all cases
would have, stopping again at the next pause or refusal (0066's decision 5 holds). Tests: `zd-app`
`continue_after_a_pause_runs_the_cases_after_it`, and the workflow bench check now puts a pause in as step 2, runs all
cases, presses Continue and finds the second case's pieces.

## 4. After a run, and at the pause, the view is empty

**Seen:** with Tidy after a run on (the default), the view after the full run showed only the strings and 0 triangles.
The only layer left on show was `10 reserves`, the reserves table, which has no geometry. At the pause (step 14,
whose note says it's there to look at the strips), the strips layer was hidden as well.

**Where:** Tidy after a run in `app/src/panels/WorkflowPanel.tsx` (around lines 20 and 33).

**Wanted:**
- The result left on show is the last geometry the run made (the blocks on the demo), not a table.
- At a pause, what the step before the pause made is on show, so a pause meant for looking at the strips shows them.

**Check:** the workflow bench check finds triangles in view after a run and at the pause, and the demo's strips on show
at step 14.

Done (19 Nov, branch `app/review-fixes`): Tidy after a run now shows what the last step that made something drawn made
(strings, surfaces, solids, grids, seam models), walking back from the end. On the demo, the reserves table written last
leaves the blocks on show, and the strip delete just before the pause leaves the strips on show. Tests: `tidyResult` in
`app/tests/workflow.test.mjs`, and the workflow bench check finds the cut pieces on show at its pause. Not checked on
the demo itself in the app yet.

## 5. The ROM section isn't listed in stack order

**Seen:** in the Reserves table at All, `Working section\name` reads `U; M; L` (stack order), but `ROM\section` reads
`M; L; U`.

**Wanted:** every list of seams or sections in the table follows the seam model's stack order, as working sections do.

**Check:** a reserves test with three sections in one column, finding the ROM section list in stack order.

Done (19 Nov, branch `app/review-fixes`): the cause was that the Reserves table listed a column's words in the order the
leaves were met. U is lost in the first blocks, so ROM sections met M first, while working sections met U first. The
table now lists seam and section names in the stack's order, worked out from the table itself (each column's seams
top down, merged across columns), and other words in the order met. Test: `section_lists_follow_the_stack_not_the_order_met`
(`zd-commands` reserves view: U subcropping in the first block, All reads U; M; L). On a fresh run of the demo, Base
and 50m both read `U; M; L` for ROM sections and working sections.

## 6. The demo's recorded totals have moved

**Seen:** Base after a fresh run:

| | Now | 0074 recorded |
|---|---|---|
| ROM coal | 6,247,592.2 m³, 9,234,488.8 t | 6,247,551.1 m³, 9,234,451.4 t |
| ROM waste | 69,785,935.0 m³ | 69,785,976.1 m³ |

41.1 m³ has moved from ROM waste to ROM coal, and the total is identical. Probably a deliberate change since 0074
(0077's Spry-method scripts say each change to the demo's totals would be listed), but the figure 0074 records no longer
catches the next change.

**Wanted:** find which change moved it and say so in its decision record. Then keep one place (the demo script's
asserts, or `bench/RESULTS.md`) holding the demo's current Base and 50m totals, with the check failing when they move.

Done (19 Nov, branch `app/review-fixes`). I ran the demo at each commit since 0074 recorded its figures, and the move
came from 25b41af, "Measure a block's length along its strip and width across it" (0074). The new lengths changed
edge loss and dilution, and 41.1 m³ went from ROM waste to ROM coal. 0074 now says so under its table. The current totals
for Base and 50m (ROM coal m³ and t, ROM waste, out of model, clean coal) are `RECORDED` in
`testdata/synthetic/workflow_demo.py`, which fails, naming each figure, when one moves by more than 0.1. `/gate` reads
them from there.

## Smaller things, take or leave

- Two face designs are both named "Highwall" (one r3, one with no revision shown). Say which the workflow uses, or keep
  names unique.
- The layer tree isn't in its numbers' order: `1 toes`, `0 pit limit`, `7 block lines`, `0 ramp strings`, then
  `parameters`, then `2 faces`.
- Starting the app again opens a second window rather than bringing the open one forward. Two windows on one project
  are kept apart by the lock, so this is only a nuisance; a single-instance option is Tim's call.

## Files left by the review

`bench/out/ui-phase/demo16/Workflow demo - Claude review.zdp` (and its stale `.lock`), and an unused copy at
`testdata/synthetic/reserves/workflow demo/Workflow demo - Claude review.zdp`. Neither is needed; delete both.
