import { ArrowDown, ArrowUp, Pause, Play, Plus, SkipForward, Trash2 } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Input } from '@/design-system/ui/input';
import type { LogEntry, Note, ObjectSummary, Workflow, WorkflowRecord, WorkflowStep } from '@/bridge';
import { objectDetails } from '@/bridge';
import { Field, IconButton, plainNote, type RunReport, Select, StatusBadge, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { fmt } from './format';
import { ruleWords } from './workflow';

/**
 * Workflow (decision 0065): saved steps run again in one go. A workflow is captured from runs in the Run log, or built
 * by adding steps from the tool panels in Add to workflow mode; each step is a command with its settings and a rule for
 * each input, or a pause. Run runs every step in order, Run from here keeps what the steps before made, a pause stops
 * the run for hand work, and the first refusal stops it with that step's notes. A run is one undo. Made from the kit.
 */

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
  const { summary, log, runRecord, runWorkflow, addingTo, setAddingTo } = useWorkspace();
  const workflows = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'workflow'), [summary]);
  const [picked, setPicked] = useState<string | null>(null);
  const chosen: ObjectSummary | null = workflows.find((o) => o.id === picked) ?? workflows[workflows.length - 1] ?? null;
  const [content, setContent] = useState<{ id: string; revision: number; workflow: Workflow } | null>(null);
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<number | null>(null);

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

  // The last run of this workflow, from the log: where Continue goes after a pause.
  const last = useMemo(() => {
    const runs = log.filter((e): e is Extract<LogEntry, { event: 'workflow' }> => e.event === 'workflow' && e.workflow === chosen?.id);
    return runs[runs.length - 1] ?? null;
  }, [log, chosen]);
  const continueFrom = last?.status === 'paused' && workflow && last.at < workflow.steps.length ? last.at + 1 : null;

  const save = async (next: Workflow) => {
    if (!chosen) return;
    const outcome = await runRecord('workflow.save', { workflow: chosen.id, name: chosen.name, content: next });
    if (outcome.error) setReport({ notes: [], error: outcome.error });
  };
  const edit = (change: (steps: WorkflowStep[]) => WorkflowStep[]) => workflow && void save({ steps: change(workflow.steps.map((s) => ({ ...s }))) });
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
    const outcome = await runWorkflow(chosen.id, from);
    setBusy(false);
    setReport(outcome.record ? reportOf(outcome.record) : { notes: [], error: outcome.error });
  };

  const steps = workflow?.steps ?? [];
  const action: ToolAction | undefined = chosen
    ? continueFrom
      ? { label: `Continue from step ${continueFrom}`, icon: <SkipForward className="size-3.5" />, disabled: !workflow, busy, busyLabel: 'Running…', title: 'Runs on from the step after the pause', run: () => run(continueFrom) }
      : {
          label: `Run ${plural(steps.filter((s) => s.on !== false && s.action.kind === 'command').length, 'step')}`,
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
          title={`Steps (${steps.length})`}
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
          {adding && <div className="text-primary">Adding: each tool panel's button reads Add to workflow, and adds its command and settings as the last step.</div>}
          {steps.length === 0 && <div className="text-muted-foreground">No steps yet.</div>}
          <ol className="flex flex-col gap-1">
            {steps.map((step, at) => (
              <li key={step.id} className={step.on === false ? 'opacity-50' : undefined}>
                <div className="flex items-center gap-1">
                  <input type="checkbox" checked={step.on !== false} aria-label={`Step ${at + 1} on`} title="On: switched off, a step is passed over" onChange={(e) => edit((s) => s.map((t, k) => (k === at ? { ...t, on: e.target.checked ? undefined : false } : t)))} />
                  <span className="w-5 text-right tabular-nums text-muted-foreground">{at + 1}</span>
                  <span className="min-w-0 flex-1 truncate font-medium" title={step.action.kind === 'command' ? step.action.command : 'Pause'}>
                    {step.action.kind === 'command' ? step.action.command : 'Pause'}
                  </span>
                  <LastBadge step={step} />
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
