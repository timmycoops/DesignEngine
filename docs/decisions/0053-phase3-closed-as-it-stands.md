# 0053 — Phase 3 closed as it stands

Date: 2026-11-10 · Status: accepted (Tim, 2026-11-10: "Yes, close Phase 3", on the gate table of the review of the
project to date; the Build Plan's "Review 10 Nov" tab and the Phase 3 plan tab's "Closed as it stands, 10 November").

## Why

The Build Plan says no phase starts until the gate before it passes. Phase 3's gate was left open while the work moved
on to strip solids (0049), which is Phase 4's ground, and to the shell of the app (0051). A review of the plan and the
code on 2026-11-10 set each of the gate's eight checks against what is built. Three pass, four pass in part and one
can't be measured yet. Closing the gate on that, with what's open carried forward by name, is clearer than leaving it
open behind work that has already gone past it.

## Decision

1. **Phase 3 is closed as it stands,** on the table below. Nothing in it is passed that wasn't, and nothing open is dropped.
2. **What's open is carried forward by name:** roads on grids, the measured match with Deswik, the merge with the
   topography, Tim's check of a face's DXF in Deswik, and the workstreams not built.
3. **What comes next is not decided here.** The review's proposed order (a store for thousands of solids, a structured
   run report, the app at scale, a thin reserves slice on imported blocks, block cutting, surface compare) is on the
   "Review 10 Nov" tab for Tim to agree. The store and the run report touch the object model and the command interface,
   so each needs its own short plan first.

## The gate, check by check

| # | Check | State | Where it stands |
|---|---|---|---|
| 1 | A profile set up in the form, saved and used again | Passed | The Face design panel (0024) |
| 2 | A real toe to one neat face, past the shell, re-run under 1 s | In part | To RLs on PIT B, 30 faces project in 146 ms. With ramps on the grid test pit, 11.4 s for 10 strips (0034, 2026-11-09) |
| 3 | Known answers to 1 mm | Passed | The batter, mitre, berm and step tests |
| 4 | No crossovers, spikes or slivers, nothing repaired afterwards | In part | 4 faults left on the grid test pit (0034). A face's corners within 1 mm are made one after it's built, and said |
| 5 | The wall matches the Deswik design: crest within 0.5 m along 95 %, volume within 0.5 % | Not measured | No compare command, and no clean real wall beside Deswik's |
| 6 | A ramp holds its grade on the inside edge and its width, and names the bench where it can't | Passed | 0028 onwards |
| 7 | A face cuts a real pit solid, parts watertight and adding up; the surface merges with the topography | In part | PIT B slices into 16 strip solids, each closed and adding up (0049). The merge isn't built |
| 8 | The design reads in section, undoes, saves and exports to Deswik by DXF | In part | A made-up face's DXF waits on Tim's check in Deswik. Section, undo and save were not tested again for the review |

## Carried forward

- **Roads on grids.** The four faults 0034 lists, and the time a design with ramps takes on grids. A known limit until
  Tim decides whether roads on grids must be automatic in v1, or are refused and reported with the manual builder (0048)
  for the front of each face.
- **The match with Deswik.** Waits on a surface compare (Phase 3 workstream 7) and a clean real wall from Tim.
- **Workstreams not built:** triangulate from strings with breaklines and a surface between two strings (5); merge, trim
  and compare (7); surfaces in section (9).

## What the review found, in short

The full findings are on the "Review 10 Nov" tab. The ones that bear on what comes next:

- A project of 43,181 solids saves a 588 MB manifest and 86,362 files, and every save rewrites the manifest and the run
  log whole. The store has one version and no migration.
- The app sends the whole project back after every change: at 42,218 solids, hiding one takes 0.4 s and moving one 2.3 s.
- A run's messages are plain text, which the app reads for warnings by their words, and wall faults carry no coordinates.
- Quality grids are fingerprinted but not loaded, and nothing samples a grid inside a solid, so reserves has nothing
  under it yet.
- The same design can give a slightly different number of triangles from run to run (noted in 0034).

No code changes with this record.
