import type { ColumnDid, ColumnStep, ColumnSteps, ParameterTable, StepTotals } from '@/bridge';

/**
 * The Reserves case tab's working parts (the Build Plan's "Reserves case plan", decisions 8, 10 and 11), kept free of
 * React so they can be tested: the steps by stage as the run puts them, the block map, the whole case at a step against
 * the last run, and a step's settings by seam and ply as its tables' rows are read.
 */

/** A stage of the dataflow: its name and its steps, by number (1 the first step after the blocks as read). */
export interface Stage {
  name: string;
  steps: number[];
}

/** Each built-in step's stage, in Spry's order. */
const STAGE_OF: Record<string, string> = {
  small_volumes: 'In situ',
  initialise: 'In situ',
  in_situ: 'In situ',
  join_benches: 'In situ',
  working_sections: 'Working sections',
  rom_loss: 'ROM',
  rom_dilution: 'ROM',
  rom_ash_cutoff: 'ROM',
  rom: 'ROM',
  final: 'Final',
};

/**
 * The steps by stage, in run order (decision 10): a built-in step in its own stage; a formula in the stage of the next
 * built-in step, as the starter puts the discard formula before `working_sections`, but after `rom` in ROM (the stage the
 * ROM formulas follow); an add-on's steps a stage of their own, named for it. `steps` is a column's, the blocks as read
 * first; `addOns` names an add-on by its id.
 */
export function stagesOf(steps: Pick<ColumnStep, 'step' | 'kind' | 'dataflow'>[], addOns: Record<string, string> = {}): Stage[] {
  const builtIn = (i: number) => steps[i].kind === 'built_in' && !steps[i].dataflow;
  const stageAt = (i: number): string => {
    const s = steps[i];
    if (s.dataflow) return `${addOns[s.dataflow] ?? 'Add-on'} (add-on)`;
    if (builtIn(i)) return STAGE_OF[s.step] ?? 'Steps';
    let before = i - 1;
    while (before > 0 && !builtIn(before)) before--;
    if (before > 0 && steps[before].step === 'rom') return 'ROM';
    for (let after = i + 1; after < steps.length; after++) if (builtIn(after)) return STAGE_OF[steps[after].step] ?? 'Steps';
    return before > 0 ? (STAGE_OF[steps[before].step] ?? 'Steps') : 'Steps';
  };
  const stages: Stage[] = [];
  for (let i = 1; i < steps.length; i++) {
    const name = stageAt(i);
    const last = stages[stages.length - 1];
    if (last && last.name === name) last.steps.push(i);
    else stages.push({ name, steps: [i] });
  }
  return stages;
}

/** Words in their natural order: strip 2 before strip 10. */
export const naturalCompare = (a: string, b: string) => a.localeCompare(b, 'en', { numeric: true });

/** A row of the block map: a strip (the column's words less its block) and its blocks in order. */
export interface MapRow {
  strip: string;
  blocks: ColumnDid[];
}

/** The block map (decision 8): a row a strip, its blocks along it, each in their natural order. */
export function blockMap(blocks: ColumnDid[]): MapRow[] {
  const rows = new Map<string, ColumnDid[]>();
  for (const b of blocks) {
    const strip = b.path.slice(0, -1).join(' ');
    rows.set(strip, [...(rows.get(strip) ?? []), b]);
  }
  return [...rows.entries()]
    .sort(([a], [b]) => naturalCompare(a, b))
    .map(([strip, row]) => ({ strip, blocks: row.sort((a, b) => naturalCompare(a.path[a.path.length - 1] ?? '', b.path[b.path.length - 1] ?? '')) }));
}

/** What the map shades by: the volume the step moved where it moved any, else what it wrote, else nothing (all one shade). */
export type MapMeasure = 'moved' | 'written' | 'none';

export function mapMeasure(blocks: ColumnDid[]): MapMeasure {
  if (blocks.some((b) => b.moved > 0)) return 'moved';
  if (blocks.some((b) => b.written > 0)) return 'written';
  return 'none';
}

/** A block's shade on the map, 0 to 1 by the measure, against the most any block has. */
export function shadeOf(b: ColumnDid, blocks: ColumnDid[], measure: MapMeasure): number {
  if (measure === 'none' || b.stopped) return 0;
  const most = Math.max(...blocks.map((x) => x[measure]));
  return most > 0 ? b[measure] / most : 0;
}

/** A row of the whole case at a step: what it is, its unit, and the case's figure at the last run and now (with edits). */
export interface CaseRow {
  key: string;
  label: string;
  unit: 'm3' | 't' | 'bcm';
  last: number | null;
  now: number | null;
}

