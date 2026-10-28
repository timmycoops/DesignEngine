import { useEffect, useMemo, useState, type ReactNode } from 'react';
import { Check, Minus, Trash2, X } from 'lucide-react';
import { type MeshValidity, type ObjectDetails, objectDetails, type Vec3 } from '@/bridge';
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
