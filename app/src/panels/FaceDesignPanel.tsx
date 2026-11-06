import { ArrowDown, ArrowLeftRight, ArrowUp, Crosshair, Footprints, Move, Play, Plus, Save, Trash2, Undo2 } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { type AzimuthSector, type FaceDesign, type FaceDirection, type FaceLine, type FaceProfile, type HorizonValue, objectDetails, type ObjectSummary, type ProfileStep, type Ramp, type RampBerm, type RampFixed, type RampHeading, type RuleValue, type StepTarget, type StepValue, type Vec3, type WallRule, type WallSide } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { rampColour } from '@/viewport/rampColours';
import { useWorkspace } from '@/workspace/context';

const selectClass = 'h-7 min-w-0 rounded border bg-background px-1 text-xs';
const headClass = 'px-1 font-normal';
const cellClass = 'px-1 py-0.5';

/** How long a step design waits after a change before it runs, so typing a number runs once (ms). */
const STEP_WAIT = 400;

/** What a new row starts with. */
const usualTolerance = 0.1;
const usualBatter = 70;
const usualBerm = 10;
const usualMinBench = 1;
/** How far a new wall to an RL rises past the one before it, or past the toe (m). */
const usualBenchHeight = 10;
const usualRoadWidth = 30;
const usualGrade = 10;
const usualTurnJoin = 50;
/** A switchback box's far end square across the strip, unless a ramp says otherwise (degrees). */
const squareEnd = 90;

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

const blankDesign = (register: StepTarget): FaceDesign => ({
  layer: 'faces',
  lines: [],
  profiles: [{ name: 'Highwall', direction: 'up', register, condition: usualTolerance, steps: [] }],
  rules: [],
});

/** The RL a string lies flat on, to the millimetre, if it does and isn't at 0: a toe drawn on a bench. */
function levelOf(string: ObjectSummary, origin: Vec3 | null | undefined): StepTarget | null {
  const zs = (string.vertices_local ?? []).map((v) => v[2]);
  if (zs.length === 0 || !origin) return null;
  const rl = Math.round((Math.min(...zs) + origin[2]) * 1000) / 1000;
  return Math.max(...zs) - Math.min(...zs) < 0.001 && rl !== 0 ? { kind: 'rl', rl } : null;
}

/** How far a ramp placed by clicking reaches square across the line, behind it on the pit side and out the way the walls
 * build (m). Behind, enough to cross the line wherever conditioning moves it; out, past a berm put in before the wall
 * that climbs it. Only where it crosses the toe of the first wall it climbs counts. */
const RAMP_BEHIND = 5;
const RAMP_OUT = 40;
/** How near one of the design's lines a ramp has to be placed to start on it (m). Clicks snap to strings. */
const ON_A_LINE = 2;

/** How far a point is from a string, in 3D. */
function gapTo(points: Vec3[], at: Vec3): number {
  let best = Infinity;
  for (let k = 1; k < points.length; k++) {
    const [a, b] = [points[k - 1], points[k]];
    const d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    const length = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    const t = length > 0 ? Math.max(0, Math.min(1, ((at[0] - a[0]) * d[0] + (at[1] - a[1]) * d[1] + (at[2] - a[2]) * d[2]) / length)) : 0;
    best = Math.min(best, Math.hypot(a[0] + t * d[0] - at[0], a[1] + t * d[1] - at[1], a[2] + t * d[2] - at[2]));
  }
  return best;
}

/** The way a string runs in plan at the place on it nearest `at`, as a unit vector. */
const wayAt = (points: [number, number][], at: Vec3) => chainageAt(points, at)?.way ?? null;

