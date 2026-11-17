import type { ColumnField, ColumnStep, ColumnUnit, FormulaTrace, ParamRead } from '../bridge';
import { detailOf, stepActivity, unitWords, valueText } from './columnSteps.ts';

/**
 * The column notebook (decision 0081): one cell a step, each with what went in for the unit in focus, the formula traced
 * on it or the built-in step's rule, and what the step changed across the column. Laid out and worded here; the numbers,
 * the reads and the traces are the engine's.
 */

/** The unit in focus, by what it is as well as its id, so it can be followed through a step that joins or splits it. */
export interface Focus {
  id: number;
  horizon: string;
  seam: string;
  material: string;
  bench: string | null;
}

export const focusOf = (u: ColumnUnit): Focus => ({ id: u.id, horizon: u.horizon, seam: u.seam, material: u.material, bench: u.bench });

/** The picked seam or bench's top unit as the column comes in, else its first coal, else its first unit (decision 0081, 3). */
export function defaultFocus(steps: ColumnStep[], leaf: { seam?: string; bench?: string } | null): Focus | null {
  const units = steps[0]?.units ?? [];
  const coal = (u: ColumnUnit) => u.material.toLowerCase() === 'coal';
  const picked = leaf?.seam ? units.filter((u) => u.seam === leaf.seam && (!leaf.bench || u.bench === leaf.bench)) : [];
  const u = picked.find(coal) ?? picked[0] ?? units.find(coal) ?? units[0];
  return u ? focusOf(u) : null;
}

/**
 * The focus as it stands among these units: the same unit; else the joined unit holding its bench, the unit itself before
 * any piece a step cut from it (a loss, a dilution), which holds the same benches; else a unit of its horizon and
 * material that's still itself; else none.
 */
export function focusAt(units: ColumnUnit[], focus: Focus): ColumnUnit | null {
  const same = units.find((u) => u.id === focus.id);
  if (same) return same;
  const holding = units.filter((u) => u.horizon === focus.horizon && focus.bench !== null && u.joined.some(([b]) => b === focus.bench));
  const joined = holding.find((u) => u.became === null) ?? holding[0];
  if (joined) return joined;
  return units.find((u) => u.horizon === focus.horizon && u.material === focus.material && u.became === null) ?? null;
}

/** How much a step did: values changed, units made and taken away, and whether it did nothing (folded, decision 0081, 6). */
export interface CellSummary {
  changes: number;
  made: number;
  gone: number;
  quiet: boolean;
}

export function cellSummaries(steps: ColumnStep[]): CellSummary[] {
  return stepActivity(steps).map((a, i) => ({ ...a, quiet: i > 0 && a.changes + a.made + a.gone === 0 && steps[i].said.length === 0 }));
}

/** The words a summary reads as on a cell's head. */
export function summaryWords(s: CellSummary): string {
  if (s.quiet) return 'nothing changed in this column';
  const parts: string[] = [];
  if (s.changes) parts.push(`${s.changes} value${s.changes === 1 ? '' : 's'} changed`);
  if (s.made) parts.push(`${s.made} unit${s.made === 1 ? '' : 's'} made`);
  if (s.gone) parts.push(`${s.gone} taken away`);
  return parts.join(' · ') || 'said something, changed nothing';
}

export interface OutUnit {
  unit: ColumnUnit;
  label: string;
  detail: string;
  isNew: boolean;
}

export interface OutCell {
  value: number | null;
  /** Before the step; undefined for a unit the step made. */
  before: number | null | undefined;
  changed: boolean;
}

export interface OutRow {
  field: ColumnField;
  cells: OutCell[];
}

/** What a step caused (decision 0081, 5): only the units it touched and the fields it changed on them. */
export interface CellOut {
  units: OutUnit[];
  rows: OutRow[];
  gone: string[];
  volumeIn: number | null;
  volumeOut: number | null;
}

const SAME = 1e-9;
const differs = (a: number | null, b: number | null) => (a === null || b === null ? a !== b : Math.abs(a - b) > SAME * Math.max(1, Math.abs(b)));

