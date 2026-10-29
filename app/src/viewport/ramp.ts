// The thickness colour ramp: a perceptually even dark-blue to yellow (viridis), shared by the
// viewport's vertex colours and the legend so the two always agree.

const STOPS: [number, number, number][] = [
  [68, 1, 84],
  [72, 40, 120],
  [62, 74, 137],
  [49, 104, 142],
  [38, 130, 142],
  [31, 158, 137],
  [53, 183, 121],
  [109, 205, 89],
  [180, 222, 44],
  [253, 231, 37],
];

/** Nodes with no coal: plain grey, so absent coal never reads as thin coal. */
export const NO_COAL: [number, number, number] = [112, 116, 122];
/** Nodes flagged with an issue. */
export const ISSUE: [number, number, number] = [229, 72, 77];

/** The ramp colour at `t` in [0, 1]. */
export function rampColour(t: number): [number, number, number] {
  const x = Math.min(Math.max(t, 0), 1) * (STOPS.length - 1);
  const i = Math.min(Math.floor(x), STOPS.length - 2);
  const f = x - i;
  const [a, b] = [STOPS[i], STOPS[i + 1]];
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

/** The ramp as a CSS gradient, low on the left. */
export function rampCss(): string {
  const stops = STOPS.map(([r, g, b], i) => `rgb(${r} ${g} ${b}) ${((100 * i) / (STOPS.length - 1)).toFixed(1)}%`);
  return `linear-gradient(to right, ${stops.join(', ')})`;
}

export const css = ([r, g, b]: [number, number, number]) => `rgb(${r} ${g} ${b})`;

/**
 * Vertex colours (RGB bytes) for quantised values: the ramp over the value range, grey for no
 * value, red for a flagged node.
 */
export function valueColours(values: Uint16Array, value: { min: number; max: number; step: number; null: number; flag: number }): Uint8Array {
  const out = new Uint8Array(values.length * 3);
  const span = value.max - value.min;
  for (let i = 0; i < values.length; i++) {
    const q = values[i];
    const c = q === value.flag ? ISSUE : q === value.null ? NO_COAL : rampColour(span > 0 ? (q * value.step) / span : 1);
    out[3 * i] = c[0];
    out[3 * i + 1] = c[1];
    out[3 * i + 2] = c[2];
  }
  return out;
}