/** The headline folders the whole case shows once a step has written them: coal in tonnes, waste in BCM. */
const HEADLINES: { folder: string; label: string; unit: 't' | 'bcm' }[] = [
  { folder: 'Insitu\\Coal', label: 'In situ coal', unit: 't' },
  { folder: 'ROM\\Coal', label: 'ROM coal', unit: 't' },
  { folder: 'ROM\\Waste', label: 'ROM waste', unit: 'bcm' },
];

const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/**
 * The whole case at the step against the last run (decision 8): what the step said it moved, by what, then the headline
 * folders written so far, coal in tonnes and waste in BCM. `now` is the case with the unsaved edits, where there are any.
 */
export function caseRows(saved: StepTotals, now: StepTotals | null): CaseRow[] {
  const said = new Map<string, CaseRow>();
  for (const [which, totals] of [['last', saved], ['now', now ?? saved]] as const) {
    for (const s of totals.said) {
      if (s.volume === null) continue;
      const row = said.get(s.what) ?? { key: `said:${s.what}`, label: sentence(s.what), unit: 'm3' as const, last: null, now: null };
      row[which] = s.volume;
      said.set(s.what, row);
    }
  }
  const written = (t: StepTotals, folder: string, unit: 't' | 'bcm') => {
    const g = t.written.find((w) => w.group === folder);
    return g ? (unit === 't' ? g.tonnes : (g.values.volume ?? null)) : null;
  };
  const headlines = HEADLINES.map(({ folder, label, unit }) => ({ key: `written:${folder}`, label, unit, last: written(saved, folder, unit), now: written(now ?? saved, folder, unit) })).filter(
    (r) => r.last !== null || r.now !== null,
  );
  return [...said.values(), ...headlines];
}

/** A figure's change from the last run, in its own unit and as a share where the last run had one. */
export function changeWords(row: CaseRow): string {
  if (row.last === null || row.now === null) return row.now === null ? 'gone' : 'new';
  const d = row.now - row.last;
  if (Math.abs(d) <= 1e-9 * Math.max(1, Math.abs(row.last))) return '—';
  const figure = `${d > 0 ? '+' : '−'}${Math.round(Math.abs(d)).toLocaleString('en-AU')}`;
  const share = row.last !== 0 ? ` (${d > 0 ? '+' : '−'}${((Math.abs(d) / Math.abs(row.last)) * 100).toFixed(1)} %)` : '';
  return figure + share;
}

