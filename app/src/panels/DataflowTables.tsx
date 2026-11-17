import type { ColumnSteps, ColumnUnit } from '@/bridge';
import { cn } from 'cn';
import { stepLabel, valueText } from './columnSteps';
import type { BeforeAfter, TableRow } from './dataflowView';
import { fieldLabel, unitLabel } from './reservesTable';

/**
 * Before and After the picked step (decision 0084, points 4 and 5), each its own column of the Dataflow view: every unit
 * of the block a row, the labels then the fields the step reads and changes. A field it reads is lit gold, one it
 * changed underlined blue; a value it changed is blue on After, a unit it made marked new, one it took away struck
 * through on Before. The unit followed is marked; a row clicked follows that unit.
 */
export function UnitTable({
  side,
  column,
  at,
  table,
  followed,
  onFollow,
  all,
  onAll,
}: {
  side: 'before' | 'after';
  column: ColumnSteps;
  at: number;
  table: BeforeAfter;
  followed: number | null;
  onFollow: (u: ColumnUnit) => void;
  all?: boolean;
  onAll?: (all: boolean) => void;
}) {
  const rows = side === 'before' ? table.before : table.after;
  const volume = side === 'before' ? table.volume.before : table.volume.after;
  const vField = column.fields.find((f) => f.kind === 'volume');
  const step = at === 0 ? 'the column as input' : stepLabel(column.steps[at].step);
  return (
    <div className="flex min-h-0 min-w-0 flex-col" data-slot={`${side}-table`}>
      <div className="flex shrink-0 flex-wrap items-baseline gap-x-2 border-b border-border px-2 py-1">
        <span className="font-semibold">{side === 'before' ? 'Before' : 'After'}</span>
        <span className="text-muted-foreground">{step}</span>
        {rows.length > 0 && (
          <span className="text-muted-foreground tabular-nums">
            · {rows.filter((r) => r.mark !== 'gone').length} units
            {volume !== null && vField && ` · ${valueText(volume, vField.unit)} ${unitLabel(vField.unit)}`}
          </span>
        )}
        {onAll && (
          <label className="ml-auto flex items-center gap-1 text-muted-foreground" title="Every field of the dataflow, the step's own first">
            <input type="checkbox" checked={!!all} onChange={(e) => onAll(e.target.checked)} />
            All fields{!all && table.more > 0 ? ` (${table.more} more)` : ''}
          </label>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {side === 'before' && at === 0 ? (
          <div className="p-2 text-muted-foreground">Nothing comes before the column as input: its units are the block solids as read, in Inputs.</div>
        ) : (
          <table className="w-max border-separate border-spacing-0 tabular-nums">
            <thead className="sticky top-0 z-10 bg-background">
              <tr>
                <th className="sticky left-0 z-10 border-b border-border bg-background px-2 py-0.5 text-left font-normal text-muted-foreground">unit</th>
                {table.fields.map(({ field, read, wrote }) => (
                  <th
                    key={field.name}
                    data-field={field.name}
                    data-read={read ? 'yes' : 'no'}
                    data-wrote={wrote ? 'yes' : 'no'}
                    className={cn('border-b border-border px-2 py-0.5 text-right font-medium whitespace-nowrap', read && 'bg-selection text-primary', wrote && 'underline decoration-edited decoration-2 underline-offset-2')}
                    title={[read && `${step} reads ${field.name}`, wrote && `${step} changed ${field.name}`].filter(Boolean).join('; ') || field.name}
                  >
                    {fieldLabel(field.name)} <span className="font-normal text-muted-foreground">{unitLabel(field.unit)}</span>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <Row key={r.unit.id} row={r} table={table} followed={followed === r.unit.id} onFollow={onFollow} />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}

function Row({ row, table, followed, onFollow }: { row: TableRow; table: BeforeAfter; followed: boolean; onFollow: (u: ColumnUnit) => void }) {
  const gone = row.mark === 'gone';
  return (
    <tr
      data-unit-id={row.unit.id}
      data-mark={row.mark ?? ''}
      data-followed={followed ? 'yes' : 'no'}
      className={cn('cursor-pointer hover:bg-accent/40', followed && 'bg-accent')}
      title={`${row.label}, ${row.detail}. Click to follow this unit`}
      onClick={() => onFollow(row.unit)}
    >
      <td className={cn('sticky left-0 bg-inherit px-2 py-0.5 whitespace-nowrap', followed ? 'bg-accent' : 'bg-background')}>
        {row.mark === 'new' && <span className="mr-1 rounded bg-edited/20 px-1 text-[10px] text-edited">new</span>}
        {gone && <span className="mr-1 rounded bg-muted px-1 text-[10px] text-muted-foreground">gone</span>}
        <span className={cn(followed && 'font-semibold text-primary', gone && 'text-muted-foreground line-through')}>{row.label}</span>
        <span className="ml-1.5 text-[10px] text-muted-foreground">{row.detail}</span>
      </td>
      {row.cells.map((c, i) => {
        const { field, read } = table.fields[i];
        return (
          <td
            key={field.name}
            data-changed={c.changed ? 'yes' : 'no'}
            className={cn('px-2 py-0.5 text-right whitespace-nowrap', read && 'bg-selection/40', c.changed && 'text-edited', gone && 'text-muted-foreground line-through')}
          >
            {valueText(c.value, field.unit)}
          </td>
        );
      })}
    </tr>
  );
}
