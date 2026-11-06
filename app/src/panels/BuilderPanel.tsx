import { Hammer, Plus, Trash2 } from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import { type BuildOp, type BuildRecord, objectDetails, type ObjectSummary, type Ramp, type RampBerm, type RampHeading, type StepTarget, type Vec3, type WallSide } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { rampColour } from '@/viewport/rampColours';
import { useWorkspace } from '@/workspace/context';

/**
 * The manual builder (decision 0048): pick a line, build one wall or berm from it, and edit any line between steps with the
 * ordinary string tools. Nothing is built again unless you ask. The steps of a build are listed in order: a click on one
 * picks its line and frames it, and picking a line in the viewport lights up its step.
 */

const selectClass = 'h-7 min-w-0 rounded border bg-background px-1 text-xs';

/** What a step starts with, where nothing before it says. */
const usualBatter = 70;
const usualBerm = 10;
/** How far a new wall to an RL rises past the last (m). */
const usualBenchHeight = 10;
const usualRoadWidth = 30;
const usualGrade = 10;
/** How far a road placed by clicking reaches square across the line, behind it on the pit side and out the way the walls
 * build (m): its string has to cross the line. */
const RAMP_BEHIND = 5;
const RAMP_OUT = 40;

/** How a step built a line, as the line keeps it. */
function recordOf(o: ObjectSummary | undefined): BuildRecord | null {
  const text = o?.attributes.build_record;
  if (typeof text !== 'string') return null;
  try {
    return JSON.parse(text) as BuildRecord;
  } catch {
    return null;
  }
}

/** What a line a step made lies on. */
function liesOnOf(o: ObjectSummary | undefined): StepTarget | null {
  const text = o?.attributes.build_on;
  if (typeof text !== 'string') return null;
  try {
    return JSON.parse(text) as StepTarget;
  } catch {
    return null;
  }
}

/** A road that came out on a line, as its top there says (decision 0041). */
interface RoadOnLine {
  name: string;
  width: number;
  grade: number;
  heading: RampHeading;
  berm: RampBerm;
}

function roadsOn(o: ObjectSummary | undefined): RoadOnLine[] {
  if (!o) return [];
  const a = o.attributes;
  return Object.keys(a)
    .filter((key) => key.startsWith('road_top ') && key.endsWith(' pit'))
    .map((key) => key.slice('road_top '.length, -' pit'.length))
    .sort()
    .map((name) => ({
      name,
      width: Number(a[`road_top ${name} width`]),
      grade: Number(a[`road_top ${name} grade`]),
      heading: a[`road_top ${name} heading`] === 'back' ? 'back' : 'along',
      berm: a[`road_top ${name} berm`] === 'through' ? 'through' : 'after',
    }));
}

/** The RL a string lies flat on, to the millimetre, if it does and isn't at 0: a toe drawn on a bench. */
function levelOf(string: ObjectSummary, origin: Vec3 | null | undefined): StepTarget | null {
  const zs = (string.vertices_local ?? []).map((v) => v[2]);
  if (zs.length === 0 || !origin) return null;
  const rl = Math.round((Math.min(...zs) + origin[2]) * 1000) / 1000;
  return Math.max(...zs) - Math.min(...zs) < 0.001 && rl !== 0 ? { kind: 'rl', rl } : null;
}

/** The way a string runs in plan at the place on it nearest `at`, as a unit vector. */
function wayAt(points: [number, number][], at: Vec3): [number, number] | null {
  let best: { gap: number; way: [number, number] } | null = null;
  for (let k = 1; k < points.length; k++) {
    const [a, b] = [points[k - 1], points[k]];
    const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    const length = Math.hypot(dx, dy);
    if (length === 0) continue;
    const t = Math.max(0, Math.min(1, ((at[0] - a[0]) * dx + (at[1] - a[1]) * dy) / (length * length)));
    const gap = Math.hypot(a[0] + t * dx - at[0], a[1] + t * dy - at[1]);
    if (!best || gap < best.gap) best = { gap, way: [dx / length, dy / length] };
  }
  return best?.way ?? null;
}

function targetWords(target: StepTarget | null | undefined, objects: ObjectSummary[]): string {
  if (!target) return 'what the line lies on';
  if (target.kind === 'rl') return `RL ${target.rl}`;
  return objects.find((o) => o.id === target.id)?.name ?? 'a surface that has gone';
}

