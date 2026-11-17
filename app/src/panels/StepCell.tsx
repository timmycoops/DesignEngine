import { Braces, ChevronDown, ChevronRight, Columns3 } from 'lucide-react';
import type { ReactNode } from 'react';
import type { ColumnSteps, FormulaTrace } from '@/bridge';
import { StatusBadge } from '@/design-system/kit';
import { cn } from 'cn';
import { stepLabel, unitWords, valueText } from './columnSteps';
import { type CellSummary, cellOut, codeLines, type Focus, focusAt, focusOf, readsFor, readsOnce, readWords, shortValue, summaryWords, whenWords } from './notebook';
import { fieldLabel, unitLabel } from './reservesTable';

/**
 * One step of a block's column as a notebook cell (decision 0081): its head (the step, built in or a formula, and what
 * it did), then In (what it read for the unit in focus), Code (the formula, the lines that ran on that unit tinted with
 * the values they gave) or Rule (a built-in step's rule in words), and Out (what it changed across the column). This is
 * 0075's step panel too: the dataflow canvas (5c) shows one of these for a step clicked.
 */

/** A formula's trace on the focus unit: on its way, there, or why not. */
export type CellTrace = { state: 'loading' } | { state: 'done'; trace: FormulaTrace } | { state: 'failed'; why: string };

export function StepCell({
  column,
  at,
  summary,
  focus,
  open,
  onToggle,
  onFocus,
  trace,
  onEdit,
  onWhole,
  edited = false,
  picked = false,
}: {
  column: ColumnSteps;
  at: number;
  summary: CellSummary;
  focus: Focus | null;
  open: boolean;
  onToggle: () => void;
  onFocus: (f: Focus) => void;
  trace: CellTrace | null;
  onEdit?: () => void;
  onWhole: () => void;
  /** The step being edited, run here before it's saved (decision 0081, point 7). */
  edited?: boolean;
  /** Picked in the Dataflow view's strip (decision 0084): marked down its left in gold. */
  picked?: boolean;
}) {
  const cell = column.steps[at];
  const kind = cell.kind === 'formula' ? 'formula' : cell.kind === 'built_in' ? 'built in' : 'the blocks';
  return (
    <section className={cn('border-b border-border', picked && 'border-l-2 border-l-primary')} data-slot="notebook-cell" data-step={cell.step} data-open={open ? 'yes' : 'no'} data-picked={picked ? 'yes' : 'no'}>
      <button type="button" className="flex w-full items-center gap-2 px-2 py-1 text-left hover:bg-accent/40" onClick={onToggle} aria-expanded={open}>
        {open ? <ChevronDown className="size-3.5 shrink-0" /> : <ChevronRight className="size-3.5 shrink-0" />}
        <span className="w-5 shrink-0 text-right text-muted-foreground tabular-nums">{at}</span>
        <span className={cn('font-semibold', summary.quiet && 'font-normal text-muted-foreground')}>{at === 0 ? 'as input' : stepLabel(cell.step)}</span>
        <span className="text-muted-foreground">
          {kind}
          {cell.kind === 'formula' && ` · ${whenWords(cell.when)}`}
        </span>
        {edited && (
          <StatusBadge tone="edited" title="Run here as edited, not yet saved">
            edited
          </StatusBadge>
        )}
        <span className={cn('ml-auto', summary.quiet || at === 0 ? 'text-muted-foreground' : 'text-edited')} data-slot="cell-summary">
          {at === 0 ? `${cell.units.length} units as the blocks give them` : summaryWords(summary)}
        </span>
      </button>
      {open && (
        <div className="space-y-1.5 pr-2 pb-2 pl-9">
          {at > 0 && <CellIn column={column} at={at} focus={focus} trace={trace} />}
          {at > 0 && <CellCode column={column} at={at} trace={trace} onEdit={onEdit} />}
          <CellOutView column={column} at={at} focus={focus} onFocus={onFocus} onWhole={onWhole} />
        </div>
      )}
    </section>
  );
}

