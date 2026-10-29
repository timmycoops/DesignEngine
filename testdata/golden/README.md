# Golden reference data

One folder per pit, kept local (git-ignored) unless the data is cleared for GitHub.

A pit is described either by a `pit.json` naming its files where they already are (nothing is
copied), or by files laid out by name in the folder.

## `pit.json`

```json
{
  "name": "my-pit",
  "topography": "D:/data/pit/topo.00t",
  "large_surface": "D:/data/pit/latest-survey.00t",
  "design": "D:/data/pit/shell.dxf",
  "seam_definition": "D:/data/pit/grids/seam-stack.csv",
  "seams": [
    { "name": "upper", "roof": "D:/data/pit/grids/pitupper.srg", "floor": "D:/data/pit/grids/pitupper.sfg" }
  ],
  "toe": "D:/data/pit/toe.dxf",
  "solids": "D:/data/pit/relimits.zip"
}
```

| Key | Used by |
|---|---|
| `topography` | every check: imports, string edits, the Phase 0 cut (unless `design` is given), the viewport |
| `large_surface` | optional: a big surface for the viewport check |
| `design` | optional: the surface Phase 0 Check 2 cuts seam solids with (default: the topography) |
| `seam_definition` | the seam stack CSV (decision 0007) the Phase 1 gate builds the seam model from |
| `seams` | roof and floor files for Phase 0 Check 2 and the viewport, and the seams whose Vulcan grids the Phase 1 gate imports |
| `toe` | a highwall toe string (DXF or CSV) for the Phase 1 gate; null until there is one |
| `solids` | optional: a folder or `.zip` of `.00t` solids, imported at scale |

Surfaces may be `.00t`, Vulcan elevation grids (`.srg`, `.sfg`, `.tpg`), DXF or XYZ/CSV grids.

Runs:

```powershell
.venv\Scripts\python bench\phase1_gate.py testdata\golden\<pit>\pit.json   # Phase 1 gate; report beside pit.json
cargo bench -p bench --bench check2_booleans                                # Phase 0 Check 2; bench/out/check2.md
.\bench\golden_app_bench.ps1 testdata\golden\<pit>\pit.json                 # Phase 0 Checks 1, 3, 4
```

## Files laid out by name

```
testdata/golden/<pit-name>/
  topo.dxf                 topography triangulation (or .xyz grid)
  <seam>_roof.dxf          seam roof surfaces
  <seam>_floor.dxf         seam floor surfaces
  bow.dxf                  base of weathering (optional)
  design.dxf               a design surface (e.g. pit shell) to cut seam solids with (optional;
                           Phase 0 Check 2 falls back to bow, then topo)
  toe.dxf                  a highwall toe string
  deswik_volumes.csv       reference volumes/tonnes by block and seam from Deswik
  NOTES.md                 units, coordinate system, densities, anything unusual
```

Every geometry change is tested against these, so accuracy can only go up.
