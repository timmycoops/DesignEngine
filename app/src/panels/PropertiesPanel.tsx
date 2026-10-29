import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Check, Minus, Trash2, X } from 'lucide-react';
import { type Crossings, type MeshValidity, type ObjectDetails, objectDetails, type SeamModel, type StackIssue, type Vec3 } from '@/bridge';
import { columnHelper, DataTable } from '@/design-system/DataTable';
import { Badge } from '@/design-system/ui/badge';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { ScrollArea } from '@/design-system/ui/scroll-area';
import { useWorkspace } from '@/workspace/context';
import { clock, coord, fmt, kindLabel } from './format';

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b py-2">
      <h3 className="px-3 pb-1 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">{title}</h3>
      {children}
    </section>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7.5rem_1fr] gap-2 px-3 py-0.5 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate tabular-nums">{children}</span>
    </div>
  );
}

/** One validity flag: green when it holds, red when it does not, grey when not computed. */
function Flag({ label, value, detail }: { label: string; value: boolean | null; detail?: string }) {
  const Icon = value === null ? Minus : value ? Check : X;
  const tone = value === null ? 'text-muted-foreground' : value ? 'text-ok' : 'text-destructive';
  return (
    <Badge variant="outline" className="gap-1 font-normal" title={detail}>
      <Icon className={`size-3 ${tone}`} />
      {label}
    </Badge>
  );
}

/** What the crossing check says, in a line. */
function crossingText(c: Crossings | null): string {
  if (!c) return 'Not checked: made before crossing checks, or by a cut (a cut checks its inputs itself)';
  if (c.check.by === 'lattice') return "A grid's triangulation cannot cross itself";
  const tolerance = `${fmt(c.check.tolerance * 1000, 1)} mm`;
  return c.pairs === 0 ? `No crossings by more than ${tolerance}` : `${fmt(c.pairs)} pairs of triangles cross by more than ${tolerance}`;
}

function Validity({ v, crossings }: { v: MeshValidity; crossings: Crossings | null }) {
  return (
    <div className="flex flex-wrap gap-1 px-3 pt-1">
      <Flag label="Closed" value={v.closed} detail={`${fmt(v.boundary_edges)} boundary edges`} />
      <Flag label="Manifold" value={v.manifold} detail={`${fmt(v.non_manifold_edges)} non-manifold edges`} />
      <Flag label="Consistent winding" value={v.consistent_winding} detail={`${fmt(v.inconsistent_edges)} inconsistently wound edges`} />
      <Flag label="Non-degenerate" value={v.non_degenerate} detail={`${fmt(v.degenerate_triangles)} degenerate triangles`} />
      <Flag label="Non-self-intersecting" value={v.non_self_intersecting} detail={crossingText(crossings)} />
    </div>
  );
}

/** Where a mesh crosses itself, marked in the viewport while it is selected; click one to fly there. */
function CrossingSites({ crossings }: { crossings: Crossings }) {
  const { markCrossings, flyTo } = useWorkspace();
  useEffect(() => {
    markCrossings(crossings.at);
    return () => markCrossings([]);
  }, [crossings, markCrossings]);
  return (
    <Section title="Crossings">
      <p className="px-3 pb-1 text-xs text-destructive">
        {crossingText(crossings)}. Cuts refuse this mesh until it is fixed at source.
        {crossings.pairs > crossings.at.length && ` The first ${fmt(crossings.at.length)} are listed.`}
      </p>
      <ul className="px-3 text-xs tabular-nums">
        {crossings.at.map((p, n) => (
          <li key={n} className="cursor-pointer rounded px-1 hover:bg-selection" title="Show in the viewport" onClick={() => flyTo(p)}>
            {fmt(p[0], 2)} E {fmt(p[1], 2)} N RL {fmt(p[2], 2)}
          </li>
        ))}
      </ul>
    </Section>
  );
}

interface VertexRow {
  index: number;
  x: number;
  y: number;
  z: number;
  /** Grade to the next vertex (%), along plan length; null at the end of an open string. */
  grade: number | null;
}

