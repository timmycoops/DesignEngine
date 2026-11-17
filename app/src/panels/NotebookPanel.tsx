import { useEffect, useMemo, useState } from 'react';
import { type ColumnSteps, reservesColumn, reservesColumnTrace } from '@/bridge';
import { Select } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import type { PanelId } from '@/workspace/panels';
import { ColumnStepsView } from './ColumnStepsView';
import { unitWords } from './columnSteps';
import { cellSummaries, defaultFocus, type Focus, focusAt, focusOf } from './notebook';
import { type CellTrace, StepCell } from './StepCell';

/**
 * The Notebook tab (decision 0081): a block's column as a notebook, a cell a step down the page, each showing what went
 * in for the unit in focus, the formula that ran on it or the built-in step's rule, and what the step caused across the
 * column. Opened from the Reserves table's Open its column. Steps that changed nothing fold to one line; a cell's Whole
 * column shows today's grid at that step; a formula's Edit opens it in the Parameters tab.
 */

const keyOf = (path: string[]) => path.join('\u0001');

/** The tabs a planner reads beside the notebook: the parameters its steps read, and the table whose block it follows. */
const BESIDE: readonly { id: PanelId; title: string }[] = [
  { id: 'params', title: 'Parameters' },
  { id: 'restable', title: 'Reserves table' },
];

