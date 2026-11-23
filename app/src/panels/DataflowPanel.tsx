import { RotateCw } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useState } from 'react';
import { type ColumnSteps, type Dataflow, type ParameterTable, reservesColumn, reservesColumnTrace, reservesRerunParams } from '@/bridge';
import { Choice, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { type Draft, useWorkspace } from '@/workspace/context';
import type { PanelId } from '@/workspace/panels';
import { ColumnStepsView } from './ColumnStepsView';
import { unitWords } from './columnSteps';
import { DataflowEveryBlock } from './DataflowEveryBlock';
import { DataflowGraph } from './DataflowGraph';
import { DataflowStrip } from './DataflowStrip';
import { UnitTable } from './DataflowTables';
import { beforeAfter, type RerunRange, rerunOnly } from './dataflowView';
import { cellSummaries, defaultFocus, type Focus, focusAt, focusOf } from './notebook';
import { type CellTrace, StepPane } from './StepPane';

/**
 * The Dataflow tab (decision 0084, layout B): a block's column through its dataflow. Across the top the column's inputs
 * and its steps in run order, what each wrote under it and the units it passed on, or the same as a graph; under them
 * three columns: the picked step (what it read for the unit followed, its rule or formula, its workings, what it wrote),
 * and the block's units Before and After it, a unit a row. What the step reads is lit gold in each; what it changed is
 * blue. Opened from the Reserves table's Open its column, and follows the block picked there. Was the Notebook (0081):
 * its cells are the step pane now, one at a time; Whole column shows today's grid at the step; a formula's Edit opens it
 * in the Parameters tab.
 */

const keyOf = (path: string[]) => path.join('\u0001');

/** Whether a step itself is edited: its formula, filter or rule as run on the edits isn't as saved. */
function stepChanged(saved: ColumnSteps, live: ColumnSteps, at: number): boolean {
  const s = live.steps[at];
  const was = saved.steps[at]?.step === s.step ? saved.steps[at] : saved.steps.find((x) => x.step === s.step);
  return !was || JSON.stringify([was.source, was.when, was.rule]) !== JSON.stringify([s.source, s.when, s.rule]);
}

/** How long after the last keystroke the column runs again on the edits (decision 0082, point 4). */
const LIVE_MS = 330;

/** A run on unsaved edits, with what it was run on (for the trace) and a count (to keep its traces apart). */
interface LiveRun {
  column: ColumnSteps;
  dataflow: Dataflow | null;
  tables: [string, ParameterTable][];
  n: number;
}

/**
 * The Parameters tab's unsaved edits a column reads: its own dataflow as edited, and the tables it reads (by name, as the
 * run names them). `key` changes with them; `why` is the first that doesn't check, and stops the run.
 */
function editsFor(column: ColumnSteps, drafts: Record<string, Draft>) {
  const flow = Object.entries(drafts).find(([id, d]) => d.kind === 'dataflow' && id === column.dataflow_id);
  const tables = Object.entries(drafts).filter(([, d]) => d.kind === 'table' && column.tables.includes(d.name));
  const used = [...(flow ? [flow] : []), ...tables];
  if (used.length === 0) return null;
  const stuck = used.find(([, d]) => d.why);
  return {
    key: JSON.stringify(used.map(([id, d]) => [id, d.kind === 'table' ? d.table : d.dataflow])),
    why: stuck ? `${stuck[1].name}: ${stuck[1].why}` : null,
    dataflow: flow && flow[1].kind === 'dataflow' ? flow[1].dataflow : null,
    tables: tables.flatMap(([id, d]): [string, ParameterTable][] => (d.kind === 'table' && d.table ? [[id, d.table]] : [])),
  };
}

/** The tabs a planner reads beside the Dataflow tab: the parameters its steps read, and the table whose block it follows. */
const BESIDE: readonly { id: PanelId; title: string }[] = [
  { id: 'params', title: 'Parameters' },
  { id: 'restable', title: 'Tables' },
];

export function DataflowView() {
  const { summary, notebookOf, openTool, openNotebook, placeBeside, isBeside, drafts, runRecord } = useWorkspace();
  // Bumped as a tab is put beside or back, so the buttons say which way they go next.
  const [, setLaidOut] = useState(0);
  const table = useMemo(() => (notebookOf ? (summary?.objects ?? []).find((o) => o.id === notebookOf.table) : undefined), [summary, notebookOf]);
  const node = notebookOf?.node ?? [];
  // A formula's Show in Dataflow: the step to pick.
  const shown = notebookOf?.edited ?? null;
  // The column run on what's saved, and on the Parameters tab's unsaved edits (decision 0082): the last run that worked.
  const [saved, setSaved] = useState<ColumnSteps | null>(null);
  const [live, setLive] = useState<LiveRun | null>(null);
  const [liveWhy, setLiveWhy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [whole, setWhole] = useState<number | null>(null);
  const [traces, setTraces] = useState<Map<string, CellTrace>>(new Map());
  // The step picked in the strip, the strip drawn as a graph, and every field in Before and After.
  const [picked, setPicked] = useState(0);
  const [graph, setGraph] = useState(false);
  const [all, setAll] = useState(false);
  // The picked step for every block of the table at once, in a grid (Tables and scripts plan, decision 10).
  const [every, setEvery] = useState<'one' | 'every'>('one');
  // Run again into the table (decision 0085): how much, whether it's running, and what it said.
  const [range, setRange] = useState<RerunRange>('block');
  const [rerun, setRerun] = useState<{ running: boolean; said: string | null; refused: boolean }>({ running: false, said: null, refused: false });

  // A parameter table or dataflow saved runs the column again: their revisions are part of what's asked.
  const inputsSaved = (summary?.objects ?? [])
    .filter((o) => o.kind === 'parameter_table' || o.kind === 'dataflow')
    .map((o) => `${o.id}@${o.revision}`)
    .join(',');
  const ask = `${notebookOf?.n ?? 0}:${table?.id ?? ''}@${table?.revision ?? 0}:${keyOf(node)}|${inputsSaved}`;
  useEffect(() => {
    let current = true;
    setError(null);
    setWhole(null);
    setTraces(new Map());
    setLive(null);
    setLiveWhy(null);
    if (!table) return setSaved(null);
    void reservesColumn(table.id, node)
      .then((c) => {
        if (!current) return;
        setSaved((was) => {
          // A new column, not the same one run again: the unit followed starts at the node picked.
          if (!was || keyOf(was.column) !== keyOf(c.column) || was.case !== c.case) {
            const below = node.slice(c.column.length);
            setFocus(defaultFocus(c.steps, below.length ? { seam: below[0], bench: below[1] } : null));
          }
          return c;
        });
        // The step a formula asked for; else the step already picked, where this column has it; else the first that did something.
        setPicked((was) => {
          const at = shown ? c.steps.findIndex((s) => s.step === shown.step) : -1;
          if (at > 0) return at;
          if (was > 0 && was < c.steps.length) return was;
          const busy = cellSummaries(c.steps).findIndex((s, i) => i > 0 && !s.quiet);
          return busy > 0 ? busy : 0;
        });
      })
      .catch((e) => current && (setSaved(null), setError(String(e))));
    return () => {
      current = false;
    };
  }, [ask]);

  // The unsaved edits this column reads: its own dataflow's, and the tables it reads, by name.
  const edits = useMemo(() => (saved ? editsFor(saved, drafts) : null), [saved, drafts]);
  // Run on them about a third of a second after they stop changing; an edit that doesn't check leaves the last good run.
  useEffect(() => {
    if (!edits || !table) {
      setLive(null);
      setLiveWhy(null);
      return;
    }
    if (edits.why) return setLiveWhy(edits.why);
    let current = true;
    const t = setTimeout(() => {
      void reservesColumn(table.id, node, edits.dataflow, edits.tables)
        .then((c) => {
          if (!current) return;
          setLive((was) => ({ column: c, dataflow: edits.dataflow, tables: edits.tables, n: (was?.n ?? 0) + 1 }));
          setLiveWhy(null);
        })
        .catch((e) => current && setLiveWhy(String(e)));
    }, LIVE_MS);
    return () => {
      current = false;
      clearTimeout(t);
    };
  }, [edits?.key, edits?.why, saved]);

  const column = edits && live ? live.column : saved;
  const against = column && column !== saved ? saved : null;
  const summaries = useMemo(() => (column ? cellSummaries(column.steps) : []), [column]);
  const at = column ? Math.min(picked, column.steps.length - 1) : 0;
  const tables = useMemo(() => (column ? beforeAfter(column, at, all, against) : null), [column, at, all, against]);
  // The unit followed as it stands going into the picked step (what In and Workings show, whose rows Inputs light), and
  // as the step left it (After's row).
  const into = column && focus && at > 0 ? focusAt(column.steps[at - 1].units, focus) : null;
  const outOf = column && focus ? focusAt(column.steps[at].units, focus) : null;

  // The picked formula step traced on the unit followed, on what the column was run on; kept by run, step and unit.
  const onLive = !!live && column === live.column;
  // An allocation step is traced on the whole column, whichever unit is followed (Waste allocation plan, decision 4).
  const allocation = !!column && at > 0 && column.steps[at].kind === 'allocation';
  const traced = allocation ? (column!.steps[at - 1].units[0] ?? null) : into;
  const wanted = column && at > 0 && (column.steps[at].kind === 'formula' || allocation) && traced ? `${ask}|${onLive ? live!.n : 0}|${at}:${allocation ? 'column' : traced.id}` : null;
  useEffect(() => {
    if (!table || !wanted || !traced || traces.has(wanted)) return;
    setTraces((m) => new Map(m).set(wanted, { state: 'loading' }));
    void reservesColumnTrace(table.id, node, at, traced.id, onLive ? live!.dataflow : null, onLive ? live!.tables : [])
      .then((trace) => setTraces((m) => new Map(m).set(wanted, { state: 'done', trace })))
      .catch((e) => setTraces((m) => new Map(m).set(wanted, { state: 'failed', why: String(e) })));
  }, [wanted]);
  const trace = wanted ? (traces.get(wanted) ?? { state: 'loading' }) : null;

  // The table's blocks in the range run again into it with what's saved: the table's next revision, which the tab and the
  // Reserves table then show.
  const runAgain = async () => {
    if (!table || !saved) return;
    setRerun({ running: true, said: null, refused: false });
    try {
      const only = rerunOnly(saved.column, range);
      const reply = await runRecord('reserves.run', await reservesRerunParams(table.id, only));
      const words = only.length ? only.join(' › ') : 'every block';
      setRerun(reply.error ? { running: false, said: reply.error, refused: true } : { running: false, said: `${words} run again into ${table.name}.`, refused: false });
    } catch (e) {
      setRerun({ running: false, said: String(e), refused: true });
    }
  };
  const rangeWords = (r: RerunRange) => {
    const path = saved?.column ?? [];
    return r === 'block' ? `block ${path[path.length - 1] ?? ''}` : r === 'strip' ? `strip ${path[path.length - 2] ?? ''}` : 'every block';
  };
  const runAgainControls = (
    <span className="flex items-center gap-1" data-slot="run-again">
      <Select className="h-6 w-32" aria-label="How much of the table to run again" value={range} onChange={(e) => setRange(e.target.value as RerunRange)}>
        {(['block', 'strip', 'all'] as const).map((r) => (
          <option key={r} value={r}>
            {rangeWords(r)}
          </option>
        ))}
      </Select>
      <Button
        size="sm"
        variant="outline"
        className="h-6 px-2 text-xs"
        disabled={rerun.running || !saved}
        onClick={() => void runAgain()}
        title="Run the reserves dataflow again for these blocks, on what's saved, into this table: their rows replaced, the rest kept, the table's next revision (one undo)"
      >
        <RotateCw className={cn('size-3.5', rerun.running && 'animate-spin')} /> {rerun.running ? 'Running…' : 'Run again'}
      </Button>
    </span>
  );

  // ← and → step through the strip, away from a box being typed in.
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!column || e.ctrlKey || e.altKey || e.metaKey) return;
    if ((e.target as HTMLElement).closest('input, select, textarea, [contenteditable="true"]')) return;
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    setPicked(Math.max(0, Math.min(column.steps.length - 1, at + step)));
  };

  // Tabs to read beside the Dataflow tab, each its own half of the dock, and back as a tab with a second press.
  const besideButtons = (
    <span className="flex gap-2" data-slot="notebook-beside">
      {BESIDE.map(({ id, title }) => {
        const beside = isBeside(id, 'notebook');
        return (
          <button
            key={id}
            type="button"
            className="whitespace-nowrap text-muted-foreground hover:text-foreground hover:underline"
            title={beside ? `Put the ${title} tab back behind the Dataflow tab` : `The ${title} tab beside the Dataflow tab, each with half the room, to read both at once`}
            onClick={() => (placeBeside(id, 'notebook'), setLaidOut((n) => n + 1))}
          >
            {beside ? `${title} as a tab` : `${title} beside`}
          </button>
        );
      })}
    </span>
  );
  if (!notebookOf || !table) {
    return (
      <div className="space-y-2 p-3 text-xs text-muted-foreground" data-tool-panel="notebook">
        <div>Pick a block in the Reserves table, or a seam or bench of one: its column shows here through its dataflow, a step at a time, what went in, what ran, and its units before and after. The Dataflow tab follows the block picked there.</div>
        {besideButtons}
      </div>
    );
  }
  if (whole !== null) {
    return (
      <div className="flex h-full min-h-0 text-xs" data-tool-panel="notebook">
        <ColumnStepsView table={table} node={node} backLabel="Dataflow" startStep={whole} dataflow={onLive ? live!.dataflow : null} tables={onLive ? live!.tables : []} onBack={() => setWhole(null)} />
      </div>
    );
  }
  // The units to follow: every unit of the column as it comes in, then any a step made.
  const choices = column ? [...new Map(column.steps.flatMap((s) => s.units).map((u) => [u.id, u])).values()] : [];
  const step = column?.steps[at];
  return (
    <div className="flex h-full min-h-0 flex-col text-xs" data-tool-panel="notebook" onKeyDown={onKey}>
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1">
        <span className="font-semibold text-primary" aria-label="Column shown">
          {(column?.column ?? node).join(' › ')}
        </span>
        {column && (
          <span className="text-muted-foreground">
            {column.case} · {column.dataflow}
          </span>
        )}
        {column && (
          <Choice
            value={every}
            options={['one', 'every'] as const}
            labels={{ one: 'This block', every: 'Every block' }}
            onChange={setEvery}
            label="The picked step's units for this block, before and after it; or after it for every block of the table, grouped and added up"
          />
        )}
        {column && focus && (
          <label className="flex items-center gap-1">
            <span className="text-muted-foreground">Follow</span>
            <Select
              className="h-6 w-40"
              aria-label="The unit followed: what each step read for it, its workings and formula lines, its row in Before and After"
              value={String(focus.id)}
              onChange={(e) => {
                const u = choices.find((c) => String(c.id) === e.target.value);
                if (u) setFocus(focusOf(u));
              }}
            >
              {choices.map((u) => (
                <option key={u.id} value={u.id}>
                  {unitWords(u)}
                  {u.became ? ` (${u.became})` : ''}
                </option>
              ))}
            </Select>
          </label>
        )}
        <span className="ml-auto flex flex-wrap items-center justify-end gap-x-3 gap-y-1">
          {saved && runAgainControls}
          {column && (
            <button
              type="button"
              className="whitespace-nowrap text-muted-foreground hover:text-foreground hover:underline"
              data-slot="show-as-graph"
              title={graph ? 'The steps as a strip again' : 'The steps as a graph: the inputs wired to the steps that read them, what each wrote below it'}
              onClick={() => setGraph((g) => !g)}
            >
              {graph ? 'Show as strip' : 'Show as graph'}
            </button>
          )}
          {besideButtons}
        </span>
      </div>
      {error && <div className="px-2 py-1 text-destructive">{error}</div>}
      {rerun.said && (
        <div className={cn('px-2 py-1', rerun.refused ? 'text-destructive' : 'text-ok')} data-slot="run-again-said">
          {rerun.said}
        </div>
      )}
      {!column && !error && <div className="p-3 text-muted-foreground">Running the column…</div>}
      {column?.edited && (
        <div className="px-2 py-1 text-edited" data-slot="notebook-edited">
          Run live on unsaved edits to {column.unsaved.join(' and ')}: what they moved is blue, the saved run's value struck through beside it. Nothing is
          kept; Save or Discard them in the Parameters tab.
        </div>
      )}
      {liveWhy && (
        <div className="px-2 py-1 text-warning" data-slot="live-why">
          Not run on the latest edit, {liveWhy}. {column?.edited ? 'Showing the last edit that ran.' : 'Showing the saved run.'}
        </div>
      )}
      {column && column.failing.length > 0 && (
        <div className="px-2 py-1 text-warning" data-slot="notebook-failing">
          <div>
            {column.failing.length} worked example{column.failing.length === 1 ? " doesn't" : "s don't"} hold, run anyway: saving the formula is refused until {column.failing.length === 1 ? 'it holds' : 'they hold'} or {column.failing.length === 1 ? "it's" : "they're"} changed.
          </div>
          {column.failing.map((f) => (
            <div key={f} className="pl-3">
              {f}
            </div>
          ))}
        </div>
      )}
      {column && column.changed.length > 0 && (
        <div className="px-2 py-1 text-warning" data-slot="column-changed">
          Run on what's there now: {column.changed.join(', ')} changed since this table's numbers for this block were made, so the steps may not give
          them. Run again, beside Show as graph, brings them together.
        </div>
      )}
      {column && (
        <div className="shrink-0 border-b border-border px-2 py-1.5">
          {graph ? (
            <DataflowGraph column={column} quiet={summaries.map((x) => x.quiet)} picked={at} unit={into?.id ?? null} onPick={setPicked} />
          ) : (
            <DataflowStrip column={column} saved={against} quiet={summaries.map((x) => x.quiet)} picked={at} unit={into?.id ?? null} onPick={setPicked} />
          )}
        </div>
      )}
      {column && step && tables && (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(17rem,26rem)_minmax(0,1fr)_minmax(0,1fr)] divide-x divide-border" data-slot="dataflow-columns">
          <div className="min-h-0 overflow-auto">
            <StepPane
              column={column}
              at={at}
              summary={summaries[at]}
              focus={focus}
              trace={trace}
              onEdit={step.kind === 'formula' || step.kind === 'allocation' ? () => openTool('params', `formula:${step.dataflow ?? column.dataflow_id}:${step.step}`) : undefined}
              onWhole={() => setWhole(at)}
              edited={!!against && stepChanged(against, column, at)}
              onSetting={(t, row, name) => openTool('params', `cell:${t}:${row}:${name}`)}
            />
          </div>
          {every === 'every' ? (
            <div className="col-span-2 flex min-h-0 flex-col">
              <DataflowEveryBlock
                table={table.id}
                column={column}
                at={at}
                onOpen={(words) => {
                  setEvery('one');
                  openNotebook(table.id, words);
                }}
              />
            </div>
          ) : (
            <>
              <UnitTable side="before" column={column} at={at} table={tables} followed={into?.id ?? null} onFollow={(u) => setFocus(focusOf(u))} />
              <UnitTable side="after" column={column} at={at} table={tables} followed={outOf?.id ?? null} onFollow={(u) => setFocus(focusOf(u))} all={all} onAll={setAll} />
            </>
          )}
        </div>
      )}
    </div>
  );
}
