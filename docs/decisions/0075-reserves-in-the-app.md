# 0075 — Reserves in the app: the reserves table, fields as rows

Status: agreed 15 November 2026. Tim agreed the eight decisions on the Build Plan's "Reserves app plan" tab, with the
table turned so the fields run down the side ("I like the look of that better for what I want"). This is workstream 5 of
0070. Workstream 5a, the reserves table tab, is built here; 5b to 5e follow.

## Why

Tim, 15 November: "I want to see the break down of fields as rows instead of columns though - easier to see the break
down of the values. See the leave broken down by columns." Before this, a reserves table showed only as totals by seam in
Properties. Tim also asked to see a block change from Imported to Scheduled, with records at the key levels and their
summaries.

## Decision

The eight decisions as agreed, decision 2 as Tim turned it:

1. The reserves table is a dock tab under the view, a row a level, parents added up by each field's rule.
2. **Fields as rows**: the fields down the side by folder, the picked node's children across, a child opening to its
   own. Down a column is one node from Imported to Scheduled; across a row is a field broken down.
3. The dataflow opens on the workflow canvas, steps in stage bands, in their fixed order.
4. A step's panel: what it reads, the picked block's value and row, its settings, the block before and after.
5. Parameter tables and fields edited as grids in a Parameters tab.
6. Selection is shared by level: a node picks its solids, a picked solid finds its leaf.
7. Blocks can be coloured by any reserves field.
8. Run reserves from the dataflow's panel, with Add to workflow.

## Built: 5a, the reserves table tab

- **`reserves_view`** in `zd-commands`:
  - `tree`: the table's levels, its text columns outside any folder but `case`, as a tree. A seam keeps the stack's
    order, a bench goes top down by its RL, any other level in natural order (B2 before B10).
  - `breakdown`: a node's fields down the side, in the table's order, across its total then its children. A child in
    `open` is followed by its own children.
  - Every number is added up from the leaves by the column's own rule, `add_up_column`. The report now uses the same
    function.
  - `leaf_of` and `case_of`: where the run put a solid.
- **The table records its levels.** `reserves.run` writes a `levels` attribute on each table (`pit=Pit;strip;block;
  seam;bench`), so a block with no `pit` is matched to the leaf the run put it on, under Pit.
- **The app's Reserves table tab** (toolbar, beside Blocks):
  - a tree of the levels on the left, a picked node's breakdown on the right;
  - folders fold, remembered on the machine; main fields or every field;
  - a column header opens a child to its children, and a double-click shows that child on its own;
  - **Select its solids** picks the node's solids, of the table's case;
  - **Follow the pick**: a solid picked in the view shows its block with its seam opened, its leaf in gold;
  - CSV of the breakdown, exact values.
- **A floor loss into the underburden** is now ROM waste on its own leaf. It used to land on a leaf with no seam or
  bench: 90 such leaves on the demo pit's Base case, now none.

Known answers:

- `reserves_view` unit tests, a made-up table:
  - B10 breaks down into U (opened to benches 110 and 100) and L;
  - volume sums to 6,000 m³;
  - RD by volume is 1.55 for U;
  - ash by mass is (10 × 1,400 + 20 × 4,800) ÷ 6,200;
  - a field with no values is left out.
- `reserves_run`:
  - a loss into the underburden is ROM waste on B's leaf (25,000 + 1,000 + 1,000 m³);
  - every block of the loss column matches a leaf of its table under Pit.
- The app check (`zd-app --bench --dataflow`) on the demo pit:
  - block 3_B1's seams across;
  - its ROM coal 64,519.3 m³, with U 10,840.1, M 18,541.2 and L 35,137.9;
  - U opened to benches 130 to 100;
  - Select its solids picks its 21 solids, and a solid of 4_B2's M seam is found on its leaf.

The pit equals its strips added up to 1e-9. On the 50m case's 2,847 leaves, the tree takes 8 ms, the pit's breakdown
7 ms, and a block with a seam opened 4 ms.

## Across leaves

Tim, 16 November: "i want to make sure i have the option of seing just the leaves as well from left to right (Top to
botom of the stack)". The tab's **Across** choice is Children (as above) or **Leaves**:

- the node's total, then every leaf under it in the tree's order, with no subtotals between: under a block, U's benches
  top down, then M's, then L's;
- a leaf is labelled by its words below the node ("U 130" under a block, "B10 U 110" under a strip);
- a line where the leaf's parent changes, so one seam's benches stand apart from the next;
- up to 500 leaves across; past that the rest are counted and a lower node is picked to see them, the total holding them all.

`reserves_view::breakdown_leaves`. Known answers: strip 1 of the made-up table runs B2 U, B10 U 110, B10 U 100, B10 L,
adding up to 10,000 m³. On the demo pit, block 3_B1 runs U 130 to U 100, M 110 to M 50, L 60 to L 20, its leaves adding up
to its 64,519.3 m³ of ROM coal (to the shown decimals). The 50m pit's 2,847 leaves show the first 500 in 48 ms.

## Built: 5d, the Parameters tab

Tim picked 5d before PIT B against Spry on 16 November ("the formulas will give the same answers if we have the same data
and assumptions"), and agreed its plan as written.

- **A Parameters tab** (toolbar, beside Reserves table): the project's parameter tables and dataflows listed on the left,
  the one picked edited on the right. Properties still only shows them.
- **A parameter table as a grid**: its `when` filter columns, then its value columns with their units and whether they
  hold numbers or words; cells as the CSV and bench rules write them (`U`, `1-5`, `U, M`, blank for any). Rows are added,
  taken out and moved (the order is the order it's read); columns added, renamed, moved among their kind and taken out.
  Cells pasted from Excel fill from the cell pasted into, adding rows; rows copy out with their headings.
- **Edits are held until Save**, edited cells in blue. A word in a number column, or a column with no name or named
  twice, is red with why, and Save stays off; so does anything the engine's own check refuses (`parameters_check`).
  Save (Ctrl+Enter) runs `parameters.save` on the table: its next revision, one undo. **Copy as a new table** saves it as
  a new one, a case's table to read first. A row left blank in every cell isn't saved.
- **Where a picked solid's values come from**: the tables read are those of the dataflow that reads this one, in its
  order, or this one alone. The rows the solid fits are marked by their numbers, and each value it takes from this table
  is gold in its row; a value taken from another table says which and its row under the column's heading
  (`ParameterTable::picked`). The marks are of the saved table.
- **A dataflow** the same way: its levels (attribute and default, in order), its parameter tables in the order read, its
  steps on or off with their settings, and its fields (name, kind, how it adds up, where its value comes from). Saved
  with `dataflow.save`, checked first by `Dataflow::check`.
- **Saving never reruns the reserves**: the tab says tables made before need Run reserves again. Running from the
  dataflow's panel comes with 5c.

No new command: both saves were already revisions. Known answers: `ParameterTable::picked` on a case table read before
the base (U in strip 3 takes its ash cut-off from the case's row 1, its waste RD from the base's row 2 because its row 1
fits but gives none, its loader from row 3). The grid's own logic (round trip, problems, edits followed through moved rows
and columns, Excel paste and copy) in `app/tests/parametersGrid.test.mjs`. The app check on the demo pit: the grid's
columns are the table's, the picked block's solid marks 17 cells gold and fits rows 2 and 3, `3O` in a number column is
refused, an edit saves as revision 2 and an undo puts it back.

Not yet: folder state saved with the table (it's remembered on the machine for now), and fields as columns, the other
way round.
