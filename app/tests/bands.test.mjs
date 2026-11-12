// Colour by RL: band edges land on round RLs whatever the scene origin, and the interval picked for a
// range gives a sensible number of bands. Run with `pnpm test` (Node's own runner, no dependencies).
import assert from 'node:assert/strict';
import test from 'node:test';
import { autoInterval, bandsFor, INTERVALS, MIN_PER_PIXEL, ON_EDGE } from '../src/viewport/bands.ts';

const f = Math.fround;

/** What the shader does (rlColour.ts), in f32 as the GPU has it: which band a local z is in, and how far into it. */
function shaderBand(localZ, bands) {
  const b = f(f(f(f(localZ) - f(bands.base)) + f(ON_EDGE)) / f(bands.interval));
  return { band: Math.min(Math.max(Math.floor(b), 0), bands.count - 1), b };
}

/** The shader before this rule, with nothing done about an edge: for showing the streaks it made. */
function shaderBandBefore(localZ, bands) {
  const b = f(f(f(localZ) - f(bands.base)) / f(bands.interval));
  return Math.min(Math.max(Math.floor(b), 0), bands.count - 1);
}

/** The f32 `steps` steps up (or down) from `x`: where the GPU's rounding puts a z it interpolates across a triangle. */
function ulps(x, steps) {
  if (x === 0) return f(steps * 2 ** -149);
  const buffer = new Float32Array([x]);
  const bits = new Int32Array(buffer.buffer);
  bits[0] += x >= 0 ? steps : -steps;
  return buffer[0];
}

const smoothstep = (e0, e1, x) => {
  const t = Math.min(Math.max((x - e0) / (e1 - e0), 0), 1);
  return t * t * (3 - 2 * t);
};

/** How much of a contour line the shader draws at `b`, with `change` bands across a pixel there: 0 none, 1 all. */
function lineAt(b, change) {
  const perPixel = Math.max(change, MIN_PER_PIXEL);
  const fract = b - Math.floor(b);
  return 1 - smoothstep(perPixel * 0.5, perPixel * 1.5, Math.min(fract, 1 - fract));
}

test('an interval gives at most 20 bands and is 1, 2 or 5 times a power of ten', () => {
  const cases = [
    [0, 1],
    [12, 1],
    [20, 1],
    [21, 2],
    [40, 2],
    [41, 5],
    [100, 5],
    [150, 10],
    [200, 10],
    [380, 20],
    [1000, 50],
    [4000, 200],
  ];
  for (const [span, interval] of cases) assert.equal(autoInterval(span), interval, `span ${span} m`);
});

test('band edges are at whole multiples of the interval in true RL, with the origin taken off', () => {
  // A topography from RL 123.4 to 271.8, with the scene origin at RL 187 (a whole metre, as the shell sets it).
  const originZ = 187;
  const bands = bandsFor(123.4, 271.8, originZ, null);
  assert.equal(bands.interval, 10);
  assert.equal(bands.low, 120);
  assert.equal(bands.high, 280);
  assert.equal(bands.count, 16);
  assert.equal(bands.base, 120 - originZ, 'the lowest edge, in local z');
  // Each edge in true RL is where the shader's band number turns over: at RL 150, 3 bands up, give or take the
  // millimetre an edge takes in.
  for (let rl = 130; rl <= 270; rl += 10) {
    const edge = shaderBand(rl - originZ, bands);
    assert.equal(edge.band, (rl - 120) / 10, `RL ${rl} is the bottom of its band`);
    assert.ok(Math.abs(edge.b - (rl - 120) / 10) < 1e-3, `RL ${rl} sits on an edge`);
    assert.equal(shaderBand(rl - originZ - 0.002, bands).band, edge.band - 1, `2 mm under RL ${rl} is the band below`);
  }
});

test('a berm on a band edge is one colour, the band above it, however the GPU rounds it', () => {
  // Tim's step template: a face from RL 100 to 155 with flat berms at 115, 130, 145 and 155, banded every 5 m.
  // The scene origin is any whole metre; local z out to a kilometre either side of it.
  const berms = [115, 130, 145, 155];
  let streaked = 0;
  let cases = 0;
  for (const originZ of [0, 128, 155, 1100, -845]) {
    const bands = bandsFor(100, 155.00000000000003, originZ, null);
    assert.equal(bands.interval, 5);
    for (const rl of berms) {
      const local = f(rl - originZ);
      const want = (rl - 100) / 5;
      const before = new Set();
      for (let steps = -4; steps <= 4; steps++) {
        assert.equal(shaderBand(ulps(local, steps), bands).band, want, `berm at RL ${rl}, origin ${originZ}, ${steps} f32 steps off`);
        before.add(shaderBandBefore(ulps(local, steps), bands));
      }
      cases += 1;
      if (before.size > 1) streaked += 1;
    }
  }
  // Before, a few f32 steps either side of the edge fell in two bands, so most berms came out in two colours
  // (19 of these 20; where the sums round back onto the edge, as at RL 130 with the origin at 128, it didn't show).
  assert.ok(streaked > cases / 2, `berms in two colours before: ${streaked} of ${cases}`);
});

