# 0069 — The block table: solids as rows, to see, sort, filter, add up and act on

Status: built 14 November 2026, stage 4 of the Workflow plan (Tim: "lets continue onto the next steps", after stage 3).
On 0062 (blocks are polygons), 0061 (interrogated values on the blocks) and 0068 (the report).

## Why

A cut by polygons makes hundreds of blocks (the demo pit 533, PIT B thousands) and their values sit as attributes on each
solid, seen one at a time in Properties. Picking out what's under 50 bcm at a pause, checking a strip's coal against the
report, or handing the blocks to a spreadsheet needed a table, and the plan has had it as stage 4: every block of a run in
one table, matching its solid.

## Decision

1. **A dock tab, "Block table"**, under the viewport, opened by Blocks on the toolbar. It shows only: the solids are
   changed by commands, never by typing into a cell (the UI plan: Properties shows only).
2. **Which solids:** a layer (those holding blocks offered first), what's picked, or what's on show. A row a solid, its
   name, its own volume (the volume of record) and every attribute as a column, the cut's own first (strip, block,
   seam, horizon, material, case), the rest by name. One bridge call (`solids_table`) reads them all.
3. **Filter and totals:** words every row shown has somewhere (its name, layer or a value), and a volume under which to
   look. The rows shown are added up: how many, their m³, and their tonnes by a density column (`rd` or one named for
   density, picked from those there), over the rows that have one, said when not all do.
4. **Acting on them:** a click on a row picks its solid in the view and the tree; Select shown picks every row shown;
   Delete shown deletes them in one `object.delete_many` run, one undo, after a second click naming how many and their
   volume, and that run says the volume taken out, so the report counts it (0068). CSV saves the rows shown with exact
   values.
5. **Windowed:** only the rows on screen are drawn, so tens of thousands cost no more than a screenful.

## Not here

Grouping and subtotals by strip or seam (the report has them), editing values, columns picked or hidden by hand, and the
reserves dataflow's own table (stage 5).

## Done when

Every block of a run in one table, matching its solid: the volumes are the solids' own and the attributes theirs.

## Built

`app/src/panels/blockTable.ts` (columns, filter, totals, CSV, with tests in `app/tests/blockTable.test.mjs`),
`BlockTablePanel.tsx` (the tab), `solids_table` in the shell. `zd-app --bench --workflow` lists the box pit's two pieces
at 600,000 m³, filters to the piece below RL 40 (240,000 m³), deletes it from the table and brings it back with one undo
(43/43). On the made-up demo pit, the coal filtered in the table, 263 of 533 blocks, adds up to 6,461,326 m³ and
9,358,563 t by `rd`, as the report (0068) has them.
