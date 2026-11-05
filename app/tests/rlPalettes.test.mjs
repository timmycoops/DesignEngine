// The ramps an RL can be coloured on: each is a well-formed table, a step of RL is always a visible step of
// colour, and none can be read as the seam thickness ramp. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { rampColour } from '../src/viewport/ramp.ts';
import { DEFAULT_RL_PALETTE, paletteColour, RL_PALETTE_NAMES, RL_PALETTES } from '../src/viewport/rlPalettes.ts';

/** sRGB bytes to OKLab (Ottosson), where a distance of about 0.02 is just visible. */
function oklab([r, g, b]) {
  const lin = (c) => ((c /= 255) <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  [r, g, b] = [lin(r), lin(g), lin(b)];
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
}
const apart = (x, y) => Math.hypot(...oklab(x).map((v, i) => v - oklab(y)[i]));

test('the default is one of the ramps, and every ramp is whole sRGB bytes with at least five stops', () => {
  assert.ok(RL_PALETTE_NAMES.includes(DEFAULT_RL_PALETTE));
  for (const name of RL_PALETTE_NAMES) {
    const { label, stops } = RL_PALETTES[name];
    assert.ok(label.length > 0, name);
    assert.ok(stops.length >= 5, `${name} has ${stops.length} stops`);
    for (const stop of stops) for (const c of stop) assert.ok(Number.isInteger(c) && c >= 0 && c <= 255, `${name}: ${stop}`);
  }
});

test('a ramp runs from its first stop to its last, straight between stops', () => {
  const stops = [
    [0, 0, 0],
    [100, 200, 50],
    [200, 100, 250],
  ];
  assert.deepEqual(paletteColour(stops, 0), [0, 0, 0]);
  assert.deepEqual(paletteColour(stops, 1), [200, 100, 250]);
  assert.deepEqual(paletteColour(stops, 0.5), [100, 200, 50]);
  assert.deepEqual(paletteColour(stops, 0.25), [50, 100, 25]);
  assert.deepEqual(paletteColour(stops, -1), [0, 0, 0], 'below the range is the lowest RL colour');
  assert.deepEqual(paletteColour(stops, 2), [200, 100, 250], 'above the range is the highest');
});

/** The nearest two neighbouring bands of `count`, each coloured at its middle as the shader samples it. */
function nearestBands(stops, count) {
  const bands = Array.from({ length: count }, (_, k) => paletteColour(stops, (k + 0.5) / count));
  return Math.min(...bands.slice(1).map((c, k) => apart(c, bands[k])));
}

// Auto gives at most 20 bands (bands.test.mjs). About 0.01 apart in OKLab is the least that can be seen side by
// side; 0.02 is plain at a glance. Spectral's pale middle is its closest pair (0043), with a contour line between.
test('with 20 bands, every band can be told from the next on every ramp', () => {
  for (const name of RL_PALETTE_NAMES) {
    const nearest = nearestBands(RL_PALETTES[name].stops, 20);
    assert.ok(nearest > 0.012, `${name}: neighbouring bands only ${nearest.toFixed(3)} apart in OKLab`);
  }
});

test('with 20 bands, batlow and the even rainbow step evenly, and Spectral is closest in its pale middle', () => {
  for (const name of ['batlow', 'even']) assert.ok(nearestBands(RL_PALETTES[name].stops, 20) > 0.02, name);
  const stops = RL_PALETTES.spectral.stops;
  const bands = Array.from({ length: 20 }, (_, k) => paletteColour(stops, (k + 0.5) / 20));
  const steps = bands.slice(1).map((c, k) => apart(c, bands[k]));
  const k = steps.indexOf(Math.min(...steps));
  // The pale yellow stop is the sixth of eleven, half way along: the closest pair is either side of it.
  assert.ok(Math.abs((k + 1) / 20 - 0.5) <= 0.1, `closest pair at band ${k}`);
});

test('no ramp can be taken for the seam thickness ramp', () => {
  for (const name of RL_PALETTE_NAMES) {
    const stops = RL_PALETTES[name].stops;
    const t = Array.from({ length: 11 }, (_, i) => i / 10);
    const mean = t.reduce((sum, x) => sum + apart(paletteColour(stops, x), rampColour(x)), 0) / t.length;
    assert.ok(mean > 0.1, `${name} is on average only ${mean.toFixed(3)} from viridis in OKLab`);
  }
});
