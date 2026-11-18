/**
 * The block table (Workflow plan, stage 4; decision 0069): solids as rows, each with its own volume, the volume of record,
 * and every attribute as a column. What's shown is filtered by words and by a volume under which to look, and added up:
 * how many, their volume and, by a density column, their tonnes. Kept free of React, so the tests can run it.
 */

import { type Attributes, type When, whenFits } from '../filters.ts';

export type AttrValue = boolean | number | string;

export interface SolidRow {
  id: string;
  name: string;
  layer: string;
  volume: number;
  attributes: Record<string, AttrValue>;
}

/** The attributes a cut writes, first and in this order; every other after them by name. */
const FIRST = ['strip', 'block', 'seam', 'horizon', 'bench', 'material', 'case'];

/** The attributes as columns: those any row has, the cut's own first. `numeric` when every value there is a number. */
export function attributeColumns(rows: readonly SolidRow[]): { key: string; numeric: boolean }[] {
  const seen = new Map<string, boolean>();
  for (const row of rows) {
    for (const [key, value] of Object.entries(row.attributes)) {
      seen.set(key, (seen.get(key) ?? true) && typeof value === 'number');
    }
  }
  const keys = [...seen.keys()].sort((a, b) => {
    const [i, j] = [FIRST.indexOf(a), FIRST.indexOf(b)];
    if (i >= 0 || j >= 0) return (i < 0 ? FIRST.length : i) - (j < 0 ? FIRST.length : j);
    return a.localeCompare(b);
  });
  return keys.map((key) => ({ key, numeric: seen.get(key)! }));
}

/** The numeric columns a density could be, the likeliest first: `rd`, then any named for density. */
export function densityColumns(columns: readonly { key: string; numeric: boolean }[]): string[] {
  const numeric = columns.filter((c) => c.numeric).map((c) => c.key);
  const likely = (k: string) => (k.toLowerCase() === 'rd' ? 0 : /(^|_)rd$|density|^sg$/i.test(k) ? 1 : 2);
  return numeric.filter((k) => likely(k) < 2).sort((a, b) => likely(a) - likely(b));
}

export interface Filter {
  /** Words every shown row has somewhere: its name, layer or an attribute's value. */
  words: string;
  /** Only rows under this volume (m³); 0 for any. */
  under: number;
  /** Only rows that fit this filter by attributes and, for a solid, its `volume` (decision 0088); none for any. */
  when?: When;
}

/** What a row's filter reads: its attributes and, for a solid, its volume, an attribute of that name winning. */
export const filterable = (row: SolidRow): Attributes => (Number.isFinite(row.volume) ? { volume: row.volume, ...row.attributes } : row.attributes);

/** The rows the filter keeps, in order. */
export function shownRows(rows: readonly SolidRow[], filter: Filter): SolidRow[] {
  const words = filter.words.toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter((row) => {
    if (filter.under > 0 && !(row.volume < filter.under)) return false;
    if (filter.when && !whenFits(filter.when, filterable(row))) return false;
    if (words.length === 0) return true;
    const text = [row.name, row.layer, ...Object.values(row.attributes).map(String)].join(' ').toLowerCase();
    return words.every((w) => text.includes(w));
  });
}

export interface Totals {
  count: number;
  volume: number;
  /** Volume × density over the rows that have one; null when none does or no density is picked. */
  tonnes: number | null;
  /** Rows with a density. */
  weighed: number;
}

/** The rows added up, tonnes by the density column picked. */
export function totalsOf(rows: readonly SolidRow[], density: string | null): Totals {
  let volume = 0;
  let tonnes = 0;
  let weighed = 0;
  for (const row of rows) {
    volume += row.volume;
    const rd = density ? row.attributes[density] : undefined;
    if (typeof rd === 'number' && Number.isFinite(rd)) {
      tonnes += row.volume * rd;
      weighed += 1;
    }
  }
  return { count: rows.length, volume, tonnes: weighed > 0 ? tonnes : null, weighed };
}

/** The rows as CSV: name, layer, volume and each attribute column, exact values, quoted where they need it. */
export function csvOf(rows: readonly SolidRow[], columns: readonly { key: string }[]): string {
  const cell = (v: unknown) => {
    const text = v === undefined || v === null ? '' : String(v);
    return /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
  };
  const head = ['name', 'layer', 'volume_m3', ...columns.map((c) => c.key)].map(cell).join(',');
  const lines = rows.map((r) => [r.name, r.layer, r.volume, ...columns.map((c) => r.attributes[c.key])].map(cell).join(','));
  return [head, ...lines].join('\r\n');
}
