import type { ColumnField, ColumnStep, ColumnUnit, FieldSet } from '@/bridge';
import { unitWords } from './columnSteps.ts';

/**
 * The Reserves case tab's units table (the Build Plan's "Reserves case plan", decision 12), kept free of React so it can
 * be tested: a block's units before and after a step as pieces of ground, grouped with subtotals by each field's own
 * rule, a piece that leaves its group listed there too; the field sets; the picked piece's journey and the last step to
 * write each field; and the field map, steps by fields.
 */

/** A unit's ground: its horizon and bench, which a split's pieces keep and a join brings together. */
export const groundOf = (u: ColumnUnit) => `${u.horizon}|${u.bench ?? ''}`;

/**
 * Whether a unit after the step came from one before it: the same ground (a piece split off keeps its unit's horizon
 * and bench, a joined unit's pieces their joins), or a joined unit from one of its benches.
 */
const cameFrom = (a: ColumnUnit, b: ColumnUnit) => groundOf(a) === groundOf(b) || (a.horizon === b.horizon && b.bench !== null && a.joined.some(([bench]) => bench === b.bench));

/** How a piece came through the step: as it was (perhaps less), split off, joined from benches, new, or gone. */
export type PieceHow = 'kept' | 'split' | 'joined' | 'new' | 'gone';

/**
 * A row of the units table: a piece of ground before and after the step (decision 12), the before and after boundaries
 * together. A unit that kept its id is one row, before and after; a piece the step split off, or a unit it joined from
 * benches, a row of its own under the ground it came from; a unit it took away or joined into another, a row with no
 * after.
 */
export interface PieceRow {
  key: string;
  before: ColumnUnit | null;
  after: ColumnUnit | null;
  how: PieceHow;
  /** The unit before the step it came from or went into, where it isn't itself. */
  other: ColumnUnit | null;
}

/** The pieces of a block before and after a step, under the ground each came from, in stack order. */
export function piecesOf(before: ColumnUnit[], after: ColumnUnit[]): PieceRow[] {
  const ids = new Set(before.map((u) => u.id));
  const placed = new Set<number>();
  const rows: PieceRow[] = [];
  for (const b of before) {
    const kept = after.find((a) => a.id === b.id);
    if (kept) {
      rows.push({ key: `a:${kept.id}`, before: b, after: kept, how: 'kept', other: null });
      placed.add(kept.id);
    }
    for (const a of after) {
      if (placed.has(a.id) || ids.has(a.id) || !cameFrom(a, b)) continue;
      rows.push({ key: `a:${a.id}`, before: null, after: a, how: groundOf(a) === groundOf(b) ? 'split' : 'joined', other: b });
      placed.add(a.id);
    }
    if (!kept) rows.push({ key: `b:${b.id}`, before: b, after: null, how: 'gone', other: after.find((a) => cameFrom(a, b)) ?? null });
  }
  for (const a of after) if (!placed.has(a.id)) rows.push({ key: `a:${a.id}`, before: null, after: a, how: 'new', other: null });
  return rows;
}

/** What the units table groups by: a unit's working section (where it goes in ROM once sent), seam, horizon, material, bench, a word field, or nothing. */
export type GroupBy = 'section' | 'seam' | 'horizon' | 'material' | 'bench' | 'none' | `word:${string}`;

export const GROUP_WORDS: Record<string, string> = { section: 'Working section', seam: 'Seam', horizon: 'Horizon', material: 'Material', bench: 'Bench', none: 'No groups' };

/** A unit's group by `by`. */
export function groupName(u: ColumnUnit, by: GroupBy): string {
  switch (by) {
    case 'section':
      return u.rom ?? u.section ?? 'no section';
    case 'seam':
      return u.seam || 'no seam';
    case 'horizon':
      return u.horizon;
    case 'material':
      return u.material;
    case 'bench':
      return u.bench ?? (u.joined.length ? 'benches joined' : 'no bench');
    case 'none':
      return 'Every unit';
    default:
      return u.words?.[by.slice('word:'.length)] ?? 'none';
  }
}

/**
 * Units added up by each field's own rule, as the engine adds them (`add_up_values`): a sum adds, a volume or mass
 * average weighs by volume or volume × density leaving out what has no weight, the largest and smallest are of all. A
 * value missing among those that weigh gives none. A field with no rule given adds.
 */
