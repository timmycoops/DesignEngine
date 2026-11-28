import { useEffect, useMemo, useState } from 'react';
import { Check, Copy, Focus, Trash2 } from 'lucide-react';
import {
  type AddUp,
  type Colouring,
  type Crossings,
  type Dataflow,
  layerOpacityOf,
  lookOf,
  type MeshValidity,
  type ObjectDetails,
  objectDetails,
  objectsBounds,
  type ObjectSummary,
  type SeamModel,
  solidsUnder,
  type StackIssue,
  type Vec3,
  type WriteFigures,
} from '@/bridge';
import { columnHelper, DataTable } from '@/design-system/DataTable';
import { Badge } from '@/design-system/ui/badge';
import { Button } from '@/design-system/ui/button';
import { ScrollArea } from '@/design-system/ui/scroll-area';
import { Choice as KitChoice, Field, FlagBadge, NumberInput, PanelButton, PanelHead, PercentSlider, quiet, type Said, SaidLine, ShownRow, ShownSection, TextInput } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { canColour, canSeeThrough, clock, coord, fmt, kindLabel } from './format';

/**
 * The object's own 128-bit id, as 32 hex digits, with a button that copies it: what tells two objects of one name apart,
 * and what a filter picks it by, `id: …` (decision 0091).
 */
function IdField({ id }: { id: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <ShownRow label="Id">
      <span className="flex min-w-0 items-center gap-1">
        <span data-slot="object-id" className="min-w-0 truncate font-mono text-[11px]" title={id}>
          {id}
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          title={copied ? 'Copied: paste it into a filter as id: …' : 'Copy the id, to pick this object by it in a filter (id: …)'}
          aria-label="Copy the id"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(id);
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            } catch {
              setCopied(false);
            }
          }}
        >
          {copied ? <Check /> : <Copy />}
        </Button>
      </span>
    </ShownRow>
  );
}

/** What the crossing check says, in a line. */
function crossingText(c: Crossings | null): string {
  if (!c) return 'Not checked: made before crossing checks';
  if (c.check.by === 'lattice') return "A grid's triangulation cannot cross itself";
  const tolerance = `${fmt(c.check.tolerance * 1000, 1)} mm`;
  const crossing = c.pairs === 0 ? `No crossings by more than ${tolerance}` : `${fmt(c.pairs)} pairs of triangles cross by more than ${tolerance}`;
  return c.thin > 0 ? `${crossing}; ${fmt(c.thin)} thin pairs (knife edges, hairline gaps or thin lips), kept` : crossing;
}

function Validity({ v, crossings }: { v: MeshValidity; crossings: Crossings | null }) {
  return (
    <div className="flex flex-wrap gap-1 px-3 pt-1">
      <FlagBadge label="Closed" value={v.closed} detail={`${fmt(v.boundary_edges)} boundary edges`} />
      <FlagBadge label="Manifold" value={v.manifold} detail={`${fmt(v.non_manifold_edges)} non-manifold edges`} />
      <FlagBadge label="Consistent winding" value={v.consistent_winding} detail={`${fmt(v.inconsistent_edges)} inconsistently wound edges`} />
      <FlagBadge label="Non-degenerate" value={v.non_degenerate} detail={`${fmt(v.degenerate_triangles)} degenerate triangles`} />
      <FlagBadge label="Non-self-intersecting" value={v.non_self_intersecting} detail={crossingText(crossings)} />
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
    <ShownSection title="Crossings">
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
    </ShownSection>
  );
}

/**
 * Where a solid comes within the tolerance of itself without crossing: a knife edge, a hairline
 * gap or a thin lip, kept as it is. Marked in the viewport while selected, unless there are crossings.
 */
