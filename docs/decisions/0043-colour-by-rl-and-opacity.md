# 0043 — Colour by RL and see-through surfaces

Date: 2026-11-05 · Status: proposed (Tim agreed the plan on 2026-11-05: "each object" for the colour range, and saving
in the manifest beside hidden objects. On the PIT B pictures he judged the look good and good enough for now, but wasn't
convinced by the palette; on 2026-11-05 he picked Spectral from five ramps drawn side by side on PIT B, as the one
default, with no picker)

## Why

Every surface and grid drew in one flat colour, its layer's, so topography, a roof and a floor on one layer read as one
grey sheet, and a surface under another couldn't be seen at all.

## Decision

1. **Surfaces and elevation grids are coloured by RL by default**, in bands between round RLs with a thin contour line
   at each band edge. The layer's colour stays as the other choice, per object. Seam horizons keep their thickness
   colours, and solids keep their layer's colour.
2. **The colouring is in the shader**, a TSL node graph on `MeshStandardNodeMaterial`, so it runs on WebGPU and WebGL2
   alike and nothing per vertex is built in JS. A change of colouring, bands or opacity is uniforms only: no refetch,
   no new buffers, no shader rebuild (a change between solid and see-through rebuilds the pipeline once).
3. **Band edges land on round RLs.** The GPU has f32 positions less the scene origin, whose RL is a whole metre
   (`state.rs` `origin`), so the first edge is passed in local z, exactly, and each fragment's band is
   `floor((z - base + 1 mm) / interval)`: **what lies on an edge is in the band above it**, as a bench is named by its
   floor, so a berm at RL 115 is one colour, the 115 to 120 band (below, 2026-11-06). A surface whose top is on an
   edge gets a band above it, so its top berm is coloured as the rest are.
4. **Each object takes the whole ramp over its own RLs** (Tim's choice), at an interval picked from its range: 1, 2 or
   5 times a power of ten, the smallest giving at most 20 bands. The interval can be set (1 to 100 m) for every object
   at once. **Same colours on every surface** puts everything coloured by RL on show over one range, so an RL is one
   colour on topography, roof and floor; showing or hiding a surface can then shift the colours.
5. **The ramp is Spectral** (ColorBrewer's eleven classes, reversed: purple-blue low, through teal, green and pale
   yellow, to orange and red high), Tim's pick from five drawn on PIT B. It's a softer rainbow, in the blue-low,
   red-high order of the rainbow legends Vulcan and Deswik users know, and it isn't viridis, so an RL is never read as a
   seam thickness. It's one default for every surface: there's no picker and nothing more is saved. The other four
   stay as tables in `app/src/viewport/rlPalettes.ts` so the bench can draw them all again (below), but the app
   offers none of them.
6. **Contour lines are about a pixel wide at any zoom**, from the screen-space derivative of the band number, and fade
   out where edges crowd closer than a few pixels. Zoomed out until a band is under a few pixels, the bands blend into
   the smooth ramp rather than shimmer.
7. **Opacity is per object and per layer, and they multiply.** A layer's solids draw as one mesh (0019, 0026), so a
   solid is seen through by its layer alone. See-through surfaces draw after the solid ones, without writing depth,
   **a whole surface at a time**, the one farthest up or down from the camera first (looking down on a pit: floor,
   roof, topography). Left to three.js each chunk sorts on its own, and grid tiles of two see-through surfaces
   interleaved into a patchwork of which is on top. Each draws in one pass, not back faces then front.
8. **Strings, handles and markers stay on top.** They draw over everything in the solid pass, so a see-through
   surface drawn after would dim them. With one on show, the overlays draw again in a pass of their own (the scene's
   background is off for it, as a background colour clears the frame whatever `autoClear` says). A selected solid's
   copy draws after the see-through surfaces too.
9. **Looks are display state, saved with the project, as hiding is** (0026): in `manifest.json` under `display`
   (`objects`: colouring and opacity where they differ from the default, `layers`: opacity, `contours`: interval,
   lines, shared). Not a revision, not undone, and the project's generation doesn't move, so nothing worked out from a
   project is worked out again. A project saved before this opens with every surface by RL and solid, and a project
   with no looks set writes no `display` at all. A look on a deleted object stays in the session for an undo and isn't
   saved.
10. **Set in three places:** the viewport's RL legend (interval, contour lines, same colours), Properties for the
    selected object (colour, opacity, layer opacity, by slider), and the tree's right-click menu (Colour and Opacity
    for objects, groups, marked rows and layers). A row shows its opacity when it's under 100 %. A slider shows as it
    moves and is kept when it's let go, so a drag is one change. A look change sends back only the looks, not the
    project summary.

