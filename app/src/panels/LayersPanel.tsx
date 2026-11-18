import { type MouseEvent, useEffect, useMemo, useRef, useState } from 'react';
import { type Colouring, type Layer, layerOpacityOf, lookOf } from '@/bridge';
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
import { canColour, fmt } from './format';
import { kindsOf, type LayerEntry, layerEntries } from './layerTree';

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
        if (e.key === 'Escape') {
          // The rename put back is this press: it doesn't go on to clear the selection (decision 0080).
          e.preventDefault();
          void finish(layer.name);
        }
      }}
    />
  );
}

interface LayerRowProps {
  entry: LayerEntry;
  renaming: boolean;
  onRenamed: () => void;
}

function LayerRow({ entry: { layer, objects, hidden, picked }, renaming, onRenamed }: LayerRowProps) {
  const { summary, activeLayer, setActiveLayer, setLayerVisible, setObjectsVisible, sectionOpen } = useWorkspace();
  const active = layer.name === activeLayer;
  const count = objects.length;
  const kinds = kindsOf(objects);
  // The tick says what is on screen: all of the layer, none of it, or some.
  const tick = !layer.visible || (count > 0 && hidden === count) ? false : hidden === 0 ? true : 'indeterminate';
  const tickLayer = (on: boolean) => {
    // Turned off and on, a layer comes back as it was. With nothing to come back to, or some showing, it all shows.
    if (!on || (!layer.visible && (count === 0 || hidden < count))) setLayerVisible(layer.id, on);
    else setObjectsVisible(objects.map((o) => o.id), true);
  };
  return (
    <div
      data-slot="tree-row"
      data-layer={layer.name}
      data-state={picked > 0 ? 'selected' : undefined}
      className={cn('flex items-center gap-1.5 pr-2 pl-2 text-xs hover:bg-accent', active && 'bg-selection')}
      style={{ height: ROW_HEIGHT }}
      onClick={() => setActiveLayer(layer.name)}
      title={`${layer.name}${kinds ? `: ${kinds}` : ''}. Click to draw new strings on this layer; right-click for more`}
    >
      <Checkbox checked={tick} onClick={(e) => e.stopPropagation()} onCheckedChange={(checked) => tickLayer(checked === true)} aria-label={`Show ${layer.name}`} />
      <Swatch layer={layer} />
      {renaming ? <RenameBox layer={layer} onDone={onRenamed} /> : <span className={cn('min-w-0 shrink-0 truncate', active && 'text-primary')} style={{ maxWidth: '55%' }}>{layer.name}</span>}
      {picked > 0 && (
        <span className="shrink-0 text-primary tabular-nums" title={`${fmt(picked)} of what's on it picked`}>
          {fmt(picked)} picked
        </span>
      )}
      <span className="min-w-0 flex-1 truncate text-right text-muted-foreground tabular-nums" data-slot="layer-kinds">
        {count === 0 ? 'empty' : kinds}
      </span>
      {layer.visible && hidden > 0 && (
        <span className="shrink-0 text-muted-foreground tabular-nums" title={`${fmt(hidden)} of ${fmt(count)} hidden: right-click for Show all on it`}>
          {fmt(count - hidden)} of {fmt(count)}
        </span>
      )}
      {sectionOpen && <LineWeight layer={layer.id} />}
      <OpacityNote opacity={layerOpacityOf(summary?.display, layer.id)} title="Layer opacity: right-click to change it" />
    </div>
  );
}

const plural = (count: number) => `${fmt(count)} object${count === 1 ? '' : 's'}`;

/** The opacities the menus offer; Properties has a slider for the rest. */
const OPACITIES = [1, 0.75, 0.5, 0.25];
const percent = (opacity: number) => `${Math.round(opacity * 100)}%`;

/** Coloured by RL or its layer: surfaces and elevation grids. A seam horizon keeps its thickness colours. */

/** An opacity under 100 %, beside a row's name. */
function OpacityNote({ opacity, title }: { opacity: number; title: string }) {
  if (opacity >= 1) return null;
  return (
    <span className="text-muted-foreground tabular-nums" title={title}>
      {percent(opacity)}
    </span>
  );
}

/**
 * Every layer, one row each (Filtering plan, L1): what's on it by kind, never a row an object. Click a layer to draw new
 * strings on it; right-click it to zoom to it, select or show all on it, colour it, rename or delete it. An object is
 * picked, hidden or shown in the view, with its right-click; a pick lights its layer's row here (L3).
 */
