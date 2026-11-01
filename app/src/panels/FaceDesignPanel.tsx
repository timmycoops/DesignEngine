import { ArrowDown, ArrowUp, Crosshair, Play, Plus, Save, Trash2 } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useId, useMemo, useState } from 'react';
import { type AzimuthSector, type FaceDesign, type FaceDirection, type FaceLine, type FaceProfile, type HorizonValue, objectDetails, type ObjectSummary, type ProfileStep, type Ramp, type RampFixed, type RampHeading, type RuleValue, type StepTarget, type StepValue, type WallRule, type WallSide } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { rampColour } from '@/viewport/rampColours';
import { useWorkspace } from '@/workspace/context';

const selectClass = 'h-7 min-w-0 rounded border bg-background px-1 text-xs';
const headClass = 'px-1 font-normal';
const cellClass = 'px-1 py-0.5';

/** What a new row starts with. */
const usualTolerance = 0.1;
const usualBatter = 70;
const usualBerm = 10;
const usualMinBench = 1;
const usualRoadWidth = 30;
const usualGrade = 10;

/** A number that can be typed through half-finished values: it only reports what parses, and goes back to what's held on leaving it. */
function NumberBox({ value, onChange, title, width = 'w-16', disabled }: { value: number | null; onChange: (v: number) => void; title: string; width?: string; disabled?: boolean }) {
  const held = value === null ? '' : String(value);
  const [text, setText] = useState(held);
  useEffect(() => {
    setText((t) => (t.trim() !== '' && Number(t) === value ? t : held));
  }, [value, held]);
  return (
    <Input
      className={`h-7 ${width} px-1 text-xs tabular-nums`}
      value={text}
      title={title}
      aria-label={title}
      disabled={disabled}
      onBlur={() => setText(held)}
      onChange={(e) => {
        setText(e.target.value);
        const n = Number(e.target.value);
        if (e.target.value.trim() !== '' && Number.isFinite(n)) onChange(n);
      }}
    />
  );
}

/** What's typed in a batter or width: a number if it reads as one, otherwise a rule's name. */
function stepValue(text: string): StepValue {
  const n = Number(text);
  return text.trim() !== '' && Number.isFinite(n) ? n : text.trim();
}

/** A batter or width: a number typed on the row, or a rule picked from the list. */
function ValueBox({ value, unit, what, rules, onChange }: { value: StepValue; unit: string; what: string; rules: WallRule[]; onChange: (v: StepValue) => void }) {
  const [text, setText] = useState(String(value));
  const list = useId();
  useEffect(() => {
    setText((t) => (stepValue(t) === value ? t : String(value)));
  }, [value]);
  return (
    <span className="flex items-center gap-1">
      <Input
        className="h-7 w-24 px-1 text-xs tabular-nums"
        list={list}
        value={text}
        title={`${what}, or the name of a rule`}
        aria-label={what}
        onBlur={() => setText(String(value))}
        onChange={(e) => {
          setText(e.target.value);
          onChange(stepValue(e.target.value));
        }}
      />
      <datalist id={list}>
        {rules.map((r) => (
          <option key={r.name} value={r.name} />
        ))}
      </datalist>
      <span className="text-muted-foreground">{unit}</span>
    </span>
  );
}

function Heading({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="mt-3 flex items-center gap-2 border-t px-3 pt-2 pb-1 text-xs font-medium text-muted-foreground">
      <span className="uppercase tracking-wide">{children}</span>
      <span className="ml-auto flex items-center gap-1">{action}</span>
    </div>
  );
}

function SmallButton({ onClick, title, children, disabled }: { onClick: () => void; title?: string; children: ReactNode; disabled?: boolean }) {
  return (
    <Button variant="outline" size="sm" className="h-7 gap-1 px-2 text-xs" title={title} disabled={disabled} onClick={onClick}>
      {children}
    </Button>
  );
}

function IconButton({ onClick, title, children, disabled }: { onClick: () => void; title: string; children: ReactNode; disabled?: boolean }) {
  return (
    <button className="rounded p-1 text-muted-foreground hover:text-foreground disabled:opacity-30" title={title} aria-label={title} disabled={disabled} onClick={onClick}>
      {children}
    </button>
  );
}

function Choice<T extends string>({ value, options, onChange }: { value: T; options: readonly T[]; onChange: (v: T) => void }) {
  return (
    <span className="inline-flex rounded border">
      {options.map((option) => (
        <button key={option} className={`px-1.5 py-1 capitalize ${option === value ? 'bg-selection text-primary' : 'text-muted-foreground'}`} onClick={() => onChange(option)}>
          {option}
        </button>
      ))}
    </span>
  );
}

const SIDES: readonly WallSide[] = ['left', 'right'];
const DIRECTIONS: readonly FaceDirection[] = ['up', 'down'];
const HEADINGS: readonly RampHeading[] = ['along', 'back'];

/** A picked point to the millimetre. */
const toMillimetres = (metres: number) => Math.round(metres * 1000) / 1000;

const blankDesign = (register: string): FaceDesign => ({
  layer: 'faces',
  lines: [],
  profiles: [{ name: 'Highwall', direction: 'up', register, condition: usualTolerance, steps: [] }],
  rules: [],
});

/** A name not already taken, from a starting word. */
function freeName(word: string, taken: string[]) {
  if (!taken.includes(word)) return word;
  for (let k = 2; ; k++) if (!taken.includes(`${word} ${k}`)) return `${word} ${k}`;
}

function replaceAt<T>(items: T[], at: number, item: T) {
  return items.map((old, k) => (k === at ? item : old));
}

function moved<T>(items: T[], at: number, by: number) {
  const to = at + by;
  if (to < 0 || to >= items.length) return items;
  const out = items.slice();
  [out[at], out[to]] = [out[to], out[at]];
  return out;
}

/** The number a value comes to, if it's one number all along. */
function numberOf(value: StepValue, rules: WallRule[]) {
  if (typeof value === 'number') return value;
  const rule = rules.find((r) => r.name === value)?.value;
  return rule?.type === 'fixed' ? rule.value : undefined;
}