export function Notebook() {
  const { summary, notebookOf, openTool, runInNotebook, placeBeside, isBeside } = useWorkspace();
  // Bumped as a tab is put beside or back, so the buttons say which way they go next.
  const [, setLaidOut] = useState(0);
  const table = useMemo(() => (notebookOf ? (summary?.objects ?? []).find((o) => o.id === notebookOf.table) : undefined), [summary, notebookOf]);
  const node = notebookOf?.node ?? [];
  const edited = notebookOf?.edited ?? null;
  const [column, setColumn] = useState<ColumnSteps | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [fold, setFold] = useState(true);
  const [opened, setOpened] = useState<Map<number, boolean>>(new Map());
  const [whole, setWhole] = useState<number | null>(null);
  const [traces, setTraces] = useState<Map<string, CellTrace>>(new Map());

  const ask = `${notebookOf?.n ?? 0}:${table?.id ?? ''}@${table?.revision ?? 0}:${keyOf(node)}`;
  useEffect(() => {
    let live = true;
    setColumn(null);
    setError(null);
    setWhole(null);
    setOpened(new Map());
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
        // The edited step's cell open and in sight.
        const at = edited ? c.steps.findIndex((s) => s.step === edited.step) : -1;
        if (at > 0) {
          setOpened(new Map([[at, true]]));
          requestAnimationFrame(() => document.querySelector(`[data-slot="notebook-cell"][data-step="${CSS.escape(edited!.step)}"]`)?.scrollIntoView({ block: 'start' }));
        }
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [ask]);

  const summaries = useMemo(() => (column ? cellSummaries(column.steps) : []), [column]);
  const isOpen = (i: number) => opened.get(i) ?? !(fold && summaries[i]?.quiet);
  const toggle = (i: number) => setOpened((m) => new Map(m).set(i, !isOpen(i)));

  // Each open formula cell traced on the focus unit as it stands before the step; a trace is kept by step and unit.
  const wanted = useMemo(() => {
    if (!column || !focus) return [] as { at: number; unit: number; key: string }[];
    return column.steps.flatMap((c, i) => {
      if (i === 0 || c.kind !== 'formula' || !isOpen(i)) return [];
      const u = focusAt(column.steps[i - 1].units, focus);
      return u ? [{ at: i, unit: u.id, key: `${ask}|${i}:${u.id}` }] : [];
    });
  }, [column, focus, opened, fold, summaries]);
  // Keyed by the ask too, so a trace that comes back after the notebook moved on lands nowhere it's read.
  useEffect(() => {
    if (!table) return;
    for (const w of wanted) {
      if (traces.has(w.key)) continue;
      setTraces((m) => new Map(m).set(w.key, { state: 'loading' }));
      void reservesColumnTrace(table.id, node, w.at, w.unit, edited?.dataflow ?? null)
        .then((trace) => setTraces((m) => new Map(m).set(w.key, { state: 'done', trace })))
        .catch((e) => setTraces((m) => new Map(m).set(w.key, { state: 'failed', why: String(e) })));
    }
  }, [wanted]);
  const traceAt = (i: number): CellTrace | null => {
    if (!column || !focus || i === 0) return null;
    const u = focusAt(column.steps[i - 1].units, focus);
    return u ? (traces.get(`${ask}|${i}:${u.id}`) ?? { state: 'loading' }) : null;
  };

  // Tabs to read beside the notebook, each its own half of the dock, and back as a tab with a second press.
  const besideButtons = (
    <span className="flex gap-2" data-slot="notebook-beside">
      {BESIDE.map(({ id, title }) => {
        const beside = isBeside(id, 'notebook');
        return (
          <button
            key={id}
            type="button"
            className="text-muted-foreground hover:text-foreground hover:underline"
            title={beside ? `Put the ${title} tab back behind the Notebook` : `The ${title} tab beside the Notebook, each with half the room, to read both at once`}
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
        <div>Pick a block in the Reserves table, or a seam or bench of one: its column shows here a step at a time, what went in, what ran and what it changed. The Notebook follows the block picked there.</div>
        {besideButtons}
      </div>
    );
  }
  if (whole !== null) {
    return (
      <div className="flex h-full min-h-0 text-xs" data-tool-panel="notebook">
        <ColumnStepsView table={table} node={node} backLabel="Notebook" startStep={whole} dataflow={edited?.dataflow ?? null} onBack={() => setWhole(null)} />
      </div>
    );
  }
  // The units to follow: every unit of the column as it comes in, then any a step made.
  const choices = column ? [...new Map(column.steps.flatMap((s) => s.units).map((u) => [u.id, u])).values()] : [];
  return (
    <div className="flex h-full min-h-0 flex-col text-xs" data-tool-panel="notebook">
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
              aria-label="The unit in focus: what each step read for it, and its formula lines"
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
        <label className="flex items-center gap-1" title="Steps that changed nothing in this column fold to one line">
          <input type="checkbox" checked={fold} onChange={(e) => (setFold(e.target.checked), setOpened(new Map()))} />
          Fold steps that changed nothing
        </label>
        <span className="ml-auto flex gap-3">
          {besideButtons}
          {column && (
            <span className="flex gap-2">
              <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => setOpened(new Map(column.steps.map((_, i) => [i, true])))}>
              Open all
            </button>
              <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => setOpened(new Map(column.steps.map((_, i) => [i, false])))}>
                Fold all
              </button>
            </span>
          )}
        </span>
      </div>
      {error && <div className="px-2 py-1 text-destructive">{error}</div>}
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
          Run on what's there now: {column.changed.join(', ')} changed since this table was made, so the cells may not give the table's numbers. Run reserves again to bring them together.
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto" data-slot="notebook">
        {!column && !error && <div className="p-3 text-muted-foreground">Running the column…</div>}
        {column &&
          column.steps.map((c, i) => (
            <StepCell
              key={`${i}:${c.step}`}
              column={column}
              at={i}
              summary={summaries[i]}
              focus={focus}
              open={isOpen(i)}
              onToggle={() => toggle(i)}
              onFocus={setFocus}
              trace={traceAt(i)}
              onEdit={c.kind === 'formula' ? () => openTool('params', `formula:${column.dataflow_id}:${c.step}`) : undefined}
              onWhole={() => setWhole(i)}
              edited={!!column.edited && edited?.step === c.step}
            />
          ))}
      </div>
    </div>
  );
}
