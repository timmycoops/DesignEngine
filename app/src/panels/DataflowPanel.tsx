import { type KeyboardEvent, useEffect, useMemo, useState } from 'react';
import { type ColumnSteps, reservesColumn, reservesColumnTrace } from '@/bridge';
import { Select } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import type { PanelId } from '@/workspace/panels';
import { ColumnStepsView } from './ColumnStepsView';
import { unitWords } from './columnSteps';
import { DataflowGraph } from './DataflowGraph';
import { DataflowStrip } from './DataflowStrip';
import { UnitTable } from './DataflowTables';
import { beforeAfter } from './dataflowView';
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

/** The tabs a planner reads beside the Dataflow tab: the parameters its steps read, and the table whose block it follows. */
const BESIDE: readonly { id: PanelId; title: string }[] = [
  { id: 'params', title: 'Parameters' },
  { id: 'restable', title: 'Reserves table' },
];

export function DataflowView() {
  const { summary, notebookOf, openTool, runInNotebook, placeBeside, isBeside } = useWorkspace();
  // Bumped as a tab is put beside or back, so the buttons say which way they go next.
  const [, setLaidOut] = useState(0);
  const table = useMemo(() => (notebookOf ? (summary?.objects ?? []).find((o) => o.id === notebookOf.table) : undefined), [summary, notebookOf]);
  const node = notebookOf?.node ?? [];
  const edited = notebookOf?.edited ?? null;
  const [column, setColumn] = useState<ColumnSteps | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [whole, setWhole] = useState<number | null>(null);
  const [traces, setTraces] = useState<Map<string, CellTrace>>(new Map());
  // The step picked in the strip, the strip drawn as a graph, and every field in Before and After.
  const [picked, setPicked] = useState(0);
  const [graph, setGraph] = useState(false);
  const [all, setAll] = useState(false);

  const ask = `${notebookOf?.n ?? 0}:${table?.id ?? ''}@${table?.revision ?? 0}:${keyOf(node)}`;
  useEffect(() => {
    let live = true;
    setColumn(null);
    setError(null);
    setWhole(null);
    setTraces(new Map());
    if (!table) return;
    void reservesColumn(table.id, node, edited?.dataflow ?? null)
      .then((c) => {
        if (!live) return;
        // An edited formula runs on the column of a table its own dataflow made, never another's.
        if (edited && c.dataflow_id !== edited.dataflowId) {
          setError(`This table was made by the ${c.dataflow} dataflow, not the one being edited: open a column of a table it made to run the formula there.`);
          return;
        }
        setColumn(c);
        const below = node.slice(c.column.length);
        setFocus(defaultFocus(c.steps, below.length ? { seam: below[0], bench: below[1] } : null));
        // The edited step picked; else the step already picked, where this column has it; else the first that did something.
        setPicked((was) => {
          const at = edited ? c.steps.findIndex((s) => s.step === edited.step) : -1;
          if (at > 0) return at;
          if (was > 0 && was < c.steps.length) return was;
          const busy = cellSummaries(c.steps).findIndex((s, i) => i > 0 && !s.quiet);
          return busy > 0 ? busy : 0;
        });
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [ask]);

  const summaries = useMemo(() => (column ? cellSummaries(column.steps) : []), [column]);
  const at = column ? Math.min(picked, column.steps.length - 1) : 0;
  const tables = useMemo(() => (column ? beforeAfter(column, at, all) : null), [column, at, all]);
  // The unit followed as it stands going into the picked step (what In and Workings show, whose rows Inputs light), and
  // as the step left it (After's row).
  const into = column && focus && at > 0 ? focusAt(column.steps[at - 1].units, focus) : null;
  const outOf = column && focus ? focusAt(column.steps[at].units, focus) : null;

  // The picked formula step traced on the unit followed; a trace is kept by step and unit.
  const wanted = column && at > 0 && column.steps[at].kind === 'formula' && into ? `${ask}|${at}:${into.id}` : null;
  useEffect(() => {
    if (!table || !wanted || !into || traces.has(wanted)) return;
    setTraces((m) => new Map(m).set(wanted, { state: 'loading' }));
    void reservesColumnTrace(table.id, node, at, into.id, edited?.dataflow ?? null)
      .then((trace) => setTraces((m) => new Map(m).set(wanted, { state: 'done', trace })))
      .catch((e) => setTraces((m) => new Map(m).set(wanted, { state: 'failed', why: String(e) })));
  }, [wanted]);
  const trace = wanted ? (traces.get(wanted) ?? { state: 'loading' }) : null;

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
            className="text-muted-foreground hover:text-foreground hover:underline"
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
        <ColumnStepsView table={table} node={node} backLabel="Dataflow" startStep={whole} dataflow={edited?.dataflow ?? null} onBack={() => setWhole(null)} />
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
        <span className="ml-auto flex gap-3">
          {column && (
            <button
              type="button"
              className="text-muted-foreground hover:text-foreground hover:underline"
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
      {!column && !error && <div className="p-3 text-muted-foreground">Running the column…</div>}
      {column?.edited && edited && (
        <div className="flex flex-wrap items-center gap-x-3 px-2 py-1 text-edited" data-slot="notebook-edited">
          <span>
            Run on the {column.dataflow} dataflow as edited, not saved: {edited.step.replaceAll('_', ' ')} changed. Nothing is kept.
          </span>
          <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => runInNotebook(null)}>
            Back to the saved formula
          </button>
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
          Run on what's there now: {column.changed.join(', ')} changed since this table was made, so the steps may not give the table's numbers. Run reserves again to bring them together.
        </div>
      )}
      {column && (
        <div className="shrink-0 border-b border-border px-2 py-1.5">
          {graph ? (
            <DataflowGraph column={column} quiet={summaries.map((x) => x.quiet)} picked={at} unit={into?.id ?? null} onPick={setPicked} />
          ) : (
            <DataflowStrip column={column} quiet={summaries.map((x) => x.quiet)} picked={at} unit={into?.id ?? null} onPick={setPicked} />
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
              onEdit={step.kind === 'formula' ? () => openTool('params', `formula:${column.dataflow_id}:${step.step}`) : undefined}
              onWhole={() => setWhole(at)}
              edited={!!column.edited && edited?.step === step.step}
            />
          </div>
          <UnitTable side="before" column={column} at={at} table={tables} followed={into?.id ?? null} onFollow={(u) => setFocus(focusOf(u))} />
          <UnitTable side="after" column={column} at={at} table={tables} followed={outOf?.id ?? null} onFollow={(u) => setFocus(focusOf(u))} all={all} onAll={setAll} />
        </div>
      )}
    </div>
  );
}
