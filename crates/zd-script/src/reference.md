# Writing a reserves formula

A formula is a step of a reserves dataflow written in Starlark, a small dialect of Python. It works out values for the
units of a block column (a seam's bench, a piece of dilution, a ply in a working section) between the engine's built-in
steps. It reads and sets field values; it never moves material between units, which the built-in steps do.

## Its shape

```python
def formula(unit, p):
    if unit.material.lower() != "coal":
        return
    unit.rd_is = in_situ_rd(unit.rd, unit.mad, unit.mis)
```

`formula` is called once for each unit the step's filter fits, top down. Leave `p` out if it reads no parameters.

## What it reads and sets

- `unit.<field>`: a field of the dataflow (a number, or `None` where the unit has none). Set it with `unit.<field> = …`.
  Only fields the dataflow declares can be set, and never the volume.
- `unit["<field>"]`: the same, for a name worked out as it runs, or one that isn't a plain name (`unit["as"]`).
- Labels, read only: `unit.seam`, `unit.material`, `unit.bench` (none once a seam's benches are joined), `unit.became`
  (`None`, or what the unit became: `loss`, `dilution`, `thin_loss`, `high_ash_loss`, `small_volume`, `discarded`).
- `unit.block.<attribute>`: the block solid's attributes (`unit.block.strip`); `None` where it has none.
- `unit.above.<field>`: the unit above in the column; `unit.above` is `None` for the top unit.
- `unit.section.<field>`: the unit's working section added up, each field by its rule (qualities by tonnes).
- `unit.rom_coal.<field>`: for a unit in a ROM coal section, the section's coal (its plies) added up by tonnes;
  `unit.rom_coal` is `None` outside one.
- `unit.column.<field>`: the whole column added up.
- `p.<column>`: the parameters for the unit's block, each from the first row of the dataflow's tables that fits it.
  Reading one no row gives stops the block; `getattr(p, "column", default)` reads one that may be missing.

## Functions

- `in_situ_rd(rd, mad, mis)`: in situ density by Preston and Sanders; moistures in %.
- `air_dried_rd(rd_is, mad, mis)`: the air-dried RD back from an in situ density.
- `moisture_factor(from, to)`: (100 − to) ÷ (100 − from), a moisture basis to another.
- `to_basis(value, from, to)`: a value on one moisture basis put on another.
- `weighted(values, weights)`: an average weighted by the weights; `None` where they add to nothing.
- `abs`, `round(x, digits)`, `sqrt`, `clamp(x, low, high)`, `min`, `max`, `len`, `str`, `float`, `int`, `range`.
- `print(…)`: said with the step's name after a run.
- `fail("why")`: stops the block, said with the line.

## Rules

- Qualities are always averaged by tonnes on their own basis, never by volume.
- Python's syntax; no `while`, no recursion, no `import`, no files, no network: the same answer every run.
- Each formula keeps worked examples (tests): a unit's fields and parameters, and the fields it must leave it with.
