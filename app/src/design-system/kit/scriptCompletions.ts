import { type Completion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { localCompletionSource } from '@codemirror/lang-python';
import { type EditorView, hoverTooltip, type Tooltip } from '@codemirror/view';

/**
 * Completions and hover help for a script box (decision 0099): what the script can call and the names it can read,
 * offered as it's typed. The calls come from the engine's own lists (`zd-script`'s help, tested against what runs them);
 * the names from the panel: a dataflow's fields and parameters, a table's columns, the project's scripts.
 */

/** A call a script can make: its name, how it's called, what it gives. */
export interface ScriptCallInfo {
  name: string;
  signature: string;
  what: string;
}

/** The engine's lists of calls. */
export interface ScriptCalls {
  table: ScriptCallInfo[];
  tableGlobals: ScriptCallInfo[];
  library: ScriptCallInfo[];
  unitParts: ScriptCallInfo[];
  starlark: ScriptCallInfo[];
}

/** A name a script reads, with its unit where it has one. */
export interface ScriptName {
  name: string;
  unit?: string;
}

/** What a script box offers: its kind, the names it reads and the calls it can make. */
export interface ScriptNames {
  /** A formula on a unit or a row, a table script on a whole table, or functions other scripts load. */
  kind: 'formula' | 'table' | 'library';
  /** A formula's unit or row fields, which it reads and sets. */
  fields?: ScriptName[];
  /** A unit's or row's words (seam, material, bench), read only. */
  labels?: string[];
  /** The parameter columns `p` reads. */
  parameters?: ScriptName[];
  /** A table script's table's columns, for `t["…"]`. */
  columns?: ScriptName[];
  /** The project's scripts, for `load("…")`. */
  scripts?: string[];
  /** The project's parameter tables, for `lookup("…", "…", key=…)` (Schedule quantities plan, decision 1). */
  tables?: { name: string; columns: ScriptName[]; filters: string[] }[];
  calls?: ScriptCalls | null;
}

/** A name as it's written inside a Starlark string: each `\` doubled. */
const inString = (name: string) => name.replaceAll('\\', '\\\\');

/** The names `def formula(unit, p)` or `def run(t, p)` gives its handle and parameters. */
function handles(doc: string, kind: ScriptNames['kind']): { handle: string | null; p: string | null } {
  const m = new RegExp(`^\\s*def\\s+${kind === 'table' ? 'run' : 'formula'}\\s*\\(([^)]*)\\)`, 'm').exec(doc);
  const args = m ? m[1].split(',').map((a) => a.split('=')[0].trim()).filter(Boolean) : [];
  return { handle: args[0] ?? null, p: args[1] ?? null };
}

const call = (c: ScriptCallInfo, type: string): Completion => ({ label: c.name, type, detail: type === 'method' || type === 'property' ? '' : 'function', info: `${c.signature}\n\n${c.what}` });
const named = (n: ScriptName, type: string, detail?: string): Completion => ({ label: n.name, type, detail: detail ?? n.unit ?? '' });

/** The completion source: what fits where the cursor is. */
export function scriptCompletions(names: () => ScriptNames) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    const n = names();
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const { handle, p } = handles(ctx.state.doc.toString(), n.kind);
    const calls = n.calls;

    // lookup("…"): the project's parameter tables; then the table's columns; then the keys it filters on.
    let m = /(?:lookup|interpolate)\(\s*"([^"]*)$/.exec(before);
    if (m) return { from: ctx.pos - m[1].length, options: (n.tables ?? []).map((t) => ({ label: t.name, type: 'namespace', detail: 'parameter table', info: `columns: ${t.columns.map((c) => c.name).join(', ') || 'none'}\nfilters: ${t.filters.join(', ') || 'none'}` })), validFor: /^[^"]*$/ };
    // interpolate("…", value, x="…", y="…"): the curve's columns; then its keys, x=, y= and ends=.
    m = /interpolate\(\s*"([^"]+)"[^()]*\b[xy]\s*=\s*"([^"]*)$/.exec(before);
    if (m) {
      const t = (n.tables ?? []).find((x) => x.name === m![1]);
      return t ? { from: ctx.pos - m[2].length, options: t.columns.map((c) => named(c, 'property')), validFor: /^[^"]*$/ } : null;
    }
    m = /interpolate\(\s*"([^"]+)"\s*,[^()]*,\s*(\w*)$/.exec(before);
    if (m) {
      const t = (n.tables ?? []).find((x) => x.name === m![1]);
      if (t) return { from: ctx.pos - m[2].length, options: [...['x=', 'y='].map((k) => ({ label: k, type: 'keyword', detail: 'column' })), { label: 'ends=', type: 'keyword', detail: '"refuse" or "hold"' }, ...t.filters.map((f) => ({ label: `${f}=`, type: 'variable', detail: 'key' }))], validFor: /^\w*=?$/ };
    }
    m = /lookup\(\s*"([^"]+)"\s*,\s*"([^"]*)$/.exec(before);
    if (m) {
      const t = (n.tables ?? []).find((x) => x.name === m![1]);
      return t ? { from: ctx.pos - m[2].length, options: t.columns.map((c) => named(c, 'property')), validFor: /^[^"]*$/ } : null;
    }
    m = /lookup\(\s*"([^"]+)"\s*,\s*"[^"]*"\s*(?:,[^()]*)?,\s*(\w*)$/.exec(before);
    if (m) {
      const t = (n.tables ?? []).find((x) => x.name === m![1]);
      if (t) return { from: ctx.pos - m[2].length, options: [...t.filters.map((f) => ({ label: `${f}=`, type: 'variable', detail: 'key' })), { label: 'default=', type: 'keyword', detail: 'where no row fits' }], validFor: /^\w*=?$/ };
    }
    // check(…) and required(…) in a table script (decision 0101, workstream 3): a column's name in quotes is the table's;
    // after a comma, their own words.
    if (n.kind === 'table') {
      const cols = () => (n.columns ?? []).map((c) => named({ name: inString(c.name), unit: c.unit }, 'property'));
      m = /\brequired\(\s*"([^"]*)$/.exec(before) ?? /\bcheck\(\s*"[^"]*"\s*,(?:[^()]*?,)?\s*(?:equals\s*=\s*)?"([^"]*)$/.exec(before);
      if (m) return { from: ctx.pos - m[1].length, options: cols(), validFor: /^[^"]*$/ };
      m = /\brequired\(\s*"[^"]*"\s*(?:,[^()]*)?,\s*(\w*)$/.exec(before);
      if (m) return { from: ctx.pos - m[1].length, options: [{ label: 'material=', type: 'keyword', detail: '"coal" or "waste"' }, { label: 'name=', type: 'keyword', detail: "the check's name" }], validFor: /^\w*=?$/ };
      m = /\bcheck\(\s*"[^"]*"\s*,[^()]*,\s*(\w+)$/.exec(before);
      if (m) return { from: ctx.pos - m[1].length, options: [{ label: 'equals=', type: 'keyword', detail: 'what the parts add to' }, { label: 'tolerance=', type: 'keyword', detail: 'how far out a leaf may be' }], validFor: /^\w*=?$/ };
    }
    // load("…"): the project's scripts.
    m = /load\(\s*"([^"]*)$/.exec(before);
    if (m) return { from: ctx.pos - m[1].length, options: (n.scripts ?? []).map((s) => ({ label: s, type: 'namespace', detail: 'script' })), validFor: /^[^"]*$/ };
    // x["…"]: a formula's fields and labels by name, or a table's columns.
    m = /(\w+)((?:\.\w+)*)\[\s*"([^"]*)$/.exec(before);
    if (m) {
      const [, base, chain, typed] = m;
      let options: Completion[] = [];
      if (n.kind === 'formula' && base === handle) {
        options = chain ? (n.fields ?? []).map((f) => named({ name: inString(f.name), unit: f.unit }, 'property')) : [...(n.fields ?? []).map((f) => named({ name: inString(f.name), unit: f.unit }, 'property')), ...(n.labels ?? []).map((l) => named({ name: inString(l) }, 'constant', 'words'))];
      } else if (n.kind === 'table' && base !== p) {
        options = (n.columns ?? []).map((c) => named({ name: inString(c.name), unit: c.unit }, 'property'));
      }
      return options.length ? { from: ctx.pos - typed.length, options, validFor: /^[^"]*$/ } : null;
    }
    // x.… : a unit's fields and parts, the parameters, a table's calls.
    m = /(\w+)((?:\.\w+)*)\.(\w*)$/.exec(before);
    if (m) {
      const [, base, chain, typed] = m;
      let options: Completion[] = [];
      if (p && base === p && !chain) {
        options = (n.parameters ?? []).map((x) => named(x, 'constant'));
      } else if (n.kind === 'formula' && base === handle) {
        const part = chain.slice(1);
        if (!part) options = [...(n.fields ?? []).filter((f) => /^\w+$/.test(f.name)).map((f) => named(f, 'property')), ...(n.labels ?? []).filter((l) => /^\w+$/.test(l)).map((l) => named({ name: l }, 'constant', 'words')), ...(calls?.unitParts ?? []).map((c) => call(c, 'namespace'))];
        else if (['section', 'column', 'above', 'rom_coal'].includes(part)) options = (n.fields ?? []).filter((f) => /^\w+$/.test(f.name)).map((f) => named(f, 'property'));
      } else if (n.kind === 'table' && !chain && !/^\d/.test(base)) {
        // Any name a table script holds may be a table (t, s = t.group(...), j = ...): its calls.
        options = (calls?.table ?? []).map((c) => call(c, ['rows', 'columns', 'name'].includes(c.name) ? 'property' : 'method'));
      }
      return options.length ? { from: ctx.pos - typed.length, options, validFor: /^\w*$/ } : null;
    }
    // A word: the library, Starlark's own, a table script's functions, and the script's own names.
    const word = ctx.matchBefore(/\w+/);
    if (!word && !ctx.explicit) return null;
    if (word && /\.$/.test(before.slice(0, word.from - line.from))) return null;
    const own = await localCompletionSource(ctx);
    const options: Completion[] = [
      ...(calls?.library ?? []).map((c) => call(c, 'function')),
      ...(n.kind === 'table' ? (calls?.tableGlobals ?? []).map((c) => call(c, 'function')) : []),
      ...(calls?.starlark ?? []).map((c) => call(c, 'function')),
      ...(own?.options ?? []),
    ];
    return { from: word ? word.from : ctx.pos, options, validFor: /^\w*$/ };
  };
}

/** Hover help: a call under the pointer says how it's called and what it gives. */
export function scriptHover(names: () => ScriptNames) {
  return hoverTooltip((view: EditorView, pos: number): Tooltip | null => {
    const n = names();
    const calls = n.calls;
    if (!calls) return null;
    const line = view.state.doc.lineAt(pos);
    const at = pos - line.from;
    let start = at;
    let end = at;
    while (start > 0 && /\w/.test(line.text[start - 1])) start--;
    while (end < line.text.length && /\w/.test(line.text[end])) end++;
    if (start === end) return null;
    const word = line.text.slice(start, end);
    const afterDot = line.text[start - 1] === '.';
    const { handle } = handles(view.state.doc.toString(), n.kind);
    const pool = afterDot
      ? n.kind === 'table'
        ? calls.table
        : n.kind === 'formula' && new RegExp(`\\b${handle}\\.$`).test(line.text.slice(0, start))
          ? calls.unitParts
          : []
      : [...calls.library, ...(n.kind === 'table' ? calls.tableGlobals : []), ...calls.starlark];
    const found = pool.find((c) => c.name === word);
    if (!found) return null;
    return {
      pos: line.from + start,
      end: line.from + end,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'cm-script-help';
        const sig = document.createElement('div');
        sig.className = 'cm-script-help-signature';
        sig.textContent = found.signature;
        const what = document.createElement('div');
        what.textContent = found.what;
        dom.append(sig, what);
        return { dom };
      },
    };
  });
}
