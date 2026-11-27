# 0108 — Two sides plan: design work and table work on two sides of the app, joined by the Objects table and a prepared input; light by default. All 10 decisions agreed by Tim 2026-11-27.

Date: 2026-11-27 · Status: accepted (Tim, 2026-11-27: "Agree all 10", on the Build Plan's "Two sides plan" tab)

## Why

Tim, 27 November: "we have gotten off track with our UI ... we need a different concept to help make this easier as we
add more tools and panels ... I feel we are piece mealing ideas into the platform."

A review of main at #299 found the shell set by 0076 assumes the 3D view is always the main thing, with every other
panel round it. That held for strings, walls and solids. The reserves workbench has grown inside it since, and the
scheduler is next. In the 11 days from 0076 the tool list went from 64 to 103 tools and the panel files from 17 to 46,
and 112 of 134 merged PRs changed the UI. How to see a block's reserves went through 11 plans in 13 days (0069 to
0107), each replacing or patching the last. The Reserves case tab works only filling the window. The UI rules covered
widgets, not places, so each plan picked its own place. Settings tables could be edited in five places, what a tool
works on was built again in each panel, and "case" meant two things (0066's layer copies, 0105's tables).

Tim's steer the same day: "I feel we need a cleaner separation between the design work and the table work - the table
work would end up at the schedule anyway ... The design outputs need to become the inputs into the dataflow but we can
separate them out." Then: "The objects table would then have the ability to run Starlark formulas (with clear workflow
steps available); this would ensure the design outputs get into the correct format for a dataflow import." And: "I
prefer a white background and feel", with dark kept as an option.

