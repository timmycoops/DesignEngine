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

### Workstream 2: the working area (decisions 6 and 8)

- **Three columns to the foot of the dock** (`ReservesCasePanel.tsx`, `data-slot="case-columns"`): the step, the units
  table and the block map side by side, the units table the widest, each scrolling in its own column; the fixed 28 rem
  goes. The units table is moved up from the foot into its column (`case-units`), its toolbar staying over the rows as
  they scroll; with no block picked the column says to pick one.
- **The settings by seam and ply its own width**, so a value sits next to its row.
- **The block map's cells grow to fit** (`mapColumns`, `reservesCase.ts`): a column a block of the longest strip, so the
  strips line up; past 40 blocks a strip they stay small and wrap. Its own scroll box goes: the column scrolls.
- **The flow says its losses in full**: each kind named (`582,418 t loss, highashloss`), the line wrapping, no "…".
- **Narrow docks** (`caseFold`, the tab's own width watched): from 1,400 px three columns; under it the block map under
  the step with the units table beside both; under 1,000 px the units table under both, the tab scrolling, and the
  header's figures left to the case pill's tip.
- **Checked:** `pnpm test` for the folds at 2,000, 1,400, 1,399, 1,000, 999 and 0 px and the map's columns (7, 40, past
  40, none). In the app (`zd-app --bench --case`, the workflow demo, the tab given the 1,600 × 1,000 window, 1,600 × 878):
  56 of 56, the four new ones the three layouts by where each part sits (the columns' tops level and their feet at the
  tab's foot when wide), nothing on the flow cut off at 1,600 and 1,399 px, and 28 unit rows in sight against the plan's
  10; pictures at 1,600, 1,399 and 999 px.

## Known limits

- A case pill's tip is the browser's own: it shows after a pause, and not on touch.
- The folds go by the tab's width, not the window's: a dock made narrow by the layout folds as a narrow window would.
- Under 1,000 px the parts are under each other and the tab scrolls; the units table is then as tall as its rows.
- Not tried by hand.
