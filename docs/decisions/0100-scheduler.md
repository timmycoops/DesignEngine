# 0100 — The scheduler: medium to long term, dependencies from the table's levels, a coal blender, Spry imported

Date: 2026-11-22 · Status: accepted (Tim, 2026-11-22: the twelve decisions of the Build Plan's "Scheduler plan" tab with
his changes, then "Agree all 12" on its "Dependencies plan" sub-tab)

## Why

Tim, 22 November:

> With the reserving and dataflow capability built out as an MVP i need to explore the next step which is converting the
> reserves into a schedule and visualing the results. Obviosuly this is a significant nexty step on the journey so we
> need to plan accordingly and only set off when we have enough of the first step completed you feel to build something
> thats useful and not require a full re-work.

Then: "Defintley want to focus on the medium to long term horizon - with the key being running scenarios fast and
comparing outputs in a format that allows you to make a more informed understanding of the levers that move the plan."
And: "this is a coal focused product and i would like to have a Coal blender as a default solution built into the
Schedule ouptuts so we can create products that hit a blend target - thats important."

On dependencies: "dependencies should be he pri.ary driver to e sure we get good schedules and reliable outcomes for the
levers changes when run different scenarios", "We don't need to copy the spry logic", and the model he set out: "this
is jus the levels in the table - a pit might include strips and blocks - but block 1 -5 might be able to mined as one
group and 6-10 another group. Then simple Drill > Load > Blast > Dig at the Blast horizon".

Until now CLAUDE.md said scheduling stays in Spry, and v1 had no scheduling. The reserves table already carries a
`Scheduled` folder that copies ROM "until scheduling sets it" (0074); nothing sets it yet.

## Decision

### The scheduler (Scheduler plan tab)

1. **Scheduling comes into the product** as its own licence module, `schedule` (0098), for the medium to long term
   (months to years). Short-term operational scheduling, detailed haul networks and dumps stay in Spry. CLAUDE.md's
   scope lines change with this record.
2. **Stage A first: see Spry's schedule.** Import a `.spry` project's schedule with the tables it uses (tasks, periods,
   fleet, dependencies, its data tables, which 0039 left out), and build every picture and period table against Spry's
   own answer before writing a sequencer.
3. **A basic rule-based sequencer first** (stage B): event-driven list scheduling, priorities, releases, limits, and a
   mode that chases period targets; deterministic and explainable. An optimiser is a firm stage C behind the same
   interface (tasks, links and settings in, the same event log out).
4. **The event log is the record**: what each fleet unit dug, from when to when, and why. Period tables of any length are
   cut from it without a rerun. Dates sit on a calendar, not period numbers.
5. **A task is a process on a leaf.** Wherever the dataflow gives a leaf a driving quantity for a process (drill metres,
   explosive tonnes, BCM blasted, dig BCM or coal tonnes), that is a task, worked by equipment that can do it at its
   rate. Quantities come from the reserves table and are never worked out again, so scheduled totals equal the reserves
   by construction; the run sets the `Scheduled` folder.
6. **Dependencies are the main driver** of a good schedule and of lever comparisons that can be trusted. They have their
   own plan, below.
7. **Cases are a base plus lever changes**, run in parallel. Compare is the main screen: periods overlaid, a bridge from
   case to case, a tornado of levers, blocks coloured by how many periods earlier or later. Every key output is also a
   full table in the one grid (0099), exported to CSV, Excel and the HTML report (0068), with saved report setups.
8. **A coal blender is in every run's outputs, on by default**, and a case can turn it off. It reads blend sources,
   washability (yield–ash) and products tables filled by dataflows, scripts and formulas through a field mapping, with
   default curves from a formula, named as defaults. Each period it gives two answers:
   - the **ideal**, a linear programme with density fractions taken lightest first, so several sources into one product
     end cut at the same incremental ash;
   - a **campaign plan** the plant can run: feed group × standard density × product as options sharing the plant's
     hours, with a minimum campaign length and density changes as small yes/no choices.
   The gap between them, and the rule causing it, is reported, and each spec's cost in product tonnes comes from the
   solver. Plant efficiency is a factor per product and/or feed source in a parameters table (0072, first row that
   fits), on yield only. One product per plant setup is the default; two from one wash is possible. Stockpiles run
   period by period at a running-average quality. The prep plant is in the assumptions (plant setups, capacity from the
   calendar, moisture stated at each step, a mass balance a period, a flowsheet in React Flow). One wash a module first;
   size-fraction circuits later.
9. **New dependencies for speed and reporting**, each named here: HiGHS through the `highs` crate (the blender's linear
   programmes, MIT), Apache ECharts (charts, Gantts, Sankey, bridge, tornado; Apache-2.0), `rayon` (cases in parallel),
   `rust_xlsxwriter` (Excel output; MIT or Apache-2.0), and `chrono` becomes a direct dependency (it is already in
   `Cargo.lock`). The stage C optimiser uses the same HiGHS. The engine is a new crate, `zd-sched`: no UI, f64, the same
   inputs give the same schedule. Commands: `schedule.import_spry`, `schedule.dependencies`, `schedule.run`,
   `schedule.blend`, `schedule.cases`, `schedule.periods`.
