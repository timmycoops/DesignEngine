# 0007 — The seam model

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29)

## Decision

A seam model is an explicit stack of horizons, top first, checked against the grids it links to.

1. **The stack is a definition the planner owns.** Geological models name splits differently, so
   nothing is inferred from names. The definition is a CSV file, edited in Excel: one row per horizon
   with its code, parent (the horizon a split comes from), material, working flag (stacked for solids
   and reserves), roof and floor grids, and a column per other grid variable (qualities, density,
   thicknesses). `seam.suggest_definition` writes a starting definition for a folder of Vulcan grids;
   `seam.build_model` reads one, checks it and makes the model.
2. **Grids stay where they are.** The model records each grid's file name and fingerprint (size,
   modified time, 64-bit FNV-1a checksum) and reads it in place; grid data is never copied into the
   project. A model with many quality grids costs a few kilobytes in the manifest. Grids a design
   command works on (a roof to project a face onto) are imported as objects, as before.
3. **Check and flag, never fix.** Building a model never changes a grid. Each problem is recorded with
   the nodes it affects, the largest overlap and its easting and northing, so it can be fixed in the
   source model. Blocking issues stop solids and reserves using the model; the model is still made, so
   the issues can be seen. This replaces the earlier rule that thickness is clamped at zero.

## The definition file

```text
# comment lines start with #
folder,D:\models\pit\grids
horizon,parent,material,working,roof,floor,as,rd,ts
upper,,coal,yes,pitupper.srg,pitupper.sfg,pitupper.asg,pitupper.rdg,pitupper.tsg
```

`horizon`, `roof` and `floor` are required. A blank `working` means yes for a horizon with no
daughters and no for a parent; `material` defaults to coal. Files are relative to `folder`, and a
relative folder (or none) is relative to the definition. Excel's quoting, padded rows, byte-order mark
and Windows code page all read. A definition with problems (a missing column, a duplicate horizon, an
unknown parent, parents that loop, a working value that is not yes or no) is refused with every problem
listed. So is a model whose grids cannot all be read or are not all on one lattice.

## The checks

| Issue | Where | Blocks |
|---|---|---|
| Floor above roof | every horizon | when the worst overlap is beyond the tolerance |
| Roof above the floor of the next live working horizon up | the working section, node by node | beyond the tolerance |
| Coal outside the parent (above its roof or below its floor) | every daughter, where it holds more than the tolerance | beyond the tolerance |
| Roof and floor not live at the same nodes | every horizon | always |
| No density grid | working horizons | never (a warning: tonnes will need one) |

The tolerance defaults to 1 mm, so single-precision rounding in exported grids is listed without
blocking. Zero thickness is not an issue: it is how Vulcan models mark absent coal.

## The suggested stack

A horizon is suggested as the daughter of the thinnest horizon that holds it: at 99 % or more of the
nodes where it has coal, it lies within that horizon's roof and floor, and that horizon holds more coal
overall (so the tree cannot loop). Horizons with the same parent are ordered by the median difference
between their roofs at nodes where both are live. Comparing on shared nodes matters: a seam that holds
coal only up-dip ranks above the seam over it if each is judged on its own nodes. The working section
defaults to the horizons with no daughters.

## Evidence

On a client's folder of 15 seams (client data: kept local, never committed), the suggestion recovers a
four-level split tree (a composite holding upper and lower seams, each split twice more) and a working
section of ten plies. Those plies match, in reverse, the bottom-up stack the client's own Deswik
surface-stacking rules use for the same grids. Building the model from it reads 117 grids in place in
28 ms and checks the stack in 14 ms (the whole run takes 69 ms, the suggestion 78 ms). It finds no
blocking issues, matching an independent check of the same grids. The three seams without a density
grid are flagged as warnings.

## Not yet

- A crop surface (topography or base of weathering), existence polygons and fault-block domains.
- Showing thickness and issue locations in the viewport (next slice).
- Checking fingerprints when a project opens, to flag a model whose grids have changed since it was built
  (`fingerprint_file` is in place for it).
- Grids on more than one lattice; for now they are refused and named.
