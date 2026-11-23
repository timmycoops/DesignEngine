# Writing a reserves formula

A formula is one step of a reserves dataflow, written in Starlark: Python's syntax, run inside the engine. It works out
values on the units of a block column between the engine's built-in steps: a seam's bench, a joined seam, a ply of a
working section, a piece of dilution or loss. It reads and sets field values. It never moves material between units:
splitting, joining and cutting loss and dilution are the built-in steps' work.

Answer with the whole formula and its worked examples (below). Use only the field, label and parameter names you're
given; the engine refuses any other before the formula is saved. Make up numbers for the worked examples: never use a
client's.

## Its shape

```python
# What it does, in a line or two: a planner reads this first.
def formula(unit, p):
    if unit.material.lower() != "coal":
        return
    unit.rd_is = in_situ_rd(unit.rd, unit.mad, unit.mis)
```

- `formula` is called once for each unit the step's filter fits, top down the column. Leave `p` out (`def
  formula(unit):`) when it reads no parameters.
- It returns nothing: it sets fields on `unit`. A `return` ends it for that unit.
- Helper functions and constants may sit above it (`SIZE = ["thickness", "area"]`, `def basis(v, a, b): …`).
- The step's filter, set beside the formula in the bench rules' words (`material: coal; seam: U, M`), decides which
  units it's called for. A formula can test the same things itself.

## When it runs

A formula step sits right after one of these built-in steps, in this order:

| After | What's true of the units | Typical work |
|---|---|---|
| `initialise` | each block (a seam's bench) is a unit, top down; underburden added below the last coal; the in situ rows not yet written | sizes filled in, limits and defaults, densities |
| `join_benches` | each seam's benches joined into one unit (`unit.bench` is `None`) | tests on a seam whole, such as discarding coal |
| `working_sections` | coal formed into working sections (`unit.section`); partings are dilution | per-section values |
| `rom_dilution` | loss and dilution cut and placed | before the ROM ash cut-off |
| `rom` | the ROM ash cut-off done; ROM sections final (`unit.rom_coal`) | ROM moistures and qualities, ROM densities |

What a formula sets after `initialise` is in the in situ rows; anything set later shows from the stage after.

## What it reads and sets

- `unit.<field>`: a field of the dataflow: a number, or `None` where the unit has none. Set it with `unit.<field> =
  …`. Only fields the dataflow declares can be set, and never the volume (the volume of record). Setting `None` clears
  a value.
- `unit["<field>"]`: the same, for a name worked out as the formula runs (`unit[name]`), or one that isn't a plain name:
  Vulcan's ash is `as`, a reserved word, so it's `unit["as"]`.
- Labels, read only:
  - `unit.seam`: the horizon the unit was cut as;
  - `unit.material`: `coal` or `waste` (compare with `.lower()`);
  - `unit.bench`: its bench label, `None` once a seam's benches are joined;
  - `unit.became`: `None`, or what it became: `loss`, `dilution`, `thin_loss`, `high_ash_loss`, `small_volume`,
    `discarded`.
- `unit.block.<attribute>`: the block solid's attributes (`unit.block.strip`, `unit.block.block`); `None` where it has
  none.
- `unit.above.<field>`: the unit above it in the column; `unit.above` is `None` for the top unit.
- `unit.section.<field>`: its working section, added up (from `working_sections` on).
- `unit.rom_coal.<field>`: for a unit in a ROM coal section (its coal, partings, dilution, coal lost into it), the
  section's coal plies added up; `unit.rom_coal` is `None` outside one. This is what dilution takes its coal's values
  from.
- `unit.column.<field>`: the whole column added up.
- Each of these reads by a name worked out as it runs too: `unit.rom_coal[m]`, `unit.above["area"]`.
- Schedule fields: a field named with a folder, `Waste\Dragline\Prime Volume`, is written to the reserves table under
  `Scheduled` only (`Scheduled\Waste\Dragline\Prime Volume`), a section a process as Spry's Scheduled is laid out. Set
  it by its name in square brackets, each backslash doubled: `unit["Waste\\Dragline\\Prime Volume"] = prime`. A word
  field named so (`Waste\Dragline\Method`) is written there too.
- `unit.place_onto("<schedule field>", value, seam=…, bench=…)`: the value written on another leaf of the block, not the
  unit's own, as a blast's drill and blast volume goes on its blasting horizon's leaf. The levels named (any below the
  column) say which leaf; one not named is the unit's own. Only a schedule field that adds up by sum is placed, and a
  leaf with nothing scheduled on it takes nothing: that's said with the block. It sets nothing on the unit.
- `p.<column>`: the parameters for the unit's block: each value from the first row of the dataflow's parameter tables
  that fits the block, tables read in order. A number column gives a number, a words column words (`"yes"`).
  - Reading one no row gives stops the block. Read one that may be missing with `getattr(p, "column", default)`.
  - `dir(p)` lists the columns that have a value for this block.

Added up means each field by its own rule: volumes summed, densities averaged by volume, **qualities averaged by tonnes
on their own basis**, settings by their largest or smallest.

## Bases and units

- Moistures are percentages: `mad` air dried, `mis` in situ, `mrom` ROM.
- `rd` is the air-dried relative density (t/m³) from the lab and the seam grids; `rd_is` the in situ density;
  `rd_rom` the ROM density; `tonnes_ad_per_m3` the air-dried tonnes a cubic metre in situ (in situ density × (100 − Mis) ÷ (100 −
  Mad)), what qualities and Mad are weighed by.
- Qualities (ash, sulphur, …) are stored air dried. A quality on another basis is `to_basis(value, mad, mis)` (in
  situ) or `to_basis(value, mad, mrom)` (ROM).
- Tonnes are volume × density on the basis wanted: `unit.volume * unit.rd_is` in situ.
- Qualities are always averaged by tonnes, never by volume. Where a formula averages them itself, use `weighted(values,
  tonnes)`.

## Functions

- `in_situ_rd(rd, mad, mis)`: in situ density by Preston and Sanders: RD × (100 − Mad) ÷ (100 + RD × (Mis − Mad) −
  Mis).
- `air_dried_rd(rd_is, mad, mis)`: the air-dried RD back from an in situ density.
- `moisture_factor(from, to)`: (100 − to) ÷ (100 − from), from one moisture basis to another.
- `to_basis(value, from, to)`: a value on one moisture basis put on another.
- `weighted(values, weights)`: an average weighted by the weights; `None` where they add to nothing.
- `abs(x)`, `round(x, digits)`, `sqrt(x)`, `clamp(x, low, high)`, `min`, `max`, `len`, `str`, `float`, `int`, `range`,
  `sorted`, `enumerate`, `zip`, `any`, `all`.
- `print(…)`: said after a run with the step's name, counted over the units that printed it.
- `fail("why")`: stops the block, said with the line. Use it for data that can't be reserved.

## Looking up a parameter by any value

`p` reads each parameter from the first row that fits the block's attributes. To read a row by anything else, the
unit's worked values, its labels, or keys of your own, look it up:

- `lookup("Rates", "bcm_per_hour", equipment="DL1", bench_height=unit.thickness)`: the first row of the parameter table
  named `Rates` that fits every key given, as rows are matched everywhere: a list (`U, M`), a range (`10-15`), a
  comparison (`<0.3`), blank for any. A row filtering on something not given doesn't fit. Give `default=0` for a value
  where no row fits; without one, the block stops and says the table and the keys.
- `p.at(seam="U", thickness=unit.thickness).dilution`: the same over the dataflow's own parameter tables, in their
  order, for the keys given and only those.

Name the table in quotes, as written in the project: it's found by name when the dataflow is saved, kept by its id, and
each run names the revision it read. A traced unit shows each lookup's table, row and value.

## Reading a curve

Yield against ash or cut-point, dilution against dip, productivity against haul distance: a curve is a parameter table
with a row a point, read between its rows:

- `interpolate("Yield curve", unit.ash, x="ash", y="yield", seam=unit.seam)`: the rows that fit the keys (here, the
  seam's own curve), in order of their `x` column, and the `y` worked linearly between the two either side of the
  value. A value on a row gives that row's `y`.
- Outside the rows it's refused, saying the value and the curve's range, unless `ends="hold"`, which takes the nearer
  end's `y`.
- A curve has one row a point: two rows with the same filters at one `x` are refused when the table is saved (while
  anything reads it as a curve) and when what reads it is saved. Write `x="…"` in quotes so it's checked.

## Reading a grid of rows

Dragline factors by method, dip, pit width and zone thickness, a rate by bench height and haul distance: a parameter
table of rows, each a point on several number columns, read nearest or between on each:

- `lookup("Dragline factors", "dragline", method=unit.method, dip=nearest(unit.dip), zone_thickness=between(t))`: the
  rows that fit the plain keys (here, the method), then each marked key read off its own column of numbers, in the order
  written. `nearest(x)` takes the rows at the value nearest x (the smaller of two as near; outside the rows, the end's).
  `between(x)` takes the rows at the values either side, each weighted by how near, so two `between` keys read a value
  off four rows. On a row's own value, that row's.
- `interpolate("Dragline factors", t, x="zone_thickness", y="dragline", method=m, dip=nearest(d))` reads the same way,
  its own `x` between.
- A `between` key outside its rows is refused, saying the value and the range, unless `ends="hold"` (interpolate), which
  takes the nearer end's rows. A traced unit shows the rows read and their weights: `rows 3 (0.6), 4 (0.4)`.
- `p.at` takes plain keys only.

```python
# Dragline hours by bench height: the rate from the Rates table, the unit's own thickness as the key.
def formula(unit):
    if unit.material.lower() != "waste":
        return
    rate = lookup("Rates", "bcm_per_hour", equipment="DL1", bench_height=unit.thickness, default=0)
    unit.hours = unit.volume / rate if rate else None
