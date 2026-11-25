import type { CaseHeadline } from '@/bridge';
import { type Attributes, type When, whenFits } from '../filters.ts';

/**
 * A way in to a case (the Build Plan's "Reserves case plan", decision 13, workstream 7): New case, Copy case and Add a
 * dataflow made from the app, and each case's headline figures set against Base.
 */

/** A name no object has yet: the name asked, else with ` 2`, ` 3` … after it. */
export function freeName(name: string, taken: readonly string[]): string {
  const used = new Set(taken.map((t) => t.toLowerCase()));
  const base = name.trim();
  if (!used.has(base.toLowerCase())) return base;
  for (let n = 2; ; n++) if (!used.has(`${base} ${n}`.toLowerCase())) return `${base} ${n}`;
}

/** The solids a new case runs on: those on the layer whose attributes the filter fits, in the order given. */
export function blocksFitting(solids: readonly { id: string; layer: string }[], attributes: readonly Attributes[], layer: string, when: When): string[] {
  return solids.filter((s, i) => s.layer === layer && whenFits(when, attributes[i] ?? {})).map((s) => s.id);
}

/** A headline figure's change against Base: `+1.2 %`, `−0.4 %`; nothing for Base itself, or where either has none. */
export function againstBase(value: number | null, base: number | null, isBase: boolean): string {
  if (isBase || value === null || base === null || base === 0) return '';
  const d = (value - base) / Math.abs(base);
  if (Math.abs(d) < 0.0005) return '±0.0 %';
  return `${d > 0 ? '+' : '−'}${(Math.abs(d) * 100).toFixed(1)} %`;
}

/** The headline figures shown, in order: what, unit, how many decimals. */
export const HEADLINES: { key: keyof Pick<CaseHeadline, 'insitu_coal' | 'rom_coal' | 'rom_waste' | 'strip_ratio'>; label: string; unit: string; dp: number }[] = [
  { key: 'insitu_coal', label: 'In situ coal', unit: 't', dp: 0 },
  { key: 'rom_coal', label: 'ROM coal', unit: 't', dp: 0 },
  { key: 'rom_waste', label: 'ROM waste', unit: 'BCM', dp: 0 },
  { key: 'strip_ratio', label: 'Strip ratio', unit: 'BCM/t', dp: 2 },
];

/** A headline figure in words, or a dash where the table hasn't it. */
export const headlineWords = (v: number | null, dp: number) => (v === null ? '—' : v.toLocaleString('en-AU', { minimumFractionDigits: dp, maximumFractionDigits: dp }));

/**
 * A row dataflow to start from (decision 2): one field and one formula step that leaves it empty, to write over in the
 * formula panel; a row dataflow with no steps isn't saved.
 */
export const EMPTY_ROW_DATAFLOW = {
  rows: true,
  fields: [{ name: 'value', kind: 'other', add_up: { by: 'sum' }, from: 'worked' }],
  steps: [{ step: 'first_step', formula: { script: 'def formula(row, p):\n    row.value = None\n' } }],
};
