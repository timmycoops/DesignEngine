/**
 * What a tool works on (decision 0108, decision 6): the solids picked, every solid on the first picked one's layer, or
 * every solid on show. One choice for every tool that reads it, set at the foot of the view, never a choice of a
 * panel's own. Kept free of React, so the tests can read it.
 */

export type WorksOn = 'picked' | 'layer' | 'visible';

export const WORKS_ON: readonly WorksOn[] = ['picked', 'layer', 'visible'];

interface Shown {
  id: string;
  kind: string;
  layer: string;
  name: string;
}

/** The solids each choice takes, the ones the choice in use takes, and each choice's words with its count. */
export function solidsWorkedOn<O extends Shown>(
  objects: readonly O[],
  layers: readonly { id: string; visible: boolean }[],
  selection: readonly string[],
  worksOn: WorksOn,
  concealed: (o: O) => boolean,
): { picked: O[]; onLayer: O[]; visible: O[]; chosen: O[]; labels: Record<WorksOn, string> } {
  const solids = objects.filter((o) => o.kind === 'solid');
  const pickedIds = new Set(selection);
  const picked = solids.filter((o) => pickedIds.has(o.id));
  const onLayer = picked.length > 0 ? solids.filter((o) => o.layer === picked[0].layer) : [];
  const shown = new Set(layers.filter((l) => l.visible).map((l) => l.id));
  const visible = solids.filter((o) => shown.has(o.layer) && !concealed(o));
  const chosen = worksOn === 'picked' ? picked : worksOn === 'layer' ? onLayer : visible;
  const labels: Record<WorksOn, string> = { picked: `Picked (${picked.length})`, layer: `Its layer (${onLayer.length})`, visible: `On show (${visible.length})` };
  return { picked, onLayer, visible, chosen, labels };
}

/** The panels whose tools read the choice: the bar at the foot of the view shows while one of them is in front. */
export const READ_BY: readonly string[] = ['cut', 'reserves'];
