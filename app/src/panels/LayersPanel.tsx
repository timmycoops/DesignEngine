import { Box, ChevronDown, ChevronRight, Grid3x3, Layers, type LucideIcon, Mountain, PencilRuler, Spline } from 'lucide-react';
import { useMemo, useState } from 'react';
import type { Layer, ObjectKind, ObjectSummary } from '@/bridge';
import { ROW_HEIGHT, useRowWindow } from '@/design-system/DataTable';
import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { kindLabel } from './format';

/** A layer holding more objects than this starts closed. */
const OPEN_UP_TO = 50;

const kindIcon: Record<ObjectKind, LucideIcon> = {
  string: Spline,
  surface: Mountain,
  solid: Box,
  grid: Grid3x3,
  seam_model: Layers,
  face_design: PencilRuler,
};

type LayerEntry = { kind: 'layer'; layer: Layer; count: number; open: boolean };
type TreeRow = LayerEntry | { kind: 'object'; object: ObjectSummary };

/** Each layer, followed by its objects if it is open. `toggled` holds the layers opened or closed by hand. */
function treeRows(layers: Layer[], objects: ObjectSummary[], toggled: Map<string, boolean>): TreeRow[] {
  const held = new Map<string, ObjectSummary[]>();
  for (const object of objects) {
    const on = held.get(object.layer);
    if (on) on.push(object);
    else held.set(object.layer, [object]);
  }
  const rows: TreeRow[] = [];
  for (const layer of layers) {
    const on = held.get(layer.id) ?? [];
    const open = toggled.get(layer.id) ?? on.length <= OPEN_UP_TO;
    rows.push({ kind: 'layer', layer, count: on.length, open });
    if (open) for (const object of on) rows.push({ kind: 'object', object });
  }
  return rows;
}

/** The layer whose objects fill the top of the tree, once its own row has scrolled away. */
function layerAbove(rows: TreeRow[], row: number): { entry: LayerEntry; at: number } | undefined {
  if (rows[row]?.kind !== 'object') return undefined;
  for (let at = row - 1; at >= 0; at--) {
    const entry = rows[at];
    if (entry.kind === 'layer') return { entry, at };
  }
  return undefined;
}

/** What a row's name doesn't say, shown on hover. */
function noteOf(object: ObjectSummary): string | undefined {
  if (object.kind === 'grid' && !object.drawn) return 'Property grid: listed, not drawn';
  if (object.kind === 'seam_model') return 'Seam model: its grids stay in their files';
  if (object.kind === 'face_design') return 'Face design: open it in the Face design panel';
  return undefined;
}

/** A layer's line weight in the section, in pixels; blank takes the default for what is cut. */
function LineWeight({ layer }: { layer: string }) {
  const { lineWeight, setLineWeight } = useWorkspace();
  const [text, setText] = useState(lineWeight(layer)?.toString() ?? '');
  return (
    <Input
      className="h-5 w-9 px-1 text-xs tabular-nums"
      value={text}
      placeholder="px"
      title="Line weight in the section (px); blank for the default"
      aria-label="Line weight in the section"
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => {
        setText(e.target.value);
        const px = Number(e.target.value);
        setLineWeight(layer, e.target.value.trim() !== '' && Number.isFinite(px) && px > 0 ? px : null);
      }}
    />
  );
}

function LayerRow({ layer, count, open, onToggle }: { layer: Layer; count: number; open: boolean; onToggle: () => void }) {
  const { activeLayer, setActiveLayer, setLayerVisible, sectionOpen } = useWorkspace();
  const Chevron = open ? ChevronDown : ChevronRight;
  const active = layer.name === activeLayer;
  return (
    <div
      data-slot="tree-row"
      className={cn('flex items-center gap-1.5 pr-2 pl-0.5 text-xs hover:bg-accent', active && 'bg-selection')}
      style={{ height: ROW_HEIGHT }}
      onClick={() => setActiveLayer(layer.name)}
      title="Click to draw new strings on this layer"
    >
      {count > 0 ? (
        <button
          type="button"
          className="grid size-4 shrink-0 place-items-center text-muted-foreground hover:text-foreground"
          aria-label={`${open ? 'Close' : 'Open'} ${layer.name}`}
          aria-expanded={open}
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
        >
          <Chevron className="size-3.5" />
        </button>
      ) : (
        <span className="size-4 shrink-0" />
      )}
      <Checkbox
        checked={layer.visible}
        onClick={(e) => e.stopPropagation()}
        onCheckedChange={(checked) => setLayerVisible(layer.id, checked === true)}
        aria-label={`Show ${layer.name}`}
      />
      <span className="size-2.5 shrink-0 rounded-sm" style={{ background: `rgb(${layer.colour.join(',')})` }} />
      <span className={cn('min-w-0 flex-1 truncate', active && 'text-primary')}>{layer.name}</span>
      {sectionOpen && <LineWeight layer={layer.id} />}
      <span className="text-muted-foreground tabular-nums">{count}</span>
    </div>
  );
}

