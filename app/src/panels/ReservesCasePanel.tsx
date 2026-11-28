import { Box, Braces, RotateCw, Save, Settings2, Undo2 } from 'lucide-react';
import { type KeyboardEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { type CaseState, caseState, type CaseStepView, type ColumnSteps, type ColumnUnit, type DataflowState, objectDetails, type ParameterTable, reservesCaseStep, reservesColumn, reservesColumnTrace, reservesRerunParams, reservesSolids } from '@/bridge';
import { openItem, viewBeside } from '@/workspace/tablesSide';
import { Disclosure, ok, PanelHead, refused, type Said, SaidLine, StatusBadge, TextButton } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { type Draft, useWorkspace } from '@/workspace/context';
import { stepLabel, unitWords } from './columnSteps';
import { writesByFolder } from './dataflowView';
import { CaseFlow } from './CaseFlow';
import { CaseSetup } from './CaseSetup';
import { CaseFigures, CaseForms, CasePills, useCases } from './CasesBar';
import { cellSummaries, focusOf } from './notebook';
import { type CellTrace, StepPane } from './StepPane';
import { rangeWords } from './flowStages';
import { DataflowChain } from './CaseSwitcher';
import { BlockMap, SettingsBySeam, StripLog, WholeCase } from './ReservesCaseParts';
import { RowDataflowView } from './RowDataflowView';
import { UnitsTable } from './UnitsTable';
import { type CaseFold, caseFold, mapMeasure, settingsGrid, shadeOf, stagesOf, stepSettings, suppliedFor, withCell } from './reservesCase';

/**
 * The Reserves case tab (the Build Plan's "Reserves case plan", decisions 8, 9 and 11, workstream 4; laid out by the
 * "Case tab layout plan", workstream 1), now the Tables side's Cases item (decision 0108, workstream 4): a reserves table's
 * case a step at a time. Across the top one header line: + for New, Copy and Add a dataflow, the case's dataflows as a
 * chain with Setup, and the open case's figures against Base with whether any ground is made or lost; the case itself is
 * picked in the list, and run from the top row. Under it the flow, which is the step list; then the step picked (its rule, what it
 * reads, settings tables lit gold, and writes, its settings by seam and ply to type in, the whole case at the step against
 * the last run), and the block map with the picked block's strip log before and after. An edit, here or in the Parameters
 * tab, runs the whole case again from the first step that reads it as it's typed; Save and run the case keeps it.
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

export function ReservesCasePanel({ caseTable = null }: { caseTable?: string | null }) {
  const { summary, selection, notebookOf, drafts, setDraft, runRecord, openTool, setSelection, zoomTo } = useWorkspace();
  const tables = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own), [summary]);
  const [pick, setPick] = useState<string | null>(caseTable);
  const followed = notebookOf?.table ?? null;
  const last = selection[selection.length - 1] ?? null;
  useEffect(() => {
    if (last && tables.some((t) => t.id === last)) setPick(last);
  }, [last, tables]);
  useEffect(() => {
    if (followed) setPick(followed);
  }, [followed]);
  // The case opened in the Tables side's list (decision 0108, workstream 4).
  useEffect(() => {
    if (caseTable) setPick(caseTable);
  }, [caseTable]);
  const table = tables.find((t) => t.id === pick) ?? tables[tables.length - 1] ?? null;
  // A case opened here another way (a table picked, Open in the case) is the one the list and the top row show.
  useEffect(() => {
    if (table && caseTable && table.id !== caseTable) openItem('case', table.id);
  }, [table?.id]);
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
  const [said, setSaid] = useState<Said | null>(null);
  const [saving, setSaving] = useState(false);
  // The case's dataflows and which is open (decision 4, workstream 6): none picked, its reserves dataflow.
  const [state, setState] = useState<CaseState | null>(null);
  const [openFlow, setOpenFlow] = useState<string | null>(null);
  const [caseSaid, setCaseSaid] = useState<Said | null>(null);
  // The case's Setup, its own view in place of the working area (Case setup and unit plan, decision 1), and the step unit
  // by unit.
  const [setupOpen, setSetupOpen] = useState(false);
  const [unitOpen, setUnitOpen] = useState(false);
  const [rerunning, setRerunning] = useState(false);
  const cases = useCases();
  const [foldRef, fold] = useCaseFold();

  // A new table starts again: its first step that does something, its busiest block.
  useEffect(() => {
    setStep(null);
    setBlock(null);
    setView(null);
    setColumn(null);
    setSaid(null);
    setOpenFlow(null);
    setCaseSaid(null);
  }, [table?.id]);

  // Whether each of the case's dataflows is up to date, read again whenever anything moves.
  useEffect(() => {
    if (!table) return setState(null);
    let current = true;
    void caseState(table.id)
      .then((s) => current && setState(s))
      .catch(() => current && setState(null));
    return () => {
      current = false;
    };
  }, [table?.id, changed]);
  const openRow: DataflowState | null = state?.dataflows.find((d) => !d.reserves && d.dataflow === openFlow) ?? null;
  const reservesState = state?.dataflows.find((d) => d.reserves) ?? null;
  const openState = openRow ?? reservesState;

  const edits = useMemo(() => editsOf(column, drafts), [column, drafts]);
  // The case at the step: at once for a new table, step or save; a third of a second after typing stops for an edit.
  const lastEdits = useRef<string | null>(null);
  useEffect(() => {
    if (!table || openRow) return;
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
  }, [table?.id, step, edits?.key, edits?.why, changed, !!openRow]);

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

  // The block sent from the Reserves table's Open in the case: the block column it's in, once the case has run.
  const sentBlock = useRef(0);
  useEffect(() => {
    if (!view || !notebookOf || notebookOf.n === sentBlock.current || notebookOf.node.length === 0 || notebookOf.table !== table?.id) return;
    const node = notebookOf.node;
    const hit = view.blocks.find((b) => b.path.length <= node.length && b.path.every((w, i) => node[i] === w));
    if (!hit) return;
    sentBlock.current = notebookOf.n;
    setOpenFlow(null);
    setBlock(hit.path);
  }, [notebookOf?.n, view]);

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

  // The step sent from the formula panel's Show in the case: that step, once the column has it.
  const sentStep = useRef(0);
  useEffect(() => {
    const edited = notebookOf?.edited;
    if (!column || !edited || !notebookOf || notebookOf.n === sentStep.current) return;
    const k = column.steps.findIndex((s) => s.step === edited.step && (s.dataflow ?? column.dataflow_id) === edited.dataflowId);
    if (k < 1) return;
    sentStep.current = notebookOf.n;
    setOpenFlow(null);
    setStep(k);
  }, [notebookOf?.n, column]);

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
        if (reply.error) return setSaid(refused(`${name}: ${reply.error}`));
        setDraft(id, null);
      }
      const reply = await runRecord('reserves.run', await reservesRerunParams(table.id, []));
      setSaid(reply.error ? refused(reply.error) : ok(`Saved, and ${table.name} run again from its setup.`));
    } catch (e) {
      setSaid(refused(String(e)));
    } finally {
      setSaving(false);
    }
  };
  const discard = () => edits?.tables.forEach(([id]) => setDraft(id, null));

  // The step unit by unit (the Dataflow tab's step pane, decisions 0081 and 0082): the unit followed's workings, a
  // formula traced on it, kept by what it ran on.
  const summaries = useMemo(() => (column ? cellSummaries(column.steps) : []), [column]);
  const into = before.find((u) => u.id === focus) ?? null;
  const allocation = !!column && at !== null && at > 0 && column.steps[at].kind === 'allocation';
  const traced = allocation ? (before[0] ?? null) : into;
  const wanted = unitOpen && table && block && column && at !== null && at > 0 && (column.steps[at].kind === 'formula' || allocation) && traced ? `${table.id}|${block.join('/')}|${edits?.key ?? ''}|${at}|${allocation ? 'column' : traced.id}` : null;
  const [traces, setTraces] = useState<Map<string, CellTrace>>(new Map());
  useEffect(() => {
    if (!wanted || !table || !block || at === null || !traced || traces.has(wanted)) return;
    setTraces((m) => new Map(m).set(wanted, { state: 'loading' }));
    void reservesColumnTrace(table.id, block, at, traced.id, edits?.dataflow ?? null, edits?.tables ?? [])
      .then((trace) => setTraces((m) => new Map(m).set(wanted, { state: 'done', trace })))
      .catch((e) => setTraces((m) => new Map(m).set(wanted, { state: 'failed', why: String(e) })));
  }, [wanted]);
  const trace = wanted ? (traces.get(wanted) ?? { state: 'loading' }) : null;

  // The picked block, its strip or every block run again into the table from its setup (decision 0085): its next revision.
  const runAgain = async (only: string[]) => {
    if (!table) return;
    setRerunning(true);
    setCaseSaid(null);
    try {
      const reply = await runRecord('reserves.run', await reservesRerunParams(table.id, only));
      setCaseSaid(reply.error ? refused(reply.error) : ok(`${only.length ? only.join(' › ') : 'Every block'} run again into ${table.name}.`));
    } catch (e) {
      setCaseSaid(refused(String(e)));
    } finally {
      setRerunning(false);
    }
  };

  // ↑ and ↓ step through the steps, away from a box being typed in.
  const steps = column?.steps ?? [];
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    if ((e.target as HTMLElement).closest('input, select, textarea, [contenteditable="true"]')) return;
    // Esc leaves the Setup for the case (decision 1).
    if (setupOpen && e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      return setSetupOpen(false);
    }
    if (setupOpen || step === null) return;
    const by = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0;
    if (!by) return;
    e.preventDefault();
    setStep(Math.max(1, Math.min(steps.length - 1, step + by)));
  };

  if (!table) {
    return (
      <div className="text-xs" data-tool-panel="case">
        <div className="flex items-center gap-2 border-b border-border px-2 py-1.5" data-slot="case-band">
          <CasePills cases={cases} tables={[]} open={null} more="" onOpen={setPick} />
        </div>
        <CaseForms cases={cases} open={null} onOpen={setPick} onOpenDataflow={setOpenFlow} />
        <p className="p-3 text-muted-foreground">No case yet: New case, under +, runs a reserves dataflow over blocks, and the case opens here a step at a time.</p>
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
  // The open case's own tip on its pill (decision 4 and 7): its blocks, seam model and steps, said once.
  const caseMore = column && shown ? `${shown.columns} block columns · ${column.model ?? "the run's own seam stack"}, ${column.horizons} horizons · ${shown.steps.length} steps` : '';

  // The step (decisions 8, 9 and 11).
  const stepPane = view && (
    <div className={cn('space-y-2 p-2', fold === 'three' && 'min-h-0 overflow-auto')} data-slot="case-step-pane">
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
              <TextButton className="gap-1" onClick={() => openTool('params', `formula:${cell.dataflow ?? column!.dataflow_id}:${cell.step}`)}>
                <Braces className="size-3" /> Open its script in the formula panel
              </TextButton>
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
              {/* The settings tables it reads them from (Case tab layout plan, decision 4: the input cards' place). */}
              {readsTable.size > 0 && <span className="mr-1 text-muted-foreground">from</span>}
              {column &&
                column.tables.map((name, t) => {
                  if (!readsTable.has(t)) return null;
                  const id = column.table_ids[t];
                  const editedHere = !!id && drafts[id]?.kind === 'table';
                  return (
                    <button
                      key={`t${t}`}
                      type="button"
                      className={cn('mr-1 mb-0.5 inline-block rounded border border-primary bg-selection px-1 text-[11px] hover:underline', editedHere ? 'text-edited' : 'text-primary')}
                      title={`${name}: a settings table this step reads${editedHere ? ', run as edited' : ''}. Open it in the Parameters tab`}
                      onClick={() => id && openTool('params', `show:${id}`)}
                      data-slot="case-reads-table"
                    >
                      {name}
                    </button>
                  );
                })}
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
            <div className="text-muted-foreground" title={`The rows read in order: the first that fits and holds a setting gives it. Ringed: what ${into ? unitWords(into) : 'the unit followed'} used.`}>
              Settings by seam and ply
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
          <SaidLine said={said} />
          <section className="border-t border-border pt-1" data-slot="case-unit-by-unit">
            <Disclosure open={unitOpen} onToggle={() => setUnitOpen((o) => !o)}>
              This step unit by unit: {into ? unitWords(into) : 'the unit followed'}'s workings{cell.kind === 'formula' ? ' and its formula traced' : ''}
            </Disclosure>
            {unitOpen && column && (
              <StepPane
                column={column}
                at={at}
                summary={summaries[at]}
                focus={into ? focusOf(into) : null}
                trace={trace}
                onEdit={cell.kind === 'formula' || cell.kind === 'allocation' ? () => openTool('params', `formula:${cell.dataflow ?? column.dataflow_id}:${cell.step}`) : undefined}
                onWhole={() => setUnitOpen(false)}
                onSetting={(t, row, name) => openTool('params', `cell:${t}:${row}:${name}`)}
              />
            )}
          </section>
        </>
      )}
    </div>
  );
  /** A block's solids picked and framed in the view: the Tables side's 3D, on demand (decision 0108). */
  const showIn3d = async (path: string[]) => {
    if (!table) return;
    const ids = await reservesSolids(table.id, path);
    if (ids.length === 0) return setCaseSaid(refused(`${path.join(' › ')} has no solids in the project to show`));
    setSelection(ids);
    // With the view beside the case (both shown, or popped out): framed there, the Tables side kept (decision 0109).
    if (!viewBeside()) openTool('properties');
    zoomTo(ids);
  };
  // With the view beside it (both shown, or popped out), the block picked on the map is picked in the view (decision 0109).
  useEffect(() => {
    if (!table || !block || !viewBeside()) return;
    let current = true;
    void reservesSolids(table.id, block).then((ids) => current && ids.length > 0 && setSelection(ids));
    return () => {
      current = false;
    };
  }, [table?.id, block?.join('/')]);
  // The block map and the picked block's strip log (decision 8).
  const blocksPane = view && (
    <div className={cn('space-y-2 p-2', fold === 'three' && 'min-h-0 overflow-auto')} data-slot="case-blocks">
      <BlockMap blocks={view.blocks} picked={block} onPick={setBlock} />
      <div className="flex flex-wrap items-center gap-1" data-slot="case-run-again">
        <RotateCw className={cn('size-3 text-muted-foreground', rerunning && 'animate-spin')} />
        <span className="text-muted-foreground" title="Every block runs from Run, in the header">Run again</span>
        {block && (
          <Button size="sm" variant="outline" className="h-5 px-1.5 text-[11px]" disabled={rerunning} onClick={() => void runAgain(block)} title={`${block.join(' › ')} run again into the table from its setup, the rest kept`}>
            this block
          </Button>
        )}
        {block && block.length > 1 && (
          <Button size="sm" variant="outline" className="h-5 px-1.5 text-[11px]" disabled={rerunning} onClick={() => void runAgain(block.slice(0, -1))} title={`Every block of ${block.slice(0, -1).join(' › ')} run again into the table`}>
            its strip
          </Button>
        )}
      </div>
      {block && (
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="font-semibold">{block.join(' / ')}</span>
            {/* The view on demand (decision 0108, workstream 4): the block's solids picked and framed on Design. */}
            <Button size="sm" variant="outline" className="ml-auto h-5 gap-1 px-1.5 text-[11px]" onClick={() => void showIn3d(block)} data-slot="case-show-3d" title={`${block.join(' › ')}: its solids picked and framed in the view, on Design`}>
              <Box className="size-3" /> Show in 3D
            </Button>
          </div>
          {column && at !== null ? <StripLog before={at > 0 ? before : after} after={after} fields={column.fields} followed={focus} onFollow={(u) => setFocus(u.id)} /> : <div className="text-muted-foreground">Running the block…</div>}
        </div>
      )}
    </div>
  );
  // The units table (decision 12), in its own column filling the height (Case tab layout plan, decision 6).
  const unitsPane = (
    <div className={cn(fold !== 'one' && 'min-h-0 overflow-auto')} data-slot="case-units">
      {column && at !== null && block ? <UnitsTable column={column} at={at} block={block} fill={fold !== 'one'} /> : <div className="p-2 text-muted-foreground">Pick a block on the map to see its units before and after the step.</div>}
    </div>
  );

  return (
    <div ref={foldRef} className="flex h-full min-h-0 flex-col overflow-auto text-xs" data-tool-panel="case" data-case={shown?.table ?? ''} onKeyDown={onKey}>
      <PanelHead line="A reserves table's case a step at a time: what each step reads and does to the whole case, its settings to change and see it run again." />
      {/* The header (Case tab layout plan, decisions 1 to 3): the cases, the case's dataflows, its figures, no ground made or lost. */}
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1.5" data-slot="case-band">
        {/* The cases are the list's, Run the top row's (decision 0108, workstream 4): here New, Copy and Add a dataflow. */}
        <CasePills cases={cases} tables={tables} open={table.id} more={caseMore} onOpen={setPick} pills={false} />
        <span className="h-4 w-px bg-border" />
        <DataflowChain state={state} open={openState?.dataflow ?? null} addOns={addOnNames} fallback={column?.dataflow ?? '…'} onOpen={(d) => (setSetupOpen(false), setOpenFlow(d.reserves ? null : d.dataflow))} />
        {openRow && <span className="text-muted-foreground">rows, on {rangeWords(openRow)}</span>}
        <Button size="sm" variant={setupOpen ? 'secondary' : 'outline'} className="h-6 gap-1 px-2" onClick={() => setSetupOpen((o) => !o)} data-slot="case-setup" title="What fills this case: its blocks, dataflows, settings tables in order and where each field comes from">
          <Settings2 className="size-3" /> Setup
        </Button>
        <span className="ml-auto flex flex-wrap items-center gap-x-3">
          {fold !== 'one' && <CaseFigures cases={cases} open={table.id} />}
          {!openRow && shown && shown.stopped.length > 0 && <span className="text-destructive">{shown.stopped.length} block columns stopped</span>}
          {!openRow &&
            balance &&
            (balance.balanced ? (
              <StatusBadge tone="ok" title={`No ground made or lost: every column at this step holds what came in from its blocks (${Math.round(balance.volume_in).toLocaleString('en-AU')} m³), to rounding`}>
                ✓ Balanced
              </StatusBadge>
            ) : (
              <StatusBadge tone="refused" title={balance.unbalanced.join('; ')}>
                Out of balance: {balance.unbalanced.join('; ')}
              </StatusBadge>
            ))}
        </span>
      </div>
      <CaseForms cases={cases} open={table.id} onOpen={setPick} onOpenDataflow={(d) => (setSetupOpen(false), setOpenFlow(d))} />
      {setupOpen && <CaseSetup table={table.id} onDone={() => setSetupOpen(false)} />}
      <SaidLine said={caseSaid} className="px-2 py-1" slot="case-said" />
      {!setupOpen && openRow && <RowDataflowView table={table} dataflow={openRow} />}
      {!setupOpen && !openRow && error && <div className="px-2 py-1 text-destructive">{error}</div>}
      {!setupOpen && !openRow && liveWhy && (
        <div className="px-2 py-1 text-warning" data-slot="live-why">
          Not run on the latest edit: {liveWhy}. Showing the last that ran.
        </div>
      )}
      {!setupOpen && !openRow && !view && !error && <div className="p-3 text-muted-foreground">Running the case…</div>}
      {/* The flow (decision 10), and the step list (Case tab layout plan, decision 5): the case through every stage, as
          edited where there are edits; ↑ and ↓ step through it. */}
      {!setupOpen && !openRow && view && column && (
        <CaseFlow
          flow={view.now_flow ?? view.saved_flow}
          saved={view.now_flow ? view.saved_flow : null}
          stages={stages}
          names={steps.map((x) => x.step)}
          kinds={steps.map((x) => kindWords[x.kind])}
          ranMs={ranMs}
          at={at}
          onStep={(n) => setStep(Math.max(0, n))}
        />
      )}
      {/* The working area (Case tab layout plan, decisions 6 and 8): step, units table and block map side by side to the
          foot of the dock; narrower, the block map under the step, then the units table under both. */}
      {!setupOpen && !openRow && view && (
        <div
          className={cn(
            'divide-border',
            fold === 'three' && 'grid min-h-[20rem] flex-1 grid-cols-[minmax(22rem,1fr)_minmax(0,1.4fr)_minmax(16rem,0.9fr)] grid-rows-[minmax(0,1fr)] divide-x',
            fold === 'two' && 'grid min-h-[20rem] flex-1 grid-cols-[minmax(20rem,1fr)_minmax(0,1.3fr)] grid-rows-[minmax(0,1fr)] divide-x',
            fold === 'one' && 'flex flex-col divide-y',
          )}
          data-slot="case-columns"
          data-fold={fold}
        >
          {fold === 'three' && (
            <>
              {stepPane}
              {unitsPane}
              {blocksPane}
            </>
          )}
          {fold === 'two' && (
            <>
              <div className="min-h-0 divide-y divide-border overflow-auto">
                {stepPane}
                {blocksPane}
              </div>
              {unitsPane}
            </>
          )}
          {fold === 'one' && (
            <>
              {stepPane}
              {blocksPane}
              {unitsPane}
            </>
          )}
        </div>
      )}
    </div>
  );
}

/** A name the step reads or writes: a setting lit gold, a field or folder plain. */
function Chip({ children, lit = false }: { children: string; lit?: boolean }) {
  return <span className={cn('mr-1 mb-0.5 inline-block rounded border px-1 font-mono text-[11px]', lit ? 'border-primary text-primary' : 'border-border')}>{children}</span>;
}

/** The tab's width, watched, as a fold (Case tab layout plan, decision 8): the dock's own width, not the window's. */
function useCaseFold() {
  const [fold, setFold] = useState<CaseFold>('three');
  const watching = useRef<ResizeObserver | null>(null);
  const ref = useCallback((element: HTMLDivElement | null) => {
    watching.current?.disconnect();
    watching.current = null;
    if (!element) return;
    const look = () => setFold(caseFold(element.clientWidth));
    look();
    watching.current = new ResizeObserver(look);
    watching.current.observe(element);
  }, []);
  return [ref, fold] as const;
}
