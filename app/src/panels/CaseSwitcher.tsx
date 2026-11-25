import { ChevronDown, Play } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { CaseState, DataflowState } from '@/bridge';
import { MENU_SURFACE, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { rangeWords, toRun } from './flowStages';

/**
 * Open another dataflow (Reserves case plan, decisions 4 and 6, workstream 6): from the band, the case's dataflows in run
 * order, each with its range, the settings tables it reads, whether it's up to date and why not, Open and Run; then Run
 * what's out of date and Run all, in order, each run its own undo. Nothing runs by itself.
 */
export function CaseSwitcher({ state, open, running, onOpen, onRun }: { state: CaseState | null; open: string | null; running: boolean; onOpen: (d: DataflowState) => void; onRun: (what: 'stale' | 'all' | DataflowState) => void }) {
  const [shown, setShown] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!shown) return;
    const away = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setShown(false);
    document.addEventListener('mousedown', away);
    return () => document.removeEventListener('mousedown', away);
  }, [shown]);
  const dataflows = state?.dataflows ?? [];
  const stale = toRun(dataflows, false);
  const at = dataflows.findIndex((d) => d.dataflow === open);
  return (
    <div className="relative" ref={box} onKeyDown={(e) => e.key === 'Escape' && shown && (e.stopPropagation(), setShown(false))}>
      <Button size="sm" variant="outline" className="h-6 gap-1 px-2" onClick={() => setShown((s) => !s)} data-slot="case-switch" title="The case's dataflows in run order: open another, run one, what's out of date, or all">
        {at >= 0 && dataflows.length > 0 ? `${at + 1} of ${dataflows.length}` : 'Dataflows'}
        {stale.length > 0 && <span className="text-warning">· {stale.length} out of date</span>}
        <ChevronDown className="size-3" />
      </Button>
      {shown && (
        <div className={cn(MENU_SURFACE, 'absolute top-7 left-0 z-30 w-[34rem] max-w-[90vw]')} data-slot="case-dataflows">
          <div className="px-1.5 py-1 text-muted-foreground">{state?.name ?? 'The case'}'s dataflows, run in this order; each writes its own columns.</div>
          {dataflows.map((d, i) => (
            <div key={d.dataflow} className={cn('grid grid-cols-[1.25rem_1fr_auto] items-start gap-x-2 rounded px-1.5 py-1', d.dataflow === open && 'bg-selection')} data-slot="case-dataflow-row" data-dataflow={d.name} data-up-to-date={d.up_to_date ? 'yes' : 'no'}>
              <span className="pt-0.5 text-muted-foreground tabular-nums">{i + 1}</span>
              <div className="min-w-0">
                <div className="flex flex-wrap items-baseline gap-x-2">
                  <span className="font-semibold">{d.name}</span>
                  <span className="text-[10px] text-muted-foreground uppercase">{d.reserves ? 'Reserves · makes the rows' : 'Rows'}</span>
                </div>
                <div className="text-muted-foreground">
                  On {rangeWords(d)} · reads {d.reserves ? 'the blocks, the seam order' : 'the columns above it'}
                  {d.tables.length > 0 ? `, ${d.tables.join(', ')}` : ''}
                </div>
                {d.up_to_date ? (
                  <StatusBadge tone="ok" title={d.ran ? `Ran ${d.ran}` : undefined}>
                    ✓ Up to date{d.ran ? ` · ran ${d.ran}` : ''}
                  </StatusBadge>
                ) : (
                  <div className="text-warning" data-slot="case-why">
                    Out of date: {d.why.join('; ')}
                  </div>
                )}
              </div>
              <div className="flex gap-1">
                <Button size="sm" variant="outline" className="h-6 px-2" disabled={d.dataflow === open} onClick={() => (onOpen(d), setShown(false))} data-slot="case-open">
                  Open
                </Button>
                <Button size="sm" variant="outline" className="h-6 px-2" disabled={running} onClick={() => onRun(d)} title={`Run ${d.name} alone: those below it aren't run`} data-slot="case-run-one">
                  <Play className="size-3" /> Run
                </Button>
              </div>
            </div>
          ))}
          <div className="mt-1 flex flex-wrap items-center gap-2 border-t border-border px-1.5 pt-1.5">
            <Button size="sm" className="h-6 px-2" disabled={running || stale.length === 0} onClick={() => onRun('stale')} data-slot="case-run-stale">
              Run what's out of date{stale.length ? ` (${stale.length})` : ''}
            </Button>
            <Button size="sm" variant="outline" className="h-6 px-2" disabled={running || dataflows.length === 0} onClick={() => onRun('all')} data-slot="case-run-all">
              Run all, in order
            </Button>
            <span className="text-muted-foreground">{running ? 'Running…' : 'Each run is the table’s next revision, one undo.'}</span>
          </div>
        </div>
      )}
    </div>
  );
}
