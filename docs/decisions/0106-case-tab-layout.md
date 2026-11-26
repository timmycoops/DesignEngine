# 0106 — Case tab layout plan: the Reserves case tab laid out again, each part said once, the room to what's used. All 8 decisions agreed by Tim 2026-11-26.

Date: 2026-11-26 · Status: accepted (Tim, 2026-11-26: "Agree all eight", on the Build Plan's "Case tab layout plan" tab)

## Why

Tim, 26 November, on the Reserves case tab with all seven workstreams of 0105 in, a picture of Reserves 50m at
`small_volumes` on a 2000 px screen: "We have added some great tools but this is still confusing to look at and there is
a lot of wasted space it feels."

Each workstream of 0105 added a band down the tab. Measured off his picture: the band, the cases, the input cards and
the flow took about a third of the height while using a quarter to two fifths of the width each; the open case was said
three times (the cases table, the in case list, 1 of 1), the steps twice (the flow's numbers and the list down the left),
the blocks twice; the flow's cards cut their own figures off; the three working columns were a fixed 28 rem, leaving the
units table about two rows; and three lines of help sat over the flow, the block map and the settings on every step.

## Decision

The app only: no change to the object model, the commands or the stack, no new dependency. Every part of 0105 keeps a
place.

1. **The cases as pills in the header.** The cases table under the band goes: each case a pill, Base marked, the open one
   lit, its four figures (in situ coal, ROM coal, ROM waste, strip ratio, each against Base) in its tip; the open case's
   four at the header's right.
2. **One +** holds New case, Copy case, Add a dataflow and Every case's figures (the old cases table, on demand).
3. **The case's dataflows as a chain in the header**, in run order, the reserves one named with its add-ons, the open one
   lit, one out of date marked on its link with why in its tip; a click opens it. The in case list and 1 of 1 go. Run
   holds the case's dataflows with Run each, Run what's out of date and Run all. Setup stays.
4. **The input cards fold into the step.** Its Reads line names the settings tables it reads, lit gold (blue when run as
   edited), a click opening the table in the Parameters tab; the blocks, seam model and steps are said once, in the open
   case pill's tip.
5. **The flow is the step list.** It spans the width, the stages sharing it, each naming its steps; a click or ↑ ↓ moves
   a step, a step that ran again on the last edit shows its time. The list down the left goes; Built-in or Formula is in
   the step's title and the step's tip.
6. **Step, units table and block map side by side, filling the height** (workstream 2).
7. **Help as tips.** The lines over the flow, the block map and the settings become tips on their titles; the head line
   stays one line (the UI rules). "Balanced" is the badge, its meaning in its tip.
8. **Narrow docks** fold the columns under each other below 1,400 and 1,000 px (workstream 2).

## Built

### Workstream 1: the header and the flow (decisions 1 to 5 and 7)

- **The header** (`ReservesCasePanel.tsx`, `data-slot="case-band"`): the case pills and + (`CasePills`, `CasesBar.tsx`,
  the forms and every case's figures under the header in `CaseForms`, the figures shared through `useCases`), a rule,
  the dataflow chain (`DataflowChain`, `CaseSwitcher.tsx`), Run (`CaseSwitcher`, its Open buttons gone to the chain) and
  Setup; at the right the open case's figures (`CaseFigures`), stopped block columns when any, and the balance badge.
- **The flow as the step list** (`CaseFlow.tsx`): a grid of equal columns across the width; each stage's steps named
  (`as read` for the blocks), the open one lit, with its time after an edit; the stage's tip says how to read it. The steps
  column is gone from the three columns (`case-columns` is now step | blocks).
- **Reads** names the settings tables the step reads (`case-reads-table`), in place of the input cards.
- **Tips:** the flow's, the block map's and the settings' lines.
- **Checked** in the app (`zd-app --bench --case`, the workflow demo): the 0105 checks moved to the new places, none
  dropped: the case picked by its pill, steps found in the flow, the lit settings table on Reads, Copy case, Add a
  dataflow and New case from +, a row dataflow opened from the chain, figures read off the pills. The `--dataflow` bench's
  case pick and out-of-date warning moved too.

## Known limits

- The three columns are still a fixed 28 rem and the units table sits under them until workstream 2.
- A case pill's tip is the browser's own: it shows after a pause, and not on touch.
