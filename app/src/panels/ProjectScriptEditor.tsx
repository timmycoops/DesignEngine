import { Bot } from 'lucide-react';
import { useEffect, useState } from 'react';
import { formulaReference, type ObjectSummary, objectDetails, scriptCheck } from '@/bridge';
import { Field, PanelHead, ScriptEditor } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { useWorkspace } from '@/workspace/context';

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
export function ProjectScriptEditor({ object, onSaved }: { object: ObjectSummary | null; onSaved: (id: string) => void }) {
  const { runRecord } = useWorkspace();
  const [name, setName] = useState(object?.name ?? 'qualities');
  const [note, setNote] = useState('');
  const [source, setSource] = useState(object ? '' : STARTER);
  const [loaded, setLoaded] = useState(!object);
  const [problems, setProblems] = useState<{ message: string; line: number | null }[]>([]);
  const [said, setSaid] = useState<{ error?: string; text?: string }>({});
  const [reference, setReference] = useState('');

  useEffect(() => {
    void formulaReference().then(setReference);
    if (!object) return;
    let live = true;
    void objectDetails(object.id).then((d) => {
      if (!live || d.measures.kind !== 'script') return;
      setSource(d.measures.script.source);
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
    if (made) onSaved(made);
  };
  const copy = () => {
    const text = [
      reference.trim(),
      '',
      '## This script',
      `A script of the project named \`${name.trim()}\`: functions a formula or a calculated column loads, \`load("${name.trim()}", "function")\`. It has no unit or row of its own.`,
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
      <ScriptEditor value={source} onChange={setSource} problems={problems} label="The script's Starlark" className="min-h-40 flex-1" />
      {problems.length > 0 && (
        <div className="text-destructive" data-slot="script-problems">
          {problems.map((p, i) => (
            <div key={i}>{p.message}</div>
          ))}
        </div>
      )}
      {said.error ? <span className="text-destructive">{said.error}</span> : said.text ? <span className="text-ok">{said.text}</span> : null}
    </div>
  );
}
