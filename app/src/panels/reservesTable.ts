import type { Breakdown, BreakdownField } from '../bridge';

// As `fmt` in format.ts, kept here so the tests run this file on its own.
const formats = new Map<number, Intl.NumberFormat>();
function fmt(value: number, places: number): string {
  let format = formats.get(places);
  if (!format) {
    format = new Intl.NumberFormat('en-AU', { minimumFractionDigits: places, maximumFractionDigits: places });
    formats.set(places, format);
  }
  return format.format(value);
}

/**
 * The reserves table's fields as rows (decision 0075): a node's fields down the side, grouped by their folders, and its
 * children across. What's here is laid out and worded; every number is the engine's.
 */

/** A row of the table: a folder's heading, or a field. */
export type FieldRow =
  | { kind: 'folder'; key: string; label: string; depth: number }
  | { kind: 'field'; key: string; label: string; unit: string; depth: number; field: BreakdownField };

/**
 * Fields left out unless every field is asked for: densities, moistures and plan geometry, which the steps read. A folder's
 * own density, the one its tonnes are on, is shown all the same (`densityOfTonnes`).
 */
const DETAIL = new Set(['area', 'length', 'width', 'dip', 'centroid_x', 'centroid_y', 'centroid_z', 'mad', 'mis', 'mrom', 'rd', 'rd_is', 'rd_rom', 'tonnes_ad', 'tonnes_ad_per_m3', 'id']);

/**
 * Field names a reserves table written before a rename still has, read as they're called now (app review brief, 17 Nov,
 * item 2): `rd_adt` is the air-dried tonnes a cubic metre in situ. A table keeps its columns as written; run again, it
 * has the new name.
 */
const RENAMED: Record<string, string> = { rd_adt: 'tonnes_ad_per_m3' };
const nowNamed = (name: string) => RENAMED[name.toLowerCase()] ?? name;

export function isDetail(name: string): boolean {
  return DETAIL.has(nowNamed(name).toLowerCase());
}

/**
 * The density a folder's tonnes are on, as `reserves.run` writes them: ROM and Scheduled on the ROM density, any other
 * folder with an in situ density on it, else on the RD; none where the folder has no tonnes. Volume × it is the tonnes
 * shown (the engine's `every_folder_s_tonnes_are_its_volume_by_the_density_main_fields_shows` checks the same rule).
 */
export function densityOfTonnes(folders: string[], names: string[]): string | null {
  if (!names.includes('tonnes')) return null;
  if ((folders[0] === 'ROM' || folders[0] === 'Scheduled') && names.includes('rd_rom')) return 'rd_rom';
  if (names.includes('rd_is')) return 'rd_is';
  return names.includes('rd') ? 'rd' : null;
}

/** A field's name read out: `as_is` is "as, in situ", `as_rom` "as, ROM basis", `tonnes_ad` "tonnes, air dried". */
export function fieldLabel(field: string): string {
  const name = nowNamed(field);
  if (name.endsWith('_per_m3')) return `${fieldLabel(name.slice(0, -'_per_m3'.length))}, per m³`;
  const suffixes: [string, string][] = [
    ['_is', ', in situ'],
    ['_rom', ', ROM basis'],
    ['_ad', ', air dried'],
  ];
  for (const [end, words] of suffixes) if (name.endsWith(end) && name.length > end.length) return name.slice(0, -end.length) + words;
  return name;
}

/** How many decimals a value shows, by its unit. */
export function decimals(unit: string): number {
  if (unit === 'm3' || unit === 't' || unit === 'm2') return 1;
  if (unit === 't/m3') return 3;
  return 2;
}

export function unitLabel(unit: string): string {
  if (unit === 'deg') return '°';
  return unit.replace('m3', 'm³').replace('m2', 'm²');
}

/**
 * The rows: each top folder in the order the table first has it, then its sub-folders (a material, a kind of loss), each
 * with its fields; fields straight under a top folder (a section's name) first. Folded folders show their heading only.
 */
export function fieldRows(b: Breakdown, all: boolean, folded: ReadonlySet<string>): FieldRow[] {
  // Main fields: no detail, but each folder's own density beside its tonnes.
  const namesIn = new Map<string, string[]>();
  const folderOf = (f: BreakdownField) => f.folders.join('/');
  for (const f of b.fields) namesIn.set(folderOf(f), [...(namesIn.get(folderOf(f)) ?? []), f.name]);
  const fields = b.fields.filter((f) => all || !isDetail(f.name) || f.name === densityOfTonnes(f.folders, namesIn.get(folderOf(f)) ?? []));
  const tops: string[] = [];
  for (const f of fields) if (f.folders.length > 0 && !tops.includes(f.folders[0])) tops.push(f.folders[0]);
  const rows: FieldRow[] = [];
  for (const top of tops) {
    rows.push({ kind: 'folder', key: top, label: top, depth: 0 });
    if (folded.has(top)) continue;
    const here = fields.filter((f) => f.folders[0] === top);
    for (const f of here.filter((f) => f.folders.length === 1)) rows.push(fieldRow(f, 1));
    const subs: string[] = [];
    for (const f of here) {
      const sub = f.folders.slice(1).join('\\');
      if (sub && !subs.includes(sub)) subs.push(sub);
    }
    for (const sub of subs) {
      const key = `${top}\\${sub}`;
      // A schedule field's section (Waste allocation plan, decision 5) reads `Waste › Dragline`.
      rows.push({ kind: 'folder', key, label: sub.split('\\').join(' › '), depth: 1 });
      if (folded.has(key)) continue;
      for (const f of here.filter((f) => f.folders.slice(1).join('\\') === sub)) rows.push(fieldRow(f, 2));
    }
  }
  return rows;
}

