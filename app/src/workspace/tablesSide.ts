/**
 * The Tables side (decision 0108, workstream 4): a list down the left in the order the work flows, the item picked filling
 * the middle, and the case run from the top row. Kept free of React, so the workspace, the ribbon and the tests can read it.
 *
 * What's open, and the case the top row runs, live here rather than in a panel: the ribbon and the panel both read them,
 * and a tool asked for from anywhere (Find a command, a Setup's Edit) opens its item.
 */

/** The list's sections, in the order the work flows. */
export type Section = 'inputs' | 'case' | 'dataflows' | 'params' | 'results' | 'schedule';

export const SECTIONS: readonly { id: Section; title: string; tip: string }[] = [
  { id: 'inputs', title: 'Inputs', tip: "What a case reads from Design: the Objects table, and any table imported in the solids' place" },
  { id: 'case', title: 'Cases', tip: 'Each reserves case a step at a time: what each step reads and does to the whole case' },
  { id: 'dataflows', title: 'Dataflows', tip: 'The dataflows a case runs, and the scripts their formulas load' },
  { id: 'params', title: 'Parameters', tip: 'The settings tables every number comes from' },
  { id: 'results', title: 'Results', tip: "Each case's reserves table, grouped by strip, block or seam" },
  { id: 'schedule', title: 'Schedule', tip: "The schedule quantities a case's run gives, for the scheduler (0100): not built yet" },
];

/** An item of the list: its section, and the object it shows (none for the Objects table and the Schedule). */
export interface Item {
  section: Section;
  id: string | null;
}

/** The Objects table, the first input: the design's solids as rows. */
export const OBJECTS = 'objects';

interface Listed {
  id: string;
  name: string;
  kind: string;
  objects_own?: boolean;
}

/** A row of the list, as the panel draws it. */
export interface ListRow {
  section: Section;
  id: string | null;
  name: string;
  /** What it is or says, in a word or two, beside its name. */
  note: string;
}

/**
 * The list from the project's objects and its cases (the reserves tables): inputs (the Objects table, then any other
 * table, such as one imported from a CSV), cases, dataflows and scripts, parameter tables, a reserves table for each case,
 * and the schedule's place.
 */
export function listOf(objects: readonly Listed[], cases: readonly { table: string; name: string; base: boolean }[]): ListRow[] {
  const isCase = new Set(cases.map((c) => c.table));
  const tables = objects.filter((o) => o.kind === 'table' && !o.objects_own);
  const rows: ListRow[] = [{ section: 'inputs', id: OBJECTS, name: 'Objects table', note: 'the solids' }];
  for (const t of tables) if (!isCase.has(t.id)) rows.push({ section: 'inputs', id: t.id, name: t.name, note: 'table' });
  for (const c of cases) rows.push({ section: 'case', id: c.table, name: c.name, note: c.base ? 'Base' : '' });
  for (const o of objects) if (o.kind === 'dataflow') rows.push({ section: 'dataflows', id: o.id, name: o.name, note: '' });
  for (const o of objects) if (o.kind === 'script') rows.push({ section: 'dataflows', id: o.id, name: o.name, note: 'script' });
  for (const o of objects) if (o.kind === 'parameter_table') rows.push({ section: 'params', id: o.id, name: o.name, note: '' });
  for (const c of cases) rows.push({ section: 'results', id: c.table, name: c.name, note: 'reserves table' });
  rows.push({ section: 'schedule', id: null, name: 'Schedule quantities', note: 'not built' });
  return rows;
}

/** The section an object opens in, from its kind; a table opens as a case if it's one, else as an input. */
export function sectionOf(kind: string, isCase: boolean): Section | null {
  if (kind === 'dataflow' || kind === 'script') return 'dataflows';
  if (kind === 'parameter_table') return 'params';
  if (kind === 'table') return isCase ? 'case' : 'inputs';
  return null;
}

let open: Item = { section: 'case', id: null };
/** The case the top row runs: the last case or result opened. */
let caseNow: string | null = null;
const listeners = new Set<() => void>();
/** The item each section showed last, so going back to it finds it as it was. */
const last = new Map<Section, string>();

/** What's open, and the case the top row runs. */
export const itemNow = (): Item => open;
export const caseOpen = (): string | null => caseNow;

/**
 * Opens an item. A case or a result makes its table the case the top row runs; an item with no id keeps the one its
 * section showed last.
 */
export function openItem(section: Section, id: string | null = null) {
  const kept = id ?? (open.section === section ? open.id : (last.get(section) ?? null));
  open = { section, id: kept };
  if (kept) last.set(section, kept);
  if ((section === 'case' || section === 'results') && kept) caseNow = kept;
  for (const f of listeners) f();
}

/** The case the top row runs, set from the top row: its case opened too, where a case or a result is open. */
export function setCase(table: string) {
  caseNow = table;
  if (open.section === 'case' || open.section === 'results') return openItem(open.section, table);
  for (const f of listeners) f();
}

/** Told whenever what's open, or the case, changes. Returns the way to stop. */
export function onItem(f: () => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

/** What the window shows, set by the workspace as it changes (decision 0109): the case's picks go to the view in Both. */
let showing: 'design' | 'tables' | 'both' = 'design';
export const showingNow = () => showing;
export const setShowing = (now: 'design' | 'tables' | 'both') => {
  showing = now;
};

/** For the tests: back to as it starts. */
export function resetItems() {
  open = { section: 'case', id: null };
  caseNow = null;
  last.clear();
}
