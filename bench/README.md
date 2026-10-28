# Phase 0 bench

Results go in [RESULTS.md](RESULTS.md). Raw outputs land in `bench/out/` (git-ignored).

## Check 2: booleans (pure Rust)

```powershell
cargo bench -p bench --bench check2_booleans
```

Runs synthetic pits (250, 500 and 1000 cells a side at 10 m; set `ZD_CHECK2_SIZES=250,2000` to
change) plus every seam found under `testdata/golden/<pit>/` (`<seam>_roof` and `<seam>_floor`,
cut by `design`, else `bow`, else `topo`). Real roof and floor triangulations are sampled onto a
common lattice (`ZD_GOLDEN_CELL`, default 5 m). Writes `bench/out/check2.md`.

## Checks 1, 3 and 4: viewport, transfer, editing (in the app)

```powershell
cargo run --release -p bench --bin make_synthetic      # writes bench/out/synthetic/
cd app; pnpm tauri build --no-bundle
..\target\release\zd-app.exe --bench                   # runs unattended, then exits
```

`--bench` loads the surfaces, orbits the scene at its real size and then with copies tiled up
to 100 million triangles, times transfers of 2, 8 and 18 million triangles (as a general
triangulation and as grid elevations, each over a WebView2 shared buffer and over IPC), and drags
a polyline vertex through 300 snapped updates. Results go to `bench/out/app-results.json`. Add
`--surfaces a.dxf,b.xyz` to use real data instead of the synthetic pit. Leave the window in
front while it runs: a hidden window throttles its frame rate.
