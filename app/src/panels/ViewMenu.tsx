import { useMemo } from 'react';
import { type Colouring, lookOf } from '@/bridge';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/design-system/ui/dropdown-menu';
import { useWorkspace } from '@/workspace/context';
import { canColour, canSeeThrough, fmt } from './format';
import { IN_TREE } from './layerTree';

/** The opacities the menu offers; Properties has a slider for the rest. */
const OPACITIES = [1, 0.75, 0.5, 0.25];
const percent = (opacity: number) => `${Math.round(opacity * 100)}%`;

/**
 * The view's right-click menu (Filtering plan, L2), where an object is shown, hidden and acted on now the tree has a row
 * a layer and none an object. A right click in Select opens it on what's picked, the thing under the cursor picked first
 * if it wasn't: zoom to it, hide it, show only it (the rest of its layers hidden), colour and opacity, move it to a layer
 * or delete it. Show all hidden brings back everything hidden on the layers showing. Hidden is display state, saved and
 * not undone; a move or a delete is a run, one undo.
 */
export function ViewMenu() {
  const { summary, viewMenu, closeViewMenu, zoomTo, setObjectsVisible, setLooks, setSelection, run } = useWorkspace();
  const ids = useMemo(() => new Set(viewMenu?.ids ?? []), [viewMenu]);
  if (!viewMenu || !summary) return null;
  const objects = summary.objects.filter((o) => ids.has(o.id));
  const taken = objects.map((o) => o.id);
  const layersOf = new Set(objects.map((o) => o.layer));
  const others = summary.objects.filter((o) => IN_TREE(o) && layersOf.has(o.layer) && !ids.has(o.id) && !o.hidden).map((o) => o.id);
  const visible = new Set(summary.layers.filter((l) => l.visible).map((l) => l.id));
  const hidden = summary.objects.filter((o) => IN_TREE(o) && o.hidden && visible.has(o.layer)).map((o) => o.id);
  const label = objects.length === 1 ? objects[0].name : `${fmt(objects.length)} objects`;
  const coloured = objects.filter(canColour);
  const colourings = new Set(coloured.map((o) => lookOf(summary.display, o.id).colouring));
  const looking = objects.filter(canSeeThrough);
  const opacities = new Set(looking.map((o) => lookOf(summary.display, o.id).opacity));
  const colour = (colouring: Colouring) => setLooks(coloured.map((o) => o.id), colouring, null);
  const tick = (on: boolean) => <span className="w-3 text-primary">{on ? '✓' : ''}</span>;
  return (
    <DropdownMenu key={viewMenu.n} open onOpenChange={(open) => !open && closeViewMenu()}>
      <DropdownMenuTrigger asChild>
        <span aria-hidden className="pointer-events-none fixed size-px" style={{ left: viewMenu.x, top: viewMenu.y }} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side="bottom" sideOffset={2} className="min-w-48" data-slot="view-menu" onCloseAutoFocus={(e) => e.preventDefault()}>
        {objects.length > 0 && (
          <>
            <div className="truncate px-2 py-1 text-xs text-muted-foreground">{label}</div>
            <DropdownMenuItem onSelect={() => zoomTo(taken)}>Zoom to</DropdownMenuItem>
            <DropdownMenuItem onSelect={() => (setObjectsVisible(taken, false), setSelection([]))}>Hide</DropdownMenuItem>
            <DropdownMenuItem disabled={others.length === 0} onSelect={() => setObjectsVisible(others, false)}>
              Show only {objects.length === 1 ? 'this' : 'these'} on {layersOf.size === 1 ? 'its layer' : 'their layers'}
            </DropdownMenuItem>
            {coloured.length > 0 && (
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Colour</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  <DropdownMenuItem onSelect={() => colour('by_rl')}>
                    {tick(colourings.size === 1 && colourings.has('by_rl'))}By RL
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => colour('layer')}>
                    {tick(colourings.size === 1 && colourings.has('layer'))}Layer colour
                  </DropdownMenuItem>
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            )}
            {looking.length > 0 && (
              <DropdownMenuSub>
                <DropdownMenuSubTrigger>Opacity</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  {OPACITIES.map((o) => (
                    <DropdownMenuItem key={o} onSelect={() => setLooks(looking.map((x) => x.id), null, o)}>
                      {tick(opacities.size === 1 && opacities.has(o))}
                      {percent(o)}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            )}
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>Move to layer</DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="max-h-80 overflow-auto">
                {summary.layers
                  .filter((l) => layersOf.size > 1 || !layersOf.has(l.id))
                  .map((l) => (
                    <DropdownMenuItem key={l.id} onSelect={() => void run('object.move', { objects: taken, layer: l.name })}>
                      {l.name}
                    </DropdownMenuItem>
                  ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuItem variant="destructive" onSelect={() => void run('object.delete_many', { objects: taken })}>
              {objects.length === 1 ? 'Delete' : `Delete ${fmt(objects.length)}`}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
          </>
        )}
        <DropdownMenuItem disabled={hidden.length === 0} onSelect={() => setObjectsVisible(hidden, true)}>
          Show all hidden{hidden.length > 0 ? ` (${fmt(hidden.length)})` : ''}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
