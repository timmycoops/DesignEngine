import { ArrowDown, ArrowUp, Download, Plus, Upload, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/design-system/ui/button';
import { Check, Choice, Field, IconButton, NumberInput, TextInput, ToolSection } from '@/design-system/kit';
import { type Attributes, type BenchRule, catchAll, everything, horizonState, horizonsOf, readWhen, rulesFromCsv, rulesToCsv, setHorizonBenched, taken, whenText } from './benchRules';

/**
 * By benches in the Cut solids panel (decision 0071): the levels for everything else, a tick for each horizon the solids
 * carry, and the bench rules in full, read top down, the first a solid fits winning, each with how many of the solids it
 * takes. The rules go out and come in as CSV.
 */

const NAMED_BY = ['bottom', 'top'] as const;
const NAMED_BY_LABELS = { bottom: 'Bottom', top: 'Top' };

/** A rule's filter, typed as `horizon: U-M, M-L; strip: 1-5`, taken when it reads and the box is left. */
function WhenInput({ when, onChange, index }: { when: Record<string, string>; onChange: (when: Record<string, string>) => void; index: number }) {
  const shown = whenText(when);
  const [text, setText] = useState(shown);
  const [why, setWhy] = useState<string | null>(null);
  // A filter set from outside (a CSV read, a rule moved) shows.
  useEffect(() => setText(shown), [shown]);
  return (
    <TextInput
      className={`h-7 min-w-0 flex-1 px-1.5 text-xs ${why ? 'border-warning ring-1 ring-warning/40' : ''}`}
      value={text}
      placeholder="Everything"
      label={`Rule ${index + 1}: which solids`}
      aria-invalid={why !== null}
      title={why ?? 'Attribute: values; another: values. A comma list of words, numbers, ranges such as 1-5 or comparisons such as <0.3, and !WASTE for a value it may not hold; blank is everything'}
      onChange={(value) => {
        setText(value);
        const read = readWhen(value);
        setWhy('why' in read ? read.why : null);
      }}
      onBlur={() => {
        const read = readWhen(text);
        if ('when' in read) onChange(read.when);
        else setText(whenText(when));
        setWhy(null);
      }}
    />
  );
}

/** The levels a benched rule takes: from, to, every and which end names a bench. */
function Levels({ rule, onChange, index }: { rule: BenchRule; onChange: (rule: BenchRule) => void; index?: number }) {
  const of = index === undefined ? '' : `Rule ${index + 1}: `;
  return (
    <>
      <NumberInput value={rule.from} onChange={(from) => onChange({ ...rule, from })} label={`${of}lowest level`} unit="RL" width="w-20" />
      <span className="text-muted-foreground">to</span>
      <NumberInput value={rule.to} onChange={(to) => onChange({ ...rule, to })} label={`${of}highest level`} unit="RL" width="w-20" />
      <span className="text-muted-foreground">every</span>
      <NumberInput value={rule.every} onChange={(every) => onChange({ ...rule, every })} label={`${of}metres between levels`} unit="m" width="w-16" range={{ min: 0, minIncluded: false }} />
      <Choice value={rule.named_by} options={NAMED_BY} labels={NAMED_BY_LABELS} onChange={(named_by) => onChange({ ...rule, named_by })} label={`${of}a bench named by its bottom or its top RL`} />
    </>
  );
}

export function BenchRulesSection({ rules, onChange, solids, flapThickness, onFlapThickness }: { rules: BenchRule[]; onChange: (rules: BenchRule[]) => void; solids: Attributes[] | null; flapThickness: number; onFlapThickness: (value: number) => void }) {
  const file = useRef<HTMLInputElement>(null);
  const [csvErrors, setCsvErrors] = useState<string[]>([]);
  const last = catchAll(rules);
  const horizons = solids ? horizonsOf(solids) : [];
  const counts = solids ? taken(rules, solids) : null;
  const set = (k: number, rule: BenchRule) => onChange(rules.map((r, j) => (j === k ? rule : r)));
  const move = (k: number, by: number) => {
    const out = [...rules];
    const [rule] = out.splice(k, 1);
    out.splice(k + by, 0, rule);
    onChange(out);
  };
  const addRule = () => {
    const at = last >= 0 ? last : rules.length;
    onChange([...rules.slice(0, at), { ...everything(), when: { material: 'coal' } }, ...rules.slice(at)]);
  };
  const saveCsv = () => {
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([rulesToCsv(rules)], { type: 'text/csv' }));
    link.download = 'bench rules.csv';
    link.click();
    URL.revokeObjectURL(link.href);
  };
  const readCsv = async (picked: File | undefined) => {
    if (!picked) return;
    const { rules: read, errors } = rulesFromCsv(await picked.text());
    setCsvErrors(errors.map((e) => `${picked.name}: ${e}`));
    if (errors.length === 0) onChange(read);
  };
  const benched = counts ? counts.byRule.reduce((n, c, k) => n + (rules[k].benched ? c : 0), 0) : 0;
  const whole = counts ? (solids?.length ?? 0) - benched : 0;

  return (
    <ToolSection
      title="Benches"
      aside={
        <>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" title="Read bench rules from a CSV: a column an attribute, then benched, from, to, every, named_by and layer" onClick={() => file.current?.click()}>
            <Upload className="size-3.5" /> CSV
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" title="The bench rules as CSV, to keep or edit" onClick={saveCsv}>
            <Download className="size-3.5" /> CSV
          </Button>
          <input ref={file} type="file" accept=".csv,text/csv" className="hidden" aria-label="Bench rules CSV" onChange={(e) => void readCsv(e.target.files?.[0]).finally(() => (e.target.value = ''))} />
        </>
      }
    >
      {last >= 0 ? (
        <Field label={rules.length === 1 ? 'Levels' : 'Others'} hint={rules.length === 1 ? 'Every solid, from RL to RL every so many metres, each bench named by its floor or its top' : 'Everything the rules above leave'}>
          <Levels rule={rules[last]} onChange={(rule) => set(last, rule)} />
        </Field>
      ) : (
        <Field label="Others" hint="No rule takes everything else: those solids are carried over whole">
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => onChange([...rules, everything()])}>
            <Plus className="size-3.5" /> Bench everything else
          </Button>
        </Field>
      )}

      {horizons.length > 0 && (
        <Field label="Horizons" hint="Unticked, a horizon is carried over whole. A tick set by a rule below says which">
          {horizons.map((h) => {
            const state = horizonState(rules, solids ?? [], h);
            const by = state.rules.filter((k) => !(k === 0 && state.byTicks)).map((k) => k + 1);
            return (
              <label key={h} className="inline-flex items-center gap-1" title={state.byTicks ? `${h}: benched or not` : `${h}: set by rule ${by.join(', ')}`}>
                <Check
                  checked={state.benched === 'all' ? true : state.benched === 'none' ? false : 'indeterminate'}
                  disabled={!state.byTicks}
                  label={`Bench ${h}`}
                  onChange={(on) => onChange(setHorizonBenched(rules, h, on))}
                />
                {h}
              </label>
            );
          })}
        </Field>
      )}

      <Field label="Flaps" hint="Where a bench is thinner than this between its top and bottom, as where a level runs into a seam's roof at a low angle, it's taken off; the volume is said. 0 for none">
        <NumberInput value={flapThickness} onChange={onFlapThickness} label="Flap thickness" unit="m" range={{ min: 0 }} />
      </Field>

      <div className="flex items-center gap-2">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">Rules, first that fits</span>
        <Button variant="outline" size="sm" className="ml-auto h-6 px-2 text-xs" title="A rule above the one for everything else" onClick={addRule}>
          <Plus className="size-3.5" /> Rule
        </Button>
      </div>
      <ol className="flex flex-col gap-1.5" aria-label="Bench rules">
        {rules.map((rule, k) => (
          <li key={k} className="flex flex-col gap-1 rounded-md border border-border p-1.5">
            <div className="flex items-center gap-1.5">
              <span className="w-4 shrink-0 text-right tabular-nums text-muted-foreground">{k + 1}</span>
              <WhenInput when={rule.when} index={k} onChange={(when) => set(k, { ...rule, when })} />
              <label className="inline-flex shrink-0 items-center gap-1" title="Benched, or carried over whole">
                <Check checked={rule.benched} label={`Rule ${k + 1}: benched`} onChange={(on) => set(k, { ...rule, benched: on })} />
                Bench
              </label>
              <span className="w-14 shrink-0 text-right tabular-nums text-muted-foreground" title="How many of the solids this rule takes">
                {counts ? `${counts.byRule[k]} taken` : ''}
              </span>
              <IconButton label={`Rule ${k + 1} up`} disabled={k === 0} onClick={() => move(k, -1)}>
                <ArrowUp />
              </IconButton>
              <IconButton label={`Rule ${k + 1} down`} disabled={k === rules.length - 1} onClick={() => move(k, 1)}>
                <ArrowDown />
              </IconButton>
              <IconButton label={`Remove rule ${k + 1}`} disabled={rules.length === 1} onClick={() => onChange(rules.filter((_, j) => j !== k))}>
                <X />
              </IconButton>
            </div>
            <div className="flex flex-wrap items-center gap-1.5 pl-5">
              {rule.benched ? <Levels rule={rule} index={k} onChange={(r) => set(k, r)} /> : <span className="text-muted-foreground">Carried over whole</span>}
              <TextInput
                className="w-36"
                value={rule.layer ?? ''}
                placeholder="The run's layer"
                label={`Rule ${k + 1}: its own layer`}
                title="Its own output layer; blank, the run's layer"
                onChange={(value) => set(k, { ...rule, layer: value })}
              />
            </div>
          </li>
        ))}
      </ol>
      {counts && (
        <div className="text-muted-foreground">
          Of the {solids?.length ?? 0} solids, {benched} benched and {whole} carried over whole{counts.none > 0 ? `, ${counts.none} of them taken by no rule` : ''}.
        </div>
      )}
      {csvErrors.length > 0 && (
        <div className="flex flex-col gap-0.5 text-destructive" role="alert">
          {csvErrors.map((e, k) => (
            <span key={k}>{e}</span>
          ))}
        </div>
      )}
    </ToolSection>
  );
}
