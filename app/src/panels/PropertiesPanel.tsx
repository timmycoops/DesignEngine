import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Check, Minus, Trash2, X } from 'lucide-react';
import { type MeshValidity, type ObjectDetails, objectDetails, type SeamModel, type StackIssue, type Vec3 } from '@/bridge';
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

function Validity({ v }: { v: MeshValidity }) {
  return (
    <div className="flex flex-wrap gap-1 px-3 pt-1">
      <Flag label="Closed" value={v.closed} detail={`${fmt(v.boundary_edges)} boundary edges`} />
      <Flag label="Manifold" value={v.manifold} detail={`${fmt(v.non_manifold_edges)} non-manifold edges`} />
      <Flag label="Consistent winding" value={v.consistent_winding} detail={`${fmt(v.inconsistent_edges)} inconsistently wound edges`} />
      <Flag label="Non-degenerate" value={v.non_degenerate} detail={`${fmt(v.degenerate_triangles)} degenerate triangles`} />
      <Flag label="Non-self-intersecting" value={v.non_self_intersecting} detail="Not computed yet" />
    </div>
  );
}

interface VertexRow {
  index: number;
  x: number;
  y: number;
  z: number;
}

const vertexHelper = columnHelper<VertexRow>();
const vertexColumns = vertexHelper.columns([
  vertexHelper.accessor('index', { header: '#', cell: (c) => <span className="text-muted-foreground tabular-nums">{c.getValue()}</span> }),
  vertexHelper.accessor('x', { header: 'Easting', cell: (c) => <span className="tabular-nums">{coord(c.getValue())}</span> }),
  vertexHelper.accessor('y', { header: 'Northing', cell: (c) => <span className="tabular-nums">{coord(c.getValue())}</span> }),
  vertexHelper.accessor('z', { header: 'RL', cell: (c) => <span className="tabular-nums">{coord(c.getValue())}</span> }),
]);

function Vertices({ points }: { points: Vec3[] }) {
  const rows = useMemo(() => points.map(([x, y, z], i) => ({ index: i + 1, x, y, z })), [points]);
  return <DataTable data={rows} columns={vertexColumns} rowId={(r) => String(r.index)} />;
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

function Horizons({ model }: { model: SeamModel }) {
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
  return <DataTable data={rows} columns={horizonColumns} rowId={(r) => r.code} />;
}

const issueText: Record<StackIssue['kind'], (i: StackIssue) => string> = {
  floor_above_roof: (i) => `${i.horizon} floor above its roof`,
  crossing: (i) => `${i.horizon} roof above the floor of ${i.other ?? '?'}`,
  outside_parent: (i) => `${i.horizon} outside its parent ${i.other ?? '?'}`,
  unpaired: (i) => `${i.horizon} roof and floor not live at the same nodes`,
  no_density: (i) => `${i.horizon} has no density grid`,
};

function Issues({ issues }: { issues: StackIssue[] }) {
  if (issues.length === 0) return <p className="px-3 py-1 text-xs text-muted-foreground">None: the stack is consistent.</p>;
  return (
    <ul className="space-y-1 px-3 py-1 text-xs">
      {issues.map((i, n) => (
        <li key={n} className="flex gap-2">
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
          <Section title="Vertices">
            <Vertices points={m.points} />
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
          <Validity v={m.validity} />
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
            <Issues issues={model.issues} />
          </Section>
          <Section title="Horizons (top first)">
            <Horizons model={model} />
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
