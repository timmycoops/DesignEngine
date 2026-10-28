# Golden reference data

One folder per pit, kept local (git-ignored) unless the data is cleared for GitHub.

```
testdata/golden/<pit-name>/
  topo.dxf                 topography triangulation (or .xyz grid)
  <seam>_roof.dxf          seam roof surfaces
  <seam>_floor.dxf         seam floor surfaces
  bow.dxf                  base of weathering (optional)
  toe.dxf                  a highwall toe string
  deswik_volumes.csv       reference volumes/tonnes by block and seam from Deswik
  NOTES.md                 units, coordinate system, densities, anything unusual
```

Every geometry change is tested against these, so accuracy can only go up.
