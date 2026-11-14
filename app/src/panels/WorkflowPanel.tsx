import { ArrowDown, ArrowUp, Eye, Layers, Pause, Play, Plus, SkipForward, SlidersHorizontal, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import type { LogEntry, Note, ObjectSummary, Workflow, WorkflowRecord, WorkflowStep } from '@/bridge';
import { objectDetails, projectSummary } from '@/bridge';
import { Field, IconButton, plainNote, type RunReport, Select, StatusBadge, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { fmt } from './format';
import { BASE, caseValue, changesOfCase, onlyLayers, ruleWords, type SettingRow, settingsOfStep, stepsOfCase, tidyLayers, withSetting } from './workflow';

/**
 * Workflow (decision 0065): saved steps run again in one go. A workflow is captured from runs in the Run log, or built
 * by adding steps from the tool panels in Add to workflow mode; each step is a command with its settings and a rule for
 * each input, or a pause. Run runs every step in order, Run from here keeps what the steps before made, a pause stops
 * the run for hand work, and the first refusal stops it with that step's notes. A run is one undo. Made from the kit.
 *
 * Seeing what a step did: a click on a step selects what it last made and zooms to it, and its eye shows only that,
 * every other layer off until Show all again. Tidy after a run (on by default) leaves on screen what the run started
 * from and what it ended with: the layers of what its last step made are shown, the layers only the steps in between
 * made are hidden, and a layer holding anything drawn by hand is left as it is.
 *
 * Cases (decision 0066): Base is the steps as they stand; a case changes some settings and runs on its own layers
 * (`8 blocks 50m`), tagging what it makes with its case. The case picked is the one Run runs, and the one the steps'
 * badges, a click on a step, its eye and its settings show; Run all cases runs Base and every case as one undo.
 *
 * Settings sit with their step: its sliders open them under it, as the case picked has them. In Base they're the
 * step's own; in a case an empty box reads as Base (its value greyed in it) and what the case changes shows in blue,
 * with a blue badge on the step saying how many it changes.
 */

/** Tidy after a run: on unless turned off, remembered on this machine. */
const TIDY_KEY = 'zd.workflow.tidy';
const readTidy = () => {
  try {
    return localStorage.getItem(TIDY_KEY) !== 'off';
  } catch {
    return true;
  }
};

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** What a workflow run said, as the panel's result: what each step that stopped it found. */
function reportOf(record: WorkflowRecord): RunReport {
  const ran = record.steps.filter((s) => s.outcome === 'ran').length;
  const notes: Note[] = [plainNote(`${plural(ran, 'step')} run from step ${record.from} in ${fmt(record.duration_ms / 1000, 1)} s`)];
  if (record.status === 'paused') notes.push(plainNote(`Paused at step ${record.at}: Continue runs on from step ${record.at + 1}`));
  const stopped = record.steps.find((s) => s.outcome === 'refused');
  if (stopped) notes.push(...(stopped.checked ?? []).filter((n) => n.severity !== 'info'));
  return { notes, error: stopped ? `Step ${stopped.position} (${stopped.command}) refused: ${stopped.refused ?? 'see the run log'}` : undefined };
}

export function WorkflowPanel() {
  const { summary, log, runRecord, runWorkflow, runAllCases, addingTo, setAddingTo, setSelection, zoomTo, setLayersVisible } = useWorkspace();
  const [pickedCase, setPickedCase] = useState(BASE);
  const [newCase, setNewCase] = useState('');
  const [tidy, setTidy] = useState(readTidy);
  // Only one step on show: which, and how the layers stood before, to put back.
  const [only, setOnly] = useState<{ step: number; was: { id: string; visible: boolean }[] } | null>(null);
  const workflows = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'workflow'), [summary]);
  const [picked, setPicked] = useState<string | null>(null);
  const chosen: ObjectSummary | null = workflows.find((o) => o.id === picked) ?? workflows[workflows.length - 1] ?? null;
  const [content, setContent] = useState<{ id: string; revision: number; workflow: Workflow } | null>(null);
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<number | null>(null);
  // The steps whose settings are open under them, by id.
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());
  const toggleSettings = (step: number) =>
    setOpened((was) => {
      const next = new Set(was);
      if (!next.delete(step)) next.add(step);
      return next;
    });

  // The workflow's steps, read again whenever it's saved as a new revision.
  const id = chosen?.id ?? null;
  const revision = chosen?.revision ?? 0;
  useEffect(() => {
    if (!id) return;
    let stale = false;
    void objectDetails(id).then((details) => {
      if (!stale && details.measures.kind === 'workflow') setContent({ id, revision, workflow: details.measures.workflow });
    });
    return () => {
      stale = true;
    };
  }, [id, revision]);
  // The revision before shows until the new one has been read.
  const workflow = content && content.id === id ? content.workflow : null;
  const cases = workflow?.cases ?? [];
  const caseName = pickedCase === BASE || cases.some((c) => c.name === pickedCase) ? pickedCase : BASE;

  // The last run of this workflow's case, from the log: where Continue goes after a pause.
  const last = useMemo(() => {
    const runs = log.filter((e): e is Extract<LogEntry, { event: 'workflow' }> => e.event === 'workflow' && e.workflow === chosen?.id && (e.case ?? BASE) === caseName);
    return runs[runs.length - 1] ?? null;
  }, [log, chosen, caseName]);
  const continueFrom = last?.status === 'paused' && workflow && last.at < workflow.steps.length ? last.at + 1 : null;

  const save = async (next: Workflow) => {
    if (!chosen) return;
    const outcome = await runRecord('workflow.save', { workflow: chosen.id, name: chosen.name, content: next });
    if (outcome.error) setReport({ notes: [], error: outcome.error });
  };
  const edit = (change: (steps: WorkflowStep[]) => WorkflowStep[]) => {
    if (!workflow) return;
    const steps = change(workflow.steps.map((s) => ({ ...s })));
    // A step taken out takes what each case changed of it, and its runs in each case, with it.
    const kept = new Set(steps.map((s) => String(s.id)));
    const keep = <T,>(m: Record<string, T> | undefined) => Object.fromEntries(Object.entries(m ?? {}).filter(([k]) => kept.has(k)));
    void save({ ...workflow, steps, cases: (workflow.cases ?? []).map((c) => ({ ...c, settings: keep(c.settings), last: keep(c.last) })) });
  };
  const move = (at: number, by: number) =>
    edit((steps) => {
      const [step] = steps.splice(at, 1);
      steps.splice(at + by, 0, step);
      return steps;
    });
  const nextId = (steps: WorkflowStep[]) => Math.max(0, ...steps.map((s) => s.id)) + 1;

  const run = async (from: number) => {
    if (!chosen) return;
    setBusy(true);
    const outcome = await runWorkflow(chosen.id, from, caseName);
    if (outcome.record && tidy) await tidyAfter(chosen.id, outcome.record);
    setBusy(false);
    setReport(outcome.record ? reportOf(outcome.record) : { notes: [], error: outcome.error });
  };

  /** Base and every case, from their first steps, as one undo. */
  const runAll = async () => {
    if (!chosen) return;
    setBusy(true);
    const outcome = await runAllCases(chosen.id);
    const records = outcome.records ?? [];
    if (records.length > 0 && tidy) await tidyAfter(chosen.id, records[records.length - 1], records.map((r) => r.case));
    setBusy(false);
    if (!outcome.records) return setReport({ notes: [], error: outcome.error });
    const reports = records.map((r) => ({ case: r.case, report: reportOf(r) }));
    const failed = reports.find((r) => r.report.error);
    setReport({
      notes: reports.flatMap((r) => r.report.notes.map((n, k) => (k === 0 ? { ...n, text: `${r.case}: ${n.text}` } : n))),
      error: failed ? `${failed.case}: ${failed.report.error}` : undefined,
    });
  };

  /** A setting changed under its step: Base's are the step's own, a case's are what it changes. */
  const setCell = (name: string, row: SettingRow, text: string) => {
    if (!workflow) return;
    const next = withSetting(workflow, name, row, text);
    if (!next) return setReport({ notes: [], error: `'${text}' isn't a ${typeof row.base === 'number' ? 'number' : 'true or false'}, as ${row.field} has to be` });
    void save(next);
  };

  const addCase = () => {
    const name = newCase.trim();
    if (!workflow || !name) return;
    setNewCase('');
    setPickedCase(name);
    void save({ ...workflow, cases: [...cases, { name }] });
  };

  /** The run's inputs and its result on screen, the layers in between hidden: for every case run, when several are. */
  const tidyAfter = async (id: string, record: WorkflowRecord, ran: string[] = [record.case ?? BASE]) => {
    const last = [...record.steps].reverse().find((s) => s.outcome === 'ran');
    if (!last) return;
    const [details, now] = await Promise.all([objectDetails(id), projectSummary()]);
    if (details.measures.kind !== 'workflow') return;
    const all = details.measures.workflow;
    const made = [BASE, ...(all.cases ?? []).map((c) => c.name)].flatMap((name) => stepsOfCase(all, name).map((s) => s.last?.made ?? []));
    const result = ran.flatMap((name) => stepsOfCase(all, name).find((s) => s.id === last.step)?.last?.made ?? []);
    const { show, hide } = tidyLayers(made, result, now.objects);
    setOnly(null);
    await setLayersVisible(show, hide);
  };

  /** What a step last made, selected and zoomed to. */
  const showStep = (step: WorkflowStep) => {
    const made = (step.last?.made ?? []).filter((id) => summary?.objects.some((o) => o.id === id));
    if (made.length === 0) return;
    setSelection(made);
    zoomTo(made);
  };

  /** Only what a step last made on show; again, or Show all again, puts the layers back as they were. */
  const onlyStep = async (step: WorkflowStep) => {
    const layers = summary?.layers ?? [];
    if (only) {
      const was = only.was;
      setOnly(null);
      await setLayersVisible(
        was.filter((l) => l.visible).map((l) => l.id),
        was.filter((l) => !l.visible).map((l) => l.id),
      );
      if (only.step === step.id) return;
    }
    const { show, hide } = onlyLayers(step.last?.made ?? [], summary?.objects ?? [], layers);
    if (show.length === 0) return;
    setOnly({ step: step.id, was: layers.map((l) => ({ id: l.id, visible: l.visible })) });
    await setLayersVisible(show, hide);
  };

  const steps = workflow ? stepsOfCase(workflow, caseName) : [];
  const changes = workflow ? changesOfCase(workflow, caseName) : new Map<number, number>();
  const columns = [BASE, ...cases.map((c) => c.name)];
  const ofCase = caseName === BASE ? '' : ` of ${caseName}`;
  const action: ToolAction | undefined = chosen
    ? continueFrom
      ? { label: `Continue${ofCase} from step ${continueFrom}`, icon: <SkipForward className="size-3.5" />, disabled: !workflow, busy, busyLabel: 'Running…', title: 'Runs on from the step after the pause', run: () => run(continueFrom) }
      : {
          label: `Run ${plural(steps.filter((s) => s.on !== false && s.action.kind === 'command').length, 'step')}${ofCase}`,
          icon: <Play className="size-3.5" />,
          disabled: !workflow || steps.length === 0,
          busy,
          busyLabel: 'Running…',
          title: 'Every step in order, to the end, a pause or the first refusal; what each made last time is made again as its next revision',
          hint: 'One undo · Ctrl+Enter',
          run: () => run(1),
        }
    : undefined;

  const adding = addingTo !== null && chosen !== null && addingTo.id === chosen.id;

  return (
    <ToolPanel
      title="Workflow"
      panel="workflow"
      summary="Saved steps run again in one go: capture runs from the Run log, or add steps from the tool panels."
      result={report}
      action={action}
    >
      <ToolSection title="Workflow">
        {workflows.length === 0 ? (
          <div className="text-muted-foreground">No workflow yet: tick runs in the Run log and Capture them, the quickest way to start.</div>
        ) : (
          <Field label="Workflow">
            <Select value={chosen?.id ?? ''} aria-label="Workflow" onChange={(e) => setPicked(e.target.value)}>
              {workflows.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </Select>
          </Field>
        )}
      </ToolSection>

      {chosen && workflow && (
        <ToolSection
          title="Cases"
          aside={
            cases.length > 0 ? (
              <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={busy} title="Base and every case from their first steps, each on its own layers, as one undo" onClick={() => void runAll()}>
                <Layers className="size-3.5" /> Run all {columns.length} cases
              </Button>
            ) : undefined
          }
        >
          <Field label="Case" hint="Run runs this one; the steps show what it made">
            <Select value={caseName} aria-label="Case" onChange={(e) => setPickedCase(e.target.value)}>
              {columns.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </Select>
            {caseName !== BASE && (
              <IconButton
                label={`Take case ${caseName} out (what it made stays, on its own layers)`}
                onClick={() => {
                  setPickedCase(BASE);
                  void save({ ...workflow, cases: cases.filter((c) => c.name !== caseName) });
                }}
              >
                <Trash2 />
              </IconButton>
            )}
          </Field>
          <Field label="New case" hint="The steps as Base, on layers named for it, until you change a step's settings">
            <Input className="h-7 w-28 px-1.5 text-xs" value={newCase} placeholder="50m" aria-label="New case name" onChange={(e) => setNewCase(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && addCase()} />
            <Button variant="outline" size="sm" className="h-7 px-2 text-xs" disabled={!newCase.trim() || columns.some((c) => c.toLowerCase() === newCase.trim().toLowerCase())} onClick={addCase}>
              <Plus className="size-3.5" /> Add
            </Button>
          </Field>
        </ToolSection>
      )}

      {chosen && workflow && (
        <ToolSection
          title={`Steps (${steps.length})${ofCase}`}
          aside={
            <>
              <IconButton
                label={adding ? 'Stop adding from the tool panels' : 'Add steps from the tool panels: their button reads Add to workflow'}
                className={adding ? 'bg-primary/20 text-primary' : undefined}
                onClick={() => setAddingTo(adding ? null : { id: chosen.id, name: chosen.name })}
              >
                <Plus />
              </IconButton>
              <IconButton label="Add a pause at the end" onClick={() => edit((s) => [...s, { id: nextId(s), action: { kind: 'pause' }, note: 'Look, or work by hand, then Continue' }])}>
                <Pause />
              </IconButton>
            </>
          }
        >
          <label className="flex items-center gap-1.5 text-muted-foreground" title="After a run, show what it started from and what its last step made, and hide the layers only the steps in between made">
            <input
              type="checkbox"
              checked={tidy}
              aria-label="Tidy after a run"
              onChange={(e) => {
                setTidy(e.target.checked);
                try {
                  localStorage.setItem(TIDY_KEY, e.target.checked ? 'on' : 'off');
                } catch {
                  // Storage unavailable: the switch holds for this session.
                }
              }}
            />
            Tidy after a run: inputs and the result on show, the layers in between hidden
          </label>
          {only && (
            <div className="flex items-center gap-2 text-primary">
              Showing only step {steps.findIndex((s) => s.id === only.step) + 1}
              <button type="button" className="underline" onClick={() => void onlyStep(steps.find((s) => s.id === only.step)!)}>
                Show all again
              </button>
            </div>
          )}
          {adding && <div className="text-primary">Adding: each tool panel's button reads Add to workflow, and adds its command and settings as the last step.</div>}
          {steps.length === 0 && <div className="text-muted-foreground">No steps yet.</div>}
          <ol className="flex flex-col gap-1">
            {steps.map((step, at) => (
              <li key={step.id} className={step.on === false ? 'opacity-50' : undefined}>
                <div className="flex items-center gap-1">
                  <input type="checkbox" checked={step.on !== false} aria-label={`Step ${at + 1} on`} title="On: switched off, a step is passed over" onChange={(e) => edit((s) => s.map((t, k) => (k === at ? { ...t, on: e.target.checked ? undefined : false } : t)))} />
                  <span className="w-5 text-right tabular-nums text-muted-foreground">{at + 1}</span>
                  <button
                    type="button"
                    className="min-w-0 flex-1 truncate text-left font-medium hover:text-primary disabled:hover:text-foreground"
                    title={step.last?.made.length ? `Select and zoom to the ${plural(step.last.made.length, 'object')} it made` : step.action.kind === 'command' ? step.action.command : 'Pause'}
                    aria-label={`Show step ${at + 1}`}
                    disabled={!step.last?.made.length}
                    onClick={() => showStep(step)}
                  >
                    {step.action.kind === 'command' ? step.action.command : 'Pause'}
                  </button>
                  {changes.has(step.id) && (
                    <StatusBadge tone="edited" title={`${caseName} changes ${plural(changes.get(step.id)!, 'setting')} of this step`}>
                      {changes.get(step.id)} changed
                    </StatusBadge>
                  )}
                  <LastBadge step={step} />
                  {step.action.kind === 'command' && (
                    <IconButton
                      label={`Settings of step ${at + 1}`}
                      className={opened.has(step.id) ? 'bg-primary/20 text-primary' : undefined}
                      disabled={settingsOfStep(workflow, step.id).length === 0}
                      onClick={() => toggleSettings(step.id)}
                    >
                      <SlidersHorizontal />
                    </IconButton>
                  )}
                  {step.action.kind === 'command' && (
                    <IconButton
                      label={only?.step === step.id ? 'Show all again' : `Only step ${at + 1}: what it made on show, every other layer off`}
                      className={only?.step === step.id ? 'bg-primary/20 text-primary' : undefined}
                      disabled={!step.last?.made.length}
                      onClick={() => void onlyStep(step)}
                    >
                      <Eye />
                    </IconButton>
                  )}
                  <IconButton label={`Run from step ${at + 1}, keeping what the steps before made`} disabled={busy} onClick={() => void run(at + 1)}>
                    <Play />
                  </IconButton>
                  <IconButton label="Up" disabled={at === 0} onClick={() => move(at, -1)}>
                    <ArrowUp />
                  </IconButton>
                  <IconButton label="Down" disabled={at === steps.length - 1} onClick={() => move(at, 1)}>
                    <ArrowDown />
                  </IconButton>
                  <IconButton label="Take this step out" onClick={() => edit((s) => s.filter((_, k) => k !== at))}>
                    <Trash2 />
                  </IconButton>
                </div>
                <div className="ml-10 flex flex-col text-muted-foreground">
                  {step.action.kind === 'command' &&
                    Object.entries(step.action.inputs).map(([field, rule]) => (
                      <span key={field} className="truncate" title={ruleWords(workflow, rule)}>
                        {field}: {ruleWords(workflow, rule)}
                      </span>
                    ))}
                  {step.action.kind === 'command' && opened.has(step.id) && (
                    <StepSettings
                      rows={settingsOfStep(workflow, step.id)}
                      hidden={Object.keys(step.action.settings ?? {}).length - settingsOfStep(workflow, step.id).length}
                      caseName={caseName}
                      revision={revision}
                      valueOf={(row) => caseValue(workflow, caseName, row)}
                      set={(row, text) => setCell(caseName, row, text)}
                    />
                  )}
                  {editing === at ? (
                    <Input
                      autoFocus
                      className="h-6 px-1.5 text-xs"
                      defaultValue={step.note ?? ''}
                      aria-label={`Step ${at + 1} note`}
                      onBlur={(e) => {
                        setEditing(null);
                        if (e.target.value !== (step.note ?? '')) edit((s) => s.map((t, k) => (k === at ? { ...t, note: e.target.value || undefined } : t)));
                      }}
                      onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
                    />
                  ) : (
                    <button type="button" className="truncate text-left italic hover:text-foreground" onClick={() => setEditing(at)}>
                      {step.note || 'Add a note on why'}
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ol>
        </ToolSection>
      )}
    </ToolPanel>
  );
}

/**
 * A step's plain settings as the case picked has them, under the step: Base's are the step's own; a case's empty box
 * reads as Base, its value greyed in it, and what the case changes shows in blue. Emptied, a case's box goes back to Base.
 */
function StepSettings({ rows, hidden, caseName, revision, valueOf, set }: { rows: SettingRow[]; hidden: number; caseName: string; revision: number; valueOf: (row: SettingRow) => unknown; set: (row: SettingRow, text: string) => void }) {
  const inCase = caseName !== BASE;
  return (
    <div className="my-1 rounded border border-border/60 px-2 py-1">
      <div className="mb-0.5 text-[11px]">{inCase ? `As ${caseName}: an empty box reads as Base, what ${caseName} changes is blue` : 'Base: the step as it stands'}</div>
      <div className="grid grid-cols-[minmax(0,1fr)_6rem] items-center gap-x-2 gap-y-0.5">
        {rows.map((row) => {
          const own = valueOf(row);
          const shown = own === undefined ? '' : String(own);
          return (
            <label key={row.field} className="contents">
              <span className="truncate" title={`${row.command}: ${row.field}`}>
                {row.field}
              </span>
              <Input
                key={`${revision}:${caseName}:${shown}`}
                className={`h-6 px-1 text-xs ${inCase && own !== undefined ? 'text-edited' : ''}`}
                defaultValue={shown}
                placeholder={inCase ? String(row.base) : ''}
                aria-label={`${caseName}: step ${row.position} ${row.field}`}
                title={inCase && own === undefined ? 'As Base' : undefined}
                onBlur={(e) => e.target.value !== shown && set(row, e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
              />
            </label>
          );
        })}
      </div>
      {hidden > 0 && <div className="mt-0.5 text-[11px]">{plural(hidden, 'more setting')} (lists and tables) set in its tool</div>}
    </div>
  );
}

/** What a step's last run did, in a word and its colour: green ran, orange warned, red refused. */
function LastBadge({ step }: { step: WorkflowStep }) {
  const last = step.last;
  if (!last) return null;
  const made = `${last.made.length} made`;
  const title = `${made}${last.revised?.length ? `, ${last.revised.length} changed` : ''}${last.deleted ? `, ${last.deleted} deleted` : ''} in ${fmt(last.ms, 0)} ms`;
  if (last.refused) return <StatusBadge tone="refused" title="Refused the last time: the run log says why">refused</StatusBadge>;
  if (last.warnings) return <StatusBadge tone="warning" title={title}>{plural(last.warnings, 'warning')}</StatusBadge>;
  return <StatusBadge tone="ok" title={title}>{made}</StatusBadge>;
}