/** How far along a string, from its first vertex in plan, the place on it nearest `at` is (m), and its way there. */
function chainageAt(points: [number, number][], at: Vec3): { metres: number; way: [number, number] } | null {
  let best: { gap: number; metres: number; way: [number, number] } | null = null;
  let walked = 0;
  for (let k = 1; k < points.length; k++) {
    const [a, b] = [points[k - 1], points[k]];
    const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    const length = Math.hypot(dx, dy);
    if (length === 0) continue;
    const t = Math.max(0, Math.min(1, ((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / (length * length)));
    const gap = Math.hypot(a[0] + t * dx - at[0], a[1] + t * dy - at[1]);
    if (!best || gap < best.gap) best = { gap, metres: walked + t * length, way: [dx / length, dy / length] };
    walked += length;
  }
  return best && { metres: best.metres, way: best.way };
}

const COMPASS = ['north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west', 'north-west'];

/** A way in plan as one of eight compass points. */
function compassOf([x, y]: [number, number]): string {
  const bearing = ((Math.atan2(x, y) * 180) / Math.PI + 360) % 360;
  return COMPASS[Math.round(bearing / 45) % 8];
}

/** Where a ramp placed by clicking starts, in words: how far along the design's string, the step it climbs from, and
 * the way it climbs, and the way it would climb flipped. */
type Placed = { line: string; metres: number; from: number | null; climbs: string; flipped: string };

/** A design on one string, as a step design is. */
const onOneString = (design: FaceDesign) => design.lines.length === 1 && design.lines[0].string !== undefined;

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

/** What a new profile's toe starts on: the first surface, or an RL if there's none. */
const firstRegister = (surfaces: ObjectSummary[]): StepTarget => (surfaces[0] ? { kind: 'surface', id: surfaces[0].id } : { kind: 'rl', rl: 0 });

/** Where a new step should start: the surface after the last one the table went to, or after the toe's own. On RLs,
 * a bench height past the last RL, up or down the way the profile goes. */
function nextTarget(steps: ProfileStep[], surfaces: ObjectSummary[], register: StepTarget, direction: FaceDirection): StepTarget {
  const lastWall = [...steps].reverse().find((s) => s.step === 'project');
  const from = lastWall?.step === 'project' ? lastWall.to : register;
  // A toe on an RL stays on RLs until a row names a surface, and past the last surface a face goes on to an RL.
  const next = from.kind === 'surface' ? surfaces[surfaces.findIndex((s) => s.id === from.id) + 1] : undefined;
  if (next) return { kind: 'surface', id: next.id };
  const lastRl = [...steps].reverse().find((s) => s.step === 'project' && s.to.kind === 'rl');
  const rl = lastRl?.step === 'project' && lastRl.to.kind === 'rl' ? lastRl.to.rl : register.kind === 'rl' ? register.rl : null;
  return { kind: 'rl', rl: rl === null ? 0 : rl + (direction === 'down' ? -usualBenchHeight : usualBenchHeight) };
}

/** A new step of a kind, starting from the table's last one of that kind. */
function blankStep(kind: ProfileStep['step'], steps: ProfileStep[], surfaces: ObjectSummary[], register: StepTarget, direction: FaceDirection, condition: number | null): ProfileStep {
  const last = [...steps].reverse().find((s) => s.step === kind);
  if (kind === 'project') return { step: 'project', to: nextTarget(steps, surfaces, register, direction), batter: last ? stepValueOf(last) : usualBatter, condition };
  const minBench = last?.step === 'berm' ? last.min_bench : usualMinBench;
  return { step: 'berm', on: null, width: last ? stepValueOf(last) : usualBerm, register: true, condition, min_bench: minBench };
}

/** Whether a row hands its crest on as one wall with the row below: same batter, no berm between. */
function carriesOn(steps: ProfileStep[], at: number, rules: WallRule[]) {
  const [step, next] = [steps[at], steps[at + 1]];
  if (step.step !== 'project' || next?.step !== 'project') return false;
  if ((step.ramps ?? []).join() !== (next.ramps ?? []).join() || (next.turns ?? []).length > 0) return false;
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
function StepRow({ number, step, surfaces, rules, above, carries, ramps, bridged, canTurn, blank, onChange, onMove, onRemove }: { number: number; step: ProfileStep; surfaces: ObjectSummary[]; rules: WallRule[]; above: string; carries: boolean; ramps: Ramp[]; bridged: (ramp: string) => Bridge; canTurn: (ramp: string) => boolean; blank: (kind: ProfileStep['step']) => ProfileStep; onChange: (s: ProfileStep) => void; onMove: (by: number) => void; onRemove: () => void }) {
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
      {ramps.map(({ name }) => (
        <td key={name} className="px-0.5 py-0.5">
          {step.step === 'project' ? (
            <RampCell name={name} on={onWall(step, name)} canTurn={canTurn(name)} onChange={(on) => onChange(withRamp(step, name, on))} />
          ) : (
            bridged(name) !== 'none' && <span className="mx-auto block h-6 w-1" style={{ background: rampColour(name) }} title={bridged(name) === 'turns' ? `Ramp ${name} turns back on this berm: a flat box past the road's top, the road, the berm and the next road wide` : `Ramp ${name} crosses this berm: the road is the bench here`} />
          )}
        </td>
      ))}
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

/** How a berm carries a ramp. */
type Bridge = 'none' | 'crosses' | 'turns';

/** Whether a ramp is on a wall: not, climbing it, or turning back before it. */
type OnWall = 'off' | 'on' | 'turn';

function onWall(step: Extract<ProfileStep, { step: 'project' }>, ramp: string): OnWall {
  if (!(step.ramps ?? []).includes(ramp)) return 'off';
  return (step.turns ?? []).includes(ramp) ? 'turn' : 'on';
}

function withRamp(step: Extract<ProfileStep, { step: 'project' }>, ramp: string, on: OnWall): ProfileStep {
  const ramps = (step.ramps ?? []).filter((other) => other !== ramp);
  const turns = (step.turns ?? []).filter((other) => other !== ramp);
  return { ...step, ramps: on === 'off' ? ramps : [...ramps, ramp], turns: on === 'turn' ? [...turns, ramp] : turns };
}

/** Whether a ramp climbs a wall, filled in its colour if it does, so a ramp's column reads up the walls it climbs. A
 * click goes off, on, then turning back before the wall where the wall before holds the ramp too. */
function RampCell({ name, on, canTurn, onChange }: { name: string; on: OnWall; canTurn: boolean; onChange: (on: OnWall) => void }) {
  const colour = rampColour(name);
  const next: OnWall = on === 'off' ? 'on' : on === 'on' && canTurn ? 'turn' : 'off';
  const what = on === 'turn' ? `Ramp ${name} turns back before this wall, on a flat box past its top on the berm below, and climbs it the other way. Click to take it off` : `Click if ramp ${name} climbs this wall${canTurn ? ', and again if it turns back before it on a flat pad, a switchback' : ''}. Walls it climbs must come together, and berms between them are crossed. A wall can hold several ramps: running one way they stack one above the other, and running opposite ways, as a straight ramp beside a switchback, they sit side by side along it`;
  return (
    <button
      type="button"
      className="mx-auto flex h-6 w-5 items-center justify-center rounded-sm border text-white"
      style={on !== 'off' ? { background: colour, borderColor: colour } : undefined}
      aria-pressed={on !== 'off'}
      aria-label={on === 'turn' ? `Ramp ${name} turns back before this wall` : `Ramp ${name} on this wall`}
      title={what}
      onClick={() => onChange(next)}
    >
      {on === 'turn' && <Undo2 className="size-3" />}
    </button>
  );
}

/** The roads the design below left on these strings, by ramp name, each with the width and grade it was built at. */
function roadTops(on: ObjectSummary[]): Map<string, { width: number; grade: number }> {
  const tops = new Map<string, { width: number; grade: number }>();
  for (const s of on) {
    for (const key of Object.keys(s.attributes)) {
      const name = /^road_top (.+) pit$/.exec(key)?.[1];
      if (!name || tops.has(name)) continue;
      const number = (what: string, otherwise: number) => {
        const value = s.attributes[`road_top ${name} ${what}`];
        return typeof value === 'number' ? value : otherwise;
      };
      tops.set(name, { width: number('width', usualRoadWidth), grade: number('grade', usualGrade) });
    }
  }
  return tops;
}

/** One ramp: its string, what's held, the road, and how far the last run got it on each strip. */
function RampRow({ ramp, stringName, placed, onWalls, waits, turns, results, swap, onShow, onMove, onChange, onRemove }: { ramp: Ramp; stringName: string | null; placed: Placed | null; onWalls: boolean; waits: boolean; turns: boolean; results: string[]; swap: (() => void) | null; onShow: () => void; onMove: (() => void) | null; onChange: (r: Partial<Ramp>) => void; onRemove: () => void }) {
  const fromLine = ramp.fixed === 'line';
  const flip = () => onChange({ heading: ramp.heading === 'along' ? 'back' : 'along' });
  return (
    <div className="border-t py-1">
      <div className="flex flex-wrap items-center gap-2">
        <span className="w-7 rounded px-1 py-0.5 text-center font-medium text-white" style={{ background: rampColour(ramp.name) }}>
          {ramp.name}
        </span>
        {placed ? (
          <>
            <button type="button" className="truncate hover:text-primary" title="Where the road starts, measured along the string the design runs on. Click to light up its marker" onClick={onShow}>
              Starts {Math.round(placed.metres).toLocaleString()} m along {placed.line}
              {placed.from !== null && `, up from step ${placed.from}`}
            </button>
            <SmallButton disabled={!onMove} title="Click a new start on the line, then the way the road runs" onClick={() => onMove?.()}>
              <Move className="size-3" /> Move
            </SmallButton>
            <SmallButton title={`Climbs ${placed.climbs}. Click to climb ${placed.flipped} instead, from the same start`} onClick={flip}>
              <ArrowLeftRight className="size-3" /> Climbs {placed.climbs}
            </SmallButton>
          </>
        ) : (
          <>
            <select
              className={selectClass}
              value={ramp.fixed}
              aria-label="Where the road is set"
              title={`Starts: the road starts where the string crosses each strip. Ends: it comes out on the top crest level with where the string crosses each strip, and the run moves its start along the toe to get it there. From line: no string, and each line carries on road ${ramp.name} from where it came out on that line, as the design below left it`}
              onChange={(e) => onChange({ fixed: e.target.value as RampFixed })}
            >
              <option value="start">Starts</option>
              <option value="end">Ends</option>
              <option value="line">From line</option>
            </select>
            {fromLine ? (
              <span className="text-muted-foreground" title={`No string: each line carries on road ${ramp.name} from where the design below left it, heading the way it was`}>
                carries on from the design below
              </span>
            ) : (
              <>
                <span className="text-muted-foreground">where</span>
                <button type="button" className={`max-w-36 truncate hover:text-primary ${stringName ? '' : 'text-primary'}`} title={stringName ? `The string across the strips. Click to light it up, then drag its vertices and run again` : 'Its string has gone'} onClick={onShow}>
                  {stringName ?? 'String gone'}
                </button>
                <IconButton title="Use the string selected in the viewport instead. It has to run across the strips, so a strip itself can't be one" disabled={!swap} onClick={() => swap?.()}>
                  <Crosshair className="size-3" />
                </IconButton>
                <span className="text-muted-foreground">crosses, climbing</span>
                <span title="Along: the way each strip's string runs, from its first vertex to its last. Back: the other way">
                  <Choice value={ramp.heading} options={HEADINGS} onChange={(heading) => onChange({ heading })} />
                </span>
              </>
            )}
          </>
        )}
        <select
          className={selectClass}
          value={ramp.berm ?? 'after'}
          aria-label="How it gets past a berm"
          title="After berm: the berm keeps its full width and opens onto the top of the road, which crosses it level and straight across and starts up the next wall from its toe. Through berm: the road carries on up at its grade and the berm narrows to nothing where it comes through. A road that turns back on a berm always crosses it after"
          onChange={(e) => onChange({ berm: e.target.value as RampBerm })}
        >
          <option value="after">After berm</option>
          <option value="through">Through berm</option>
        </select>
        <Labelled label="Width, m">
          <NumberBox width="w-12" value={ramp.width} title="Road width in metres, square to the road" onChange={(width) => onChange({ width })} />
        </Labelled>
        <Labelled label="Grade, %">
          <NumberBox width="w-12" value={ramp.grade} title="Rise over plan run in percent, on the pit-side edge" onChange={(grade) => onChange({ grade })} />
        </Labelled>
        {turns && (
          <Labelled label="Join, m">
            <NumberBox
              width="w-12"
              value={ramp.turn_join ?? usualTurnJoin}
              title="Where the road turns back with no box (a flat of 0), how far on from the turn, along the road, the toe of the wall above runs straight back to the berm's line from the next road's corner, in metres. Longer than the line goes on, it runs to the end of the string"
              onChange={(turn_join) => onChange({ turn_join })}
            />
          </Labelled>
        )}
        {turns && (
          <Labelled label="Flat, m">
            <NumberBox
              width="w-12"
              value={ramp.turn_flat ?? ramp.width}
              title="Where the road turns back, how long the flat box past its top is, in line with it and on the floor on a grid, in metres, for trucks to turn on: the road, the berm and the next road wide. The next road climbs the other way from its near end, straight across the berm from the top, and the wall above goes round the box. The road's width unless you set it; 0 turns at the top with no box"
              onChange={(turn_flat) => onChange({ turn_flat })}
            />
          </Labelled>
        )}
        {turns && (
          <Labelled label="End, °">
            <NumberBox
              width="w-12"
              value={ramp.turn_end ?? squareEnd}
              title="Where the road turns back on a box, the angle in plan between the box's far end and the strip, in degrees, 30 to 90. 90 is square across, the wall above coming straight down the far end. Less flares it out past the box: the wall runs from the box's back corner at that angle out to the berm's line past it, and the ground it leaves is flat like the box. With a 100 m deep box, 45 runs 100 m on past the box"
              onChange={(turn_end) => onChange({ turn_end })}
            />
          </Labelled>
        )}
        <span className="ml-auto">
          <IconButton title="Remove this ramp" onClick={onRemove}>
            <Trash2 className="size-3" />
          </IconButton>
        </span>
      </div>
      {!onWalls && <p className="pt-0.5 text-primary">{waits ? 'Not on a wall yet: the next wall you add climbs it.' : 'Not on a wall yet: click the walls it climbs in its column above.'}</p>}
      {results.map((line, k) => (
        <p key={k} className={`pt-0.5 ${/stopped|isn't|doesn't|comes out|gone/.test(line) ? 'text-primary' : 'text-muted-foreground'}`}>
          {line.replace(/^Ramp [^:]*: /, '')}
        </p>
      ))}
    </div>
  );
}

/** A profile as one table, read top to bottom: the toe's Register row, then each step going one way from it. */
function StepTable({ profile, surfaces, rules, carryRamps, onChange }: { profile: FaceProfile; surfaces: ObjectSummary[]; rules: WallRule[]; carryRamps: boolean; onChange: (p: Partial<FaceProfile>) => void }) {
  const { steps, register } = profile;
  const ramps = profile.ramps ?? [];
  const blank = (kind: ProfileStep['step']) => blankStep(kind, steps, surfaces, register, profile.direction, usualTolerance);
  /** A new step. In a step design a new wall carries on the roads up the wall below it, and any ramp placed since. */
  const add = (kind: ProfileStep['step']) => {
    const row = blank(kind);
    if (carryRamps && row.step === 'project') {
      const below = [...steps].reverse().find((s) => s.step === 'project');
      const climbing = below?.step === 'project' ? (below.ramps ?? []) : [];
      const placed = ramps.map((r) => r.name).filter((name) => !steps.some((s) => s.step === 'project' && (s.ramps ?? []).includes(name)));
      row.ramps = [...climbing, ...placed];
    }
    onChange({ steps: [...steps, row] });
  };
  /** Whether a ramp crosses the berm at a row: it's on the walls either side of it. */
  /** The first wall in `from`, if it holds the ramp. */
  const climbedBy = (from: ProfileStep[], ramp: string) => {
    const wall = from.find((s) => s.step === 'project');
    return wall?.step === 'project' && (wall.ramps ?? []).includes(ramp) ? wall : undefined;
  };
  const climbsBefore = (at: number, ramp: string) => climbedBy(steps.slice(0, at).reverse(), ramp) !== undefined;
  /** How a berm carries a ramp: as the bench under a road crossing it, as the pad the road turns back on, or not at all. */
  const bridged = (at: number, ramp: string): Bridge => {
    const next = climbedBy(steps.slice(at + 1), ramp);
    if (!next || !climbsBefore(at, ramp)) return 'none';
    return (next.turns ?? []).includes(ramp) ? 'turns' : 'crosses';
  };
  const setSteps = (next: ProfileStep[]) => onChange({ steps: next });
  const nameOf = (target: StepTarget) => (target.kind === 'rl' ? `RL ${target.rl}` : (surfaces.find((s) => s.id === target.id)?.name ?? 'missing'));
  /** What the rows above a row last reached, which a berm registers on unless it names another. */
  const reached = (at: number) => {
    const last = steps.slice(0, at).reverse().map(targetOf).find((t) => t);
    return nameOf(last ?? register);
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
              {ramps.map(({ name }) => (
                <th key={name} className="px-0.5 text-center font-normal" title={`Ramp ${name}: click the walls it climbs`}>
                  <span className="inline-block w-5 rounded-sm font-medium text-white" style={{ background: rampColour(name) }}>
                    {name}
                  </span>
                </th>
              ))}
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
                <select className={`${selectClass} w-full max-w-36`} value={register.kind === 'surface' ? register.id : 'rl'} aria-label="Register toe to" title="What the toe registers to: a surface it follows, or – for an RL it lies flat on" onChange={(e) => onChange({ register: e.target.value === 'rl' ? { kind: 'rl', rl: 0 } : { kind: 'surface', id: e.target.value } })}>
                  <SurfaceOptions surfaces={surfaces} />
                  <option value="rl">–</option>
                </select>
              </td>
              <td className={cellClass}>
                <NumberBox width="w-14" value={register.kind === 'rl' ? register.rl : null} title="The RL the toe lies flat on" onChange={(rl) => onChange({ register: { kind: 'rl', rl } })} />
              </td>
              <td />
              <td />
              {ramps.map(({ name }) => (
                <td key={name} />
              ))}
              <td className={cellClass}>{register.kind === 'surface' && <RegisterCell checked />}</td>
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
                bridged={(ramp) => bridged(k, ramp)}
                canTurn={(ramp) => climbsBefore(k, ramp)}
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
  const { summary, selected, select, marked, setMarked, runRecord, clearRefusal, pickString, pickLine } = useWorkspace();
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
  /** Each change saves the design and runs it, one undo a step: a step design. */
  const [stepping, setStepping] = useState(false);
  /** Edits made, so a run only marks the design saved if nothing changed while it ran. */
  const edits = useRef(0);
  /** The design whose steps were last loaded, so opening another sets whether it runs each step. */
  const loaded = useRef<string | null>(null);

  const open = designs.find((d) => d.id === id);
  const revision = open?.revision;
  // What a design's own runs made are results, not things to design with. Another design's crest can be a line, as a
  // planner designs bottom up from the crest of the last (decision 0041).
  const made = (o: ObjectSummary) => o.attributes.face_design !== undefined;
  const strings = useMemo(() => objects.filter((o) => o.kind === 'string' && o.attributes.face_design !== id), [objects, id]);
  const surfaces = useMemo(() => objects.filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid') && !made(o)), [objects]);

  useEffect(() => {
    if (!id || dirty) return;
    let stale = false;
    void objectDetails(id).then((details) => {
      if (stale || details.measures.kind !== 'face_design') return;
      setDraft(details.measures.design);
      setName(details.name);
      if (loaded.current !== id) setStepping(onOneString(details.measures.design));
      loaded.current = id;
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
    edits.current += 1;
    // What a refused run said was about the design as it was. How far ramps got stays until the next run.
    setReport((was) => (was.error ? { lines: was.lines } : was));
    clearRefusal();
  }, [clearRefusal]);

  const start = () => {
    setId(null);
    setStepping(false);
    loaded.current = null;
    setDraft(blankDesign(firstRegister(surfaces)));
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

  // A step design runs a moment after each change, saved and run in one undo. One that can't run says why and stays
  // unsaved, with what the last step built still standing.
  const ready = !!draft && draft.profiles.every((p) => p.steps.length > 0);
  useEffect(() => {
    if (!stepping || !dirty || !id || !draft || !ready) return;
    const at = edits.current;
    const timer = setTimeout(() => {
      void runRecord('face.step', { design: id, name, content: draft }).then((outcome) => {
        if (!outcome.record) return setReport({ lines: [], error: outcome.error });
        if (edits.current === at) setDirty(false);
        setReport({ lines: outcome.record.messages });
      });
    }, STEP_WAIT);
    return () => clearTimeout(timer);
  }, [stepping, dirty, id, draft, name, ready, runRecord]);

  /** The string selected in the viewport, open, to start a step design from. */
  const from = objects.find((o) => o.id === selected && o.kind === 'string' && !o.closed);
  /** A step design from the selected string: saved empty, then each step added runs as it goes in. */
  const startSteps = async () => {
    if (!from) return;
    const register = levelOf(from, summary?.origin) ?? firstRegister(surfaces);
    const content: FaceDesign = { layer: 'faces', lines: [{ string: from.id, profile: 'Steps', side: 'right', run_on: [0, 0] }], profiles: [{ name: 'Steps', direction: 'up', register, condition: usualTolerance, steps: [] }], rules: [] };
    const named = freeName(`Steps from ${from.name}`, designs.map((d) => d.name));
    const outcome = await runRecord('face.save_design', { name: named, content });
    const saved = outcome.record?.outputs[0]?.id;
    if (!saved) return setReport({ lines: [], error: outcome.error });
    loaded.current = saved;
    setId(saved);
    setDraft(content);
    setName(named);
    setProfileName(null);
    setDirty(false);
    setStepping(true);
    setReport({ lines: [] });
  };
  const stepTitle = from
    ? `A face built by hand from ${from.name}, a step at a time: each step you add or change runs straight away, and Ctrl+Z takes it off`
    : 'Select a string in the viewport to start from: a toe, or a crest going down';

  if (!draft) {
    return (
      <div className="flex h-full flex-col gap-2 overflow-auto p-3 text-xs">
        <p className="text-muted-foreground">A face design holds the lines to run, a table of steps for each, and any rules those steps share.</p>
        <div className="flex items-center gap-2">
          <SmallButton onClick={start}>
            <Plus className="size-3" /> New design
          </SmallButton>
          <SmallButton disabled={!from} title={stepTitle} onClick={() => void startSteps()}>
            <Footprints className="size-3" /> Step design
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
    const made_: FaceProfile = copy && profile ? { ...profile, name: fresh } : { name: fresh, direction: profile?.direction ?? 'up', register: profile?.register ?? firstRegister(surfaces), condition: usualTolerance, steps: [] };
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
  /** A new ramp along a string drawn across the strips, up every wall. */
  const addRamp = async () => {
    if (!profile) return;
    setPicking(true);
    const points = await pickString('Click across the strips, a point on each. Enter finishes, Esc cancels');
    setPicking(false);
    if (!points) return;
    const taken = (profile.ramps ?? []).map((r) => r.name);
    const name = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').find((letter) => !taken.includes(letter)) ?? freeName('R', taken);
    const made = await runRecord('string.create', { name: `Ramp ${name}`, layer: 'ramps', points });
    const string = made.record?.outputs[0]?.id;
    if (!string) return;
    const ramp: Ramp = { name, string, fixed: 'end', heading: 'along', width: usualRoadWidth, grade: usualGrade, berm: 'after', turn_join: usualTurnJoin };
    setProfile({ ramps: [...(profile.ramps ?? []), ramp], steps: profile.steps.map((s) => (s.step === 'project' ? { ...s, ramps: [...(s.ramps ?? []), name] } : s)) });
  };
  /** The string a step design runs on. */
  const stepLine = draft && onOneString(draft) ? objects.find((o) => o.id === draft.lines[0].string) : undefined;
  /** A ramp placed by clicking, in a step design: it starts where it's clicked on one of the design's lines, running the
   * way clicked along it. Its string is a short one square across the line there. */
  const pickRampStart = async (): Promise<{ at: Vec3; heading: RampHeading; points: Vec3[] } | null> => {
    const origin = summary?.origin;
    if (!stepLine?.vertices_local || !origin || !draft) return null;
    setPicking(true);
    const picked = await pickLine('Click where the ramp starts: on a toe or a berm line it climbs every wall above, on the top line it waits for the next wall. Esc cancels', 'Click along the line the way the road runs. Esc cancels');
    setPicking(false);
    if (!picked) return null;
    const [at, towards] = picked;
    const way = wayAt(stepLine.vertices_local.map((v) => [v[0] + origin[0], v[1] + origin[1]]), at);
    if (!way) return null;
    const heading: RampHeading = (towards[0] - at[0]) * way[0] + (towards[1] - at[1]) * way[1] >= 0 ? 'along' : 'back';
    const out: [number, number] = draft.lines[0].side === 'left' ? [-way[1], way[0]] : [way[1], -way[0]];
    return { at, heading, points: [-RAMP_BEHIND, RAMP_OUT].map((d): Vec3 => [at[0] + out[0] * d, at[1] + out[1] * d, at[2]]) };
  };
  /** The row of the wall a ramp placed at `at` climbs first, from the design's own line it's on: the wall that line is the
   * start of, or for a crest the next wall up, past any berm. Null where no wall starts from it yet, on the berm's line
   * or the top crest: the next wall added climbs it. Undefined where it isn't on one of the design's lines. */
  const firstWallAt = (at: Vec3): number | null | undefined => {
    const origin = summary?.origin;
    if (!origin || !profile || !stepLine?.vertices_local || !id) return undefined;
    const [start, end] = profile.direction === 'up' ? ['toe', 'crest'] : ['crest', 'toe'];
    const base = `${stepLine.name} ${profile.name}`;
    const wallAfter = (row: number) => {
      const next = profile.steps.findIndex((s, k) => k > row && s.step === 'project');
      return next < 0 ? null : next;
    };
    const world = (v: Vec3): Vec3 => [v[0] + origin[0], v[1] + origin[1], v[2] + origin[2]];
    // The design's own string starts the first wall, wherever its register puts the toe: measured in plan.
    const flat = (v: Vec3): Vec3 => [v[0] + origin[0], v[1] + origin[1], at[2]];
    let best = { gap: gapTo(stepLine.vertices_local.map(flat), at), row: wallAfter(-1) };
    for (const o of objects) {
      if (o.kind !== 'string' || o.attributes.face_design !== id || !o.vertices_local) continue;
      const step = o.attributes.step;
      const row = o.name === `${base} next ${start}` ? null : typeof step !== 'number' ? undefined : o.name.startsWith(`${base} ${start} `) ? step - 1 : o.name.startsWith(`${base} ${end} `) ? wallAfter(step - 1) : undefined;
      if (row === undefined) continue;
      const gap = gapTo(o.vertices_local.map(world), at);
      if (gap < best.gap) best = { gap, row };
    }
    return best.gap <= ON_A_LINE ? best.row : undefined;
  };
  /** A ramp on every wall from `row` up, and off those below; none at all while it waits for the next wall. A wall it
   * turns back before keeps the turn. */
  const onWallsFrom = (steps: ProfileStep[], name: string, row: number | null) =>
    steps.map((s, k) => (s.step !== 'project' ? s : withRamp(s, name, row === null || k < row ? 'off' : onWall(s, name) === 'off' ? 'on' : onWall(s, name))));
  const notOnALine = "Click on one of this design's lines: the toe, a crest, or a berm's line";
  const placeRamp = async () => {
    if (!profile) return;
    const start = await pickRampStart();
    if (!start) return;
    const { at, heading, points } = start;
    const row = firstWallAt(at);
    if (row === undefined) return setReport((was) => ({ lines: was.lines, error: notOnALine }));
    const taken = (profile.ramps ?? []).map((r) => r.name);
    const name = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').find((letter) => !taken.includes(letter)) ?? freeName('R', taken);
    const made = await runRecord('string.create', { name: `Ramp ${name}`, layer: 'ramps', points });
    const string = made.record?.outputs[0]?.id;
    if (!string) return;
    const ramp: Ramp = { name, string, fixed: 'start', heading, width: usualRoadWidth, grade: usualGrade, berm: 'after', turn_join: usualTurnJoin };
    // On a wall's line it climbs that wall and every one above at once; on the berm's line or the top crest it waits.
    setProfile({ ramps: [...(profile.ramps ?? []), ramp], steps: onWallsFrom(profile.steps, name, row) });
  };
  /** A placed ramp moved: its short string goes where the new start is clicked, and the step runs again. */
  const moveRamp = async (at: number) => {
    const ramp = profile?.ramps?.[at];
    if (!profile || !ramp?.string) return;
    const start = await pickRampStart();
    if (!start) return;
    const row = firstWallAt(start.at);
    if (row === undefined) return setReport((was) => ({ lines: was.lines, error: notOnALine }));
    const string = ramp.string;
    for (const [index, to] of start.points.entries()) if (!(await runRecord('string.move_vertex', { string, index, to })).record) return;
    // The walls it climbs go with it, and the design changing runs the step.
    setProfile({ ramps: replaceAt(profile.ramps ?? [], at, { ...ramp, heading: start.heading }), steps: onWallsFrom(profile.steps, ramp.name, row) });
  };
  /** Where a placed ramp starts and the way it climbs, in words, from its short string: a step design's ramps held at
   * their start. A ramp on a longer string reads as one across the strips. */
  const placedAt = (ramp: Ramp): Placed | null => {
    const origin = summary?.origin;
    const stub = objects.find((o) => o.id === ramp.string)?.vertices_local;
    if (!stepping || ramp.fixed !== 'start' || !stepLine?.vertices_local || !origin || !profile || stub?.length !== 2) return null;
    // Where it was clicked: its string's length behind the line from the end on the pit side.
    const reach = Math.hypot(stub[1][0] - stub[0][0], stub[1][1] - stub[0][1]);
    const t = reach > 0 ? Math.min(1, RAMP_BEHIND / reach) : 0;
    const clicked = [0, 1, 2].map((j) => stub[0][j] + t * (stub[1][j] - stub[0][j]) + origin[j]) as Vec3;
    const on = chainageAt(stepLine.vertices_local.map((v) => [v[0] + origin[0], v[1] + origin[1]]), clicked);
    if (!on) return null;
    const way: [number, number] = ramp.heading === 'along' ? on.way : [-on.way[0], -on.way[1]];
    const first = profile.steps.findIndex((s) => s.step === 'project' && (s.ramps ?? []).includes(ramp.name));
    return { line: stepLine.name, metres: on.metres, from: first < 0 ? null : first + 1, climbs: compassOf(way), flipped: compassOf([-way[0], -way[1]]) };
  };
  /** The roads the design below left on this profile's lines that it has no ramp for yet. */
  const newTops = (() => {
    if (!profile || !draft) return new Map<string, { width: number; grade: number }>();
    const on = draft.lines.filter((l) => l.profile === profile.name).flatMap((l) => strings.filter((s) => (l.string ? s.id === l.string : s.layer === l.layer)));
    const tops = roadTops(on);
    for (const ramp of profile.ramps ?? []) tops.delete(ramp.name);
    return tops;
  })();
  /** A ramp for each road the design below left on the lines, carrying it on from there, up every wall. */
  const addFromLine = () => {
    if (!profile || newTops.size === 0) return;
    const added: Ramp[] = [...newTops].map(([name, { width, grade }]) => ({ name, fixed: 'line', heading: 'along', width, grade, berm: 'after', turn_join: usualTurnJoin }));
    const names = added.map((r) => r.name);
    setProfile({ ramps: [...(profile.ramps ?? []), ...added], steps: profile.steps.map((s) => (s.step === 'project' ? { ...s, ramps: [...(s.ramps ?? []), ...names] } : s)) });
  };
  const setRamp = (at: number, change: Partial<Ramp>) => profile && setProfile({ ramps: replaceAt(profile.ramps ?? [], at, { ...(profile.ramps ?? [])[at], ...change }) });
  const removeRamp = (at: number) => {
    if (!profile) return;
    const { name, string } = (profile.ramps ?? [])[at];
    setProfile({ ramps: (profile.ramps ?? []).filter((_, k) => k !== at), steps: profile.steps.map((s) => (s.step === 'project' ? withRamp(s, name, 'off') : s)) });
    // A string drawn for the ramp goes with it. Undo brings it back.
    const used = draft?.profiles.some((p) => (p.ramps ?? []).some((r) => r.string === string && (p !== profile || r.name !== name)));
    if (string && !used && objects.find((o) => o.id === string)?.name === `Ramp ${name}`) void runRecord('object.delete', { object: string });
  };
  const rampLine = (line: string) => /^Ramp [^:]*: /.test(line);
  /** A click on a line lights up what it runs on, in the viewport and the tree. */
  const showLine = (line: FaceLine) => {
    if (line.string) {
      setMarked([]);
      select(line.string);
    } else {
      setMarked(strings.filter((s) => s.layer === line.layer && s.attributes.face_design !== id).map((s) => s.id));
    }
  };
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
        <SmallButton disabled={!from} title={stepTitle} onClick={() => void startSteps()}>
          <Footprints className="size-3" /> Steps
        </SmallButton>
        <SmallButton onClick={() => void save()} title="Save this design with the project">
          <Save className="size-3" /> Save{dirty ? ' *' : ''}
        </SmallButton>
      </div>

      {stepping && (
        <p className="px-3 pt-2 text-xs text-muted-foreground">
          Each step runs as you add or change it, and Ctrl+Z takes the last one off. Add a Project or Berm under the profile. The next step starts from the last crest, or after a berm from its next toe.
        </p>
      )}

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
                    <td
                      className={`max-w-28 cursor-pointer truncate px-1 py-0.5 hover:text-primary ${line.string && line.string === selected ? 'text-primary' : ''}`}
                      title={`${line.layer ? `${lineName(line)}: what's on the layer at each run` : lineName(line)}. Click to light it up`}
                      onClick={() => showLine(line)}
                    >
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
          <StepTable profile={profile} surfaces={surfaces} rules={draft.rules} carryRamps={stepping} onChange={setProfile} />
          <div className="px-3 pt-1 text-xs">
            <div className="flex items-center gap-2 pb-1">
              <span className="text-muted-foreground">Ramps</span>
              {stepping && (
                <SmallButton disabled={picking || !stepLine} title="Click where the road starts on one of the design's lines, then along the line the way it runs. On a toe or a berm's line it climbs every wall above at once; on the top line it waits, and the next wall you add climbs it. Click it off a wall in its column to stop it there" onClick={() => void placeRamp()}>
                  <Plus className="size-3" /> Place ramp
                </SmallButton>
              )}
              <SmallButton disabled={picking || profile.steps.length === 0} title="Then draw a string across the strips. Each line it crosses gets a road" onClick={() => void addRamp()}>
                <Plus className="size-3" /> Ramp
              </SmallButton>
              <SmallButton
                disabled={picking || profile.steps.length === 0 || newTops.size === 0}
                title={
                  newTops.size > 0
                    ? `Carry on ${[...newTops.keys()].map((n) => `road ${n}`).join(', ')} from where the design below left ${newTops.size === 1 ? 'it' : 'them'} on these lines, with no string`
                    : "None of this profile's lines has a road that came out on it. Run the design below, then add its top crests here as lines"
                }
                onClick={addFromLine}
              >
                <Plus className="size-3" /> From the line{newTops.size > 0 ? ` (${newTops.size})` : ''}
              </SmallButton>
              {picking && <span className="text-primary">{stepping ? 'Click where the road starts, then the way it runs. Esc cancels.' : 'Click across the strips, a point on each. Enter finishes, Esc cancels.'}</span>}
            </div>
            {(profile.ramps ?? []).map((ramp, k) => (
              <RampRow
                key={ramp.name}
                ramp={ramp}
                stringName={objects.find((o) => o.id === ramp.string)?.name ?? null}
                placed={placedAt(ramp)}
                onWalls={profile.steps.some((s) => s.step === 'project' && (s.ramps ?? []).includes(ramp.name))}
                waits={stepping}
                turns={profile.steps.some((s) => s.step === 'project' && (s.turns ?? []).includes(ramp.name))}
                results={report.lines.filter((line) => line.startsWith(`Ramp ${ramp.name}: `))}
                swap={ramp.fixed !== 'line' && objects.some((o) => o.id === selected && o.kind === 'string') && selected !== ramp.string && !strings.some((s) => s.id === selected && (s.attributes.strip !== undefined || draft.lines.some((line) => line.string === s.id))) ? () => selected && setRamp(k, { string: selected }) : null}
                onShow={() => ramp.string && select(ramp.string)}
                onMove={picking ? null : () => void moveRamp(k)}
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
      <label className="flex items-center gap-2 px-3 pt-2 text-xs" title="Save and run the design a moment after each change, one undo a step: a design built by hand a step at a time. Best on a design with one line, as every line runs each time">
        <Checkbox checked={stepping} disabled={!id} aria-label="Run each step" onCheckedChange={(ticked) => setStepping(ticked === true)} />
        Run each step{!id && ' (save the design first)'}
      </label>
      {report.error && <p className="whitespace-pre-line px-3 pt-2 text-xs text-destructive">{report.error}</p>}
      {report.lines
        .filter((line) => !rampLine(line))
        .map((line, k) => (
          <p key={k} className={`px-3 pt-2 text-xs ${line.endsWith('Nothing is built on this string') ? 'text-destructive' : 'text-muted-foreground'}`}>
            {line}
          </p>
        ))}
    </div>
  );
}
