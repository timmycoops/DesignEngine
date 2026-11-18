# 0089 — Lists drop in the app's own look

Date: 2026-11-19 · Status: accepted (Tim, 2026-11-19: "I'm happy with apps own menus to fix the drop down also")

## Why

Tim, 19 November, of the Strings panel's Tool box: "I don't like how these panels show up — this is in a lot of places."
Every choice box was a plain `<select>`, and its choices opened in the system's list: white, the words pale grey, as if
every one were greyed out, against the dark panels. Of the filter's suggestions: "similar issues that hides — needs to be
cleaner to see and know it's on". Their list was the panels' own grey, edged at 10%, so it sank into what it lay over,
and a filter that's on looked the same as one that isn't.

## Decision

1. **A select's choices open in the app's own list.** The kit's `Select` keeps its `<select>` and `<option>`s, so every
   panel sets, reads and changes it as before, but the system's list never opens: a click, or Enter, Space, F4 or Alt+↓,
   opens the app's. The choice it holds is ticked in gold, the one under the mouse or the arrow keys is lit, groups
   (`<optgroup>`) are headed, a choice that can't be taken is pale. ↑ ↓, Home and End and a letter move; Enter, Space,
   Tab or a click picks; Esc closes it (decision 0080: the press is the list's). It opens below the box, or above where
   there's no room, over the docks, and over a dialog without closing it; a dock scrolled closes it.
2. **No select is hand-rolled.** The fifteen in the Face design panel and the one in the Deswik import are the kit's.
3. **Every list that drops is one look** (`MENU_SURFACE`, `MENU_LIT` in the kit): the menus, the right-click menus, a
   select's choices and the filter's suggestions sit lighter than the panels (`--popover` 0.25 to 0.29), edged at 20% and
   shadowed, and the row lit is plainer (`--accent` 0.30 to 0.37).
4. **A filter that's on shows it**: its box edged in gold, its funnel lit, and the count of what it keeps (a layer's "N of
   M showing", the Reserves table's leaves) in gold.

## Checked

| Test | Known answer |
|---|---|
| `zd-app --bench --workflow` | the Workflow panel's Case select: the mousedown that would open the system's list is stopped; the app's list holds Base and 60, 60 ticked; a click on Base picks it and closes the list; Enter opens it, ↓ and Enter pick 60; Esc closes it |
| every app bench | the selects they set by value and change still set and change |

## Known limits

- **A select's own box still draws the system's arrow**, in the dark box as before.
- **Typing several letters** doesn't spell a choice out: each letter goes to the next choice starting with it.
