import { Box, ChevronDown, ChevronRight, Grid3x3, Layers, type LucideIcon, Mountain, PencilRuler, Spline } from 'lucide-react';
import { type MouseEvent, useEffect, useMemo, useRef, useState } from 'react';
import type { Layer, ObjectKind, ObjectSummary } from '@/bridge';
import { ROW_HEIGHT, useRowWindow } from '@/design-system/DataTable';
import { Checkbox } from '@/design-system/ui/checkbox';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/design-system/ui/context-menu';
import { Input } from '@/design-system/ui/input';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { fmt, kindLabel } from './format';

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

type LayerEntry = { kind: 'layer'; layer: Layer; count: number; hidden: number; open: boolean };
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
    rows.push({ kind: 'layer', layer, count: on.length, hidden: on.filter((o) => o.hidden).length, open });
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

/** A property grid or a face design has nothing to show or hide. */
const canHide = (object: ObjectSummary) => object.drawn || object.kind === 'string' || object.kind === 'seam_model';

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

/** A layer's colour; click to change it. The picker reports as it moves, so the change runs when it closes. */
function Swatch({ layer }: { layer: Layer }) {
  const { run } = useWorkspace();
  const input = useRef<HTMLInputElement>(null);
  const hex = `#${layer.colour.map((c) => c.toString(16).padStart(2, '0')).join('')}`;
  useEffect(() => {
    const element = input.current;
    if (!element) return;
    const changed = () => void run('layer.recolour', { layer: layer.name, colour: [1, 3, 5].map((k) => parseInt(element.value.slice(k, k + 2), 16)) });
    element.addEventListener('change', changed);
    return () => element.removeEventListener('change', changed);
  }, [layer.name, hex, run]);
  return (
    <label className="relative size-2.5 shrink-0 overflow-hidden rounded-sm" style={{ background: hex }} title="Change the layer's colour" onClick={(e) => e.stopPropagation()}>
      <input ref={input} key={hex} type="color" defaultValue={hex} className="absolute inset-0 cursor-pointer opacity-0" aria-label={`Colour of ${layer.name}`} />
    </label>
  );
}

/** A layer's name, being typed over. Enter or leaving the box renames it; Esc leaves it as it was. */
function RenameBox({ layer, onDone }: { layer: Layer; onDone: () => void }) {
  const { run, activeLayer, setActiveLayer } = useWorkspace();
  const finished = useRef(false);
  const finish = async (value: string) => {
    if (finished.current) return;
    finished.current = true;
    onDone();
    const name = value.trim();
    if (!name || name === layer.name) return;
    const active = layer.name === activeLayer;
    if ((await run('layer.rename', { layer: layer.name, name })) && active) setActiveLayer(name);
  };
  return (
    <Input
      autoFocus
      className="h-5 min-w-0 flex-1 px-1 text-xs"
      defaultValue={layer.name}
      aria-label={`New name for ${layer.name}`}
      onClick={(e) => e.stopPropagation()}
      ref={(box) => {
        // After the menu that opened this has let go of the focus.
        setTimeout(() => box?.select());
      }}
      onBlur={(e) => void finish(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') void finish(e.currentTarget.value);
        if (e.key === 'Escape') void finish(layer.name);
      }}
    />
  );
}

interface LayerRowProps {
  entry: LayerEntry;
  renaming: boolean;
  onRenamed: () => void;
  onToggle: () => void;
}

function LayerRow({ entry: { layer, count, hidden, open }, renaming, onRenamed, onToggle }: LayerRowProps) {
  const { activeLayer, setActiveLayer, setLayerVisible, sectionOpen } = useWorkspace();
  const Chevron = open ? ChevronDown : ChevronRight;
  const active = layer.name === activeLayer;
  return (
    <div
      data-slot="tree-row"
      className={cn('flex items-center gap-1.5 pr-2 pl-0.5 text-xs hover:bg-accent', active && 'bg-selection')}
      style={{ height: ROW_HEIGHT }}
      onClick={() => setActiveLayer(layer.name)}
      title="Click to draw new strings on this layer; right-click for more"
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
      <Swatch layer={layer} />
      {renaming ? <RenameBox layer={layer} onDone={onRenamed} /> : <span className={cn('min-w-0 flex-1 truncate', active && 'text-primary')}>{layer.name}</span>}
      {sectionOpen && <LineWeight layer={layer.id} />}
      <span className="text-muted-foreground tabular-nums" title={hidden > 0 ? `${hidden} of ${count} hidden` : undefined}>
        {hidden > 0 ? `${count - hidden} of ${count}` : count}
      </span>
    </div>
  );
}

