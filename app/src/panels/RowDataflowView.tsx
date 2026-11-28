import { Braces, Save, Undo2 } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { type DataflowState, objectDetails, type ParameterTable, type RowDataflowAtStep, rowDataflowStep } from '@/bridge';
import { ok, refused, type Said, SaidLine, StatusBadge, TextButton } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { rangeWords } from './flowStages';
import { stepLabel } from './columnSteps';
import { SettingsBySeam } from './ReservesCaseParts';
import { gridOf, scriptSettings, withCell } from './reservesCase';

/**
 * A case's row dataflow opened in the Reserves case tab (Reserves case plan, decisions 4 and 8, workstream 6): its rows
 * in range and the settings tables it reads as inputs, its steps, the step picked (its script, what it reads and sets,
 * its columns over the range before and after the step and as the last run left them, its settings by row to type in),
 * and the rows before and after the step. An edit here or in the Parameters tab runs it again as it's typed; Save and run
 * saves the tables and runs this dataflow alone.
 */

const LIVE_MS = 330;
const ROWS_SHOWN = 400;

const cellWords = (v: number | string | null, unit = '') =>
  v === null ? '—' : typeof v === 'number' ? `${v.toLocaleString('en-AU', { maximumFractionDigits: Math.abs(v) >= 100 ? 0 : 3 })}${unit ? ` ${unit}` : ''}` : v;

