import { DialogList, KitDialog } from '../design-system/kit';
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
    <KitDialog
      open={meshes !== null}
      onClose={onLeave}
      width={720}
      title={`${n === 1 ? '1 mesh' : `${n.toLocaleString()} meshes`} couldn't be made solids`}
      line="They came in as surfaces, flagged not_a_solid with why. Repair takes fins off, joins cracks, fills holes up to 10 % of the surface and rebuilds crossings to the solid each encloses, checked against the original. Each one fixed replaces its surface, keeping its name, layer and attributes; undo brings the surfaces back."
      foot={
        <>
          <Button variant="ghost" size="sm" onClick={onLeave}>
            Leave as surfaces
          </Button>
          <Button size="sm" autoFocus onClick={() => onFix((meshes ?? []).map((m) => m.id))}>
            Fix {n === 1 ? 'it' : 'them'}
          </Button>
        </>
      }
    >
      <ul className="list-disc pl-5">
        {[...counts].map(([what, count]) => (
          <li key={what}>
            {count.toLocaleString()} {what}
          </li>
        ))}
      </ul>
      <DialogList>
        {(meshes ?? []).slice(0, LISTED).map((m) => (
          <div key={m.id} className="truncate px-2 py-0.5" title={m.why}>
            {m.name}: <span className="text-muted-foreground">{m.why}</span>
          </div>
        ))}
        {n > LISTED && <div className="px-2 py-0.5 text-muted-foreground">and {(n - LISTED).toLocaleString()} more</div>}
      </DialogList>
    </KitDialog>
  );
}