export function LayersPanel() {
  const { summary, selection, setSelection, zoomTo, setObjectsVisible, setLooks, setLayerOpacity, run } = useWorkspace();
  const picked = useMemo(() => new Set(selection), [selection]);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [target, setTarget] = useState<LayerEntry | null>(null);
  const rows = useMemo(() => layerEntries(summary?.layers ?? [], summary?.objects ?? [], picked), [summary, picked]);
  const { scroller, first, last, onScroll } = useRowWindow(rows.length, true);

  // Something picked in the view: its layer's row comes into view, lit.
  const lastPicked = selection.at(-1) ?? null;
  useEffect(() => {
    const layer = lastPicked ? summary?.objects.find((o) => o.id === lastPicked)?.layer : undefined;
    const at = layer ? rows.findIndex((r) => r.layer.id === layer) : -1;
    const element = scroller.current;
    if (at < 0 || !element) return;
    const y = at * ROW_HEIGHT;
    if (y < element.scrollTop || y + ROW_HEIGHT > element.scrollTop + element.clientHeight) element.scrollTo(0, y - element.clientHeight / 2);
    // Only a new pick moves the tree, never a change to what's on the layers.
  }, [lastPicked]);

  /** What's on a layer and on show, or everything on it if nothing is. */
  const layerShowing = (entry: LayerEntry) => {
    const showing = entry.objects.filter((o) => !o.hidden);
    return (showing.length > 0 ? showing : entry.objects).map((o) => o.id);
  };

  const rowAt = (e: MouseEvent) => {
    const element = scroller.current;
    if (!element) return null;
    const y = e.clientY - element.getBoundingClientRect().top;
    return rows[Math.floor((y + element.scrollTop) / ROW_HEIGHT)] ?? null;
  };

  const newLayer = async () => {
    const taken = new Set(summary?.layers.map((l) => l.name));
    let name = 'new layer';
    for (let k = 2; taken.has(name); k++) name = `new layer ${k}`;
    if (await run('layer.create', { name })) setRenaming(name);
  };

  /** Colour by RL or the layer's for what can take it, and the layer's opacity. A tick shows the value they all share. */
  const lookMenu = (entry: LayerEntry) => {
    const coloured = entry.objects.filter(canColour);
    const colourings = new Set(coloured.map((o) => lookOf(summary?.display, o.id).colouring));
    const colour = (colouring: Colouring) => setLooks(coloured.map((o) => o.id), colouring, null);
    const opacity = layerOpacityOf(summary?.display, entry.layer.id);
    const tick = (on: boolean) => <span className="w-3 text-primary">{on ? '✓' : ''}</span>;
    return (
      <>
        {coloured.length > 0 && (
          <ContextMenuSub>
            <ContextMenuSubTrigger>Colour</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem onSelect={() => colour('by_rl')}>
                {tick(colourings.size === 1 && colourings.has('by_rl'))}By RL
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => colour('layer')}>
                {tick(colourings.size === 1 && colourings.has('layer'))}Layer colour
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
        )}
        <ContextMenuSub>
          <ContextMenuSubTrigger>Opacity</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            {OPACITIES.map((o) => (
              <ContextMenuItem key={o} onSelect={() => setLayerOpacity(entry.layer.id, o)}>
                {tick(opacity === o)}
                {percent(o)}
              </ContextMenuItem>
            ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
      </>
    );
  };

  return (
    <div className="flex h-full flex-col">
      <DrawOn />
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div ref={scroller} onScroll={onScroll} onContextMenu={(e) => setTarget(rowAt(e))} data-slot="tree" className="min-h-0 flex-1 overflow-auto select-none">
            {rows.length === 0 && <p className="px-2 py-3 text-xs text-muted-foreground">Nothing here yet: import a surface or draw a string.</p>}
            <div style={{ height: first * ROW_HEIGHT }} />
            {rows.slice(first, last).map((entry) => (
              <LayerRow key={entry.layer.id} entry={entry} renaming={renaming === entry.layer.name} onRenamed={() => setRenaming(null)} />
            ))}
            <div style={{ height: (rows.length - last) * ROW_HEIGHT }} />
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent onCloseAutoFocus={(e) => e.preventDefault()}>
          {target && (
            <>
              <ContextMenuItem disabled={target.objects.length === 0} onSelect={() => zoomTo(layerShowing(target))}>
                Zoom to
              </ContextMenuItem>
              <ContextMenuItem disabled={target.objects.length === 0} onSelect={() => setSelection(layerShowing(target))}>
                Select all on it
              </ContextMenuItem>
              {target.hidden > 0 && (
                <ContextMenuItem onSelect={() => setObjectsVisible(target.objects.filter((o) => o.hidden).map((o) => o.id), true)}>
                  Show all on it ({fmt(target.hidden)} hidden)
                </ContextMenuItem>
              )}
              {lookMenu(target)}
              <ContextMenuItem onSelect={() => setRenaming(target.layer.name)}>Rename</ContextMenuItem>
              <ContextMenuItem variant="destructive" onSelect={() => void run('layer.delete', { layer: target.layer.name })}>
                Delete{target.all > 0 ? ` with ${plural(target.all)}` : ''}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          <ContextMenuItem onSelect={() => void newLayer()}>New layer</ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    </div>
  );
}

/** The layer new strings are drawn on; it is created with the first string. */
function DrawOn() {
  const { activeLayer, setActiveLayer } = useWorkspace();
  return (
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
  );
}
