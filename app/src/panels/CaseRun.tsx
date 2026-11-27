import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { type CaseState, caseState, type DataflowState, reservesRerunParams } from '@/bridge';
import { Select, StatusBadge } from '@/design-system/kit';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { caseOpen, onItem, setCase } from '@/workspace/tablesSide';
import { useCases } from './CasesBar';
import { CaseSwitcher } from './CaseSwitcher';
import { toRun } from './flowStages';

/** What a run said: the dataflows run, or the first refusal. */
export type Said = { text: string; refused: boolean };

/**
 * Runs a case's dataflows (Reserves case plan, decision 4): one alone, those out of date in run order (each judged after
 * the one before it ran), or all. Each run is its own undo; the first refused stops the rest.
 */
export async function runCaseDataflows(
  table: { id: string; name: string },
  state: CaseState,
  what: 'stale' | 'all' | DataflowState,
  runRecord: (command: string, params: Record<string, unknown>) => Promise<{ error?: string }>,
): Promise<Said> {
  const ran: string[] = [];
  const runOne = async (d: DataflowState) =>
    d.reserves ? runRecord('reserves.run', await reservesRerunParams(table.id, [])) : runRecord('table.run_dataflow', { table: table.id, dataflow: d.dataflow });
  const order = typeof what === 'string' ? state.dataflows : [what];
  for (const listed of order) {
    let d = listed;
    if (what === 'stale') {
      const now = await caseState(table.id);
      const it = now.dataflows.find((x) => x.dataflow === listed.dataflow);
      if (!it || it.up_to_date) continue;
      d = it;
    }
    const reply = await runOne(d);
    if (reply.error) return { text: `${d.name}: ${reply.error}`, refused: true };
    ran.push(d.name);
  }
  return { text: ran.length ? `Ran ${ran.join(', then ')} on ${table.name}: each its own undo.` : 'Nothing was out of date.', refused: false };
}

/** The case the top row runs, followed as the Tables side's list opens a case or a result. */
export function useCaseOpen(): string | null {
  return useSyncExternalStore(onItem, caseOpen);
}

/**
 * Case and Run on the Tables side's top row (decision 0108, workstream 4), the one place on the side to run: the case,
 * picked here or by opening it in the list; Run, its dataflows one at a time, what's out of date or all; and whether
 * it's up to date, with why when it isn't.
 */
export function CaseRun() {
  const { summary, runRecord } = useWorkspace();
  const cases = useCases();
  const picked = useCaseOpen();
  const table = cases.heads.find((h) => h.table === picked) ?? cases.base ?? cases.heads[0] ?? null;
  const changed = useMemo(() => (summary?.objects ?? []).map((o) => `${o.id}:${o.revision}`).join(','), [summary]);
  const [state, setState] = useState<CaseState | null>(null);
  const [running, setRunning] = useState(false);
  const [said, setSaid] = useState<Said | null>(null);

  // Whether each of the case's dataflows is up to date, read again whenever anything moves.
  useEffect(() => {
    if (!table) return setState(null);
    let current = true;
    void caseState(table.table)
      .then((s) => current && setState(s))
      .catch(() => current && setState(null));
    return () => {
      current = false;
    };
  }, [table?.table, changed]);
  useEffect(() => setSaid(null), [table?.table]);

  if (!table)
    return (
      <span className="ml-3 text-xs text-muted-foreground" data-slot="case-run" data-case="">
        No case yet: Cases, then + New case.
      </span>
    );
  const run = async (what: 'stale' | 'all' | DataflowState) => {
    if (!state) return;
    setRunning(true);
    setSaid(null);
    try {
      setSaid(await runCaseDataflows({ id: table.table, name: table.name }, state, what, runRecord));
    } catch (e) {
      setSaid({ text: String(e), refused: true });
    } finally {
      setRunning(false);
    }
  };
  const stale = toRun(state?.dataflows ?? [], false);
  return (
    <div className="ml-3 flex min-w-0 items-center gap-2 text-xs" data-slot="case-run" data-case={table.table}>
      <span className="text-muted-foreground">Case</span>
      <Select value={table.table} aria-label="Case" data-slot="case-pick" className="h-6 w-40" onChange={(e) => setCase(e.target.value)}>
        {cases.heads.map((h) => (
          <option key={h.table} value={h.table}>
            {h.name}
            {h.base ? ' (Base)' : ''}
          </option>
        ))}
      </Select>
      <CaseSwitcher state={state} open={null} running={running} onRun={(w) => void run(w)} />
      {state &&
        (stale.length === 0 ? (
          <StatusBadge tone="ok" title="Every dataflow of the case has run on what it reads as it stands">
            ✓ Up to date
          </StatusBadge>
        ) : (
          <StatusBadge tone="warning" title={stale.map((d) => `${d.name}: ${d.why.join('; ')}`).join('\n')}>
            Out of date
          </StatusBadge>
        ))}
      {running && <span className="text-muted-foreground">Running…</span>}
      {said && (
        <span className={cn('min-w-0 truncate', said.refused ? 'text-destructive' : 'text-ok')} title={said.text} data-slot="case-run-said">
          {said.text}
        </span>
      )}
    </div>
  );
}
