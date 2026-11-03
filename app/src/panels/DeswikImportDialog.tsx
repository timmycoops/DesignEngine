import { useMemo, useState } from 'react';
import { Dialog } from 'radix-ui';
import { Button } from '../design-system/ui/button';
import { Checkbox } from '../design-system/ui/checkbox';
import { Input } from '../design-system/ui/input';

export type MeshesAs = 'auto' | 'solids' | 'surfaces' | 'none';

export interface DeswikChoice {
  layers: string[];
  meshes: MeshesAs;
  repair: boolean;
  /** One layer for everything; blank keeps each object on a layer named after its Deswik layer. */
  layer: string;
}

/** A layer to pick: its name (levels joined by `\`), its size, and what it holds where that is known. */
export interface PickableLayer {
  name: string;
  bytes: number;
  detail?: string;
}

/** What the dialog says about the file it lists. */
export interface ImportSource {
  /** Deswik, Spry. */
  from: string;
  description: string;
  /** The filter box's hint. */
  placeholder: string;
}

const DESWIK: ImportSource = {
  from: 'Deswik',
  description:
    'Pick the layers to read. Strings keep their attributes and projection details; a closed mesh becomes a solid and an open one a surface, each on a layer named as in Deswik.',
  placeholder: 'Filter layers: words in any order, e.g. design strip 12',
};

/** A size in bytes, as kB or MB. */
const size = (bytes: number) => (bytes < 1e6 ? `${Math.max(1, Math.round(bytes / 1e3))} kB` : `${(bytes / 1e6).toFixed(1)} MB`);

/**
 * Picks the layers of a Deswik drawing (decision 0036) or the Solid fields and layers of a Spry
 * project (decision 0039) to import. The list comes without reading any geometry, so even a file of
 * gigabytes opens at once; for Deswik the sizes are of the XML unzipped, and a layer of a few tens
 * of kB holds little or nothing.
 */
export function DeswikImportDialog({
  file,
  source = DESWIK,
  onCancel,
  onImport,
}: {
  file: { path: string; layers: PickableLayer[] } | null;
  source?: ImportSource;
  onCancel: () => void;
  onImport: (choice: DeswikChoice) => void;
}) {
  const [filter, setFilter] = useState('');
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [meshes, setMeshes] = useState<MeshesAs>('auto');
  const [repair, setRepair] = useState(false);
  const [layer, setLayer] = useState('');

  const shown = useMemo(() => {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    return (file?.layers ?? []).filter((l) => words.every((w) => l.name.toLowerCase().includes(w)));
  }, [file, filter]);
  const picked = (file?.layers ?? []).filter((l) => chosen.has(l.name));
  const toggle = (name: string, on: boolean) =>
    setChosen((was) => {
      const next = new Set(was);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });
  const choose = (names: string[], on: boolean) =>
    setChosen((was) => {
      const next = new Set(was);
      for (const name of names) {
        if (on) next.add(name);
        else next.delete(name);
      }
      return next;
    });
  const close = () => {
    setChosen(new Set());
    setFilter('');
    onCancel();
  };
  const fileName = file?.path.split(/[\\/]/).pop() ?? '';

  return (
    <Dialog.Root open={file !== null} onOpenChange={(open) => !open && close()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[min(960px,92vw)] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-lg border bg-card p-4 text-sm shadow-lg">
          <div>
            <Dialog.Title className="font-semibold">
              Import from {source.from}: {fileName}
            </Dialog.Title>
            <Dialog.Description className="text-xs text-muted-foreground">{source.description}</Dialog.Description>
          </div>
          <div className="flex items-center gap-2">
            <Input placeholder={source.placeholder} value={filter} onChange={(e) => setFilter(e.target.value)} autoFocus />
            <Button variant="outline" size="sm" onClick={() => choose(shown.map((l) => l.name), true)}>
              Tick shown
            </Button>
            <Button variant="outline" size="sm" onClick={() => choose(shown.map((l) => l.name), false)}>
              Untick shown
            </Button>
          </div>
          <div className="min-h-0 flex-1 overflow-auto rounded border">
            {shown.map((l) => {
              const cut = l.name.lastIndexOf('\\');
              return (
                <label key={l.name} className="flex cursor-pointer items-center gap-2 px-2 py-0.5 hover:bg-muted/50">
                  <Checkbox checked={chosen.has(l.name)} onCheckedChange={(on) => toggle(l.name, on === true)} />
                  <span className="min-w-0 flex-1 truncate" title={l.name}>
                    <span className="text-muted-foreground">{l.name.slice(0, cut + 1)}</span>
                    {l.name.slice(cut + 1)}
                  </span>
                  {l.detail && <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{l.detail}</span>}
                  <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{size(l.bytes)}</span>
                </label>
              );
            })}
            {shown.length === 0 && <div className="p-2 text-muted-foreground">No layer matches.</div>}
          </div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <label className="flex items-center gap-1.5">
              Meshes
              <select className="h-7 rounded border bg-transparent px-1" value={meshes} onChange={(e) => setMeshes(e.target.value as MeshesAs)}>
                <option value="auto">closed ones solids, open ones surfaces</option>
                <option value="solids">all solids</option>
                <option value="surfaces">all surfaces</option>
                <option value="none">none: strings only</option>
              </select>
            </label>
            <label className="flex items-center gap-1.5" title="Sheets and fins off, cracks joined, holes up to 10 % filled">
              <Checkbox checked={repair} onCheckedChange={(on) => setRepair(on === true)} />
              Repair solids
            </label>
            <label className="flex items-center gap-1.5">
              One layer for all
              <Input className="h-7 w-48" placeholder={`blank: as in ${source.from}`} value={layer} onChange={(e) => setLayer(e.target.value)} />
            </label>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs text-muted-foreground">
              {picked.length} of {file?.layers.length ?? 0} layers ticked, {size(picked.reduce((sum, l) => sum + l.bytes, 0))}
            </span>
            <div className="flex gap-2">
              <Button variant="ghost" size="sm" onClick={close}>
                Cancel
              </Button>
              <Button
                size="sm"
                disabled={picked.length === 0}
                onClick={() => {
                  onImport({ layers: picked.map((l) => l.name), meshes, repair, layer: layer.trim() });
                  setChosen(new Set());
                  setFilter('');
                }}
              >
                Import {picked.length === 1 ? '1 layer' : `${picked.length} layers`}
              </Button>
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