export function RowDataflowView({ table, dataflow }: { table: { id: string; name: string }; dataflow: DataflowState }) {
  const { drafts, setDraft, runRecord, openTool, summary } = useWorkspace();
  const [step, setStep] = useState(1);
  const [view, setView] = useState<RowDataflowAtStep | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [params, setParams] = useState<Record<string, ParameterTable>>({});
  const [said, setSaid] = useState<Said | null>(null);
  const [saving, setSaving] = useState(false);
  const changed = (summary?.objects ?? []).map((o) => `${o.id}:${o.revision}`).join(',');

  useEffect(() => {
    setStep(1);
    setView(null);
    setSaid(null);
  }, [table.id, dataflow.dataflow]);

  // Its tables and its own dataflow as edited, not saved.
  const ids = view?.tables.map(([id]) => id) ?? [];
  const flowDraft = drafts[dataflow.dataflow];
  const edits = useMemo(() => {
    const tables = ids.flatMap((id): [string, ParameterTable][] => {
      const d = drafts[id];
      return d?.kind === 'table' && d.table ? [[id, d.table]] : [];
    });
    const edited = flowDraft?.kind === 'dataflow' ? flowDraft.dataflow : null;
    const stuck = [...ids.map((id) => drafts[id]), flowDraft].find((d) => d?.why);
    return { tables, edited, key: JSON.stringify([tables, edited]), why: stuck ? `${stuck.name}: ${stuck.why}` : null, any: tables.length > 0 || edited !== null };
  }, [ids.join(','), drafts, flowDraft]);

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    if (edits.why) return;
    const typing = edits.key !== lastKey.current;
    lastKey.current = edits.key;
    let current = true;
    const t = setTimeout(
      () => {
        void rowDataflowStep(table.id, dataflow.dataflow, step, edits.edited, edits.tables)
          .then((v) => current && (setView(v), setError(null)))
          .catch((e) => current && setError(String(e)));
      },
      typing && view ? LIVE_MS : 0,
    );
    return () => {
      current = false;
      clearTimeout(t);
    };
  }, [table.id, dataflow.dataflow, step, edits.key, edits.why, changed]);

  useEffect(() => {
    let current = true;
    void Promise.all(ids.map((id) => objectDetails(id).then((d) => [id, d.measures.kind === 'parameter_table' ? d.measures.parameters : null] as const))).then(
      (got) => current && setParams(Object.fromEntries(got.filter(([, t]) => t !== null)) as Record<string, ParameterTable>),
    );
    return () => {
      current = false;
    };
  }, [ids.join(','), changed]);

  const steps = view?.steps ?? [];
  const cell = steps[step - 1] ?? null;
  const tableAt = (t: number): ParameterTable | null => {
    const id = ids[t];
    const d = id ? drafts[id] : undefined;
    return d?.kind === 'table' && d.table ? d.table : (params[id] ?? null);
  };
  const settings = cell ? scriptSettings(cell.script) : [];
  const grid = view ? gridOf(settings, view.tables.map(([, n]) => n), ids, view.tables.map((_, t) => tableAt(t)), 'every row') : null;
  const readsTable = new Set((grid?.rows ?? []).map((r) => r.table));
  const own = new Set(view?.columns.map((c) => c.field) ?? []);
  const above = (cell?.reads ?? []).filter((r) => !own.has(r));
  const editedCells = new Set<string>();
  for (const r of grid?.rows ?? []) {
    const saved = r.tableId ? params[r.tableId] : null;
    const d = r.tableId ? drafts[r.tableId] : undefined;
    if (!saved || d?.kind !== 'table' || !d.table) continue;
    for (const c of grid!.columns) {
      const was = saved.rows[r.row]?.values ?? {};
      const key = Object.keys(was).find((k) => k.toLowerCase() === c.toLowerCase());
      if ((key === undefined ? null : was[key]) !== r.values[c]) editedCells.add(`${r.table}:${r.row}:${c.toLowerCase()}`);
    }
  }
  const setCell = (t: number, row: number, name: string, text: string) => {
    const id = ids[t];
    const base = tableAt(t);
    if (!id || !base || !view) return;
    const next = withCell(base, row, name, text);
    setSaid(null);
    setDraft(id, JSON.stringify(next) === JSON.stringify(params[id]) ? null : { kind: 'table', name: view.tables[t][1], table: next, why: null });
  };
  const saveAndRun = async () => {
    setSaving(true);
    try {
      for (const [id, t] of edits.tables) {
        const name = view?.tables.find(([x]) => x === id)?.[1] ?? id;
        const reply = await runRecord('parameters.save', { table: id, name, content: t });
        if (reply.error) return setSaid(refused(`${name}: ${reply.error}`));
        setDraft(id, null);
      }
      const reply = await runRecord('table.run_dataflow', { table: table.id, dataflow: dataflow.dataflow });
      setSaid(reply.error ? refused(reply.error) : ok(`Saved, and ${dataflow.name} run on ${table.name}.`));
    } catch (e) {
      setSaid(refused(String(e)));
    } finally {
      setSaving(false);
    }
  };
  const discard = () => edits.tables.forEach(([id]) => setDraft(id, null));
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    if ((e.target as HTMLElement).closest('input, select, textarea, [contenteditable="true"]')) return;
    const by = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!by || steps.length === 0) return;
    e.preventDefault();
    setStep(Math.max(1, Math.min(steps.length, step + by)));
  };

  if (error && !view) return <div className="px-2 py-1 text-destructive">{error}</div>;
  if (!view) return <div className="p-3 text-muted-foreground">Running {dataflow.name}…</div>;
  return (
    <div className="flex min-h-0 flex-col" onKeyDown={onKey} data-slot="row-dataflow" data-dataflow={view.name}>
      {/* Its inputs: the rows in range, the columns above it this step reads, its settings tables. */}
      <div className="flex shrink-0 flex-wrap gap-1.5 border-b border-border px-2 py-1.5" data-slot="case-inputs">
        <Card head="Rows" body={`${view.rows_in_range} of ${view.rows_in_range + view.outside} rows`} more={`on ${rangeWords({ range: view.range, reserves: false })}${view.outside ? `; ${view.outside} outside left empty` : ''}`} lit={false} />
        <Card head="Columns above it" body={above.length ? `${above.length} read by this step` : 'none read by this step'} more={above.slice(0, 2).join(', ') + (above.length > 2 ? ' …' : '')} lit={above.length > 0} />
        {view.tables.map(([id, name], t) => (
          <Card key={id} head="Settings" body={name} more={readsTable.has(t) ? 'read by this step' : 'not read by this step'} lit={readsTable.has(t)} edited={drafts[id]?.kind === 'table'} />
        ))}
      </div>
      {error && <div className="px-2 py-1 text-warning">Not run on the latest edit: {error}. Showing the last that ran.</div>}
      <div className="grid h-[28rem] shrink-0 grid-cols-[minmax(10rem,13rem)_minmax(0,1fr)_minmax(16rem,24rem)] divide-x divide-border" data-slot="case-columns">
        <div className="min-h-0 overflow-auto p-2" data-slot="case-steps">
          <div className="mb-1 text-muted-foreground">Steps · ↑ ↓ to step</div>
          {steps.map((x, i) => (
            <button
              key={x.name}
              type="button"
              data-slot="case-step"
              data-step={x.name}
              className={cn('flex w-full items-baseline gap-1.5 rounded px-1 py-0.5 text-left hover:bg-accent', i + 1 === step && 'bg-selection font-semibold text-primary')}
              onClick={() => setStep(i + 1)}
            >
              <span className="w-5 shrink-0 text-right text-muted-foreground tabular-nums">{i + 1}</span>
              <span className="min-w-0 flex-1 truncate">{stepLabel(x.name)}</span>
              <span className="text-[10px] text-muted-foreground">{x.kind === 'formula' ? 'Formula' : 'Table script'}</span>
            </button>
          ))}
          <div className="mt-2 text-muted-foreground">A row dataflow works on the table's rows, a formula on each row in its range, and writes only its own columns.</div>
        </div>
        <div className="min-h-0 space-y-2 overflow-auto p-2" data-slot="case-step-pane">
          {cell && (
            <>
              <div className="flex flex-wrap items-baseline gap-x-2">
                <span className="text-sm font-semibold">
                  {step} · {stepLabel(cell.name)}
                </span>
                <span className="text-muted-foreground">{cell.kind === 'formula' ? 'Formula on each row' : 'Table script on the table'}</span>
                {edits.any && (
                  <StatusBadge tone="edited" title="Run on unsaved edits">
                    run on edits
                  </StatusBadge>
                )}
              </div>
              <TextButton className="gap-1" onClick={() => openTool('params', `formula:${dataflow.dataflow}:${cell.name}`)}>
                <Braces className="size-3" /> Open its script in the formula panel
              </TextButton>
              <div className="grid grid-cols-[3.5rem_1fr] gap-x-2 gap-y-0.5">
                <span className="text-muted-foreground">Reads</span>
                <span>
                  {[...cell.reads, ...settings].length === 0 && <span className="text-muted-foreground">nothing it names</span>}
                  {cell.reads.map((f) => (
                    <Chip key={`f${f}`}>{f}</Chip>
                  ))}
                  {settings.map((f) => (
                    <Chip key={`s${f}`} lit>
                      {f}
                    </Chip>
                  ))}
                </span>
                <span className="text-muted-foreground">Sets</span>
                <span>{cell.sets.length === 0 ? <span className="text-muted-foreground">nothing on a row</span> : cell.sets.map((f) => <Chip key={f}>{f}</Chip>)}</span>
              </div>
              <section className="space-y-1">
                <div className="text-muted-foreground">Its columns over {rangeWords({ range: view.range, reserves: false })} · before and after this step · as its last run left them</div>
                <table className="w-full border-collapse tabular-nums" data-slot="row-totals">
                  <thead>
                    <tr className="text-muted-foreground">
                      <th className="px-1 text-left font-normal">Field</th>
                      <th className="px-1 text-right font-normal">Before</th>
                      <th className="px-1 text-right font-normal">After</th>
                      <th className="px-1 text-right font-normal">Last run</th>
                    </tr>
                  </thead>
                  <tbody>
                    {view.columns.map((c) => (
                      <tr key={c.column} className="border-t border-border" data-column={c.column} data-after={c.after ?? ''}>
                        <td className="px-1" title={c.column}>
                          {c.field}
                        </td>
                        <td className="px-1 text-right">{c.words ? 'words' : cellWords(c.before, c.unit)}</td>
                        <td className={cn('px-1 text-right', c.after !== c.before && 'font-semibold')}>{c.words ? 'words' : cellWords(c.after, c.unit)}</td>
                        <td className={cn('px-1 text-right', c.last_run !== null && c.after !== null && step === steps.length && Math.abs(c.after - c.last_run) > 1e-9 * Math.max(1, Math.abs(c.last_run)) ? 'text-edited' : 'text-muted-foreground')}>
                          {c.words ? '' : c.last_run === null ? 'not run' : cellWords(c.last_run, c.unit)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>
              <section className="space-y-1">
                <div className="text-muted-foreground">Settings by row · the rows read in order, the first that fits the row and holds a setting gives it</div>
                {grid && <SettingsBySeam grid={grid} supplied={new Map()} edited={editedCells} onSet={setCell} />}
              </section>
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" className="h-7" disabled={edits.tables.length === 0 || saving || !!edits.why} onClick={() => void saveAndRun()} title={`Save the settings tables as typed, then run ${dataflow.name} on ${table.name}: each its next revision, one undo each`}>
                  <Save className="size-3.5" /> {saving ? 'Saving…' : `Save and run ${dataflow.name}`}
                </Button>
                {edits.tables.length > 0 && (
                  <Button size="sm" variant="outline" className="h-7" onClick={discard}>
                    <Undo2 className="size-3.5" /> Discard
                  </Button>
                )}
                <span className={cn(edits.any ? 'text-edited' : 'text-muted-foreground')}>{edits.any ? `Unsaved: ${dataflow.name} runs again as you type (${Math.round(view.ms)} ms).` : 'Type a setting to see it run again.'}</span>
              </div>
              <SaidLine said={said} />
              {view.stopped.map((s) => (
                <div key={s} className="text-destructive">
                  {s}
                </div>
              ))}
              {view.said.slice(0, 5).map((s) => (
                <div key={s} className="text-muted-foreground">
                  {s}
                </div>
              ))}
            </>
          )}
        </div>
        <div className="min-h-0 overflow-auto p-2" data-slot="row-rows">
          <div className="mb-1 text-muted-foreground">
            The rows in range · before → after this step{view.rows.length > ROWS_SHOWN ? ` · the first ${ROWS_SHOWN} of ${view.rows.length}` : ''}
          </div>
          <table className="w-full border-collapse tabular-nums">
            <thead className="sticky top-0 bg-background">
              <tr className="text-muted-foreground">
                <th className="px-1 text-left font-normal">Row</th>
                {view.columns.map((c) => (
                  <th key={c.column} className="px-1 text-right font-normal" title={c.column}>
                    {c.field}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {view.rows.slice(0, ROWS_SHOWN).map((r) => (
                <tr key={r.row} className="border-t border-border" data-slot="row-row">
                  <td className="max-w-40 truncate px-1" title={r.label}>
                    {r.label}
                  </td>
                  {view.columns.map((c, k) => {
                    const moved = r.after[k] !== r.before[k];
                    return (
                      <td key={c.column} className={cn('px-1 text-right whitespace-nowrap', moved && 'font-semibold text-primary')} title={moved ? `was ${cellWords(r.before[k])}` : undefined}>
                        {cellWords(r.after[k])}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function Card({ head, body, more, lit, edited = false }: { head: string; body: string; more: string; lit: boolean; edited?: boolean }) {
  return (
    <div className={cn('min-w-32 rounded border px-2 py-1', lit ? 'border-primary bg-selection' : 'border-border')} data-slot="case-input" data-lit={lit ? 'yes' : 'no'}>
      <div className="text-[10px] tracking-wide text-muted-foreground uppercase">{head}</div>
      <div className={cn('font-semibold', edited && 'text-edited')}>{body}</div>
      <div className="text-muted-foreground">{edited ? 'run as edited' : more}</div>
    </div>
  );
}

function Chip({ children, lit = false }: { children: string; lit?: boolean }) {
  return <span className={cn('mr-1 mb-0.5 inline-block rounded border px-1 font-mono text-[11px]', lit ? 'border-primary text-primary' : 'border-border')}>{children}</span>;
}