The review and a clickable prototype of the two sides were put to him before the plan (Tim: "what you are showing is
better").

## Decision

Settled in Tim's words before the plan's decisions:

- **A.** Two sides, Design and Tables, each with its own layout.
- **B.** Design scenarios (face options, layers, 0066's workflow cases) and table cases stay separate. There is no
  single scenario from the toe to the schedule.
- **C.** Design outputs feed a dataflow's Imported values; a case's run gives the schedule quantities (its Scheduled
  folder), which the scheduler maps to (0100).
- **D.** The Objects table runs Starlark formulas as clear steps, to shape design outputs for a dataflow.
- **E.** Light by default, dark as an option.
- **F.** The prototype's direction.

The ten decisions, as agreed:

1. **What goes on each side.** Design: Strings, Walls, Surfaces, Solids and Geology, with the layer list, face
   designs, seam models and workflows, and the Objects table and Run log under the view; Interrogate and Check by eye
   move into Geology. Tables: inputs, cases, dataflows, parameters, results and schedule; Compare with Spry or Deswik
   moves to Results. The Objects table shows on both sides.
2. **A case reads the design live.** A design change marks the inputs that read it, and the cases using them, out of
   date with why. Run clears it. Each run already records the revisions it read.
3. **A prepared input is a named object a case picks**: one recipe, copied for each design scenario by changing its
   take. It changes the object model, so workstream 5 starts with its own plan.
4. **An input's steps are worked out when read**, as calculated columns are (0103), never written onto the solids.
   Nothing on the Tables side writes into the design.
5. **The kinds of step:** take (layers and a filter), words, formula (Starlark, every number from a parameter row),
   check (stops a row, never the input), output (the fields the dataflow reads, checked as ready before a run).
6. **One "works on" for every tool:** Picked, Its layer or On show, plus a filter, at the foot of the view.
7. **One home for workflows:** the Workflow panel and the canvas become one, a document over the view.
8. **No new panels or tabs until the Tables side lands** (workstream 4). Engine work and fixes carry on.
9. **Now, before scheduler stage A**, so the scheduler is built into the Tables side.
10. **The map of places and the house rules go into `.claude/rules/app-ui.md`.**

Workstreams, a PR each, in order: 1 the map and the light look; 2 the two sides (the switch, a layout for each side,
the ribbon on Design only in five groups, the project menu); 3 the Design side; 4 the Tables side; 5 prepared inputs,
after their own plan; 6 the kit everywhere.

Not in this plan: one scenario from the toe to the schedule (declined, B); the scheduler itself; sort order and named
hierarchies; floating windows; engine changes outside workstream 5.

## Built: workstream 1, the map and the light look

- **Light is the default** (`app/src/app.css`). The tokens on `:root` are light: white panels on a faint grey, grey
  hairlines, the gold kept for what's active or picked but deeper (`oklch(0.6 0.13 75)`) so it reads on white, and the
  same one meaning each: orange warning, blue edited, red refused, green valid. The dark tokens are as they were,
  under `.dark`. Two tokens are new in both looks: `--strip` (the tab strips) and `--faint-foreground`, and
  `--view-background` for the view's ground while it starts.
- **Dark is a setting saved on this machine** (`app/src/viewport/look.ts`), like the view controls, under **Look** in
  View controls. `index.html` no longer fixes `dark`; the look is put on the root before the app draws. A change
  takes at once: the panels by the class, dockview's frame (`themeLight` or `themeDark`, our colours on both), the
  workflow canvas, and the view.
- **The view's colours come from the look** (`view` in `look.ts`), read when drawn: the ground, a line with no colour
  of its own (dark on light), what's picked, a string being drawn, the section guide, the section's grid, and the
  five snap marks. A change of look recolours the ground, the picks, handles, previews, guide and plain lines as they
  stand (`Viewport.applyLook`, `StringLayer.applyLook`).
- **The map of places and five house rules** are in `.claude/rules/app-ui.md`: where each part of the app lives on
  each side, the concepts with one home each, and the rules that keep a new feature to its place.
- **Tests** (`app/tests/look.test.mjs`): the app starts light with nothing saved; both looks give every colour the
  view draws; on each look's ground, ink reads at 7:1 or more, and the picked gold, a string being drawn and each
  snap mark at 2.5:1 or more (WCAG contrast); a change of look carries to the view and to whoever listens, and back.
  `pnpm test` passes 193; `pnpm typecheck` is clean.

## Built: workstream 2, the two sides

- **The Design or Tables switch** on the ribbon's top row (`SideSwitch` in `Ribbon.tsx`), remembered a project on this
  machine like the group (`sideOf`, `rememberSide` in `workspace/stage.ts`, `zd.side.v1`).
- **Each side its own layout** (`showSide` in `Workspace.tsx`). The Tables side's panels (Reserves case, Parameters, and
  the Tables panel while on Tables) are a group of their own, which fills the window on Tables and is hidden on Design;
  Design's docks round the view stay as they were left, their sizes held across a switch. A layout saved before the
  two sides has its Tables panels gathered into their group the first time it opens. ` on Tables shows the view in the
  window and brings the tables back; Reset layout goes back to Design.
- **A panel names its side** (`side` in `PANELS`: `tables`, `both`, Design by default). Asking for one on the other side,
  from a row, Find a command or another panel, switches side. The Tables panel (`both`) moves to the side that asks:
  the Objects table under the view on Design, reserves tables on Tables.
- **Five Design groups** (Strings · Walls · Surfaces · Solids · Geology), each tool on one row (`ROWS`): Set grade,
  Offset and Project only on Strings, the surface tools on their own Surfaces row, Interrogate and Compare blocks on
  Geology. Move, Rotate, Mirror and Delete (`EDIT_ROW`) are at the end of every Design row. The Tables side has one row
  (Reserves case, Tables, Parameters) until its list comes in workstream 4.
- **The project menu** (☰, `PROJECT_MENU`): New, Open (a project or an older folder), Save as, Import, Export, Licence
  and View controls. The Project and Reserves stages are gone; the project's name is a label with its path in its tip.
- **Find a command** lists Project, the five groups, Tables and Always there.

| Check | Result |
|---|---|
| `pnpm test` | 194 pass: each tool on one row, the rows' names, the project menu, a panel's side and its tool's agree, the group and the side remembered (a stage there no longer is opens on Strings) |
| `--shell` on the release build from this branch, the 16 Nov demo | 61 of 61: the docks as before, Design's tables under the view and the Tables side's in their own group hidden on Design, the project menu and its Open, Import and Export, the Solids row's menus found in More where the row folds them, Find from a group that stays, every panel's first control within 40 px with each panel shown on its side |
| `--case` on the same | 51 of 53, as on main: the two left are the demo's (a "Check haulage" dataflow and a second "Reserves assumptions" already in the 16 Nov demo, saved by an earlier run) |

The shell check's stale menu list (no "A table (CSV)…", "Dataflow" for the Reserves case) is put right with it.

## Built: workstream 3, the Design side

- **One tool slot** (`dock: 'tools'`, `inSlot` in `Workspace.tsx`): a tool asked for closes the one before it, and opens
  over Properties, which has a group of its own under it (`dock: 'inspector'`). The slot's tab is named for the tool
  in it (By seams, Offset), not the panel that holds it. A layout saved before is put right as it opens: the tool in
  front kept, the others closed, Properties under it.
- **No tool list inside a tool**: the Tool lists of Strings, Strings from surfaces, Surfaces and Move and copy, and the
  What to do choices of Cut solids and Reserves, are gone. The ribbon picks the tool; the panel's head says what it
  does.
- **What a tool works on**, at the foot of the view (`WorksOnBar`, `panels/worksOn.ts`): Picked, Its layer or On show,
  with how many solids each takes, shown while Cut solids or Interrogate is in front. Both read it and say what it
  takes where their own choice was.
- **Documents over the view** (`dock: 'document'`): Face design, and the workflow, which is now one document: its canvas
  with the workflow, its cases, its steps, its report and its run beside it (`WorkflowDocument`). The Workflow canvas
  panel and its tool are gone (a saved layout's canvas tab closes as it opens), and so are the pickers the two had each.
  Closing a document puts the view back.
- **The Objects table and the Run log under the view** came with workstream 2; Interrogate is on Geology since then.

| Check | Result |
|---|---|
| `pnpm test` | 198 pass, 4 new (`tests/worksOn.test.mjs`): what Picked, Its layer and On show each take, with their counts, and the tools that read it |
| `--shell`, release build from this branch, 16 Nov demo | 61 of 61: a tool in the slot over Properties, named for the tool; one tool at a time; Face design and the workflow over the view; a minimised slot back with the tool asked for; the string tools picked from the row; Cut solids with no choice of its own and the bar at the foot of the view; every panel's first control or line within 40 px of its tab |
| `--workflow` | 38 of 38 |
| `--select` | 40 of 40 |
| `--case` | 51 of 53, as on main (the demo's own two) |

## Known limits

- **The section's labels and the overview** keep their dark plate with white words in both looks, so they read over
  any RL colour.
- **Colours of their own stay as they are**: layer colours, the RL palettes, the ramp colours, and the section's coal
  and waste fill.
- **What's picked is a deep amber on light** (`0xc28400`), the gold made darker so a 1 px line reads on the pale
  ground (2.7:1). It's still the picked colour; the panels' gold matches it.
- **A section cut before the change** keeps its line colours until it's cut again.
- **The Tables side has a row, not yet its list** (workstream 4), and Compare blocks stays on Geology with the Reserves
  panel until results have their own place.
- **On a narrow window the Design rows fold sooner**: Move, Rotate, Mirror and Delete take room at each row's end.
- **The app benches need `bench/out` to exist**: in a fresh worktree a bench ran to its end and then waited, never
  writing its results. Each bench run here used its own WebView2 folder (`WEBVIEW2_USER_DATA_FOLDER`), as one that
  waited held the usual one.
- **The string tools still work on what's picked**: Its layer and On show are for the tools that take solids. A string
  tool that reads the bar is for a plan of its own, as the commands would take more than they do today.
- **The workflow's canvas is narrow** at 1,600 px with the tool list beside it open: close the list (its button at the
  top) for the room.
- **A tool's typed settings go with it** when another takes the slot: the next time it opens, it starts as it does.
