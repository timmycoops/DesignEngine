/**
 * Any table as a grid (the Build Plan's "Tables and scripts plan", decision 1): a row a leaf, a column a field, grouped by
 * word columns with a subtotal a group, every subtotal the engine's add-up by the column's own rule. Kept free of React,
 * so the tests can run it.
 */

import type { Grid, GridColumn, GridRow } from '../bridge';
import { decimals } from './reservesTable.ts';
import { fmt } from './format.ts';

export const pathKey = (path: readonly string[]) => path.join('\u0001');

/** A group opened, or closed with every group under it. */
export function toggleOpen(open: readonly string[][], path: readonly string[]): string[][] {
  const at = pathKey(path);
  if (open.some((p) => pathKey(p) === at)) return open.filter((p) => !(p.length >= path.length && path.every((w, i) => w === p[i])));
  return [...open, [...path]];
}

export type Sort = { column: string; descending: boolean } | null;

/** A header clicked: up, then down, then the groups' own order again. */
export function nextSort(sort: Sort, column: string): Sort {
  if (sort?.column !== column) return { column, descending: false };
  return sort.descending ? null : { column, descending: true };
}

/** A cell as shown: a number to its unit's decimals (a whole number as it is when there's no unit), words as they are. */
export function cellText(value: number | string | null | undefined, unit: string): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  if (!Number.isFinite(value)) return '';
  if (unit === '' && Number.isInteger(value)) return String(value);
  return fmt(value, unit === '' ? 3 : decimals(unit));
}

/** What a group's row says it holds: "12 leaves", "1 block". */
export function heldText(row: GridRow, one: string, many: string): string {
  return `${fmt(row.leaves)} ${row.leaves === 1 ? one : many}`;
}

/** A column's header: its last part, and its folders above it when it has any, for its tip. */
export const columnTip = (c: GridColumn) => [...c.folders, c.label].join(' \\ ') + (c.unit ? ` (${c.unit})` : '');

/** The rows shown as CSV, exact values: each row's kind, depth, label and leaves, then a column a field. */
export function gridCsv(grid: Grid): string {
  const cell = (v: unknown) => {
    const text = v === undefined || v === null ? '' : String(v);
    return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
  };
  const head = ['row', 'depth', 'label', 'leaves', ...grid.columns.map((c) => (c.unit ? `${c.name} (${c.unit})` : c.name))].map(cell).join(',');
  const lines = grid.rows.map((r) => [r.kind, r.depth, [...r.path, ...(r.kind === 'leaf' ? [r.label] : [])].join(' > ') || 'All', r.leaves, ...r.cells].map(cell).join(','));
  return [head, ...lines].join('\r\n');
}

/** Group-by columns kept only where the table still has them, in the order picked. */
export const groupsKept = (group: readonly string[], groups: readonly string[]) => group.filter((g) => groups.includes(g));