interface ObjectRowProps {
  object: ObjectSummary;
  /** Ticked along with others by Ctrl or Shift: what a tick or the menu acts on. */
  marked: boolean;
  onClick: (e: MouseEvent) => void;
  onTick: (shown: boolean) => void;
}

function ObjectRow({ object, marked, onClick, onTick }: ObjectRowProps) {
  const { selected } = useWorkspace();
  const Icon = kindIcon[object.kind];
  const note = noteOf(object);
  return (
    <div
      data-slot="tree-row"
      data-state={object.id === selected ? 'selected' : undefined}
      className={cn('flex cursor-pointer items-center gap-1.5 pr-2 pl-9 text-xs hover:bg-accent data-[state=selected]:bg-selection', marked && 'bg-accent')}
      style={{ height: ROW_HEIGHT }}
      onClick={onClick}
    >
      {canHide(object) ? (
        <Checkbox checked={!object.hidden} onClick={(e) => e.stopPropagation()} onCheckedChange={(checked) => onTick(checked === true)} aria-label={`Show ${object.name}`} />
      ) : (
        <span className="size-4 shrink-0" />
      )}
      <span title={kindLabel[object.kind]}>
        <Icon className="size-3.5 shrink-0 text-muted-foreground" />
      </span>
      <span className={cn('min-w-0 flex-1 truncate', object.hidden && 'text-muted-foreground')} title={note ?? object.name}>
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

const plural = (count: number) => `${fmt(count)} object${count === 1 ? '' : 's'}`;

/**
 * Every layer and what is on it, in one tree that draws only the rows on screen. Click a layer to draw
 * new strings on it, or an object to select it; Ctrl and Shift mark several for a tick or the right-click
 * menu to act on together.
 */
export function LayersPanel() {
  const { summary, selected, select, activeLayer, setActiveLayer, setObjectsVisible, run } = useWorkspace();
  const [toggled, setToggled] = useState(new Map<string, boolean>());
  const [marked, setMarked] = useState<ReadonlySet<string>>(new Set());
  const [renaming, setRenaming] = useState<string | null>(null);
  const [target, setTarget] = useState<TreeRow | null>(null);
  /** The last row clicked, where a Shift-click's run starts. */
  const anchor = useRef<number | null>(null);
  const rows = useMemo(() => treeRows(summary?.layers ?? [], summary?.objects ?? [], toggled), [summary, toggled]);
  const { scroller, top, first, last, onScroll } = useRowWindow(rows.length, true);
  const toggle = (entry: LayerEntry) => setToggled((was) => new Map(was).set(entry.layer.id, !entry.open));
  const pinned = layerAbove(rows, Math.floor(top / ROW_HEIGHT));

  // Something picked in the viewport: open its layer, then bring its row into view.
  const revealing = useRef<string | null>(null);
  useEffect(() => {
    const object = selected ? summary?.objects.find((o) => o.id === selected) : undefined;
    if (!object) return;
    revealing.current = object.id;
    setToggled((was) => (rows.some((r) => r.kind === 'layer' && r.layer.id === object.layer && r.open) ? was : new Map(was).set(object.layer, true)));
  }, [selected]);
  useEffect(() => {
    const at = revealing.current ? rows.findIndex((r) => r.kind === 'object' && r.object.id === revealing.current) : -1;
    const element = scroller.current;
    if (at < 0 || !element) return;
    revealing.current = null;
    const y = at * ROW_HEIGHT;
    // The row under the pinned one is covered.
    if (y < element.scrollTop + ROW_HEIGHT || y + ROW_HEIGHT > element.scrollTop + element.clientHeight) element.scrollTo(0, y - element.clientHeight / 2);
  }, [rows, selected]);

  const clickObject = (e: MouseEvent, at: number, id: string) => {
    if (e.shiftKey && anchor.current !== null) {
      const [from, to] = [Math.min(anchor.current, at), Math.max(anchor.current, at)];
      setMarked(new Set(rows.slice(from, to + 1).flatMap((r) => (r.kind === 'object' ? [r.object.id] : []))));
      return;
    }
    anchor.current = at;
    if (e.ctrlKey || e.metaKey) {
      setMarked((was) => {
        const next = new Set(was.size === 0 && selected ? [selected] : was);
        if (!next.delete(id)) next.add(id);
        return next;
      });
      return;
    }
    setMarked(new Set());
    select(id);
  };

  /** What an action on this object takes: everything marked, if it is. */
  const takenWith = (id: string) => {
    if (!marked.has(id)) return [id];
    const live = new Set(summary?.objects.map((o) => o.id));
    return [...marked].filter((other) => live.has(other));
  };

  const rowAt = (e: MouseEvent) => {
    const element = scroller.current;
    if (!element) return null;
    const y = e.clientY - element.getBoundingClientRect().top;
    return pinned && y < ROW_HEIGHT ? pinned.entry : (rows[Math.floor((y + element.scrollTop) / ROW_HEIGHT)] ?? null);
  };

  const newLayer = async () => {
    const taken = new Set(summary?.layers.map((l) => l.name));
    let name = 'new layer';
    for (let k = 2; taken.has(name); k++) name = `new layer ${k}`;
    if (await run('layer.create', { name })) setRenaming(name);
  };

  const layerRow = (entry: LayerEntry, onToggle: () => void, key?: string) => (
    <LayerRow key={key} entry={entry} renaming={renaming === entry.layer.name} onRenamed={() => setRenaming(null)} onToggle={onToggle} />
  );

  const objectMenu = (object: ObjectSummary) => {
    const taken = takenWith(object.id);
    const label = taken.length > 1 ? ` ${plural(taken.length)}` : '';
    const act = (name: string, params: Record<string, unknown>) => {
      setMarked(new Set());
      void run(name, params);
    };
    return (
      <>
        <ContextMenuItem onSelect={() => setObjectsVisible(taken, true)}>Show{label}</ContextMenuItem>
        <ContextMenuItem onSelect={() => setObjectsVisible(taken, false)}>Hide{label}</ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>Move{label} to</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            {summary?.layers
              .filter((l) => l.id !== object.layer || taken.length > 1)
              .map((l) => (
                <ContextMenuItem key={l.id} onSelect={() => act('object.move', { objects: taken, layer: l.name })}>
                  {l.name}
                </ContextMenuItem>
              ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuItem variant="destructive" onSelect={() => act('object.delete_many', { objects: taken })}>
          Delete{label}
        </ContextMenuItem>
        <ContextMenuSeparator />
      </>
    );
  };

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
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div ref={scroller} onScroll={onScroll} onContextMenu={(e) => setTarget(rowAt(e))} data-slot="tree" className="min-h-0 flex-1 overflow-auto select-none">
            {rows.length === 0 && <p className="px-2 py-3 text-xs text-muted-foreground">Nothing here yet: import a surface or draw a string.</p>}
            {pinned && (
              <div className="sticky top-0 z-10 bg-background" style={{ marginBottom: -ROW_HEIGHT }}>
                {layerRow(pinned.entry, () => {
                  toggle(pinned.entry);
                  scroller.current?.scrollTo(0, pinned.at * ROW_HEIGHT);
                })}
              </div>
            )}
            <div style={{ height: first * ROW_HEIGHT }} />
            {rows.slice(first, last).map((row, k) =>
              row.kind === 'layer' ? (
                layerRow(row, () => toggle(row), row.layer.id)
              ) : (
                <ObjectRow
                  key={row.object.id}
                  object={row.object}
                  marked={marked.has(row.object.id)}
                  onClick={(e) => clickObject(e, first + k, row.object.id)}
                  onTick={(shown) => setObjectsVisible(takenWith(row.object.id), shown)}
                />
              ),
            )}
            <div style={{ height: (rows.length - last) * ROW_HEIGHT }} />
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent onCloseAutoFocus={(e) => e.preventDefault()}>
          {target?.kind === 'layer' && (
            <>
              <ContextMenuItem onSelect={() => setRenaming(target.layer.name)}>Rename</ContextMenuItem>
              <ContextMenuItem variant="destructive" onSelect={() => void run('layer.delete', { layer: target.layer.name })}>
                Delete{target.count > 0 ? ` with ${plural(target.count)}` : ''}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          {target?.kind === 'object' && objectMenu(target.object)}
          <ContextMenuItem onSelect={() => void newLayer()}>New layer</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
}