const vertexHelper = columnHelper<VertexRow>();
const vertexColumns = vertexHelper.columns([
  vertexHelper.accessor('index', { header: '#', cell: (c) => <span className="text-muted-foreground tabular-nums">{c.getValue()}</span> }),
  vertexHelper.accessor('x', { header: 'Easting', cell: (c) => <span className="tabular-nums">{coord(c.getValue())}</span> }),
  vertexHelper.accessor('y', { header: 'Northing', cell: (c) => <span className="tabular-nums">{coord(c.getValue())}</span> }),
  vertexHelper.accessor('z', { header: 'RL', cell: (c) => <span className="tabular-nums">{coord(c.getValue())}</span> }),
  vertexHelper.accessor('grade', {
    header: 'Grade to next',
    cell: (c) => {
      const g = c.getValue();
      return <span className="text-muted-foreground tabular-nums">{g === null ? '' : `${fmt(g, 2)}%`}</span>;
    },
  }),
]);

function Vertices({ points, closed }: { points: Vec3[]; closed: boolean }) {
  const rows = useMemo(
    () =>
      points.map(([x, y, z], i) => {
        const next = i + 1 < points.length ? points[i + 1] : closed ? points[0] : null;
        const run = next ? Math.hypot(next[0] - x, next[1] - y) : 0;
        return { index: i + 1, x, y, z, grade: next && run > 0 ? (100 * (next[2] - z)) / run : null };
      }),
    [points, closed],
  );
  return <DataTable data={rows} columns={vertexColumns} rowId={(r) => String(r.index)} />;
}

/** One row of the string tools: a label, its inputs, and the button that runs the command. */
function ToolRow({ label, children, onApply, disabled }: { label: string; children?: ReactNode; onApply: () => void; disabled?: boolean }) {
  return (
    <div className="flex items-center gap-1 px-3 py-0.5 text-xs">
      <span className="w-14 shrink-0 text-muted-foreground">{label}</span>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1">{children}</div>
      <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={disabled} onClick={onApply}>
        Apply
      </Button>
    </div>
  );
}