10. **When to set off:** stage A after the Phase 4 gate; stage B after the Phase 5 gate with Tables ws3 (entered columns)
    and ws5 (calculated columns). A difference from Spry can't be chased while the reserves themselves still differ.
11. **Spry's dataflows import with the schedule** and convert to our formulas (Starlark, 0077). Each converted step sits
    beside Spry's original, anything not converted is flagged and listed, and both run on the same blocks and
    reconcile. The conversion is our own code on the machine: nothing from a client project leaves it.
12. **Truck haulage is a simple constraint a case switches on.** The input is a total cycle time set a block or a period
    (first row that fits, so a block's own value beats its period's); truck hours a period come from truck classes and
    the calendar; truck-loaded digging waits when a period's hours run out, and the report says where and by how much.
    No haul or dump calculation in the first build. Worked-out haul profiles and haulage as an output come later and
    fill the same cycle time column.

### Dependencies (Dependencies plan tab)

Not Spry's eight types and path syntax: dependencies come from the table's levels and a few attributes.

1. **A process exists wherever a leaf has its driving quantity** from the dataflow. Processes are a table (name, its
   quantity field, unit, place in the order); equipment works the processes it can, at its rates.
2. **Dependencies come from the levels** (pit > strip > block > seam or bench) and a few attributes set by a script, a
   dataflow or the grid: mining group, blast group, blast horizon. Each strip's block order (1 to 10, 10 to 1 or any
   list) is set in the dependency. No paths, wildcards, spans or arrays.
3. **Three orders carry most of it:** the process order in a blast unit (drill, charge, blast, dig; Tim's "Load" is
   charging), the seam and bench order down the stack, and the group order along the strip, for digging only (drilling
   and charging may run ahead).
4. **Groups set the mining unit; blocks keep their cut interval** (such as 100 m) for quantities, qualities and
   reporting. Grouping is one step (every n blocks, by length, at split lines, by formula, in the grid), can differ by
   horizon, and is a lever: changing it relinks tasks and never re-cuts a block. The blast group, the blocks blasted at
   one time, is a grouping like any other.
5. **A blast takes the blast unit's volume** (blast group × blast horizon) at the assigned blast equipment's rate; each
   block's dig waits for its blast.
6. **Leads are hard minimums between fronts by default**, which makes them dependencies; a target above the minimum is
   optional (the lagging fleet favoured, periods under target flagged). Metres, blocks, days and tonnes of coal exposed
   are all available. A front is measured in plan along the strip, a part-dug block pro rata.
7. **Fleet limits** are a constraint set per mine and fleet type: how many units may work at once in a strip, a group or
   a horizon (one dragline a strip, two excavators a group).
8. **Across strips, holds and links by hand are extras**, each a row switched on when wanted. Lags: after finish (the
   default), after a share done, or a number of days after.
9. **Geometry is an overall option:** off (the default), check only (digs under undug material in plan flagged after a
   run), or on (those overlaps become links). Real dips and reserve shapes make it noisy as the main control.
10. **In the schedule:** binding links (the link that actually held a block back), trace from a picked block, settings
    ranked by the waiting they caused, the why panel's reasons in order (a block, a fleet, a limit, haulage), the link
    table in the one grid, the chain in the Gantt. No arrows until a block or a setting is picked.
11. **Built once a case into compact arrays** (each task's successors and a count of what it waits on); a change to a
    setting or a grouping rebuilds only what it touches; the same settings always make the same links in the same
    order, so a difference between cases is the lever, never noise. Cycles, unreachable blocks and processes with no
    equipment are refused before a run, named.
12. **Spry import:** Spry's process order and sequence dependencies map onto ours; its ranges and grouping expressions
    become groups and blast horizons where they line up; the rest come in as links by hand, named by the Spry type that
    made them, with counts a type.

Workstreams, in the tabs' order: A1 import Spry's schedule and tables; A2 event log and period tables from it; A3 colour
by period, slider, period chart, Gantt; A4 Spry dataflows converted; B1 dependencies (D1 engine, D2 grouping, D3 leads
and lags, D4 in the view and the schedule, D5 Spry import, D6 geometry option); B2 calendar, fleet and rates; B3
`schedule.run`; B4 the ideal blend; B5 the campaign plan; B6 the plant and stockpiles; B7 truck haulage; B8 cases; B9
compare; B10 reporting. Stage C (worked-out haulage, an optimiser to targets and blends, piles across the horizon,
size-fraction circuits, dragline detail) gets its own plan when B is in use.

## Built

Nothing yet. Stage A waits for the Phase 4 gate.

## Known limits

- Open on the Scheduler plan tab: which PIT B Spry schedule stage A reads; which fleet comes first in stage B; physical
  levers only or cost and value too; campaign rules for a first real case; an outloading limit in the first build; a
  period's coal as a cap or a target; the period lengths planned in.
- CSN, fluidity and HGI don't blend linearly: they are blended as weighted averages and marked approximate, or held by a
  source rule.
- The plant efficiency factor scales yield only; qualities stay at the washability values.
- Stage B blends what the schedule dug; scheduling to the blend is stage C.
- Seam work is judged on PIT B; on client data, imports and reconciliations print counts and differences only.