const RULE_TYPES: { type: RuleValue['type']; label: string; does: string }[] = [
  { type: 'fixed', label: 'Fixed', does: 'One value all along. Change it here and every row that names it follows.' },
  { type: 'fixed_list', label: 'By segment', does: 'One value for each segment of the string, in order.' },
  { type: 'inside_polygon', label: 'Inside polygon', does: 'Changes where a line crosses the polygon.' },
  { type: 'inside_solid', label: 'Inside solid', does: 'Changes where the line a step starts from enters the solid.' },
  { type: 'segment_azimuth', label: 'By azimuth', does: 'By the way the wall faces, clockwise from grid north.' },
  { type: 'by_horizon', label: 'By horizon', does: 'By the surface the row finishes on.' },
];

/** A first number to carry over when a rule changes type. */
function firstNumber(rule: RuleValue) {
  switch (rule.type) {
    case 'fixed':
      return rule.value;
    case 'fixed_list':
      return rule.values[0] ?? 0;
    case 'inside_polygon':
    case 'inside_solid':
      return rule.inside;
    default:
      return rule.otherwise;
  }
}

/** One value for each segment of the string, typed in a line. */
function ListBox({ values, onChange }: { values: number[]; onChange: (values: number[]) => void }) {
  const [text, setText] = useState(values.join(', '));
  return (
    <Input
      className="h-7 min-w-0 flex-1 px-1 text-xs tabular-nums"
      value={text}
      placeholder="70, 65, 60"
      title="One value for each segment of the string, in order"
      aria-label="Values by segment"
      onChange={(e) => {
        setText(e.target.value);
        onChange(e.target.value.split(/[\s,]+/).filter(Boolean).map(Number).filter(Number.isFinite));
      }}
    />
  );
}

/** What a rule of its type does, and which rows use it, as it does nothing until one does. */
function RuleNote({ rule, profiles }: { rule: WallRule; profiles: FaceProfile[] }) {
  const rows = profiles.flatMap((p) => p.steps.flatMap((step, k) => (stepValueOf(step) === rule.name ? [`${p.name} step ${k + 1}`] : [])));
  return (
    <p className="mt-0.5 text-muted-foreground">
      {RULE_TYPES.find((t) => t.type === rule.value.type)?.does}{' '}
      {rows.length > 0 ? `Used by ${rows.join(', ')}.` : <span className="text-primary">No row uses it yet.</span>}
    </p>
  );
}

function Labelled({ label, children }: { label: string; children: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1 text-muted-foreground">
      {label}
      {children}
    </span>
  );
}

