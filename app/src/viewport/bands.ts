// Where the bands of colour by RL lie: a round interval for an RL range, and the first band edge in the
// viewport's local z, so every edge falls on a whole multiple of the interval in true RL. Plain numbers,
// no three.js, so tests/bands.test.mjs runs it under Node.

/** The intervals offered, in metres. */
export const INTERVALS = [1, 2, 5, 10, 20, 25, 50, 100];

/** About this many bands at most, when the interval is picked for a range. */
const MOST_BANDS = 20;

/** A round interval (1, 2 or 5 times a power of ten, at least 1 m) giving at most 20 bands over `span` metres. */
export function autoInterval(span: number): number {
  for (let power = 1; ; power *= 10) {
    for (const step of [1, 2, 5]) {
      if (span / (step * power) <= MOST_BANDS) return step * power;
    }
    if (power > 1e6) return 1e7;
  }
}

/** How one object's bands lie: in local z, from the edge at or below its lowest RL. */
export interface Bands {
  /** Local z of the lowest band's bottom edge. */
  base: number;
  /** Metres between edges. */
  interval: number;
  /** How many bands the ramp spreads over. */
  count: number;
  /** The true RLs the bands cover, for the legend. */
  low: number;
  high: number;
}

/**
 * The bands over true RLs `low` to `high` (local z plus `originZ`), at `interval` metres or, with null,
 * one picked from the range. Each band takes one colour from the ramp, spread from the lowest to the highest.
 */
export function bandsFor(low: number, high: number, originZ: number, interval: number | null): Bands {
  const step = interval ?? autoInterval(Math.max(high - low, 0));
  const first = Math.floor(low / step + 1e-9);
  const last = Math.max(Math.ceil(high / step - 1e-9), first + 1);
  return { base: first * step - originZ, interval: step, count: last - first, low: first * step, high: last * step };
}
