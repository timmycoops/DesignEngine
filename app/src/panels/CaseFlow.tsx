import type { FlowStep } from '@/bridge';
import { cn } from 'cn';
import { type FlowStage, flowStages } from './flowStages';
import { stepLabel } from './columnSteps';
import type { Stage } from './reservesCase';

/**
 * The flow across the Reserves case tab (Reserves case plan, decision 10, workstream 6): the case carried through the
 * dataflow a stage a column. The coal lane in tonnes and the waste lane in BCM, each as wide as it is against the most it
 * holds; coal a stage takes leaves red, rock it lets in amber. A click on a stage or a step goes there; the stage open is
 * lit; a figure the edits moved is blue, with what it was.
 */

const tonnes = (v: number) => `${Math.round(v).toLocaleString('en-AU')} t`;
const bcm = (v: number) => `${Math.round(v).toLocaleString('en-AU')} BCM`;
const coalOf = (s: FlowStage) => s.coalTonnes ?? s.coal;
const coalWords = (s: FlowStage, v: number) => (s.coalTonnes === null ? `${Math.round(v).toLocaleString('en-AU')} m³` : tonnes(v));
const moved = (a: number, b: number) => Math.abs(a - b) > 1e-6 * Math.max(1, Math.abs(b));

export function CaseFlow({ flow, saved, stages, names, at, onStep }: { flow: FlowStep[]; saved: FlowStep[] | null; stages: Stage[]; names: string[]; at: number | null; onStep: (step: number) => void }) {
  const shown = flowStages(flow, stages);
  if (shown.length === 0) return null;
  const was = saved ? new Map(flowStages(saved, stages).map((s) => [s.name + s.steps[0], s])) : null;
  const coalMost = Math.max(...shown.map(coalOf), 1);
  const wasteMost = Math.max(...shown.map((s) => s.waste), 1);
  return (
    <div className="shrink-0 border-b border-border px-2 py-1.5" data-slot="case-flow">
      <div className="mb-1 text-muted-foreground">The flow · coal in tonnes and waste in BCM through each stage · red: coal a stage takes · amber: rock let into ROM · click a stage or step to go there</div>
      <div className="flex gap-1 overflow-x-auto pb-1">
        {shown.map((s) => {
          const open = at !== null && s.steps.includes(at);
          const before = was?.get(s.name + s.steps[0]);
          const coalMoved = before ? moved(coalOf(s), coalOf(before)) : false;
          const lostMoved = before ? moved(s.lostTonnes ?? s.lost, before.lostTonnes ?? before.lost) : false;
          return (
            <div
              key={s.name + s.steps[0]}
              className={cn('w-36 shrink-0 rounded border px-1.5 py-1', open ? 'border-primary bg-selection' : 'border-border')}
              data-slot="flow-stage"
              data-stage={s.name}
              data-coal={Math.round(coalOf(s))}
              data-lost={Math.round(s.lostTonnes ?? s.lost)}
              data-let-in={Math.round(s.letIn)}
              data-unit={s.coalTonnes === null ? 'm3' : 't'}
              data-coal-m3={s.coal}
              data-lost-m3={s.lost}
            >
              <button type="button" className={cn('block w-full truncate text-left font-semibold hover:underline', open && 'text-primary')} title={`Go to ${s.name}`} onClick={() => onStep(s.steps.find((n) => n > 0) ?? 0)}>
                {s.name}
              </button>
              <div className="mb-1 flex flex-wrap gap-0.5">
                {s.steps.map((n) => (
                  <button
                    key={n}
                    type="button"
                    data-slot="flow-step"
                    data-step={n}
                    title={n === 0 ? 'The blocks as read' : `${n} · ${stepLabel(names[n] ?? '')}`}
                    className={cn('h-4 min-w-4 rounded px-0.5 text-[10px] tabular-nums', n === at ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground hover:bg-accent')}
                    onClick={() => onStep(n)}
                  >
                    {n}
                  </button>
                ))}
              </div>
              <Lane look="bg-coal" share={coalOf(s) / coalMost} label={coalWords(s, coalOf(s))} edited={coalMoved} was={before ? `was ${coalWords(before, coalOf(before))}` : undefined} title="Coal still in play after this stage" />
              <div className={cn('h-4 truncate text-[11px]', s.lost > 1e-9 ? (lostMoved ? 'text-edited' : 'text-destructive') : 'text-transparent')} title={s.lostBy.map((m) => `${m.what}: ${m.tonnes === null ? `${Math.round(m.volume).toLocaleString('en-AU')} m³` : tonnes(m.tonnes)}`).join('\n')} data-slot="flow-lost">
                {s.lost > 1e-9 ? `− ${s.lostTonnes === null ? `${Math.round(s.lost).toLocaleString('en-AU')} m³` : tonnes(s.lostTonnes)} ${s.lostBy[0] ? s.lostBy[0].what.split(' ')[0] : 'taken'}${s.lostBy.length > 1 ? ' …' : ''}` : '·'}
              </div>
              <Lane look="bg-waste" share={s.waste / wasteMost} label={bcm(s.waste)} edited={false} title="Waste still in play after this stage" />
              <div className={cn('h-4 truncate text-[11px]', s.letIn > 1e-9 ? 'text-warning' : 'text-transparent')} title={s.letInBy.map((m) => `${m.what}: ${bcm(m.volume)}`).join('\n')} data-slot="flow-let-in">
                {s.letIn > 1e-9 ? `+ ${bcm(s.letIn)} rock into ROM` : '·'}
              </div>
              {s.stopped > 0 && <div className="text-[11px] text-destructive">{s.stopped} blocks stopped</div>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** One lane in a stage: a bar as wide as its share, its figure under it. */
function Lane({ look, share, label, edited, was, title }: { look: string; share: number; label: string; edited: boolean; was?: string; title: string }) {
  return (
    <div title={edited && was ? `${title}: ${label}, ${was}` : `${title}: ${label}`}>
      <div className="h-2 w-full rounded bg-muted">
        <div className={cn('h-2 rounded', look)} style={{ width: `${Math.max(2, Math.min(100, share * 100))}%` }} />
      </div>
      <div className={cn('truncate text-[11px] tabular-nums', edited && 'text-edited')}>{label}</div>
    </div>
  );
}
