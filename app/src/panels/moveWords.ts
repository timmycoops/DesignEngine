/**
 * Move and copy's words and sums (String tools plan, workstream 3), kept free of React so the tests can read them.
 */

type Vec3 = readonly [number, number, number];

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

/** How many of each kind, in words: "3 strings and 2 solids". */
export function countedWords(objects: readonly { kind: string }[]): string {
  const parts = ['string', 'surface', 'solid', 'grid'].flatMap((kind) => {
    const n = objects.filter((o) => o.kind === kind).length;
    return n > 0 ? [plural(n, kind)] : [];
  });
  return parts.length === 0 ? 'nothing' : parts.length === 1 ? parts[0] : `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}`;
}

/**
 * A move from one click to another: in plan with the heights kept, or in 3D. Rounded to a micrometre, so two clicks
 * snapped to vertices whole metres apart move by whole metres, not by what f64 subtraction leaves at mine grid
 * coordinates.
 */
export function moveBetween(from: Vec3, to: Vec3, heights: 'keep' | 'move'): [number, number, number] {
  const round = (v: number) => Math.round(v * 1e6) / 1e6 + 0;
  return [round(to[0] - from[0]), round(to[1] - from[1]), heights === 'move' ? round(to[2] - from[2]) : 0];
}
