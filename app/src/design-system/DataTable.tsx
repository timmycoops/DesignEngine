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
}

/** A compact, sortable table (TanStack Table on shadcn/ui's table) with a sticky header. */
export function DataTable<T extends RowData>({ data, columns, rowId, selected, onRowClick, rowClassName, empty }: DataTableProps<T>) {
  const table = useTable({ features, columns, data, getRowId: rowId });
  const rows = table.getRowModel().rows;
  return (
    <Table className="text-xs">
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
          rows.map((row) => (
            <TableRow
              key={row.id}
              data-state={row.id === selected ? 'selected' : undefined}
              className={cn(onRowClick && 'cursor-pointer', 'data-[state=selected]:bg-selection', rowClassName?.(row.original))}
              onClick={onRowClick && (() => onRowClick(row.original))}
            >
              {row.getAllCells().map((cell) => (
                <TableCell key={cell.id} className="px-2 py-1">
                  <table.FlexRender cell={cell} />
                </TableCell>
              ))}
            </TableRow>
          ))
        )}
      </TableBody>
    </Table>
  );
}
