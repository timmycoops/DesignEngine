# 0087 — The layer tree is layers only

Date: 2026-11-18 · Status: accepted (Tim, 2026-11-18: L1 to L6 of the Filtering plan) · Replaces 0026's decisions 1, 7, 8
and 16 (object rows, rows for what comes in numbers, the pinned layer row, the tree following a pick)

## Why

Tim, looking at `2 faces → Strip 1 → Strip 1 Highwall crest 1, 2, 3` in the tree: "I'm confused, each one is one
string, there are three layers". A layer, a group of a strip or a kind, and one object all drew as the same row with the
same tick, so the tree read as layers within layers. Asked which way to go, he picked layers only, Deswik's way, over a
row a kind under each layer or keeping object rows drawn differently. Then: "Designs are just layers though, aren't
they? Do we even need to see the dataflow, parameter or reserve tables in the main layer tree?" No: each already has a
list in its own tab. This is workstream 0 of the Filtering plan (Build Plan, "Filtering plan" tab), done before the
filter itself, which works on these rows.

## Decision

1. **A row a layer (L1).** Layers are folders by name as before. A layer's row says what it holds by kind, in a fixed
   order (`60 strings · 12 surfaces · 1 solid`; quality grids and seam models by name), `empty` when nothing is on it,
   and when some are hidden how many show (`3 of 72`). No row for an object, a strip or a kind.
2. **One object is hidden from the view (L2).** A right click in Select (the button let up where it went down; a right
   drag still pans, and in Draw or Measure a right click still finishes, 0080) opens a menu on what's picked; on
   something not picked, it picks it alone first, as a left click would. The menu: Zoom to, Hide, Show only this (or
   these) on its layer, Colour, Opacity, Move to layer and Delete, which were the tree's object menu, and Show all hidden,
   on the layers showing. A layer's right-click adds Show all on it. Hidden stays display state, saved and not undone
   (0026); a move or a delete is a run, one undo.
3. **A pick lights its layer (L3).** The layer's row says `1 picked` in gold and scrolls into view; the tree no longer
   opens to the object, as there's no row to open to. Properties shows the object as before.
4. **What isn't drawn leaves the tree (L4).** Face designs, workflows, parameter tables, dataflows and reserves tables
   are found only in the list their own tab or panel already has: the Face design panel, the Workflow panel, the
   Parameters tab and the Reserves table tab. Each list gains a `⋯` beside it (the kit's `ObjectMenu`): Show in
   Properties, which picks the object for its revisions and history, Rename and Delete. Picking from the list alone
   doesn't pick it, as that would drop the solids picked, which those tabs follow to light their cells and blocks. A
   layer holding only such things has no row; an empty layer keeps its row, as the next string may be drawn on it. In the
   engine every object keeps its layer, so old projects open as they are and nothing in the object model changes.
   Quality grids are counted on their layer's row and listed in the Objects table; a seam model's own grids stay in
   their files and show in its Properties, as before.
5. **The Objects table (L5).** The Block table becomes the Objects table: solids by default as before, or strings,
   surfaces, grids or all of them (Of), with name, layer, attributes and, for solids, the volume of record; the words
   filter, totals and CSV as before. Zoom to and Hide shown join Select shown and Delete shown. It's where an object is
   found by name now the tree has no row for it.
6. **Strips (L6).** The `Strip N` groups go. A strip is shown by the filter (`strip: 1`) once the filter's workstreams
   are built, and meanwhile by the Objects table's words.

## Where

- `app/src/panels/layerTree.ts`: `IN_TREE`, `kindsOf`, `layerEntries`, with `app/tests/layerTree.test.mjs`.
- `app/src/panels/LayersPanel.tsx`: the rows, the layer's right-click.
- `app/src/panels/ViewMenu.tsx`, opened from `StringLayer.rightClick` in Select through App's `menu` action and the
  workspace's `viewMenu`.
- `app/src/design-system/kit/ObjectMenu.tsx`, beside the lists in `FaceDesignPanel`, `WorkflowPanel`, `ParametersPanel`
  and `ReservesTablePanel`.
- `app/src/panels/BlockTablePanel.tsx`: the Objects table (the panel's id stays `blocks`).

## Checked

| Check | What it shows |
|---|---|
| `layerTree.test.mjs` | 60 strings, 12 surfaces and a solid read `60 strings · 12 surfaces · 1 solid`; grids, quality grids and a seam model each named; a layer of only a parameter table, a dataflow, or face designs and workflows has no row, an empty one has; a layer's hidden and picked counts; a table on a layer of solids isn't counted on its row, but its Delete takes it |
| `zd-app --bench --string-edits` | the tree has a row a layer and none for a string, the toes' row reading `4 strings`; a right click on the dog-leg in the view picks it and opens its menu, Hide hides it and its layer reads `3 of 4`; a right click on nothing offers `Show all hidden (1)`, which brings it back |
| `zd-app --bench --dataflow` | a parameter table, a dataflow and a reserves table, each picked in its own tab's list and shown by Show in Properties on its `⋯`, show in Properties as before |
| `zd-app --bench --solids` | its tree check is now the one row for the layer of solids saying how many (counts only); not run here, as it needs the golden pits |

## Known limits

- **Hiding one object needs the view.** It's picked and right-clicked there, or found in the Objects table and hidden
  from it. There's no list of what's hidden yet; Show all hidden and Show all on it bring everything back.
- **No Isolate yet.** Showing one layer alone is the Filtering plan's decision 9, not yet agreed.
