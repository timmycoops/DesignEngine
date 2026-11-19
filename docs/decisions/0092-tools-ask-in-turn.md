# 0092 — Tools ask for their inputs in turn

Date: 2026-11-20 · Status: accepted (Tim, 2026-11-20: "Agree, start ws0", the Surface tools plan tab, decision 7)

## Why

Tim, 20 November, on how Trim and the solid and surface tools should behave:

> You load the function up, trim in this case, it asks what to trim and then what to trim with (polygon). Other solids
> and surface functions have the same requirements, I assume.

Today it runs the other way. You pick first and press the button, and only then does Trim ask for the cutting string.
Each panel words its asking its own way, and a tool does one run and stops.

## Decision

1. **Opened, a tool asks what to work on.** The prompt line under the view says what: "Pick the strings to trim: a
   click, a box or Ctrl-click. Right-click when done; Esc stops." Picking works as ever. A right click, or the tool's
   button, says that's all, and there's no menu while it asks. If something fitting is already picked when the tool
   opens, it fills this step the first time, so picking first still works.
2. **Then it asks what to work with:** the closed string to clip to, or the string to trim by and the side to keep.
3. **It runs with the panel's settings as they stand when the last click lands,** so a setting changed while it waited
   counts. It's one run and one undo.
4. **It stays loaded and asks again** what to work on. What it made is picked, so a right click works on it next, and a
   click picks something else.
5. **Esc steps back one prompt a press.** At a click, it goes back to what to work on, with nothing changed. There, it
   stops asking, and what's picked stays picked. The button, or opening the tool again from the ribbon or Find a
   command, starts it again.
6. **It asks only while its panel is the one in front of its dock,** so another tool's right click is that tool's own.
   It doesn't ask while the Workflow panel is adding steps.
7. **The panel lists its steps.** The step it's waiting on is lit, done ones are ticked, and what fills the first step
   is named: "1 What to trim: 3 strings · 2 The string to trim by · 3 The side to keep". It's one kit part, so every
   tool reads alike.
8. **First Clip and Trim.** The other tools that take two inputs move onto it in the next workstream: Extend, Split at
   a click, the Cut solids modes, Between surfaces and Where surfaces meet. The Surface tools use it from the start.
   One-input tools only ever ask step 1.

## Where

- `app/src/design-system/kit/Asking.tsx`: `useAsking` (the loop, its `Asker`, `start`, `stop` and `next`) and
  `AskSteps`.
- `pickObjects`, `finishObjects` and `cancelPicks` in `app/src/viewport/strings.ts` and the workspace context: a right
  click finishes the gathering, Esc stops it.
- `panelsShown` in the workspace context.
- The Strings panel: `ASKS`, `askLoop`, and `settings`, which holds the settings as they stand.

## Checked

`zd-app --bench --string-edits`, 43 of 43:

- Trim with nothing picked asks "Pick the strings to trim". After a run, it asks again with step 1 lit.
- Esc at the cutting string's click goes back to what to trim with nothing changed, and a second Esc stops asking.
- Keep both, chosen while Trim was asking, splits all three toes.
- Clip opened with a toe picked goes straight to the closed string to clip to.

`--shell` 62 of 62 (its tooltip check passed on a second run; it fails whenever another window is in front), `--workflow`
54 of 54, `--select` 29 of 29, `--clicks` 24 of 24.

Known limits:

- Only Clip and Trim ask in turn so far.
- A click on nothing at "the string to trim by" is said, and it goes back to what to trim.
- The steps list shows the strings picked, not the string clicked, which runs at once.
