# 0074 — ROM, and the reserves table at the key levels

Status: agreed 15 November 2026. Tim said "They look good" to the decisions on the Build Plan's "ROM plan" tab, all ten
as recommended. This is workstream 4 of the reserves dataflow (0070), built as four PRs:

- 4a: the reserves table;
- 4b: ROM loss;
- 4c: dilution and the ROM ash cut-off;
- 4d: ROM, final, Scheduled and clean coal.

## Why

Tim, seeing the reserves in Properties as a list of write points, said:

> we need to work on the table for the dataflows though it needs to have separation so you can see the changes in the
> block from Imported > Scheduled

Then, on how it should be shaped:

> I expect to see a record at the key levels (pit, ramp, strip, block, seam, bench). You would then have folders that
> allow you to see that process. The table would allow you to see summaries - sum of volumes and weight averaged if
> qualities

On levels:

> The levels would be a custom attribute essentially - maybe we don't need ramp on some but these control the grouping
> logic and how the table tree looks is all

On folders:

> The folders are just a way of viewing the data in my head. We could have any folder we want - margin, strip ratio etc
> may be added in the future as well

And on materials:

> spry would store the data as imported/waste/volume and imported/coal/volume - this controls the materials types from
> the seam stack essentially. So if you had records that were unbenched it might have coal and waste on the same node

Ratios and margins are worked by scripts from the summed parts for now, never stored as summed or averaged fields.

## Decision

1. **Levels on the dataflow.** An ordered list of block attributes, each with an optional default: `pit` (default
   `Pit`), `strip`, `block`, `seam`, `bench`. Add `ramp` or any other attribute where a site uses it.
   - The levels above `seam` make a column, as Spry's grouping expression does; all of them shape the tree.
   - A block missing an upper level with no default is left out, and said.
2. **The reserves table: a row a leaf, a column a field under its folders.**
   - A leaf is a case and a value for each level. A field's column is its folder path, such as
     `Insitu\Coal\volume`.
   - Folders are only how fields are shown. The materials come from the seam stack (coal and waste, or whatever the
     stack calls them), so a leaf holds its coal and the waste above it, as Spry's node does.
   - Each column keeps its field's add-up rule, so any node of the tree is its leaves summarised. Thickness and area add
     only down a column, so they're not summarised.
