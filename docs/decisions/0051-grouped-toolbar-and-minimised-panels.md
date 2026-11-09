# 0051 — A grouped toolbar, and panels minimised to the window's edges

Date: 2026-11-09 · Status: proposed (Tim, 2026-11-09, picked A of two layouts: "Lets go with a"). Before the UI plan's
workflow tabs (the Build Plan's UI plan tab, decision 1, still open), which this doesn't rule out.

## Why

Tim, 2026-11-09: "I want to be able to minimize the panels so they allow more screen size easier. Also the top buttons
are pretty messy we have enough now to sort better and put them into groups I would think ?" The toolbar had grown to
about thirty buttons and menus in one row, seven of them imports, and the side panels could only be dragged narrower or
closed.

## Decision

1. **The toolbar is in groups**, left to right, with a divider between each:
   - **Project:** its name opens a menu with New, Open, Save and Save as. Save, Undo and Redo stay one click away.
   - **Data:** **Import ▾** holds surfaces, Vulcan grids, strings, solids (a submenu: zip or files, folder, and each
     repaired as it comes in), Deswik drawings, Spry projects, and the seam model's two steps. **Export ▾** is as before.
   - **Strings:** Select, Draw string, Snap.
   - **View:** 3D or Plan, Zoom to selection, Zoom to everything, the field of view, and Section, with the section's step
     and fill beside it while one is open.
   - **Design:** Face, Builder, Cut.
   - **Panels ▾** sits on the right.
   Every tip names its key.
2. **A group of panels minimises to a strip at the window's edge.** The – button in its header shrinks it to its panels'
   names, on the side of the viewport it was on (left, right, top or bottom), and the viewport takes the room. A click on
   a name brings the group back where it was, at its size, with that panel in front. Opening a minimised panel from the
   toolbar or the Panels menu brings it back too. The viewport's own group has no button.
3. **The layout keeps it.** A minimised group is a hidden group in the saved layout, and the edge it went to is saved
   beside it, so the app opens as it was left. Reset layout brings every group back.
4. **` hides every panel**, so the viewport fills the window, and brings them back. The strips hide with them. A
   double-click on the Viewport tab, or Panels ▸ Hide every panel, does the same.
5. **Built on dockview's free core.** Its VS Code-style auto-hide edge groups are in dockview-enterprise, a paid add-on
   outside the locked stack. Hiding a group in place (`group.api.setVisible`) is in the core, and a hidden group keeps its
   slot and size.

## Evidence

Release app, `zd-app --bench --shell` (2026-11-09), through clicks on the buttons and key events, from the default
layout, with the layout saved on the machine put back after. 10 of 10 checks pass:

| Check | Result |
|---|---|
| Minimise Properties | viewport 1,000 → 1,297 px wide; "Properties" on the right strip |
| Minimise Layers and the Run log | on the left and bottom strips; viewport 1,544 × 881 px |
| The layout saved and read again | all three still minimised, on the same strips |
| Click "Properties" on its strip | back at its width: 298 px given back, 297 px it took |
| ` | the viewport fills the window, 1,600 × 909 px; no strips |
| ` again | back, 1,252 px against 1,246 before (see Known limits) |
| The toolbar | Import, Export and Panels menus; no import buttons of their own |
| The Import menu | all eight imports and seam model steps in it |

`zd-app --bench --select` still passes all 25 checks with the new toolbar.

## Known limits

- **A few pixels move** when every panel comes back after `: the strips hide with the panels, and dockview shares their
  room out again on the way back (6 px on the viewport in the bench).
- **No sliding out.** A minimised group comes back into the layout; it doesn't slide over the viewport and away again,
  as auto-hide panels in VS Code do (that's the paid add-on).
- **A floating group has no – button**, as it isn't in the layout to leave a slot.
- **One row still.** The workflow tabs of the UI plan (Project · Strings · Walls · Solids · Geology · Reserves) wait on
  its decisions.
