# 0002 — Industry research: how the incumbents do it

Date: 2026-10-28 · Status: accepted. Full write-up with every source is the "Industry research" tab of the
build plan doc. This is the condensed version for building commands.

## Decisions taken from the research

| Area | What the industry does | What we do |
|---|---|---|
| Geology | Coal is a stack of seam grids per horizon (Vulcan ISM, Minex, MineScape prism models, MinePlan GSM, Deswik surface stacking) | `SeamModel` object (see CLAUDE.md) |
| Reserves | 2.5D columns in vertical-walled polygons with exact partial cells (Vulcan Rsvute subdivides polygons up to 32×); solids at the battered wall (Vulcan Batvol) and for scheduler export (Vulcan Split Pit, XPAC); Deswik does both | Solids are the volume of record; grids give qualities; a column calculation reconciles |
| Limits | Vulcan's grid workflow requires dip < 30°; grid fault methods "are not suited to modelling reverse or thrust faults"; faulting applies "to structural surfaces; never to thicknesses" | v1: dip < 30°, normal faults as fault-block domains |
| Face design | Project a string at a batter to an RL or surface, berm, repeat. Values per segment (Vulcan W-tag, Deswik rules returning one value per vertex). Coal horizons carry berm on roof and berm on floor, min/max bench height (Vulcan Create Pit/Highwall Surfaces); face/side/end walls (Vulcan Benching & Batters) | `Rule` + `FaceProfile` objects |
| Offsets | Everyone offsets then repairs crossovers, corner "butterflies", spikes and slivers | Weighted straight skeleton (Held & Palfrader) for exact mitred offsets |
| Ramps | Width, grade on inside edge by default, taper, berm exits, flat or graded switchbacks; vendors document failures to reach grade | `Ramp` object that reports the failing bench |
| Automation | Vulcan APD, Datamine Auto Pit Design, Deswik.SPD and RPMGlobal APD regenerate a whole pit from seed + rules; blending only widens berms and flattens slopes (Datamine) | A saved face profile re-run over chosen lines, target < 1 s per pit |
| Validity | Every package refuses volumes on invalid solids; Evo schema flags `Manifold`, `ConsistentWinding`, `NonDegenerate`, `NonSelfIntersecting`, `Continuous` | Explicit flags + repair on import |
| JORC | In situ moisture and density by Preston–Sanders; RD by thickness, raw quality by mass, clean coal by mass × yield | Reserves conventions in CLAUDE.md |
| Technology | Incumbents are C++ or .NET on OpenGL 3.3–4.6; none uses Rust or WebGPU; open data is moving to Parquet with f64 (OMF2, Evo) | Stack confirmed |

## Deswik 2025.2 engine (from the API reference in `D:\Deswik Helper\API Reference\`)

- **Pit design elements:** projection, berm, ramp, flat switchback, graded switchback. Outputs a closed
  solid, a full pit surface and a surface cropped to topography.
- **Rules** (`Deswik.Graphics.PitDesign.Rules`): fixed, fixed list, faces, nearest, inside polygon, inside
  solid, inside geotech solid (transition distance), block model (lowest / midpoint / highest / transition
  value), segment azimuth, azimuth inside solid, azimuth by model; optional smoothing. Each returns a list of
  values along the string.
- **Ramps** (`RampConfigurationData`, `Ramps.Ramp`): width; gradient with a dip convention; inside and outside
  ratio and taper; gradient option closest to base / centre / furthest / longest / shortest; maintain full
  road at start, end or both; flat switchback ratio and inside radius; graded switchback radius with radius to
  grade on inside / centre / outside; switchback shape default / rectangular / smooth; ramp face angle rule;
  entrance chamfered or flat pad. Errors: invalid inputs, no ramp for start point and gradient, ramp does not
  hit destination contour. Auto ramps use A* on a gridded map with exclusion zones.
- **Berms:** graded and inclined (peak, valley, slope), berm angle, maximum berm width and bench height,
  drainage validation, taper distance, no expansion when berm width is zero.
- **Tolerances:** 0.1, 0.01 and 0.001 m; fillet angles for projections and offsets; simplify tolerance;
  minimum intersection-polygon area.
- **Strip Designer** (`Deswik.Tools.StripDesigner`): at each point and azimuth, a rule list of
  "project to elevation at angle, Up / Down / UpOrDown, limited to surface" and "berm of width", left or right.
- **Surface stacking** (`Deswik.Graphics.SurfaceStacking`): ordered surfaces with material, horizon, parent
  horizon, density field, qualities flag, limiting surface; `<<Upper>>` / `<<Lower>>` identifiers. Produces
  closed solids cut by block polylines and grid column reserves per cell and horizon (area, top and bottom
  RL, thickness, tonnes, qualities, masked flag). Cut Solids by Seams by intersection or cutting, keeping
  invalid results apart. Grid cell elevations are stored as 32-bit floats: a precision risk we avoid.
- **Block cutting** (`CutBlocks.BlockSolidsRule2`): base polylines, cut by surfaces via a stacking rule, cut by
  benches (height, min and max RL), minimum volume, area and thickness.
- **Projection to a surface** looks graph-based (nodes, edges, shortest-path fields in `ProjectionBruteForce`).
  Inferred from class names; test it in Phase 3.

## Key public references

- Vulcan ISM: https://help.maptek.com/vulcan/2026/topics/GridCalc/IntegrateStratModeling/GridCalc_model_strat_Method.htm
- Vulcan Rsvute: https://help.maptek.com/vulcan/2026/topics/Rsvute/Introduction/Rsvute_01INTRO.HTM
- Vulcan highwall horizons: https://help.maptek.com/vulcan/2026/topics/OpenPit/Integrated_Stratigraphic_Planning/OpenPit_CreatePitHighwallSurfaces_Horizons.htm
- Vulcan run-of-mine model: https://help.maptek.com/vulcan/2026/topics/GridCalc/Composite_Models/GridCalc_CreateRunOfMineModel.htm
- Datamine conditioning: https://docs.dataminesoftware.com/StudioOP/Latest/STUDIO_OP/OP%20Task%20String%20Conditioning.htm
- JORC Coal Guidelines 2014: https://jorc.org/docs/Coal_Guidelines_2014_-_Final_Ratified_Document.pdf
- Weighted straight skeletons: https://ar5iv.arxiv.org/html/1604.03362
- OMF2 format: https://gmggroup.github.io/omf-rust/format.html · Evo schemas: https://github.com/SeequentEvo/evo-schemas