export function cellOut(fields: ColumnField[], steps: ColumnStep[], at: number): CellOut {
  const step = steps[at];
  const before = at > 0 ? new Map(steps[at - 1].units.map((u) => [u.id, u])) : null;
  const value = (u: ColumnUnit, f: string) => u.values[f] ?? null;
  const touched = step.units.filter((u) => {
    if (!before) return false;
    const was = before.get(u.id);
    return !was || fields.some((f) => differs(value(u, f.name), value(was, f.name)));
  });
  const units: OutUnit[] = touched.map((u) => ({ unit: u, label: unitWords(u), detail: detailOf(u), isNew: !before?.has(u.id) }));
  const rows: OutRow[] = [];
  for (const field of fields) {
    const cells = touched.map((u): OutCell => {
      const was = before?.get(u.id);
      const v = value(u, field.name);
      if (!was) return { value: v, before: undefined, changed: false };
      const b = value(was, field.name);
      return { value: v, before: b, changed: differs(v, b) };
    });
    if (cells.some((c) => c.changed || (c.before === undefined && c.value !== null))) rows.push({ field, cells });
  }
  const here = new Set(step.units.map((u) => u.id));
  const gone = at > 0 ? steps[at - 1].units.filter((u) => !here.has(u.id)).map(unitWords) : [];
  const volume = fields.find((f) => f.kind === 'volume');
  const total = (i: number) => (volume ? steps[i].units.reduce((s, u) => s + (u.values[volume.name] ?? 0), 0) : null);
  return { units, rows, gone, volumeIn: at > 0 ? total(at - 1) : null, volumeOut: total(at) };
}

/** A built-in step's reads for one unit, in the order read. */
export const readsFor = (cell: ColumnStep, unit: number): ParamRead[] => cell.reads.filter((r) => r.unit === unit);

/**
 * A read in words: `roof_dilution 0.05 (assumptions, column 3)`, the column being the table's row as the Parameters tab
 * shows it, numbered from 1; `none (no row gives it)` where nothing did.
 */
export function readWords(r: ParamRead, tables: string[], unit = ''): { name: string; value: string; from: string } {
  const value = r.value !== null && r.text === null ? valueText(r.value, unit) || String(r.value) : (r.text ?? '');
  if (r.table === null || r.row === null) return { name: r.column, value: 'none', from: 'no row gives it' };
  return { name: r.column, value, from: `${tables[r.table] ?? `table ${r.table + 1}`}, column ${r.row + 1}` };
}

/** A value a formula gave or read, as words: a long decimal cut to 4 places, trailing zeros dropped; anything else as it is. */
export function shortValue(v: string): string {
  if (!/^-?\d+\.\d{5,}(e-?\d+)?$/i.test(v.trim())) return v;
  const n = Number(v);
  return Number.isFinite(n) ? String(Number(n.toFixed(4))) : v;
}

/** What a formula read, each name once, in the order first read, as its value then stood. */
export function readsOnce(reads: [string, string][]): [string, string][] {
  const seen = new Set<string>();
  return reads.filter(([what]) => (seen.has(what) ? false : (seen.add(what), true))).map(([what, v]) => [what, shortValue(v)]);
}

/** A formula's filter in words: `material coal · seam U, M`; every unit with none. */
export function whenWords(when: Record<string, string>): string {
  const parts = Object.entries(when).map(([k, v]) => `${k} ${v}`);
  return parts.length ? parts.join(' · ') : 'every unit';
}

/** A line of a formula as the cell shows it: whether it ran on the focus unit, how often, and the values its last run gave. */
export interface CodeLine {
  n: number;
  text: string;
  ran: number;
  values: [string, string][];
}

/** The formula's own lines, each marked by the trace: the lines a loop runs again show the values of their last run. */
export function codeLines(source: string, trace: FormulaTrace | null): CodeLine[] {
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  const ran = new Map<number, { count: number; values: [string, string][] }>();
  for (const l of trace?.lines ?? []) {
    const had = ran.get(l.line);
    ran.set(l.line, { count: (had?.count ?? 0) + 1, values: l.values.length ? l.values : (had?.values ?? []) });
  }
  return lines.map((text, i) => ({ n: i + 1, text, ran: ran.get(i + 1)?.count ?? 0, values: ran.get(i + 1)?.values ?? [] }));
}
