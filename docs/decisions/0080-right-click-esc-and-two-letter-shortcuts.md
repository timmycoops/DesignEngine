# 0080 — Right-click to finish, Esc a step a press, and two-letter shortcuts

Status: agreed 17 November 2026 (Tim: "Yes, build it"; the menu he meant is a tool panel).

## Why

Tim, on how the software is actually used: he expects to start a string with left clicks and finish it with a right
click, to press Esc in a tool panel and see the selection go, and to start a new string from the keyboard (NS). These
habits come from Deswik and the other packages planners use every day, and they'll matter more as the app is used for
real work.

Before this, a string was finished with Enter or a double-click only. The right button belonged to the camera, where a
right drag pans and a right click with no drag did nothing. Esc did nothing while the cursor was in a box, which it
usually is just after a width or an RL is typed on a tool panel. When Esc did clear the selection, nothing said so.
Drawing could only be started from the pen on the view bar or from Find a command.

## Decision

1. **Left adds, right finishes**, wherever points are clicked in the view. A right click (the button let up within a
   few pixels of where it went down) finishes the string being drawn, as Enter or a double-click do. It also finishes
   a string a panel asks for (`pickString`), and it ends a measure: the reading stays until the next click starts
   another. A right drag still pans. With fewer than two points, the right click says a string needs two and keeps
   what's there. A right click in Select does nothing for now; that's where a menu on what's under the cursor would go.
2. **Esc takes back one thing a press, the nearest first, and says what it did:**
   1. A menu or dialog that's open closes (Radix marks the press handled, and that's the press).
   2. A pick a panel is waiting on is cancelled ("Pick cancelled").
   3. The points put down are dropped ("String abandoned", or the measure or section starts again).
   4. Draw, Measure or Section with nothing in hand goes back to Select (the status line says Select).
   5. The selection clears ("Nothing selected").

   In a box (a tool panel's number, a name, the typed-vertex entry) Esc leaves the box and carries on down the list,
   so the press is never lost to where the cursor was. A box that uses Esc for itself marks it handled and that's the
   press: renaming a layer or an object puts the old name back and doesn't go on to clear the selection.
3. **Two-letter shortcuts**: typed one letter after the other, as a tool's `shortcut` of two capitals in
   `app/src/tools.ts` (`NS` on Draw string). The first letter waits 1.5 s for the second and says so in the status
   bar ("N… then S Draw string"); the pair carries out the tool as a click on it would; Esc after the first letter lets
   it go and does nothing else; a pair that isn't a shortcut says so. They're listened for ahead of every other key
   handler (capture, on the window), so the second letter is never also its own shortcut (S, snap). The first letter is
   never a shortcut on its own, nor Z (held to turn), which `tests/tools.test.mjs` checks. Not while typing in a box.
   Find a command and the tips show them as they show S or T.

## Where

`app/src/viewport/strings.ts` (`rightClick`, `escape`), `app/src/workspace/chords.ts`, the `toSelect` action in
`App.tsx`, and the rename boxes in `LayersPanel.tsx` and `PropertiesPanel.tsx`.

## Checked

`tests/tools.test.mjs`: NS is Draw string, and no two-letter shortcut starts with a key that's a shortcut on its own.
The app's own check (`zd-app --bench --clicks`) drives it as Tim would. NS starts drawing and leaves snap as it was;
N then Esc lets go; three clicks and a right click make a three-vertex string and stay in Draw; a right drag makes
nothing; Esc abandons the points, then goes back to Select with the new string still selected, then clears the
selection and says so; Esc in a box on the Strings panel leaves the box and clears the selection; a right click ends a
measure with its reading kept, and the next click starts another.
