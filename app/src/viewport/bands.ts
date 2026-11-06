// Where the bands of colour by RL lie: a round interval for an RL range, and the first band edge in the
// viewport's local z, so every edge falls on a whole multiple of the interval in true RL. Plain numbers,
// no three.js, so tests/bands.test.mjs runs it under Node.

/** The intervals offered, in metres. */
export const INTERVALS = [1, 2, 5, 10, 20, 25, 50, 100];

/** About this many bands at most, when the interval is picked for a range. */
const MOST_BANDS = 20;

/**
 * How near a band edge, in metres, an RL counts as on it. A berm designed at RL 115 is 115.000 in the engine, but
 * the GPU works in f32 and puts it a hair either side of the edge pixel by pixel, which streaked it in two colours.
 * What lies on an edge goes in the band above it, as a bench is named by its floor: the berm at 115 is coloured as
 * 115 to 120. A millimetre is well past the GPU's rounding at a local z of a kilometre (0.06 mm a step) and too
 * small to see.
 */
export const ON_EDGE = 0.001;

/**
 * The least the shader takes a band to change across a pixel, in bands. On a flat the change is nothing, or only
 * the GPU's rounding: this keeps a contour line's fade from having both ends at zero, and a berm ON_EDGE into its
 * band from being drawn as one wide line.
 */
export const MIN_PER_PIXEL = 1e-6;

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
 * What lies on an edge is in the band above it, the highest RL too: a design whose top berm is on a round RL gets
 * a band above it, so that berm is coloured as the ones below it are.
 */
export function bandsFor(low: number, high: number, originZ: number, interval: number | null): Bands {
  const step = interval ?? autoInterval(Math.max(high - low, 0));
  const first = Math.floor((low + ON_EDGE) / step);
  const last = Math.max(Math.floor((high + ON_EDGE) / step) + 1, first + 1);
  return { base: first * step - originZ, interval: step, count: last - first, low: first * step, high: last * step };
}