```

Starlark is Python with no `while`, no recursion, no `import`, no `try`, no classes, no files and no network, so every
formula ends and gives the same answer every run. Strings, lists, dicts, `for`, `if`, comprehensions and functions work
as in Python.

## What's checked

Before it's saved, a formula is refused, said with the line, if:

- it doesn't parse;
- it uses a name it never gave a value;
- it has no `def formula(unit, p):`;
- `unit.<x>` isn't a field or label of the dataflow (with "did you mean");
- it sets a label;
- `p.<x>` isn't a parameter column;
- it sets a parameter;
- it adds up (`unit.section.<x>`, `unit.column.<x>`, `unit.rom_coal.<x>`) a field that isn't a number;
- any of its worked examples doesn't give its answer.

As it runs, a block stops, said with the line, where:

- a field is set to words, or to something that isn't a number, such as after a division by zero;
- `unit["x"]` names no field;
- a parameter no row gives is read;
- `fail` is called.

## Worked examples

Each formula keeps a few worked examples, run every time the dataflow is saved. Each is a unit on its own: its fields
and labels, its block's attributes, its parameters, the ROM section's coal where it matters, and the fields the formula
must leave it with, to 1e-9 relative. A field it should leave alone is expected at its own value; one it should leave
with no value is expected `null`. An example where the formula must stop the block gives `stops`: words its message
must hold.

```json
[
  {
    "name": "coal by Preston and Sanders",
    "unit": { "material": "coal", "rd": 1.4, "mad": 3, "mis": 8 },
    "parameters": {},
    "expect": { "rd_is": 1.371717171717 }
  },
  {
    "name": "dilution takes its coal's Mad",
    "unit": { "material": "waste", "became": "dilution", "ash": 80 },
    "rom_coal": { "mad": 3, "ash": 12 },
    "expect": { "mad": 3 }
  },
  {
    "name": "coal with no Mrom has no ROM density",
    "unit": { "material": "coal", "rd": 1.4, "mad": 3, "mis": 8 },
    "expect": { "rd_rom": null }
  },
  {
    "name": "coal with no Mis stops its block",
    "unit": { "material": "coal", "rd": 1.4, "mad": 3 },
    "stops": "coal with no mis"
  }
]
```

Keys: `name`, `unit`, and `expect` or `stops`, with `block`, `parameters` and `rom_coal` where used. Values are
numbers or words; `expect` is numbers, or `null` for no value. Work each answer by hand and say how in the name or a
comment beside the formula.

## Examples

**Densities, coal and waste** (after `initialise`):

```python
# In situ density: coal by Preston and Sanders, which must have RD, Mad and Mis; waste its RD as given.
def formula(unit):
    if unit.material.lower() == "coal":
        missing = [n for n in ["rd", "mad", "mis"] if unit[n] == None]
        if missing:
            fail("coal with no " + " or ".join(missing))
        unit.rd_is = in_situ_rd(unit.rd, unit.mad, unit.mis)
    elif unit.rd != None:
        unit.rd_is = unit.rd