export function addUp(fields: ColumnField[], units: ColumnUnit[]): Record<string, number | null> {
  const volume = fields.find((f) => f.kind === 'volume')?.name ?? 'volume';
  const get = (u: ColumnUnit, name: string) => u.values[name] ?? Number.NaN;
  const out: Record<string, number | null> = {};
  for (const f of fields) {
    const rule = f.add_up ?? { by: 'sum' as const };
    let v: number;
    if (rule.by === 'sum') v = units.reduce((s, u) => s + get(u, f.name), 0);
    else if (rule.by === 'volume' || rule.by === 'mass') {
      let sum = 0;
      let total = 0;
      for (const u of units) {
        const w = rule.by === 'mass' ? get(u, volume) * get(u, rule.density) : get(u, volume);
        if (w !== 0) {
          sum += w * get(u, f.name);
          total += w;
        }
      }
      v = total === 0 ? Number.NaN : sum / total;
    } else if (rule.by === 'max' || rule.by === 'min') {
      v = units.length ? units.map((u) => get(u, f.name)).reduce((a, b) => (Number.isNaN(a) || Number.isNaN(b) ? Number.NaN : rule.by === 'max' ? Math.max(a, b) : Math.min(a, b))) : Number.NaN;
    } else v = Number.NaN;
    out[f.name] = Number.isFinite(v) ? v : null;
  }
  return out;
}

/** A row in a group: the piece, and whether it's listed here because it left (red) or is the piece the group holds now. */
export interface GroupRow {
  row: PieceRow;
  left: boolean;
}

/** A group of the units table: its rows, and its units before and after the step added up. */
export interface UnitGroup {
  name: string;
  rows: GroupRow[];
  before: Record<string, number | null>;
  after: Record<string, number | null>;
  /** How many units it held before and holds after. */
  counts: [number, number];
}

/**
 * The pieces in groups (decision 12): a piece in the group it's in after the step; one that left a group (or went, or was
 * split off from a unit in it and went elsewhere) listed in the group it left as well, in red. Subtotals add up each
 * group's units before and after by each field's own rule, so the groups' together are the block's.
 */
export function unitGroups(rows: PieceRow[], by: GroupBy, fields: ColumnField[]): UnitGroup[] {
  const order: string[] = [];
  const at = new Map<string, { rows: GroupRow[]; before: ColumnUnit[]; after: ColumnUnit[] }>();
  const group = (name: string) => {
    if (!at.has(name)) {
      order.push(name);
      at.set(name, { rows: [], before: [], after: [] });
    }
    return at.get(name)!;
  };
  for (const row of rows) {
    const was = row.before ?? (row.how === 'split' ? row.other : null);
    const from = was ? groupName(was, by) : null;
    const to = row.after ? groupName(row.after, by) : null;
    if (from !== null && from !== to) group(from).rows.push({ row, left: true });
    if (to !== null) group(to).rows.push({ row, left: false });
    if (row.before) group(groupName(row.before, by)).before.push(row.before);
    if (row.after) group(to!).after.push(row.after);
  }
  return order.map((name) => {
    const g = at.get(name)!;
    return { name, rows: g.rows, before: addUp(fields, g.before), after: addUp(fields, g.after), counts: [g.before.length, g.after.length] };
  });
}

/** The table's own field sets (decision 12), then a planner's. */
export const OWN_SETS = ['This step', 'Quantities', 'Qualities', 'Class and audit', 'All'] as const;

const QUANTITIES = ['volume', 'thickness', 'area', 'length', 'density'];
const QUALITIES = ['quality', 'moisture', 'moisture_factor'];

/** Whether a value moved: beyond rounding, relative. */
const moved = (a: number | null | undefined, b: number | null | undefined) =>
  (a === null || a === undefined) !== (b === null || b === undefined) || (a != null && b != null && Math.abs(a - b) > 1e-9 * Math.max(1, Math.abs(a)));

/** The fields the step changed on the block: any value moved on a unit that kept its id; the volume where it split or took a unit. */
export function changedFields(rows: PieceRow[], fields: ColumnField[]): string[] {
  const out = new Set<string>();
  const volume = fields.find((f) => f.kind === 'volume')?.name;
  for (const r of rows) {
    if (r.how !== 'kept') {
      if (volume) out.add(volume);
      continue;
    }
    for (const f of fields) if (moved(r.before!.values[f.name], r.after!.values[f.name])) out.add(f.name);
  }
  return fields.map((f) => f.name).filter((n) => out.has(n));
}

