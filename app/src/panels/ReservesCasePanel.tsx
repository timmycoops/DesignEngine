import { Braces, Save, Undo2 } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { type CaseStepView, type ColumnSteps, type ColumnUnit, objectDetails, type ParameterTable, reservesCaseStep, reservesColumn, reservesRerunParams } from '@/bridge';
import { PanelHead, Select, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { type Draft, useWorkspace } from '@/workspace/context';
import { stepLabel, unitWords } from './columnSteps';
import { writesByFolder } from './dataflowView';
import { BlockMap, SettingsBySeam, StripLog, WholeCase } from './ReservesCaseParts';
import { UnitsTable } from './UnitsTable';
import { mapMeasure, settingsGrid, shadeOf, stagesOf, stepSettings, suppliedFor, withCell } from './reservesCase';

/**
 * The Reserves case tab (the Build Plan's "Reserves case plan", decisions 8, 9 and 11, workstream 4): a reserves table's
 * case a step at a time. Across the top the band (the dataflow open, its case, whether any ground is made or lost) and
 * the inputs, those the step reads lit gold; under them the steps by stage, the step picked (its rule, what it reads and
 * writes, its settings by seam and ply to type in, the whole case at the step against the last run), and the block map
 * with the picked block's strip log before and after. An edit, here or in the Parameters tab, runs the whole case again
 * from the first step that reads it as it's typed; Save and run the case keeps it.
 */

/** How long after the last keystroke the case runs again on the edits (decision 0082's pace). */
const LIVE_MS = 330;

/** The unsaved edits a case reads: its dataflow as edited, and the parameter tables it reads, by id. */
function editsOf(column: ColumnSteps | null, drafts: Record<string, Draft>) {
  if (!column) return null;
  const ids = new Set(column.table_ids.filter((t): t is string => t !== null));
  const flow = drafts[column.dataflow_id];
  const tables = Object.entries(drafts).filter(([id, d]) => d.kind === 'table' && ids.has(id));
  const used: [string, Draft][] = [...(flow?.kind === 'dataflow' ? [[column.dataflow_id, flow] as [string, Draft]] : []), ...tables];
  if (used.length === 0) return null;
  const stuck = used.find(([, d]) => d.why);
  return {
    key: JSON.stringify(used.map(([id, d]) => [id, d.kind === 'table' ? d.table : d.dataflow])),
    why: stuck ? `${stuck[1].name}: ${stuck[1].why}` : null,
    dataflow: flow?.kind === 'dataflow' ? flow.dataflow : null,
    tables: tables.flatMap(([id, d]): [string, ParameterTable][] => (d.kind === 'table' && d.table ? [[id, d.table]] : [])),
  };
}

const kindWords = { input: 'the blocks', built_in: 'Built-in', formula: 'Formula', allocation: 'Allocation' } as const;

export function ReservesCasePanel() {
  const { summary, selection, notebookOf, drafts, setDraft, runRecord, openTool } = useWorkspace();
  const tables = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own), [summary]);
  const [pick, setPick] = useState<string | null>(null);
  const followed = notebookOf?.table ?? null;
  const last = selection[selection.length - 1] ?? null;
  useEffect(() => {
    if (last && tables.some((t) => t.id === last)) setPick(last);
  }, [last, tables]);
  useEffect(() => {
    if (followed) setPick(followed);
  }, [followed]);
  const table = tables.find((t) => t.id === pick) ?? tables[tables.length - 1] ?? null;
  // Anything the case reads may move it: its blocks, dataflow and tables, saved; the table run again.
  const changed = (summary?.objects ?? []).map((o) => `${o.id}:${o.revision}`).join(',');

  const [step, setStep] = useState<number | null>(null);
  const [block, setBlock] = useState<string[] | null>(null);
  const [focus, setFocus] = useState<number | null>(null);
  const [view, setView] = useState<CaseStepView | null>(null);
  const [column, setColumn] = useState<ColumnSteps | null>(null);
  const [params, setParams] = useState<Record<string, ParameterTable>>({});
  const [error, setError] = useState<string | null>(null);
  const [liveWhy, setLiveWhy] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; refused: boolean } | null>(null);
  const [saving, setSaving] = useState(false);

  // A new table starts again: its first step that does something, its busiest block.
  useEffect(() => {
    setStep(null);
    setBlock(null);
    setView(null);
    setColumn(null);
    setSaid(null);
  }, [table?.id]);

  const edits = useMemo(() => editsOf(column, drafts), [column, drafts]);
  // The case at the step: at once for a new table, step or save; a third of a second after typing stops for an edit.
  const lastEdits = useRef<string | null>(null);
  useEffect(() => {
    if (!table) return;
    if (edits?.why) return setLiveWhy(edits.why);
    const typing = (edits?.key ?? null) !== lastEdits.current;
    lastEdits.current = edits?.key ?? null;
    let current = true;
    const t = setTimeout(
      () => {
        void reservesCaseStep(table.id, step ?? 0, edits?.dataflow ?? null, edits?.tables ?? [])
          .then((v) => {
            if (!current) return;
            setView(v);
            setError(null);
            setLiveWhy(null);
            if (step === null) {
              const named = v.saved.steps.indexOf('rom_loss');
              setStep(named >= 0 ? named + 1 : Math.min(1, v.saved.steps.length));
            }
          })
          .catch((e) => current && (edits ? setLiveWhy(String(e)) : (setView(null), setError(String(e)))));
      },
      typing && view ? LIVE_MS : 0,
    );
    return () => {
      current = false;
      clearTimeout(t);
    };
  }, [table?.id, step, edits?.key, edits?.why, changed]);

  // The block followed: kept while the case has it, else the one the step moved most in.
  useEffect(() => {
    if (!view) return;
    const has = (p: string[] | null) => !!p && view.blocks.some((b) => b.path.join('\u0001') === p.join('\u0001'));
    if (has(block)) return;
    const measure = mapMeasure(view.blocks);
    const ran = view.blocks.filter((b) => !b.stopped);
    const best = [...ran].sort((a, b) => shadeOf(b, view.blocks, measure) - shadeOf(a, view.blocks, measure))[0];
    setBlock(best?.path ?? null);
  }, [view]);

  // The block's column through every step, on the edits, for its strip log, its reads and the steps' kinds.
  useEffect(() => {
    if (!table || !block) return;
    let current = true;
    const t = setTimeout(
      () => {
        void reservesColumn(table.id, block, edits?.dataflow ?? null, edits?.tables ?? [])
          .then((c) => current && setColumn(c))
          .catch((e) => current && setLiveWhy(String(e)));
      },
      edits && column ? LIVE_MS : 0,
    );
    return () => {
      current = false;
      clearTimeout(t);
    };
  }, [table?.id, block?.join('\u0001'), edits?.key, changed]);

  // The parameter tables the case reads, as saved: what the settings grid starts from.
  const tableIds = (column?.table_ids ?? []).filter((t): t is string => t !== null);
  useEffect(() => {
    let current = true;
    void Promise.all(tableIds.map((id) => objectDetails(id).then((d) => [id, d.measures.kind === 'parameter_table' ? d.measures.parameters : null] as const))).then(
      (got) => current && setParams(Object.fromEntries(got.filter(([, t]) => t !== null)) as Record<string, ParameterTable>),
    );
    return () => {
      current = false;
    };
  }, [tableIds.join(','), changed]);

  const at = column && step !== null ? Math.min(step, column.steps.length - 1) : null;
  const cell = column && at !== null ? column.steps[at] : null;
  // The unit followed: as it goes into the step, the block's first coal by default.
  const before: ColumnUnit[] = column && at !== null ? column.steps[Math.max(0, at - 1)].units : [];
  const after: ColumnUnit[] = cell?.units ?? [];
  useEffect(() => {
    if (focus !== null && before.some((u) => u.id === focus)) return;
    setFocus(before.find((u) => u.material.toLowerCase() === 'coal' && !u.became)?.id ?? before[0]?.id ?? null);
  }, [before.map((u) => u.id).join(',')]);

  const tableAt = (t: number): ParameterTable | null => {
    const id = column?.table_ids[t];
    if (!id) return null;
    const d = drafts[id];
    return d?.kind === 'table' && d.table ? d.table : (params[id] ?? null);
  };
  const grid = useMemo(() => (column && at !== null ? settingsGrid(column, at, column.tables.map((_, t) => tableAt(t))) : null), [column, at, params, drafts]);
  const supplied = cell ? suppliedFor(cell, focus) : new Map();
  // The cells typed and not yet saved, by table, row and setting, lit blue.
  const editedCells = useMemo(() => {
    const out = new Set<string>();
    if (!column || !grid) return out;
    for (const r of grid.rows) {
      const savedTable = r.tableId ? params[r.tableId] : null;
      const d = r.tableId ? drafts[r.tableId] : undefined;
      if (!savedTable || d?.kind !== 'table' || !d.table) continue;
      for (const c of grid.columns) {
        const was = savedTable.rows[r.row]?.values ?? {};
        const key = Object.keys(was).find((k) => k.toLowerCase() === c.toLowerCase());
        if ((key === undefined ? null : was[key]) !== r.values[c]) out.add(`${r.table}:${r.row}:${c.toLowerCase()}`);
      }
    }
    return out;
  }, [grid, params, drafts]);

  const setCell = (t: number, row: number, name: string, text: string) => {
    const id = column?.table_ids[t];
    const base = tableAt(t);
    if (!id || !base || !column) return;
    const next = withCell(base, row, name, text);
    setSaid(null);
    setDraft(id, JSON.stringify(next) === JSON.stringify(params[id]) ? null : { kind: 'table', name: column.tables[t], table: next, why: null });
  };

  // Save the tables typed in and run the case again from its setup: the table's next revision, one undo each.
  const saveAndRun = async () => {
    if (!table || !edits) return;
    setSaving(true);
    try {
      for (const [id, t] of edits.tables) {
        const name = (summary?.objects ?? []).find((o) => o.id === id)?.name ?? id;
        const reply = await runRecord('parameters.save', { table: id, name, content: t });
        if (reply.error) return setSaid({ text: `${name}: ${reply.error}`, refused: true });
        setDraft(id, null);
      }
      const reply = await runRecord('reserves.run', await reservesRerunParams(table.id, []));
      setSaid(reply.error ? { text: reply.error, refused: true } : { text: `Saved, and ${table.name} run again from its setup.`, refused: false });
    } catch (e) {
      setSaid({ text: String(e), refused: true });
    } finally {
      setSaving(false);
    }
  };
  const discard = () => edits?.tables.forEach(([id]) => setDraft(id, null));

  // ↑ and ↓ step through the steps, away from a box being typed in.
  const steps = column?.steps ?? [];
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (step === null || e.ctrlKey || e.altKey || e.metaKey) return;
    if ((e.target as HTMLElement).closest('input, select, textarea, [contenteditable="true"]')) return;
    const by = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!by) return;
    e.preventDefault();
    setStep(Math.max(1, Math.min(steps.length - 1, step + by)));
  };

  if (!table) {
    return (
      <div className="p-3 text-xs text-muted-foreground" data-tool-panel="case">
        No reserves table yet: run a reserves dataflow over blocks, and its case opens here a step at a time.
      </div>
    );
  }

  const shown = view?.now ?? view?.saved ?? null;
  const addOns = Object.fromEntries((summary?.objects ?? []).filter((o) => o.kind === 'dataflow').map((o) => [o.id, o.name]));
  const addOnNames = [...new Set(steps.map((s) => s.dataflow).filter((d): d is string => !!d))].map((d) => addOns[d] ?? 'an add-on');
  const stages = stagesOf(steps, addOns);
  const ranHere = view?.now && view.now.ran_from !== null ? view.now.ran : [];
  const ranMs = new Map(ranHere.map((r) => [r.step, r.ms]));
  const readsTable = new Set((grid?.rows ?? []).map((r) => r.table));
  const settingsRead = cell ? stepSettings(cell) : [];
  const writes = cell ? writesByFolder(cell.writes ?? []) : [];
  const balance = shown?.balance;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-auto text-xs" data-tool-panel="case" data-case={shown?.table ?? ''} onKeyDown={onKey}>
      <PanelHead line="A reserves table's case a step at a time: what each step reads and does to the whole case, its settings to change and see it run again." />
      {/* The band (decision 8): the dataflow open, its case, no ground made or lost. */}
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1.5" data-slot="case-band">
        <span className="text-muted-foreground">Dataflow open</span>
        <span className="text-sm font-semibold" data-slot="case-dataflow">
          {column?.dataflow ?? '…'}
          {addOnNames.length > 0 && <span className="font-normal text-muted-foreground"> + {addOnNames.join(', ')}</span>}
        </span>
        <label className="flex items-center gap-1">
          <span className="text-muted-foreground">in case</span>
          <Select className="h-6 w-44" aria-label="The reserves table whose case to show" value={table.id} onChange={(e) => setPick(e.target.value)}>
            {tables.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </Select>
        </label>
        <span className="ml-auto flex flex-wrap items-center gap-x-3">
          {balance &&
            (balance.balanced ? (
              <StatusBadge tone="ok" title={`Every column at this step holds what came in from its blocks (${Math.round(balance.volume_in).toLocaleString('en-AU')} m³), to rounding`}>
                ✓ Balanced: no ground made or lost
              </StatusBadge>
            ) : (
              <StatusBadge tone="refused" title={balance.unbalanced.join('; ')}>
                Out of balance: {balance.unbalanced.join('; ')}
              </StatusBadge>
            ))}
          {shown && (
            <span className="text-muted-foreground" data-slot="case-counts">
              {shown.steps.length} steps · {shown.columns} block columns{shown.stopped.length > 0 && ` · ${shown.stopped.length} stopped`}
            </span>
          )}
        </span>
      </div>
      {/* The inputs (decision 8): the blocks, the seam order, each settings table, those the step reads lit gold. */}
      {column && shown && (
        <div className="flex shrink-0 flex-wrap gap-1.5 border-b border-border px-2 py-1.5" data-slot="case-inputs">
          <InputCard head="Blocks" body={`${shown.columns} block columns`} more={shown.stopped.length ? `${shown.stopped.length} stopped` : `${column.blocks.length} solids in this block`} lit={at === 0} />
          <InputCard head="Seam order" body={column.model ?? 'The run\'s own stack'} more={`${column.horizons} horizons`} lit={at === 0} />
          {column.tables.map((name, t) => {
            const id = column.table_ids[t];
            const editedHere = !!id && drafts[id]?.kind === 'table';
            return <InputCard key={`${name}${t}`} head="Settings" body={name} more={readsTable.has(t) ? 'read by this step' : 'not read by this step'} lit={readsTable.has(t)} edited={editedHere} />;
          })}
        </div>
      )}
      {error && <div className="px-2 py-1 text-destructive">{error}</div>}
      {liveWhy && <div className="px-2 py-1 text-warning" data-slot="live-why">Not run on the latest edit: {liveWhy}. Showing the last that ran.</div>}
      {!view && !error && <div className="p-3 text-muted-foreground">Running the case…</div>}
      {view && (
        <div className="grid h-[28rem] shrink-0 grid-cols-[minmax(11rem,15rem)_minmax(0,1fr)_minmax(14rem,20rem)] divide-x divide-border" data-slot="case-columns">
          {/* The steps by stage (decision 10). */}
          <div className="min-h-0 overflow-auto p-2" data-slot="case-steps">
            <div className="mb-1 text-muted-foreground">Steps · ↑ ↓ to step</div>
            {stages.map((s) => (
              <div key={`${s.name}${s.steps[0]}`} className="mb-1.5">
                <div className="text-[10px] font-semibold tracking-wide text-muted-foreground uppercase">{s.name}</div>
                {s.steps.map((n) => {
                  const x = steps[n];
                  return (
                    <button
                      key={n}
                      type="button"
                      data-slot="case-step"
                      data-step={x.step}
                      className={cn('flex w-full items-baseline gap-1.5 rounded px-1 py-0.5 text-left hover:bg-accent', n === at && 'bg-selection font-semibold text-primary')}
                      onClick={() => setStep(n)}
                    >
                      <span className="w-5 shrink-0 text-right text-muted-foreground tabular-nums">{n}</span>
                      <span className="min-w-0 flex-1 truncate">{stepLabel(x.step)}</span>
                      {ranMs.has(n) ? (
                        <span className="text-edited tabular-nums" title="Ran again on the last edit">
                          {Math.round(ranMs.get(n)!)} ms
                        </span>
                      ) : (
                        <span className="text-[10px] text-muted-foreground">{kindWords[x.kind]}</span>
                      )}
                    </button>
                  );
                })}
              </div>
            ))}
            <div className="mt-2 text-muted-foreground">After an edit the steps that ran again show their time; the steps before them were kept.</div>
          </div>
          {/* The step (decisions 8, 9 and 11). */}
          <div className="min-h-0 space-y-2 overflow-auto p-2" data-slot="case-step-pane">
            {cell && at !== null && shown && (
              <>
                <div className="flex flex-wrap items-baseline gap-x-2">
                  <span className="text-sm font-semibold">
                    {at} · {stepLabel(cell.step)}
                  </span>
                  <span className="text-muted-foreground">{kindWords[cell.kind]}</span>
                  {edits && (
                    <StatusBadge tone="edited" title="The case is run on unsaved edits">
                      run on edits
                    </StatusBadge>
                  )}
                </div>
                <div data-slot="case-rule" className={cn(cell.kind === 'built_in' && 'truncate')} title={cell.kind === 'built_in' ? cell.rule : undefined}>
                  {cell.kind === 'built_in' ? (
                    cell.rule
                  ) : (
                    <button type="button" className="inline-flex items-center gap-1 text-muted-foreground hover:text-foreground hover:underline" onClick={() => openTool('params', `formula:${cell.dataflow ?? column!.dataflow_id}:${cell.step}`)}>
                      <Braces className="size-3" /> Open its script in the formula panel
                    </button>
                  )}
                </div>
                <div className="grid grid-cols-[3.5rem_1fr] gap-x-2 gap-y-0.5">
                  <span className="text-muted-foreground">Reads</span>
                  <span>
                    {[...cell.fields_read, ...settingsRead].length === 0 && <span className="text-muted-foreground">nothing it names</span>}
                    {cell.fields_read.map((f) => (
                      <Chip key={`f${f}`}>{f}</Chip>
                    ))}
                    {settingsRead.map((f) => (
                      <Chip key={`s${f}`} lit>
                        {f}
                      </Chip>
                    ))}
                  </span>
                  <span className="text-muted-foreground">Writes</span>
                  <span>
                    {writes.length === 0 ? <span className="text-muted-foreground">nothing to the table</span> : writes.flatMap((w) => w.parts).map((w) => <Chip key={w.folder}>{w.folder}</Chip>)}
                  </span>
                </div>
                {/* The whole case first, so it's in sight as a setting is typed under it. */}
                <section className="space-y-1">
                  <div className="text-muted-foreground">Whole case at this step · every block{view.now?.ran_from ? ` · ran again from step ${view.now.ran_from}` : ''}</div>
                  <WholeCase saved={view.saved} now={view.now} />
                </section>
                <section className="space-y-1">
                  <div className="text-muted-foreground">
                    Settings by seam and ply · the rows read in order, the first that fits and holds a setting gives it · ringed: what {before.find((u) => u.id === focus) ? unitWords(before.find((u) => u.id === focus)!) : 'the unit followed'} used
                  </div>
                  {grid && <SettingsBySeam grid={grid} supplied={supplied} edited={editedCells} onSet={setCell} />}
                </section>
                <div className="flex flex-wrap items-center gap-2">
                  <Button size="sm" className="h-7" disabled={!edits || edits.tables.length === 0 || saving || !!edits.why} onClick={() => void saveAndRun()} title="Save the settings tables as typed, then run the case again from its setup: each its next revision, one undo each">
                    <Save className="size-3.5" /> {saving ? 'Saving…' : 'Save and run the case'}
                  </Button>
                  {edits && edits.tables.length > 0 && (
                    <Button size="sm" variant="outline" className="h-7" onClick={discard} title="Put the settings back as saved">
                      <Undo2 className="size-3.5" /> Discard
                    </Button>
                  )}
                  <span className={cn(edits ? 'text-edited' : 'text-muted-foreground')}>{edits ? 'Unsaved: the whole case runs again as you type.' : 'Type a setting to see the whole case run again.'}</span>
                  {edits?.dataflow && <span className="text-muted-foreground">The dataflow's own edits save in the Parameters tab.</span>}
                </div>
                {said && <div className={said.refused ? 'text-destructive' : 'text-ok'}>{said.text}</div>}
              </>
            )}
          </div>
          {/* The block map and the picked block's strip log (decision 8). */}
          <div className="min-h-0 space-y-2 overflow-auto p-2" data-slot="case-blocks">
            <BlockMap blocks={view.blocks} picked={block} onPick={setBlock} />
            {block && (
              <div className="space-y-1">
                <div className="font-semibold">{block.join(' / ')}</div>
                {column && at !== null ? <StripLog before={at > 0 ? before : after} after={after} fields={column.fields} followed={focus} onFollow={(u) => setFocus(u.id)} /> : <div className="text-muted-foreground">Running the block…</div>}
              </div>
            )}
          </div>
        </div>
      )}
      {/* The units table under the step view (decision 12). */}
      {view && column && at !== null && block && <UnitsTable column={column} at={at} block={block} />}
    </div>
  );
}

/** An input as a card: what it is, its name, and whether the step reads it (lit gold) or it's run as edited (blue). */
function InputCard({ head, body, more, lit, edited = false }: { head: string; body: string; more: string; lit: boolean; edited?: boolean }) {
  return (
    <div className={cn('min-w-32 rounded border px-2 py-1', lit ? 'border-primary bg-selection' : 'border-border')} data-slot="case-input" data-lit={lit ? 'yes' : 'no'}>
      <div className="text-[10px] tracking-wide text-muted-foreground uppercase">{head}</div>
      <div className={cn('font-semibold', edited && 'text-edited')}>{body}</div>
      <div className="text-muted-foreground">{edited ? 'run as edited' : more}</div>
    </div>
  );
}

/** A name the step reads or writes: a setting lit gold, a field or folder plain. */
function Chip({ children, lit = false }: { children: string; lit?: boolean }) {
  return <span className={cn('mr-1 mb-0.5 inline-block rounded border px-1 font-mono text-[11px]', lit ? 'border-primary text-primary' : 'border-border')}>{children}</span>;
}