function Row({ label, children, slot }: { label: string; children: ReactNode; slot: string }) {
  return (
    <div className="flex gap-2" data-slot={slot}>
      <span className="w-10 shrink-0 pt-px font-semibold text-muted-foreground">{label}</span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

/** What the step read for the focus unit: a built-in step's parameters with where each came from, a formula's reads. */
function CellIn({ column, at, focus, trace }: { column: ColumnSteps; at: number; focus: Focus | null; trace: CellTrace | null }) {
  const cell = column.steps[at];
  const unit = focus ? focusAt(column.steps[at - 1].units, focus) : null;
  const who = unit ? unitWords(unit) : 'the unit in focus';
  if (cell.kind === 'formula') {
    if (!trace || trace.state === 'loading') return <Row label="In" slot="cell-in">{unit ? 'Tracing…' : `${who} isn't in the column at this step`}</Row>;
    if (trace.state === 'failed') return <Row label="In" slot="cell-in"><span className="text-destructive">{trace.why}</span></Row>;
    if (!trace.trace.fits) {
      return (
        <Row label="In" slot="cell-in">
          <span className="text-muted-foreground">
            Its filter ({whenWords(cell.when)}) leaves {who} out: the step doesn't run on it.
          </span>
        </Row>
      );
    }
    const reads = readsOnce(trace.trace.reads);
    return (
      <Row label="In" slot="cell-in">
        <span className="text-muted-foreground">{who}: </span>
        {reads.length === 0 ? <span className="text-muted-foreground">nothing read</span> : reads.map(([what, value], i) => <Chip key={`${what}${i}`} name={what} value={value} />)}
      </Row>
    );
  }
  const reads = unit ? readsFor(cell, unit.id) : [];
  const others = new Set(cell.reads.filter((r) => r.unit !== unit?.id).map((r) => r.unit)).size;
  const unitOf = (name: string) => column.fields.find((f) => f.name === name)?.unit ?? '';
  return (
    <Row label="In" slot="cell-in">
      <span className="text-muted-foreground">{who}: </span>
      {reads.length === 0 ? (
        <span className="text-muted-foreground">no parameter read</span>
      ) : (
        reads.map((r) => {
          const w = readWords(r, column.tables, unitOf(r.column));
          return <Chip key={r.column} name={w.name} value={w.value} from={w.from} missing={r.table === null} />;
        })
      )}
      {others > 0 && <span className="text-muted-foreground"> · and for {others} other unit{others === 1 ? '' : 's'}</span>}
    </Row>
  );
}

function Chip({ name, value, from, missing }: { name: string; value: string; from?: string; missing?: boolean }) {
  return (
    <span className="mr-3 inline-block whitespace-nowrap" data-slot="cell-read" data-name={name}>
      {name} <span className={cn('font-semibold tabular-nums', missing && 'font-normal text-muted-foreground')}>{value}</span>
      {from && <span className="text-muted-foreground"> ({from})</span>}
    </span>
  );
}

/** A formula with the lines that ran on the focus unit tinted and the values they gave; a built-in step's rule. */
function CellCode({ column, at, trace, onEdit }: { column: ColumnSteps; at: number; trace: CellTrace | null; onEdit?: () => void }) {
  const cell = column.steps[at];
  if (cell.kind !== 'formula' || !cell.source) {
    return (
      <Row label="Rule" slot="cell-rule">
        {cell.rule || 'built in'}
      </Row>
    );
  }
  const done = trace?.state === 'done' ? trace.trace : null;
  const lines = codeLines(cell.source, done && done.fits ? done : null);
  return (
    <Row label="Code" slot="cell-code">
      <div className="overflow-x-auto rounded border border-border bg-card font-mono text-[11px] leading-4">
        {lines.map((l) => (
          <div key={l.n} className={cn('flex gap-2 pr-2', l.ran > 0 && 'bg-selection')} data-line={l.n} data-ran={l.ran > 0 ? 'yes' : 'no'}>
            <span className="w-7 shrink-0 text-right text-muted-foreground select-none">{l.n}</span>
            <span className="whitespace-pre">{l.text}</span>
            {l.values.length > 0 && (
              <span className="ml-auto pl-4 whitespace-nowrap text-edited">
                {l.values.map(([k, v]) => `${k} = ${shortValue(v)}`).join(' · ')}
                {l.ran > 1 && <span className="text-muted-foreground"> ({l.ran} times)</span>}
              </span>
            )}
          </div>
        ))}
      </div>
      {done?.stopped && <div className="text-destructive">Stopped{done.stopped.line ? ` at line ${done.stopped.line}` : ''}: {done.stopped.message}</div>}
      {done && done.said.length > 0 && <div className="text-muted-foreground">Printed: {done.said.join(' · ')}</div>}
      {onEdit && (
        <button type="button" className="mt-0.5 inline-flex items-center gap-1 text-muted-foreground hover:text-foreground hover:underline" onClick={onEdit} title="Open this formula in the Parameters tab to edit it">
          <Braces className="size-3" /> Edit in the formula panel
        </button>
      )}
    </Row>
  );
}

/** What the step caused across the column: the units it touched and the fields it changed, what it said, the volume. */
function CellOutView({ column, at, focus, onFocus, onWhole }: { column: ColumnSteps; at: number; focus: Focus | null; onFocus: (f: Focus) => void; onWhole: () => void }) {
  const cell = column.steps[at];
  const out = cellOut(column.fields, column.steps, at);
  const focused = focus ? focusAt(cell.units, focus) : null;
  const volume = column.fields.find((f) => f.kind === 'volume');
  const balance =
    volume && out.volumeOut !== null
      ? out.volumeIn === null || Math.abs(out.volumeOut - out.volumeIn) <= 1e-6 * Math.max(1, out.volumeIn)
        ? `${valueText(out.volumeOut, volume.unit)} ${unitLabel(volume.unit)} in the column`
        : `${valueText(out.volumeIn, volume.unit)} → ${valueText(out.volumeOut, volume.unit)} ${unitLabel(volume.unit)} in the column`
      : '';
  return (
    <Row label="Out" slot="cell-out">
      {cell.said.length > 0 && <div className="text-muted-foreground">{cell.said.join(' · ')}</div>}
      {out.gone.length > 0 && <div className="text-edited">Taken away: {out.gone.join(', ')}</div>}
      {out.units.length > 0 && (
        <table className="mt-0.5 border-separate border-spacing-0 tabular-nums" data-slot="cell-out-table">
          <thead>
            <tr>
              <th className="pr-3 text-left font-normal text-muted-foreground">field</th>
              {out.units.map((u) => (
                <th
                  key={u.unit.id}
                  data-unit={u.label}
                  className={cn('min-w-20 cursor-pointer px-2 text-right align-bottom font-medium whitespace-nowrap hover:underline', focused?.id === u.unit.id && 'text-primary')}
                  title={`${u.detail}. Click to follow this unit`}
                  onClick={() => onFocus(focusOf(u.unit))}
                >
                  {u.isNew && <span className="mr-1 rounded bg-edited/20 px-1 text-[10px] font-normal text-edited">new</span>}
                  {u.label}
                  <div className="max-w-36 text-[10px] leading-tight font-normal whitespace-normal text-muted-foreground">{u.detail}</div>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {out.rows.map((r) => (
              <tr key={r.field.name} data-field={r.field.name}>
                <td className="pr-3 whitespace-nowrap">
                  {fieldLabel(r.field.name)} <span className="text-muted-foreground">{unitLabel(r.field.unit)}</span>
                </td>
                {r.cells.map((c, i) => (
                  <td key={out.units[i].unit.id} className={cn('px-2 text-right whitespace-nowrap', c.changed && 'text-edited')}>
                    {valueText(c.value, r.field.unit)}
                    {c.changed && <span className="ml-1 text-[10px] text-muted-foreground line-through">{valueText(c.before, r.field.unit) || 'none'}</span>}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {at > 0 && out.units.length === 0 && out.gone.length === 0 && <div className="text-muted-foreground">Nothing changed in this column.</div>}
      <div className="flex items-center gap-3 text-muted-foreground">
        {balance && <span data-slot="cell-volume">{balance}</span>}
        <button type="button" className="inline-flex items-center gap-1 hover:text-foreground hover:underline" onClick={onWhole} title="Every unit and every field after this step">
          <Columns3 className="size-3" /> Whole column
        </button>
      </div>
    </Row>
  );
}