test('a surface whose top is on an edge has a band above it, so its top berm is coloured as the rest', () => {
  const bands = bandsFor(100, 155.00000000000003, 128, null);
  assert.deepEqual(bands, { base: 100 - 128, interval: 5, count: 12, low: 100, high: 160 });
  assert.equal(shaderBand(155 - 128, bands).band, 11, 'the berm at 155 is the 155 to 160 band, not squeezed into 150 to 155');
  assert.equal(shaderBand(100 - 128, bands).band, 0);
  // Within a millimetre under an edge counts as on it, at the bottom and top of a range alike.
  assert.equal(bandsFor(99.9995, 154.9995, 0, 5).low, 100);
  assert.equal(bandsFor(99.9995, 154.9995, 0, 5).high, 160);
  assert.equal(bandsFor(99.998, 154.998, 0, 5).low, 95);
  assert.equal(bandsFor(99.998, 154.998, 0, 5).high, 155);
});

test('a contour line runs where a wall crosses an edge, not across a berm lying on it', () => {
  // Every interval offered, with local z out to about a kilometre and a bit.
  for (const interval of INTERVALS) {
    for (const originZ of [0, 1000, -1000]) {
      const bands = bandsFor(100, 160, originZ, interval);
      const edge = Math.ceil(110 / interval) * interval;
      const local = f(edge - originZ);
      // A flat berm on the edge: a band changes across a pixel by nothing, or by the GPU's rounding.
      for (let steps = -4; steps <= 4; steps++) {
        const b = shaderBand(ulps(local, steps), bands).b;
        for (const change of [0, 1e-9, 1e-7]) {
          assert.ok(lineAt(b, change) < 1e-6, `no line across a berm at RL ${edge} (${interval} m bands, origin ${originZ})`);
        }
      }
      // A wall climbing through the edge, a few centimetres a pixel: the line is drawn either side of it.
      const change = 0.05 / interval;
      for (const off of [-0.01, 0, 0.01]) {
        const b = shaderBand(edge - originZ + off, bands).b;
        assert.equal(lineAt(b, change), 1, `line on a wall ${off} m from RL ${edge} (${interval} m bands)`);
      }
    }
  }
});

test('a set interval is kept, and an RL already on an edge starts the lowest band', () => {
  const bands = bandsFor(-35, 62.5, -12, 5);
  assert.deepEqual(bands, { base: -35 + 12, interval: 5, count: 20, low: -35, high: 65 });
  assert.equal(shaderBand(-35 + 12, bands).band, 0);
  assert.equal(shaderBand(62.5 + 12, bands).band, 19);
});

test('a flat pad still has one band', () => {
  const bands = bandsFor(150, 150, 150, null);
  assert.equal(bands.count, 1);
  assert.equal(bands.low, 150);
  assert.equal(bands.high, 151);
});

test('a shared range puts the same RL in the same band on every surface', () => {
  // Topography, roof and floor over one range: one set of bands, so RL 140 is one colour on all three.
  const originZ = 187;
  const shared = bandsFor(80.2, 271.8, originZ, null);
  const at140 = shaderBand(140 - originZ, shared).band;
  assert.equal(shared.interval, 10);
  assert.equal(at140, (140 - 80) / 10);
});

test('a grid of ash or RD takes round bands in its own units, and a block its band by the same edges', async () => {
  const { valueBands, valueStep, bandOf } = await import('../src/viewport/bands.ts');
  assert.equal(valueStep(6), 0.5, 'ash 10 to 16 %: half a per cent');
  assert.equal(valueStep(0.2), 0.02, 'RD 1.35 to 1.55: 0.02');
  const ash = valueBands(10.2, 15.9);
  assert.deepEqual([ash.base, ash.interval, ash.count], [10, 0.5, 12]);
  assert.equal(bandOf(10.2, ash), 0);
  assert.equal(bandOf(12.5, ash), 5, 'on an edge, in the band above it');
  assert.equal(bandOf(15.9, ash), 11);
  assert.equal(bandOf(17, ash), null);
  assert.equal(bandOf(Number.NaN, ash), null);
  const rd = valueBands(1.35, 1.55);
  assert.equal(rd.interval, 0.02);
  assert.equal(bandOf(1.45, rd), 5, '1.45 a hair under its edge in floating point still goes up');
});