function ObjectRow({ object }: { object: ObjectSummary }) {
  const { selected, select } = useWorkspace();
  const Icon = kindIcon[object.kind];
  const note = noteOf(object);
  return (
    <div
      data-slot="tree-row"
      data-state={object.id === selected ? 'selected' : undefined}
      className="flex cursor-pointer items-center gap-1.5 pr-2 pl-8 text-xs hover:bg-accent data-[state=selected]:bg-selection"
      style={{ height: ROW_HEIGHT }}
      onClick={() => select(object.id)}
    >
      <span title={kindLabel[object.kind]}>
        <Icon className="size-3.5 shrink-0 text-muted-foreground" />
      </span>
      <span className="min-w-0 flex-1 truncate" title={note ?? object.name}>
        {object.name}
        {note && <span className="ml-1 text-muted-foreground">·</span>}
      </span>
      {object.revision > 1 && (
        <span className="text-muted-foreground tabular-nums" title={`Revision ${object.revision}`}>
          r{object.revision}
        </span>
      )}
    </div>
  );
}

/**
 * Every layer and what is on it, in one tree that draws only the rows on screen. Click a layer to draw
 * new strings on it, or an object to select it.
 */
export function LayersPanel() {
  const { summary, activeLayer, setActiveLayer } = useWorkspace();
  const [toggled, setToggled] = useState(new Map<string, boolean>());
  const rows = useMemo(() => treeRows(summary?.layers ?? [], summary?.objects ?? [], toggled), [summary, toggled]);
  const { scroller, top, first, last, onScroll } = useRowWindow(rows.length, true);
  const toggle = (entry: LayerEntry) => setToggled((was) => new Map(was).set(entry.layer.id, !entry.open));
  const pinned = layerAbove(rows, Math.floor(top / ROW_HEIGHT));
  return (
    <div className="flex h-full flex-col">
      <label className="flex items-center gap-2 border-b px-2 py-1.5 text-xs text-muted-foreground">
        Draw on
        <Input
          className="h-6 flex-1 px-1.5 text-xs"
          value={activeLayer}
          onChange={(e) => setActiveLayer(e.target.value)}
          onBlur={(e) => !e.target.value.trim() && setActiveLayer('design')}
          title="New strings go on this layer; it is created with the first string"
        />
      </label>
      <div ref={scroller} onScroll={onScroll} data-slot="tree" className="min-h-0 flex-1 overflow-auto">
        {rows.length === 0 && <p className="px-2 py-3 text-xs text-muted-foreground">Nothing here yet: import a surface or draw a string.</p>}
        {pinned && (
          <div className="sticky top-0 z-10 bg-background" style={{ marginBottom: -ROW_HEIGHT }}>
            <LayerRow
              layer={pinned.entry.layer}
              count={pinned.entry.count}
              open
              onToggle={() => {
                toggle(pinned.entry);
                scroller.current?.scrollTo(0, pinned.at * ROW_HEIGHT);
              }}
            />
          </div>
        )}
        <div style={{ height: first * ROW_HEIGHT }} />
        {rows.slice(first, last).map((row) =>
          row.kind === 'layer' ? (
            <LayerRow key={row.layer.id} layer={row.layer} count={row.count} open={row.open} onToggle={() => toggle(row)} />
          ) : (
            <ObjectRow key={row.object.id} object={row.object} />
          ),
        )}
        <div style={{ height: (rows.length - last) * ROW_HEIGHT }} />
      </div>
    </div>
  );
}
