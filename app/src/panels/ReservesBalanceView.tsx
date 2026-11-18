import type { BreakdownField } from '@/bridge';
import { StatusBadge } from '@/design-system/kit';
import { cn } from 'cn';
import { valueText } from './columnSteps';
import { balanceRows, hasBalance } from './reservesBalance';

/**
 * The Balance view of the Reserves table tab (decision 0086): the node's coal, waste and total a row a stage, Imported ›
 * Insitu › ROM › Scheduled, with what moved between them signed as it changes each part, and each stage's check, green
 * where it holds and red where it doesn't.
 */
export function ReservesBalance({ fields, node }: { fields: BreakdownField[]; node: string }) {
  if (!hasBalance(fields)) {
    return (
      <div className="p-3 text-muted-foreground" data-slot="reserves-balance">
        This table was made before the balance: Run again, every block, in the Dataflow tab writes it in place.
      </div>
    );
  }
  const rows = balanceRows(fields);
  const moved = (x: number | null) => (x === null ? '' : `${x > 0 ? '+' : x < 0 ? '−' : ''}${valueText(Math.abs(x), 'm3')}`);
  return (
    <div className="p-2" data-slot="reserves-balance">
      <div className="mb-1 text-muted-foreground">
        {node}: volumes in m³, coal and waste from Imported to Scheduled, what moved between them, and each stage checked against the one before.
      </div>
      <table className="border-separate border-spacing-0 tabular-nums">
        <thead>
          <tr>
            <th className="border-b border-border px-2 py-1 text-left font-medium text-muted-foreground">stage</th>
            {['Coal', 'Waste', 'Total'].map((p) => (
              <th key={p} className="min-w-28 border-b border-border px-2 py-1 text-right font-medium">
                {p}
              </th>
            ))}
            <th className="border-b border-border px-2 py-1" />
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr
              key={r.key}
              data-balance={r.key}
              data-kind={r.kind}
              data-holds={r.holds === undefined ? undefined : r.holds ? 'yes' : 'no'}
              className={cn(r.kind === 'stage' && 'font-semibold', r.kind === 'move' && 'text-muted-foreground', r.kind === 'check' && 'text-[11px]')}
            >
              <td className={cn('px-2 py-0.5', r.kind !== 'stage' && 'pl-6', r.kind === 'stage' && 'border-t border-border')}>{r.label}</td>
              {[r.coal, r.waste, r.total].map((x, i) => (
                <td
                  key={i}
                  className={cn('px-2 py-0.5 text-right', r.kind === 'stage' && 'border-t border-border', r.kind === 'check' && (r.holds ? 'text-ok' : 'text-destructive'))}
                >
                  {r.kind === 'move' ? moved(x) : valueText(r.kind === 'check' && r.holds && x !== null ? 0 : x, 'm3')}
                </td>
              ))}
              <td className={cn('px-2 py-0.5', r.kind === 'stage' && 'border-t border-border')}>
                {r.kind === 'check' &&
                  (r.holds ? (
                    <StatusBadge tone="ok" title="Coal, waste and the total each hold to within 1e-6 of the imported volume">
                      holds
                    </StatusBadge>
                  ) : (
                    <StatusBadge tone="refused" title="Volume isn't accounted for between these stages: open the block's column in the Dataflow tab to see where">
                      off
                    </StatusBadge>
                  ))}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