/**
 * The fields a set shows, in the dataflow's order (a planner's in theirs): This step, what the step reads and changes
 * with the thickness and volume (the default); Quantities; Qualities; Class and audit, the settings and marks carried on
 * the units; All; or a planner's set by name, the fields it names the dataflow has.
 */
export function fieldsOfSet(set: string, fields: ColumnField[], step: ColumnStep | null, rows: PieceRow[], own: FieldSet[] = []): string[] {
  const names = fields.map((f) => f.name);
  const of = (kinds: string[]) => fields.filter((f) => kinds.includes(f.kind)).map((f) => f.name);
  switch (set) {
    case 'All':
      return names;
    case 'Quantities':
      return of(QUANTITIES);
    case 'Qualities':
      return of(QUALITIES);
    case 'Class and audit':
      return fields.filter((f) => !QUANTITIES.includes(f.kind) && !QUALITIES.includes(f.kind)).map((f) => f.name);
    case 'This step': {
      const wanted = new Set([...of(['volume', 'thickness']), ...(step?.fields_read ?? []), ...changedFields(rows, fields)]);
      return names.filter((n) => wanted.has(n));
    }
    default: {
      const mine = own.find((s) => s.name === set);
      return mine ? mine.fields.filter((f) => names.includes(f)) : [];
    }
  }
}

/** Whether a row changed at the step: split, joined, new or gone, or what it is, where it goes or any shown field moved. */
export function rowChanged(r: PieceRow, shown: string[]): boolean {
  if (r.how !== 'kept') return true;
  const [b, a] = [r.before!, r.after!];
  return b.became !== a.became || b.rom !== a.rom || b.section !== a.section || shown.some((f) => moved(b.values[f], a.values[f]));
}

/** A step in a piece's journey: its number and name, and what it did to the piece. */
export interface JourneyStep {
  step: number;
  name: string;
  what: string;
}

const figure = (v: number | null | undefined) => (v == null ? '—' : v.toLocaleString('en-AU', { maximumFractionDigits: Math.abs(v) >= 100 ? 0 : 3 }));

/**
 * A unit's journey through the steps up to `at` (decision 12): each step that made it, changed its volume, what it is or
 * where it goes, or took it away, in words.
 */
export function journeyOf(steps: ColumnStep[], at: number, unit: number, volume = 'volume'): JourneyStep[] {
  const out: JourneyStep[] = [];
  for (let s = 0; s <= at && s < steps.length; s++) {
    const now = steps[s].units.find((u) => u.id === unit);
    const was = s > 0 ? steps[s - 1].units.find((u) => u.id === unit) : undefined;
    const name = s === 0 ? 'as input' : steps[s].step;
    if (now && !was) {
      out.push({ step: s, name, what: s === 0 ? `read from the block, ${figure(now.values[volume])} m³` : `made here, ${figure(now.values[volume])} m³${now.became ? `, ${now.became}` : ''}` });
      continue;
    }
    if (!now && was) {
      out.push({ step: s, name, what: 'taken away or joined into another' });
      break;
    }
    if (!now || !was) continue;
    const parts: string[] = [];
    if (moved(was.values[volume], now.values[volume])) parts.push(`${figure(was.values[volume])} → ${figure(now.values[volume])} m³`);
    if (was.became !== now.became) parts.push(`became ${now.became ?? 'itself again'}`);
    if (was.section !== now.section && now.section) parts.push(`in section ${now.section}`);
    if (was.rom !== now.rom && now.rom) parts.push(`to ${now.rom}`);
    if (parts.length) out.push({ step: s, name, what: parts.join(', ') });
  }
  return out;
}

/** The last step up to `at` that wrote a unit's field: the step that made it, or the last that moved its value; none, as read. */
export function lastWriter(steps: ColumnStep[], at: number, unit: number, field: string): string | null {
  return lastWriterAt(steps, at, unit, field)?.name ?? null;
}

/** The last step at or before `at` to write a field on a unit, by number and name; null for a value as read. */
export function lastWriterAt(steps: ColumnStep[], at: number, unit: number, field: string): { step: number; name: string } | null {
  for (let s = Math.min(at, steps.length - 1); s > 0; s--) {
    const now = steps[s].units.find((u) => u.id === unit);
    const was = steps[s - 1].units.find((u) => u.id === unit);
    if (now && (!was || moved(was.values[field], now.values[field]))) return { step: s, name: steps[s].step };
  }
  return null;
}

