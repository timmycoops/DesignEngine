import {
  type ColumnDef,
  createColumnHelper,
  createSortedRowModel,
  type RowData,
  rowSortingFeature,
  sortFn_alphanumeric,
  tableFeatures,
  useTable,
} from '@tanstack/react-table';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { cn } from 'cn';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/design-system/ui/table';

/** The features every app table registers: click a header to sort. */
export const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { alphanumeric: sortFn_alphanumeric },
});

export type Features = typeof features;

export const columnHelper = <T extends RowData>() => createColumnHelper<Features, T>();

interface DataTableProps<T extends RowData> {
  data: T[];
  // Columns differ in value type, so the list takes any.
  columns: ColumnDef<Features, T, any>[];
  rowId: (row: T) => string;
  selected?: string | null;
  onRowClick?: (row: T) => void;
  rowClassName?: (row: T) => string | undefined;
  /** Shown in place of the body when there are no rows. */
  empty?: string;
  /**
   * Column widths: the table scrolls itself and draws only the rows on screen, each one line high,
   * so tens of thousands of rows cost no more than a screenful. `undefined` takes what is left.
   */
  windowed?: (string | undefined)[];
}

/** A windowed row's height (px): one line of `text-xs`, its padding and its border. */
const ROW_HEIGHT = 25;
/** Rows drawn beyond each end of the screen, so a fast scroll shows no gap. */
const OVERSCAN = 10;

/** The rows in view of a scrolling element, and that element's props. */
function useRowWindow(count: number, enabled: boolean) {
  const scroller = useRef<HTMLDivElement>(null);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(800);
  useEffect(() => {
    const element = scroller.current;
    if (!enabled || !element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight));
    observer.observe(element);
    return () => observer.disconnect();
  }, [enabled]);
  const first = Math.max(0, Math.floor(top / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(count, Math.ceil((top + height) / ROW_HEIGHT) + OVERSCAN);
  return { scroller, first: enabled ? first : 0, last: enabled ? last : count, onScroll: () => setTop(scroller.current?.scrollTop ?? 0) };
}

/** A compact, sortable table (TanStack Table on shadcn/ui's table) with a sticky header. */
export function DataTable<T extends RowData>({ data, columns, rowId, selected, onRowClick, rowClassName, empty, windowed }: DataTableProps<T>) {
  const table = useTable({ features, columns, data, getRowId: rowId });
  const rows = table.getRowModel().rows;
  const { scroller, first, last, onScroll } = useRowWindow(rows.length, windowed !== undefined);
  const body = (
    <>
      <TableHeader className="sticky top-0 z-10 bg-background">
        {table.getHeaderGroups().map((group) => (
          <TableRow key={group.id} className="hover:bg-transparent">
            {group.headers.map((header) => {
              const sorted = header.column.getIsSorted();
              return (
                <TableHead
                  key={header.id}
                  className={cn('h-7 px-2 select-none', header.column.getCanSort() && 'cursor-pointer')}
                  onClick={header.column.getToggleSortingHandler()}
                >
                  <span className="inline-flex items-center gap-1">
                    {header.isPlaceholder ? null : <table.FlexRender header={header} />}
                    {sorted === 'asc' && <ChevronUp className="size-3" />}
                    {sorted === 'desc' && <ChevronDown className="size-3" />}
                  </span>
                </TableHead>
              );
            })}
          </TableRow>
        ))}
      </TableHeader>
      <TableBody>
        {rows.length === 0 && empty ? (
          <TableRow className="hover:bg-transparent">
            <TableCell colSpan={columns.length} className="px-2 py-3 whitespace-normal text-muted-foreground">
              {empty}
            </TableCell>
          </TableRow>
        ) : (
          <>
            {first > 0 && <tr style={{ height: first * ROW_HEIGHT }} />}
            {rows.slice(first, last).map((row) => (
              <TableRow
                key={row.id}
                data-state={row.id === selected ? 'selected' : undefined}
                className={cn(onRowClick && 'cursor-pointer', 'data-[state=selected]:bg-selection', rowClassName?.(row.original))}
                style={windowed && { height: ROW_HEIGHT }}
                onClick={onRowClick && (() => onRowClick(row.original))}
              >
                {row.getAllCells().map((cell) => (
                  <TableCell key={cell.id} className={cn('px-2 py-1', windowed && 'truncate')}>
                    <table.FlexRender cell={cell} />
                  </TableCell>
                ))}
              </TableRow>
            ))}
            {last < rows.length && <tr style={{ height: (rows.length - last) * ROW_HEIGHT }} />}
          </>
        )}
      </TableBody>
    </>
  );
  if (!windowed) return <Table className="text-xs">{body}</Table>;
  // Its own scroller and no shadcn wrapper, whose overflow would unstick the header.
  return (
    <div ref={scroller} onScroll={onScroll} className="h-full overflow-auto">
      <table className="w-full table-fixed caption-bottom text-xs">
        <colgroup>
          {windowed.map((width, k) => (
            <col key={k} style={{ width }} />
          ))}
        </colgroup>
        {body}
      </table>
    </div>
  );
}
