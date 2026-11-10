import { useEffect, useMemo, useState } from 'react';
import { Dialog } from 'radix-ui';
import type { ObjectSummary } from '../bridge';
import { Choice, Select } from '../design-system/kit';
import { Button } from '../design-system/ui/button';
import { cn } from 'cn';

/** How the solids' files are named and what layer each is on in its file (`solid.export_many`, decision 0057). */
export interface SolidExportOptions {
  name_from: string[];
  separator: string;
  layer_in_file: 'id' | 'name';
}

const SEPARATORS = ['.', '_', '-', ' '] as const;
const SEPARATOR_WORDS: Record<string, string> = { '.': 'a dot', _: 'an underscore', '-': 'a dash', ' ': 'a space' };

/** A file name's stem, as the engine makes it: what Windows refuses becomes an underscore, and dots and spaces at the end go. */
const stem = (name: string) =>
  name
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_')
    .trim()
    .replace(/[. ]+$/, '') || 'solid';

/**
 * Solids out to a zip, a DXF a solid (decision 0057): the files take the solids' names, or names made from attributes
 * picked here in order, joined Spry's way, so another package can take the attributes back out of the name.
 */
export function ExportSolidsDialog({
  solids,
  layerName,
  onCancel,
  onExport,
}: {
  solids: ObjectSummary[] | null;
  layerName: (solid: ObjectSummary) => string;
  onCancel: () => void;
  onExport: (options: SolidExportOptions) => void;
}) {
  const [picked, setPicked] = useState<string[]>([]);
  const [separator, setSeparator] = useState<string>('.');
  const [layerInFile, setLayerInFile] = useState<'id' | 'name'>('id');
  // Each time it opens on other solids, the attributes on offer change: start from the solids' own names.
  useEffect(() => setPicked([]), [solids]);

  const keys = useMemo(() => {
    const found = new Set<string>();
    for (const solid of solids ?? []) for (const key of Object.keys(solid.attributes)) found.add(key);
    return ['name', 'layer', ...[...found].filter((key) => key !== 'name' && key !== 'layer').sort()];
  }, [solids]);

  const n = solids?.length ?? 0;
  const part = (solid: ObjectSummary, key: string) => {
    // Each part goes in as it is: a name from Spry is in parts already.
    return key in solid.attributes ? String(solid.attributes[key]) : key === 'name' ? solid.name : key === 'layer' ? layerName(solid) : '';
  };
  const fileOf = (solid: ObjectSummary) => `${stem(picked.length === 0 ? solid.name : picked.map((key) => part(solid, key)).join(separator))}.dxf`;
  const lacking = picked.length === 0 ? 0 : (solids ?? []).filter((solid) => picked.some((key) => key !== 'name' && key !== 'layer' && !(key in solid.attributes))).length;

  return (
    <Dialog.Root open={solids !== null} onOpenChange={(open) => !open && onCancel()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[min(640px,92vw)] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-lg border bg-card p-4 text-sm shadow-lg">
          <div>
            <Dialog.Title className="font-semibold">Export {n === 1 ? '1 solid' : `${n.toLocaleString()} solids`} to a zip</Dialog.Title>
            <Dialog.Description className="text-xs text-muted-foreground">
              One DXF a solid, with solids.csv listing each solid's number, file, volume and attributes. Deswik imports the zip and puts each file's solid on
              a layer named for the file.
            </Dialog.Description>
          </div>

          <div className="flex flex-col gap-1">
            <div className="text-xs font-medium">Name each file from</div>
            <div className="flex flex-wrap gap-1">
              {keys.map((key) => {
                const at = picked.indexOf(key);
                return (
                  <button
                    key={key}
                    type="button"
                    aria-pressed={at >= 0}
                    title={at >= 0 ? `Part ${at + 1} of the name; click to take it out` : 'Click to add it to the end of the name'}
                    className={cn('h-7 rounded-md border border-input px-2 text-xs transition-colors', at >= 0 ? 'bg-selection text-primary' : 'text-muted-foreground hover:text-foreground')}
                    onClick={() => setPicked(at >= 0 ? picked.filter((k) => k !== key) : [...picked, key])}
                  >
                    {at >= 0 ? `${at + 1} · ` : ''}
                    {key}
                  </button>
                );
              })}
            </div>
            <div className="text-xs text-muted-foreground">
              {picked.length === 0
                ? "Nothing picked: each file takes its solid's name. Pick attributes in the order they go in the name, as Spry names its solids."
                : `In this order, with ${SEPARATOR_WORDS[separator]} between.`}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs">
            <label className="flex items-center gap-2">
              Between the parts
              <Select value={separator} onChange={(event) => setSeparator(event.target.value)} disabled={picked.length === 0} aria-label="What goes between the parts of a name">
                {SEPARATORS.map((s) => (
                  <option key={s} value={s}>
                    {SEPARATOR_WORDS[s]}
                  </option>
                ))}
              </Select>
            </label>
            <span className="flex items-center gap-2">
              Layer in each file
              <Choice value={layerInFile} options={['id', 'name'] as const} labels={{ id: 'Its number', name: "Its file's name" }} onChange={setLayerInFile} label="The layer each solid is on in its file, which reaches Deswik on the solid" />
            </span>
          </div>

          <div className="min-h-0 flex-1 overflow-auto rounded border text-xs">
            {(solids ?? []).slice(0, 6).map((solid, k) => (
              <div key={solid.id} className="truncate px-2 py-0.5" title={solid.name}>
                {k + 1}. {fileOf(solid)}
              </div>
            ))}
            {n > 6 && <div className="px-2 py-0.5 text-muted-foreground">and {(n - 6).toLocaleString()} more</div>}
          </div>
          {lacking > 0 && (
            <div className="text-xs text-warning">
              {lacking.toLocaleString()} of them lack one of those attributes: that part of the file's name will be empty.
            </div>
          )}

          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={onCancel}>
              Cancel
            </Button>
            <Button size="sm" autoFocus disabled={n === 0} onClick={() => onExport({ name_from: picked, separator, layer_in_file: layerInFile })}>
              Export to a zip…
            </Button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