function ThinSites({ crossings }: { crossings: Crossings }) {
  const { markCrossings, flyTo } = useWorkspace();
  const marked = crossings.pairs === 0;
  useEffect(() => {
    if (!marked) return;
    markCrossings(crossings.thin_at);
    return () => markCrossings([]);
  }, [crossings, marked, markCrossings]);
  const tolerance = crossings.check.by === 'lattice' ? '' : ` ${fmt(crossings.check.tolerance * 1000, 1)} mm`;
  return (
    <ShownSection title="Thin">
      <p className="px-3 pb-1 text-xs text-muted-foreground">
        {fmt(crossings.thin)} pairs of faces within{tolerance} of each other: the solid tapers to a knife edge, comes within a hairline
        of itself or has a thin lip there. Not crossings; kept as they are.
        {crossings.thin > crossings.thin_at.length && ` The first ${fmt(crossings.thin_at.length)} are listed.`}
      </p>
      <ul className="px-3 text-xs tabular-nums">
        {crossings.thin_at.map((p, n) => (
          <li key={n} className="cursor-pointer rounded px-1 hover:bg-selection" title="Show in the viewport" onClick={() => flyTo(p)}>
            {fmt(p[0], 2)} E {fmt(p[1], 2)} N RL {fmt(p[2], 2)}
          </li>
        ))}
      </ul>
    </ShownSection>
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

/**
 * How the selected object draws: by RL or in its layer's colour, and how see-through, its own and its
 * layer's (they multiply). Display only: saved with the project, not undone. A layer's solids draw as one
 * mesh, so a solid is seen through by its layer alone; a seam horizon keeps its thickness colours.
 */
function Look({ object }: { object: ObjectSummary }) {
  const { summary, setLooks, setLayerOpacity } = useWorkspace();
  const look = lookOf(summary?.display, object.id);
  const solid = object.kind === 'solid';
  const seam = object.kind === 'seam_model';
  return (
    <ShownSection title="Look">
      {!solid && !seam && (
        <ShownRow label="Colour">
          <KitChoice<Colouring> value={look.colouring} options={['by_rl', 'layer']} labels={{ by_rl: 'By RL', layer: 'Layer colour' }} label="How it's coloured" onChange={(colouring) => setLooks([object.id], colouring, null)} />
        </ShownRow>
      )}
      {!solid && <PercentSlider label="Opacity" value={look.opacity} onChange={(o, commit) => setLooks([object.id], null, o, commit)} />}
      <PercentSlider label="Layer opacity" value={layerOpacityOf(summary?.display, object.layer)} onChange={(o, commit) => setLayerOpacity(object.layer, o, commit)} />
      {solid && <p className="px-3 pt-0.5 text-xs text-muted-foreground">A layer's solids draw together, so they're seen through by the layer.</p>}
    </ShownSection>
  );
}


/** Cutting lives in its own panel (decision 0049): Properties shows what's picked, and points the way. */
function OpenCut() {
  const { openPanel } = useWorkspace();
  return (
    <div className="flex items-center gap-2 px-3 py-1 text-xs text-muted-foreground">
      Cut it, or slice it into strips, in
      <PanelButton row onClick={() => openPanel('cut')}>
        Cut solids
      </PanelButton>
    </div>
  );
}

interface HorizonRow {
  code: string;
  depth: number;
  parent: string;
  material: string;
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
  horizonHelper.accessor('material', { header: 'Material' }),
  horizonHelper.accessor('working', { header: 'Working', cell: (c) => (c.getValue() ? 'Yes' : '') }),
  horizonHelper.accessor('grids', { header: 'Grids', cell: (c) => <span className="text-muted-foreground">{c.getValue()}</span> }),
]);

/** The model's horizons, top first; click one to show it in the viewport. */
function Horizons({ id, model }: { id: string; model: SeamModel }) {
  const { seamViews, showSeamHorizon } = useWorkspace();
  const shown = seamViews.find((s) => s.id === id)?.result.horizon ?? null;
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
      material: h.material,
      working: h.working,
      grids: ['srg', 'sfg', ...Object.keys(h.grids)].join(' '),
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
  holes: (i) => `${i.horizon} roof or floor has holes: no height at ${i.nodes} nodes`,
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

type Cells = { key: string; cells: string[] };
const cellHelper = columnHelper<Cells>();

/** A small read-only table of words, a header a column. */
function CellTable({ headers, rows }: { headers: string[]; rows: string[][] }) {
  const data = useMemo(() => rows.map((cells, i) => ({ key: String(i), cells })), [rows]);
  const columns = useMemo(
    () => cellHelper.columns(headers.map((h, c) => cellHelper.accessor((r) => r.cells[c] ?? '', { id: `c${c}`, header: h, cell: (x) => <span className="tabular-nums">{x.getValue()}</span> }))),
    [headers],
  );
  return <DataTable data={data} columns={columns} rowId={(r) => r.key} />;
}

const addUpWords = (a: AddUp | undefined) => {
  if (!a) return '';
  switch (a.by) {
    case 'sum':
      return 'summed';
    case 'volume':
      return 'by volume';
    case 'mass':
      return `by tonnes (${a.density})`;
    case 'max':
      return 'largest';
    case 'min':
      return 'smallest';
    case 'ratio':
      return `${a.of} over ${a.per}`;
  }
};

const fromWords = (f: Dataflow['fields'][number]) => {
  switch (f.from) {
    case 'volume':
      return 'solid volume';
    case 'plan_area':
      return 'plan area';
    case 'thickness':
      return 'volume ÷ plan area';
    case 'attribute':
      return `attribute ${f.attribute ?? ''}`;
    case 'parameter':
      return `parameter ${f.column ?? ''}`;
    case 'worked':
      return 'worked by a step';
    default:
      return f.from;
  }
};

const cell = (v: number | string | null | undefined) => (v === null || v === undefined ? '–' : typeof v === 'string' ? v : Math.abs(v) >= 1000 || Number.isInteger(v) ? fmt(v) : fmt(v, 3));

/** A dataflow's fields, steps and the parameter tables it reads, by name (decision 0072). */
function DataflowView({ flow }: { flow: Dataflow }) {
  const { summary } = useWorkspace();
  const names = new Map((summary?.objects ?? []).map((o) => [o.id, o.name]));
  const steps = flow.steps.map((s) => (s.on === false ? `${s.step} (off)` : s.step));
  return (
    <>
      <ShownSection title="Dataflow">
        <ShownRow label="Levels">{(flow.levels ?? []).map((l) => (l.default ? `${l.attribute} (else ${l.default})` : l.attribute)).join(' › ') || 'pit › strip › block › seam › bench'}</ShownRow>
        <ShownRow label="Steps">{steps.join(', ') || 'none'}</ShownRow>
        <ShownRow label="Parameter tables">{flow.parameters.map((id) => names.get(id) ?? 'missing').join(', ') || 'none'}</ShownRow>
        <ShownRow label="Fields">{fmt(flow.fields.length)}</ShownRow>
      </ShownSection>
      <ShownSection title="Fields">
        <CellTable headers={['Field', 'Kind', 'Adds up', 'From']} rows={flow.fields.map((f) => [f.name, f.kind, addUpWords(f.add_up), fromWords(f)])} />
      </ShownSection>
      {(flow.words ?? []).length > 0 && (
        <ShownSection title="Word fields">
          <CellTable headers={['Field', 'From']} rows={(flow.words ?? []).map((w) => [w.name, w.from === 'attribute' ? `attribute ${w.attribute ?? ''}` : w.from === 'parameter' ? `parameter column ${w.column ?? ''}` : 'worked by a step'])} />
        </ShownSection>
      )}
    </>
  );
}

/** A reserves table's leaves added up by a top folder (decision 0074): by seam and the folder below, its totals, and by
 * bench. */
function WriteView({ w }: { w: WriteFigures }) {
  const heads = w.columns.map(([n, u]) => (u ? `${n.replace(/_/g, ' ')} ${u.replace('m3', 'm³').replace('m2', 'm²')}` : n.replace(/_/g, ' ')));
  const line = (r: WriteFigures['seams'][number]) => [r.label, r.material, ...r.values.map(cell), fmt(r.rows)];
  return (
    <>
      <ShownSection title={`${w.write} by seam`}>
        <CellTable headers={['Seam', 'Folder', ...heads, 'Leaves']} rows={[...w.seams, ...w.totals].map(line)} />
      </ShownSection>
      {w.benches.length > 0 && (
        <ShownSection title={`${w.write} by bench`}>
          <CellTable headers={['Bench', 'Folder', ...heads, 'Leaves']} rows={w.benches.map(line)} />
        </ShownSection>
      )}
    </>
  );
}

function Measurements({ d }: { d: ObjectDetails }) {
  const m = d.measures;
  switch (m.kind) {
    case 'string':
      return (
        <>
          <ShownSection title="String">
            <ShownRow label="Vertices">{fmt(m.points.length)}</ShownRow>
            <ShownRow label="Closed">{m.closed ? 'Yes' : 'No'}</ShownRow>
            <ShownRow label="Length (slope)">{fmt(m.length, 2)} m</ShownRow>
            <ShownRow label="Length (plan)">{fmt(m.plan_length, 2)} m</ShownRow>
            {m.plan_area !== null && <ShownRow label="Area (plan)">{fmt(m.plan_area, 1)} m²</ShownRow>}
          </ShownSection>
          <ShownSection title="Vertices">
            <Vertices points={m.points} closed={m.closed} />
          </ShownSection>
        </>
      );
    case 'surface':
    case 'solid':
      return (
        <>
          <ShownSection title={m.kind === 'solid' ? 'Solid' : 'Surface'}>
            <ShownRow label="Triangles">{fmt(m.triangles)}</ShownRow>
            <ShownRow label="Vertices">{fmt(m.vertices)}</ShownRow>
            <ShownRow label="Area">{fmt(m.area, 1)} m²</ShownRow>
            {m.kind === 'solid' && <ShownRow label="Volume">{fmt(m.volume, 1)} m³</ShownRow>}
            {m.kind === 'solid' && m.pieces.pieces > 1 && (
              <ShownRow label="Pieces">
                {fmt(m.pieces.pieces)}, the smallest {fmt(m.pieces.smallest_piece, m.pieces.smallest_piece < 10 ? 3 : 1)} m³
              </ShownRow>
            )}
            {m.kind === 'solid' && m.pieces.cavities > 0 && (
              <ShownRow label="Cavities">
                {fmt(m.pieces.cavities)}, {fmt(m.pieces.cavity_volume, m.pieces.cavity_volume < 10 ? 3 : 1)} m³ in all
              </ShownRow>
            )}
            <Validity v={m.validity} crossings={m.crossings} />
            {m.crossings && m.crossings.pairs > 0 && <CrossingSites crossings={m.crossings} />}
            {m.crossings && m.crossings.thin > 0 && <ThinSites crossings={m.crossings} />}
          </ShownSection>
          {m.kind === 'solid' && <OpenCut />}
        </>
      );
    case 'grid': {
      const nodes = m.cols * m.rows;
      return (
        <ShownSection title="Grid">
          <ShownRow label="Nodes">
            {fmt(m.cols)} × {fmt(m.rows)}
          </ShownRow>
          <ShownRow label="Cell size">
            {fmt(m.cell_size[0], 2)} × {fmt(m.cell_size[1], 2)} m
          </ShownRow>
          <ShownRow label="Origin (SW node)">
            {coord(m.origin[0])} E, {coord(m.origin[1])} N
          </ShownRow>
          <ShownRow label="Nulls">
            {fmt(m.nulls)} ({fmt((100 * m.nulls) / Math.max(nodes, 1), 1)}%)
          </ShownRow>
          <ShownRow label="Value range">{m.z_range ? `${fmt(m.z_range[0], 3)} to ${fmt(m.z_range[1], 3)}` : 'all null'}</ShownRow>
        </ShownSection>
      );
    }
    case 'parameter_table': {
      const t = m.parameters;
      const headers = [...t.filters.map((f) => `when ${f}`), ...t.columns.map((c) => (c.unit ? `${c.name} (${c.unit})` : c.name))];
      const rows = t.rows.map((r) => [
        ...t.filters.map((f) => Object.entries(r.when ?? {}).find(([k]) => k.toLowerCase() === f.toLowerCase())?.[1] ?? ''),
        ...t.columns.map((c) => {
          const v = r.values[c.name];
          return v === undefined ? '' : String(v);
        }),
      ]);
      const saveCsv = () => {
        const link = document.createElement('a');
        link.href = URL.createObjectURL(new Blob([m.csv], { type: 'text/csv' }));
        link.download = `${d.name}.csv`;
        link.click();
        URL.revokeObjectURL(link.href);
      };
      return (
        <>
          <ShownSection title="Parameter table">
            <ShownRow label="Rows">{fmt(t.rows.length)}</ShownRow>
            <ShownRow label="Filters on">{t.filters.join(', ') || 'nothing'}</ShownRow>
            <ShownRow label="Values">{t.columns.map((c) => c.name).join(', ')}</ShownRow>
            <p className="px-3 py-1 text-xs text-muted-foreground">Read top down: each value comes from the first row that fits and holds it; a blank filter fits anything.</p>
            <div className="px-3 py-1">
              <Button size="sm" variant="outline" onClick={saveCsv}>
                Save as CSV
              </Button>
            </div>
          </ShownSection>
          <ShownSection title="Rows">
            <CellTable headers={headers} rows={rows} />
          </ShownSection>
        </>
      );
    }
    case 'dataflow':
      return <DataflowView flow={m.dataflow} />;
    case 'script':
      return (
        <ShownSection title="Script">
          {m.script.note && <p className="px-3 py-1 text-xs text-muted-foreground">{m.script.note}</p>}
          <pre className="mx-3 my-1 overflow-auto rounded bg-muted/40 p-2 font-mono text-[11px]">{m.script.source}</pre>
          <p className="px-3 py-1 text-xs text-muted-foreground">Loaded by its name, load("name", "function"); edited in the Parameters tab.</p>
        </ShownSection>
      );
    case 'table':
      return (
        <>
          <ShownSection title="Reserves table">
            <ShownRow label="Leaves">{fmt(m.rows)}</ShownRow>
            <ShownRow label="Columns">{fmt(m.columns.length)}</ShownRow>
            <ShownRow label="Top folders">{m.figures.writes.map((w) => w.write).join(', ') || 'none'}</ShownRow>
            <p className="px-3 py-1 text-xs text-muted-foreground">Each column added up by its own rule: summed, by volume, by tonnes, the largest or the smallest.</p>
          </ShownSection>
          {m.figures.writes.map((w) => (
            <WriteView key={w.write} w={w} />
          ))}
        </>
      );
    case 'seam_model': {
      const { model } = m;
      const blocking = model.issues.filter((i) => i.blocking).length;
      return (
        <>
          <ShownSection title="Seam model">
            <ShownRow label="Horizons">
              {fmt(model.horizons.length)} ({fmt(model.horizons.filter((h) => h.working).length)} working)
            </ShownRow>
            <ShownRow label="Nodes">
              {fmt(model.lattice.cols)} × {fmt(model.lattice.rows)} at {fmt(model.lattice.cell_size[0], 2)} m
            </ShownRow>
            <ShownRow label="Grid folder">
              <span title={model.folder}>{model.folder}</span>
            </ShownRow>
            <ShownRow label="Density grid">{model.density}</ShownRow>
            <ShownRow label="Tolerance">{fmt(model.tolerance, 3)} m</ShownRow>
            {model.no_seam !== null && <ShownRow label="No seam at">RL {fmt(model.no_seam, 2)}</ShownRow>}
            <ShownRow label="Status">
              {blocking === 0 ? 'Ready for solids and reserves' : `${blocking} blocking issue${blocking === 1 ? '' : 's'}: fix the grids and rebuild`}
            </ShownRow>
          </ShownSection>
          <ShownSection title="Issues">
            <Issues id={d.id} issues={model.issues} />
          </ShownSection>
          <ShownSection title="Horizons (top first): click to show">
            <Horizons id={d.id} model={model} />
          </ShownSection>
        </>
      );
    }
  }
}

/**
 * Numbers strings by a line drawn across them (`string.number_across`), each named with its number in place of #.
 * `which` says what it numbers, for the tip.
 */
/**
 * Keeps selected only the solids holding less than a volume: small pieces to look at and delete by hand (decision 0062;
 * no automatic limit). Select a layer's solids first: Select all on it, on its right-click in the tree.
 */
function SolidsUnder({ solids }: { solids: ObjectSummary[] }) {
  const { setSelection } = useWorkspace();
  const [volume, setVolume] = useState(100);
  const [said, setSaid] = useState<Said | null>(null);
  const narrow = async () => {
    const under = await solidsUnder(
      solids.map((o) => o.id),
      volume,
    );
    if (under.length === 0) return setSaid(quiet(`None of the ${fmt(solids.length)} holds less than ${fmt(volume)} m³`));
    setSaid(null);
    setSelection(under);
  };
  return (
    <>
      <div className="px-3 text-xs">
        <Field label="Under">
          <NumberInput value={volume} onChange={setVolume} label="Keep selected only the solids holding less than this" unit="m³" range={{ min: 0, minIncluded: false }} />
          <PanelButton row onClick={() => void narrow()}>
            Keep those selected
          </PanelButton>
        </Field>
      </div>
      <SaidLine said={said} className="px-3 py-0.5 text-xs" />
    </>
  );
}

/** How many of each kind, most first: "12 solids · 3 strings". */
function kindCounts(objects: ObjectSummary[]): string {
  const counts = new Map<string, number>();
  for (const o of objects) counts.set(kindLabel[o.kind], (counts.get(kindLabel[o.kind]) ?? 0) + 1);
  return [...counts]
    .sort((a, b) => b[1] - a[1])
    .map(([kind, n]) => `${fmt(n)} ${kind.toLowerCase()}${n === 1 ? '' : 's'}`)
    .join(' · ');
}

/** The most rows the list of several selected shows; the rest are counted. */
const MOST_LISTED = 200;

/**
 * Several selected: how many of each kind and on which layers, where they are together, how they look, numbering
 * for the strings among them, and a list where a click selects one alone. Zoom to and delete take all of them.
 */
function SelectionProperties({ objects }: { objects: ObjectSummary[] }) {
  const { summary, select, run, setLooks, zoomTo } = useWorkspace();
  const [box, setBox] = useState<[Vec3, Vec3] | null>(null);
  const ids = objects.map((o) => o.id);
  // Changes when what's selected does, or any of it is revised, without a key as long as the selection.
  const key = `${objects.length}:${objects.reduce((sum, o) => sum + o.revision, 0)}:${ids[0]}:${ids.at(-1)}`;
  useEffect(() => {
    let live = true;
    setBox(null);
    objectsBounds(ids)
      .then((b) => live && setBox(b))
      .catch(() => live && setBox(null));
    return () => {
      live = false;
    };
  }, [key]);

  const layerNames = new Map(summary?.layers.map((l) => [l.id, l.name]));
  const perLayer = new Map<string, number>();
  for (const o of objects) perLayer.set(o.layer, (perLayer.get(o.layer) ?? 0) + 1);
  const coloured = objects.filter(canColour);
  const seeThrough = objects.filter(canSeeThrough);
  const colourings = new Set(coloured.map((o) => lookOf(summary?.display, o.id).colouring));
  const solids = objects.filter((o) => o.kind === 'solid');

  return (
    <ScrollArea className="h-full">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <span className="min-w-0 flex-1 truncate text-sm font-medium" title={kindCounts(objects)}>
          {fmt(objects.length)} selected
        </span>
        <Button variant="ghost" size="icon-sm" title="Zoom to them (F)" onClick={() => zoomTo(ids)}>
          <Focus />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          title={`Delete all ${fmt(objects.length)} (one undo brings them back)`}
          onClick={async () => (await run('object.delete_many', { objects: ids })) && select(null)}
        >
          <Trash2 />
        </Button>
      </div>
      <ShownSection title="Selection">
        <ShownRow label="What">{kindCounts(objects)}</ShownRow>
        <ShownRow label={perLayer.size === 1 ? 'Layer' : 'Layers'}>
          <span title={[...perLayer].map(([id, n]) => `${layerNames.get(id) ?? id} (${fmt(n)})`).join('\n')}>
            {[...perLayer].map(([id, n]) => (perLayer.size === 1 ? layerNames.get(id) : `${layerNames.get(id)} (${fmt(n)})`)).join(', ')}
          </span>
        </ShownRow>
        {box && (
          <>
            <ShownRow label="Easting">
              {coord(box[0][0])} to {coord(box[1][0])}
            </ShownRow>
            <ShownRow label="Northing">
              {coord(box[0][1])} to {coord(box[1][1])}
            </ShownRow>
            <ShownRow label="RL">
              {coord(box[0][2])} to {coord(box[1][2])}
            </ShownRow>
          </>
        )}
      </ShownSection>
      {seeThrough.length > 0 && (
        <ShownSection title="Look">
          {coloured.length > 0 && (
            <div className="grid grid-cols-[7.5rem_1fr] items-center gap-2 px-3 py-0.5 text-xs">
              <span className="text-muted-foreground">Colour</span>
              <span>
                <KitChoice<Colouring | 'mixed'>
                  label={`How the ${fmt(coloured.length)} surfaces and grids among them are coloured`}
                  value={colourings.size === 1 ? [...colourings][0] : 'mixed'}
                  options={['by_rl', 'layer']}
                  labels={{ by_rl: 'By RL', layer: 'Layer colour' }}
                  onChange={(colouring) => colouring !== 'mixed' && setLooks(coloured.map((o) => o.id), colouring, null)}
                />
              </span>
            </div>
          )}
          <PercentSlider
            label="Opacity"
            value={lookOf(summary?.display, seeThrough[0].id).opacity}
            onChange={(o, commit) => setLooks(seeThrough.map((x) => x.id), null, o, commit)}
          />
          {seeThrough.length < objects.length && <p className="px-3 pt-0.5 text-xs text-muted-foreground">Solids and strings are seen through by their layers: right-click a layer in the tree.</p>}
        </ShownSection>
      )}
      {solids.length > 0 && (
        <ShownSection title="Solids">
          <SolidsUnder solids={solids} />
        </ShownSection>
      )}
      <ShownSection title="Each">
        {objects.slice(0, MOST_LISTED).map((o) => (
          <button
            key={o.id}
            type="button"
            className="grid w-full grid-cols-[7.5rem_1fr] gap-2 px-3 py-0.5 text-left text-xs hover:bg-accent"
            title="Select this one alone"
            onClick={() => select(o.id)}
          >
            <span className="text-muted-foreground">{kindLabel[o.kind]}</span>
            <span className="min-w-0 truncate">{o.name}</span>
          </button>
        ))}
        {objects.length > MOST_LISTED && <p className="px-3 py-0.5 text-xs text-muted-foreground">and {fmt(objects.length - MOST_LISTED)} more</p>}
      </ShownSection>
    </ScrollArea>
  );
}

/**
 * Properties (decision 0076): what's picked, under a head saying so, as every panel has. It shows only; a command on
 * what's picked is a tool.
 */
export function PropertiesPanel() {
  return (
    <div className="flex h-full flex-col">
      <PanelHead line="What's picked: name, look, measures, history" more="What's picked: its name, look, attributes, measures and history. Properties shows only. A command on what's picked is a tool on the ribbon; renaming is here, and Delete." />
      <div className="min-h-0 flex-1">
        <PickedProperties />
      </div>
    </div>
  );
}

/** The selected object: name (editable), layer, measurements, validity, attributes and history. Several selected show together. */
function PickedProperties() {
  const { summary, selected, selection, run, select } = useWorkspace();
  const several = useMemo(() => {
    if (selection.length < 2) return null;
    const ids = new Set(selection);
    return summary?.objects.filter((o) => ids.has(o.id)) ?? null;
  }, [summary, selection]);
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

  if (several && several.length > 1) return <SelectionProperties objects={several} />;
  if (!object) return <p className="p-3 text-xs text-muted-foreground">Select an object in the viewport or the layer tree. Ctrl-click or Shift-drag a box to select several.</p>;

  const rename = () => {
    const next = name.trim();
    if (next && next !== object.name) void run('object.rename', { object: object.id, name: next });
    else setName(object.name);
  };
  const current = details?.id === object.id ? details : null;
  // A row carries only the attributes the app reads by name; the details have them all.
  const attributes = Object.entries(current?.attributes ?? object.attributes);

  return (
    <ScrollArea className="h-full">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <Badge variant="secondary">{kindLabel[object.kind]}</Badge>
        <TextInput
          className="flex-1 text-sm font-medium"
          value={name}
          label="Name"
          onChange={setName}
          onBlur={rename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur();
            if (e.key === 'Escape') {
              // The name put back is this press: it doesn't go on to clear the selection (decision 0080).
              e.preventDefault();
              setName(object.name);
              e.currentTarget.blur();
            }
          }}
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
      <ShownSection title="Object">
        <ShownRow label="Layer">{current?.layer ?? '…'}</ShownRow>
        <ShownRow label="Revision">{object.revision}</ShownRow>
        <IdField id={object.id} />
        {current?.bounds && (
          <>
            <ShownRow label="Easting">
              {coord(current.bounds[0][0])} to {coord(current.bounds[1][0])}
            </ShownRow>
            <ShownRow label="Northing">
              {coord(current.bounds[0][1])} to {coord(current.bounds[1][1])}
            </ShownRow>
            <ShownRow label="RL">
              {coord(current.bounds[0][2])} to {coord(current.bounds[1][2])}
            </ShownRow>
          </>
        )}
      </ShownSection>
      {(object.drawn || object.kind === 'seam_model') && <Look object={object} />}
      {error && <p className="px-3 py-2 text-xs text-destructive">{error}</p>}
      {current && <Measurements d={current} />}
      {attributes.length > 0 && (
        <ShownSection title="Attributes">
          {attributes.map(([key, value]) => (
            <ShownRow key={key} label={key}>
              {String(value)}
            </ShownRow>
          ))}
        </ShownSection>
      )}
      {current && (
        <ShownSection title="History">
          {[...current.history].reverse().map((h) => (
            <ShownRow key={h.revision} label={`Revision ${h.revision}`}>
              <span title={`run ${h.run}`}>
                {h.command ?? 'earlier session'}
                {h.at_unix_ms !== null && <span className="text-muted-foreground"> · {clock(h.at_unix_ms)}</span>}
              </span>
            </ShownRow>
          ))}
        </ShownSection>
      )}
    </ScrollArea>
  );
}
