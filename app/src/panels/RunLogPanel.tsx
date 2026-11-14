import { ListPlus, X } from 'lucide-react';
import { useMemo, useState } from 'react';
import type { LogEntry, RunRecord } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { columnHelper, DataTable } from '@/design-system/DataTable';
import { useWorkspace } from '@/workspace/context';
import { lookAt, NoteLine } from '@/design-system/kit';
import { clock, fmt } from './format';
import { tickRuns } from './workflow';

type Outcome = 'ok' | 'failed' | 'undone' | 'session';

interface LogRow {
  index: number;
  at: number;
  what: string;
  result: string;
  ms: number | null;
  outcome: Outcome;
  record: RunRecord | null;
}

const outcomeTone: Record<Outcome, string> = {
  ok: '',
  failed: 'text-destructive',
  undone: 'text-muted-foreground line-through',
  session: 'text-muted-foreground',
};

/** Rows newest first. A run reads as undone while the last undo or redo of it was an undo. */
function toRows(log: LogEntry[]): LogRow[] {
  const undone = new Set<string>();
  for (const e of log) {
    if (e.event === 'undo') undone.add(e.run);
    if (e.event === 'redo') undone.delete(e.run);
  }
  const commandOf = new Map<string, string>();
  const rows = log.map((e, i): LogRow => {
    const index = i + 1;
    switch (e.event) {
      case 'run': {
        commandOf.set(e.run, e.command);
        const failed = e.status !== 'ok';
        const outputs = e.outputs.slice(0, OUTPUTS_IN_A_ROW).map((o) => `${o.name} (${o.change})`);
        const more = e.outputs.length > OUTPUTS_IN_A_ROW ? ` and ${e.outputs.length - OUTPUTS_IN_A_ROW} more` : '';
        const look = lookAt(e.notes ?? []);
        const made = failed ? (e.status as { failed: string }).failed : outputs.join(', ') + more || (e.messages[0] ?? 'no change');
        const result = look && !failed ? `${made} · ${look}` : made;
        return { index, at: e.started_unix_ms, what: e.command, result, ms: e.duration_ms, outcome: failed ? 'failed' : undone.has(e.run) ? 'undone' : 'ok', record: e };
      }
      case 'undo':
      case 'redo':
        return { index, at: e.at_unix_ms, what: e.event === 'undo' ? 'Undo' : 'Redo', result: commandOf.get(e.run) ?? e.run, ms: null, outcome: 'session', record: null };
      case 'saved':
      case 'opened':
        return { index, at: e.at_unix_ms, what: e.event === 'saved' ? 'Saved' : 'Opened', result: e.path, ms: null, outcome: 'session', record: null };
      case 'workflow': {
        const ran = e.steps.filter((s) => s.outcome === 'ran').length;
        const ended = e.status === 'finished' ? 'finished' : `${e.status} at step ${e.at}`;
        const which = e.case && e.case !== 'Base' ? ` (${e.case})` : '';
        const result = `${e.name}${which}: ${ran} step${ran === 1 ? '' : 's'} run from step ${e.from}, ${ended}`;
        return { index, at: e.started_unix_ms, what: 'Workflow', result, ms: e.duration_ms, outcome: e.status === 'refused' ? 'failed' : undone.has(e.run) ? 'undone' : 'ok', record: null };
      }
    }
  });
  return rows.reverse();
}

/** A run's outputs named in its row, and in its details: a run can make tens of thousands. */
const OUTPUTS_IN_A_ROW = 4;
const OUTPUTS_IN_DETAILS = 200;

const helper = columnHelper<LogRow>();
const logColumns = [
  helper.accessor('index', { header: '#', cell: (c) => <span className="text-muted-foreground tabular-nums">{c.getValue()}</span> }),
  helper.accessor('at', { header: 'Time', cell: (c) => <span className="tabular-nums">{clock(c.getValue())}</span> }),
  helper.accessor('what', { header: 'Command', cell: (c) => <span className={outcomeTone[c.row.original.outcome]}>{c.getValue()}</span> }),
  helper.accessor('result', {
    header: 'Result',
    cell: (c) => (
      <span className={`block max-w-[40rem] truncate ${c.row.original.outcome === 'failed' ? 'text-destructive' : ''}`} title={c.getValue()}>
        {c.getValue()}
      </span>
    ),
  }),
  helper.accessor('ms', {
    header: 'ms',
    cell: (c) => {
      const ms = c.getValue();
      return <span className="tabular-nums">{ms === null ? '' : fmt(ms, ms < 10 ? 1 : 0)}</span>;
    },
  }),
];

/** A run that can be captured into a workflow: applied, not refused or undone (decision 0065). */
const capturable = (r: LogRow) => r.record !== null && r.outcome === 'ok';

