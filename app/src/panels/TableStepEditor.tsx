import { Bot, Trash2, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { type DataflowStep, formulaReference, scriptCheck } from '@/bridge';
import { CellInput, IconButton, PanelHead, ScriptEditor, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { useWorkspace } from '@/workspace/context';
import { guideFor, useParameterColumns, useParameterTables, useProjectScripts, useTableColumns } from './scriptNames';

/** A new table script step's script (decision 0099, workstream 7): each leaf's share of its strip's ROM coal. */
export const TABLE_STEP_TEMPLATE = `# Runs on the case's table after the final writes: the columns it adds are the run's own.
def run(t):
    strips = t.group("strip")
    whole = {s: c for s, c in zip(strips["strip"], strips["ROM\\\\Coal\\\\tonnes"])}
    share = []
    for s, c in zip(t["strip"], t["ROM\\\\Coal\\\\tonnes"]):
        total = whole.get(s)
        share.append(c / total if c != None and total else None)
    return t.with_column("strip_coal_share", share, adds="sum")
`;

/**
 * A table script step of a dataflow edited (Tables and scripts plan, decision 11): its name and its Starlark, `def run(t,
 * p):`, checked as it's typed, each problem on its line. It runs on the case's table once the final writes are done, every
 * run; the columns it adds are the run's own. Saved with the dataflow.
 */
export function TableStepEditor({
  step,
  saved,
  parameterTables,
  onStep,
  onRemove,
  onClose,
}: {
  step: DataflowStep;
  saved?: DataflowStep;
  /** The dataflow's parameter tables, whose columns `p` reads. */
  parameterTables: string[];
  onStep: (s: DataflowStep) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  const { summary } = useWorkspace();
  // The columns offered for t["…"] are the newest table's: what a run of this dataflow writes, near enough.
  const newest = (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own).at(-1)?.id ?? null;
  const columns = useTableColumns(newest);
  const parameters = useParameterColumns(parameterTables);
  const scripts = useProjectScripts();
  const lookupTables = useParameterTables();
  const script = step.table?.script ?? '';
  const [problems, setProblems] = useState<{ message: string; line: number | null }[]>([]);
  const [said, setSaid] = useState<string | null>(null);
  const [reference, setReference] = useState('');
  const edited = JSON.stringify(saved) !== JSON.stringify(step);

  useEffect(() => {
    void formulaReference().then(setReference);
  }, []);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      void scriptCheck(step.step, script, null)
        .then((p) => live && setProblems(p))
        .catch((e) => live && setProblems([{ message: String(e), line: null }]));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [step.step, script]);

  const copy = () => {
    const text = [reference.trim(), '', '## This table script step', `A table script step of a reserves dataflow named \`${step.step}\`, run on the case's table after the final writes.`, '```py', script.trimEnd(), '```', '', 'Write or change it as I describe below, as a table script in this form.'];
    void navigator.clipboard.writeText(text.join('\n')).then(() => setSaid('Copied for an AI: how table scripts are written and this step, no values'));
  };

  return (
    <div className="flex h-full min-h-0 flex-col gap-1" data-slot="table-step">
      <div className="flex items-center gap-1">
        <CellInput value={step.step} label="The step's name" className="w-40 font-semibold" onChange={(name) => onStep({ ...step, step: name.trim().replaceAll(' ', '_') })} />
        {edited && <StatusBadge tone="edited">edited</StatusBadge>}
        <span className="ml-auto flex items-center gap-1">
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={copy} title="Copy how table scripts are written and this step for an AI: no values">
            <Bot className="size-3.5" /> Copy for AI
          </Button>
          <IconButton label="Take this table script step out" className="size-6" onClick={onRemove}>
            <Trash2 />
          </IconButton>
          <IconButton label="Back to the fields" className="size-6" onClick={onClose}>
            <X />
          </IconButton>
        </span>
      </div>
      <PanelHead line="Runs on the case's table after the final writes: the columns it adds are the run's own" more="Grouping, joins and adding up are the table's own calls, done in Rust: t.group, t.where, t.join, t.total, t.with_column. A script never writes over a column the run wrote; that's refused, naming it." />
      <ScriptEditor value={script} onChange={(s) => onStep({ ...step, table: { script: s } })} problems={problems} label="The table script's Starlark" className="min-h-40 flex-1" names={{ kind: 'table', columns, parameters, scripts, tables: lookupTables }} />
      {problems.length > 0 ? (
        <div className="text-destructive" data-slot="table-step-problems">
          {problems.map((p, i) => (
            <div key={i}>{p.line ? `line ${p.line}: ` : ''}{p.message}</div>
          ))}
        </div>
      ) : (
        <span className="text-ok">Checks clean: it runs when the dataflow is saved and run.</span>
      )}
      {said && <span className="text-muted-foreground">{said}</span>}
      <details className="shrink-0 text-muted-foreground" data-slot="script-guide">
        <summary className="cursor-pointer">How table scripts are written</summary>
        <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-xs">{guideFor(reference, true)}</pre>
      </details>
    </div>
  );
}
