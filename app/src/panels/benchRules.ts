/**
 * Bench rules (decision 0071): a short table read top down, the first rule a solid's attributes fit saying whether it's
 * benched and how. The same matching as the engine's `solid.cut_by_benches`, so the panel can show what each rule takes
 * before the cut; the horizon ticks, which keep one rule at the top listing the horizons left whole; and the rules as CSV,
 * a column an attribute. Kept free of React, so the tests can run it.
 */

export type AttrValue = boolean | number | string;
export type Attributes = Record<string, AttrValue>;

export interface BenchRule {
  /** Attribute name to the values it may hold: a comma list of words, numbers, ranges (`1-5`) or comparisons (`<0.3`), `!` before one it may not; blank or `*` is any. */
  when: Record<string, string>;
  benched: boolean;
  from: number;
  to: number;
  every: number;
  named_by: 'bottom' | 'top';
  /** Its own output layer; blank or left out, the run's. */
  layer?: string;
}

/** Everything, RL 0 to 500 every 10 m, named by the floor: the default, and the rule for everything else. */
export const everything = (): BenchRule => ({ when: {}, benched: true, from: 0, to: 500, every: 10, named_by: 'bottom' });

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

/** A value as it reads in a name or a filter: whole numbers without a decimal point. */
const word = (v: AttrValue) => (typeof v === 'number' ? String(v) : String(v).trim());

/** A comparison such as `<0.3` or `>=120`: how it compares and the number; `<abc` is none, and is read as a word. */
export function comparison(item: string): [string, number] | null {
  const op = ['<=', '>=', '<', '>'].find((o) => item.startsWith(o));
  if (!op) return null;
  const rest = item.slice(op.length).trim();
  const x = Number(rest);
  return rest !== '' && Number.isFinite(x) ? [op, x] : null;
}

