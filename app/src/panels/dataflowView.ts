import type { ColumnStep, ColumnSteps, StepWrite } from '../bridge';

/**
 * The Dataflow view's strip (decision 0084, layout B): the column's inputs, then its steps in run order, each with what it
 * wrote to the reserves table under it and the units it passed on. Worded here; the numbers are the engine's.
 */

/** What a step wrote under one top folder (`Dilution`, from `Dilution\Roof` and `Dilution\Floor`), its volumes added up. */
export interface StripWrite {
  folder: string;
  volume: number;
  parts: StepWrite[];
}

/** A step as the strip shows it. */
export interface StripStep {
  at: number;
  name: string;
  kind: ColumnStep['kind'];
  /** It changed nothing in this column: drawn quieter. */
  quiet: boolean;
  /** The units it passed on. */
  units: number;
  writes: StripWrite[];
}

const top = (folder: string) => folder.split('\\')[0];

/** A step's writes under their top folders, in the order first written. */
export function writesByFolder(writes: StepWrite[]): StripWrite[] {
  const out: StripWrite[] = [];
  for (const w of writes) {
    const name = top(w.folder);
    const had = out.find((o) => o.folder === name);
    if (had) {
      had.volume += w.volume;
      had.parts.push(w);
    } else out.push({ folder: name, volume: w.volume, parts: [w] });
  }
  return out;
}

export function stripSteps(steps: ColumnStep[], quiet: boolean[]): StripStep[] {
  return steps.map((s, at) => ({
    at,
    name: at === 0 ? 'Imported' : s.step,
    kind: s.kind,
    quiet: quiet[at] ?? false,
    units: s.units.length,
    writes: writesByFolder(s.writes ?? []),
  }));
}

/** A volume short enough for a tag: 412.3k, 1.1k, 350, −19.5k; m³ is understood. */
export function volumeShort(v: number): string {
  const a = Math.abs(v);
  const sign = v < 0 ? '−' : '';
  if (a >= 1e6) return `${sign}${(a / 1e6).toFixed(1)}M`;
  if (a >= 1e3) return `${sign}${(a / 1e3).toFixed(1)}k`;
  return `${sign}${a >= 10 ? Math.round(a) : Number(a.toFixed(1))}`;
}

/** A write's folders in full, for its tip: `Dilution\Roof 500.0 m³ (2 units)`. */
export function writeTip(w: StripWrite): string {
  return w.parts.map((p) => `${p.folder} ${p.volume.toLocaleString('en-AU', { minimumFractionDigits: 1, maximumFractionDigits: 1 })} m³ (${p.units.length} unit${p.units.length === 1 ? '' : 's'})`).join('\n');
}

/** An input of the column, and whether the picked step read it. */
export interface InputChip {
  key: string;
  kind: 'blocks' | 'model' | 'table';
  label: string;
  title: string;
  lit: boolean;
  /** For a table, the rows the picked step read, numbered as the Parameters tab shows them (its columns, from 1). */
  rows: number[];
}

/**
 * The column's inputs: its block solids, the seam model or the run's own stack, and each parameter table. What the
 * picked step read is lit: the blocks and the stack when the column comes in, a table and the rows taken from it by a
 * built-in step (for `unit`, the unit followed, where it read any; else for every unit). A formula's reads are in its
 * trace, by name, and light nothing here.
 */
export function inputsOf(column: ColumnSteps, picked: number | null, unit: number | null): InputChip[] {
  const asInput = picked === 0;
  const blocks = column.blocks ?? [];
  const chips: InputChip[] = [
    {
      key: 'blocks',
      kind: 'blocks',
      label: `${blocks.length} block solid${blocks.length === 1 ? '' : 's'}`,
      title: `The column as input, a unit a solid: ${blocks.join(', ')}`,
      lit: asInput,
      rows: [],
    },
    {
      key: 'model',
      kind: 'model',
      label: column.model ?? `Stack of ${column.horizons} horizons`,
      title: column.model ? `The seam model whose stack orders the column, ${column.horizons} horizons` : `The run's own stack of ${column.horizons} horizons, no seam model: it orders the column`,
      lit: asInput,
      rows: [],
    },
  ];
  const step = picked !== null ? column.steps[picked] : undefined;
  const reads = step && step.kind === 'built_in' ? step.reads : [];
  const own = unit !== null ? reads.filter((r) => r.unit === unit) : [];
  const counted = own.some((r) => r.table !== null) ? own : reads;
  column.tables.forEach((name, i) => {
    const rows = [...new Set(counted.filter((r) => r.table === i && r.row !== null).map((r) => r.row! + 1))].sort((a, b) => a - b);
    const lit = rows.length > 0;
    chips.push({
      key: `table:${i}`,
      kind: 'table',
      label: lit ? `${name} · column ${rows.join(', ')}` : name,
      title: lit ? `${step!.step} read ${name}, column ${rows.join(', ')} as the Parameters tab shows it` : `Parameter table ${name}`,
      lit,
      rows,
    });
  });
  return chips;
}

/** The tables a step read for any unit, by their place in `tables`: what the graph wires to it. */
export function tablesRead(step: ColumnStep): number[] {
  if (step.kind !== 'built_in') return [];
  return [...new Set(step.reads.filter((r) => r.table !== null).map((r) => r.table!))].sort((a, b) => a - b);
}