## Evidence

**Bands on round RLs**, `app/tests/bands.test.mjs` (`pnpm test`): with the origin at RL 187 and a topography from
123.4 to 271.8, the interval is 10 m, the lowest edge is local z −67, and in f32, as the shader has it, the band number
turns over exactly at RL 130, 140 … 270 and is one less a millimetre below each. Intervals for spans of 0 to 4,000 m,
a set interval, a flat pad and a shared range are checked too.

**Frame rate**, Check 1 of the app bench (`--bench --surfaces`) on the golden pit's surfaces as
`bench\golden_app_bench.ps1` lists them (topography, the large surface and three seams' roofs and floors, 24,958,820
real triangles, tiled copies above that), release app, WebGPU on the same machine, canvas
2400 × 1363, 2026-11-05. `main` and this branch were each run twice, back to back; the last column has all eight
surfaces at 50 % (`--opacity 0.5`), stacked, with the overlay pass on:

| Triangles | `main`, fps | By RL, fps | By RL, all at 50 %, fps |
|---:|---:|---:|---:|
| **25.0 M** | **56.9, 56.9** | **55.4, 55.2** | **44.8** |
| 49.9 M | 41.3, 41.4 | 40.1, 39.9 | 33.2 |
| 74.9 M | 31.0, 30.9 | 30.4, 30.2 | 26.0 |
| 124.8 M | 21.1, 21.2 | 20.8, 20.7 | 18.5 |

The median frame at 25 M is 16.7 ms in all of them but the last (17.0 ms). Colour by RL costs about 3 %; eight
stacked see-through surfaces about 21 %, the cost of blending each pixel up to eight times. `main` ran at 59.5 fps
in Phase 0 (`bench/RESULTS.md`); it was 56.9 today on the same build path, with Tim's own copy of the app open.

**PIT B in the app** (`--pictures`, a new bench mode that imports as the app does, runs through the looks and saves the
view): the mined-out topography, 4UU roof, 4LL floor and the 4U resource strings, on WebGPU and again on WebGL2
(`--webgl`), which draw the same. Bands and lines land on the benches; the topography at 50 % shows the roof and floor
under it with the strings on top at full strength; all three at 50 % show no tile patchwork once surfaces sort whole.

**The palette**, `--pictures <folder> --palettes` (`app/src/bench/pictures.ts`): the same PIT B surfaces and strings,
in four views (topography alone, everything, a close-up of the contours, plan), each drawn on every ramp from one
camera, saved one by one and as a sheet, a row per ramp. The five: batlow (as it was), a turbo rainbow (Google's
polynomial, trimmed to 0.06 to 0.94 so its ends are blue and red, not near black), hypsometric terrain tints (greens,
tans and browns, then pale), Spectral, and a rainbow at one OKLCH lightness and chroma in even hue steps. How far each
band is from the next with 20 bands (OKLab distance, about 0.01 just seen side by side, 0.02 plain at a glance;
`app/tests/rlPalettes.test.mjs`):

| Ramp | Nearest bands | Farthest bands | Mean distance from viridis |
|---|---:|---:|---:|
| Batlow | 0.031 | 0.039 | 0.180 |
| Rainbow (turbo) | 0.025 | 0.095 | 0.322 |
| Terrain | 0.023 | 0.100 | 0.272 |
| **Spectral** | **0.014** | **0.097** | **0.349** |
| Even rainbow | 0.024 | 0.025 | 0.250 |

Batlow and the even rainbow step evenly; the even rainbow, with no change of lightness, came out drab on the pit.
Turbo was the brightest and Terrain read as a map but left the topography mostly brown.

## Known limits