```

**A cut-off from the parameters** (after `join_benches`, each seam whole):

```python
# Coal over its in situ ash cut-off is marked for the working sections to set aside: 2. Kept: 0.
def formula(unit, p):
    if unit.material.lower() != "coal" or unit.became != None:
        return
    cutoff = getattr(p, "insitu_ash_cutoff", None)
    unit.discarded = 2 if cutoff != None and unit.ash != None and unit.ash > cutoff else 0
```

**A quality on the in situ basis, into a field of its own** (after `initialise`):

```python
# Ash on the in situ basis: air-dried ash x (100 - Mis) / (100 - Mad).
def formula(unit):
    if unit.ash != None and unit.mad != None and unit.mis != None:
        unit.ash_is = to_basis(unit.ash, unit.mad, unit.mis)
```

**From the unit above** (after `initialise`):

```python
# A unit with no area takes the one above's, the column's blocks being one footprint.
def formula(unit):
    if unit.area == None and unit.above != None:
        unit.area = unit.above.area
```

**Dilution against its coal** (after `rom`):

```python
# Dilution and partings with no moisture take their coal's; a negative ash (a site's way of saying "as the coal") too.
def formula(unit):
    coal = unit.rom_coal
    if coal == None or (unit.material.lower() == "coal" and unit.became == None):
        return
    for m in ["mad", "mis", "mrom"]:
        if unit[m] == None:
            unit[m] = getattr(coal, m)
    if unit.ash != None and unit.ash < 0:
        unit.ash = coal.ash