function itemFits(item: string, value: AttrValue): boolean {
  if (item === '*') return true;
  const n = typeof value === 'number' ? value : typeof value === 'string' && value.trim() !== '' && Number.isFinite(Number(value)) ? Number(value) : null;
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

const items = (list: string) => list.split(',').map((i) => i.trim()).filter((i) => i !== '');

/**
 * Whether a filter's comma list fits a value, as the engine's `list_fits` (Filtering plan, decision 1): blank fits
 * anything; of the items without `!` one must fit, so a value that isn't there fits none; of those with `!` none may.
 */
export function listFits(list: string, value: AttrValue | undefined): boolean {
  const all = items(list);
  const not = all.filter((i) => i.startsWith('!')).map((i) => i.slice(1).trim()).filter((i) => i !== '');
  const may = all.filter((i) => !i.startsWith('!'));
  const mayFits = may.length === 0 || may.includes('*') || (value !== undefined && may.some((i) => itemFits(i, value)));
  return mayFits && !(value !== undefined && not.some((i) => itemFits(i, value)));
}

/** An attribute's value by name, the name's case aside. */
function attribute(attributes: Attributes, key: string): AttrValue | undefined {
  if (key in attributes) return attributes[key];
  const k = Object.keys(attributes).find((a) => a.toLowerCase() === key.toLowerCase());
  return k === undefined ? undefined : attributes[k];
}

export function fits(rule: BenchRule, attributes: Attributes): boolean {
  return Object.entries(rule.when).every(([key, list]) => listFits(list, attribute(attributes, key)));
}

/** The index of the first rule a solid fits, or -1 where none does. */
export const ruleFor = (rules: readonly BenchRule[], attributes: Attributes) => rules.findIndex((r) => fits(r, attributes));

/** How many solids each rule takes, and how many no rule takes. */
export function taken(rules: readonly BenchRule[], solids: readonly Attributes[]): { byRule: number[]; none: number } {
  const byRule = rules.map(() => 0);
  let none = 0;
  for (const a of solids) {
    const k = ruleFor(rules, a);
    if (k < 0) none++;
    else byRule[k]++;
  }
  return { byRule, none };
}

/** The horizons the solids carry, in the order they first come (the seam cut makes them top down). */
export function horizonsOf(solids: readonly Attributes[]): string[] {
  const seen: string[] = [];
  for (const a of solids) {
    const h = attribute(a, 'horizon');
    if (h !== undefined && !seen.includes(word(h))) seen.push(word(h));
  }
  return seen;
}

/** The rule the horizon ticks keep at the top: the horizons left whole, and nothing else. */
const isTickRule = (r: BenchRule | undefined) => !!r && !r.benched && !r.layer?.trim() && Object.keys(r.when).length === 1 && 'horizon' in r.when;

/**
 * Whether a horizon's solids are benched under the rules: `all`, `none` or `some`, and whether the ticks decide it (the
 * horizon is listed in their rule, or no rule takes it out) or another rule does, which the panel then names.
 */
export function horizonState(rules: readonly BenchRule[], solids: readonly Attributes[], horizon: string): { benched: 'all' | 'none' | 'some'; byTicks: boolean; rules: number[] } {
  const of = solids.filter((a) => {
    const h = attribute(a, 'horizon');
    return h !== undefined && word(h) === horizon;
  });
  const which = of.map((a) => ruleFor(rules, a));
  const yes = which.filter((k) => k >= 0 && rules[k].benched).length;
  const benched = yes === of.length ? 'all' : yes === 0 ? 'none' : 'some';
  const deciding = [...new Set(which.filter((k) => k >= 0 && !rules[k].benched))];
  const byTicks = deciding.every((k) => k === 0 && isTickRule(rules[0]));
  return { benched, byTicks, rules: deciding };
}

/** The rules with a horizon ticked (benched) or not: the ticks' own rule at the top gains or loses it. */
export function setHorizonBenched(rules: readonly BenchRule[], horizon: string, benched: boolean): BenchRule[] {
  const out = rules.map((r) => ({ ...r, when: { ...r.when } }));
  const tick = isTickRule(out[0]) ? out[0] : null;
  const listed = tick ? items(tick.when.horizon) : [];
  const next = benched ? listed.filter((h) => h.toLowerCase() !== horizon.toLowerCase()) : listed.some((h) => h.toLowerCase() === horizon.toLowerCase()) ? listed : [...listed, horizon];
  if (tick) out.shift();
  return next.length > 0 ? [{ when: { horizon: next.join(', ') }, benched: false, from: 0, to: 500, every: 10, named_by: 'bottom' }, ...out] : out;
}

/** The rule for everything else: the last one, if it fits every solid. */
export const catchAll = (rules: readonly BenchRule[]) => {
  const last = rules.length - 1;
  return last >= 0 && Object.values(rules[last].when).every((l) => items(l).length === 0 || items(l).includes('*')) ? last : -1;
};

/** A rule's filter as it's typed: `horizon: U-M, M-L; strip: 1-5`. */
export const whenText = (when: Record<string, string>) =>
  Object.entries(when)
    .filter(([, l]) => l.trim() !== '')
    .map(([k, l]) => `${k}: ${l.trim()}`)
    .join('; ');

/** A filter read back from its text, or why it can't be. */
export function readWhen(text: string): { when: Record<string, string> } | { why: string } {
  const when: Record<string, string> = {};
  for (const part of text.split(';').map((p) => p.trim()).filter((p) => p !== '')) {
    const at = part.indexOf(':');
    if (at <= 0) return { why: `"${part}" needs an attribute, a colon and its values, as "horizon: U-M, M-L"` };
    const key = part.slice(0, at).trim();
    if (key in when) return { why: `${key} is named twice` };
    when[key] = part.slice(at + 1).trim();
  }
  return { when };
}

/** The columns a rule's own settings take in CSV, after one column for each attribute filtered on. */
const SETTINGS = ['benched', 'from', 'to', 'every', 'named_by', 'layer'] as const;
/** The cut's attributes first, in this order; any other by name after them. */
const FIRST = ['horizon', 'material', 'seam', 'strip', 'block'];

const cell = (v: string) => (/[",\n\r]/.test(v) || v !== v.trim() ? `"${v.replace(/"/g, '""')}"` : v);

/** The rules as CSV: a column for each attribute any rule filters on, then the rule's settings. */
export function rulesToCsv(rules: readonly BenchRule[]): string {
  const keys = [...new Set(rules.flatMap((r) => Object.keys(r.when)))].sort((a, b) => {
    const [x, y] = [FIRST.indexOf(a), FIRST.indexOf(b)];
    return x >= 0 && y >= 0 ? x - y : x >= 0 ? -1 : y >= 0 ? 1 : a.localeCompare(b);
  });
  const lines = [[...keys, ...SETTINGS].map(cell).join(',')];
  for (const r of rules) {
    const settings = [r.benched ? 'yes' : 'no', ...(r.benched ? [String(r.from), String(r.to), String(r.every), r.named_by] : ['', '', '', '']), r.layer ?? ''];
    lines.push([...keys.map((k) => r.when[k] ?? ''), ...settings].map(cell).join(','));
  }
  return lines.join('\n') + '\n';
}

/** CSV split into rows of cells, quotes and doubled quotes read. */
function rowsOf(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') {
      row.push(field);
      field = '';
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else field += c;
  }
  if (field !== '' || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows.filter((r) => r.some((c) => c.trim() !== '') && !r[0].trim().startsWith('#'));
}

const YES = ['yes', 'y', 'true', '1'];
const NO = ['no', 'n', 'false', '0'];

/**
 * Rules read from CSV: the first row names the columns, `benched`, `from`, `to`, `every`, `named_by` and `layer` the
 * rule's own (any may be left out, taking the default) and every other an attribute to filter on. Each mistake is said
 * by its line, and with any, no rules are taken.
 */
export function rulesFromCsv(text: string): { rules: BenchRule[]; errors: string[] } {
  const rows = rowsOf(text.replace(/^﻿/, ''));
  if (rows.length === 0) return { rules: [], errors: ['the file is empty'] };
  const head = rows[0].map((h) => h.trim());
  const lower = head.map((h) => h.toLowerCase());
  const errors: string[] = [];
  const rules: BenchRule[] = [];
  rows.slice(1).forEach((row, n) => {
    const line = n + 2;
    const rule = everything();
    for (let k = 0; k < head.length; k++) {
      const v = (row[k] ?? '').trim();
      const name = lower[k];
      if (name === '' || (v === '' && name !== 'layer')) continue;
      if (name === 'benched') {
        if (YES.includes(v.toLowerCase())) rule.benched = true;
        else if (NO.includes(v.toLowerCase())) rule.benched = false;
        else errors.push(`line ${line}: benched is "${v}", not yes or no`);
      } else if (name === 'from' || name === 'to' || name === 'every') {
        const x = Number(v);
        if (!Number.isFinite(x)) errors.push(`line ${line}: ${name} is "${v}", not a number`);
        else if (name === 'from') rule.from = x;
        else if (name === 'to') rule.to = x;
        else rule.every = x;
      } else if (name === 'named_by') {
        if (v.toLowerCase() === 'bottom' || v.toLowerCase() === 'top') rule.named_by = v.toLowerCase() as 'bottom' | 'top';
        else errors.push(`line ${line}: named_by is "${v}", not bottom or top`);
      } else if (name === 'layer') {
        if (v !== '') rule.layer = v;
      } else {
        rule.when[head[k]] = v;
      }
    }
    if (rule.benched && !(rule.every > 0)) errors.push(`line ${line}: every must be more than nothing`);
    if (rule.benched && rule.to < rule.from) errors.push(`line ${line}: from RL ${rule.from} is above to RL ${rule.to}`);
    rules.push(rule);
  });
  if (rules.length === 0 && errors.length === 0) errors.push('no rules under the column names');
  return errors.length > 0 ? { rules: [], errors } : { rules, errors };
}

/** The rules as the command takes them: blank filters and layers left out. */
export function rulesParam(rules: readonly BenchRule[]) {
  return rules.map((r) => {
    const when = Object.fromEntries(Object.entries(r.when).filter(([, l]) => l.trim() !== ''));
    const layer = r.layer?.trim();
    return r.benched ? { when, benched: true, from: r.from, to: r.to, every: r.every, named_by: r.named_by, ...(layer ? { layer } : {}) } : { when, benched: false, ...(layer ? { layer } : {}) };
  });
}
