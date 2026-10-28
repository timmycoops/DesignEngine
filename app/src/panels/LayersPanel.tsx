import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { ScrollArea } from '@/design-system/ui/scroll-area';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';

/** Layers with visibility, colour and object counts; click a layer to draw new strings on it. */
export function LayersPanel() {
  const { summary, activeLayer, setActiveLayer, setLayerVisible } = useWorkspace();
  const counts = new Map<string, number>();
  for (const o of summary?.objects ?? []) counts.set(o.layer, (counts.get(o.layer) ?? 0) + 1);
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
      <ScrollArea className="min-h-0 flex-1">
        {summary?.layers.length === 0 && <p className="px-2 py-3 text-xs text-muted-foreground">No layers yet.</p>}
        {summary?.layers.map((layer) => (
          <div
            key={layer.id}
            className={cn('flex items-center gap-2 px-2 py-1 hover:bg-accent', layer.name === activeLayer && 'bg-selection')}
            onClick={() => setActiveLayer(layer.name)}
            title="Click to draw new strings on this layer"
          >
            <Checkbox
              checked={layer.visible}
              onClick={(e) => e.stopPropagation()}
              onCheckedChange={(checked) => setLayerVisible(layer.id, checked === true)}
              aria-label={`Show ${layer.name}`}
            />
            <span className="size-2.5 shrink-0 rounded-sm" style={{ background: `rgb(${layer.colour.join(',')})` }} />
            <span className={cn('min-w-0 flex-1 truncate', layer.name === activeLayer && 'text-primary')}>{layer.name}</span>
            <span className="text-xs text-muted-foreground tabular-nums">{counts.get(layer.id) ?? 0}</span>
          </div>
        ))}
      </ScrollArea>
    </div>
  );
}
