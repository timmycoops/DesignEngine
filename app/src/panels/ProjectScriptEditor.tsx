import { Bot, Play } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { formulaReference, type ObjectSummary, objectDetails, scriptCheck } from '@/bridge';
import { Choice, Field, PanelHead, ScriptEditor, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { useWorkspace } from '@/workspace/context';
import { guideFor, useParameterTables, useProjectScripts, useTableColumns } from './scriptNames';

const STARTER = `# Functions a formula or a calculated column loads by this script's name:
#   load("qualities", "by_tonnes")
def by_tonnes(values, tonnes):
    total = 0.0
    weighed = 0.0
    for v, t in zip(values, tonnes):
        total += t
        weighed += v * t
    return weighed / total if total else None
`;

/**
 * A script of the project (Tables and scripts plan, decision 9): its name, which a `load` asks for, a line on what it's
 * for, and its Starlark, checked as it's typed against the project's other scripts, every problem on its line. Save is
 * one `script.save` run, the next revision of one already there. A dataflow or calculated column that loads it is bound
 * to it by name when it's saved, and a run names the revision it read.
 */

/** Run on the Objects table (Table setup plan, decision 10), not a table of the project. */
const OBJECTS = 'objects';
export function ProjectScriptEditor({ object, onSaved }: { object: ObjectSummary | null; onSaved: (id: string) => void }) {
  const { runRecord, summary, select, openPanel } = useWorkspace();
  const [name, setName] = useState(object?.name ?? 'qualities');
  const [note, setNote] = useState('');
  const [source, setSource] = useState(object ? '' : STARTER);
  const [loaded, setLoaded] = useState(!object);
  const [problems, setProblems] = useState<{ message: string; line: number | null }[]>([]);
  const [said, setSaid] = useState<{ error?: string; text?: string }>({});
  const [reference, setReference] = useState('');
  // What's saved, so a table script runs only as saved (decision 0099, workstream 7).
  const [savedSource, setSavedSource] = useState<string | null>(null);
  const [runOn, setRunOn] = useState('');
  const [into, setInto] = useState<'new' | 'columns'>('new');
  const isTable = /^\s*def\s+run\s*\(/m.test(source);
  const tables = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own), [summary]);
  // The Objects table too (Table setup plan, decision 10): every solid's row, its calculated columns worked out.
  const on = runOn === OBJECTS || tables.some((t) => t.id === runOn) ? runOn : (tables[tables.length - 1]?.id ?? OBJECTS);
  const columns = useTableColumns(isTable && on !== OBJECTS ? on : null);
  const scripts = useProjectScripts().filter((s) => s !== name.trim());
  const parameterTables = useParameterTables();

  useEffect(() => {
    void formulaReference().then(setReference);
    if (!object) return;
    let live = true;
    void objectDetails(object.id).then((d) => {
      if (!live || d.measures.kind !== 'script') return;
      setSource(d.measures.script.source);
      setSavedSource(d.measures.script.source);
      setNote(d.measures.script.note ?? '');
      setLoaded(true);
    });
    return () => {
      live = false;
    };
  }, []);
  // Checked as it's typed: every problem on its line.
  useEffect(() => {
    if (!loaded) return;
    let live = true;
    const t = setTimeout(() => {
      void scriptCheck(name, source, object?.id ?? null)
        .then((p) => live && setProblems(p))
        .catch((e) => live && setSaid({ error: String(e) }));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [name, source, loaded]);

  const save = async () => {
    const outcome = await runRecord('script.save', { script: object?.id ?? null, name: name.trim(), source, note });
    if (outcome.error) return setSaid({ error: outcome.error });
    const made = outcome.record?.outputs?.[0]?.id ?? object?.id;
    setSaid({ text: `${name.trim()} saved: what loads it reads it as it is now` });
    setSavedSource(source);
    if (made) onSaved(made);
  };
  // A table script run as a task, `table.script`, as saved: a new table shown in the Tables tab, or columns on this one.
  const runTable = async () => {
    if (!object || !on) return;
    const outcome = await runRecord('table.script', on === OBJECTS ? { objects: true, script: object.id } : { table: on, script: object.id, into });
    if (outcome.error) return setSaid({ error: outcome.error });
    const made = outcome.record?.outputs?.[0]?.id ?? on;
    setSaid({ text: outcome.record?.messages?.join('; ') || 'Ran' });
    select(made);
    openPanel('restable');
  };
  const copy = () => {
    const text = [
      reference.trim(),
      '',
      '## This script',
      isTable
        ? `A table script of the project named \`${name.trim()}\`: \`def run(t, p):\` run on a whole table, giving back a table.`
        : `A script of the project named \`${name.trim()}\`: functions a formula or a calculated column loads, \`load("${name.trim()}", "function")\`. It has no unit or row of its own.`,
      note ? `What it's for: ${note}` : '',
      '```python',
      source.trimEnd(),
      '```',
      '',
      'Write or change the functions I describe below, as Starlark in this form.',
    ];
    void navigator.clipboard.writeText(text.join('\n')).then(() => setSaid({ text: 'Copied for an AI: how formulas are written and this script, no values' }));
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 p-2" data-slot="project-script">
      <PanelHead line="Functions written once, loaded by name from any formula or calculated column" more="A dataflow or calculated column that loads this script is bound to it when it is saved; each run names the revision it read." />
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <Field label="Name">
          <Input className="h-7 w-40 px-1.5 text-xs" value={name} aria-label="The script's name, what a load asks for" onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="For">
          <Input className="h-7 w-72 px-1.5 text-xs" value={note} aria-label="What the script is for" placeholder="qualities added up by tonnes" onChange={(e) => setNote(e.target.value)} />
        </Field>
        <span className="text-muted-foreground">
          load(&quot;{name.trim() || 'name'}&quot;, &quot;function&quot;)
        </span>
        <span className="ml-auto flex items-center gap-1">
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={copy} title="Copy how formulas are written and this script for an AI: no values">
            <Bot className="size-3.5" /> Copy for AI
          </Button>
          <Button size="sm" className="h-6 px-2 text-xs" disabled={problems.length > 0 || !name.trim() || !loaded} onClick={() => void save()} title="Save it: one run, its next revision; what loads it reads it as it is now">
            Save
          </Button>
        </span>
      </div>
      {isTable && (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1" data-slot="run-table-script">
          <Field label="Run on">
            <Select className="w-56" value={on} aria-label="The table it runs on" onChange={(e) => setRunOn(e.target.value)}>
              <option value={OBJECTS}>Objects</option>
              {tables.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </Select>
          </Field>
          {on !== OBJECTS && <Choice value={into} options={['new', 'columns'] as const} labels={{ new: 'New table', columns: 'Columns on it' }} label="Where its table goes" onChange={setInto} />}
          <Button
            size="sm"
            variant="outline"
            className="h-6 px-2 text-xs"
            disabled={!object || !on || source !== savedSource || problems.length > 0}
            onClick={() => void runTable()}
            title={!object || source !== savedSource ? 'Save it first: a table script runs as saved' : 'Run it on the table: one run, recorded, one undo'}
          >
            <Play className="size-3.5" /> Run
          </Button>
        </div>
      )}
      <ScriptEditor value={source} onChange={setSource} problems={problems} label="The script's Starlark" className="min-h-40 flex-1" names={{ kind: isTable ? 'table' : 'library', columns, scripts, tables: parameterTables }} />
      {problems.length > 0 && (
        <div className="text-destructive" data-slot="script-problems">
          {problems.map((p, i) => (
            <div key={i}>{p.message}</div>
          ))}
        </div>
      )}
      {said.error ? <span className="text-destructive">{said.error}</span> : said.text ? <span className="text-ok">{said.text}</span> : null}
      <details className="shrink-0 text-muted-foreground" data-slot="script-guide">
        <summary className="cursor-pointer">{isTable ? 'How table scripts are written' : 'How formulas and their functions are written'}</summary>
        <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-xs">{guideFor(reference, isTable)}</pre>
      </details>
    </div>
  );
}
