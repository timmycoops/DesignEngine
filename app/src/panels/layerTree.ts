/**
 * The layer tree's rows (Filtering plan, L1 and L4): a row a layer, what's on it counted by kind, never a row an object.
 * Kept free of React, so the tests can run it.
 */
import type { Layer, ObjectKind, ObjectSummary } from '@/bridge';
import { fmt } from './format.ts';

/**
 * What isn't drawn has no place in the tree (Filtering plan, L4): a face design, workflow, parameter table, dataflow or
 * reserves table is found in the list its own tab or panel has, with Rename and Delete there.
 */
export const IN_TREE = (o: ObjectSummary) => !['face_design', 'workflow', 'parameter_table', 'dataflow', 'table'].includes(o.kind);

/** What a layer holds, by kind, in this order: the words for one and for several. */
const KINDS: { kind: ObjectKind; drawn?: boolean; one: string; many: string }[] = [
  { kind: 'string', one: 'string', many: 'strings' },
  { kind: 'surface', one: 'surface', many: 'surfaces' },
  { kind: 'grid', drawn: true, one: 'grid', many: 'grids' },
  { kind: 'grid', drawn: false, one: 'quality grid', many: 'quality grids' },
  { kind: 'solid', one: 'solid', many: 'solids' },
  { kind: 'seam_model', one: 'seam model', many: 'seam models' },
];

/** A layer's objects by kind in words: `60 strings · 12 surfaces`. */
export function kindsOf(objects: readonly ObjectSummary[]): string {
  return KINDS.flatMap(({ kind, drawn, one, many }) => {
    const n = objects.filter((o) => o.kind === kind && (drawn === undefined || o.drawn === drawn)).length;
    return n === 0 ? [] : [`${fmt(n)} ${n === 1 ? one : many}`];
  }).join(' · ');
}

/** A row a layer (Filtering plan, L1): what's on it counted by kind, how many show, and how many of them are picked. */
export interface LayerEntry {
  layer: Layer;
  /** What's on it that the tree speaks for: everything drawn, and seam models. */
  objects: ObjectSummary[];
  hidden: number;
  picked: number;
  /** Everything on it, things not in the tree too: what deleting it takes. */
  all: number;
}

/**
 * Every layer in the project's order, but those holding only what isn't in the tree (a layer of face designs, say). A
 * layer with nothing on it yet shows, as it's where the next string is drawn.
 */
export function layerEntries(layers: readonly Layer[], objects: readonly ObjectSummary[], selection: ReadonlySet<string>): LayerEntry[] {
  const held = new Map<string, ObjectSummary[]>();
  for (const object of objects) {
    const on = held.get(object.layer);
    if (on) on.push(object);
    else held.set(object.layer, [object]);
  }
  return layers.flatMap((layer) => {
    const all = held.get(layer.id) ?? [];
    const shown = all.filter(IN_TREE);
    if (all.length > 0 && shown.length === 0) return [];
    return [{ layer, objects: shown, hidden: shown.filter((o) => o.hidden).length, picked: shown.filter((o) => selection.has(o.id)).length, all: all.length }];
  });
}