```

**Limits and defaults for any field** (after `initialise`), names worked out as it runs:

```python
# Each field with a `<field>_default` parameter takes it where it has no value or is outside `<field>_min` to `_max`.
def formula(unit, p):
    for name in dir(p):
        if not name.endswith("_default"):
            continue
        field = name[:-len("_default")]
        if not hasattr(unit, field) or field == "volume":
            continue
        low, high = getattr(p, field + "_min", None), getattr(p, field + "_max", None)
        value = unit[field]
        if value == None or (low != None and value < low) or (high != None and value > high):
            unit[field] = getattr(p, name)
            print(field + " given its default")
```

# Writing a table script

A table script works on a whole table at once: a reserves table, one imported, or one another script made. It's
Starlark too, `def run(t, p):`, and gives back a table. Use it to sum up by strip and seam, to work ratios on those sums,
to join another table (a Vulcan or Minex export for a reconciliation, a yield lookup), or to lay a table out as a
schedule input. It reads numbers; it never moves material between units, and never writes over a number a run wrote.

The work is the engine's: filtering, grouping, joining and adding up are calls on the table, done in Rust, so a table of
140,000 rows is never walked a row at a time. Write the script as a few such calls, then a list made from whole columns
where you need one of your own.

## Its shape

```py
# Strip ratio by strip and seam: waste BCM over coal tonnes, from each group's own sums.
def run(t):
    s = t.group(["strip", "seam"])
    waste, coal = s["ROM\\Waste\\volume"], s["ROM\\Coal\\tonnes"]
    ratio = [w / c if w != None and c else None for w, c in zip(waste, coal)]
    return s.with_column("strip_ratio", ratio, unit="bcm/t", adds="ratio of ROM\\Waste\\volume per ROM\\Coal\\tonnes")
