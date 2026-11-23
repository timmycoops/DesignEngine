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

/** A value typed in a cell: a number, words or nothing. */
export type Entry = number | string | null;

/**
 * What's typed in a cell, read for its column (Tables and scripts plan, decision 2): blank is nothing; in a column of
 * numbers, a number (commas between thousands allowed) or refused with why; in a column of words, the words.
 */
export function readEntry(text: string, numeric: boolean): { value: Entry } | { error: string } {
  const t = text.trim();
  if (t === '') return { value: null };
  if (!numeric) return { value: t };
  const n = Number(t.replace(/,(?=\d{3}(\D|$))/g, ''));
  return Number.isFinite(n) ? { value: n } : { error: `${t} isn't a number` };
}

/** A block of cells pasted: one value a line, a spreadsheet's row cut to its first cell. */
export function pastedLines(text: string): string[] {
  const lines = text.replace(/\r\n?/g, '\n').split('\n');
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
  return lines.map((l) => l.split('\t')[0]);
}

/** What's changed since a table's run, in words; empty when nothing has (decision 4). */
export function changedWords(c: { blocks: number; others: string[]; gone: number }): string {
  const parts = [...(c.blocks ? [`${fmt(c.blocks)} block${c.blocks === 1 ? '' : 's'}`] : []), ...c.others, ...(c.gone ? [`${fmt(c.gone)} input${c.gone === 1 ? '' : 's'} deleted`] : [])];
  if (parts.length === 0) return '';
  const list = parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}`;
  return `${list} changed since this table's run`;
}

/**
 * A CSV's columns by name, from its header row: commas between them, double quotes around one holding a comma, a unit
 * in brackets after a name left off (`rd (t/m3)` is `rd`), as update records reads them (decision 5).
 */
export function csvColumns(text: string): string[] {
  const first = text.replace(/^\uFEFF/, '').split(/\r?\n/).find((l) => l.trim() !== '') ?? '';
  const cells: string[] = [''];
  let quoted = false;
  for (let i = 0; i < first.length; i++) {
    const c = first[i];
    if (c === '"' && quoted && first[i + 1] === '"') {
      cells[cells.length - 1] += '"';
      i++;
    } else if (c === '"') quoted = !quoted;
    else if (c === ',' && !quoted) cells.push('');
    else cells[cells.length - 1] += c;
  }
  return cells.map((h) => h.trim().replace(/\s*\([^()]*\)$/, '')).filter((h) => h !== '');
}

/** The file's columns likely to be its key, the ones the target has by name: a table's levels, a block's cut words. */
export const likelyKey = (columns: readonly string[], keys: readonly string[]) => columns.filter((c) => keys.some((k) => k.toLowerCase() === c.toLowerCase()));

/** A new calculated column's formula to start from: it sets the column on the row (decision 6). */
export function starterScript(name: string): string {
  const field = /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ? `row.${name}` : `row[${JSON.stringify(name)}]`;
  return `def formula(row, p):\n    # Read the row's columns by name, row["Insitu\\\\Coal\\\\tonnes"], its words (row.seam) and p's parameters.\n    ${field} = 0\n`;
}

