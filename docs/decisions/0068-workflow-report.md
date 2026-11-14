# 0068 — The workflow report: one HTML page from the runs themselves

Status: agreed 14 November 2026 (Tim: "Agree all six - as long as we can update and change in the future", on the Build
Plan's "Report plan" tab). Stage 3 of the Workflow plan, report v1, on 0059, 0061, 0062, 0065 and 0066. Each decision
is open to change as the report grows.

## Why

Tim: all reporting is HTML. A workflow makes the blocks of each case, but what they add up to, what was left out and
whether it all comes back to the pit had to be pieced together from the Reserves panel, the run log and the layers. The
report writes it down from the runs themselves, never typed up by hand, so it can be sent on as it is.

## Decision

1. **Where it goes:** an HTML file in a folder beside the project, `<project> reports/<workflow> <date> <time>.html`,
   written and opened in the browser from Write report on the Workflow panel, the latest listed there. Not inside the
   `.zdp`, so it can be emailed as it is. A second report in the same minute takes the next number; none is overwritten.
2. **What one report covers:** the latest run of every case, Base first, side by side.
3. **What a block is:** a solid a step of the case last made or changed that carries a `block` attribute and is still in
   the project. Coal is material `coal`; the rest is waste, what lies below the last floor kept out of the strip ratio.
4. **Tonnes in v1:** volume × the density the interrogation wrote (the model's density code, with its prefix), on the
   density grid's own basis, usually air dried, and the page says so. RD is composited by volume and every other value
   by coal tonnes, as the interrogation averages them; waste in bcm; the strip ratio as bcm : t. In situ tonnes by
   Preston–Sanders come with the reserves dataflow (stage 5).
5. **The reconciliation:** the pit (what the first `solid.between_surfaces` step made, or else what the first cut read)
   against the blocks plus what was left out, the difference always shown, in red beyond 1e-6 of the pit (engine
   rule 9). Left out: the seam cut's measured notes (layers thinner than the minimum, flaps), and solids deleted, by a
   step (as the demo deletes what's behind the last face) or by hand at a pause. For that, `object.delete` and
   `object.delete_many` now say the volume of solids they take out in a note (`object.solids_deleted`), as the undo
   history doesn't survive closing; a delete undone no longer counts.
6. **Built in the engine:** `Session::workflow_report` works out the figures and `Session::write_workflow_report` writes
   the page, one self-contained file (styles and a small script inline, nothing fetched), so the app, Python
   (`zd.Session.workflow_report`, `write_workflow_report`) and the tests write the same file. Changed from the plan: a
   session call beside save rather than a registry command, as it reads the run log, which a command can't, and changes
   nothing in the project.

## The page

A header with the cases and when each ran, and badges for the reconciliation, the warnings and the tonnes basis; the
summary by case with differences from Base in blue and what each case changes; then, a case at a time by tab (all one
after another without the script, and in print): by seam and material, every block (sort by a heading, filter by words,
save as CSV), its volume beside its 2.5D column, what was left out with the reconciliation, and the checks and warnings;
then the steps with their inputs in words, settings, each case's changes and last results, and the inputs with what they
find now and the seam models' grid files with their fingerprints. Dates in the machine's local time.

## Not here

Comparing two runs (October against November geology); the workflow canvas drawn in the report; charts; in situ and ROM
tonnes, product and yield; an export for Spry: report v2 (stage 7). Weights an interrogation was given for a variable are
not followed yet: the report composites by tonnes.

## Done when

The demo workflow's report opens offline with Base and 50m side by side; each case's blocks plus what was left out equal
the pit to 1e-6 relative; block volumes and interrogated values are the blocks' own, as the Reserves panel reads them;
the made-up pit's numbers are checked by tests.

## Built

`crates/zd-commands/src/report.rs` (the figures) and `report_html.rs` (the page); known answers in
`crates/zd-commands/tests/report.rs` on a made-up two-seam strip, 300 × 60 m: overburden 180,000 m³, cu 72,000, cu-cl
36,000, cl 72,000 and 180,000 below; coal 208,800 t at RD 1.45 (less 0.0017 t, the f32 grid's 1.4), ash by tonnes, strip
ratio 216,000 bcm over the coal tonnes; the 540,000 m³ strip reconciled; a block deleted at a pause left out as 60,000
m³ and still reconciled, and no longer once undone. Figures in 0.4 ms. `testdata/synthetic/workflow_report.py` builds
the demo and reports it: Base 533 blocks and 50m 1,064, both 9,358,563 t of coal in 6,461,326 m³ (RD 1.448) and
69,541,883 bcm of waste, strip ratio 7.43, each reconciled to the 104,472,557 m³ pit within 1e-6 m³ (the solid behind
the last face deleted by step 13, 28,469,334 m³); figures in 10 ms, the page (500 kB) in 7 ms. The app's Workflow panel
has a Report section; `zd-app --bench --workflow` checks it asks for the project to be saved first.