function fieldRow(f: BreakdownField, depth: number): FieldRow {
  return { kind: 'field', key: f.column, label: fieldLabel(f.name), unit: f.unit, depth, field: f };
}

/** A field's value in one column, as shown. */
export function cellText(f: BreakdownField, column: number): string {
  if (f.words) return f.words[column] ?? '';
  const v = f.values[column];
  return v === null || v === undefined || !Number.isFinite(v) ? '' : fmt(v, decimals(f.unit));
}

/** The breakdown as CSV, exact values: a row a field (its folder path), a column a node (its level values). */
export function breakdownCsv(b: Breakdown): string {
  const quote = (s: string) => (/[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
  const head = ['field', 'unit', ...b.columns.map((c) => c.path.join(' > ') || 'All')];
  const lines = [head.map(quote).join(',')];
  for (const f of b.fields) {
    const cells = b.columns.map((_, i) => (f.words ? (f.words[i] ?? '') : f.values[i] === null ? '' : String(f.values[i])));
    lines.push([f.column, f.unit, ...cells].map(quote).join(','));
  }
  return `${lines.join('\n')}\n`;
}

/** Where a picked solid's leaf is shown: its block (the level above the seam) with that seam opened. */
export function nodeForLeaf(levels: string[], leaf: string[]): { node: string[]; open: string[][] } {
  const seam = levels.findIndex((l) => l.toLowerCase() === 'seam');
  const at = seam > 0 ? seam : Math.max(leaf.length - 1, 0);
  return { node: leaf.slice(0, at), open: [leaf.slice(0, at + 1)] };
}

export const samePath = (a: string[], b: string[]) => a.length === b.length && a.every((w, i) => w === b[i]);
export const startsWith = (path: string[], head: string[]) => head.length <= path.length && head.every((w, i) => w === path[i]);

/** What Isolate its solids changes, and what it puts back. */
export interface Isolation {
  /** The layers holding the solids, turned on. */
  showLayers: string[];
  /** Every other layer that was on, turned off. */
  hideLayers: string[];
  /** What else is drawn on the solids' layers and was showing, hidden. */
  hide: string[];
  /** The node's solids that were hidden on their own, shown. */
  unhide: string[];
}

const DRAWN = new Set(['string', 'surface', 'solid', 'grid']);

/**
 * A node's solids alone on screen: their layers on, every other layer off, and on their layers everything else drawn
 * hidden. Put back, the layers return as they were and only what this hid or showed changes back, so an object hidden
 * by hand beforehand stays hidden.
 */
export function isolation(
  solids: string[],
  objects: { id: string; layer: string; kind: string; hidden: boolean }[],
  layers: { id: string; visible: boolean }[],
): Isolation {
  const ids = new Set(solids);
  const theirs = new Set(objects.filter((o) => ids.has(o.id)).map((o) => o.layer));
  return {
    showLayers: [...theirs],
    hideLayers: layers.filter((l) => l.visible && !theirs.has(l.id)).map((l) => l.id),
    hide: objects.filter((o) => theirs.has(o.layer) && !ids.has(o.id) && !o.hidden && DRAWN.has(o.kind)).map((o) => o.id),
    unhide: objects.filter((o) => ids.has(o.id) && o.hidden).map((o) => o.id),
  };
}

/**
 * The layers and objects as they'll be once what's alone on screen is put back: each layer's tick as it was before, and
 * what Isolate hid shown and what it showed hidden again. Isolate or Show pressed while the other is on puts it back first
 * and works from this, not from the screen it's leaving.
 */
export function asPutBack<O extends { id: string; hidden: boolean }, L extends { id: string; visible: boolean }>(
  objects: O[],
  layers: L[],
  alone: { layers: { id: string; visible: boolean }[]; changed: Isolation | null } | null,
): { objects: O[]; layers: L[] } {
  if (!alone) return { objects, layers };
  const was = new Map(alone.layers.map((l) => [l.id, l.visible]));
  const shown = new Set(alone.changed?.hide ?? []);
  const hidden = new Set(alone.changed?.unhide ?? []);
  return {
    layers: layers.map((l) => (was.has(l.id) && was.get(l.id) !== l.visible ? { ...l, visible: was.get(l.id)! } : l)),
    objects: objects.map((o) => (shown.has(o.id) ? { ...o, hidden: false } : hidden.has(o.id) ? { ...o, hidden: true } : o)),
  };
}
