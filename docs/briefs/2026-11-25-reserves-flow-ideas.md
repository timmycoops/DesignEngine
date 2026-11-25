# Reserves Flow: ideas for the dataflow view, parameters, units and sort order

25 November 2026. From a chat with Tim in the Design Engine claude.ai project, outside the code. **Ideas, not
decisions.** Nothing here is agreed for building: plan it with Tim first, as CLAUDE.md says for new work, and check it
against the decision records named below, which win where they differ.

- Prototype: `2026-11-25-reserves-flow-prototype.html` in this folder. Open it in a browser; it is one self-contained
  page. Hosted copy: https://claude.ai/artifact/WjPnzhCN8WWErB2bfzVZVd
- The prototype runs a **stand-in engine written in the page** (JavaScript, simplified rules, a made-up 14 × 8 block pit
  with 15 plies and partings). Its numbers are illustrative only. Take the ideas, not the code or the styling: the app's
  stack and look are locked (0001, 0089).

## How this sits against what is already built

| Area | Already decided | What these ideas add |
|---|---|---|
| Dataflow view | 0084: Inputs row, flow strip, step pane, Before and After, reads lit gold, writes underlined blue, All fields, Show as graph | Quantities carried on the strip as a material river; the units table below (groups, field sets, drawer); the field map |
| Parameter tables | 0072: filter columns, each value from the first row that fits and holds it, last row the default; a case's rows first | A seam and ply view of those rows, with the cell that supplied each value outlined for the picked unit |
| Grid | 0099: one grid, grouped by any word column, subtotals by each column's own rule, siblings sorted | Group by and custom field sets in the Dataflow view, using that grid |
| Stack order | 0070 decision 2 and 0071: the seam model's stack order on each unit; a horizon's benches top down by their floors | The two other orders (mining sequence, output order) and named hierarchies |
| Waste allocation, table setup | 0102, 0103 | Process as a level a hierarchy can group and sort by |

## What Tim said, in his words

- "The list view sounds good similar to power query as long as we see the before and after. The Inputs and parameters
  are the challenge here to keep it looking neat and easy to use."
- "I can't see the formulas and or the individual changes" (on a version that showed only totals).
- "I can see issues when we have more units inside the group and you need to see more fields on the unit."
- "I like the unit groups (we need to be able to change these), I like the filter, I like the quantities and qualities
  (could we custom groups here also)."
- "The issue with the parameters toggle will be each seam will have different settings per seam or ply generally, which
  makes that trickier to display as a toggle."
- "The sorting logic is still a really important part of how this will work ... the seams in theory come from the
  geology, potentially we have records that aren't in the geological model, but things like benches, strips, blocks,
  pits. How do we control the sort order in the dataflows for these?"
- "The logic of the data flow could provide any grouping in theory, it's just that's a good template."

## The ideas in the prototype

### 1. Material river (the flow strip carrying quantities)

Across all steps: ROM coal and waste as two lanes. Coal leaving to waste (loss, rejected plies, ash cut-off) is drawn
red, rock entering ROM as dilution is amber, product gold, topsoil green, and the margin is written at the end. Each
column is a step; click it to pick the step. Extends 0084 decision 3 (the strip carries the flow).

### 2. Step list

Steps grouped by stage, each tagged by kind: built-in, move (settings-driven splits and labels such as `take_top`),
formula, ledger. After a change, the steps that re-ran light up with their time in ms, and the run restarts at the first
step that reads the changed value; earlier steps are reused.

### 3. Step pane

The rule in words, Reads and Writes as chips, the formula (one editable line in the prototype; a refusal says why), then
the settings. A built-in step is a glass box: its rule in words, a settings form generated from its settings list, the
parameter tables it reads, the fields it reads and writes, a balance check, known-answer tests, the Spry snippet it
replaces and its version in the run record.

### 4. Parameters by seam and ply

- A single number for the whole pit (plant efficiency, topsoil depth, price, costs) is a setting.
- Anything that varies by seam or ply is a **table, not a slider**: rows Default, then each seam, then any ply with its
  own value, indented under its seam. Columns are the step's parameters with units.
- Inherited values show faint and set values bold. Type to set, clear to inherit, and "Add a ply row" gives one ply its
  own value. A case's changes carry a dot.
- Pick a unit and its rows shade, and the cell that actually supplied each of its values is outlined. GU1a, for
  example, takes its roof loss from its own row and its floor loss from the default.
- Audit notes name the row, e.g. "Roof loss 0.12 m (GU1a)".
- **For the app, this is a view of 0072's rows, not a new lookup rule.** The prototype's "ply, then seam, then
  default" is a simplification. The app keeps first row that fits and holds it, and the view should show which row fit
  and why.

### 5. Units table (Before and After at scale)

The answer to many units in a group and many fields on a unit:

- **A row is a piece of ground**: the union of the before and after boundaries, so a split shows as its own rows. A
  piece is named after its ply, with a suffix only when it is the smaller part, e.g. "GU1c · base 0.15 m".
- **Groups you can change**: working section, seam, ply, material, none. Later, any word column or hierarchy (see sort
  order below).
