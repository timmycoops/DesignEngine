import { ListPlus } from 'lucide-react';
import type { ReactNode } from 'react';
import { cn } from 'cn';
import type { Note } from '../../bridge';
import { useWorkspace } from '../../workspace/context';
import { NoteLine } from './NoteLine';
import { useRunning } from './RunGoing';
import { waitingOn } from '../../running';

/**
 * The frame of every tool panel (UI plan, one way of working, item 2): what it does in a line at the top, its sections, and
 * at the foot what the last run did above the one button named for what it does. Ctrl+Enter anywhere in the panel presses
 * that button. While the Workflow panel is adding steps, a panel whose action says what it runs reads Add to workflow
 * instead, and adds its command and settings as the workflow's last step, running nothing (decision 0065).
 */
export function ToolPanel({ title, summary, children, result, action: own, panel }: { title: string; summary: ReactNode; children: ReactNode; result?: RunReport; action?: ToolAction; panel?: string }) {
  const { addingTo, addStep } = useWorkspace();
  const step = own?.step;
  const action: ToolAction | undefined =
    addingTo && own && step
      ? {
          label: `Add to ${addingTo.name}`,
          icon: <ListPlus className="size-3.5" />,
          disabled: own.disabled,
          title: `Adds ${step.command} with these settings as the last step of ${addingTo.name}, its objects found by rule; nothing runs`,
          hint: 'Adds a step; nothing runs · Ctrl+Enter',
          run: () => addStep(step.command, step.params),
        }
      : own;
  // Another run going: this one waits, saying on what (decision 0060).
  const going = useRunning();
  const waiting = action && !action.busy ? waitingOn(going) : null;
  return (
    <div
      className="flex h-full flex-col text-xs"
      data-tool-panel={panel ?? title}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && action && !action.disabled && !action.busy && !waiting) {
          e.preventDefault();
          void action.run();
        }
      }}
    >
      <div className="shrink-0 border-b px-3 py-2">
        <div className="text-sm font-medium">{title}</div>
        <div className="text-muted-foreground">{summary}</div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">{children}</div>
      {(result || action) && (
        <div className="flex shrink-0 flex-col gap-2 border-t px-3 py-2">
          {result && <RunResult report={result} />}
          {action && (
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-3 font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
                disabled={action.disabled || action.busy || waiting !== null}
                title={waiting ?? action.title}
                onClick={() => void action.run()}
              >
                {action.icon}
                {action.busy ? action.busyLabel ?? 'Working…' : action.label}
              </button>
              <span className="text-muted-foreground">{waiting ?? action.hint ?? 'One undo · Ctrl+Enter'}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** The one button at a tool panel's foot. */
export interface ToolAction {
  /** A verb and what it acts on: "Build wall", "Cut 212 solids". */
  label: string;
  run: () => unknown;
  icon?: ReactNode;
  title?: string;
  disabled?: boolean;
  busy?: boolean;
  busyLabel?: string;
  hint?: string;
  /** What it runs, so it can be added to a workflow as a step instead (decision 0065). */
  step?: { command: string; params: Record<string, unknown> };
}

/** A titled part of a tool panel: Inputs, Settings, Output, or one of its own. */
export function ToolSection({ title, children, aside, className }: { title?: ReactNode; children: ReactNode; aside?: ReactNode; className?: string }) {
  return (
    <section className={cn('flex flex-col gap-1.5 px-3 py-2', className)}>
      {(title || aside) && (
        <div className="flex items-center gap-2">
          {title && <h3 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{title}</h3>}
          {aside && <span className="ml-auto flex items-center gap-1">{aside}</span>}
        </div>
      )}
      {children}
    </section>
  );
}

/** What a run did, as the panel shows it: its notes, or why it was refused. */
export interface RunReport {
  notes: Note[];
  error?: string;
}

/**
 * The notes a run reported (decision 0059). The first says what it made; the rest are muted, but for warnings, which show
 * in the warning colour, and what blocks later use, in red, each with Show where it has a place. A refusal shows in red.
 */
export function RunResult({ report }: { report: RunReport }) {
  if (!report.error && report.notes.length === 0) return null;
  return (
    <div className={cn('flex max-h-48 flex-col gap-0.5 overflow-auto rounded-md border px-2 py-1.5', report.error ? 'border-destructive/40' : 'border-ok/30')} role="status">
      {report.error && <div className="text-destructive">{report.error}</div>}
      {report.notes.map((note, k) => (
        <NoteLine key={k} note={note} index={k} />
      ))}
    </div>
  );
}