function SmallInput({ value, onChange, placeholder, title }: { value: string; onChange: (v: string) => void; placeholder: string; title: string }) {
  return (
    <Input
      className="h-6 w-16 px-1 text-xs tabular-nums"
      value={value}
      placeholder={placeholder}
      title={title}
      aria-label={title}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/** Two-way choice as a pair of small buttons. */
function Choice<T extends string>({ value, options, onChange }: { value: T; options: [T, string][]; onChange: (v: T) => void }) {
  return (
    <span className="inline-flex rounded border">
      {options.map(([v, label]) => (
        <button key={v} className={`px-1.5 py-0.5 ${v === value ? 'bg-selection text-primary' : 'text-muted-foreground'}`} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </span>
  );
}

/**
 * Commands on the selected string. Vertex numbers are as listed below (from 1). Grade, RL, drape,
 * reverse and close write a new revision; offset makes a new string. All undo.
 */
function StringTools({ id, closed }: { id: string; closed: boolean }) {
  const { run, summary } = useWorkspace();
  const [from, setFrom] = useState('1');
  const [to, setTo] = useState('');
  const [grade, setGrade] = useState('');
  const [startRl, setStartRl] = useState('');
  const [distance, setDistance] = useState('');
  const [side, setSide] = useState<'left' | 'right'>('right');
  const [corners, setCorners] = useState<'mitre' | 'round'>('mitre');
  const [level, setLevel] = useState('');
  const [surface, setSurface] = useState<string | null>(null);
  const [crossings, setCrossings] = useState(true);
  const [splitAt, setSplitAt] = useState('');
  const [joinWith, setJoinWith] = useState<string | null>(null);
  const [spacing, setSpacing] = useState('5');
  const [tolerance, setTolerance] = useState('');
  const others = (summary?.objects ?? []).filter((o) => o.kind === 'string' && o.id !== id && !o.closed);
  const partner = others.find((o) => o.id === joinWith) ?? others[0];

  const num = (s: string) => (s.trim() === '' ? null : Number(s));
  const ok = (v: number | null) => v !== null && Number.isFinite(v);
  const optional = (v: number | null) => v === null || Number.isFinite(v);
  const surfaces = (summary?.objects ?? []).filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid'));
  const target = surfaces.find((s) => s.id === surface) ?? surfaces[0];

  const gradeParams = () => {
    const params: Record<string, unknown> = { string: id, from: Math.round(num(from) ?? 1) - 1, grade: num(grade) };
    if (num(to) !== null) params.to = Math.round(num(to)!) - 1;
    if (num(startRl) !== null) params.rl = num(startRl);
    return params;
  };

  return (
    <>
      <ToolRow label="Grade" disabled={!ok(num(grade)) || !ok(num(from)) || !optional(num(to)) || !optional(num(startRl))} onApply={() => void run('string.set_grade', gradeParams())}>
        <SmallInput value={grade} onChange={setGrade} placeholder="%" title="Grade in %, positive uphill along the string" />
        <span className="text-muted-foreground">from</span>
        <SmallInput value={from} onChange={setFrom} placeholder="#" title="Start vertex" />
        <span className="text-muted-foreground">to</span>
        <SmallInput value={to} onChange={setTo} placeholder="end" title="End vertex (the last by default)" />
        <span className="text-muted-foreground">at RL</span>
        <SmallInput value={startRl} onChange={setStartRl} placeholder="own" title="RL at the start vertex (its own by default)" />
      </ToolRow>
      <ToolRow
        label="Offset"
        disabled={!(ok(num(distance)) && num(distance)! > 0)}
        onApply={() => void run('string.offset', { string: id, distance: num(distance), side, corners })}
      >
        <SmallInput value={distance} onChange={setDistance} placeholder="m" title="Horizontal distance (m)" />
        <Choice value={side} onChange={setSide} options={[['left', 'Left'], ['right', 'Right']]} />
        <Choice value={corners} onChange={setCorners} options={[['mitre', 'Mitre'], ['round', 'Round']]} />
      </ToolRow>
      <ToolRow label="Set RL" disabled={!ok(num(level))} onApply={() => void run('string.set_elevation', { string: id, rl: num(level) })}>
        <SmallInput value={level} onChange={setLevel} placeholder="RL" title="Every vertex to this RL" />
      </ToolRow>
      <ToolRow label="Drape" disabled={!target} onApply={() => target && void run('string.drape', { string: id, surface: target.id, crossings })}>
        {surfaces.length === 0 ? (
          <span className="text-muted-foreground">no surface loaded</span>
        ) : (
          <select
            className="h-6 max-w-40 rounded border bg-background px-1 text-xs"
            value={target?.id}
            onChange={(e) => setSurface(e.target.value)}
            aria-label="Surface to drape onto"
          >
            {surfaces.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
        <label className="flex items-center gap-1 text-muted-foreground" title="Add vertices where the string crosses the surface's triangle edges">
          <input type="checkbox" checked={crossings} onChange={(e) => setCrossings(e.target.checked)} /> follow
        </label>
      </ToolRow>
      <ToolRow label="Split" disabled={!ok(num(splitAt))} onApply={() => void run('string.split', { string: id, at: Math.round(num(splitAt)!) - 1 })}>
        <span className="text-muted-foreground">at vertex</span>
        <SmallInput value={splitAt} onChange={setSplitAt} placeholder="#" title="Split here; a closed string opens here" />
      </ToolRow>
      <ToolRow label="Join" disabled={closed || !partner} onApply={() => partner && void run('string.join', { string: id, other: partner.id })}>
        {closed ? (
          <span className="text-muted-foreground">open the string first</span>
        ) : others.length === 0 ? (
          <span className="text-muted-foreground">no other open string</span>
        ) : (
          <select
            className="h-6 max-w-40 rounded border bg-background px-1 text-xs"
            value={partner?.id}
            onChange={(e) => setJoinWith(e.target.value)}
            aria-label="String to join on"
            title="Joined at the nearest ends; that string is deleted (undo brings it back)"
          >
            {others.map((o) => (
              <option key={o.id} value={o.id}>
                {o.name}
              </option>
            ))}
          </select>
        )}
      </ToolRow>
      <ToolRow label="Smooth" disabled={!(ok(num(spacing)) && num(spacing)! > 0)} onApply={() => void run('string.smooth', { string: id, spacing: num(spacing) })}>
        <span className="text-muted-foreground">every</span>
        <SmallInput value={spacing} onChange={setSpacing} placeholder="m" title="Longest step along the curve in plan (m)" />
        <span className="text-muted-foreground">m</span>
      </ToolRow>
      <ToolRow label="Filter" disabled={!(ok(num(tolerance)) && num(tolerance)! >= 0)} onApply={() => void run('string.filter', { string: id, tolerance: num(tolerance) })}>
        <span className="text-muted-foreground">within</span>
        <SmallInput value={tolerance} onChange={setTolerance} placeholder="m" title="Remove vertices within this 3D distance of the line (m)" />
        <span className="text-muted-foreground">m</span>
      </ToolRow>
      <div className="flex gap-1 px-3 py-1">
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => void run('string.reverse', { string: id })}>
          Reverse
        </Button>
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => void run('string.set_closed', { string: id, closed: !closed })}>
          {closed ? 'Open' : 'Close'}
        </Button>
      </div>
    </>
  );
}

interface HorizonRow {
  code: string;
  depth: number;
  parent: string;
  working: boolean;
  grids: string;
}

const horizonHelper = columnHelper<HorizonRow>();
const horizonColumns = horizonHelper.columns([
  horizonHelper.accessor('code', {
    header: 'Horizon',
    cell: (c) => <span style={{ paddingLeft: `${c.row.original.depth * 0.75}rem` }}>{c.getValue()}</span>,
  }),
  horizonHelper.accessor('parent', { header: 'Parent' }),
  horizonHelper.accessor('working', { header: 'Working', cell: (c) => (c.getValue() ? 'Yes' : '') }),
  horizonHelper.accessor('grids', { header: 'Other grids', cell: (c) => <span className="text-muted-foreground">{c.getValue()}</span> }),
]);

/** The model's horizons, top first; click one to show it in the viewport. */
function Horizons({ id, model }: { id: string; model: SeamModel }) {
  const { seamView, showSeamHorizon } = useWorkspace();
  const shown = seamView?.id === id ? seamView.result.horizon : null;
  const rows = useMemo(() => {
    const parents = new Map(model.horizons.map((h) => [h.code, h.parent]));
    const depth = (code: string | null): number => {
      let d = 0;
      for (let p = parents.get(code ?? '') ?? null; p !== null && d < model.horizons.length; p = parents.get(p) ?? null) d += 1;
      return d;
    };
    return model.horizons.map((h) => ({
      code: h.code,
      depth: depth(h.code),
      parent: h.parent ?? '',
      working: h.working,
      grids: Object.keys(h.grids).join(' '),
    }));
  }, [model]);
  return <DataTable data={rows} columns={horizonColumns} rowId={(r) => r.code} selected={shown} onRowClick={(r) => showSeamHorizon(id, r.code)} />;
}

const issueText: Record<StackIssue['kind'], (i: StackIssue) => string> = {
  floor_above_roof: (i) => `${i.horizon} floor above its roof`,
  crossing: (i) => `${i.horizon} roof above the floor of ${i.other ?? '?'}`,
  outside_parent: (i) => `${i.horizon} outside its parent ${i.other ?? '?'}`,
  unpaired: (i) => `${i.horizon} roof and floor not live at the same nodes`,
  no_density: (i) => `${i.horizon} has no density grid`,
};

/** The model's issues; click a blocking one to show its horizon and fly to its worst node. */
function Issues({ id, issues }: { id: string; issues: StackIssue[] }) {
  const { focusIssue } = useWorkspace();
  if (issues.length === 0) return <p className="px-3 py-1 text-xs text-muted-foreground">None: the stack is consistent.</p>;
  return (
    <ul className="space-y-1 px-3 py-1 text-xs">
      {issues.map((i, n) => (
        <li
          key={n}
          className={`flex gap-2 ${i.blocking && i.at ? 'cursor-pointer rounded hover:bg-selection' : ''}`}
          title={i.blocking && i.at ? 'Show in the viewport' : undefined}
          onClick={i.blocking && i.at ? () => focusIssue(id, i.horizon, n) : undefined}
        >
          <Badge variant={i.blocking ? 'destructive' : 'outline'} className="shrink-0 font-normal">
            {i.blocking ? 'Blocking' : 'Warning'}
          </Badge>
          <span className="min-w-0">
            {issueText[i.kind](i)}
            {i.nodes > 0 && (
              <span className="text-muted-foreground">
                {' '}
                · {fmt(i.nodes)} nodes{i.worst > 0 && `, worst ${fmt(i.worst, 3)} m`}
                {i.at && ` at ${fmt(i.at[0], 1)} E ${fmt(i.at[1], 1)} N`}
              </span>
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}

function Measurements({ d }: { d: ObjectDetails }) {
  const m = d.measures;
  switch (m.kind) {
    case 'string':
      return (
        <>
          <Section title="String">
            <Field label="Vertices">{fmt(m.points.length)}</Field>
            <Field label="Closed">{m.closed ? 'Yes' : 'No'}</Field>
            <Field label="Length (slope)">{fmt(m.length, 2)} m</Field>
            <Field label="Length (plan)">{fmt(m.plan_length, 2)} m</Field>
            {m.plan_area !== null && <Field label="Area (plan)">{fmt(m.plan_area, 1)} m²</Field>}
          </Section>
          <Section title="String tools">
            <StringTools id={d.id} closed={m.closed} />
          </Section>
          <Section title="Vertices">
            <Vertices points={m.points} closed={m.closed} />
          </Section>
        </>
      );
    case 'surface':
    case 'solid':
      return (
        <Section title={m.kind === 'solid' ? 'Solid' : 'Surface'}>
          <Field label="Triangles">{fmt(m.triangles)}</Field>
          <Field label="Vertices">{fmt(m.vertices)}</Field>
          <Field label="Area">{fmt(m.area, 1)} m²</Field>
          {m.kind === 'solid' && <Field label="Volume">{fmt(m.volume, 1)} m³</Field>}
          <Validity v={m.validity} crossings={m.crossings} />
          {m.crossings && m.crossings.pairs > 0 && <CrossingSites crossings={m.crossings} />}
        </Section>
      );
    case 'grid': {
      const nodes = m.cols * m.rows;
      return (
        <Section title="Grid">
          <Field label="Nodes">
            {fmt(m.cols)} × {fmt(m.rows)}
          </Field>
          <Field label="Cell size">
            {fmt(m.cell_size[0], 2)} × {fmt(m.cell_size[1], 2)} m
          </Field>
          <Field label="Origin (SW node)">
            {coord(m.origin[0])} E, {coord(m.origin[1])} N
          </Field>
          <Field label="Nulls">
            {fmt(m.nulls)} ({fmt((100 * m.nulls) / Math.max(nodes, 1), 1)}%)
          </Field>
          <Field label="Value range">{m.z_range ? `${fmt(m.z_range[0], 3)} to ${fmt(m.z_range[1], 3)}` : 'all null'}</Field>
        </Section>
      );
    }
    case 'seam_model': {
      const { model } = m;
      const blocking = model.issues.filter((i) => i.blocking).length;
      return (
        <>
          <Section title="Seam model">
            <Field label="Horizons">
              {fmt(model.horizons.length)} ({fmt(model.horizons.filter((h) => h.working).length)} working)
            </Field>
            <Field label="Nodes">
              {fmt(model.lattice.cols)} × {fmt(model.lattice.rows)} at {fmt(model.lattice.cell_size[0], 2)} m
            </Field>
            <Field label="Grid folder">
              <span title={model.folder}>{model.folder}</span>
            </Field>
            <Field label="Density grid">{model.density}</Field>
            <Field label="Tolerance">{fmt(model.tolerance, 3)} m</Field>
            <Field label="Status">
              {blocking === 0 ? 'Ready for solids and reserves' : `${blocking} blocking issue${blocking === 1 ? '' : 's'}: fix the grids and rebuild`}
            </Field>
          </Section>
          <Section title="Issues">
            <Issues id={d.id} issues={model.issues} />
          </Section>
          <Section title="Horizons (top first): click to show">
            <Horizons id={d.id} model={model} />
          </Section>
        </>
      );
    }
  }
}

/** The selected object: name (editable), layer, measurements, validity, attributes and history. */
export function PropertiesPanel() {
  const { summary, selected, run, select } = useWorkspace();
  const object = summary?.objects.find((o) => o.id === selected);
  const [details, setDetails] = useState<ObjectDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState('');

  // Fetch again whenever the selection or its revision changes.
  useEffect(() => {
    setError(null);
    if (!object) {
      setDetails(null);
      return;
    }
    let live = true;
    objectDetails(object.id)
      .then((d) => live && setDetails(d))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [object?.id, object?.revision]);

  useEffect(() => setName(object?.name ?? ''), [object?.id, object?.name]);

  if (!object) return <p className="p-3 text-xs text-muted-foreground">Select an object in the viewport or the objects list.</p>;

  const rename = () => {
    const next = name.trim();
    if (next && next !== object.name) void run('object.rename', { object: object.id, name: next });
    else setName(object.name);
  };
  const current = details?.id === object.id ? details : null;
  const attributes = Object.entries(object.attributes);

  return (
    <ScrollArea className="h-full">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <Badge variant="secondary">{kindLabel[object.kind]}</Badge>
        <Input
          className="h-7 flex-1 text-sm font-medium"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={rename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur();
            if (e.key === 'Escape') {
              setName(object.name);
              e.currentTarget.blur();
            }
          }}
          aria-label="Name"
        />
        <Button
          variant="ghost"
          size="icon-sm"
          title="Delete (undo brings it back)"
          onClick={async () => (await run('object.delete', { object: object.id })) && select(null)}
        >
          <Trash2 />
        </Button>
      </div>
      <Section title="Object">
        <Field label="Layer">{current?.layer ?? '…'}</Field>
        <Field label="Revision">{object.revision}</Field>
        {current?.bounds && (
          <>
            <Field label="Easting">
              {coord(current.bounds[0][0])} to {coord(current.bounds[1][0])}
            </Field>
            <Field label="Northing">
              {coord(current.bounds[0][1])} to {coord(current.bounds[1][1])}
            </Field>
            <Field label="RL">
              {coord(current.bounds[0][2])} to {coord(current.bounds[1][2])}
            </Field>
          </>
        )}
      </Section>
      {error && <p className="px-3 py-2 text-xs text-destructive">{error}</p>}
      {current && <Measurements d={current} />}
      {attributes.length > 0 && (
        <Section title="Attributes">
          {attributes.map(([key, value]) => (
            <Field key={key} label={key}>
              {String(value)}
            </Field>
          ))}
        </Section>
      )}
      {current && (
        <Section title="History">
          {[...current.history].reverse().map((h) => (
            <Field key={h.revision} label={`Revision ${h.revision}`}>
              <span title={`run ${h.run}`}>
                {h.command ?? 'earlier session'}
                {h.at_unix_ms !== null && <span className="text-muted-foreground"> · {clock(h.at_unix_ms)}</span>}
              </span>
            </Field>
          ))}
        </Section>
      )}
    </ScrollArea>
  );
}