- **Group rows subtotal before and after** by each field's own rule: tonnes added up, qualities weighted by tonnes.
- **A piece that leaves a group stays listed in the group it left**, shown red. Everything that happened to GU-1, loss
  and dilution both, is in one place.
- **Field sets**:
  - This step: what the step reads and writes, plus thickness and tonnes. This is the default.
  - Quantities; Qualities; Class and audit; All.
  - Custom named sets you can create, edit and delete.
  - A field search across all of them.
- **Changed only** filter, and two ways to show a change: "after, was …" or "before → after". Groups the step changed
  open on their own.
- **Unit drawer**: every field on the picked piece, grouped, before and after, the last step that wrote each, and the
  piece's journey through every step.
- **Field map**: steps × fields, which step reads and which writes each field. This is the overview once a flow has
  60 to 100 fields.

### 6. Block map and strip log

- **Block map:** each block coloured by what the picked step changed (coal moved, product, margin on a diverging scale
  at the ledger). Click a block to pick it.
- **Strip log:** the block before and after the step, drawn to scale. Waste is shortened, coal moved to waste and rock
  moved into ROM are hatched, and arrows mark what moved.

### 7. Cases and checks

Case chips (Base, Low price, Tight loss) with the headline figures' change against Base. A balance check is always on
screen: no ground made or lost.

## Sort order and hierarchy (discussed, not yet in the prototype)

There are three orders, and the geology supplies only the first.

1. **Inside a column**: stack order. 0070 decision 2 and 0071 stand: the seam model's order on each unit, a horizon's
   benches top down by their floors. Pieces the geology doesn't have (dilution slivers, topsoil, partings, wedge, a
   prestrip horizon) take their place from their geometry, as the column engine's `sort` keeps a unit's pieces in order.
   This order is fixed; column steps rely on it.
2. **Across columns: the mining sequence**, from the design, not the geology.
   - Strip order comes from the strip layout's index and advance direction, block order from position along strike and
     which end mining starts, bench order from RL.
   - It is set once per project, with manual overrides for exceptions: a box cut advancing both ways, a strip reversed
     past a fault, stage 2 before stage 1.
   - Keys are integers stored on the objects. Never sort by name: S10 sorts before S2.
3. **Output order**: Spry's table levels and positions. Spry's ranges, spans and wildcards pick by position, so the
   write order decides what a range in a schedule picks up.

**Records with no column** (stockpiles, rehandle, ramps, dumps, prestrip by strip) need an anchor: a parent level and a
position rule ("first in strip", "after block B8", "end of pit"). A record without a key is refused and listed.

**Any grouping is possible; Pit\Strip\Block\Bench\Seam is a template** (Tim). So:

- **A named hierarchy is an ordered list of levels.** Each level is a field and an order:
  - The field can come from the design (pit, strip, block), the geology (seam, ply) or a step (process, dragline zone,
    horizon, stage, blast group).
  - Siblings are ordered by a key field, by geometry, by an explicit list (Topsoil, Freedig, Prestrip, Dragline,
    Excavator), or by natural name as a last resort. Each can be ascending or descending.
- **Templates in the library**, for example:
  - "Spry standard": Pit, Strip, Block, Bench, Seam
  - "Dragline": Pit, Strip, Block, Horizon, Zone
  - "Truck and shovel by stage": Pit, Stage, Bench, Block, Process
- **Steps name the hierarchy they run in.** A cumulative step says "Mining sequence, resets at Block". Rolling margin
  says "Mining sequence, next N strips". The write step says "Spry standard". The units table's Group by reads the same
  list.
- A case can change the mining sequence, say reverse the block direction, and only the cumulative and across-block
  steps re-run.
- **Checks**:
  - Every record has a unique path through the hierarchy.
  - Every level is fully ordered: no ties, and no value missing from an explicit list.
  - A level's field exists before the step that uses it. The field map knows this.

Questions for Tim, not yet answered:

1. How often does the real strip or block order differ from the layout order?
2. Does the cumulative strip ratio reset per block, per strip or per stage in his flows?
3. Is the Spry tree the same across clients, or does the level order vary?

## Other open points from the chat

- **Built-in step tiers**:
  - Core: the Import to ROM chain, fixed and driven by parameters.
  - Move: `take_top`, `take_bottom`, `label_thin`, `label_rest`, options and choose, all settings-driven.
  - Specialist: dragline, margin ledger, across-blocks.

  Leanings, not agreed:
  - The core is locked.
  - Single numbers are settings that cases can still override.
  - Python steps may move material only if they pass the balance check.

  Waste allocation (0102) has since settled part of the move tier.
- **Where custom field sets and hierarchies live**: the project (`.zdp`), a company library, or both. This touches the
  open decision on own use or a licensed product.
- **A ply row overriding only some of its seam's columns, cell by cell**, matches 0072's "first row that fits and holds
  it".

## Not to take from the prototype

- The engine: losses, dilution, cut-offs, yields and costs are made up, and the lookup order is simplified.
- Its look: the app has its own design system.
- Its storage: custom field sets go to the browser's local storage, which is only a stand-in for the project file.
