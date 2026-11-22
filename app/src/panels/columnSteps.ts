import type { ColumnField, ColumnStep, ColumnUnit } from '../bridge';
import { decimals, fieldLabel, isDetail } from './reservesTable.ts';

/**
 * Open its column (decision 0075, 5b): one block's column after a step of its dataflow, the fields down the side and the
 * units across top of the stack to the bottom, each value set beside the one it had before the step. Units are matched
 * across the step by their id, which the engine keeps; a unit with an id the step before didn't have is new (a join, a
 * piece split off, the underburden), and one the step left behind is gone. Laid out and worded here; the numbers are
 * the engine's.
 */

/** A value, relative, that counts as changed: rounding in the steps' add-ups is not a change. */
const SAME = 1e-9;

/** A unit across the grid: its words, whether the step made it, and whether it's of the seam or bench picked. */
export interface UnitHead {
  unit: ColumnUnit;
  label: string;
  /** What it is past its horizon: its material, what it became, its working section or where it went in ROM. */
  detail: string;
  isNew: boolean;
  picked: boolean;
}

export interface StepCell {
  value: number | null;
  /** What it was before the step; undefined for a unit the step made. */
  before: number | null | undefined;
  changed: boolean;
}

export interface StepRow {
  field: ColumnField;
  label: string;
  cells: StepCell[];
  changed: boolean;
}

export interface StepGrid {
  units: UnitHead[];
  rows: StepRow[];
  /** Units the step took away, by their words. */
  gone: string[];
  /** How many values the step changed, over every unit. */
  changes: number;
}

/** The node's words below the column, to mark: its seam and, picked down to one, its bench. */
export interface PickedLeaf {
  seam?: string;
  bench?: string;
}

/** A unit's words: its horizon and bench, a joined unit its horizon with how many benches. */
export function unitWords(u: ColumnUnit): string {
  if (u.joined.length > 0) return `${u.horizon} (${u.joined.length} benches)`;
  return u.bench ? `${u.horizon} ${u.bench}` : u.horizon;
}

/** What a unit is past its horizon: its material, what it became, its working section and where it goes in ROM. */
export function detailOf(u: ColumnUnit): string {
  const parts = [u.material];
  if (u.became) parts.push(u.became.replace(/_/g, ' '));
  if (u.section) parts.push(`section ${u.section}`);
  if (u.rom) parts.push(`ROM ${u.rom}`);
  for (const [field, word] of Object.entries(u.words ?? {})) parts.push(`${field} ${word}`);
  return parts.join(' · ');
}

function isPicked(u: ColumnUnit, leaf: PickedLeaf | null): boolean {
  if (!leaf?.seam || u.seam !== leaf.seam) return false;
  if (!leaf.bench) return true;
  return u.bench === leaf.bench || u.joined.some(([b]) => b === leaf.bench);
}

function differs(a: number | null, b: number | null): boolean {
  if (a === null || b === null) return a !== b;
  return Math.abs(a - b) > SAME * Math.max(1, Math.abs(b));
}

/**
 * The grid for step `at` (0, the column as input, has nothing before it): every field, or the main ones, or only the
 * fields the step changed on some unit.
 */
export function stepGrid(fields: ColumnField[], steps: ColumnStep[], at: number, show: 'main' | 'all' | 'changed', leaf: PickedLeaf | null): StepGrid {
  const step = steps[at];
  const before = at > 0 ? new Map(steps[at - 1].units.map((u) => [u.id, u])) : null;
  const units: UnitHead[] = step.units.map((u) => ({ unit: u, label: unitWords(u), detail: detailOf(u), isNew: before !== null && !before.has(u.id), picked: isPicked(u, leaf) }));
  const here = new Set(step.units.map((u) => u.id));
  const gone = at > 0 ? steps[at - 1].units.filter((u) => !here.has(u.id)).map(unitWords) : [];
  let changes = 0;
  const rows: StepRow[] = [];
  for (const field of fields) {
    const cells = step.units.map((u): StepCell => {
      const value = u.values[field.name] ?? null;
      if (!before) return { value, before: value, changed: false };
      const was = before.get(u.id);
      if (!was) return { value, before: undefined, changed: false };
      const prior = was.values[field.name] ?? null;
      return { value, before: prior, changed: differs(value, prior) };
    });
    const changed = cells.some((c) => c.changed || (c.before === undefined && c.value !== null));
    changes += cells.filter((c) => c.changed).length;
    if (show === 'changed' && !changed) continue;
    if (show === 'main' && isDetail(field.name) && !changed) continue;
    rows.push({ field, label: fieldLabel(field.name), cells, changed });
  }
  return { units, rows, gone, changes };
}

/** How many values each step changed and how many units it made or took away: to mark the steps that did something. */
export function stepActivity(steps: ColumnStep[]): { changes: number; made: number; gone: number }[] {
  return steps.map((step, at) => {
    if (at === 0) return { changes: 0, made: 0, gone: 0 };
    const before = new Map(steps[at - 1].units.map((u) => [u.id, u]));
    const here = new Set(step.units.map((u) => u.id));
    let changes = 0;
    for (const u of step.units) {
      const was = before.get(u.id);
      if (!was) continue;
      for (const [k, v] of Object.entries(u.values)) if (differs(v ?? null, was.values[k] ?? null)) changes++;
    }
    return { changes, made: step.units.filter((u) => !before.has(u.id)).length, gone: steps[at - 1].units.filter((u) => !here.has(u.id)).length };
  });
}

/** A value as shown, by its unit's decimals; blank for none. */
export function valueText(value: number | null | undefined, unit: string): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '';
  return new Intl.NumberFormat('en-AU', { minimumFractionDigits: decimals(unit), maximumFractionDigits: decimals(unit) }).format(value);
}

/** A step's name read out: `rom_dilution` is "rom dilution", the column as input as it is. */
export function stepLabel(step: string): string {
  return step.replace(/_/g, ' ');
}
