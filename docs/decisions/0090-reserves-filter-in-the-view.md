# 0090 — The Reserves table's filter in the view

Date: 2026-11-19 · Status: accepted (Tim, 2026-11-19: "Build all three plus the coal tick")

## Why

Tim, 19 November, with the Reserves table filtered to `Imported\Coal\as: >0`:

> When I filter the reserves table, we want to be able to make it easier to show the solids layer that is associated
> with that reserves table. Also, when we filter the table, should we be filtering solids also?

Before this, the table's filter only changed the table. Select its solids and Isolate its solids still took every solid of
the node, so what was picked didn't match the numbers shown. Nothing in the table said which layer held its solids.

## Decision

1. **The table names its solids' layer.** Under the table picker, "Solids on 9 blocks 50m" names the layer of every solid
   on a leaf the table holds.
   - **Show** turns that layer on and every other layer off.
   - Pressed again, it reads **Back** and puts the layers back as they were.
   - Show and Isolate its solids are one at a time: a press of either puts the other back first.
2. **Select and Isolate its solids obey the filter.** A node's solids are those of the table's case on a leaf the table
   holds, under the node, and on a leaf the filter keeps.
   - A solid on no leaf of the table is never one of a node's solids: a strip solid, a seam piece or a bench before the
     blocks. Before this, a node above the block took those too.
3. **Filter the view too**, a tick beside the leaves filter, carries the filter to the view.
   - The solids on the table's leaves that the filter leaves out aren't drawn, snapped to or cut by a section, as a
     layer's filter leaves them out (0088). They aren't hidden one by one, so Show all hidden doesn't bring them back.
     Unticking the box, or clearing the filter, does.
   - Ticking it also turns the table's layer on, so what the filter keeps is on show.
   - It follows the box as it's typed, a moment after the last change. It's worked out again when the table runs again,
     and it goes when the table is deleted.
   - It's saved with the project (`display.reserves`): one table at a time. A table whose filter is in the view picks the
     filter and its ticks up again when it's shown.
   - A layer's count of what's showing ("3,563 of 3,580") is gold whenever a filter leaves something on it out.
4. **Coal solids only**, ticked with Filter the view too, also leaves out the waste on the leaves kept, by the `material`
   the seam cut gave each piece (anything but `waste` is coal).
   - It's a leaf (a bench of a seam in a block) that a reserves filter keeps or leaves out, and as in Spry coal and waste
     share one. So `Insitu\Coal\volume: >0` keeps a bench with coal, its waste included, unless Coal solids only is ticked.
   - Coal that the working sections reject still shows, as its solid is coal.
5. **The reserves values aren't written onto the solids.** A copy of every field on thousands of solids would go stale
   as soon as the table ran again. The view reads the table.

The leaf helpers (`levels`, `leaf_of`, `case_of`, `rows_fitting`, and `TableLeaves`) moved from `zd-commands`'
`reserves_view` to `zd_core::leaves`, so a project works out what a reserves filter leaves out as it does a layer's.
`reserves_view` re-exports them.

## Checked

| Test | Known answer |
|---|---|
| `a_reserves_tables_filter_leaves_out_the_solids_on_the_leaves_it_does_not_keep` (zd-core) | four leaves; Coal volume >0 leaves out only the solid on the leaf with none; Coal solids only the waste on a leaf kept too, and on its own every bench's waste; another case's solid and a strip solid untouched; the table revised, worked out again; the table deleted, nothing out |
| `zd-app --bench --dataflow` (16 Nov demo) | the table names its solids' layer, Show puts exactly that layer alone and Back puts every tick back; with `seam: U`, Select its solids picks only B1's U solids; Filter the view too leaves out exactly the table's solids not on a U leaf, Coal solids only the waste on those too; unticked, none out |

## Known limits

- **One table's filter in the view at a time.** Ticking it on another table moves it there.
- **A leaf is the finest a reserves filter goes.** Within a leaf, only Coal solids only tells pieces apart.