/** What a rule of its type needs, typed on its row. */
function RuleDetail({ rule, polygons, solidName, selectedSolid, surfaces, onChange }: { rule: RuleValue; polygons: ObjectSummary[]; solidName: string; selectedSolid: string | null; surfaces: ObjectSummary[]; onChange: (rule: RuleValue) => void }) {
  const unit = 'Degrees for a batter, metres for a berm';
  const number = (label: string, value: number, change: (v: number) => void) => (
    <Labelled label={label}>
      <NumberBox width="w-14" value={value} title={unit} onChange={change} />
    </Labelled>
  );
  switch (rule.type) {
    case 'fixed':
      return <NumberBox value={rule.value} title={unit} onChange={(value) => onChange({ type: 'fixed', value })} />;
    case 'fixed_list':
      return <ListBox values={rule.values} onChange={(values) => onChange({ ...rule, values })} />;
    case 'inside_polygon':
      return (
        <span className="flex flex-wrap items-center gap-2">
          <select className={`${selectClass} max-w-36`} value={rule.polygon} aria-label="Polygon" onChange={(e) => onChange({ ...rule, polygon: e.target.value })}>
            {polygons.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
          {number('Inside', rule.inside, (inside) => onChange({ ...rule, inside }))}
          {number('Outside', rule.outside, (outside) => onChange({ ...rule, outside }))}
        </span>
      );
    case 'inside_solid':
      return (
        <span className="flex flex-wrap items-center gap-2">
          <span className="max-w-32 truncate" title={solidName}>
            {solidName}
          </span>
          <SmallButton disabled={!selectedSolid} title="Takes the solid selected in the viewport or the layer tree" onClick={() => selectedSolid && onChange({ ...rule, solid: selectedSolid })}>
            Use selected
          </SmallButton>
          {number('Inside', rule.inside, (inside) => onChange({ ...rule, inside }))}
          {number('Outside', rule.outside, (outside) => onChange({ ...rule, outside }))}
        </span>
      );
    case 'segment_azimuth':
      return (
        <span className="flex flex-col gap-1">
          {rule.sectors.map((sector, k) => {
            const change = (c: Partial<AzimuthSector>) => onChange({ ...rule, sectors: replaceAt(rule.sectors, k, { ...sector, ...c }) });
            return (
              <span key={k} className="flex items-center gap-2" title="The way the wall faces, clockwise from grid north">
                {number('Facing', sector.from, (from) => change({ from }))}
                {number('to', sector.to, (to) => change({ to }))}
                {number('Value', sector.value, (value) => change({ value }))}
                <IconButton title="Remove this sector" onClick={() => onChange({ ...rule, sectors: rule.sectors.filter((_, j) => j !== k) })}>
                  <Trash2 className="size-3" />
                </IconButton>
              </span>
            );
          })}
          <span className="flex items-center gap-2">
            {number('Otherwise', rule.otherwise, (otherwise) => onChange({ ...rule, otherwise }))}
            <SmallButton onClick={() => onChange({ ...rule, sectors: [...rule.sectors, { from: 0, to: 90, value: rule.otherwise }] })}>
              <Plus className="size-3" /> Sector
            </SmallButton>
          </span>
        </span>
      );
    case 'by_horizon':
      return (
        <span className="flex flex-col gap-1">
          {rule.values.map((entry, k) => {
            const change = (c: Partial<HorizonValue>) => onChange({ ...rule, values: replaceAt(rule.values, k, { ...entry, ...c }) });
            return (
              <span key={k} className="flex items-center gap-2" title="A row that finishes on this surface takes this value">
                <select className={`${selectClass} max-w-36`} value={entry.surface} aria-label="Horizon" onChange={(e) => change({ surface: e.target.value })}>
                  <SurfaceOptions surfaces={surfaces} />
                </select>
                {number('Value', entry.value, (value) => change({ value }))}
                <IconButton title="Remove this horizon" onClick={() => onChange({ ...rule, values: rule.values.filter((_, j) => j !== k) })}>
                  <Trash2 className="size-3" />
                </IconButton>
              </span>
            );
          })}
          <span className="flex items-center gap-2">
            {number('Otherwise', rule.otherwise, (otherwise) => onChange({ ...rule, otherwise }))}
            <SmallButton disabled={surfaces.length === 0} onClick={() => onChange({ ...rule, values: [...rule.values, { surface: surfaces[0].id, value: rule.otherwise }] })}>
              <Plus className="size-3" /> Horizon
            </SmallButton>
          </span>
        </span>
      );
  }
}

function stepValueOf(step: ProfileStep) {
  return step.step === 'project' ? step.batter : step.width;
}

function targetOf(step: ProfileStep) {
  return step.step === 'project' ? step.to : step.on;
}

/** Where a new step should start: the surface after the last one the table went to, or after the toe's own. */
function nextTarget(steps: ProfileStep[], surfaces: ObjectSummary[], register: string): StepTarget {
  const last = [...steps].reverse().find((s) => s.step === 'project' && s.to.kind === 'surface');
  const from = last?.step === 'project' && last.to.kind === 'surface' ? last.to.id : register;
  // Past the last surface a face goes on to an RL.
  const next = surfaces[surfaces.findIndex((s) => s.id === from) + 1];
  return next ? { kind: 'surface', id: next.id } : { kind: 'rl', rl: 0 };
}

/** A new step of a kind, starting from the table's last one of that kind. */
function blankStep(kind: ProfileStep['step'], steps: ProfileStep[], surfaces: ObjectSummary[], register: string, condition: number | null): ProfileStep {
  const last = [...steps].reverse().find((s) => s.step === kind);
  if (kind === 'project') return { step: 'project', to: nextTarget(steps, surfaces, register), batter: last ? stepValueOf(last) : usualBatter, condition };
  const minBench = last?.step === 'berm' ? last.min_bench : usualMinBench;
  return { step: 'berm', on: null, width: last ? stepValueOf(last) : usualBerm, register: true, condition, min_bench: minBench };
}

/** Whether a row hands its crest on as one wall with the row below: same batter, no berm between. */
function carriesOn(steps: ProfileStep[], at: number, rules: WallRule[]) {
  const [step, next] = [steps[at], steps[at + 1]];
  if (step.step !== 'project' || next?.step !== 'project') return false;
  if ((step.ramp ?? null) !== (next.ramp ?? null)) return false;
  // A rule that changes along the wall is the same wall if both rows name it. By horizon goes row by row.
  const rule = rules.find((r) => r.name === step.batter)?.value;
  if (step.batter === next.batter && rule && rule.type !== 'by_horizon') return true;
  const batter = numberOf(step.batter, rules);
  return batter !== undefined && batter === numberOf(next.batter, rules);
}

/** A tick and its tolerance: off leaves the line as it is. */
function ConditionCell({ value, onChange, carries }: { value: number | null; onChange: (v: number | null) => void; carries?: boolean }) {
  const on = value !== null && !carries;
  return (
    <span className="flex items-center gap-2" title={carries ? 'One wall carries straight on from here, so there is no line to condition' : undefined}>
      <Checkbox className="after:inset-0" checked={on} disabled={carries} aria-label="Condition this line" onCheckedChange={(ticked) => onChange(ticked === true ? usualTolerance : null)} />
      <NumberBox width="w-12" value={on ? value : null} disabled={!on} title="How far the line may move in plan (m): 0.1 is usual, 2 at most" onChange={onChange} />
    </span>
  );
}

function RegisterCell({ checked, onChange }: { checked: boolean; onChange?: (v: boolean) => void }) {
  const locked = !onChange;
  return <Checkbox className="after:inset-0" checked={checked} disabled={locked} title={locked ? 'A wall always stops on its target, with a vertex at every triangle it crosses' : 'Follow the surface. Unticked, the line keeps the heights of its crest'} aria-label="Register" onCheckedChange={(ticked) => onChange?.(ticked === true)} />;
}

function SurfaceOptions({ surfaces }: { surfaces: ObjectSummary[] }) {
  return surfaces.map((s) => (
    <option key={s.id} value={s.id}>
      {s.name}
    </option>
  ));
}

/** One step of a profile. A row says where it finishes: a wall's target, or the surface a berm's line registers on. */
function StepRow({ number, step, surfaces, rules, above, carries, ramps, bridged, blank, onChange, onMove, onRemove }: { number: number; step: ProfileStep; surfaces: ObjectSummary[]; rules: WallRule[]; above: string; carries: boolean; ramps: Ramp[]; bridged: string | null; blank: (kind: ProfileStep['step']) => ProfileStep; onChange: (s: ProfileStep) => void; onMove: (by: number) => void; onRemove: () => void }) {
  const target = targetOf(step);
  const surfaceValue = target?.kind === 'surface' ? target.id : target?.kind === 'rl' ? 'rl' : '';
  const setSurface = (value: string) => {
    if (step.step === 'project') onChange({ ...step, to: value === 'rl' ? { kind: 'rl', rl: 0 } : { kind: 'surface', id: value } });
    else onChange({ ...step, on: value === '' ? null : { kind: 'surface', id: value } });
  };
  const setRl = (rl: number) => onChange(step.step === 'project' ? { ...step, to: { kind: 'rl', rl } } : { ...step, on: { kind: 'rl', rl } });
  // A berm that keeps its crest's heights doesn't register on anything.
  const held = step.step === 'berm' && !step.register;
  return (
    <tr className="border-t">
      <td className={`${cellClass} tabular-nums text-muted-foreground`}>{number}</td>
      <td className={cellClass}>
        <select className={selectClass} value={step.step} aria-label="Type" onChange={(e) => onChange({ ...blank(e.target.value as ProfileStep['step']), condition: step.condition })}>
          <option value="project">Project</option>
          <option value="berm">Berm</option>
        </select>
      </td>
      <td className={cellClass}>
        <select className={`${selectClass} w-full max-w-36`} value={surfaceValue} aria-label="Surface" disabled={held} onChange={(e) => setSurface(e.target.value)}>
          {step.step === 'berm' && <option value="">{above} (as above)</option>}
          <SurfaceOptions surfaces={surfaces} />
          {(step.step === 'project' || target?.kind === 'rl') && <option value="rl">–</option>}
        </select>
      </td>
      <td className={cellClass}>
        <NumberBox width="w-14" value={target?.kind === 'rl' ? target.rl : null} title="RL" disabled={held} onChange={setRl} />
      </td>
      <td className={cellClass}>
        {step.step === 'project' ? (
          <ValueBox value={step.batter} unit="°" what="Batter in degrees" rules={rules} onChange={(batter) => onChange({ ...step, batter })} />
        ) : (
          <ValueBox value={step.width} unit="m" what="Width in metres" rules={rules} onChange={(width) => onChange({ ...step, width })} />
        )}
      </td>
      <td className={cellClass}>
        {step.step === 'berm' && <NumberBox width="w-12" value={step.min_bench ?? null} title="No berm where the wall below it is lower than this (m). Blank or 0 is off" onChange={(height) => onChange({ ...step, min_bench: height > 0 ? height : null })} />}
      </td>
      <td className={cellClass}>
        {step.step === 'project' ? <RampCell value={step.ramp ?? null} ramps={ramps} onChange={(ramp) => onChange({ ...step, ramp })} /> : bridged && <span className="ml-1.5 block h-6 w-1" style={{ background: rampColour(bridged) }} title={`Ramp ${bridged} crosses this berm: the road is the bench here`} />}
      </td>
      <td className={cellClass}>{step.step === 'berm' ? <RegisterCell checked={step.register} onChange={(register) => onChange({ ...step, register })} /> : step.to.kind === 'surface' && <RegisterCell checked />}</td>
      <td className={cellClass}>
        <ConditionCell value={step.condition} carries={carries} onChange={(condition) => onChange({ ...step, condition })} />
      </td>
      <td className="whitespace-nowrap py-0.5 text-right">
        <IconButton title="Move up" onClick={() => onMove(-1)}>
          <ArrowUp className="size-3" />
        </IconButton>
        <IconButton title="Move down" onClick={() => onMove(1)}>
          <ArrowDown className="size-3" />
        </IconButton>
        <IconButton title="Remove this step" onClick={onRemove}>
          <Trash2 className="size-3" />
        </IconButton>
      </td>
    </tr>
  );
}

/** Which ramp climbs a wall: a letter in its colour, or nothing. */
function RampCell({ value, ramps, onChange }: { value: string | null; ramps: Ramp[]; onChange: (ramp: string | null) => void }) {
  return (
    <select
      className={`${selectClass} w-12 font-medium`}
      style={value ? { background: rampColour(value), color: 'white' } : undefined}
      value={value ?? ''}
      aria-label="Ramp on this wall"
      title="The ramp that climbs this wall. Walls with the same letter carry one road, across any berms between them"
      disabled={ramps.length === 0 && !value}
      onChange={(e) => onChange(e.target.value || null)}
    >
      <option value="">–</option>
      {value && !ramps.some((r) => r.name === value) && <option value={value}>{value}</option>}
      {ramps.map((r) => (
        <option key={r.name} value={r.name}>
          {r.name}
        </option>
      ))}
    </select>
  );
}

/** One ramp: what it's on, what's held, the road, and how far the last run got it. */
function RampRow({ ramp, strings, results, picking, onPick, onChange, onRemove }: { ramp: Ramp; strings: ObjectSummary[]; results: string[]; picking: boolean; onPick: (use: (at: [number, number]) => void) => void; onChange: (r: Partial<Ramp>) => void; onRemove: () => void }) {
  return (
    <div className="border-t py-1">
      <div className="flex flex-wrap items-center gap-2">
        <span className="w-7 rounded px-1 py-0.5 text-center font-medium text-white" style={{ background: rampColour(ramp.name) }}>
          {ramp.name}
        </span>
        <select className={`${selectClass} max-w-36`} value={ramp.string} aria-label="String" title="The string this road goes on" onChange={(e) => onChange({ string: e.target.value })}>
          {!strings.some((s) => s.id === ramp.string) && <option value={ramp.string}>not on a line</option>}
          {strings.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <select className={selectClass} value={ramp.fixed} aria-label="Which end is held" title="Ends at: the run moves the start along the toe until the road comes out here" onChange={(e) => onChange({ fixed: e.target.value as RampFixed })}>
          <option value="end">Ends at</option>
          <option value="start">Starts at</option>
        </select>
        <span className="flex items-center gap-1">
          <NumberBox width="w-20" value={ramp.at[0]} title="Easting" onChange={(e) => onChange({ at: [e, ramp.at[1]] })} />
          <NumberBox width="w-20" value={ramp.at[1]} title="Northing" onChange={(n) => onChange({ at: [ramp.at[0], n] })} />
          <IconButton title="Pick it in the viewport" disabled={picking} onClick={() => onPick((at) => onChange({ at }))}>
            <Crosshair className="size-3" />
          </IconButton>
        </span>
        <Choice value={ramp.heading} options={HEADINGS} onChange={(heading) => onChange({ heading })} />
        <Labelled label="Width, m">
          <NumberBox width="w-12" value={ramp.width} title="Road width in metres, square to the road" onChange={(width) => onChange({ width })} />
        </Labelled>
        <Labelled label="Grade, %">
          <NumberBox width="w-12" value={ramp.grade} title="Rise over plan run in percent, on the pit-side edge" onChange={(grade) => onChange({ grade })} />
        </Labelled>
        <span className="ml-auto">
          <IconButton title="Remove this ramp" onClick={onRemove}>
            <Trash2 className="size-3" />
          </IconButton>
        </span>
      </div>
      {results.map((line, k) => (
        <p key={k} className={`pt-0.5 ${/stopped|isn't|comes out/.test(line) ? 'text-primary' : 'text-muted-foreground'}`}>
          {line.replace(/^Ramp [^:]*: /, '')}
        </p>
      ))}
    </div>
  );
}

/** A profile as one table, read top to bottom: the toe's Register row, then each step going one way from it. */
function StepTable({ profile, surfaces, rules, onChange }: { profile: FaceProfile; surfaces: ObjectSummary[]; rules: WallRule[]; onChange: (p: Partial<FaceProfile>) => void }) {
  const { steps, register } = profile;
  const ramps = profile.ramps ?? [];
  const blank = (kind: ProfileStep['step']) => blankStep(kind, steps, surfaces, register, usualTolerance);
  const add = (kind: ProfileStep['step']) => onChange({ steps: [...steps, blank(kind)] });
  /** The ramp crossing the berm at a row: the one on the walls either side of it. */
  const bridged = (at: number) => {
    const wall = (from: ProfileStep[]) => from.find((s) => s.step === 'project');
    const [below, above] = [wall(steps.slice(0, at).reverse()), wall(steps.slice(at + 1))];
    return below?.step === 'project' && above?.step === 'project' && below.ramp && below.ramp === above.ramp ? below.ramp : null;
  };
  const setSteps = (next: ProfileStep[]) => onChange({ steps: next });
  const nameOf = (target: StepTarget) => (target.kind === 'rl' ? `RL ${target.rl}` : (surfaces.find((s) => s.id === target.id)?.name ?? 'missing'));
  /** What the rows above a row last reached, which a berm registers on unless it names another. */
  const reached = (at: number) => {
    const last = steps.slice(0, at).reverse().map(targetOf).find((t) => t);
    return nameOf(last ?? { kind: 'surface', id: register });
  };
  return (
    <div className="px-3 pb-1">
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead className="text-left text-muted-foreground">
            <tr>
              <th className={headClass}>Step</th>
              <th className={headClass}>Type</th>
              <th className={headClass} title="Where the row finishes: a wall's target, or the surface a berm's line registers on">
                Surface
              </th>
              <th className={headClass}>RL</th>
              <th className={headClass}>Batter / Width</th>
              <th className={headClass} title="No berm where the wall below it is lower than this, as where grids sit on each other">
                Min bench, m
              </th>
              <th className={headClass} title="The ramp that climbs this wall">
                Ramp
              </th>
              <th className={headClass}>Register</th>
              <th className={headClass} title="Clean the line before the next step, to within this many metres">
                Condition, m
              </th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr className="border-t">
              <td />
              <td className={`${cellClass} px-2`}>Register</td>
              <td className={cellClass}>
                <select className={`${selectClass} w-full max-w-36`} value={register} aria-label="Register toe to" onChange={(e) => onChange({ register: e.target.value })}>
                  <SurfaceOptions surfaces={surfaces} />
                </select>
              </td>
              <td />
              <td />
              <td />
              <td />
              <td className={cellClass}>
                <RegisterCell checked />
              </td>
              <td className={cellClass}>
                <ConditionCell value={profile.condition} onChange={(condition) => onChange({ condition })} />
              </td>
              <td />
            </tr>
            {steps.map((step, k) => (
              <StepRow
                key={k}
                number={k + 1}
                step={step}
                surfaces={surfaces}
                rules={rules}
                above={reached(k)}
                carries={carriesOn(steps, k, rules)}
                ramps={ramps}
                bridged={step.step === 'berm' ? bridged(k) : null}
                blank={blank}
                onChange={(s) => setSteps(replaceAt(steps, k, s))}
                onMove={(by) => setSteps(moved(steps, k, by))}
                onRemove={() => setSteps(steps.filter((_, j) => j !== k))}
              />
            ))}
          </tbody>
        </table>
      </div>
      <div className="flex items-center gap-1 pt-1">
        <SmallButton onClick={() => add('project')}>
          <Plus className="size-3" /> Project
        </SmallButton>
        <SmallButton onClick={() => add('berm')}>
          <Plus className="size-3" /> Berm
        </SmallButton>
      </div>
    </div>
  );
}

/**
 * A face design: the lines to run, the profile each one takes, and any rules those profiles share. Each
 * profile is one table going one way from the toe. A design stands apart from the selection. Run saves it
 * and steps every line in one run, one undo.
 */
export function FaceDesignPanel() {
  const { summary, selected, marked, setMarked, runRecord, pickPoint } = useWorkspace();
  const objects = useMemo(() => summary?.objects ?? [], [summary]);
  const designs = useMemo(() => objects.filter((o) => o.kind === 'face_design'), [objects]);
  const [id, setId] = useState<string | null>(null);
  const [name, setName] = useState('Face design');
  const [draft, setDraft] = useState<FaceDesign | null>(null);
  const [dirty, setDirty] = useState(false);
  const [profileName, setProfileName] = useState<string | null>(null);
  const [addingLayer, setAddingLayer] = useState('');
  const [report, setReport] = useState<{ lines: string[]; error?: string }>({ lines: [] });
  const [picking, setPicking] = useState(false);

  const open = designs.find((d) => d.id === id);
  const revision = open?.revision;
  // What a design's own runs made are results, not things to design with.
  const made = (o: ObjectSummary) => o.attributes.face_design !== undefined;
  const strings = useMemo(() => objects.filter((o) => o.kind === 'string' && !made(o)), [objects]);
  const surfaces = useMemo(() => objects.filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid') && !made(o)), [objects]);

  useEffect(() => {
    if (!id || dirty) return;
    let stale = false;
    void objectDetails(id).then((details) => {
      if (stale || details.measures.kind !== 'face_design') return;
      setDraft(details.measures.design);
      setName(details.name);
    });
    return () => {
      stale = true;
    };
  }, [id, revision, dirty]);

  // A design undone out of the project goes with it.
  useEffect(() => {
    if (id && summary && !open) {
      setId(null);
      setDraft(null);
      setDirty(false);
    }
  }, [id, open, summary]);

  const edit = useCallback((change: (d: FaceDesign) => FaceDesign) => {
    setDraft((d) => (d ? change(d) : d));
    setDirty(true);
  }, []);

  const start = () => {
    if (!surfaces[0]) {
      setReport({ lines: [], error: 'Import a grid or surface first: a profile registers its toes to one' });
      return;
    }
    setId(null);
    setDraft(blankDesign(surfaces[0].id));
    setName(freeName('Face design', designs.map((d) => d.name)));
    setProfileName(null);
    setDirty(true);
    setReport({ lines: [] });
  };

  const save = async () => {
    if (!draft) return null;
    const outcome = await runRecord('face.save_design', { design: id, name, content: draft });
    if (!outcome.record) {
      setReport({ lines: [], error: outcome.error });
      return null;
    }
    const saved = outcome.record.outputs[0].id;
    setId(saved);
    setDirty(false);
    return saved;
  };

  const runAll = async () => {
    const saved = await save();
    if (!saved) return;
    const outcome = await runRecord('face.project', { design: saved });
    setReport(outcome.record ? { lines: outcome.record.messages } : { lines: [], error: outcome.error });
  };

  if (!draft) {
    return (
      <div className="flex h-full flex-col gap-2 overflow-auto p-3 text-xs">
        <p className="text-muted-foreground">A face design holds the lines to run, a table of steps for each, and any rules those steps share.</p>
        <div className="flex items-center gap-2">
          <SmallButton onClick={start}>
            <Plus className="size-3" /> New design
          </SmallButton>
          {designs.length > 0 && (
            <select className={selectClass} value="" aria-label="Open a design" onChange={(e) => setId(e.target.value)}>
              <option value="">Open a design</option>
              {designs.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          )}
        </div>
        {report.error && <p className="text-destructive">{report.error}</p>}
      </div>
    );
  }

  const profile: FaceProfile | undefined = draft.profiles.find((p) => p.name === profileName) ?? draft.profiles[0];
  const profileNames = draft.profiles.map((p) => p.name);
  const ruleNames = draft.rules.map((r) => r.name);
  const stepValues = draft.profiles.flatMap((p) => p.steps.map(stepValueOf));
  const setProfile = (change: Partial<FaceProfile>) => {
    if (!profile) return;
    const at = draft.profiles.indexOf(profile);
    edit((d) => ({ ...d, profiles: replaceAt(d.profiles, at, { ...d.profiles[at], ...change }) }));
  };
  const renameProfile = (to: string) => {
    if (!profile) return;
    const from = profile.name;
    edit((d) => ({ ...d, profiles: d.profiles.map((p) => (p.name === from ? { ...p, name: to } : p)), lines: d.lines.map((l) => (l.profile === from ? { ...l, profile: to } : l)) }));
    setProfileName(to);
  };
  const addProfile = (copy: boolean) => {
    const fresh = freeName(copy && profile ? `${profile.name} copy` : 'Profile', profileNames);
    const made_: FaceProfile = copy && profile ? { ...profile, name: fresh } : { name: fresh, direction: profile?.direction ?? 'up', register: profile?.register ?? surfaces[0]?.id ?? '', condition: usualTolerance, steps: [] };
    edit((d) => ({ ...d, profiles: [...d.profiles, made_] }));
    setProfileName(fresh);
  };
  const removeProfile = () => {
    if (!profile) return;
    edit((d) => ({ ...d, profiles: d.profiles.filter((p) => p !== profile) }));
    setProfileName(null);
  };
  const renameRule = (from: string, to: string) => {
    const follow = (s: ProfileStep): ProfileStep => (stepValueOf(s) !== from ? s : s.step === 'project' ? { ...s, batter: to } : { ...s, width: to });
    edit((d) => ({
      ...d,
      rules: d.rules.map((r) => (r.name === from ? { ...r, name: to } : r)),
      profiles: d.profiles.map((p) => ({ ...p, steps: p.steps.map(follow) })),
    }));
  };
  // A string can go in again under another profile, to go both ways from it.
  const addLine = (on: { string: string } | { layer: string }) => {
    const same = (l: FaceLine) => ('string' in on ? l.string === on.string : l.layer === on.layer);
    edit((d) => {
      const taken = d.lines.filter(same).map((l) => l.profile);
      const free = [profile?.name ?? '', ...d.profiles.map((p) => p.name)].find((p) => p && !taken.includes(p));
      if (!free) return d;
      const line: FaceLine = { ...on, profile: free, side: 'right', run_on: [100, 100] };
      return { ...d, lines: [...d.lines, line] };
    });
  };
  const chosen = strings.filter((s) => s.id === selected || marked.includes(s.id));
  const layers = (summary?.layers ?? []).filter((l) => strings.some((s) => s.layer === l.id));
  const nameOf = (objectId: string) => objects.find((o) => o.id === objectId)?.name ?? 'missing';
  const polygons = strings.filter((s) => s.closed);
  const selectedSolid = objects.find((o) => o.id === selected && o.kind === 'solid')?.id ?? null;
  const changeRule = (k: number, value: RuleValue) => edit((d) => ({ ...d, rules: replaceAt(d.rules, k, { ...d.rules[k], value }) }));
  /** A rule of a type, starting from a number. Polygons and solids it can't have are kept off the list. */
  const blankRule = (type: RuleValue['type'], at: number): RuleValue => {
    switch (type) {
      case 'fixed':
        return { type, value: at };
      case 'fixed_list':
        return { type, values: [at] };
      case 'inside_polygon':
        return { type, polygon: polygons[0].id, inside: at, outside: at };
      case 'inside_solid':
        return { type, solid: selectedSolid ?? '', inside: at, outside: at };
      case 'segment_azimuth':
        return { type, sectors: [], otherwise: at };
      case 'by_horizon':
        return { type, values: [], otherwise: at };
    }
  };
  /** The next click in the viewport, as a ramp's point. */
  const pickRampPoint = async (use: (at: [number, number]) => void, prompt = 'Click where the road has to come out. Esc cancels') => {
    setPicking(true);
    const point = await pickPoint(prompt);
    setPicking(false);
    if (point) use([toMillimetres(point[0]), toMillimetres(point[1])]);
  };
  /** The strings a profile runs on: each line's own, and what's on a line's layer. */
  const stringsOf = (name: string) => strings.filter((s) => draft.lines.some((l) => l.profile === name && (l.string === s.id || l.layer === s.layer)));
  /** Of those, the one nearest a point in plan, as what a click on a line most likely meant. */
  const nearestString = (name: string, at: [number, number]) => {
    const origin = summary?.origin ?? [0, 0, 0];
    const [x, y] = [at[0] - origin[0], at[1] - origin[1]];
    const gap = (s: ObjectSummary) => {
      const line = s.vertices_local ?? [];
      let least = Infinity;
      for (let k = 1; k < line.length; k++) {
        const [a, b] = [line[k - 1], line[k]];
        const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
        const t = Math.max(0, Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy || 1)));
        least = Math.min(least, Math.hypot(a[0] + t * dx - x, a[1] + t * dy - y));
      }
      return least;
    };
    return stringsOf(name).reduce<ObjectSummary | undefined>((best, s) => (!best || gap(s) < gap(best) ? s : best), undefined)?.id;
  };
  /** A new ramp where the next click says, up the walls that have no ramp yet and finish on a surface. */
  const addRamp = () => {
    if (!profile) return;
    void pickRampPoint((at) => {
      const taken = (profile.ramps ?? []).map((r) => r.name);
      const name = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').find((letter) => !taken.includes(letter)) ?? freeName('R', taken);
      const string = nearestString(profile.name, at);
      if (!string) return;
      const ramp: Ramp = { name, string, fixed: 'end', at, heading: 'along', width: usualRoadWidth, grade: usualGrade };
      setProfile({ ramps: [...(profile.ramps ?? []), ramp], steps: profile.steps.map((s) => (s.step === 'project' && !s.ramp && s.to.kind === 'surface' ? { ...s, ramp: name } : s)) });
    });
  };
  const setRamp = (at: number, change: Partial<Ramp>) => profile && setProfile({ ramps: replaceAt(profile.ramps ?? [], at, { ...(profile.ramps ?? [])[at], ...change }) });
  const removeRamp = (at: number) => {
    if (!profile) return;
    const name = (profile.ramps ?? [])[at].name;
    setProfile({ ramps: (profile.ramps ?? []).filter((_, k) => k !== at), steps: profile.steps.map((s) => (s.step === 'project' && s.ramp === name ? { ...s, ramp: null } : s)) });
  };
  const rampLine = (line: string) => /^Ramp [^:]*: /.test(line);
  const lineName = (line: FaceLine) => {
    if (!line.layer) return nameOf(line.string ?? '');
    const on = strings.filter((s) => s.layer === line.layer && s.attributes.face_design !== id).length;
    return `${summary?.layers.find((l) => l.id === line.layer)?.name ?? 'missing'} (${on} string${on === 1 ? '' : 's'})`;
  };

  return (
    <div className="h-full overflow-auto pb-3">
      <div className="flex items-center gap-1 px-3 pt-2">
        <select
          className={`${selectClass} max-w-40`}
          value={id ?? ''}
          aria-label="Design"
          onChange={(e) => {
            setDirty(false);
            setId(e.target.value || null);
          }}
        >
          {!id && <option value="">{name} (not saved)</option>}
          {designs.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </select>
        <Input className="h-7 min-w-0 flex-1 px-1 text-xs" value={name} aria-label="Design name" onChange={(e) => (setName(e.target.value), setDirty(true))} />
        <SmallButton onClick={start}>
          <Plus className="size-3" /> New
        </SmallButton>
        <SmallButton onClick={() => void save()} title="Save this design with the project">
          <Save className="size-3" /> Save{dirty ? ' *' : ''}
        </SmallButton>
      </div>

      <Heading>1. Lines</Heading>
      {draft.lines.length > 0 && (
        <div className="px-3">
          <table className="w-full text-xs">
            <thead className="text-left text-muted-foreground">
              <tr>
                <th className="px-1 font-normal">Line</th>
                <th className="px-1 font-normal">Profile</th>
                <th className="px-1 font-normal">Side</th>
                <th className="px-1 font-normal" title="How far the toe is extended past its start and end (m)">
                  Run on, m
                </th>
                <th />
              </tr>
            </thead>
            <tbody>
              {draft.lines.map((line, k) => {
                const change = (c: Partial<FaceLine>) => edit((d) => ({ ...d, lines: replaceAt(d.lines, k, { ...line, ...c }) }));
                return (
                  <tr key={k} className="border-t">
                    <td className="max-w-28 truncate px-1 py-0.5" title={line.layer ? `${lineName(line)}: what's on the layer at each run` : lineName(line)}>
                      {lineName(line)}
                    </td>
                    <td className="px-1 py-0.5">
                      <select className={`${selectClass} max-w-40`} value={line.profile} aria-label="Profile" onChange={(e) => change({ profile: e.target.value })}>
                        {draft.profiles.map((p) => (
                          <option key={p.name} value={p.name}>
                            {p.name} ({p.direction})
                          </option>
                        ))}
                      </select>
                    </td>
                    <td className="px-1 py-0.5">
                      <Choice value={line.side} options={SIDES} onChange={(side) => change({ side })} />
                    </td>
                    <td className="whitespace-nowrap px-1 py-0.5">
                      <NumberBox width="w-12" value={line.run_on[0]} title="Past the start (m)" onChange={(v) => change({ run_on: [v, line.run_on[1]] })} />{' '}
                      <NumberBox width="w-12" value={line.run_on[1]} title="Past the end (m)" onChange={(v) => change({ run_on: [line.run_on[0], v] })} />
                    </td>
                    <td className="py-0.5 text-right">
                      <IconButton title="Remove this line" onClick={() => edit((d) => ({ ...d, lines: d.lines.filter((_, j) => j !== k) }))}>
                        <Trash2 className="size-3" />
                      </IconButton>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-1 px-3 pt-1">
        <SmallButton
          disabled={chosen.length === 0}
          title="A line for each string selected. Ctrl-click strings in the viewport, or Ctrl or Shift in the tree, to select several"
          onClick={() => {
            chosen.forEach((s) => addLine({ string: s.id }));
            setMarked([]);
          }}
        >
          <Plus className="size-3" /> Apply to selected{chosen.length > 0 ? ` (${chosen.length})` : ''}
        </SmallButton>
        <select className={`${selectClass} max-w-32`} value={addingLayer} aria-label="A layer of strings to add" onChange={(e) => setAddingLayer(e.target.value)}>
          <option value="">A layer…</option>
          {layers.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>
        <SmallButton disabled={!addingLayer} title="One line for the layer: each run takes the strings on it at the time" onClick={() => (addLine({ layer: addingLayer }), setAddingLayer(''))}>
          Apply to layer
        </SmallButton>
      </div>

      <Heading
        action={
          <>
            <SmallButton onClick={() => addProfile(false)}>New</SmallButton>
            <SmallButton onClick={() => addProfile(true)}>Copy</SmallButton>
            <SmallButton disabled={!profile || draft.lines.some((l) => l.profile === profile.name)} title="A profile a line uses can't be removed" onClick={removeProfile}>
              Remove
            </SmallButton>
          </>
        }
      >
        2. Profile
      </Heading>
      {profile && (
        <>
          <div className="flex items-center gap-1 px-3 pb-1 text-xs">
            <select className={`${selectClass} max-w-32`} value={profile.name} aria-label="Profile to edit" onChange={(e) => setProfileName(e.target.value)}>
              {profileNames.map((p) => (
                <option key={p}>{p}</option>
              ))}
            </select>
            <Input className="h-7 w-28 px-1 text-xs" value={profile.name} aria-label="Profile name" onChange={(e) => renameProfile(e.target.value)} />
            <span className="ml-auto text-muted-foreground">Direction</span>
            <Choice value={profile.direction} options={DIRECTIONS} onChange={(direction) => setProfile({ direction })} />
          </div>
          <StepTable profile={profile} surfaces={surfaces} rules={draft.rules} onChange={setProfile} />
          <div className="px-3 pt-1 text-xs">
            <div className="flex items-center gap-2 pb-1">
              <span className="text-muted-foreground">Ramps</span>
              <SmallButton disabled={picking || profile.steps.length === 0 || stringsOf(profile.name).length === 0} title="Then click where the road has to come out" onClick={addRamp}>
                <Plus className="size-3" /> Ramp
              </SmallButton>
              {picking && <span className="text-primary">Click where the road has to come out. Esc cancels.</span>}
            </div>
            {(profile.ramps ?? []).map((ramp, k) => (
              <RampRow
                key={ramp.name}
                ramp={ramp}
                strings={stringsOf(profile.name)}
                results={report.lines.filter((line) => line.startsWith(`Ramp ${ramp.name}: `))}
                picking={picking}
                onPick={(use) => void pickRampPoint(use, ramp.fixed === 'end' ? undefined : 'Click the toe where the road starts. Esc cancels')}
                onChange={(change) => setRamp(k, change)}
                onRemove={() => removeRamp(k)}
              />
            ))}
          </div>
        </>
      )}

      <Heading
        action={
          <SmallButton onClick={() => edit((d) => ({ ...d, rules: [...d.rules, { name: freeName('Rule', ruleNames), value: { type: 'fixed', value: 0 } }] }))}>
            <Plus className="size-3" /> Add rule
          </SmallButton>
        }
      >
        3. Rules
      </Heading>
      <p className="px-3 pb-1 text-xs text-muted-foreground">
        A rule does nothing until a row names it: type or pick its name in that row's Batter / Width in place of a number. A rule can change along a wall, with the flatter batter or wider berm reaching across the change.
      </p>
      <div className="px-3">
        <table className="w-full text-xs">
          <thead className="text-left text-muted-foreground">
            <tr>
              <th className="px-1 font-normal">Name</th>
              <th className="px-1 font-normal">Type</th>
              <th className="w-full px-1 font-normal" title="Degrees for a batter, metres for a berm">
                Value
              </th>
              <th />
            </tr>
          </thead>
          <tbody>
            {draft.rules.map((rule, k) => (
              <tr key={k} className="border-t">
                <td className="px-1 py-0.5">
                  <Input className="h-7 w-32 px-1 text-xs" value={rule.name} aria-label="Rule name" onChange={(e) => renameRule(rule.name, e.target.value)} />
                </td>
                <td className="px-1 py-0.5 align-top">
                  <select
                    className={`${selectClass} w-28`}
                    value={rule.value.type}
                    aria-label="Rule type"
                    onChange={(e) => changeRule(k, blankRule(e.target.value as RuleValue['type'], firstNumber(rule.value)))}
                  >
                    {RULE_TYPES.map(({ type, label }) => (
                      <option key={type} value={type} disabled={(type === 'inside_polygon' && polygons.length === 0) || (type === 'inside_solid' && !selectedSolid)}>
                        {label}
                      </option>
                    ))}
                  </select>
                </td>
                <td className="px-1 py-0.5">
                  <RuleDetail
                    rule={rule.value}
                    polygons={polygons}
                    solidName={rule.value.type === 'inside_solid' ? nameOf(rule.value.solid) : ''}
                    selectedSolid={selectedSolid}
                    surfaces={surfaces}
                    onChange={(value) => changeRule(k, value)}
                  />
                  <RuleNote rule={rule} profiles={draft.profiles} />
                </td>
                <td className="py-0.5 text-right">
                  <IconButton title="Remove this rule" disabled={stepValues.includes(rule.name)} onClick={() => edit((d) => ({ ...d, rules: d.rules.filter((_, j) => j !== k) }))}>
                    <Trash2 className="size-3" />
                  </IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <Heading>4. Run</Heading>
      <div className="flex items-center gap-1 px-3 text-xs">
        <span className="text-muted-foreground">On layer</span>
        <Input className="h-7 min-w-0 flex-1 px-1 text-xs" value={draft.layer} aria-label="Layer for what a run makes" onChange={(e) => edit((d) => ({ ...d, layer: e.target.value }))} />
        <Button size="sm" className="h-7 gap-1 px-2 text-xs" onClick={() => void runAll()}>
          <Play className="size-3" /> Run all lines
        </Button>
      </div>
      {report.error && <p className="px-3 pt-2 text-xs text-destructive">{report.error}</p>}
      {report.lines
        .filter((line) => !rampLine(line))
        .map((line, k) => (
          <p key={k} className="px-3 pt-2 text-xs text-muted-foreground">
            {line}
          </p>
        ))}
    </div>
  );
}
