import type { ParameterTable } from '../bridge';

/**
 * A parameter table as the Parameters tab edits it (decision 0075, 5d): filter columns then value columns, each row's cells
 * as typed, the same text as the CSV and the bench rules (`U`, `1-5`, `U,M`, blank for any). Every row and column keeps a
 * key and what it held when the table was opened, so a cell edited since shows as edited wherever rows and columns have
 * moved. Kept apart from the panel, with no runtime imports, so the tests run it on its own.
 */

export type ColumnKind = 'filter' | 'value';

export interface GridColumn {
  key: string;
  kind: ColumnKind;
  name: string;
  unit: string;
  /** Holds words, not numbers (a value column). */
  text: boolean;
  note: string;
  /** As it was when opened; null for a column added since. */
  was: { name: string; unit: string; text: boolean } | null;
}

export interface GridRow {
  key: string;
  /** Its place in the saved table; null for a row added since. */
  saved: number | null;
  cells: Record<string, string>;
  /** Its cells when opened, by column key; null for a row added since. */
  was: Record<string, string> | null;
}

export interface Grid {
  columns: GridColumn[];
  rows: GridRow[];
}

/** A cell or a column's header that can't be saved as it is: its row (null for the header), its column and why. */
export interface Problem {
  row: string | null;
  column: string;
  why: string;
}

let next = 0;
const key = (prefix: string) => `${prefix}${++next}`;

const cellOf = (v: number | string | undefined) => (v === undefined ? '' : String(v));
const sameName = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
const get = (r: Record<string, string> | undefined, name: string) => (r ? (Object.entries(r).find(([k]) => sameName(k, name))?.[1] ?? '') : '');

/** The table as a grid, filters first. */
export function toGrid(t: ParameterTable): Grid {
  const columns: GridColumn[] = [
    ...t.filters.map((f) => ({ key: key('c'), kind: 'filter' as const, name: f, unit: '', text: true, note: '', was: { name: f, unit: '', text: true } })),
    ...t.columns.map((c) => ({ key: key('c'), kind: 'value' as const, name: c.name, unit: c.unit ?? '', text: !!c.text, note: c.note ?? '', was: { name: c.name, unit: c.unit ?? '', text: !!c.text } })),
  ];
  const rows = t.rows.map((r, i) => {
    const cells: Record<string, string> = {};
    for (const c of columns) {
      cells[c.key] = c.kind === 'filter' ? get(r.when, c.name) : cellOf(r.values[c.name] ?? Object.entries(r.values).find(([k]) => sameName(k, c.name))?.[1]);
    }
    return { key: key('r'), saved: i, cells, was: { ...cells } };
  });
  return { columns, rows };
}

const NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;

/** Whether a number column's cell reads as a number (blank gives nothing, and is fine). */
export const readsAsNumber = (cell: string) => cell.trim() === '' || NUMBER.test(cell.trim());

/**
 * The grid as a table to save, with what stops it being saved: a column with no name or named twice, a word in a number
 * column. A row left blank in every cell is left out.
 */
export function fromGrid(g: Grid): { table: ParameterTable; problems: Problem[]; blankRows: number } {
  const problems: Problem[] = [];
  g.columns.forEach((c, i) => {
    if (c.name.trim() === '') problems.push({ row: null, column: c.key, why: 'This column has no name' });
    else if (g.columns.some((d, j) => j < i && sameName(d.name, c.name))) problems.push({ row: null, column: c.key, why: `${c.name.trim()} is named twice` });
  });
  const filters = g.columns.filter((c) => c.kind === 'filter');
  const values = g.columns.filter((c) => c.kind === 'value');
  const table: ParameterTable = {
    filters: filters.map((c) => c.name.trim()),
    columns: values.map((c) => ({ name: c.name.trim(), ...(c.unit.trim() ? { unit: c.unit.trim() } : {}), ...(c.text ? { text: true } : {}), ...(c.note ? { note: c.note } : {}) })),
    rows: [],
  };
  let blankRows = 0;
  for (const r of g.rows) {
    if (g.columns.every((c) => (r.cells[c.key] ?? '').trim() === '')) {
      blankRows++;
      continue;
    }
    const when: Record<string, string> = {};
    const vals: Record<string, number | string> = {};
    for (const c of filters) {
      const cell = (r.cells[c.key] ?? '').trim();
      if (cell) when[c.name.trim()] = cell;
    }
    for (const c of values) {
      const cell = (r.cells[c.key] ?? '').trim();
      if (!cell) continue;
      if (!c.text && !readsAsNumber(cell)) {
        problems.push({ row: r.key, column: c.key, why: `${c.name.trim() || 'This column'} holds numbers, not '${cell}'` });
        continue;
      }
      vals[c.name.trim()] = c.text ? cell : Number(cell);
    }
    table.rows.push(Object.keys(when).length ? { when, values: vals } : { values: vals });
  }
  return { table, problems, blankRows };
}

