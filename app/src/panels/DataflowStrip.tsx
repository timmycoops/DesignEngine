import { Box, Layers, Table2 } from 'lucide-react';
import { Fragment } from 'react';
import type { ColumnSteps } from '@/bridge';
import { cn } from 'cn';
import { stepLabel } from './columnSteps';
import { type InputChip, inputsOf, stripSteps, volumeShort, writeTip } from './dataflowView';

/**
 * The Dataflow view's strip (decision 0084, layout B): the column's inputs, then its steps in run order, each with what it
 * wrote to the reserves table under it and the units it passed on along the arrow after it. A step clicked is picked, lit
 * gold, and what it read is lit in the inputs: the table and the rows it took.
 */
export function DataflowStrip({ column, quiet, picked, unit, onPick }: { column: ColumnSteps; quiet: boolean[]; picked: number | null; unit: number | null; onPick: (at: number) => void }) {
  const inputs = inputsOf(column, picked, unit);
  const steps = stripSteps(column.steps, quiet);
  return (
    <div className="flex flex-col gap-1.5" data-slot="dataflow-strip">
      <div className="flex flex-wrap items-center gap-1.5" data-slot="dataflow-inputs">
        <span className="text-muted-foreground">Inputs</span>
        {inputs.map((c) => (
          <InputTag key={c.key} chip={c} />
        ))}
      </div>
      <div className="flex flex-wrap items-start gap-y-1.5" data-slot="dataflow-steps">
        {steps.map((s, i) => (
          <Fragment key={`${s.at}:${s.name}`}>
            <button
              type="button"
              data-strip-step={s.at === 0 ? 'as input' : s.name}
              aria-pressed={picked === s.at}
              onClick={() => onPick(s.at)}
              title={`${s.at === 0 ? 'The column as input, the blocks as read' : `${stepLabel(s.name)}, ${s.kind === 'formula' ? 'a formula' : 'built in'}`}${s.quiet ? ': changed nothing in this column' : ''}. Click to pick it.`}
              className={cn(
                'flex flex-col items-center gap-0.5 rounded-md border px-2 py-1 hover:border-primary',
                picked === s.at ? 'border-primary bg-selection text-primary ring-1 ring-primary' : s.kind === 'formula' ? 'border-dashed border-muted-foreground' : 'border-border',
                s.quiet && picked !== s.at && 'text-muted-foreground',
              )}
            >
              <span className={cn(!s.quiet && 'font-medium')}>
                {s.kind === 'formula' && <span className="text-muted-foreground">ƒ </span>}
                {s.at === 0 ? s.name : stepLabel(s.name)}
              </span>
              {s.writes.length > 0 && (
                <span className="flex flex-wrap justify-center gap-0.5" data-slot="step-writes">
                  {s.writes.map((w) => (
                    <span key={w.folder} className="rounded border border-border bg-muted px-1 text-[11px] font-normal text-muted-foreground" title={`Written to the reserves table:\n${writeTip(w)}`}>
                      {w.folder} {volumeShort(w.volume)}
                    </span>
                  ))}
                </span>
              )}
            </button>
            {i < steps.length - 1 && (
              <span className="flex flex-col items-center self-center px-1 text-muted-foreground" title={`${s.units} unit${s.units === 1 ? '' : 's'} passed on`} data-slot="step-units">
                <span aria-hidden>→</span>
                <span className="text-[10px] tabular-nums">{s.units}</span>
              </span>
            )}
          </Fragment>
        ))}
      </div>
    </div>
  );
}

const icons = { blocks: Box, model: Layers, table: Table2 };

function InputTag({ chip }: { chip: InputChip }) {
  const Icon = icons[chip.kind];
  return (
    <span
      className={cn('inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5', chip.lit ? 'border-dashed border-primary bg-selection text-primary' : 'border-border text-muted-foreground')}
      title={chip.title}
      data-input={chip.key}
      data-lit={chip.lit ? 'yes' : 'no'}
    >
      <Icon className="size-3" aria-hidden />
      {chip.label}
    </span>
  );
}