```

- `run` is called once, with the table it's run on. Leave `p` out (`def run(t):`) when it reads no parameters; `p.name`
  reads a parameter table's value for the table as a whole (its first row that fits no words in particular).
- It returns a table: `t` itself, one made from it, or `new_table(...)`. A `return` with nothing is refused.
- A column's name is its folder path, `"ROM\\Coal\\tonnes"`, with each `\` doubled inside a string.

## What a table does

Each of these gives a new table; `t` itself never changes.

| Call | What it gives |
|---|---|
| `t["ROM\\Coal\\tonnes"]` | the column as a list, `None` where a row has no value |
| `len(t)`, `t.rows`, `t.columns`, `t.name`, `t.unit("ash")` | its size, its column names, its name, a column's unit |
| `t.where({"seam": "U, M", "Insitu\\Coal\\thickness": "<0.3"})`, `t.where(material="coal")` | the rows a filter keeps, in the grid's words (lists, ranges `1-5`, `<`, `>`) |
| `t.group(["strip", "seam"])` | a row a group, in the grid's order (seams in the stack's order, B2 before B10): each number added up by its own rule (tonnes summed, RD by volume, ash by tonnes, a ratio of its sums), words where the group agrees |
| `t.total("ROM\\Coal\\tonnes")` | one number, added up over every row by its rule |
| `t.join(other, on=["strip", "block", "seam"], prefix="vulcan ", how="left")` | each row with the other's columns beside it, matched on the `on` words; `how="outer"` adds the other's unmatched rows at the end |
| `t.with_column("name", values, unit="t", adds="sum")` | a column added, or one of its own replaced: a value a row |
| `t.select([...])`, `t.drop([...])`, `t.rename({"old": "new"})` | its columns chosen, dropped or renamed |
| `t.sort("ROM\\Coal\\tonnes", descending=True)` | its rows in a column's order |
| `t.row(0)` | one row as a dict of its columns |
| `table("Vulcan check")` | another table of the project, by its name |
| `new_table({"block": blocks, "tonnes": tonnes}, units={"tonnes": "t"}, adds={"tonnes": "sum"}, name="check")` | a table from lists, all as long |

`adds` says how a number adds up in a group: `"sum"`, `"max"`, `"min"`, `"volume"` (averaged by the `volume` beside it),
`"mass of rd"` (averaged by volume × that density), or `"ratio of <column> per <column>"`. A number with no rule is left
empty in a group, and the run says so. The formula library (`in_situ_rd`, `weighted`, `round`, …) works here too.

## How it runs

- **As a task**, `table.script`, on a table: a new table of its own (a summary, a reconciliation, a schedule input), or
  with `into: columns` its new columns put on the table it ran on, kept by each row's key. Columns go on a table only
  if the script kept its rows: grouped, sorted or filtered, it makes a new table instead.
- **As a step of the dataflow**, after `final`: it runs on the case's table each time the dataflow runs, and the
  columns it adds are the run's own, written again by the next run. What every case and every month needs goes here.
- **As a workflow step**, after the reserves run that makes its table.

A script never writes over a column the run wrote or a person entered; the run refuses it, naming the column.

## A mine's own checks

The stages (Imported, Insitu, ROM, Scheduled) are checked by the engine's balance on every block. Which processes a
mine has, and what they add back to, are the mine's own: write them as a table script step after `final`, with two
helpers so every site's checks read the same.

| Call | What it checks |
|---|---|
| `check("waste processes", t["ROM\\Waste\\dragline_bcm"], t["ROM\\Waste\\truck_bcm"], equals=t["ROM\\Waste\\volume"], tolerance=0.01)` | the parts added up on every leaf against `equals` (a leaf with no value counts it as 0): the difference goes to `Summary\Checks\waste processes\difference`, 1 on each leaf out by more than the tolerance to `…\leaves out`, and the run names each leaf out |
| `required("ROM\\Waste\\dragline_bcm", material="waste", seam="U, M")` | every leaf of that material (its stage's `Waste\volume` above 0; ROM's where the column has no folder) that the filter keeps has a value: 1 on each with none to `Summary\Checks\dragline_bcm on waste\leaves missing`, so the Summary counts them, and the run names each |

A part or `equals` is a column's values (`t["…"]`, or a list worked out a row each) or its name. Both are worked out on
the table `run` gives back, so a column the script adds can be checked, and the checks' columns go on that table. Each
check has its own name (`required` takes `name=` where the default would repeat). The Reserves table's Balance view shows
every check under the stages, holding or off.

## Examples

**A reconciliation against another package's export** (a task):

```py
# Blocks whose in situ coal differs from Vulcan's by more than the tolerance, and blocks only one of them has.
def run(t, p):
    ours = t.group("block")
    j = ours.join(table("Vulcan check").group("block"), on="block", prefix="vulcan ", how="outer")
    blocks, diffs, why = [], [], []
    for b, a, v in zip(j["block"], j["Insitu\\Coal\\tonnes"], j["vulcan Insitu\\Coal\\tonnes"]):
        if a == None or v == None:
            blocks.append(b)
            diffs.append(None)
            why.append("in one only")
        elif abs(a - v) > p.tolerance:
            blocks.append(b)
            diffs.append(v - a)
            why.append("differs")
    return new_table({"block": blocks, "difference": diffs, "why": why}, units={"difference": "t"}, name="Vulcan differences")