function RunDetails({ record }: { record: RunRecord }) {
  const { summary, select } = useWorkspace();
  const live = new Set(summary?.objects.map((o) => o.id));
  return (
    <div className="space-y-2 px-3 py-2 text-xs">
      <div className="text-muted-foreground">
        Run {record.run} · {record.command} · {fmt(record.duration_ms, 1)} ms
        {record.layers_created.length > 0 && ` · new layers: ${record.layers_created.join(', ')}`}
      </div>
      {record.status !== 'ok' && <p className="break-words whitespace-pre-wrap text-destructive">{record.status.failed}</p>}
      {record.outputs.length > 0 && (
        <div className="flex flex-wrap gap-x-3">
          {record.outputs.slice(0, OUTPUTS_IN_DETAILS).map((o) =>
            live.has(o.id) ? (
              <button key={`${o.id}-${o.revision}`} className="text-primary hover:underline" onClick={() => select(o.id)}>
                {o.name} r{o.revision} ({o.change})
              </button>
            ) : (
              <span key={`${o.id}-${o.revision}`} className="text-muted-foreground">
                {o.name} r{o.revision} ({o.change})
              </span>
            ),
          )}
          {record.outputs.length > OUTPUTS_IN_DETAILS && <span className="text-muted-foreground">and {record.outputs.length - OUTPUTS_IN_DETAILS} more</span>}
        </div>
      )}
      <pre className="max-h-40 overflow-auto rounded bg-muted px-2 py-1 font-mono text-[11px]">{JSON.stringify(record.params, null, 2)}</pre>
      {record.notes.length > 0 && (
        <div className="flex flex-col gap-0.5">
          {record.notes.map((note, i) => (
            <NoteLine key={i} note={note} index={i} />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Every run, undo, redo, save and open this session; click a run for its parameters and messages. Tick runs (Shift-click
 * ticks every run between) and Capture them as a workflow, each run a step (decision 0065).
 */
export function RunLogPanel() {
  const { log, captureWorkflow } = useWorkspace();
  const rows = useMemo(() => toRows(log), [log]);
  const [picked, setPicked] = useState<number | null>(null);
  const record = rows.find((r) => r.index === picked)?.record ?? null;
  const [ticked, setTicked] = useState<Set<number>>(new Set());
  const [lastTicked, setLastTicked] = useState<number | null>(null);
  const [name, setName] = useState('Workflow');
  const [capturing, setCapturing] = useState(false);
  const tickable = useMemo(() => new Set(rows.filter(capturable).map((r) => r.index)), [rows]);
  const live = [...ticked].filter((i) => tickable.has(i));
  const tick = (index: number, range: boolean) => {
    setTicked((was) => tickRuns(was, tickable, index, lastTicked, range));
    setLastTicked(index);
  };
  const columns = useMemo(
    () =>
      helper.columns([
        helper.display({
          id: 'capture',
          header: () => <span title="Tick runs to capture them as a workflow; Shift-click ticks every run between">✓</span>,
          cell: (c) =>
            capturable(c.row.original) ? (
              <input
                type="checkbox"
                checked={ticked.has(c.row.original.index)}
                aria-label={`Capture run ${c.row.original.index}`}
                onClick={(e) => {
                  e.stopPropagation();
                  tick(c.row.original.index, e.shiftKey);
                }}
                onChange={() => undefined}
              />
            ) : null,
        }),
        ...logColumns,
      ]),
    [ticked, tickable, lastTicked],
  );
  const capture = async () => {
    const runs = rows
      .filter((r) => ticked.has(r.index) && capturable(r))
      .sort((a, b) => a.index - b.index)
      .map((r) => r.record!.run);
    setCapturing(true);
    const outcome = await captureWorkflow(name.trim() || 'Workflow', runs);
    setCapturing(false);
    if (outcome.record) setTicked(new Set());
  };
  // The panel is usually wide and short, so details sit beside the table.
  return (
    <div className="flex h-full">
      <div className="flex min-w-0 flex-1 flex-col">
        {live.length > 0 && (
          <div className="flex shrink-0 items-center gap-2 border-b px-2 py-1 text-xs">
            <span className="text-muted-foreground">{live.length} run{live.length === 1 ? '' : 's'} ticked</span>
            <Input className="h-6 w-48 px-1.5 text-xs" value={name} aria-label="Workflow name" onChange={(e) => setName(e.target.value)} />
            <Button size="sm" className="h-6 px-2 text-xs" disabled={capturing} title="Each run a step, in log order, its objects found by rule; opens the Workflow panel" onClick={() => void capture()}>
              <ListPlus className="size-3.5" /> Capture as a workflow
            </Button>
            <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => setTicked(new Set())}>
              <X className="size-3.5" /> Clear
            </Button>
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-auto">
          <DataTable
            data={rows}
            columns={columns}
            rowId={(r) => String(r.index)}
            selected={picked === null ? null : String(picked)}
            onRowClick={(r) => setPicked(r.index === picked ? null : r.index)}
            empty="No runs yet."
          />
        </div>
      </div>
      {record && (
        <div className="w-[38%] max-w-[36rem] shrink-0 overflow-auto border-l">
          <RunDetails record={record} />
        </div>
      )}
    </div>
  );
}
