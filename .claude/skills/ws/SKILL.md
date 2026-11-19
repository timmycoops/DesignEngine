---
name: ws
description: Pick up a workstream from a tab of the Pit Design Engine Build Plan doc and take it to a PR. Use when Tim names a plan tab and a workstream number.
argument-hint: "\"<plan tab>\" <workstream number>"
arguments: [tab, ws]
---
1. **Read the workstream.** The Build Plan is a Claude Docs doc: https://claude.ai/artifact/R9pWpyriv4fqPAEUFiKD91
   ("Pit Design Engine — Build Plan"). Read it with the docs tools, never a web fetch. Open the "$tab" tab and find
   workstream $ws, what it depends on, and its decision record in `docs/decisions/`.
2. **Read the area.** The decision record, and the `.claude/rules/` file for the objects it touches (CLAUDE.md's
   object model names them). For a new command, `docs/decisions/0002-industry-research.md` and the Deswik API reference
   CLAUDE.md points to.
3. **Plan.** Say in mining terms what lands, then a short plan: files, commands, tests (a synthetic case with an
   analytic answer, plus golden data when it exists, tolerances stated). If it touches the object model, the command
   interface or the stack, **stop and wait for Tim's yes**. Otherwise go on.
4. **Branch** `<area>/<topic>` from an up-to-date `main` (`git pull --ff-only` first), e.g. `strings/vertices`,
   `reserves/view-filter`.
5. **Build with tests.** Engine rules in CLAUDE.md hold throughout. Report timings for anything geometry-heavy
   (triangles in, ms, triangles out). Golden data: counts, totals and timings only.
6. **Check:** run `/pr-ready` and fix anything red. If geometry or reserves changed, tell Tim `/gate` is worth a run.
7. **Record.** Add or extend the decision record's "Built" section (what landed, tests, timings, known limits). Say
   what to change on the Build Plan tab and change it on Tim's yes.
8. **Ship.** Commit as `<what changed, in a planner's words> (<plan name> ws$ws, <decision number>)`, push, open the PR
   with the same title, and bind it. Don't merge: Tim says when. "Looks right" isn't a merge; ask.
9. **After Tim merges:** pull `main` and rebuild the exe he runs from `app/` with `pnpm tauri build --no-bundle`
   (never `cargo build --release -p zd-app`), so it isn't older than `main`.
