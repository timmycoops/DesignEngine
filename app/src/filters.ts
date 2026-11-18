/**
 * The filter, as the engine reads it (decision 0088; `zd-core` filters): attribute names, each with a comma list of the
 * values it may hold, typed as `strip: 1-5; seam: U, M`. An item is a word, a number, a range (`1-5`), a comparison
 * (`<0.3`, `>=120`) or `*` for any; `!` before one is a value it may not hold. Names and words match with their case aside.
 * Shared by the bench rules, the filter box (`FilterInput`) and the tables, so each reads a filter the same way. Kept free
 * of React, so the tests can run it.
 */

export type AttrValue = boolean | number | string;
export type Attributes = Record<string, AttrValue>;
/** Attribute name to the comma list of what it may hold; a blank list is anything. */
export type When = Record<string, string>;

/** A range such as `1-5` or `-20--5`: two numbers either side of a dash, lowest first. `U-M` is no range. */
export function range(item: string): [number, number] | null {
  for (let i = 1; i < item.length; i++) {
    if (item[i] !== '-') continue;
    const [a, b] = [item.slice(0, i).trim(), item.slice(i + 1).trim()];
    if (a === '' || b === '') continue;
    const [x, y] = [Number(a), Number(b)];
    if (Number.isFinite(x) && Number.isFinite(y)) return [Math.min(x, y), Math.max(x, y)];
  }
  return null;
}

/** A comparison such as `<0.3` or `>=120`: how it compares and the number; `<abc` is none, and is read as a word. */
export function comparison(item: string): [string, number] | null {
  const op = ['<=', '>=', '<', '>'].find((o) => item.startsWith(o));
  if (!op) return null;
  const rest = item.slice(op.length).trim();
  const x = Number(rest);
  return rest !== '' && Number.isFinite(x) ? [op, x] : null;
}

/** A value as it reads in a name or a filter: whole numbers without a decimal point. */
export const word = (v: AttrValue) => (typeof v === 'number' ? String(v) : String(v).trim());

const numberOf = (value: AttrValue) => (typeof value === 'number' ? value : typeof value === 'string' && value.trim() !== '' && Number.isFinite(Number(value)) ? Number(value) : null);

function itemFits(item: string, value: AttrValue): boolean {
  if (item === '*') return true;
  const n = numberOf(value);
  const compared = comparison(item);
  if (n !== null) {
    if (compared) {
      const [op, x] = compared;
      return op === '<' ? n < x : op === '<=' ? n <= x : op === '>' ? n > x : n >= x;
    }
    const r = range(item);
    if (r) return r[0] <= n && n <= r[1];
    const x = Number(item);
    if (item !== '' && Number.isFinite(x)) return Math.abs(x - n) <= 1e-9 * Math.max(Math.abs(x), 1);
  }
  // A comparison never fits a word.
  return !compared && word(value).toLowerCase() === item.toLowerCase();
}

/** A list's items, trimmed, blanks dropped. */
export const items = (list: string) => list.split(',').map((i) => i.trim()).filter((i) => i !== '');

/**
 * Whether a filter's comma list fits a value, as the engine's `list_fits`: blank fits anything; of the items without `!`
 * one must fit, so a value that isn't there fits none; of those with `!` none may.
 */
export function listFits(list: string, value: AttrValue | undefined): boolean {
  const all = items(list);
  const not = all.filter((i) => i.startsWith('!')).map((i) => i.slice(1).trim()).filter((i) => i !== '');
  const may = all.filter((i) => !i.startsWith('!'));
  const mayFits = may.length === 0 || may.includes('*') || (value !== undefined && may.some((i) => itemFits(i, value)));
  return mayFits && !(value !== undefined && not.some((i) => itemFits(i, value)));
}

/** An attribute's value by name, the name's case aside. */
export function attributeOf(attributes: Attributes, key: string): AttrValue | undefined {
  if (key in attributes) return attributes[key];
  const k = Object.keys(attributes).find((a) => a.toLowerCase() === key.toLowerCase());
  return k === undefined ? undefined : attributes[k];
}

/** Whether attributes fit a filter: every attribute it names fits its list. */
export const whenFits = (when: When, attributes: Attributes) => Object.entries(when).every(([key, list]) => listFits(list, attributeOf(attributes, key)));

/** A filter as it's typed: `horizon: U-M, M-L; strip: 1-5`. */
export const whenText = (when: When) =>
  Object.entries(when)
    .filter(([, l]) => l.trim() !== '')
    .map(([k, l]) => `${k}: ${l.trim()}`)
    .join('; ');