- **Spectral's pale middle has its closest bands**, either side of the pale yellow stop half way up the ramp: 0.014
  apart with 20 bands, against 0.03 to 0.1 elsewhere. A contour line still marks each edge; with the lines off, two
  bands there can look like one.
- **See-through surfaces sort whole.** Where two see-through surfaces cross (a roof left in the air over a mined-out
  pit floor, both see-through), the one drawn later shows on top everywhere. Order-independent transparency would fix
  it, at the cost of more render targets and a composite pass; not done until it's needed.
- **One interval for everything once it's set.** Auto picks per object; a set interval applies to all.
- **Bands narrower than a few pixels blend into the smooth ramp**, on purpose, so a zoomed-out pit wall doesn't shimmer;
  it reads softer than the flats.
- **The pictures are of the 3D view.** The legend, the Look section and the menus were built and typechecked but not
  seen running by the session that made them: Tim declined screen control, as his own copy was open.

## Berms on band edges (2026-11-06)

Tim, in the release with everything merged, on his step template: "What's going on with the colours? On the flat
berms." Every flat berm of that face is on a 5 m band edge (RL 115, 130, 145 and 155: a design's berms are on round RLs
almost always). The engine has them at 115.000, but the GPU interpolates z in f32 across each triangle and lands a hair
either side of the edge pixel by pixel, so `floor` put the berm in two bands at once: diagonal streaks of two colours,
following how the triangles are filled. With contour lines on, the whole berm was within a line's width of the edge
and drew dark. The PIT B pictures never showed it: nothing natural lies flat on a round RL.

- **On an edge is in the band above,** to a millimetre (`ON_EDGE` in `bands.ts`): the shader adds it before dividing,
  and `bandsFor` counts a lowest or highest RL within it as on the edge, so a top berm on an edge has a band of its own.
  A millimetre is well past the GPU's rounding at a local z of a kilometre (about 0.06 mm a step) and too small to see.
- **A flat draws no contour line across it.** The line's width comes from how fast the band number changes across a
  pixel, which on a flat is nothing or the GPU's rounding, and a smoothstep with both ends at zero is undefined on the
  GPU. That change is now taken as at least a millionth of a band (`MIN_PER_PIXEL`), which a berm a millimetre into its
  band is well clear of at every interval offered: the line runs along the crest of the wall below and the toe of the
  wall above, where walls cross the edge.
- **Tests** (`tests/bands.test.mjs`, the shader's sums in f32): the template's four berms with the scene origin at five
  whole metres from -845 to 1,100, each up to 4 f32 steps either side of its RL, all land in the band above; the rule
  before put 19 of those 20 in two bands. No line across a berm on an edge at every interval offered, at local z out to
  about a kilometre either way; a full line on a wall 1 cm either side of it.
- **Pictures** (`--bench --pictures`, the template's face as DXF): before, the berms streak and the ones in contour view
  draw dark; after, each berm is one colour with the line along its edges. The legend for that face now reads 100 to
  160 RL, not 155: the berm at 155 has its own band.

## Contour lines off by default (2026-11-09)

Tim, 2026-11-09: "we need to have the contour lines not on by default for the surface coloring". A surface or grid coloured
by RL now shows its bands alone; the **Contour lines** tick in the RL legend turns the lines on, for every surface at once,
as before. Item 1 stands but for that: the lines are an option, off unless turned on.

- **Saved as `edge_lines`**, not `lines`, in `display.contours` in `manifest.json`. A project saved while the lines were on
  by default wrote `"lines": true` whether anyone chose them or not, and the two can't be told apart, so the old key is
  passed over: every project saved before opens with the lines off, and its interval, shared range and looks as saved. A
  project saved since keeps the lines on if they were turned on. An older build reading a newer project finds no `lines`
  and draws them, as was its default.
- **Tests** (`crates/zd-io/tests/store.rs`): a manifest with `"lines": true` opens with them off and everything else as
  saved; a new project's are off. Spectral's pale middle (Known limits) has its closest bands with no line between them.
- **Pictures** (`--bench --pictures`): `04-close-up-bands` is now the bands alone, as a project opens, and
  `04b-close-up-contour-lines` the same view with the lines turned on.
