import { ArrowDown, ArrowUp, Play, Plus, Save, Trash2 } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useId, useMemo, useState } from 'react';
import { type FaceDesign, type FaceDirection, type FaceLine, type FaceProfile, objectDetails, type ObjectSummary, type ProfileStep, type StepTarget, type StepValue, type WallRule, type WallSide } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { useWorkspace } from '@/workspace/context';

const selectClass = 'h-7 min-w-0 rounded border bg-background px-1 text-xs';
const headClass = 'px-1 font-normal';
const cellClass = 'px-1 py-0.5';

/** What a new row starts with. */
const usualTolerance = 0.1;
const usualBatter = 70;
const usualBerm = 10;

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

/** The number a value comes to, if it comes to one yet. */
function numberOf(value: StepValue, rules: WallRule[]) {
  return typeof value === 'number' ? value : rules.find((r) => r.name === value)?.value.value;
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
  return { step: 'berm', on: null, width: last ? stepValueOf(last) : usualBerm, register: true, condition };
}

/** Whether a row hands its crest on as one wall with the row below: same batter, no berm between. */
function carriesOn(steps: ProfileStep[], at: number, rules: WallRule[]) {
  const [step, next] = [steps[at], steps[at + 1]];
  if (step.step !== 'project' || next?.step !== 'project') return false;
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
function StepRow({ number, step, surfaces, rules, above, carries, blank, onChange, onMove, onRemove }: { number: number; step: ProfileStep; surfaces: ObjectSummary[]; rules: WallRule[]; above: string; carries: boolean; blank: (kind: ProfileStep['step']) => ProfileStep; onChange: (s: ProfileStep) => void; onMove: (by: number) => void; onRemove: () => void }) {
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

/** A profile as one table, read top to bottom: the toe's Register row, then each step going one way from it. */
function StepTable({ profile, surfaces, rules, onChange }: { profile: FaceProfile; surfaces: ObjectSummary[]; rules: WallRule[]; onChange: (p: Partial<FaceProfile>) => void }) {
  const { steps, register } = profile;
  const blank = (kind: ProfileStep['step']) => blankStep(kind, steps, surfaces, register, usualTolerance);
  const add = (kind: ProfileStep['step']) => onChange({ steps: [...steps, blank(kind)] });
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
  const { summary, selected, runRecord } = useWorkspace();
  const objects = useMemo(() => summary?.objects ?? [], [summary]);
  const designs = useMemo(() => objects.filter((o) => o.kind === 'face_design'), [objects]);
  const [id, setId] = useState<string | null>(null);
  const [name, setName] = useState('Face design');
  const [draft, setDraft] = useState<FaceDesign | null>(null);
  const [dirty, setDirty] = useState(false);
  const [profileName, setProfileName] = useState<string | null>(null);
  const [adding, setAdding] = useState('');
  const [addingLayer, setAddingLayer] = useState('');
  const [report, setReport] = useState<{ lines: string[]; error?: string }>({ lines: [] });

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
  const addLine = (string: string) => {
    edit((d) => {
      const taken = d.lines.filter((l) => l.string === string).map((l) => l.profile);
      const free = [profile?.name ?? '', ...d.profiles.map((p) => p.name)].find((p) => p && !taken.includes(p));
      if (!free) return d;
      const line: FaceLine = { string, profile: free, side: 'right', run_on: [100, 100] };
      return { ...d, lines: [...d.lines, line] };
    });
  };
  const selectedString = strings.find((s) => s.id === selected);
  const layers = (summary?.layers ?? []).filter((l) => strings.some((s) => s.layer === l.id));
  const nameOf = (objectId: string) => objects.find((o) => o.id === objectId)?.name ?? 'missing';

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
                    <td className="max-w-28 truncate px-1 py-0.5" title={nameOf(line.string)}>
                      {nameOf(line.string)}
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
        <SmallButton disabled={!selectedString} title="Adds the string selected in the viewport or Objects" onClick={() => selectedString && addLine(selectedString.id)}>
          <Plus className="size-3" /> Selected string
        </SmallButton>
        <select className={`${selectClass} max-w-32`} value={adding} aria-label="A string to add" onChange={(e) => setAdding(e.target.value)}>
          <option value="">A string…</option>
          {strings.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <SmallButton disabled={!adding} onClick={() => (addLine(adding), setAdding(''))}>
          Add
        </SmallButton>
        <select className={`${selectClass} max-w-32`} value={addingLayer} aria-label="A layer of strings to add" onChange={(e) => setAddingLayer(e.target.value)}>
          <option value="">A layer…</option>
          {layers.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>
        <SmallButton
          disabled={!addingLayer}
          onClick={() => {
            strings.filter((s) => s.layer === addingLayer).forEach((s) => addLine(s.id));
            setAddingLayer('');
          }}
        >
          Add all
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
      <p className="px-3 pb-1 text-xs text-muted-foreground">Name a value here to share it across steps, then type its name on a row.</p>
      <div className="px-3">
        <table className="w-full text-xs">
          <thead className="text-left text-muted-foreground">
            <tr>
              <th className="px-1 font-normal">Name</th>
              <th className="px-1 font-normal">Type</th>
              <th className="px-1 font-normal" title="Degrees for a batter, metres for a berm">
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
                <td className="px-1 py-0.5 text-muted-foreground">Fixed</td>
                <td className="px-1 py-0.5">
                  <NumberBox value={rule.value.value} title="Degrees for a batter, metres for a berm" onChange={(value) => edit((d) => ({ ...d, rules: replaceAt(d.rules, k, { ...rule, value: { type: 'fixed', value } }) }))} />
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
      {report.lines.map((line, k) => (
        <p key={k} className="px-3 pt-2 text-xs text-muted-foreground">
          {line}
        </p>
      ))}
    </div>
  );
}
