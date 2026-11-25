import { Check, Play, Trash2, Waypoints, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { type Dataflow, type DataflowStep, type FormulaReport, type FormulaTest, type FormulaTrace, formulaCheck, formulaReference, formulaTrace } from '@/bridge';
import { readWhen, whenText } from './formulaText';
import { CellInput, IconButton, ScriptEditor, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { formulaNames, useParameterTables, useProjectScripts } from './scriptNames';
import { CopyForAi } from './CopyForAi';

/**
 * A formula step edited (decision 0077, 7d): its name and filter, its Starlark in a script box with each problem on its
 * line, its worked examples held or not, the formula run on the picked block's unit line by line, and Copy for AI and
 * Explain, which hand an AI the formula and the dataflow's names, never a value. Saved with the dataflow.
 */

export function FormulaEditor({
  flow,
  flowId,
  index,
  saved,
  onStep,
  onRemove,
  onClose,
}: {
  flow: Dataflow;
  /** The saved dataflow's id: Show in Dataflow picks this step in the Dataflow tab, which runs on the edit as it's typed. */
  flowId: string;
  index: number;
  saved?: DataflowStep;
  onStep: (s: DataflowStep) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  const { summary, selection, runInNotebook } = useWorkspace();
  const scripts = useProjectScripts();
  const parameterTables = useParameterTables();
  const step = flow.steps[index];
  const formula = step.formula ?? { script: '' };
  const [report, setReport] = useState<FormulaReport | null>(null);
  const [checkWhy, setCheckWhy] = useState<string | null>(null);
  const [trace, setTrace] = useState<FormulaTrace | null>(null);
  const [traceWhy, setTraceWhy] = useState<string | null>(null);
  const [tracing, setTracing] = useState(false);
  const [testsText, setTestsText] = useState<string | null>(null);
  const [reference, setReference] = useState('');

  useEffect(() => {
    void formulaReference().then(setReference);
  }, []);

  // Checked as it's typed: problems on their lines, worked examples held or not.
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      formulaCheck(flow, index)
        .then((r) => live && (setReport(r), setCheckWhy(null)))
        .catch((e) => live && setCheckWhy(String(e)));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [flow, index]);

  // A trace is of the formula as it was run: an edit puts it away.
  useEffect(() => setTrace(null), [formula.script, formula.when]);

  const solids = useMemo(() => new Map((summary?.objects ?? []).filter((o) => o.kind === 'solid').map((o) => [o.id, o.name])), [summary]);
  const solid = [...selection].reverse().find((s) => solids.has(s)) ?? null;
  const set = (change: Partial<NonNullable<DataflowStep['formula']>>) => onStep({ ...step, formula: { ...formula, ...change } });
  const edited = JSON.stringify(saved) !== JSON.stringify(step);
  const failing = report?.tests.filter((t) => !t.ok).length ?? 0;
  const ok = report && report.problems.length === 0 && failing === 0;

  const run = async () => {
    if (!solid) return;
    setTracing(true);
    setTraceWhy(null);
    try {
      setTrace(await formulaTrace(flow, step.step, solid));
    } catch (e) {
      setTrace(null);
      setTraceWhy(String(e));
    } finally {
      setTracing(false);
    }
  };
  const testsJson = testsText ?? JSON.stringify(formula.tests ?? [], null, 2);
  const testsWhy = (() => {
    if (testsText === null) return null;
    try {
      const v = JSON.parse(testsText);
      return Array.isArray(v) ? null : 'worked examples are a list';
    } catch (e) {
      return String(e).replace(/^SyntaxError: /, '');
    }
  })();

  return (
    <div className="flex min-h-0 flex-col gap-2" data-slot="formula-editor" data-step={step.step}>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="font-semibold">Formula</span>
        <CellInput value={step.step} label="The formula step's name" className="w-44 flex-none font-semibold" onChange={(name) => onStep({ ...step, step: name })} />
        <span className="text-muted-foreground">after {report?.stage?.replaceAll('_', ' ') ?? '…'}, on</span>
        <CellInput
          value={whenText(formula.when)}
          placeholder="every unit"
          label="Which units it runs on, in the bench rules' words: material: coal; seam: U, M"
          className="w-56 flex-none"
          onChange={(text) => set({ when: readWhen(text) })}
        />
        {edited && <StatusBadge tone="edited">edited</StatusBadge>}
        {ok && (
          <span className="flex items-center gap-0.5 text-ok" data-slot="formula-ok">
            <Check className="size-3.5" /> checked{report.tests.length ? `, ${report.tests.length} worked example${report.tests.length === 1 ? '' : 's'} hold` : ''}
          </span>
        )}
        <span className="ml-auto flex items-center gap-1">
          <Button
            variant="outline"
            size="sm"
            className="h-6 px-2 text-xs"
            onClick={() => runInNotebook({ dataflowId: flowId, dataflow: flow, step: step.step })}
            title="This step in the Reserves case tab, which runs the whole case on the formula as it's typed, before it's saved: the flow and every step after it show what the edit does. Nothing is kept."
          >
            <Waypoints className="size-3.5" /> Show in the case
          </Button>
          <CopyForAi brief={{ kind: 'formula', dataflow: flowId, content: flow, source: step.formula?.script ?? '', name: step.step }} problems={report?.problems ?? []} />
          <IconButton label="Take this formula step out" className="size-6" onClick={onRemove}>
            <Trash2 />
          </IconButton>
          <IconButton label="Back to the fields" className="size-6" onClick={onClose}>
            <X />
          </IconButton>
        </span>
      </div>
      <div className="flex min-h-0 gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <ScriptEditor
            value={formula.script}
            onChange={(script) => set({ script })}
            problems={report?.problems ?? []}
            ran={trace?.lines.map((l) => l.line) ?? []}
            label={`The ${step.step} formula`}
            className="min-h-72 flex-1"
            names={{ ...formulaNames(report?.names, scripts), tables: parameterTables }}
          />
          {checkWhy && <div className="text-destructive">{checkWhy}</div>}
          {report?.problems.map((p, i) => (
            <div key={i} className="text-destructive" data-slot="formula-problem">
              {p.line ? `line ${p.line}: ` : ''}
              {p.message}
            </div>
          ))}
        </div>
        <div className="flex w-80 shrink-0 flex-col gap-1">
          <div className="flex items-center gap-1">
            <span className="font-semibold">On the picked block</span>
            <Button size="sm" className="ml-auto h-6 px-2 text-xs" disabled={!solid || tracing || (report?.problems.length ?? 0) > 0} onClick={() => void run()} title={solid ? `Run the formula as written on ${solids.get(solid)}'s unit, nothing kept` : 'Pick a block in the view'}>
              <Play className="size-3.5" /> {tracing ? 'Running…' : 'Check'}
            </Button>
          </div>
          {!solid && <div className="text-muted-foreground">Pick a block in the view to run the formula on its unit, line by line.</div>}
          {traceWhy && <div className="text-destructive">{traceWhy}</div>}
          {trace && <TraceView trace={trace} />}
        </div>
      </div>
      <section>
        <div className="flex items-center gap-1">
          <span className="font-semibold">Worked examples</span>
          <span className="text-muted-foreground">run on every save; a unit's fields and parameters, and what the formula must leave it with</span>
        </div>
        {report?.tests.map((t) => (
          <div key={t.name} className={cn('flex items-center gap-1', t.ok ? 'text-ok' : 'text-destructive')} data-slot="formula-test" data-ok={t.ok}>
            {t.ok ? <Check className="size-3.5" /> : <X className="size-3.5" />}
            <span>{t.name}</span>
            {t.why && <span>: {t.why}</span>}
          </div>
        ))}
        <textarea
          className={cn('mt-1 h-28 w-full rounded-md border border-input bg-input/20 p-1.5 font-mono text-xs outline-none focus-visible:border-ring', testsWhy && 'border-destructive')}
          aria-label="The worked examples, as JSON"
          spellCheck={false}
          value={testsJson}
          onChange={(e) => {
            setTestsText(e.target.value);
            try {
              const v = JSON.parse(e.target.value);
              if (Array.isArray(v)) set({ tests: v as FormulaTest[] });
            } catch {
              // Held as typed until it reads.
            }
          }}
        />
        {testsWhy && <div className="text-destructive">{testsWhy}</div>}
      </section>
      <details className="text-muted-foreground">
        <summary className="cursor-pointer">How formulas are written</summary>
        <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-xs">{reference}</pre>
      </details>
    </div>
  );
}

/** The formula run on the picked block's unit: what it found, each line that ran with its values, what it read and set. */
function TraceView({ trace }: { trace: FormulaTrace }) {
  const shown = (v: number | null) => (v === null ? 'none' : Math.abs(v) >= 1000 ? v.toLocaleString(undefined, { maximumFractionDigits: 1 }) : String(Number(v.toPrecision(6))));
  return (
    <div className="flex min-h-0 flex-col gap-1 overflow-auto" data-slot="formula-trace">
      <div>
        <span className="text-primary">{trace.block}</span> <span className="text-muted-foreground">({trace.unit})</span>
      </div>
      {!trace.fits && <div className="text-warning">This unit is outside the formula's filter: it wouldn't run here. Shown as if it did.</div>}
      {trace.stopped && (
        <div className="text-destructive" data-slot="formula-stopped">
          Stopped{trace.stopped.line ? ` at line ${trace.stopped.line}` : ''}: {trace.stopped.message}
        </div>
      )}
      <div className="font-medium">Lines that ran</div>
      {trace.lines.length === 0 && <div className="text-muted-foreground">none</div>}
      {trace.lines.map((l, i) => (
        <div key={i} className="font-mono" data-line={l.line}>
          <span className="text-muted-foreground">{l.line}</span> {l.text}
          {l.values.length > 0 && <div className="pl-4 text-primary">{l.values.map(([k, v]) => `${k} = ${v}`).join(', ')}</div>}
        </div>
      ))}
      {trace.reads.length > 0 && (
        <>
          <div className="font-medium">Read</div>
          <div className="font-mono">{[...new Map(trace.reads).entries()].map(([k, v]) => `${k} = ${v}`).join(', ')}</div>
        </>
      )}
      <div className="font-medium">Set</div>
      <div className="font-mono text-edited" data-slot="formula-sets">
        {trace.sets.length ? trace.sets.map(([k, v]) => `${k} = ${v}`).join(', ') : 'nothing'}
      </div>
      {trace.said.length > 0 && (
        <>
          <div className="font-medium">Printed</div>
          {trace.said.map((s, i) => (
            <div key={i}>{s}</div>
          ))}
        </>
      )}
      <details>
        <summary className="cursor-pointer text-muted-foreground">The unit before it</summary>
        <div className="font-mono">
          {trace.before
            .filter(([, v]) => v !== null)
            .map(([k, v]) => `${k} ${shown(v)}`)
            .join(', ')}
        </div>
      </details>
    </div>
  );
}
