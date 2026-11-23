import { Trash2, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { type Dataflow, type DataflowStep, formulaReference, scriptCheck } from '@/bridge';
import { CellInput, IconButton, PanelHead, ScriptEditor, StatusBadge } from '@/design-system/kit';
import { allocationGuide, useParameterColumns, useParameterTables, useProjectScripts } from './scriptNames';
import { CopyForAi } from './CopyForAi';

/**
 * A new allocation step's script (Waste allocation plan, decisions 4 and 8): topsoil then freedig off the top of each
 * waste over coal, every thickness from a parameter row, the rest Excavator. Schedule fields are named from below
 * Scheduled, so this sets `Waste\Topsoil\Prime Volume` and the rest where the dataflow has them.
 */
export const ALLOCATION_STEP_TEMPLATE = `# Runs once on each block column after rom: split units, set their words and schedule fields.
# Every number comes from a parameter row (p, the column's; unit.p, a unit's): the script holds the order and the logic.
def allocate(column, p):
    for u in column.waste():
        below = column.after(u)
        if below == None or below.material != "coal":
            continue
        topsoil, freedig, rest = u.split_thickness(p.topsoil_thickness, p.freedig_thickness)
        for piece, process in [(topsoil, "Topsoil"), (freedig, "Freedig"), (rest, "Excavator")]:
            if piece != None:
                piece["Waste\\\\" + process + "\\\\Prime Volume"] = piece.volume
`;

/** The labels every unit carries, read and never set. */
const LABELS = ['seam', 'material', 'bench', 'became'];

/**
 * An allocation step of a dataflow edited (Waste allocation plan, decisions 1, 3 and 4): its name and its Starlark,
 * `def allocate(column, p):`, checked as it's typed. It runs once on each block column after `rom` and before the final
 * writes: units split by thickness from the top or by shares in Rust, the pieces adding back exactly; words and schedule
 * fields set on each; values placed onto other leaves. Saved with the dataflow; the Dataflow tab traces it line by line.
 */
export function AllocationStepEditor({
  flow,
  step,
  saved,
  onStep,
  onRemove,
  onClose,
}: {
  flow: Dataflow;
  step: DataflowStep;
  saved?: DataflowStep;
  onStep: (s: DataflowStep) => void;
  onRemove: () => void;
  onClose: () => void;
}) {
  const parameters = useParameterColumns(flow.parameters);
  const scripts = useProjectScripts();
  const lookupTables = useParameterTables();
  const script = step.allocation?.script ?? '';
  const [problems, setProblems] = useState<{ message: string; line: number | null }[]>([]);
  const [reference, setReference] = useState('');
  const edited = JSON.stringify(saved) !== JSON.stringify(step);
  const fields = [...flow.fields.map((f) => ({ name: f.name })), ...(flow.words ?? []).map((w) => ({ name: w.name, unit: 'words' }))];

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


  return (
    <div className="flex h-full min-h-0 flex-col gap-1" data-slot="allocation-step">
      <div className="flex items-center gap-1">
        <CellInput value={step.step} label="The step's name" className="w-40 font-semibold" onChange={(name) => onStep({ ...step, step: name.trim().replaceAll(' ', '_') })} />
        {edited && <StatusBadge tone="edited">edited</StatusBadge>}
        <span className="ml-auto flex items-center gap-1">
          <CopyForAi brief={{ kind: 'allocation', content: flow, source: script, name: step.step }} problems={problems} />
          <IconButton label="Take this allocation step out" className="size-6" onClick={onRemove}>
            <Trash2 />
          </IconButton>
          <IconButton label="Back to the fields" className="size-6" onClick={onClose}>
            <X />
          </IconButton>
        </span>
      </div>
      <PanelHead
        line="Runs once on each block column after rom: splits units and fills each process's schedule fields"
        more="column.waste(), column.coal(), column.after(u), column.group_while(...) and column.total(...) read the column; u.split_thickness(...) and u.split_shares([...]) split a unit in Rust, the pieces adding back exactly. Every number comes from a parameter row: p is the column's, u.p a unit's, lookup(...) any table's."
      />
      <ScriptEditor
        value={script}
        onChange={(s) => onStep({ ...step, allocation: { script: s } })}
        problems={problems}
        label="The allocation script's Starlark"
        className="min-h-40 flex-1"
        names={{ kind: 'allocation', fields, labels: LABELS, parameters, scripts, tables: lookupTables }}
      />
      {problems.length > 0 ? (
        <div className="text-destructive" data-slot="allocation-step-problems">
          {problems.map((p, i) => (
            <div key={i}>
              {p.line ? `line ${p.line}: ` : ''}
              {p.message}
            </div>
          ))}
        </div>
      ) : (
        <span className="text-ok">Checks clean: it runs when the dataflow is saved and run.</span>
      )}
      <details className="shrink-0 text-muted-foreground" data-slot="script-guide">
        <summary className="cursor-pointer">How allocation scripts are written</summary>
        <pre className="mt-1 max-h-64 overflow-auto font-mono text-xs whitespace-pre-wrap">{allocationGuide(reference)}</pre>
      </details>
    </div>
  );
}
