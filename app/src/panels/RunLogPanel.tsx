import { useMemo, useState } from 'react';
import type { LogEntry, RunRecord } from '@/bridge';
import { columnHelper, DataTable } from '@/design-system/DataTable';
import { useWorkspace } from '@/workspace/context';
import { clock, fmt } from './format';

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
        const result = failed ? (e.status as { failed: string }).failed : outputs.join(', ') + more || 'no change';
        return { index, at: e.started_unix_ms, what: e.command, result, ms: e.duration_ms, outcome: failed ? 'failed' : undone.has(e.run) ? 'undone' : 'ok', record: e };
      }
      case 'undo':
      case 'redo':
        return { index, at: e.at_unix_ms, what: e.event === 'undo' ? 'Undo' : 'Redo', result: commandOf.get(e.run) ?? e.run, ms: null, outcome: 'session', record: null };
      case 'saved':
      case 'opened':
        return { index, at: e.at_unix_ms, what: e.event === 'saved' ? 'Saved' : 'Opened', result: e.path, ms: null, outcome: 'session', record: null };
    }
  });
  return rows.reverse();
}

/** A run's outputs named in its row, and in its details: a run can make tens of thousands. */
const OUTPUTS_IN_A_ROW = 4;
const OUTPUTS_IN_DETAILS = 200;

const helper = columnHelper<LogRow>();
const columns = helper.columns([
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
]);

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
      {record.messages.length > 0 && (
        <ul className="list-disc space-y-0.5 pl-4">
          {record.messages.map((m, i) => (
            <li key={i}>{m}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Every run, undo, redo, save and open this session; click a run for its parameters and messages. */
export function RunLogPanel() {
  const { log } = useWorkspace();
  const rows = useMemo(() => toRows(log), [log]);
  const [picked, setPicked] = useState<number | null>(null);
  const record = rows.find((r) => r.index === picked)?.record ?? null;
  // The panel is usually wide and short, so details sit beside the table.
  return (
    <div className="flex h-full">
      <div className="min-w-0 flex-1 overflow-auto">
        <DataTable
          data={rows}
          columns={columns}
          rowId={(r) => String(r.index)}
          selected={picked === null ? null : String(picked)}
          onRowClick={(r) => setPicked(r.index === picked ? null : r.index)}
          empty="No runs yet."
        />
      </div>
      {record && (
        <div className="w-[38%] max-w-[36rem] shrink-0 overflow-auto border-l">
          <RunDetails record={record} />
        </div>
      )}
    </div>
  );
}
