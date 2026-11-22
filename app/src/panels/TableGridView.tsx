import { ChevronDown, ChevronRight, ChevronUp } from 'lucide-react';
import type { Grid, GridRow } from '@/bridge';
import { ROW_HEIGHT, useRowWindow } from '@/design-system/DataTable';
import { cn } from 'cn';
import { fmt } from './format';
import { cellText, columnTip, heldText, type Sort } from './tableGrid';
import { unitLabel } from './reservesTable';

/**
 * Any table as a grid (Tables and scripts plan, decision 1): the total, a row a group with its subtotal (each added up
 * by the column's own rule, in the engine), and the leaves of the groups opened. A header sorts each group's rows by it;
 * only the rows on screen are drawn, so thousands cost no more than a screenful.
 */
export function TableGrid({
  grid,
  sort,
  onSort,
  onToggle,
  onRow,
  lit,
  rowWords,
  empty,
}: {
  grid: Grid;
  sort: Sort;
  onSort: (column: string) => void;
  onToggle: (path: string[]) => void;
  onRow?: (row: GridRow) => void;
  /** Whether a row is the one picked, lit gold. */
  lit?: (row: GridRow) => boolean;
  /** What a row holds, in the words for one and many ("block", "blocks"). */
  rowWords: [string, string];
  empty?: string;
}) {
  const { scroller, first, last, onScroll } = useRowWindow(grid.rows.length, true);
  return (
    <div ref={scroller} onScroll={onScroll} className="h-full overflow-auto" data-slot="table-grid">
      <table className="border-separate border-spacing-0 text-xs tabular-nums">
        <thead className="sticky top-0 z-10 bg-card">
          <tr>
            <th className="sticky left-0 z-20 min-w-56 border-b border-border bg-card px-2 py-1 text-left font-medium text-muted-foreground">{grid.rows.length > 1 ? '' : empty}</th>
            <th className="min-w-16 border-b border-border px-2 py-1 text-right font-medium text-muted-foreground">{rowWords[1]}</th>
            {grid.columns.map((c) => (
              <th
                key={c.name}
                data-col={c.name}
                title={`${columnTip(c)}. Click to sort each group's rows by it`}
                className="min-w-24 cursor-pointer border-b border-border px-2 py-1 text-right font-medium whitespace-nowrap select-none hover:text-foreground"
                onClick={() => onSort(c.name)}
              >
                <span className="inline-flex items-center gap-0.5">
                  {c.folders.length > 0 && <span className="font-normal text-muted-foreground">{c.folders.join(' ')}</span>}
                  {c.label}
                  {c.unit && <span className="font-normal text-muted-foreground">{unitLabel(c.unit)}</span>}
                  {sort?.column === c.name && (sort.descending ? <ChevronDown className="size-3" /> : <ChevronUp className="size-3" />)}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {first > 0 && <tr style={{ height: first * ROW_HEIGHT }} />}
          {grid.rows.slice(first, last).map((r) => {
            const picked = lit?.(r) ?? false;
            return (
              <tr
                key={`${r.kind}:${r.path.join('\u0001')}:${r.row ?? ''}`}
                data-row={r.kind}
                data-path={r.path.join(' > ')}
                style={{ height: ROW_HEIGHT }}
                className={cn('group', onRow && 'cursor-pointer', picked ? 'bg-selection text-primary' : 'hover:bg-accent/40', r.kind === 'total' && 'font-semibold', r.kind === 'group' && 'font-medium')}
                onClick={onRow && (() => onRow(r))}
              >
                <td className={cn('sticky left-0 border-b border-border/40 py-px pr-2 whitespace-nowrap', picked ? 'bg-selection' : 'bg-card', r.kind === 'total' && 'bg-muted/50')} style={{ paddingLeft: 6 + r.depth * 14 }}>
                  {r.kind === 'group' ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-0.5"
                      aria-label={`${r.open ? 'Close' : 'Open'} ${r.label}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        onToggle(r.path);
                      }}
                    >
                      {r.open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
                      {r.label}
                    </button>
                  ) : (
                    <span className={cn(r.kind === 'leaf' && 'pl-3.5')}>{r.label}</span>
                  )}
                </td>
                <td className={cn('border-b border-border/40 px-2 py-px text-right text-muted-foreground', r.kind === 'total' && 'bg-muted/50')} title={heldText(r, rowWords[0], rowWords[1])}>
                  {r.kind === 'leaf' ? '' : fmt(r.leaves)}
                </td>
                {r.cells.map((v, k) => (
                  <td key={grid.columns[k].name} className={cn('border-b border-border/40 px-2 py-px text-right whitespace-nowrap', r.kind === 'total' && 'bg-muted/50', r.kind === 'leaf' && 'text-muted-foreground', picked && 'text-primary')}>
                    {cellText(v, grid.columns[k].unit)}
                  </td>
                ))}
              </tr>
            );
          })}
          {last < grid.rows.length && <tr style={{ height: (grid.rows.length - last) * ROW_HEIGHT }} />}
        </tbody>
      </table>
      {grid.cut > 0 && (
        <div className="sticky left-0 px-2 py-1 text-warning" data-slot="grid-cut">
          {fmt(grid.cut)} more {grid.cut === 1 ? rowWords[0] : rowWords[1]} not shown: group them, or close a group, to see the rest. Every total holds them all.
        </div>
      )}
    </div>
  );
}
