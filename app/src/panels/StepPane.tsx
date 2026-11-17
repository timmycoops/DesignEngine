import { Braces, Columns3 } from 'lucide-react';
import type { ReactNode } from 'react';
import type { ColumnSteps, ColumnUnit, FormulaTrace } from '@/bridge';
import { StatusBadge } from '@/design-system/kit';
import { cn } from 'cn';
import { stepLabel, unitWords, valueText } from './columnSteps';
import { writesByFolder } from './dataflowView';
import { type CellSummary, cellOut, codeLines, type Focus, focusAt, readsFor, readsOnce, readWords, shortValue, summaryWords, whenWords } from './notebook';
import { unitLabel } from './reservesTable';

/**
 * The picked step of the Dataflow view (decision 0084, layout B), left of Before and After: its head (the step, built in
 * or a formula, and what it did), then In (what it read for the unit followed, its settings with the table and column
 * each came from), Rule (a built-in step's rule in words) or Code (the formula, the lines that ran on that unit tinted
 * with the values they gave), Workings (a built-in step's arithmetic for that unit, decision 0082) and Out (what it said,
 * took away and wrote to the reserves table, the column's volume). Was the notebook's cell (decision 0081).
 */

/** A formula's trace on the unit followed: on its way, there, or why not. */
export type CellTrace = { state: 'loading' } | { state: 'done'; trace: FormulaTrace } | { state: 'failed'; why: string };

export function StepPane({
  column,
  at,
  summary,
  focus,
  trace,
  onEdit,
  onWhole,
  edited = false,
}: {
  column: ColumnSteps;
  at: number;
  summary: CellSummary;
  focus: Focus | null;
  trace: CellTrace | null;
  onEdit?: () => void;
  onWhole: () => void;
  /** The step being edited, run here before it's saved (decision 0081, point 7). */
  edited?: boolean;
}) {
  const cell = column.steps[at];
  const kind = cell.kind === 'formula' ? 'formula' : cell.kind === 'built_in' ? 'built in' : 'the blocks';
  const unit = focus && at > 0 ? focusAt(column.steps[at - 1].units, focus) : null;
  return (
    <section className="space-y-1.5 p-2" data-slot="step-pane" data-step={cell.step}>
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span className="text-muted-foreground tabular-nums">{at}</span>
        <span className="font-semibold">{at === 0 ? 'Imported, the column as input' : stepLabel(cell.step)}</span>
        <span className="text-muted-foreground">
          {kind}
          {cell.kind === 'formula' && ` · ${whenWords(cell.when)}`}
        </span>
        {edited && (
          <StatusBadge tone="edited" title="Run here as edited, not yet saved">
            edited
          </StatusBadge>
        )}
      </div>
      <div className={cn(summary.quiet || at === 0 ? 'text-muted-foreground' : 'text-edited')} data-slot="cell-summary">
        {at === 0 ? `${cell.units.length} units as the blocks give them` : summaryWords(summary)}
      </div>
      {at > 0 && <CellIn column={column} at={at} unit={unit} trace={trace} />}
      {at > 0 && <CellCode column={column} at={at} trace={trace} onEdit={onEdit} />}
      {at > 0 && cell.kind === 'built_in' && <CellWorkings column={column} at={at} unit={unit} />}
      <CellOut column={column} at={at} onWhole={onWhole} />
    </section>
  );
}