/** A row of the field map: a step, and the fields it reads and writes on this block. */
export interface MapStep {
  step: number;
  name: string;
  reads: Set<string>;
  writes: Set<string>;
}

/**
 * The field map (decision 12): every step, the fields it reads (as declared beside a built-in step, read off a formula's
 * source) and the fields it wrote on this block, moved on a unit that kept its id; a unit it made writes its volume.
 */
export function fieldMap(steps: ColumnStep[], fields: ColumnField[]): MapStep[] {
  const volume = fields.find((f) => f.kind === 'volume')?.name ?? 'volume';
  return steps.slice(1).map((step, i) => {
    const before = new Map(steps[i].units.map((u) => [u.id, u]));
    const writes = new Set<string>();
    for (const u of step.units) {
      const b = before.get(u.id);
      if (!b) {
        writes.add(volume);
        continue;
      }
      for (const f of fields) if (moved(b.values[f.name], u.values[f.name])) writes.add(f.name);
    }
    return { step: i + 1, name: step.step, reads: new Set(step.fields_read), writes };
  });
}

/** A row's name: the unit's words, and how it came through. */
export function rowWords(r: PieceRow): string {
  const u = r.after ?? r.before!;
  const how = { kept: '', split: ` · split off ${r.other ? unitWords(r.other) : ''}`, joined: ' · joined', new: ' · new', gone: r.other ? ` · into ${unitWords(r.other)}` : ' · taken away' }[r.how];
  return `${unitWords(u)}${u.became ? `, ${u.became}` : ''}${how}`;
}

/**
 * A field named as a path (`Waste\Topsoil\Prime Volume`): its heading (Waste › Topsoil) and its short name (Prime
 * Volume), so the picked unit lists it under its heading without wrapping (Case setup and unit plan, decision 7). A
 * plain name has no heading.
 */
export function fieldPath(name: string): { path: string | null; short: string } {
  const parts = name.split('\\').filter((p) => p.length > 0);
  if (parts.length < 2) return { path: null, short: name };
  return { path: parts.slice(0, -1).join(' › '), short: parts[parts.length - 1] };
}

/** A field of the picked unit, as its sheet shows it. */
export interface PieceField {
  name: string;
  short: string;
  unit: string;
  before: number | null;
  after: number | null;
  /** Moved by this step on a unit that was there before it. */
  changed: boolean;
  /** "this step", "3 rom_loss" or "as read". */
  writer: string;
  /** Written by the step open. */
  thisStep: boolean;
}

/** Fields under one heading (a path), or none, in field order. */
export interface PieceGroup {
  path: string | null;
  fields: PieceField[];
}

/**
 * The picked unit's sheet (Case setup and unit plan, decisions 5 to 7): whether the step made it (no before), the fields
 * the step changed first, then the rest, each under its path's heading; fields empty or at nought, unchanged, set aside
 * to fold. Last written says "this step" for the step open, else its number and name, or "as read".
 */
export function pieceSheet(steps: ColumnStep[], at: number, row: PieceRow, fields: ColumnField[]): { made: boolean; changed: PieceGroup[]; rest: PieceGroup[]; quiet: PieceGroup[] } {
  const u = row.after ?? row.before!;
  const made = !row.before;
  const all: PieceField[] = fields.map((f) => {
    const before = row.before?.values[f.name] ?? null;
    const after = row.after?.values[f.name] ?? null;
    const w = lastWriterAt(steps, at, u.id, f.name);
    return {
      name: f.name,
      short: fieldPath(f.name).short,
      unit: f.unit,
      before,
      after,
      changed: !!row.before && !!row.after && moved(before, after),
      writer: w === null ? 'as read' : w.step === at ? 'this step' : `${w.step} ${w.name}`,
      thisStep: w?.step === at,
    };
  });
  const nought = (v: number | null) => v === null || Number.isNaN(v) || v === 0;
  const quiet = (f: PieceField) => !f.changed && nought(f.after) && nought(f.before);
  const grouped = (list: PieceField[]): PieceGroup[] => {
    const out: PieceGroup[] = [];
    for (const f of list) {
      const path = fieldPath(f.name).path;
      const g = out.find((x) => x.path === path);
      if (g) g.fields.push(f);
      else out.push({ path, fields: [f] });
    }
    return out;
  };
  return {
    made,
    changed: grouped(all.filter((f) => f.changed)),
    rest: grouped(all.filter((f) => !f.changed && !quiet(f))),
    quiet: grouped(all.filter(quiet)),
  };
}
