# 0085 — Run a reserves table's blocks again into it: a block, a strip or every block

Status: agreed 18 November 2026. Tim picked the range (block, strip or all), the result (the table revised in place) and
the place (the Dataflow tab) from the choices put to him.

## Why

Tim, 18 November, after saving a parameter with the Dataflow tab open: "i see the text of re-run but i see no button".
The Dataflow tab already ran on the saved parameter (0082); the Reserves table kept the numbers of the run that made it,
and the tab's line said to run reserves again, with no way to from there. Running the workflow again from the start
isn't needed: "re-run dataflows only as an option might work - or a button on the dataflow", with a range.

## Decision

1. **Run again**, in the Dataflow tab's header, beside Show as graph, runs the reserves dataflow again into the table
   the tab follows, on what's saved: the dataflow, its parameter tables and the blocks as they are now, with the run's
   own settings (its blocks, stack or seam model, its own rows, its layer and name), read from the run that last wrote
   the table. The workflow steps before it (cuts, interrogation) aren't run.
2. **The range**, a choice beside it: this column's block, its strip (the column's path less its block), or every block
   of the table's case.
3. **The table is revised in place**: the rows of the blocks run are taken out and their new rows added, every other row
   kept, as the table's next revision, one undo. A column only the new rows have is added, empty on the rows kept.
4. **What changed since is said per block.** Each run again names the run that wrote the table before it, so a block's
   column, opened, is read against the run that last wrote that block: a block run again says nothing changed, one left
   alone still says what changed since its run. The table itself, revised by the run again, isn't a change.
5. It's `reserves.run` with three settings more, no new command: `table` (run into it), `only` (the start of the
   column paths run) and `previous` (the run that wrote the table before). `rerun_params` reads them from the log for
   the app; Python can pass them as any run's.

## Built

- **Engine** (`crates/zd-commands/src/builtin/reserves_run.rs`): `RunReservesParams.table`, `.only`, `.previous`; a run
  into a table keeps the columns of its case under `only` (refused, with what was asked, where there are none), builds
  the case's table as a run does, and `merge_rerun`s it into the old one as a `Revise`. `rerun_params` gives the
  settings from the table's last run. `open_column` follows `previous` back to the run whose `only` holds the node
  (`run_for_column`), and leaves the table out of what's changed.
- **App**: the Tauri command `reserves_rerun_params`; in the Dataflow tab, the range and Run again, run as any command
  (`runRecord('reserves.run', …)`), what it did said under the header; the table's next revision runs the tab's column
  again, so the warning goes. The warning now says the block's numbers were made before the change, and points to Run
  again.

| Test | Known answer |
|---|---|
| `a_table_s_blocks_run_again_into_it_a_block_or_all` | waste RD 2.3 run, the assumptions saved at 2.5: B1 run again, the same table at its next revision with as many rows, B1's waste at 2.5 and B2's at 2.3; B1 (and its seam) read against the run again, nothing changed, and B2 against the first, the assumptions changed; every block run again, B2 at 2.5 and nothing changed for either; an undo puts B2 back at 2.3 |
| `running_again_under_nothing_is_refused` | strip 9, which has no blocks: refused, "no blocks of case Base under Pit › 9 to run again into" |
| `dataflowView.test.mjs` | the block, strip and all of `Pit › 5 › B18`: the path, `Pit › 5`, none |
| `zd-app --bench --dataflow` (16 Nov demo, 3_B1) | roof dilution doubled and saved: the block's column warns the assumptions changed; Run again for block B1 says "Pit › 3 › B1 run again into Reserves Base." and the warning goes; two undos put the demo back |
