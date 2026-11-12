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

/** About this many bands at most over a grid's values. */
const MOST_VALUE_BANDS = 12;

/** A round step (1, 2 or 5 times a power of ten, as small as needed) giving at most 12 bands over `span`. */
export function valueStep(span: number): number {
  if (!(span > 0)) return 1;
  for (let power = 10 ** Math.floor(Math.log10(span / MOST_VALUE_BANDS)); ; power *= 10) {
    for (const step of [1, 2, 5]) {
      if (span / (step * power) <= MOST_VALUE_BANDS) return step * power;
    }
  }
}

/**
 * Bands over a grid's values, ash or RD, from `low` to `high` in the value's own units (decision 0061, workstream 6):
 * the grid's contours and the colour of a block of that seam come from the same bands, so a block reads against the
 * contours round it. A value on an edge is in the band above it, to a millionth of a step.
 */
export function valueBands(low: number, high: number): Bands {
  const step = valueStep(high - low);
  const onEdge = step * 1e-6;
  const first = Math.floor((low + onEdge) / step);
  const last = Math.max(Math.floor((high + onEdge) / step) + 1, first + 1);
  return { base: first * step, interval: step, count: last - first, low: first * step, high: last * step };
}

/** The band a value falls in, from 0, or null if it's outside them or isn't a number. */
export function bandOf(value: number | undefined, bands: Bands): number | null {
  if (value === undefined || !Number.isFinite(value)) return null;
  const band = Math.floor((value - bands.base + bands.interval * 1e-6) / bands.interval);
  return band >= 0 && band < bands.count ? band : null;
}