const and = (names: string[]) => (names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`);

/** A step in words, as the list shows it. */
function stepWords(step: BuildOp, objects: ObjectSummary[]): string {
  const parts: string[] = [];
  if (step.op === 'project') {
    parts.push(`Wall to ${targetWords(step.to, objects)} at ${step.batter}°`);
    if (step.ramps?.length) parts.push(`new road ${and(step.ramps.map((r) => r.name))}`);
  } else {
    parts.push(`Berm ${step.width} m`);
    if (step.turns?.length) parts.push(`${and(step.turns)} turn${step.turns.length === 1 ? 's' : ''}`);
    if (step.join === true) parts.push('Join');
    if (step.join === false) parts.push('apart');
  }
  if (step.stop?.length) parts.push(`${and(step.stop)} end${step.stop.length === 1 ? 's' : ''}`);
  return parts.join(' · ');
}

/** One step of a build: the line it made, and how. Its wall and roads share its id. */
interface BuiltStep {
  line: ObjectSummary;
  record: BuildRecord;
  number: number;
  id: string;
}

function stepOf(o: ObjectSummary): BuiltStep | null {
  const record = recordOf(o);
  const number = o.attributes.build_step;
  const id = o.attributes.build_id;
  return record && typeof number === 'number' && typeof id === 'string' && o.kind === 'string' ? { line: o, record, number, id } : null;
}

/** A number that can be typed through half-finished values: it only reports what parses. */
function NumberBox({ value, onChange, title, width = 'w-16' }: { value: number; onChange: (v: number) => void; title: string; width?: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => {
    setText((t) => (t.trim() !== '' && Number(t) === value ? t : String(value)));
  }, [value]);
  return (
    <Input
      className={`h-7 ${width} px-1 text-xs tabular-nums`}
      value={text}
      title={title}
      aria-label={title}
      onBlur={() => setText(String(value))}
      onChange={(e) => {
        setText(e.target.value);
        const n = Number(e.target.value);
        if (e.target.value.trim() !== '' && Number.isFinite(n)) onChange(n);
      }}
    />
  );
}

function Choice<T extends string>({ value, options, labels, onChange, title }: { value: T; options: readonly T[]; labels?: Record<T, string>; onChange: (v: T) => void; title?: string }) {
  return (
    <span className="inline-flex rounded border" title={title}>
      {options.map((option) => (
        <button key={option} aria-pressed={option === value} className={`px-1.5 py-1 ${option === value ? 'bg-selection text-primary' : 'text-muted-foreground'}`} onClick={() => onChange(option)}>
          {labels?.[option] ?? option}
        </button>
      ))}
    </span>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex items-center gap-2">
      <span className="w-20 shrink-0 text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

/** The grids and surfaces in the project, each kind under its own heading, or a word that there are none yet. */
function TargetOptions({ surfaces }: { surfaces: ObjectSummary[] }) {
  const grids = surfaces.filter((s) => s.kind === 'grid');
  const meshes = surfaces.filter((s) => s.kind === 'surface');
  const options = (of: ObjectSummary[]) =>
    of.map((s) => (
      <option key={s.id} value={s.id}>
        {s.name}
      </option>
    ));
  if (surfaces.length === 0) {
    return (
      <option value="none" disabled>
        No grids or surfaces in this project yet
      </option>
    );
  }
  return (
    <>
      {grids.length > 0 && <optgroup label="Grids">{options(grids)}</optgroup>}
      {meshes.length > 0 && <optgroup label="Surfaces">{options(meshes)}</optgroup>}
    </>
  );
}

/** A grid, a surface or an RL. */
function TargetBox({ value, surfaces, onChange, title }: { value: StepTarget; surfaces: ObjectSummary[]; onChange: (t: StepTarget) => void; title: string }) {
  return (
    <span className="flex items-center gap-1">
      <select className={selectClass} value={value.kind === 'rl' ? 'rl' : value.id} title={title} aria-label={title} onChange={(e) => onChange(e.target.value === 'rl' ? { kind: 'rl', rl: value.kind === 'rl' ? value.rl : 0 } : { kind: 'surface', id: e.target.value })}>
        <option value="rl">RL</option>
        <TargetOptions surfaces={surfaces} />
      </select>
      {value.kind === 'rl' && <NumberBox value={value.rl} title={`${title}: RL`} onChange={(rl) => onChange({ kind: 'rl', rl })} />}
    </span>
  );
}

function RoadBadge({ name }: { name: string }) {
  return (
    <span className="inline-block rounded px-1 text-[10px] font-semibold text-white" style={{ background: rampColour(name) }}>
      {name}
    </span>
  );
}

/** What a road on the line does at a berm. */
type AtBerm = 'crosses' | 'turns' | 'ends';
const AT_BERM: readonly AtBerm[] = ['crosses', 'turns', 'ends'];
type JoinChoice = 'auto' | 'on' | 'off';
const JOINS: readonly JoinChoice[] = ['auto', 'on', 'off'];

export function BuilderPanel() {
  const { summary, selected, select, runRecord, clearRefusal, pickLine, frameLine } = useWorkspace();
  const objects = useMemo(() => summary?.objects ?? [], [summary]);
  const surfaces = useMemo(() => objects.filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid') && o.attributes.face_design === undefined && o.attributes.build === undefined), [objects]);
  const steps = useMemo(() => objects.map(stepOf).filter((s): s is BuiltStep => s !== null), [objects]);

  // The picked line: the selected string, or the line of the step whose wall or road is selected.
  const selectedObject = objects.find((o) => o.id === selected);
  const selectedStep = typeof selectedObject?.attributes.build_id === 'string' ? steps.find((s) => s.id === selectedObject.attributes.build_id) : undefined;
  const picked = selectedObject?.kind === 'string' && selectedObject.attributes.road === undefined ? selectedObject : selectedStep?.line;
  const pickedStep = picked ? steps.find((s) => s.line.id === picked.id) : undefined;
  const byHand = !!picked && !pickedStep;

  const [buildName, setBuildName] = useState<string | null>(null);
  const showing = (picked?.attributes.build as string | undefined) ?? buildName;
  const built = useMemo(() => steps.filter((s) => s.line.attributes.build === showing).sort((a, b) => a.number - b.number || a.line.name.localeCompare(b.line.name)), [steps, showing]);

  // The next step's settings, from the steps before it where there are any.
  const [op, setOp] = useState<BuildOp['op']>('project');
  const [to, setTo] = useState<StepTarget>({ kind: 'rl', rl: 0 });
  const [batter, setBatter] = useState(usualBatter);
  const [width, setWidth] = useState(usualBerm);
  const [bermOn, setBermOn] = useState<StepTarget | null>(null);
  const [atBerm, setAtBerm] = useState<Record<string, AtBerm>>({});
  const [join, setJoin] = useState<JoinChoice>('auto');
  const [climbs, setClimbs] = useState<Record<string, boolean>>({});
  const [newRoads, setNewRoads] = useState<Ramp[]>([]);
  // Where a build starts, from a line drawn by hand.
  const [liesOn, setLiesOn] = useState<StepTarget>({ kind: 'rl', rl: 0 });
  const [side, setSide] = useState<WallSide>('left');
  const [runOn, setRunOn] = useState(0);
  const [name, setName] = useState('');
  const [report, setReport] = useState<{ lines: string[]; error?: string }>({ lines: [] });
  const [busy, setBusy] = useState(false);

  // The chain of steps up to the picked line, most recent first: what the next step takes after.
  const chain = useMemo(() => {
    const out: BuiltStep[] = [];
    let at = pickedStep;
    while (at && out.length < 200) {
      out.push(at);
      const from = at.record.from;
      at = steps.find((s) => s.line.id === from);
    }
    return out;
  }, [pickedStep, steps]);
  const lastWall = chain.map((s) => s.record.step).find((s): s is Extract<BuildOp, { op: 'project' }> => s.op === 'project');
  const lastBerm = chain.map((s) => s.record.step).find((s): s is Extract<BuildOp, { op: 'berm' }> => s.op === 'berm');
  const lineLiesOn = liesOnOf(picked) ?? (byHand ? liesOn : null);

  // A newly picked line sets up the next step from it: a berm after a wall, a wall after a berm or from a toe, with the
  // target after the last wall's.
  const pickedId = picked?.id ?? null;
  useEffect(() => {
    if (!picked) return;
    const after = pickedStep?.record.step.op === 'project' ? 'berm' : 'project';
    setOp(after);
    setBatter(lastWall?.batter ?? usualBatter);
    setWidth(lastBerm?.width ?? usualBerm);
    setBermOn(null);
    setAtBerm({});
    setJoin('auto');
    setClimbs({});
    setNewRoads([]);
    const from = lastWall?.to ?? liesOnOf(picked) ?? levelOf(picked, summary?.origin) ?? (surfaces[0] ? { kind: 'surface' as const, id: surfaces[0].id } : { kind: 'rl' as const, rl: 0 });
    const nextSurface = from.kind === 'surface' ? surfaces[surfaces.findIndex((s) => s.id === from.id) + 1] : undefined;
    setTo(nextSurface ? { kind: 'surface', id: nextSurface.id } : { kind: 'rl', rl: from.kind === 'rl' ? from.rl + usualBenchHeight : 0 });
    if (!pickedStep) {
      setLiesOn(levelOf(picked, summary?.origin) ?? (surfaces[0] ? { kind: 'surface', id: surfaces[0].id } : { kind: 'rl', rl: 0 }));
      setName(picked.name);
      setRunOn(0);
    }
    // Only when another line is picked: settings typed for this one stay as they are.
  }, [pickedId]);

  // The roads on the picked line: on a berm's line a step made, those that crossed it from its crest, which climb on.
  const crest = pickedStep?.record.step.op === 'berm' ? objects.find((o) => o.id === pickedStep.record.from) : undefined;
  const bermStep = pickedStep?.record.step.op === 'berm' ? pickedStep.record.step : undefined;
  const roads = bermStep ? roadsOn(crest).filter((r) => !(bermStep.stop ?? []).includes(r.name)) : roadsOn(picked);
  const overBerm = !!bermStep;
  const turning = roads.filter((r) => atBerm[r.name] === 'turns').map((r) => r.name);

  if (!summary) return <div className="p-3 text-xs text-muted-foreground">No project open.</div>;

  const frame = async (id: string) => {
    const details = await objectDetails(id);
    if (details.measures.kind === 'string') frameLine(details.measures.points);
  };
  const pick = (step: BuiltStep) => {
    select(step.line.id);
    void frame(step.line.id);
  };
  /** A step and every step built on it, one undo. */
  const remove = async (step: BuiltStep) => {
    const gone = new Set([step.line.id]);
    for (let grew = true; grew; ) {
      grew = false;
      for (const s of steps) {
        if (!gone.has(s.line.id) && gone.has(s.record.from)) {
          gone.add(s.line.id);
          grew = true;
        }
      }
    }
    const ids = new Set(steps.filter((s) => gone.has(s.line.id)).map((s) => s.id));
    const pieces = objects.filter((o) => typeof o.attributes.build_id === 'string' && ids.has(o.attributes.build_id)).map((o) => o.id);
    const outcome = await runRecord('object.delete_many', { objects: pieces });
    if (!outcome.record) return setReport({ lines: [], error: outcome.error });
    setReport({ lines: [`Took out step ${step.number}${gone.size > 1 ? ` and the ${gone.size - 1} built on it` : ''}`] });
    const from = steps.find((s) => s.line.id === step.record.from);
    select(from ? from.line.id : objects.some((o) => o.id === step.record.from) ? step.record.from : null);
  };

  const placeRoad = async () => {
    const origin = summary.origin;
    if (!picked?.vertices_local || !origin) return;
    const clicked = await pickLine('Click on the line where the road starts. Esc cancels', 'Click along the line the way the road climbs. Esc cancels');
    if (!clicked) return;
    const [at, towards] = clicked;
    const way = wayAt(
      picked.vertices_local.map((v) => [v[0] + origin[0], v[1] + origin[1]]),
      at,
    );
    if (!way) return;
    const heading: RampHeading = (towards[0] - at[0]) * way[0] + (towards[1] - at[1]) * way[1] >= 0 ? 'along' : 'back';
    const wallSide = pickedStep?.record.side ?? side;
    const out: [number, number] = wallSide === 'left' ? [-way[1], way[0]] : [way[1], -way[0]];
    const taken = [...roads.map((r) => r.name), ...newRoads.map((r) => r.name)];
    const letter = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').find((l) => !taken.includes(l)) ?? `R${taken.length + 1}`;
    const points = [-RAMP_BEHIND, RAMP_OUT].map((d): Vec3 => [at[0] + out[0] * d, at[1] + out[1] * d, at[2]]);
    const made = await runRecord('string.create', { name: `Ramp ${letter}`, layer: 'ramps', points });
    const string = made.record?.outputs[0]?.id;
    if (!string) return setReport({ lines: [], error: made.error });
    setNewRoads((was) => [...was, { name: letter, string, fixed: 'start', heading, width: usualRoadWidth, grade: usualGrade, berm: 'after' }]);
    // Placing the road's string picked it: the line is still the one to build from.
    select(picked.id);
  };

  const build = async () => {
    if (!picked) return;
    const stop = roads.filter((r) => (op === 'berm' ? atBerm[r.name] === 'ends' : climbs[r.name] === false)).map((r) => r.name);
    const step: BuildOp =
      op === 'project'
        ? { op: 'project', to, batter, ...(newRoads.length ? { ramps: newRoads } : {}), ...(stop.length && !overBerm ? { stop } : {}) }
        : { op: 'berm', width, ...(bermOn ? { on: bermOn } : {}), ...(turning.length ? { turns: turning } : {}), ...(join !== 'auto' ? { join: join === 'on' } : {}), ...(stop.length ? { stop } : {}) };
    const params: Record<string, unknown> = { from: picked.id, step };
    if (byHand) Object.assign(params, { lies_on: liesOn, side, name: name.trim() || picked.name, ...(runOn > 0 ? { run_on: [runOn, runOn] } : {}) });
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord('face.build_step', params);
    setBusy(false);
    if (!outcome.record) return setReport({ lines: [], error: outcome.error });
    setReport({ lines: outcome.record.messages });
    if (byHand) setBuildName(name.trim() || picked.name);
    const line = outcome.record.outputs.find((o) => o.change === 'created');
    if (line) select(line.id);
  };

  const pickedWords = (() => {
    if (!picked) return null;
    if (pickedStep) return `step ${pickedStep.number} of ${String(picked.attributes.build)}: ${stepWords(pickedStep.record.step, objects)}`;
    if (picked.attributes.face_design !== undefined) return 'made by a face design';
    return 'drawn by hand';
  })();
  const stale = (s: BuiltStep) => {
    const from = objects.find((o) => o.id === s.record.from);
    return !!from && s.record.from_revision !== undefined && from.revision > s.record.from_revision;
  };

  return (
    <div className="flex h-full flex-col overflow-auto text-xs" data-builder-panel>
      <div className="border-b px-3 py-2">
        <div className="text-sm font-medium">Builder</div>
        <div className="text-muted-foreground">Pick a line, build one wall or berm from it. Edit any line between steps; nothing is built again unless you ask.</div>
      </div>

      {built.length > 0 && (
        <div className="px-3 pt-2">
          <div className="pb-1 font-medium uppercase tracking-wide text-muted-foreground">Steps of {showing}</div>
          <div className="rounded border">
            {built.map((s) => {
              const on = s.id === pickedStep?.id || s.id === selectedObject?.attributes.build_id;
              const edited = s.line.revision > 1;
              const old = stale(s);
              const roadsHere = s.record.step.op === 'project' ? roadsOn(s.line).map((r) => r.name) : [];
              return (
                <div key={s.line.id} className={`flex items-center gap-2 border-b px-2 py-1 last:border-b-0 ${on ? 'bg-selection outline outline-1 outline-primary' : ''} ${old ? 'opacity-60' : ''}`}>
                  <button className="flex min-w-0 flex-1 items-center gap-2 text-left" title={`Pick ${s.line.name} and go to it`} aria-label={`Step ${s.number}: ${s.line.name}`} onClick={() => pick(s)}>
                    <span className="w-5 shrink-0 text-right tabular-nums text-muted-foreground">{s.number}</span>
                    <span className="min-w-0 flex-1 truncate">
                      {stepWords(s.record.step, objects)} <span className="text-muted-foreground">→ {s.line.name}</span>
                    </span>
                    {roadsHere.map((r) => (
                      <RoadBadge key={r} name={r} />
                    ))}
                    {edited && (
                      <span className="rounded bg-blue-900/60 px-1 text-[10px] text-blue-200" title="This line has been edited since the step made it">
                        edited
                      </span>
                    )}
                    {old && (
                      <span className="rounded bg-amber-900/60 px-1 text-[10px] text-amber-200" title="Built from the line below as it was before it was edited">
                        built before an edit
                      </span>
                    )}
                  </button>
                  <button className="rounded p-1 text-muted-foreground hover:text-foreground" title={`Take out step ${s.number} and every step built on it, in one undo`} aria-label={`Take out step ${s.number}`} onClick={() => void remove(s)}>
                    <Trash2 className="size-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {!picked ? (
        <div className="px-3 py-3 text-muted-foreground">Click a line in the viewport to build from it: a toe drawn by hand to start a build, or a line a step made to carry on.</div>
      ) : (
        <div className="m-3 flex flex-col gap-2 rounded border p-2">
          <div>
            <span className="font-medium text-primary">Next step from {picked.name}</span>
            <div className="text-muted-foreground">
              {pickedWords}
              {lineLiesOn && !byHand ? ` · lies on ${targetWords(lineLiesOn, objects)}` : ''}
            </div>
          </div>

          {byHand && (
            <div className="flex flex-col gap-1.5 rounded bg-muted/40 p-2">
              <div className="text-muted-foreground">Starting a build from this line:</div>
              <Field label="Build">
                <Input className="h-7 w-40 px-1 text-xs" value={name} aria-label="Build name" onChange={(e) => setName(e.target.value)} />
              </Field>
              <Field label="Lies on">
                <TargetBox value={liesOn} surfaces={surfaces} title="What the line lies on" onChange={setLiesOn} />
              </Field>
              <Field label="Walls go up">
                <Choice value={side} options={['left', 'right'] as const} labels={{ left: 'on its left', right: 'on its right' }} title="Looking along the line" onChange={setSide} />
              </Field>
              <Field label="Run on">
                <NumberBox value={runOn} title="How far to run the line on past each end first (m), so the face is built oversized" onChange={setRunOn} /> m each end
              </Field>
            </div>
          )}

          <div className="flex items-center gap-1">
            <Choice value={op} options={['project', 'berm'] as const} labels={{ project: 'Wall', berm: 'Berm' }} title="What to build from the line" onChange={setOp} />
          </div>

          {op === 'project' ? (
            <div className="flex flex-col gap-1.5">
              <Field label="To">
                <TargetBox value={to} surfaces={surfaces} title="What the wall goes up to" onChange={setTo} />
              </Field>
              {surfaces.length === 0 && <div className="pl-[5.5rem] text-muted-foreground">To go up to a grid or a surface, import it first: they're listed here once they're in the project.</div>}
              <Field label="Batter">
                <NumberBox value={batter} title="Batter (degrees from horizontal)" onChange={setBatter} />°
              </Field>
              {roads.length > 0 && (
                <Field label="Roads">
                  <span className="flex flex-wrap items-center gap-2">
                    {roads.map((r) =>
                      overBerm ? (
                        <span key={r.name} className="flex items-center gap-1" title={`Road ${r.name} crossed the berm, so it climbs this wall`}>
                          <RoadBadge name={r.name} /> climbs on
                        </span>
                      ) : (
                        <label key={r.name} className="flex items-center gap-1" title={`Road ${r.name} came out on this line: untick to end it here`}>
                          <input type="checkbox" checked={climbs[r.name] !== false} aria-label={`Road ${r.name} climbs this wall`} onChange={(e) => setClimbs((was) => ({ ...was, [r.name]: e.target.checked }))} />
                          <RoadBadge name={r.name} /> climbs
                        </label>
                      ),
                    )}
                  </span>
                </Field>
              )}
              {newRoads.map((r, k) => (
                <div key={r.string ?? r.name} className="flex flex-wrap items-center gap-2 pl-[5.5rem]">
                  <RoadBadge name={r.name} />
                  <span className="text-muted-foreground">new road, {r.heading === 'along' ? 'along the line' : 'back along it'}</span>
                  <NumberBox width="w-12" value={r.width} title={`Road ${r.name}: width (m)`} onChange={(w) => setNewRoads((was) => was.map((x, j) => (j === k ? { ...x, width: w } : x)))} />m
                  <NumberBox width="w-12" value={r.grade} title={`Road ${r.name}: grade (%)`} onChange={(g) => setNewRoads((was) => was.map((x, j) => (j === k ? { ...x, grade: g } : x)))} />%
                  <Choice value={r.berm ?? 'after'} options={['after', 'through'] as const} labels={{ after: 'after berms', through: 'through berms' }} title="How it gets past a berm" onChange={(b) => setNewRoads((was) => was.map((x, j) => (j === k ? { ...x, berm: b } : x)))} />
                  <button className="rounded p-1 text-muted-foreground hover:text-foreground" title={`Leave road ${r.name} out`} aria-label={`Leave road ${r.name} out`} onClick={() => setNewRoads((was) => was.filter((_, j) => j !== k))}>
                    <Trash2 className="size-3.5" />
                  </button>
                </div>
              ))}
              <div className="pl-[5.5rem]">
                <Button variant="outline" size="sm" className="h-7 gap-1 px-2 text-xs" title="Place a new road on this wall: click where it starts on the line, then the way it climbs" onClick={() => void placeRoad()}>
                  <Plus className="size-3.5" /> Road
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col gap-1.5">
              <Field label="Width">
                <NumberBox value={width} title="Berm width (m)" onChange={setWidth} /> m
              </Field>
              <Field label="On">
                <select
                  className={selectClass}
                  value={bermOn ? (bermOn.kind === 'rl' ? 'rl' : bermOn.id) : ''}
                  aria-label="What the berm lies on"
                  onChange={(e) => setBermOn(e.target.value === '' ? null : e.target.value === 'rl' ? { kind: 'rl', rl: 0 } : { kind: 'surface', id: e.target.value })}
                >
                  <option value="">{lineLiesOn ? `as the line: ${targetWords(lineLiesOn, objects)}` : 'as the line'}</option>
                  <option value="rl">RL</option>
                  <TargetOptions surfaces={surfaces} />
                </select>
                {bermOn?.kind === 'rl' && <NumberBox value={bermOn.rl} title="The berm's RL" onChange={(rl) => setBermOn({ kind: 'rl', rl })} />}
              </Field>
              {roads.map((r) => (
                <Field key={r.name} label={`Road ${r.name}`}>
                  <RoadBadge name={r.name} />
                  <Choice
                    value={atBerm[r.name] ?? 'crosses'}
                    options={AT_BERM}
                    labels={{ crosses: r.berm === 'through' ? 'climbs through' : 'crosses', turns: 'turns back', ends: 'ends here' }}
                    title={`What road ${r.name} does at this berm: carries on up the next wall, turns back on a box (a switchback), or ends on the line`}
                    onChange={(v) => setAtBerm((was) => ({ ...was, [r.name]: v }))}
                  />
                </Field>
              ))}
              {turning.length >= 2 && (
                <Field label="Join">
                  <Choice value={join} options={JOINS} labels={{ auto: 'when close', on: 'one pad', off: 'apart' }} title="The switchbacks' boxes as one pad: always, never, or when they come within a road's width of each other" onChange={setJoin} />
                </Field>
              )}
            </div>
          )}

          <div className="flex items-center justify-end gap-2 pt-1">
            <Button size="sm" className="h-7 gap-1 px-3 text-xs" disabled={busy} title={op === 'project' ? 'Build the wall from this line' : 'Build the berm from this line'} onClick={() => void build()}>
              <Hammer className="size-3.5" /> {busy ? 'Building…' : op === 'project' ? 'Build wall' : 'Build berm'}
            </Button>
          </div>
        </div>
      )}

      {(report.error || report.lines.length > 0) && (
        <div className="mx-3 mb-3 flex flex-col gap-0.5 rounded border p-2">
          {report.error && <div className="text-destructive">{report.error}</div>}
          {report.lines.map((line, k) => (
            <div key={k} className={k === 0 ? '' : 'text-muted-foreground'}>
              {line}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
