import { ArrowLeft, ChevronLeft, ChevronRight } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { type ColumnSteps, type Dataflow, reservesColumn } from '@/bridge';
import { Choice, Field, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { type PickedLeaf, stepActivity, stepGrid, stepLabel, valueText } from './columnSteps';
import { unitLabel } from './reservesTable';

/**
 * Open its column (decision 0075, 5b): the block column a reserves table's node sits in, run again through its dataflow
 * with the settings of the run that made the table. The steps across the top, picked or stepped with ← and →; under them
 * the fields down the side and the units across, top of the stack to the bottom, as each stands after the step, a value
 * the step changed in blue with what it was. Units the step made are marked new; units it took away are listed.
 */

type Show = 'main' | 'all' | 'changed';
const SHOWS: readonly Show[] = ['main', 'all', 'changed'];
const SHOW_LABELS: Record<Show, string> = { main: 'Main fields', all: 'Every field', changed: 'Only what changed' };

export function ColumnStepsView({
  table,
  node,
  onBack,
  backLabel = 'Table',
  startStep,
  dataflow = null,
}: {
  table: { id: string; name: string; revision: number };
  node: string[];
  onBack: () => void;
  /** What going back goes to: the table, or the notebook (decision 0081, Whole column). */
  backLabel?: string;
  /** The step to open at, by its place; else the first step that did something. */
  startStep?: number;
  /** A dataflow as edited to run the column on, not the saved one (decision 0081, point 7). */
  dataflow?: Dataflow | null;
}) {
  const [column, setColumn] = useState<ColumnSteps | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [at, setAt] = useState(0);
  const [show, setShow] = useState<Show>('main');
  const grid = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let live = true;
    setColumn(null);
    setError(null);
    void reservesColumn(table.id, node, dataflow)
      .then((c) => {
        if (!live) return;
        setColumn(c);
        // The first step that did something to this column, so there's a change to read.
        const activity = stepActivity(c.steps);
        const first = activity.findIndex((a, i) => i > 0 && a.changes + a.made + a.gone > 0);
        setAt(startStep !== undefined && startStep < c.steps.length ? startStep : first > 0 ? first : 0);
        grid.current?.focus();
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [table.id, table.revision, node.join('\u0001')]);

  const activity = useMemo(() => (column ? stepActivity(column.steps) : []), [column]);
  // The step picked kept in sight along the strip, brought to its middle when it's out of sight.
  const strip = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const s = strip.current;
    const tab = s?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
    if (!s || !tab) return;
    if (tab.offsetLeft < s.scrollLeft || tab.offsetLeft + tab.offsetWidth > s.scrollLeft + s.clientWidth) s.scrollLeft = tab.offsetLeft - (s.clientWidth - tab.offsetWidth) / 2;
  }, [at, column]);
  const leaf: PickedLeaf | null = useMemo(() => {
    if (!column) return null;
    const below = node.slice(column.column.length);
    return below.length > 0 ? { seam: below[0], bench: below[1] } : null;
  }, [column, node]);
  const view = useMemo(() => (column ? stepGrid(column.fields, column.steps, at, show, leaf) : null), [column, at, show, leaf]);
  const volume = column?.fields.find((f) => f.kind === 'volume');
  const total = (i: number) => (column && volume ? column.steps[i].units.reduce((s, u) => s + (u.values[volume.name] ?? 0), 0) : 0);

  const step = (by: number) => column && setAt((a) => Math.min(Math.max(a + by, 0), column.steps.length - 1));
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'ArrowLeft') step(-1);
    else if (e.key === 'ArrowRight') step(1);
    else return;
    e.preventDefault();
  };

  return (
    <div className="flex min-w-0 flex-1 flex-col outline-none" ref={grid} tabIndex={0} onKeyDown={onKey} data-slot="column-steps">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1">
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs" title={`Back to the ${backLabel.toLowerCase()}`} onClick={onBack}>
          <ArrowLeft className="size-3.5" /> {backLabel}
        </Button>
        <span className="font-semibold text-primary" aria-label="Column shown">
          {column ? column.column.join(' › ') : node.join(' › ')}
        </span>
        {column && (
          <span className="text-muted-foreground">
            {column.case}, {column.dataflow}
          </span>
        )}
        <Field label="Fields">
          <Choice value={show} options={SHOWS} labels={SHOW_LABELS} onChange={setShow} label="Which fields: the main ones, every one, or only those this step changed" />
        </Field>
        {column && volume && (
          <span className="ml-auto text-muted-foreground tabular-nums" title="The column's volume after this step, and before it: material is moved, never lost, but the underburden comes from outside the model">
            {valueText(total(at), volume.unit)} {unitLabel(volume.unit)}
            {at > 0 && Math.abs(total(at) - total(at - 1)) > 1e-6 * Math.max(1, total(at - 1)) && <span className="text-edited"> (was {valueText(total(at - 1), volume.unit)})</span>}
          </span>
        )}
      </div>
      {error && <div className="px-2 py-1 text-destructive">{error}</div>}
      {column && column.changed.length > 0 && (
        <div className="px-2 py-1 text-warning" data-slot="column-changed">
          Run on what's there now: {column.changed.join(', ')} changed since this table was made, so the steps may not give the table's numbers. Run reserves again to bring them together.
        </div>
      )}
      {column && (
        <div className="flex shrink-0 items-center gap-1 border-b border-border px-1 py-1" role="tablist" aria-label="Steps">
          <button type="button" className="rounded p-0.5 text-muted-foreground hover:bg-accent disabled:opacity-40" disabled={at === 0} onClick={() => step(-1)} aria-label="The step before (←)" title="The step before (←)">
            <ChevronLeft className="size-3.5" />
          </button>
          <div className="relative flex min-w-0 flex-1 gap-1 overflow-x-auto" ref={strip}>
            {column.steps.map((s, i) => {
              const a = activity[i];
              const did = a.changes + a.made + a.gone;
              return (
                <button
                  key={`${i}:${s.step}`}
                  type="button"
                  role="tab"
                  aria-selected={i === at}
                  data-step={s.step}
                  className={cn('shrink-0 rounded px-1.5 py-0.5 whitespace-nowrap hover:bg-accent', i === at ? 'bg-selection font-semibold text-primary' : i > 0 && did === 0 && 'text-muted-foreground')}
                  title={i === 0 ? 'The column as its blocks give it, before any step' : did === 0 ? 'Changed nothing in this column' : `${a.changes} value${a.changes === 1 ? '' : 's'} changed, ${a.made} unit${a.made === 1 ? '' : 's'} made, ${a.gone} taken away`}
                  onClick={() => setAt(i)}
                >
                  {stepLabel(s.step)}
                  {i > 0 && did > 0 && <span className="ml-1 text-[10px] text-edited">{did}</span>}
                </button>
              );
            })}
          </div>
          <button type="button" className="rounded p-0.5 text-muted-foreground hover:bg-accent disabled:opacity-40" disabled={at === column.steps.length - 1} onClick={() => step(1)} aria-label="The step after (→)" title="The step after (→)">
            <ChevronRight className="size-3.5" />
          </button>
        </div>
      )}
      {column && view && (
        <div className="flex shrink-0 flex-wrap gap-x-3 px-2 py-0.5 empty:hidden">
          {column.steps[at].said.map((line) => (
            <span key={line} className="text-muted-foreground" data-slot="step-said">
              {line}
            </span>
          ))}
          {view.gone.length > 0 && (
            <span className="text-edited" data-slot="step-gone">
              Taken away: {view.gone.join(', ')}
            </span>
          )}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        {view && (
          <table className="border-separate border-spacing-0 tabular-nums" data-slot="column-grid">
            <thead className="sticky top-0 z-10 bg-card">
              <tr>
                <th className="sticky left-0 z-20 min-w-48 border-b border-border bg-card px-2 py-1 text-left font-medium text-muted-foreground">field</th>
                {view.units.map((u) => (
                  <th
                    key={u.unit.id}
                    data-unit={u.label}
                    className={cn('min-w-24 border-b border-border px-2 py-1 text-right align-bottom font-medium whitespace-nowrap', u.picked && 'text-primary')}
                    title={u.unit.joined.length > 0 ? `Benches ${u.unit.joined.map(([b, s]) => `${b} (${(s * 100).toFixed(1)} %)`).join(', ')}` : undefined}
                  >
                    <div className="flex items-center justify-end gap-1">
                      {u.isNew && <StatusBadge tone="edited" title="Made by this step">new</StatusBadge>}
                      {u.label}
                    </div>
                    <div className="ml-auto max-w-36 text-[10px] leading-tight font-normal whitespace-normal text-muted-foreground">{u.detail}</div>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {view.rows.map((r) => (
                <tr key={r.field.name} data-field={r.field.name} className="hover:bg-accent/40">
                  <td className="sticky left-0 bg-card py-px pr-2 pl-2">
                    {r.label} <span className="text-muted-foreground">{unitLabel(r.field.unit)}</span>
                  </td>
                  {r.cells.map((c, i) => (
                    <td
                      key={view.units[i].unit.id}
                      className={cn('px-2 py-px text-right whitespace-nowrap', c.changed && 'bg-edited/10 text-edited', view.units[i].picked && !c.changed && 'text-primary')}
                      title={c.changed ? `was ${valueText(c.before, r.field.unit) || 'none'}` : undefined}
                    >
                      {valueText(c.value, r.field.unit)}
                      {c.changed && <div className="text-[10px] text-muted-foreground line-through">{valueText(c.before, r.field.unit) || 'none'}</div>}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {!column && !error && <div className="p-3 text-muted-foreground">Running the column…</div>}
      </div>
    </div>
  );
}
