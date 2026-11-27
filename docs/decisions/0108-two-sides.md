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

## Known limits

- **The section's labels and the overview** keep their dark plate with white words in both looks, so they read over
  any RL colour.
- **Colours of their own stay as they are**: layer colours, the RL palettes, the ramp colours, and the section's coal
  and waste fill.
- **What's picked is a deep amber on light** (`0xc28400`), the gold made darker so a 1 px line reads on the pale
  ground (2.7:1). It's still the picked colour; the panels' gold matches it.
- **A section cut before the change** keeps its line colours until it's cut again.
