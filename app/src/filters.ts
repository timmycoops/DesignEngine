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

/** One thing the filter box offers to put in as it's typed. */
export interface Suggestion {
  /** What goes in the text. */
  insert: string;
  /** What the list shows. */
  label: string;
  /** Beside it, quieter: how many hold it, or an attribute's range. */
  detail?: string;
}

/** What the box offers where the caret is: the stretch of text a suggestion takes the place of, and a line to help. */
export interface Suggestions {
  from: number;
  to: number;
  items: Suggestion[];
  hint?: string;
}

/** A name to offer that no row lists to tick, typed instead: a table's field, or a measure. */
export type Named = string | { key: string; detail: string };
const named_ = (n: Named) => (typeof n === 'string' ? { key: n, detail: 'type it' } : n);

/**
 * The measures (`zd-core` `MEASURES`) objects of these kinds have, as names to offer: a solid's volume, a string's length
 * and area (a closed one's, in plan), a surface's area, and the lowest and highest RL of all four.
 */
export function measuresOf(kinds: Iterable<string>): Named[] {
  const has = new Set(kinds);
  const out: Named[] = [];
  if (has.has('solid')) out.push({ key: 'volume', detail: 'm³' });
  if (has.has('string')) out.push({ key: 'length', detail: 'm' });
  if (has.has('string') || has.has('surface')) out.push({ key: 'area', detail: 'm²' });
  if (['solid', 'string', 'surface', 'grid'].some((k) => has.has(k))) out.push({ key: 'rl_min', detail: 'RL' }, { key: 'rl_max', detail: 'RL' });
  return out;
}

/** More than this, and the list shows only the first: typing narrows it. */
const MOST_SUGGESTED = 50;

const fmtCount = new Intl.NumberFormat('en-AU');
const fmtNumber = (n: number) => (Number.isInteger(n) ? String(n) : String(Number(n.toPrecision(6))));

/** Those that start with what's typed first, then those that hold it, case aside. */
function matching<T>(all: readonly T[], typed: string, text: (t: T) => string): T[] {
  const t = typed.trim().toLowerCase();
  if (t === '') return [...all];
  const starts = all.filter((a) => text(a).toLowerCase().startsWith(t));
  const holds = all.filter((a) => !text(a).toLowerCase().startsWith(t) && text(a).toLowerCase().includes(t));
  return [...starts, ...holds];
}

/**
 * What the filter box offers as it's typed (Filtering plan, after workstream 5): where the caret is in an attribute's
 * name, the names the rows carry and `names` (fields a table has that aren't listed to tick, `ROM\Coal\ash`), those not
 * already named; after its colon, the attribute's values with how many hold each, those not already in its list, a `!`
 * typed kept before the value; for numbers, their range and the ways to type one. A suggestion takes the place of the
 * word the caret is in, from its start to the next separator.
 */
export function suggest(text: string, caret: number, attributes: readonly AttributeValues[], names: readonly Named[] = []): Suggestions {
  const at = Math.max(0, Math.min(caret, text.length));
  const partStart = text.lastIndexOf(';', at - 1) + 1;
  const before = text.slice(partStart, at);
  const colon = before.indexOf(':');
  const lead = (s: string) => s.length - s.trimStart().length;

  if (colon < 0) {
    // A name: those carried and those only typed, less those other parts already name.
    const from = partStart + lead(before);
    const rest = text.slice(at).match(/^[^:;]*/)?.[0] ?? '';
    const to = at + rest.trimEnd().length;
    const hasColon = text.slice(at + rest.length).startsWith(':');
    const typed = text.slice(from, at);
    const named = new Set<string>();
    let start = 0;
    for (const p of text.split(';')) {
      const name = p.slice(0, Math.max(0, p.indexOf(':'))).trim().toLowerCase();
      if (start !== partStart && name !== '') named.add(name);
      start += p.length + 1;
    }
    const carried = attributes.map((a) => ({ key: a.key, detail: a.numeric && a.min !== null && a.max !== null ? (a.min === a.max ? fmtNumber(a.min) : `${fmtNumber(a.min)} to ${fmtNumber(a.max)}`) : `${fmtCount.format(a.values.length)} value${a.values.length === 1 ? '' : 's'}` }));
    const only = names.map(named_).filter((n) => !attributes.some((a) => a.key.toLowerCase() === n.key.toLowerCase()));
    const all = [...carried, ...only].filter((c) => !named.has(c.key.toLowerCase()));
    const items = matching(all, typed, (c) => c.key)
      .slice(0, MOST_SUGGESTED)
      .map((c) => ({ insert: hasColon ? c.key : `${c.key}: `, label: c.key, detail: c.detail }));
    return { from, to, items, hint: items.length === 0 && typed.trim() !== '' ? `Nothing here is called ${typed.trim()}` : undefined };
  }

  // A value of the attribute named before the colon.
  const key = before.slice(0, colon).trim();
  const attribute = attributes.find((a) => a.key.toLowerCase() === key.toLowerCase());
  const listStart = colon + 1;
  const itemStart = before.lastIndexOf(',') >= listStart ? before.lastIndexOf(',') + 1 : listStart;
  const itemText = before.slice(itemStart);
  const from = partStart + itemStart + lead(itemText);
  const to = at + (text.slice(at).match(/^[^,;]*/)?.[0] ?? '').trimEnd().length;
  const typedItem = text.slice(from, at);
  const not = typedItem.startsWith('!');
  const typed = not ? typedItem.slice(1).trimStart() : typedItem;
  const ways = 'a range (1-5), < > <= >= a number, or ! before one it may not hold';
  if (!attribute) {
    const field = names.some((n) => named_(n).key.toLowerCase() === key.toLowerCase());
    return { from, to, items: [], hint: field ? `Type ${ways}` : key === '' ? undefined : `Nothing here is called ${key}` };
  }
  const hint = attribute.numeric && attribute.min !== null && attribute.max !== null
    ? `${fmtNumber(attribute.min)} to ${fmtNumber(attribute.max)}: tick one, or type ${ways}`
    : undefined;
  if (comparison(typed) || range(typed)) return { from, to, items: [], hint };
  // The list's other items, less this one: what's already there isn't offered again.
  const listEnd = at + (text.slice(at).match(/^[^;]*/)?.[0] ?? '').length;
  const list = text.slice(partStart + listStart, listEnd);
  const used = new Set(
    items(list)
      .map((i) => (i.startsWith('!') ? i.slice(1).trim() : i).toLowerCase())
      .filter((i) => i !== typed.trim().toLowerCase()),
  );
  const offered = matching(attribute.values.filter((v) => !used.has(v.value.toLowerCase())), typed, (v) => (v.value === '' ? '(blank)' : v.value));
  const shown = offered.slice(0, MOST_SUGGESTED).map((v) => ({ insert: `${not ? '!' : ''}${v.value}`, label: `${not ? '!' : ''}${v.value === '' ? '(blank)' : v.value}`, detail: fmtCount.format(v.count) }));
  return { from, to, items: shown, hint: hint ?? (offered.length > MOST_SUGGESTED ? `${fmtCount.format(offered.length - MOST_SUGGESTED)} more: type to narrow` : undefined) };
}

/** The text with a suggestion put in, and where the caret goes: after it. */
export function accept(text: string, suggestions: Suggestions, suggestion: Suggestion): { text: string; caret: number } {
  const next = text.slice(0, suggestions.from) + suggestion.insert + text.slice(suggestions.to);
  return { text: next, caret: suggestions.from + suggestion.insert.length };
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