/** Whether a cell has changed since the table was opened (a new row's or new column's cells always have). */
export function cellEdited(row: GridRow, column: GridColumn): boolean {
  if (row.was === null || column.was === null) return (row.cells[column.key] ?? '') !== '';
  return (row.was[column.key] ?? '') !== (row.cells[column.key] ?? '');
}

/** Whether a column's name, unit or kind of value has changed since it was opened. */
export const columnEdited = (c: GridColumn) => c.was === null || c.was.name !== c.name || c.was.unit !== c.unit || c.was.text !== c.text;

/** Whether anything has changed since the table was opened: a cell, a column, or rows added, taken out or moved. */
export function gridEdited(g: Grid, savedRows: number, savedColumns: string[]): boolean {
  if (g.rows.length !== savedRows || g.rows.some((r, i) => r.saved !== i)) return true;
  if (g.columns.map((c) => c.key).join() !== savedColumns.join()) return true;
  return g.columns.some(columnEdited) || g.rows.some((r) => g.columns.some((c) => cellEdited(r, c)));
}

export const setCell = (g: Grid, row: string, column: string, text: string): Grid => ({ ...g, rows: g.rows.map((r) => (r.key === row ? { ...r, cells: { ...r.cells, [column]: text } } : r)) });

/** A blank row put in at `at` (its place, from 0); at the end if left out. */
export function insertRow(g: Grid, at = g.rows.length): Grid {
  const row: GridRow = { key: key('r'), saved: null, cells: {}, was: null };
  return { ...g, rows: [...g.rows.slice(0, at), row, ...g.rows.slice(at)] };
}

export const deleteRows = (g: Grid, rows: Set<string>): Grid => ({ ...g, rows: g.rows.filter((r) => !rows.has(r.key)) });

/** A row moved up (-1) or down (+1) one place; the order is the order the table is read. */
export function moveRow(g: Grid, row: string, by: -1 | 1): Grid {
  const i = g.rows.findIndex((r) => r.key === row);
  const j = i + by;
  if (i < 0 || j < 0 || j >= g.rows.length) return g;
  const rows = [...g.rows];
  [rows[i], rows[j]] = [rows[j], rows[i]];
  return { ...g, rows };
}

/** A new column of a kind, after the last of its kind, named so it doesn't clash. */
export function addColumn(g: Grid, kind: ColumnKind): Grid {
  const base = kind === 'filter' ? 'attribute' : 'value';
  let name = base;
  for (let n = 2; g.columns.some((c) => sameName(c.name, name)); n++) name = `${base}_${n}`;
  const column: GridColumn = { key: key('c'), kind, name, unit: '', text: kind === 'filter', note: '', was: null };
  const lastOfKind = g.columns.map((c) => c.kind).lastIndexOf(kind);
  const at = lastOfKind >= 0 ? lastOfKind + 1 : kind === 'filter' ? 0 : g.columns.length;
  return { ...g, columns: [...g.columns.slice(0, at), column, ...g.columns.slice(at)] };
}

export const deleteColumn = (g: Grid, column: string): Grid => ({ ...g, columns: g.columns.filter((c) => c.key !== column) });

export const editColumn = (g: Grid, column: string, change: Partial<Pick<GridColumn, 'name' | 'unit' | 'text'>>): Grid => ({ ...g, columns: g.columns.map((c) => (c.key === column ? { ...c, ...change } : c)) });

/** A column moved left (-1) or right (+1), among its own kind: filters stay before values. */
export function moveColumn(g: Grid, column: string, by: -1 | 1): Grid {
  const i = g.columns.findIndex((c) => c.key === column);
  const j = i + by;
  if (i < 0 || j < 0 || j >= g.columns.length || g.columns[j].kind !== g.columns[i].kind) return g;
  const columns = [...g.columns];
  [columns[i], columns[j]] = [columns[j], columns[i]];
  return { ...g, columns };
}

/**
 * Cells copied from Excel (or any sheet): a line a row, tabs between cells; a cell in quotes may hold tabs, line breaks
 * and doubled quotes, as Excel writes them.
 */
