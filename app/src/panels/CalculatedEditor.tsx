import { Play, Trash2, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { type CalcAddUp, type CalcCheck, type CalculatedColumn, calcCheck, calcParameters, calcTrace, type FormulaTrace } from '@/bridge';
import { Choice, Field, ScriptEditor, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { formulaNames, useParameterTables, useProjectScripts } from './scriptNames';
import { starterScript } from './tableGrid';
import { CopyForAi } from './CopyForAi';

type By = CalcAddUp['by'];
const BYS: readonly By[] = ['from_inputs', 'sum', 'max', 'min', 'weighted_by'];
const BY_LABELS: Record<By, string> = { from_inputs: 'From its inputs', sum: 'Sum', max: 'Largest', min: 'Smallest', weighted_by: 'Weighted by' };

/**
 * A calculated column edited (Tables and scripts plan, decision 6): its name and unit, how a group adds it up, its
 * Starlark in the script box with each problem on its line as it's typed, the formula run on the picked leaf line by
 * line, and Copy for AI and Explain, which hand an AI the table's names and this formula, never a value. Save puts it on
 * the table, one `table.calculate` run; the grid works it out when it's shown.
 */
export function CalculatedEditor({
  table,
  numbers,
  editing,
  picked,
  onClose,
}: {
  /** The reserves table; null, the Objects table (Table setup plan, decision 9): a formula on each solid's row. */
  table: string | null;
  /** The table's number columns, for Weighted by. */
  numbers: string[];
  /** The column changed, or none for a new one. */
  editing: CalculatedColumn | null;
  /** The leaf to trace on: its row and its words; on the Objects table, the solid. */
  picked: { row: number; label: string; solid?: string } | null;
  onClose: () => void;
}) {
  const { runRecord, summary } = useWorkspace();
  const scripts = useProjectScripts();
  const parameterTables = useParameterTables();
  const first = table ? 'strip_ratio' : 'coal_tonnes';
  const [name, setName] = useState(editing?.name ?? first);
  const [unit, setUnit] = useState(editing?.unit ?? '');
  const [by, setBy] = useState<By>(editing?.adds.by ?? 'from_inputs');
  const [weight, setWeight] = useState<string>(editing?.adds.by === 'weighted_by' ? editing.adds.column : (numbers[0] ?? ''));
  const [script, setScript] = useState(editing?.script ?? (table ? starterScript(first) : `def formula(row, p):
    # A solid's row of the Objects table: its attributes and measures, worked when shown, never stored.
    row.${first} = row.volume * row.rd
`));
  const [parameters, setParameters] = useState<string[]>(editing?.parameters ?? []);
  const [check, setCheck] = useState<CalcCheck | null>(null);
  const [trace, setTrace] = useState<FormulaTrace | null>(null);
  const [said, setSaid] = useState<{ error?: string; text?: string }>({});

  useEffect(() => {
    // A new column reads the parameter tables its table's dataflow reads.
    if (!editing) void calcParameters(table).then((list) => setParameters(list.map(([id]) => id)));
  }, []);

  const adds: CalcAddUp = by === 'weighted_by' ? { by, column: weight } : { by };
  const column: CalculatedColumn = useMemo(() => ({ name: name.trim(), unit: unit.trim(), script, adds, parameters }), [name, unit, script, by, weight, parameters]);
  // Checked as it's typed: each problem on its line.
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      void calcCheck(table, column)
        .then((c) => live && setCheck(c))
        .catch((e) => live && setSaid({ error: String(e) }));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [table, column]);

  const run = async () => {
    if (!picked) return setSaid({ error: table ? 'Pick a leaf in the grid to run the formula on it' : 'Pick a solid in the grid to run the formula on it' });
    try {
      setTrace(await calcTrace(table, column, picked.row, picked.label, picked.solid ?? null));
      setSaid({});
    } catch (e) {
      setTrace(null);
      setSaid({ error: String(e) });
    }
  };
  const save = async () => {
    const replaces = editing && editing.name !== column.name ? editing.name : null;
    const outcome = await runRecord('table.calculate', table ? { table, column, replaces } : { objects: true, column, replaces });
    if (outcome.error) return setSaid({ error: outcome.error });
    onClose();
  };
  const remove = async () => {
    if (!editing) return;
    // The Objects table's columns are kept in its own table (decision 9).
    const holder = table ?? summary?.objects.find((o) => o.objects_own)?.id;
    if (!holder) return;
    const outcome = await runRecord('table.remove_column', { table: holder, column: editing.name });
    if (outcome.error) return setSaid({ error: outcome.error });
    onClose();
  };

  const problems = check?.problems ?? [];
  const ran = trace?.lines.map((l) => l.line) ?? [];
  return (
    <div className="flex min-h-0 flex-col gap-2 border-b border-border bg-muted/20 p-2" data-slot="calculated-editor">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <Field label="Column">
          <Input className="h-7 w-40 px-1.5 text-xs" value={name} aria-label="The calculated column's name" onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="Unit">
          <Input className="h-7 w-20 px-1.5 text-xs" value={unit} aria-label="Its unit" placeholder="m3/t" onChange={(e) => setUnit(e.target.value)} />
        </Field>
        <Field label="A group">
          <Choice value={by} options={BYS} labels={BY_LABELS} onChange={setBy} label="How a group adds it up: worked again on the group's own inputs (a ratio's way), or each row's value summed, the largest, the smallest, or weighted by a column" />
        </Field>
        {by === 'weighted_by' && (
          <Select value={weight} aria-label="The column it's weighted by" onChange={(e) => setWeight(e.target.value)}>
            {numbers.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </Select>
        )}
        <span className="ml-auto flex items-center gap-1">
          <CopyForAi brief={{ kind: 'calculated', table: table ?? undefined, objects: table === null, source: script, name: column.name }} problems={problems} />
          <button type="button" className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Close the calculated column" onClick={onClose}>
            <X className="size-3.5" />
          </button>
        </span>
      </div>
      <ScriptEditor value={script} onChange={setScript} problems={problems} ran={ran} label="The calculated column's formula" className="max-h-56 min-h-28" names={{ ...formulaNames(check?.names, scripts), tables: parameterTables }} />
      {problems.length > 0 && (
        <div className="text-destructive" data-slot="calculated-problems">
          {problems.map((p, i) => (
            <div key={i}>{p.line ? `Line ${p.line}: ${p.message}` : p.message}</div>
          ))}
        </div>
      )}
      {trace && (
        <div className="flex flex-col gap-0.5 font-mono text-[11px]" data-slot="calculated-trace">
          <span className="font-sans text-muted-foreground">On {trace.block}:</span>
          {trace.lines.map((l, i) => (
            <span key={i}>
              <span className="text-muted-foreground">{l.line}</span> {l.text}
              {l.values.length > 0 && <span className="text-edited"> {l.values.map(([k, v]) => `${k} = ${v}`).join(', ')}</span>}
            </span>
          ))}
          {trace.sets.map(([k, v]) => (
            <span key={k} className="text-ok">
              {k} = {v}
            </span>
          ))}
          {trace.stopped && <span className="text-destructive">{trace.stopped.line ? `Stopped at line ${trace.stopped.line}: ` : 'Stopped: '}{trace.stopped.message}</span>}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={problems.length > 0} onClick={() => void run()} title={picked ? `Run the formula on ${picked.label}, line by line, nothing kept` : 'Pick a leaf in the grid to run the formula on it'}>
          <Play className="size-3.5" /> Run on {picked ? picked.label : 'the picked leaf'}
        </Button>
        {parameters.length > 0 && <span className="text-muted-foreground">p reads {parameters.length} parameter table{parameters.length === 1 ? '' : 's'}</span>}
        <span className="ml-auto flex items-center gap-1">
          {said.error ? <span className="text-destructive">{said.error}</span> : said.text ? <span className="text-ok">{said.text}</span> : null}
          {editing && (
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => void remove()} title="Take this calculated column off the table (one undo puts it back)">
              <Trash2 className="size-3.5" /> Take it out
            </Button>
          )}
          <Button size="sm" className={cn('h-6 px-2 text-xs')} disabled={problems.length > 0 || !column.name} onClick={() => void save()} title="Put it on the table, one run: the grid works it out when it's shown">
            {editing ? 'Save' : 'Put on the table'}
          </Button>
        </span>
      </div>
    </div>
  );
}