```

**Each leaf's share of its strip's coal** (a dataflow step after `final`, or a task with `into: columns`):

```py
# A leaf's ROM coal tonnes over its strip's: each strip's shares add to 1.
def run(t):
    strips = t.group("strip")
    whole = {s: c for s, c in zip(strips["strip"], strips["ROM\\Coal\\tonnes"])}
    share = []
    for s, c in zip(t["strip"], t["ROM\\Coal\\tonnes"]):
        total = whole.get(s)
        share.append(c / total if c != None and total else None)
    return t.with_column("strip_coal_share", share, adds="sum")
```

**A mine's process checks** (a dataflow step after `final`):

```py
# Dragline, truck and shovel and dozer push take all the ROM waste; every waste leaf has a dragline volume; ROM coal is
# all mined.
def run(t):
    check("waste processes", "ROM\\Waste\\dragline_bcm", "ROM\\Waste\\truck_bcm", "ROM\\Waste\\doze_bcm", equals="ROM\\Waste\\volume", tolerance=0.01)
    check("coal mined", "ROM\\Coal\\coal_mined_t", equals="ROM\\Coal\\tonnes", tolerance=0.1)
    required("ROM\\Waste\\dragline_bcm", material="waste")
    return t
```

# Writing an allocation script

An allocation step works on a whole block column at once, after `rom` and before the final writes: Spry's waste
allocation, a column (pit, strip, block) at a time. It's Starlark, `def allocate(column, p):`, run once on each column.
Use it to split the waste into what each process digs (topsoil and freedig off the top, a dragline system's zones from
the bottom up, a wedge over each coal), to set each piece's words (`process`, `zone`) and to fill each process's
schedule fields (`Waste\Dragline\Prime Volume`), which are written under `Scheduled` only.

The splitting is the engine's: a split is the column's own, done in Rust, so the pieces add back to the unit exactly and
the balance holds. A script never sets a unit's volume: it splits the unit to share it.

**Every number comes from a parameter row.** Thicknesses, limits, factors, rehandle and swell are read from the
dataflow's parameter tables (`p` for the column, `unit.p` for a unit, `lookup("Table", "column", key=…)` for any table by
its keys), never written in the script: the script holds the order and the logic, and each read is traced in the
notebook to its table and row.

## Its shape

```star
# Topsoil then freedig off the top of the first waste interval; on each interval over coal a wedge off its bottom; the
# rest Excavator. Thicknesses are the interval's, across its units.
def allocate(column, p):
    coal = column.coal()
    for k, interval in enumerate(column.intervals()):
        units = interval
        if k == 0:
            for process, thickness in [("Topsoil", p.topsoil_thickness), ("Freedig", p.freedig_thickness)]:
                taken = column.take_from_top(units, min(thickness, column.total("thickness", units=units)))
                for u in taken:
                    u.process = process
                units = [u for u in units if u not in taken]
        below = column.after(interval[-1])
        if units and below != None and below in coal:
            wedge = column.take_from_bottom(units, min(p.wedge_thickness, column.total("thickness", units=units)))
            for u in wedge:
                u.process = "Wedge"
            units = [u for u in units if u not in wedge]
        for u in units:
            u.process = "Excavator"
            u["Waste\\Excavator\\Prime Volume"] = u.volume
