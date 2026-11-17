/**
 * Which side of a string a point is on, looking along the string, in plan: what "click the side" works out for a
 * tool that takes left or right (decision 0079). It goes by the segment nearest the point, so a click beside a bent
 * string means the side of the stretch it's beside. Kept free of React, so the tests can read it.
 */
export type Side = 'left' | 'right';
type Point = readonly [number, number, number];

/** A string as the view has it: its vertices in world coordinates. */
export type Placed = { id: string; vertices: readonly Point[]; closed: boolean };

/**
 * The string nearest a point clicked, in plan, within `within` metres, or null: what a click on a cutting string, or a
 * string to run on to, means. The click is snapped, so on a string it's on it; `within` takes a click that missed.
 */
export function nearestString<T extends Placed>(strings: readonly T[], point: Point, within: number): T | null {
  let best: { gap: number; string: T } | null = null;
  for (const string of strings) {
    const v = string.vertices;
    const n = v.length;
    for (let i = 0; i < (string.closed ? n : n - 1); i++) {
      const [a, b] = [v[i], v[(i + 1) % n]];
      const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
      const length = dx * dx + dy * dy;
      const t = length === 0 ? 0 : Math.min(1, Math.max(0, ((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length));
      const gap = Math.hypot(a[0] + t * dx - point[0], a[1] + t * dy - point[1]);
      if (gap <= within && (best === null || gap < best.gap)) best = { gap, string };
    }
  }
  return best?.string ?? null;
}

/** The side of `vertices` that `point` is on, or null where it can't be told: fewer than two vertices, or on the string. */
export function sideOf(vertices: readonly Point[], closed: boolean, point: Point): Side | null {
  const n = vertices.length;
  if (n < 2) return null;
  let nearest: { gap: number; cross: number } | null = null;
  for (let i = 0; i < (closed ? n : n - 1); i++) {
    const [a, b] = [vertices[i], vertices[(i + 1) % n]];
    const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    const length = dx * dx + dy * dy;
    if (length === 0) continue;
    const t = Math.min(1, Math.max(0, ((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length));
    const gap = Math.hypot(a[0] + t * dx - point[0], a[1] + t * dy - point[1]);
    // Past an end or a corner the nearest point is a vertex two segments share: the one the point is further round wins.
    const cross = (dx * (point[1] - a[1]) - dy * (point[0] - a[0])) / Math.sqrt(length);
    if (nearest === null || gap < nearest.gap - 1e-9 || (Math.abs(gap - nearest.gap) <= 1e-9 && Math.abs(cross) > Math.abs(nearest.cross))) nearest = { gap, cross };
  }
  if (nearest === null || Math.abs(nearest.cross) < 1e-9) return null;
  return nearest.cross > 0 ? 'left' : 'right';
}