export function parseClipboard(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = '';
  let quoted = false;
  let atStart = true;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quoted) {
      if (ch === '"' && text[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (ch === '"') quoted = false;
      else cell += ch;
    } else if (ch === '"' && atStart) {
      quoted = true;
      atStart = false;
    } else if (ch === '\t') {
      row.push(cell);
      cell = '';
      atStart = true;
    } else if (ch === '\n' || ch === '\r') {
      if (ch === '\r' && text[i + 1] === '\n') i++;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
      atStart = true;
    } else {
      cell += ch;
      atStart = false;
    }
  }
  if (cell !== '' || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return rows;
}

/**
 * A block of cells pasted with its top left at a cell: rows added below as needed; cells past the last column are left
 * out and counted.
 */
export function paste(g: Grid, row: string, column: string, block: string[][]): { grid: Grid; cut: number } {
  let grid = g;
  const r0 = grid.rows.findIndex((r) => r.key === row);
  const c0 = grid.columns.findIndex((c) => c.key === column);
  if (r0 < 0 || c0 < 0) return { grid, cut: 0 };
  while (grid.rows.length < r0 + block.length) grid = insertRow(grid);
  let cut = 0;
  const rows = grid.rows.map((r, i) => {
    const line = block[i - r0];
    if (i < r0 || !line) return r;
    const cells = { ...r.cells };
    line.forEach((text, j) => {
      const c = grid.columns[c0 + j];
      if (c) cells[c.key] = text.trim();
      else cut++;
    });
    return { ...r, cells };
  });
  return { grid: { ...grid, rows }, cut };
}

/**
 * A block of cells pasted into the grid as the tab shows it, turned (Tim, 16 November: parameters down the side): a line
 * of the block runs across the table's rows for one parameter, the next line the next parameter down. Columns are added
 * to the right as needed; cells past the last parameter are left out and counted.
 */
export function pasteTurned(g: Grid, row: string, column: string, block: string[][]): { grid: Grid; cut: number } {
  const turned: string[][] = [];
  block.forEach((line, i) =>
    line.forEach((text, j) => {
      turned[j] ??= [];
      turned[j][i] = text; // a cell the block doesn't give stays a hole, and leaves the grid's cell as it was
    }),
  );
  return paste(g, row, column, turned);
}

const quoteCell = (s: string) => (/[\t\n\r"]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s);
const heading = (c: GridColumn) => (c.kind === 'filter' ? `when ${c.name}` : c.unit ? `${c.name} (${c.unit})` : c.name);

/** Rows as Excel takes them: a header line of the columns as CSV heads them, then a line a row, tabs between. */
export function copyText(g: Grid, rows: Set<string> | null): string {
  const lines = g.rows.filter((r) => !rows || rows.has(r.key)).map((r) => g.columns.map((c) => quoteCell(r.cells[c.key] ?? '')).join('\t'));
  return [g.columns.map((c) => quoteCell(heading(c))).join('\t'), ...lines].join('\n');
}

/** As the tab shows it, turned: a line a filter or parameter, its heading first, then its cells across the rows. */
export function copyTextTurned(g: Grid, rows: Set<string> | null): string {
  const picked = g.rows.filter((r) => !rows || rows.has(r.key));
  return g.columns.map((c) => [heading(c), ...picked.map((r) => r.cells[c.key] ?? '')].map(quoteCell).join('\t')).join('\n');
}

/** How a dataflow field adds up, as a choice, and back. */
export type AddUpChoice = 'sum' | 'volume' | 'mass' | 'max' | 'min';
export const ADD_UPS: readonly AddUpChoice[] = ['sum', 'volume', 'mass', 'max', 'min'];
export const ADD_UP_LABELS: Record<AddUpChoice, string> = { sum: 'summed', volume: 'by volume', mass: 'by tonnes', max: 'largest', min: 'smallest' };

/** Where a dataflow field's value comes from, as a choice. */
export type FromChoice = 'volume' | 'plan_area' | 'thickness' | 'attribute' | 'parameter' | 'worked';
export const FROMS: readonly FromChoice[] = ['volume', 'plan_area', 'thickness', 'attribute', 'parameter', 'worked'];
export const FROM_LABELS: Record<FromChoice, string> = {
  volume: 'solid volume',
  plan_area: 'plan area',
  thickness: 'volume ÷ plan area',
  attribute: 'attribute',
  parameter: 'parameter column',
  worked: 'worked by a step',
};

export const KINDS = ['volume', 'thickness', 'area', 'length', 'density', 'quality', 'moisture', 'moisture_factor', 'setting', 'other'] as const;

type FlowField = { name: string; kind: string; add_up: { by: string; density?: string }; from: string; attribute?: string; column?: string };

/** A field with where it comes from changed: an attribute or parameter column named after the field until set. */
export function withFrom<F extends FlowField>(f: F, from: FromChoice): F {
  const { attribute: _a, column: _c, ...rest } = f;
  if (from === 'attribute') return { ...rest, from, attribute: f.attribute ?? f.column ?? f.name } as F;
  if (from === 'parameter') return { ...rest, from, column: f.column ?? f.attribute ?? f.name } as F;
  return { ...rest, from } as F;
}

/** A field with how it adds up changed: by tonnes names a density field, the first of the dataflow's if it had none. */
export function withAddUp<F extends FlowField>(f: F, by: AddUpChoice, densities: string[]): F {
  if (by === 'mass') return { ...f, add_up: { by, density: f.add_up.density ?? densities[0] ?? 'rd' } };
  return { ...f, add_up: { by } };
}

/** An item of a list moved up (-1) or down (+1) one place. */
export function moved<T>(list: T[], i: number, by: -1 | 1): T[] {
  const j = i + by;
  if (j < 0 || j >= list.length) return list;
  const out = [...list];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}