/** A filter read back from its text, or why it can't be. */
export function readWhen(text: string): { when: When } | { why: string } {
  const when: When = {};
  for (const part of text.split(';').map((p) => p.trim()).filter((p) => p !== '')) {
    const at = part.indexOf(':');
    if (at <= 0) return { why: `"${part}" needs an attribute, a colon and its values, as "horizon: U-M, M-L"` };
    const key = part.slice(0, at).trim();
    if (key in when) return { why: `${key} is named twice` };
    when[key] = part.slice(at + 1).trim();
  }
  return { when };
}

/** The attributes the cuts write, first and in this order; every other after them by name. */
const FIRST = ['strip', 'block', 'seam', 'horizon', 'bench', 'material', 'case'];

/** One value an attribute holds, as it's spelt, and how many rows hold it. */
export interface ValueCount {
  value: string;
  count: number;
}

/** An attribute the rows carry: its values with counts, and whether they're all numbers, with the lowest and highest. */
export interface AttributeValues {
  key: string;
  numeric: boolean;
  values: ValueCount[];
  min: number | null;
  max: number | null;
  /** How many rows carry it. */
  rows: number;
}

/** Names in the order a person would put them: 9 before 10. */
const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' }).compare;

/**
 * The attributes the rows carry, the cuts' own first, each with its values and how many rows hold each: what the filter
 * box lists to tick. Values a word at a time, case aside (the first spelling seen kept), in a person's order.
 */
export function valuesOf(rows: readonly Attributes[]): AttributeValues[] {
  const seen = new Map<string, { numeric: boolean; counts: Map<string, ValueCount>; min: number | null; max: number | null; rows: number }>();
  for (const row of rows) {
    for (const [key, value] of Object.entries(row)) {
      const name = [...seen.keys()].find((k) => k.toLowerCase() === key.toLowerCase()) ?? key;
      const entry = seen.get(name) ?? { numeric: true, counts: new Map(), min: null, max: null, rows: 0 };
      const n = typeof value === 'number' && Number.isFinite(value) ? value : null;
      entry.numeric &&= n !== null;
      if (n !== null) {
        entry.min = entry.min === null ? n : Math.min(entry.min, n);
        entry.max = entry.max === null ? n : Math.max(entry.max, n);
      }
      const w = word(value);
      const at = w.toLowerCase();
      const count = entry.counts.get(at) ?? { value: w, count: 0 };
      count.count += 1;
      entry.counts.set(at, count);
      entry.rows += 1;
      seen.set(name, entry);
    }
  }
  const keys = [...seen.keys()].sort((a, b) => {
    const [i, j] = [FIRST.indexOf(a.toLowerCase()), FIRST.indexOf(b.toLowerCase())];
    if (i >= 0 || j >= 0) return (i < 0 ? FIRST.length : i) - (j < 0 ? FIRST.length : j);
    return byName(a, b);
  });
  return keys.map((key) => {
    const e = seen.get(key)!;
    const values = [...e.counts.values()].sort((a, b) => (e.numeric ? Number(a.value) - Number(b.value) : byName(a.value, b.value)));
    return { key, numeric: e.numeric, values, min: e.min, max: e.max, rows: e.rows };
  });
}

/** Whether a list is only values to tick: words and numbers, no range, comparison, `*` or `!`. */
export const isTicked = (list: string) => items(list).every((i) => i !== '*' && !i.startsWith('!') && !comparison(i) && !range(i));

/**
 * The filter with a value of an attribute ticked on or off. A list of ticked values gains or loses it. A list that's more
 * than ticks (a range, a comparison, a not) becomes the ticks of the values it keeps now, that one on or off, so what was
 * kept stays kept; `values` are what the rows hold. Unticking the last leaves the attribute out: anything.
 */
export function tick(when: When, key: string, value: string, on: boolean, values: readonly string[]): When {
  const list = when[key] ?? '';
  const kept = isTicked(list) ? items(list) : items(list).length === 0 ? [] : values.filter((v) => listFits(list, v));
  const has = (v: string) => kept.some((k) => k.toLowerCase() === v.toLowerCase());
  const next = on ? (has(value) ? kept : [...kept, value]) : kept.filter((k) => k.toLowerCase() !== value.toLowerCase());
  const out: When = { ...when };
  if (next.length === 0) delete out[key];
  else out[key] = next.join(', ');
  return out;
}

/** Whether a value shows ticked: the attribute is filtered on, and its list keeps the value. */
export const isOn = (when: When, key: string, value: string) => {
  const list = when[key] ?? '';
  return items(list).length > 0 && listFits(list, value);
};