```

## What the column gives

- `column.units`: every unit, top down. `column.waste()`: those that end in ROM waste (waste, losses, discarded coal);
  `column.coal()`: those in a ROM coal section (its coal, partings and dilution). Small volumes, the underburden left and
  waste below the last coal aren't mined, and are in neither.
- `column.intervals()`: the waste intervals between the coal sections, top down, each a list of its units in order:
  a unit or several (two waste horizons over a seam, a rejected coal or a discarded thin one between them, a loss mined
  with the waste). Work on an interval's thickness, `column.total("thickness", units=interval)`, not one unit's.
  `column.intervals(ends="coal")` gives the coal sections the same way.
- `column.take_from_top(units, 4.5)`, `column.take_from_bottom(units, 20)`: a thickness taken across as many of these
  units as it needs, the one it ends in split there; the pieces taken, top down. More than they hold is refused.
- `column.after(u)`, `column.before(u)`: the unit below or above, None at the ends.
- `column.group_while("process", "zone", units=column.waste())`: runs of neighbouring units with the same values, a list
  of lists (a blast is a run of waste with one blasting horizon).
- `column.total("volume", units=[a, b])`: a field over units added up by its own rule.
- `column.name` (`Pit/3/B1`) and `column.block.strip`: the column's words and its own attributes.

## What a unit gives

- Its fields, words and labels as a formula's: `u.volume`, `u.thickness`, `u.process`, `u.seam`, `u.material`,
  `u.bench`, `u.became`; a schedule field by its name in square brackets, `u["Waste\\Dragline\\Prime Volume"]`. Set a
  field or word the same way; a label isn't set.
- `u.split_thickness(2, 5)`: pieces of 2 m and 5 m off the top, then the rest: a list one longer than asked. A piece of
  0 m, or the rest when nothing is left, is None. Asking for more than the unit holds stops the column, saying how thick
  it is.
- `u.split_shares([0.2, 0.3, 0.5])`: the unit split by volume shares adding to 1, in order; a share of 0 is None. The
  last piece is what's left, so the pieces add back exactly.
- Each piece takes the unit's labels and words; its volume, thickness and other summed fields are shared, and every
  other value kept. A piece written by `final` goes on the unit's leaf, as the unit would have.
- After a split, the unit split is its last piece (the rest): work on the pieces the split gives back, and read what
  you need of the whole unit (`t = u.thickness`) before splitting it.
- `u.place_onto("Waste\\Drill and Blast\\Volume", x, seam=…, bench=…)`: a schedule field's value written on another
  leaf of the block, as a formula's; or `unit=top`, on the leaves another unit is written on (a blast's top unit).

**Add waste allocation** (the Parameters tab, or `add_waste_allocation` from Python) adds the template's way as six
allocation steps (loose ground, short-term overrides, the dragline system, excavator, partings and wedge, blasting, and
each process's quantities), a check step after `final`, four parameter tables and a schedule field section a process.
It's one way of doing it: copy a step and change it, or write your own from these calls.
- `u.block.strip`: its block's attributes. `u.p.min_burden`: the parameters by its block's attributes.

## Examples

```star
# A dragline system: on each waste interval over coal, its zones from the bottom up across the interval's units, each
# split by its method's factors read off a grid, nearest the dip and between the zone thicknesses.
def allocate(column, p):
    coal = column.coal()
    zones = int(p.zones)
    for interval in column.intervals():
        below = column.after(interval[-1])
        thickness = column.total("thickness", units=interval)
        if below == None or below not in coal or thickness < p.min_burden:
            continue
        zone_thickness = min(thickness, p.system_max) / zones
        units = interval
        for n in range(1, zones + 1):
            zone = column.take_from_bottom(units, min(zone_thickness, column.total("thickness", units=units)))
            units = [u for u in units if u not in zone]
            f = lambda name: lookup("Dragline factors", name, method=p.method, zone=n, dip=nearest(p.dip), zone_thickness=between(zone_thickness))
            for u in zone:
                u.zone = "Zone %d" % n
                cast, dozer, dragline = u.split_shares([f("cast"), f("dozer_push"), f("dragline")])
                for piece, process in [(cast, "Cast"), (dozer, "Dozer Push"), (dragline, "Dragline")]:
                    if piece != None:
                        piece.process = process
```
