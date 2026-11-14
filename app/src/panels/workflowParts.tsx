/**
 * What the Workflow panel and the workflow canvas share (decisions 0065 to 0067): the workflow and case picked, read
 * again whenever the workflow is saved as a new revision; saving it; a step's settings as the case picked has them; and
 * a step's last result as a badge.
 */
import { useEffect, useMemo, useState } from 'react';
import type { ObjectSummary, Workflow, WorkflowStep } from '@/bridge';
import { objectDetails } from '@/bridge';
import { Input } from '@/design-system/ui/input';
import { StatusBadge } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { fmt } from './format';
import { BASE, type SettingRow } from './workflow';

export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

export interface PickedWorkflow {
  /** Every workflow in the project. */
  workflows: ObjectSummary[];
  /** The one picked, or the last made. */
  chosen: ObjectSummary | null;
  /** Its content, once read; the revision before shows until the new one has been read. */
  workflow: Workflow | null;
  revision: number;
  /** The case picked: Base unless one of its cases is. */
  caseName: string;
  pick: (id: string) => void;
  pickCase: (name: string) => void;
  /** Saves the workflow as its next revision; why it couldn't, or undefined. */
  save: (next: Workflow) => Promise<string | undefined>;
}

/** The workflow and case picked, shared between the Workflow panel and the canvas. */
export function usePickedWorkflow(): PickedWorkflow {
  const { summary, runRecord, workflowPick, setWorkflowPick } = useWorkspace();
  const workflows = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'workflow'), [summary]);
  const chosen = workflows.find((o) => o.id === workflowPick.id) ?? workflows[workflows.length - 1] ?? null;
  const [content, setContent] = useState<{ id: string; revision: number; workflow: Workflow } | null>(null);
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
  const workflow = content && content.id === id ? content.workflow : null;
  const cases = workflow?.cases ?? [];
  const caseName = workflowPick.case === BASE || cases.some((c) => c.name === workflowPick.case) ? workflowPick.case : BASE;
  return {
    workflows,
    chosen,
    workflow,
    revision,
    caseName,
    pick: (next) => setWorkflowPick({ id: next, case: BASE }),
    pickCase: (name) => setWorkflowPick({ id: chosen?.id ?? null, case: name }),
    save: async (next) => {
      if (!chosen) return 'No workflow picked';
      const outcome = await runRecord('workflow.save', { workflow: chosen.id, name: chosen.name, content: next });
      return outcome.error;
    },
  };
}

/**
 * A step's plain settings as the case picked has them: Base's are the step's own; a case's empty box reads as Base, its
 * value greyed in it, and what the case changes shows in blue. Emptied, a case's box goes back to Base.
 */
export function StepSettings({ rows, hidden, caseName, revision, valueOf, set }: { rows: SettingRow[]; hidden: number; caseName: string; revision: number; valueOf: (row: SettingRow) => unknown; set: (row: SettingRow, text: string) => void }) {
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
export function LastBadge({ step }: { step: WorkflowStep }) {
  const last = step.last;
  if (!last) return null;
  const made = `${last.made.length} made`;
  const title = `${made}${last.revised?.length ? `, ${last.revised.length} changed` : ''}${last.deleted ? `, ${last.deleted} deleted` : ''} in ${fmt(last.ms, 0)} ms`;
  if (last.refused) return <StatusBadge tone="refused" title="Refused the last time: the run log says why">refused</StatusBadge>;
  if (last.warnings) return <StatusBadge tone="warning" title={title}>{plural(last.warnings, 'warning')}</StatusBadge>;
  return <StatusBadge tone="ok" title={title}>{made}</StatusBadge>;
}