3. **Each step completes its folder.**
   - Reading the blocks fills `Imported`, each block as read.
   - `small_volumes` fills `Small volume`.
   - `in_situ` fills `Insitu`.
   - `working_sections` fills `Working section` (with each section's id and name) and `Loss\Thin`,
     `Loss\High insitu ash` and `Loss\Discarded`.
   - Loss, dilution and ROM come in 4b to 4d; ROM goes on the leaf a piece ends on, its output leaf.
   - `Scheduled` is filled last.
4. to 10. Loss, block length, dilution, the ROM ash cut-off, ROM calculations, final, Scheduled and clean coal, and the
   preview in the new shape: as the plan tab has them, recorded here as each is built.

## Built: 4a, the reserves table

- **`zd_core::Level` and `Dataflow::levels`** (`default_levels()` where a dataflow has none, so saved ones read as
  before). `column_levels()` is the levels above `seam`, else all but the last. The check refuses a level named twice.
- **`ColumnKey`** is now a case and a path, the values of the upper levels; `words()` gives `Pit/3/B1`.
- **Writes are pieces.** Each step writes pieces: a unit's values under a folder.
  - A joined unit is put back on its benches by their in situ shares.
  - Each piece sits on the leaf of the block it came from, read from that block's lower-level attributes.
  - A unit's folder is the stage and its material (`Insitu\Coal`), or a folder of its own (`Loss\Thin`).
  - Words, such as a working section's name, go in columns of their own.
  - The underburden, out of model, is never written.
- **The table.**
  - One row a leaf, in the order the leaves are first written (top down).
  - The case and each level as words; then each folder's fields, each a leaf's pieces added up by the field's rule.
  - Beside them, where the folder has an in situ density: in situ tonnes, air-dried tonnes and the qualities on the in
    situ basis (the Mass rule names its density by its full path). Where it has only an RD, tonnes on its basis.
  - A column with no value anywhere is left out.
- **The report and Properties** summarise each top folder by seam and the folder below it (material or kind of loss),
  with a total for each and the benches. Weights are looked up beside each field.
- **Properties** shows the dataflow's levels, and the reserves table's leaves and top folders.
- **The preview** and Python's `Session.reserves_preview` take the block's path (`["Pit", "3", "B1"]`).

Known answers (`crates/zd-commands/tests/reserves_run.rs`; `report.rs`):

- **The two blocks give six leaves.** B1's U and L, and B2's U unbenched (holding OB), U 18, U 16 and L.
  - B1's U leaf holds its 20,000 m³ of coal (28,000 t at RD 1.4) and the 50,000 m³ of OB above it (115,000 t at 2.3).
  - Imported coal is 70,000 m³ and waste 110,000 m³. In situ, everything comes to 180,000 m³.
- **A dataflow's own levels.** Pit (default West), strip, block and seam, with no bench.
  - B2's U benches share one leaf: 20,000 m³ at RD 1.5.
  - A block with no strip is left out, said.
- **Working sections** write each seam's coal with its section's number and name, and the A-B parting on B's leaf.
  Thin C is under `Loss\Thin` and D under `Loss\High insitu ash`. With a forced split, B's leaf is named "A; B": it
  holds the parting, which went with A, and B's coal.
- **The report test.** U's bench leaves of 10,000 m³ at RD 1.4 and 1.6, with ash 10 and 12, come to U coal 20,000 m³
  at 1.5 and ash 332,000 / 30,000. U waste is reported on its own. A column of nothing is nothing.

The demo pit, Base and 50m:

| case | leaves | blocks | working sections | in situ |
|---|---|---|---|---|
| Base | 1,436 (all benched) | 1,815 | 256 | 169,112,828.499 t |
| 50m | 2,847 | 3,580 | 509 | 169,112,828.392 t |

- Both cases have 76,003,157.442 m³ in their Imported folders and their Insitu ones, 0 from their blocks.
- Base's 169,112,828.499 t in situ and 169,304,790.747 t air dried are both 0 from its blocks.
- `reserves.run` takes 12 to 15 ms for 90 columns.
- `zd-app --bench --dataflow` reads Properties back on it and passes all four checks.

## Built: 4b, ROM loss

`rom_loss` comes after `working_sections`, Spry's way (decisions 4 and 5).

- **Sections.** It reads the working sections and the waste between them, top down, from the labels
  `working_sections` gave.
- **Roof and floor loss.** For each coal section:
  - roof loss is `roof_loss` m and floor loss `floor_loss` m, the largest of the section's plies', over the section's
    area (the largest of its units');
  - the roof loss comes off its top into the section above, and the floor loss off its bottom into the section below
    (the underburden where it's the last coal);
  - a column that ends in coal with nothing below stops its block, said.
- **Coal boundaries.** Across a boundary with another coal section nothing is taken, unless the section's first ply's
  row says `loss_between_coal` yes. Then the loss goes into that coal section.
- **Edge and other loss** come off each coal ply's top into the nearest waste above. Edge loss is `edge_loss` m × the
  block's length × the ply's thickness, and other loss is `other_loss` % of the ply's volume, both as the ply stood
  before roof and floor loss, as Spry's are. With no waste above, there's none.
- **A section short of its loss.** A section that can't give its roof and floor loss, or then its edge and other loss,
  is thin loss, all of it. Its partings go back to waste, in the nearest waste section, as Spry's are. Said.
- **Labels.** Each piece taken is labelled loss of its kind, and its ROM label names the section it went to. The
  preview shows both.
- **The writes.** Each loss goes on the leaf it came from, coal alone as Spry writes it: `Loss\Roof`, `Loss\Floor`,
  `Loss\Edge`, `Loss\Other`, and `Loss\Thin` for a section short of its loss.
- **The block's length** (`block_length`) is the longer side of the smallest rectangle round its footprint, from the
  convex hull of its vertices. A block's `length` attribute (the step's setting names it) is used before it.
- **The starter** adds `roof_loss`, `floor_loss`, `edge_loss`, `other_loss` and `loss_between_coal`.

Known answers (`crates/zd-commands/tests/reserves_run.rs`):

- **Length.** An 80 m by 20 m footprint turned 30° at mine coordinates is 80 m long.
- **One 5,000 m², 100 m block.** Roof 0.1 m, floor 0.2 m, edge 0.05 m, other 2 %.
  - A, 10,000 m³: roof 500 m³, edge 10 m³ and other 200 m³ go into OB, and floor 1,000 m³ into A-B. 8,290 m³ is left.
  - B, 15,000 m³: roof 500 m³, edge 15 m³ and other 300 m³ go into A-B, and floor 1,000 m³ into the waste below.
    13,185 m³ is left.
  - Every piece is in place and labelled, each loss is on its leaf, and the column balances to 0.
- **Roof 1 m, floor 1.5 m.** A can't give 12,500 m³, so all 10,000 m³ is thin loss. B gives 5,000 m³ and 7,500 m³.
- **Two coal sections touching.** Nothing crosses the boundary. Asked on A's row, A's floor loss (500 m³, off its
  parting) goes into section B.

On the demo pit the coal rules are roof 0.1 m, floor 0.1 m, edge 0.05 m and other 1 %.

| | Base | 50m |
|---|---|---|
| roof loss | 166,168.9 m³ | 166,101.9 m³ |
| floor loss | 166,168.9 m³ | 166,101.9 m³ |
| edge loss | 5,039.9 m³ | 6,352.9 m³ |
| other loss | 64,595.1 m³ | 64,590.6 m³ |
| thin loss | 1,802.5 m³ | 2,251.6 m³ |

- In situ is unchanged, 0 from the blocks.
- The run took 19 ms.
- **Edge loss is higher in 50m.** Its blocks are 50 m along the strip and 60 m across it, so the longer side of the
  smallest rectangle is the strip's width, not the block's length along the strip. Where edge loss must follow the
  strip, the blocks need a `length` attribute. Tim to say whether the rule should be the length along the strip.
