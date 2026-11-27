# 0109 — Both sides at once, and the Tables side popped out: the table work and the design seen together, beside the view or in a window of its own. Agreed by Tim 2026-11-28.

Date: 2026-11-28 · Status: accepted (Tim, 2026-11-28: "Build both", on the plan put to him in the session after #303)

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

## Built: Both

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

## Known limits

- **On a 1,600 px window Both is tight**: with Layers and Properties open, the view and the Tables panel get about 460 px
  each. Minimise Layers or Properties to their strips for room, or pop the Tables side out (decision 3) once it lands.
- **With the list folded**, a section's number opens its first item, or the one it showed last; another case is picked
  on the top row, but another parameter table or dataflow needs the list open (wider).
- **The Tables panel shows no tab in Both**, as on Tables: it's dragged by its edge, not its tab.
- **Picks go one way from the case:** a solid picked in the view doesn't pick its block on the case's map.