function Row({ label, children, slot }: { label: string; children: ReactNode; slot: string }) {
  return (
    <div className="flex gap-2" data-slot={slot}>
      <span className="w-16 shrink-0 pt-px font-semibold text-muted-foreground">{label}</span>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

/** What the step read for the unit followed: a built-in step's parameters with where each came from, a formula's reads. */
function CellIn({ column, at, unit, trace }: { column: ColumnSteps; at: number; unit: ColumnUnit | null; trace: CellTrace | null }) {
  const cell = column.steps[at];
  const who = unit ? unitWords(unit) : 'the unit followed';
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

/** A setting read, lit gold as its table is in Inputs (decision 0084, point 2). */
function Chip({ name, value, from, missing }: { name: string; value: string; from?: string; missing?: boolean }) {
  return (
    <span className={cn('mr-1.5 mb-0.5 inline-block rounded px-1 whitespace-nowrap', !missing && from && 'bg-selection')} data-slot="cell-read" data-name={name}>
      {name} <span className={cn('font-semibold tabular-nums', missing ? 'font-normal text-muted-foreground' : from && 'text-primary')}>{value}</span>
      {from && <span className="text-muted-foreground"> ({from})</span>}
    </span>
  );
}

/** A formula with the lines that ran on the unit followed tinted and the values they gave; a built-in step's rule. */
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

/** A figure as a working gives it: thousands marked, to a tenth from 100 up, else to three places. */
const figure = (v: number) => v.toLocaleString('en-AU', { maximumFractionDigits: Math.abs(v) >= 100 ? 1 : 3 });

/**
 * A built-in step's arithmetic for the unit followed (decision 0082, workstream 2): each line what was worked out or
 * tested, the numbers that went in, what it came to and what followed. A step that writes none keeps its rule.
 */
function CellWorkings({ column, at, unit }: { column: ColumnSteps; at: number; unit: ColumnUnit | null }) {
  const cell = column.steps[at];
  if (cell.workings.length === 0) return null;
  const lines = unit ? cell.workings.filter((w) => w.unit === unit.id) : [];
  const others = new Set(cell.workings.filter((w) => w.unit !== unit?.id).map((w) => w.unit)).size;
  return (
    <Row label="Workings" slot="cell-workings">
      {lines.length === 0 && <div className="text-muted-foreground">none for {unit ? unitWords(unit) : 'the unit followed'}</div>}
      {lines.map((w, i) => (
        <div key={i} className="leading-snug" data-slot="working" data-what={w.what}>
          <span className="font-semibold">{w.what}</span> <span className="text-muted-foreground">{w.sum}</span>
          {w.value !== null && (
            <span className="tabular-nums">
              {' '}= <span className="font-semibold text-edited">{figure(w.value)}</span> {w.measure}
            </span>
          )}
          {w.then && <span className="text-muted-foreground"> → {w.then}</span>}
        </div>
      ))}
      {others > 0 && <div className="text-muted-foreground">and for {others} other unit{others === 1 ? '' : 's'}: follow one in Before or After</div>}
    </Row>
  );
}

/** What the step caused across the column: what it said and took away, what it wrote to the reserves table, the volume. */
function CellOut({ column, at, onWhole }: { column: ColumnSteps; at: number; onWhole: () => void }) {
  const cell = column.steps[at];
  const out = cellOut(column.fields, column.steps, at);
  const volume = column.fields.find((f) => f.kind === 'volume');
  const balance =
    volume && out.volumeOut !== null
      ? out.volumeIn === null || Math.abs(out.volumeOut - out.volumeIn) <= 1e-6 * Math.max(1, out.volumeIn)
        ? `${valueText(out.volumeOut, volume.unit)} ${unitLabel(volume.unit)} in the column`
        : `${valueText(out.volumeIn, volume.unit)} → ${valueText(out.volumeOut, volume.unit)} ${unitLabel(volume.unit)} in the column`
      : '';
  const writes = writesByFolder(cell.writes ?? []).flatMap((w) => w.parts);
  return (
    <Row label="Out" slot="cell-out">
      {cell.said.length > 0 && <div className="text-muted-foreground">{cell.said.join(' · ')}</div>}
      {out.gone.length > 0 && <div className="text-edited">Taken away: {out.gone.join(', ')}</div>}
      {writes.length > 0 && (
        <div data-slot="cell-writes">
          <span className="text-muted-foreground">Wrote </span>
          {writes.map((w) => (
            <span key={w.folder} className="mr-2 inline-block whitespace-nowrap tabular-nums" data-folder={w.folder}>
              {w.folder} <span className="font-semibold">{valueText(w.volume, volume?.unit ?? 'm3')}</span> {unitLabel(volume?.unit ?? 'm3')}
            </span>
          ))}
        </div>
      )}
      <div className="flex items-center gap-3 text-muted-foreground">
        {balance && <span data-slot="cell-volume">{balance}</span>}
        <button type="button" className="inline-flex items-center gap-1 hover:text-foreground hover:underline" onClick={onWhole} title="Every unit and every field after this step, a step at a time">
          <Columns3 className="size-3" /> Whole column
        </button>
      </div>
    </Row>
  );
}
