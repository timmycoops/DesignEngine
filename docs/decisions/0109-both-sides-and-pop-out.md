# 0109 — Both sides at once, and the Tables side popped out: the table work and the design seen together, beside the view or in a window of its own. Agreed by Tim 2026-11-28.

Date: 2026-11-28 · Status: accepted (Tim, 2026-11-28: "Build both", on the plan put to him in the session after #303); decision 1, Both, taken out the same day (Tim: "I don't think both make sense when we can dock as we need"), the pop-out kept

## Why

Tim, 28 November, the morning the Tables side (0108, workstream 4) merged: "I need to be able to see the table and the
design at the same time - we should be able to pop the whole table design out as well I think."

0108 gave each side its own layout and a switch between them, so the case and the view it describes were never on the
screen together. "Show in 3D" took you across to Design and left the table behind. 0108 listed floating windows and a
second monitor as "later, if the two sides want it"; they do.

A trial build on 28 November found that the app can't open a second window as it stands: the desktop shell turns
`window.open` away, so dockview's pop-out gets nothing. Tauri 2.12 lets a window take one (`on_new_window`), if the main
window is made in code.

## Decision

1. **Both on one screen.** The switch has three choices: Design, Both, Tables. Both is Design's layout with the Tables
   panel beside the view, on its right, between the view and the tool slot, half the room the two share and dragged
   wider or narrower as wanted. The ribbon shows Design's rows and the Tables side's case and Run together. A panel of
   either side opens where it lives without a switch. It's remembered for each project like the side.
2. **A pick on one shows on the other.** With both shown, a block picked on the case's map is picked in the view, and
   Show in 3D frames it there without leaving the case. Solids picked in the view light their rows in the Objects
   table, as they did.
3. **The Tables side pops out** into a window of its own, for a second monitor: one running app, so picks, edits and
   Run stay shared; closing the window puts the panel back. It needs the main window made in code with a new-window
   handler, and every menu and list to open in the window it was asked from. It comes after a trial end to end
   (menus, Esc, Ctrl+K, undo in the window popped out), in its own PR.

The map of places in `.claude/rules/app-ui.md` gains both.

## Built: Both (taken out the same day, below)

- **The switch** (`SideSwitch` in `Ribbon.tsx`): Design · Both · Tables. What the window shows is `Showing` in the tool
  list (`Side` or `both`), kept for each project in `zd.side.v1`.
- **The layout** (`showSide` in `Workspace.tsx`): in Both the Tables side's group is shown, not filling the window, and
  put beside the view on its right where it isn't (`besideView`), at half the view's room; Design's docks are held at
  the sizes they had. From Both to Design the group is hidden and the view takes its room back; to Tables it fills the
  window. ` in Both gives the view the window and brings both back.
- **The list folds by itself** when the Tables panel is narrower than 900 px, as it is beside the view: the six
  numbers down its edge, the item the room. The case is picked on the top row.
- **Shared picks** (`ReservesCasePanel`): with both shown (`showingNow` in `tablesSide.ts`), the block picked on the map
  has its solids picked in the view; Show in 3D frames them and stays.

| Check | Result |
|---|---|
| `pnpm test` | 204 pass: Both kept for a project like a side; what the window shows told to the Tables side |
| `--shell`, release build from this branch | 64 of 64: Both puts the Tables side beside the view on its right, the view kept, the tool slot after them, Design's rows and the case's Run on the ribbon; back to Design the view has its room again |
| `--case`, 16 Nov demo | 53 of 55: with both shown, a block picked on the case's map (Pit 2 B3) is picked in the view, 18 solids. The two left fail on main too, from the used demo |
| `--workflow` | 38 of 38 |
| `--select` | 40 of 40 |

## Built: the pop-out

- **The main window is made in code** (`setup` in `app/src-tauri/src/main.rs`, `create: false` on it in
  `tauri.conf.json`), from the same config, with a new-window handler: a window the page asks for is made as one of the
  app's own (`popout-<n>`, "Zovlyn Pit Design: Tables"), so the page it opens (`public/popout.html`) is served as the
  main window's is. Only the main window asks before closing with work unsaved. A trial found a plain popup opens but
  can't load the app's page, so dockview's pop-out never finished.
- **Pop out** (the button at the head of the Tables side's list, `popOutTables` in `Workspace.tsx`): dockview's pop-out
  moves the Tables panel into that window, at 70% of the main window's size; the main window goes to Design with the
  case and Run kept on its ribbon. The empty group dockview leaves behind is hidden, so the view has its room.
- **Back:** the same button, the window's X, or Tables or Both on the switch close the window; dockview puts the panel
  back and the side as it stands is put on the layout (`backFromWindow`); the main window takes the focus again
  (`core:window:allow-set-focus`). The app window it was in is closed too (`closePoppedWindows`,
  `core:window:allow-destroy`): closing its page, as the switch and its button do, left the window standing empty and
  black (Tim, 28 November); the shell bench now counts the windows left after each way back (none, where 1 and then 2
  were left without the fix).
- **Menus, lists and tips open in the window they're used in**: the kit's Select and FilterInput draw into their box's
  own page, and the dropdown, right-click and tooltip wrappers into the page `PortalRoot` gives
  (`design-system/portalRoot.ts`), which the Tables side sets while it's out.
- **Keys pressed in its window reach the app's shortcuts** (`passKeys`): Find a command, Save, Undo and Redo, Esc,
  Delete; typing in a box keeps its keys, but for Save and Find a command.
- **The case's picks reach the view while it's out**, as with both shown (`viewBeside` in `tablesSide.ts`: both shown,
  or popped out): a block picked on the case's map in its window has its solids picked in the view in the main window,
  and Show in 3D frames them there without leaving the case.

| Check | Result |
|---|---|
| `--shell`, release build from this branch | 68 of 68, 4 new: Pop out from Tables puts the panel in a window of its own with the main window on Design, its view at its full width and the case's Run on its ribbon; a click there works and Ctrl+K pressed there opens Find a command; its button brings it back, away on Design; Tables asked for while it's out brings it back filling the window |
| `--case`, every check counted | 68 of 71, 1 new: popped out, Pit 3 B5 picked on the case's map in its own window picks its 20 solids in the view in the main window. The 3 left fail the same on main (66 of 69): Open another dataflow, then both up to date, and New case, all from the used demo. Earlier counts (53 of 55) left out the 15 checks that carry no detail |
| `--workflow` | 38 of 38 |
| `--select` | 40 of 40 |
| `cargo clippy -p zd-app --all-targets -- -D warnings` | clean |

## Both taken out, the pop-out from Design (28 November)

Tim, having used the two: "I don't think we need the both - let's allow the tables to dock out but by the design tab
and it will be clean that you can then just add it back to the main screen. I don't think both make sense when we can
dock as we need."

- **The switch reads Design · Tables** again (`SideSwitch`); `Showing` is gone, the window shows a `Side`. A project
  remembered on Both opens on Design.
- **The pop-out from Design:** a button beside the switch (`PopTables` in `Ribbon.tsx`, `ribbon-pop`) pops the Tables
  side out into its own window from either side; the main window stays on Design with the view at its width, and the
  case and Run on its ribbon. Its list's head keeps its button too.
- **Back into the main window:** by either button or Tables on the switch, the Tables side comes back on Tables,
  filling the window (`popBackTables`, `backToTables`). Closed by its own X, it goes back and away, the main window
  left on Design.
- **Picks:** the case's map picks in the view only while it's popped out (`viewBeside` is the pop-out alone).
- Gone with Both: `besideView`, the Tables panel's group put beside the view; the `both` side of a panel in `PANELS`,
  which none had.

| Check | Result |
|---|---|
| `pnpm test` | 204 pass: a pick goes to the view only popped out; Both remembered reads as Design |
| `--shell`, release build from this branch | 71 of 71, 4 new: the switch reads Design and Tables, the pop-out button beside it; from Design the ribbon pops the Tables side out, Design kept with the view at its width; the same button brings it back on Tables, filling the window; closed by its X it goes away, Design kept. Brought back by its list's button it's on Tables now, not away on Design |
| `--case` | 67 of 70: popped out, a block picked on the case's map in its own window picks its solids in the view; the Both check gone. The 3 left fail the same on main, from the used demo |
| `--workflow` | 54 of 54 |
| `--select` | 46 of 46 |

## Known limits

- **With the list folded**, a section's number opens its first item, or the one it showed last; another case is picked
  on the top row, but another parameter table or dataflow needs the list open (wider).
- **Picks go one way from the case:** a solid picked in the view doesn't pick its block on the case's map.
- **The popped-out window isn't remembered across a restart** beyond what the saved layout holds: dockview reopens it
  where it can; if it can't, the panel is back in the main window.
