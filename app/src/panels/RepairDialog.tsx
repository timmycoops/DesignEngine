import { Dialog } from 'radix-ui';
import { Button } from '../design-system/ui/button';

/** A mesh an import kept as a surface, closed or a block with holes, and why it couldn't be a solid. */
export interface NotASolid {
  id: string;
  name: string;
  why: string;
}

/** Names shown one by one, the rest counted. */
const LISTED = 8;

/** Why, in a few words, to count the meshes by. */
const reason = (why: string) =>
  why.startsWith('open:')
    ? 'are open, with holes'
    : /crosses itself|cross each other/.test(why)
    ? 'cross themselves'
    : /non-manifold/.test(why)
      ? 'have edges of three or more faces'
      : /holes/.test(why)
        ? 'have holes'
        : 'other reasons';

/**
 * After a Deswik or Spry import, the closed meshes and open Spry blocks it couldn't make solids (decision 0040), with the
 * offer to fix them by repair. Fixing runs `solid.repair` on these alone, each fixed one replacing
 * its surface; what can't be fixed stays a surface saying why.
 */
export function RepairDialog({ meshes, onLeave, onFix }: { meshes: NotASolid[] | null; onLeave: () => void; onFix: (ids: string[]) => void }) {
  const counts = new Map<string, number>();
  for (const m of meshes ?? []) counts.set(reason(m.why), (counts.get(reason(m.why)) ?? 0) + 1);
  const n = meshes?.length ?? 0;

  return (
    <Dialog.Root open={meshes !== null} onOpenChange={(open) => !open && onLeave()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <Dialog.Content className="fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[min(720px,92vw)] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-lg border bg-card p-4 text-sm shadow-lg">
          <div>
            <Dialog.Title className="font-semibold">
              {n === 1 ? '1 mesh' : `${n.toLocaleString()} meshes`} couldn't be made solids
            </Dialog.Title>
            <Dialog.Description className="text-xs text-muted-foreground">
              They came in as surfaces, flagged not_a_solid with why. Repair takes fins off, joins cracks, fills holes up to 10 % of the surface and rebuilds
              crossings to the solid each encloses, checked against the original. Each one fixed replaces its surface, keeping its name, layer and attributes;
              undo brings the surfaces back.
            </Dialog.Description>
          </div>
          <ul className="list-disc pl-5">
            {[...counts].map(([what, count]) => (
              <li key={what}>
                {count.toLocaleString()} {what}
              </li>
            ))}
          </ul>
          <div className="min-h-0 flex-1 overflow-auto rounded border text-xs">
            {(meshes ?? []).slice(0, LISTED).map((m) => (
              <div key={m.id} className="truncate px-2 py-0.5" title={m.why}>
                {m.name}: <span className="text-muted-foreground">{m.why}</span>
              </div>
            ))}
            {n > LISTED && <div className="px-2 py-0.5 text-muted-foreground">and {(n - LISTED).toLocaleString()} more</div>}
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={onLeave}>
              Leave as surfaces
            </Button>
            <Button size="sm" autoFocus onClick={() => onFix((meshes ?? []).map((m) => m.id))}>
              Fix {n === 1 ? 'it' : 'them'}
            </Button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
