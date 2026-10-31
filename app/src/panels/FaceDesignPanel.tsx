import { ArrowDown, ArrowUp, Play, Plus, Save, Trash2 } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useMemo, useState } from 'react';
import { type FaceDesign, type FaceLine, type FaceProfile, objectDetails, type ObjectSummary, type ProfileStep, type StepTarget, type WallRule, type WallSide } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { useWorkspace } from '@/workspace/context';

const selectClass = 'h-7 min-w-0 rounded border bg-background px-1 text-xs';

/** A number that can be typed through half-finished values: it only reports what parses. */
function NumberBox({ value, onChange, title, width = 'w-16' }: { value: number; onChange: (v: number) => void; title: string; width?: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => {
    setText((t) => (Number(t) === value ? t : String(value)));
  }, [value]);
  return (
    <Input
      className={`h-7 ${width} px-1 text-xs tabular-nums`}
      value={text}
      title={title}
      aria-label={title}
      onChange={(e) => {
        setText(e.target.value);
        const n = Number(e.target.value);
        if (e.target.value.trim() !== '' && Number.isFinite(n)) onChange(n);
      }}
    />
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

function SideChoice({ value, onChange }: { value: WallSide; onChange: (v: WallSide) => void }) {
  return (
    <span className="inline-flex rounded border">
      {(['left', 'right'] as const).map((side) => (
        <button key={side} className={`px-1.5 py-1 capitalize ${side === value ? 'bg-selection text-primary' : 'text-muted-foreground'}`} onClick={() => onChange(side)}>
          {side}
        </button>
      ))}
    </span>
  );
}

const blankDesign = (register: string): FaceDesign => ({
  layer: 'faces',
  tolerance: 0.1,
  lines: [],
  profiles: [{ name: 'Highwall', register, up: [], down: [] }],
  rules: [
    { name: 'Batter', value: { type: 'fixed', value: 70 } },
    { name: 'Berm', value: { type: 'fixed', value: 10 } },
  ],
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

/** The rule a step of this kind should start on, going by the rule's name, so a berm never starts at a batter. */
function ruleFor(kind: ProfileStep['step'], rules: WallRule[]) {
  const word = kind === 'berm' ? 'berm' : 'batter';
  const other = kind === 'berm' ? 'batter' : 'berm';
  return (rules.find((r) => r.name.toLowerCase().includes(word)) ?? rules.find((r) => !r.name.toLowerCase().includes(other)) ?? rules[0])?.name ?? '';
}

/** Where a new step should start: the surface after the last one this list went to, or after the toe's own. */
function nextTarget(steps: ProfileStep[], surfaces: ObjectSummary[], register: string): StepTarget {
  const last = [...steps].reverse().find((s) => s.step === 'project' && s.to.kind === 'surface');
  const from = last?.step === 'project' && last.to.kind === 'surface' ? last.to.id : register;
  const next = surfaces[surfaces.findIndex((s) => s.id === from) + 1] ?? surfaces.find((s) => s.id !== register);
  return next ? { kind: 'surface', id: next.id } : { kind: 'rl', rl: 0 };
}

/** One step of a profile: a wall to a surface or RL, or a berm on whatever the last wall reached. */
function StepRow({ step, surfaces, rules, startingTarget, onChange, onMove, onRemove }: { step: ProfileStep; surfaces: ObjectSummary[]; rules: WallRule[]; startingTarget: StepTarget; onChange: (s: ProfileStep) => void; onMove: (by: number) => void; onRemove: () => void }) {
  const ruleName = step.rule;
  const toValue = step.step === 'project' ? (step.to.kind === 'rl' ? 'rl' : step.to.id) : '';
  const setTo = (value: string) => {
    if (step.step !== 'project') return;
    const to: StepTarget = value === 'rl' ? { kind: 'rl', rl: 0 } : { kind: 'surface', id: value };
    onChange({ ...step, to });
  };
  return (
    <tr className="border-t">
      <td className="px-1 py-0.5">
        <select
          className={selectClass}
          value={step.step}
          aria-label="Step"
          onChange={(e) => onChange(e.target.value === 'berm' ? { step: 'berm', rule: ruleFor('berm', rules) } : { step: 'project', to: startingTarget, rule: ruleFor('project', rules) })}
        >
          <option value="project">Project</option>
          <option value="berm">Berm</option>
        </select>
      </td>
      <td className="px-1 py-0.5">
        {step.step === 'berm' ? (
          <span className="text-muted-foreground">on the last target</span>
        ) : (
          <span className="flex items-center gap-1">
            <select className={`${selectClass} max-w-36`} value={toValue} aria-label="Project to" onChange={(e) => setTo(e.target.value)}>
              {surfaces.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
              <option value="rl">An RL</option>
            </select>
            {step.to.kind === 'rl' && <NumberBox value={step.to.rl} title="RL" onChange={(rl) => onChange({ ...step, to: { kind: 'rl', rl } })} />}
          </span>
        )}
      </td>
      <td className="px-1 py-0.5">
        <select className={`${selectClass} max-w-28`} value={ruleName} aria-label="Rule" onChange={(e) => onChange({ ...step, rule: e.target.value })}>
          {rules.map((r) => (
            <option key={r.name} value={r.name}>
              {r.name}
            </option>
          ))}
        </select>
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

/** A profile's steps one way from the toe. */
function StepTable({ title, steps, surfaces, rules, register, onChange }: { title: string; steps: ProfileStep[]; surfaces: ObjectSummary[]; rules: WallRule[]; register: string; onChange: (s: ProfileStep[]) => void }) {
  const startingTarget = nextTarget(steps, surfaces, register);
  const add = () => onChange([...steps, { step: 'project', to: startingTarget, rule: ruleFor('project', rules) }]);
  return (
    <div className="px-3 pb-1">
      <div className="flex items-center gap-2 py-1 text-xs">
        <span className="font-medium">{title}</span>
        <SmallButton onClick={add}>
          <Plus className="size-3" /> Add step
        </SmallButton>
      </div>
      {steps.length > 0 && (
        <table className="w-full text-xs">
          <thead className="text-left text-muted-foreground">
            <tr>
              <th className="px-1 font-normal">Step</th>
              <th className="px-1 font-normal">To</th>
              <th className="px-1 font-normal">Rule</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {steps.map((step, k) => (
              <StepRow
                key={k}
                step={step}
                surfaces={surfaces}
                rules={rules}
                startingTarget={startingTarget}
                onChange={(s) => onChange(replaceAt(steps, k, s))}
                onMove={(by) => onChange(moved(steps, k, by))}
                onRemove={() => onChange(steps.filter((_, j) => j !== k))}
              />
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

/**
 * A face design: the lines to run, the profile each one takes, and the named rules those profiles use.
 * It stands apart from the selection. Run saves it and steps every line in one run, one undo.
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
        <p className="text-muted-foreground">A face design holds the lines to run, a profile of steps for each, and the rules those steps use.</p>
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
  const stepRuleNames = draft.profiles.flatMap((p) => [...p.up, ...p.down].map((s) => s.rule));
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
    const made_: FaceProfile = copy && profile ? { ...profile, name: fresh } : { name: fresh, register: profile?.register ?? surfaces[0]?.id ?? '', up: [], down: [] };
    edit((d) => ({ ...d, profiles: [...d.profiles, made_] }));
    setProfileName(fresh);
  };
  const removeProfile = () => {
    if (!profile) return;
    edit((d) => ({ ...d, profiles: d.profiles.filter((p) => p !== profile) }));
    setProfileName(null);
  };
  const renameRule = (from: string, to: string) => {
    const follow = (s: ProfileStep): ProfileStep => (s.rule === from ? { ...s, rule: to } : s);
    edit((d) => ({
      ...d,
      rules: d.rules.map((r) => (r.name === from ? { ...r, name: to } : r)),
      profiles: d.profiles.map((p) => ({ ...p, up: p.up.map(follow), down: p.down.map(follow) })),
    }));
  };
  const addLine = (string: string) => {
    if (draft.lines.some((l) => l.string === string)) return;
    const line: FaceLine = { string, profile: profile?.name ?? '', side: 'right', run_on: [100, 100] };
    edit((d) => ({ ...d, lines: [...d.lines, line] }));
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
                  <tr key={line.string} className="border-t">
                    <td className="max-w-28 truncate px-1 py-0.5" title={nameOf(line.string)}>
                      {nameOf(line.string)}
                    </td>
                    <td className="px-1 py-0.5">
                      <select className={`${selectClass} max-w-24`} value={line.profile} aria-label="Profile" onChange={(e) => change({ profile: e.target.value })}>
                        {profileNames.map((p) => (
                          <option key={p}>{p}</option>
                        ))}
                      </select>
                    </td>
                    <td className="px-1 py-0.5">
                      <SideChoice value={line.side} onChange={(side) => change({ side })} />
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
            <span className="ml-auto text-muted-foreground">Register toe to</span>
            <select className={`${selectClass} max-w-32`} value={profile.register} aria-label="Register toe to" onChange={(e) => setProfile({ register: e.target.value })}>
              {surfaces.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </div>
          <StepTable title="Up from the toe" steps={profile.up} surfaces={surfaces} rules={draft.rules} register={profile.register} onChange={(up) => setProfile({ up })} />
          <StepTable title="Down from the toe" steps={profile.down} surfaces={surfaces} rules={draft.rules} register={profile.register} onChange={(down) => setProfile({ down })} />
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
                  <IconButton title="Remove this rule" disabled={stepRuleNames.includes(rule.name)} onClick={() => edit((d) => ({ ...d, rules: d.rules.filter((_, j) => j !== k) }))}>
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
        <span className="text-muted-foreground">Tolerance</span>
        <NumberBox width="w-12" value={draft.tolerance} title="How far a next toe may leave its crest (m): 0.1 is usual, 2 at most" onChange={(tolerance) => edit((d) => ({ ...d, tolerance }))} />
        <span className="text-muted-foreground">m, on layer</span>
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
