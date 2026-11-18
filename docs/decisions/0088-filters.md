# 0088 — One filter by attributes and measures

Date: 2026-11-18 · Status: accepted (Tim, 2026-11-18: decisions 1 to 9 of the Filtering plan) · Workstream 1 built; the
later workstreams add to this record as they merge

## Why

Tim, 18 November: "When do we get filtering?" The main use is filtering a layer that's on the screen, the tables too,
"but I want to easily be able to filter by the attributes. I'm pretty sure we had this on the workflow." A workflow step's
input could only be filtered by one exact value an attribute, made by capture, and nothing filtered what the view draws.
The bench rules and parameter tables already had a filter syntax (0071, 0072). The Filtering plan (Build Plan, "Filtering
plan" tab) makes it the one filter everywhere: on a layer in the view, over the tables, and on a step's input. Its
workstream 0, the layers-only tree, is 0087.

## Decision

The plan's nine decisions, as agreed:

1. **Syntax**: the bench rules' syntax as it is (`strip: 1-5; horizon: U-M, M-L`: comma lists of words, numbers and
   ranges, `*` or blank for any, names and words with their case aside), plus comparisons (`thickness: <0.3`, `<=`, `>`,
   `>=`) and not (`seam: !WASTE`). Bench rules and parameter tables take them too.
2. **Measures** filter like attributes: `volume`, `area`, `length`, `rl_min`, `rl_max` (so `volume: <500` picks the
   slivers to delete).
3. **A layer's filter** shows only what fits on that layer and the layers under it; what doesn't is hidden by the filter,
   as hidden is (0026): not drawn, snapped to or cut by a section, and a command that names it still takes it.
4. **Set from the tree**: right-click a layer → Filter by attributes…, ticks of the values its objects carry with a count
   each, and the filter as text under them.
5. **It shows**: a funnel on the layer's row with the filter in words and how many show; Clear filter on the right-click.
   Display state saved with the project, not undone.
6. **Select what's shown**, so a run takes exactly the filtered set.
7. **Tables**: the Objects table and the Reserves table get the same filter box; totals and CSV follow it.
8. **Workflow**: a step input's filter takes the same syntax and is edited on the step; one saved before reads as it did.
9. **Isolate a layer**, from the String tools plan, on the same right-click menu.

## Built: workstream 1, the engine

- **Comparisons and not** (`crates/zd-core/src/filters.rs`). An item starting `<`, `<=`, `>` or `>=` and then a number
  compares a value that's a number, or text that reads as one; it never fits a word, and `<abc` with no number is read as
  a word. Items starting `!` are values it may not hold. Of the items without `!`, one must fit, so a value that isn't
  there fits none, as before; of those with `!`, none may, so a value that isn't there fits `!WASTE`: it isn't waste.
  `*, !WASTE` is anything but waste. The app's twin (`app/src/panels/benchRules.ts`, `listFits`), which counts what each
  bench rule takes before a cut, reads them the same.
- **Measures** (`measure`, `object_value`, `object_fits`). A name an object carries no attribute of may be a measure: a
  solid's `volume` (m³, the volume of record); a string's `length` (m, along it in 3D); a closed string's `area` (m², in
  plan) or a surface's (m², over its triangles); and `rl_min` and `rl_max` of anything with heights, a grid's over the
  nodes that have one. An attribute of the same name is what's read. A measure an object hasn't (a solid's `length`)
  fits no value, as a missing attribute fits none.
- **A step's input** (`input_filter_fits`, used by `resolve_in`, capture and the workflow report). Its filter is saved as
  before, a value an attribute: a word is now read as a filter's list, so `1-5`, `<500` and `!WASTE` work there, on
  measures too; a number or a yes/no is that value, so every workflow saved before finds what it did. Capture checks its
  rule as a run reads it, so an attribute whose value reads otherwise as a filter (a word such as `1-5`) is passed over
  for one that finds the same objects.

Bench rules and parameter tables read attributes, never measures: a parameter row is matched against a unit's
attributes, and a bench rule against a piece's.

| Test | Known answer | Tolerance |
|---|---|---|
| `comparisons_take_numbers_and_never_words` (zd-core) | 0.25 fits `<0.3` and `<=0.25`, not `<0.25`; text `120` fits `>=120`, not `>120`; `<5` never fits the word U; `<abc` fits the word `<abc`; nothing fits `<1` where there's no value | exact; numbers within 1e-9 |
| `not_takes_values_out_and_fits_what_is_not_there` | `!waste` fits Coal and no value, not WASTE; `*, !WASTE` the same; `Coal, Parting, !WASTE` doesn't fit no value; strip 4 fits `1-10, !5-6`, not `1-10, !4` | exact |
| `measures_filter_as_attributes_do` | a 10 × 20 × 2.5 m block at mine coordinates, floor RL 100: 500 m³, RL 100 to 102.5, fits `volume: <=500`, not `<500`; an attribute named `volume` wins; a 3-4-5 string climbing 12 m is 13 m; a closed 30 × 40 m ring 1,200 m², 2 × (√925 + 40) m round; a 10 × 20 m flat surface 200 m²; a grid with two empty nodes RL 70 to 80 | 1e-6 m³ and m²; 1e-9 m |
| `a_workflow_input_reads_a_word_as_a_filter_and_a_number_as_itself` | strip 3 fits a saved 3, not 4; fits `1-5` with material `Coal` (case aside); `!waste` with `volume: >=1000` on a 1,000 m³ block; not `volume: <1000` | exact |
| `a_step_input_filters_by_the_filter_syntax_and_measures` (zd-commands) | the box pit cut at RL 40: `volume: <300000` finds the 240,000 m³ piece alone, `rl_min: >=40` the 360,000 m³ piece, `volume: !240000` the same, a saved 240000 the piece below; `<1` finds nothing and says the rule | 1e-6 m³ |
| `benchRules.test.mjs` (app) | the twin reads the same items the same way, and a rule typed `thickness: <0.3; seam: !P` takes 0.2 m of U, not of P, not 0.4 m | exact |

## Known limits

- **Measures cost a pass over the object** each time one is read: fine for a step's input or a table, to be timed when a
  layer's filter runs over a pit's solids in the view (workstream 3).
- **A solid has no `area`**, and a surface no `length`: they're not asked for yet.