/** The settings a script reads through `p`: `p.name` and `getattr(p, "name")`, each once, in order. */
export function scriptSettings(source: string): string[] {
  const names: string[] = [];
  const add = (n: string) => !names.includes(n) && names.push(n);
  for (const line of source.split('\n').map((l) => l.split('#')[0])) {
    // As the engine reads them (`zd_core::parameters_read_in`): `p.dip` and a unit's `u.p.dip` alike.
    for (const m of line.matchAll(/(?<![A-Za-z0-9_])p\.([A-Za-z_][A-Za-z0-9_]*)/g)) add(m[1]);
    for (const m of line.matchAll(/getattr\(\s*p\s*,\s*["']([^"']+)["']/g)) add(m[1]);
  }
  return names;
}

/**
 * The settings a step reads, by name, in the order read: a built-in or allocation step's as it read them for this block,
 * a formula's off its source.
 */
export function stepSettings(step: ColumnStep): string[] {
  if (step.kind === 'formula') return step.source ? scriptSettings(step.source) : [];
  const names: string[] = [];
  for (const r of step.reads) if (!names.includes(r.column)) names.push(r.column);
  if (step.kind === 'allocation' && step.source) for (const n of scriptSettings(step.source)) if (!names.includes(n)) names.push(n);
  return names;
}

/** A row of a step's settings by seam and ply: its table (its place in the run's list), row, what it fits, and its values. */
export interface SettingsRow {
  table: number;
  tableId: string | null;
  tableName: string;
  /** The row in its table, from 0, as saved or as edited. */
  row: number;
  fits: string;
  values: Record<string, number | string | null>;
}

/** A step's settings by seam and ply (decision 11): the columns it reads, and its tables' rows in reading order. */
export interface SettingsGrid {
  columns: string[];
  rows: SettingsRow[];
}

/** A row's filters in words: `material coal · seam M`; none, every block. */
export const fitsWords = (when: Record<string, string> | undefined) =>
  when && Object.keys(when).length
    ? Object.entries(when)
        .map(([k, v]) => `${k} ${v}`)
        .join(' · ')
    : 'every block';

/** A row's value in a column, its name as written, else case aside; null where it holds none. */
export function valueIn(values: Record<string, number | string>, column: string): number | string | null {
  if (column in values) return values[column];
  const key = Object.keys(values).find((k) => k.toLowerCase() === column.toLowerCase());
  return key === undefined ? null : values[key];
}

/**
 * The step's settings as its tables' rows are read (decision 11, 0072's lookup unchanged): the columns it reads, then
 * every row of each table that has any of them, in the order read, the case's own rows first as the run lists them.
 * `tables` holds each of the run's tables (as `column.tables` lists them), as saved or as edited; null where it can't be
 * read (the run's own rows, a table gone).
 */
export function settingsGrid(column: ColumnSteps, at: number, tables: (ParameterTable | null)[]): SettingsGrid {
  return gridOf(stepSettings(column.steps[at]), column.tables, column.table_ids ?? [], tables);
}

/**
 * Settings `columns` as tables' rows are read: every row of each table (named `names`, ids `ids`, as saved or edited in
 * `tables`) that has any of them, in the order read. A row dataflow's step's settings are its script's (workstream 6).
 */
export function gridOf(columns: string[], names: string[], ids: (string | null)[], tables: (ParameterTable | null)[], every = 'every block'): SettingsGrid {
  const rows: SettingsRow[] = [];
  names.forEach((name, t) => {
    const table = tables[t];
    if (!table) return;
    const has = (c: string) => table.columns.some((x) => x.name.toLowerCase() === c.toLowerCase());
    if (!columns.some(has)) return;
    table.rows.forEach((r, row) => {
      const fits = fitsWords(r.when);
      rows.push({ table: t, tableId: ids[t] ?? null, tableName: name, row, fits: fits === 'every block' ? every : fits, values: Object.fromEntries(columns.map((c) => [c, valueIn(r.values, c)])) });
    });
  });
  return { columns, rows };
}

/** Where each setting came from for a unit, by setting: the table (its place in the run's list) and row that gave it. */
export function suppliedFor(step: ColumnStep, unit: number | null): Map<string, { table: number; row: number }> {
  const out = new Map<string, { table: number; row: number }>();
  if (unit === null) return out;
  for (const r of step.reads) if (r.unit === unit && r.table !== null && r.row !== null && !out.has(r.column)) out.set(r.column, { table: r.table, row: r.row });
  return out;
}

/**
 * A table with one cell typed (decision 11): a number where the column holds numbers, words where it holds words;
 * cleared (`''`), the row passes it on to the rows below. A column the table hasn't is added, holding numbers.
 */
export function withCell(table: ParameterTable, row: number, column: string, text: string): ParameterTable {
  const existing = table.columns.find((c) => c.name.toLowerCase() === column.toLowerCase());
  const name = existing?.name ?? column;
  const columns = existing ? table.columns : [...table.columns, { name }];
  const rows = table.rows.map((r, i) => {
    if (i !== row) return r;
    const values = { ...r.values };
    for (const k of Object.keys(values)) if (k.toLowerCase() === column.toLowerCase()) delete values[k];
    const t = text.trim();
    if (t !== '') {
      const n = Number(t);
      values[name] = existing?.text || !Number.isFinite(n) ? t : n;
    }
    return { ...r, values };
  });
  return { ...table, columns, rows };
}

/**
 * How the case tab's working area sits at a dock's width (Case tab layout plan, decision 8): step, units table and block
 * map side by side from 1,400 px; under it the block map goes under the step; under 1,000 px the units table goes under
 * both, and the header's figures fold into the case pill's tip.
 */
export type CaseFold = 'three' | 'two' | 'one';
export function caseFold(width: number): CaseFold {
  return width >= 1400 ? 'three' : width >= 1000 ? 'two' : 'one';
}

/**
 * The block map's cells grow to fit (decision 6): a column a block of the longest strip, so a strip's blocks line up
 * under the next strip's; past 40 blocks a strip the cells stay small and wrap. Null: wrap.
 */
export function mapColumns(rows: MapRow[]): number | null {
  const most = rows.reduce((m, r) => Math.max(m, r.blocks.length), 0);
  return most > 0 && most <= 40 ? most : null;
}

/**
 * A list of names longer than this folds to a count, opened on a click (Case setup and unit plan, decision 4), so a
 * table read for 53 settings is one line until wanted.
 */
export const FOLD_NAMES = 12;
export function foldsNames(count: number): boolean {
  return count > FOLD_NAMES;
}
/** A count of things in words: "1 setting", "53 settings". */
export function countWords(count: number, noun: string): string {
  return `${count.toLocaleString('en-AU')} ${noun}${count === 1 ? '' : 's'}`;
}
