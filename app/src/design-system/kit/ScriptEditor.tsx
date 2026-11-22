import { autocompletion } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { python } from '@codemirror/lang-python';
import { bracketMatching, HighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { type Diagnostic, setDiagnostics } from '@codemirror/lint';
import { EditorState, RangeSetBuilder, StateEffect, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, drawSelection, EditorView, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumbers } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { useEffect, useRef, useState } from 'react';
import { scriptHelp } from '@/bridge';
import { cn } from 'cn';
import { type ScriptCalls, type ScriptNames, scriptCompletions, scriptHover } from './scriptCompletions';

/**
 * A script box (decision 0077, 7d): CodeMirror 6 with Starlark's syntax (Python's), line numbers and undo. A problem is
 * underlined in red on its line with why on hover; the lines that ran for the picked block are tinted in the picked gold.
 * Colours from the theme's tokens, so it reads as the rest of the app in light and dark. Given `names`, it offers what
 * the script can call and read as it's typed (Ctrl+Space asks), and a call under the pointer says how it's called and what
 * it gives (decision 0099).
 */

/** A problem to show: its line (from 1; none, the first) and why. */
export interface ScriptProblem {
  line?: number | null;
  message: string;
}

const setRan = StateEffect.define<number[]>();

const ranLines = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(lines, tr) {
    for (const e of tr.effects) {
      if (e.is(setRan)) {
        const b = new RangeSetBuilder<Decoration>();
        const doc = tr.state.doc;
        for (const n of [...new Set(e.value)].sort((a, c) => a - c)) {
          if (n >= 1 && n <= doc.lines) b.add(doc.line(n).from, doc.line(n).from, Decoration.line({ class: 'cm-ran' }));
        }
        return b.finish();
      }
    }
    return tr.docChanged ? Decoration.none : lines.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

const look = EditorView.theme({
  '&': { fontSize: '12px', backgroundColor: 'transparent', color: 'var(--foreground)', height: '100%' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '1.5' },
  '.cm-content': { caretColor: 'var(--foreground)' },
  '.cm-gutters': { backgroundColor: 'transparent', color: 'var(--muted-foreground)', border: 'none' },
  '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'color-mix(in oklch, var(--foreground) 5%, transparent)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: 'color-mix(in oklch, var(--edited) 30%, transparent) !important' },
  '.cm-ran': { backgroundColor: 'var(--selection)' },
  '.cm-lintRange-error': { backgroundImage: 'none', textDecoration: 'underline wavy var(--destructive)' },
  '.cm-diagnostic-error': { borderLeftColor: 'var(--destructive)' },
  '.cm-tooltip': { backgroundColor: 'var(--card)', color: 'var(--card-foreground)', border: '1px solid var(--border)' },
  '.cm-tooltip.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--font-mono)', fontSize: '12px', maxHeight: '14em' },
  '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: 'var(--selection)', color: 'var(--primary)' },
  '.cm-completionDetail': { color: 'var(--muted-foreground)', fontStyle: 'normal', marginLeft: '1em' },
  '.cm-completionMatchedText': { textDecoration: 'none', fontWeight: '600' },
  '.cm-tooltip.cm-completionInfo, .cm-script-help': { maxWidth: '28em', padding: '4px 8px', whiteSpace: 'pre-wrap', fontSize: '12px' },
  '.cm-script-help-signature': { fontFamily: 'var(--font-mono)', marginBottom: '2px' },
});

const highlight = HighlightStyle.define([
  { tag: tags.comment, color: 'var(--muted-foreground)', fontStyle: 'italic' },
  { tag: [tags.keyword, tags.controlKeyword, tags.definitionKeyword], fontWeight: '600' },
  { tag: [tags.string, tags.number, tags.bool, tags.null], color: 'color-mix(in oklch, var(--foreground) 75%, var(--edited))' },
  { tag: tags.function(tags.variableName), textDecoration: 'underline', textDecorationColor: 'var(--border)' },
]);

export function ScriptEditor({
  value,
  onChange,
  problems = [],
  ran = [],
  label,
  className,
  names,
}: {
  value: string;
  onChange: (value: string) => void;
  problems?: ScriptProblem[];
  ran?: number[];
  label: string;
  className?: string;
  /** What it offers as it's typed: the script's kind and the names it reads. */
  names?: Omit<ScriptNames, 'calls'>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [calls, setCalls] = useState<ScriptCalls | null>(null);
  useEffect(() => {
    void scriptHelp().then(setCalls).catch(() => setCalls(null));
  }, []);
  // Read when asked, so names that come later (a check, a table picked) are offered without making the box again.
  const offered = useRef<ScriptNames>({ kind: 'library' });
  offered.current = { kind: 'library', ...names, calls };
  const view = useRef<EditorView | null>(null);
  const changed = useRef(onChange);
  changed.current = onChange;

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          highlightActiveLine(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          python(),
          syntaxHighlighting(highlight),
          autocompletion({ override: [scriptCompletions(() => offered.current)], icons: false, activateOnTypingDelay: 80 }),
          scriptHover(() => offered.current),
          keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
          ranLines,
          look,
          EditorView.contentAttributes.of({ 'aria-label': label }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) changed.current(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // The editor is made once; the text from outside is put in below.
  }, []);

  // A value set from outside (a different formula, Discard) replaces the text; one being typed is already there.
  useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== value) v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
  }, [value]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const doc = v.state.doc;
    const diagnostics: Diagnostic[] = problems.map((p) => {
      const line = doc.line(Math.min(Math.max(p.line ?? 1, 1), doc.lines));
      return { from: line.from, to: Math.max(line.to, line.from), severity: 'error', message: p.message };
    });
    v.dispatch(setDiagnostics(v.state, diagnostics));
  }, [problems]);

  useEffect(() => {
    view.current?.dispatch({ effects: setRan.of(ran) });
  }, [ran]);

  return <div ref={host} data-slot="script-editor" className={cn('min-h-0 overflow-hidden rounded-md border border-input bg-input/20 focus-within:border-ring', className)} />;
}
