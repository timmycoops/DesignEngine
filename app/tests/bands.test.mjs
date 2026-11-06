// Colour by RL: band edges land on round RLs whatever the scene origin, and the interval picked for a
// range gives a sensible number of bands. Run with `pnpm test` (Node's own runner, no dependencies).
import assert from 'node:assert/strict';
import test from 'node:test';
import { autoInterval, bandsFor } from '../src/viewport/bands.ts';

/** What the shader does, in f32 as the GPU has it: which band a local z is in, and how far into it. */
function shaderBand(localZ, bands) {
  const f = Math.fround;
  const b = f(f(f(localZ) - f(bands.base)) / f(bands.interval));
  return { band: Math.min(Math.max(Math.floor(b), 0), bands.count - 1), b };
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
  // Each edge in true RL is exactly where the shader's band number turns over: at RL 150, 3.000 bands up.
  for (let rl = 130; rl <= 270; rl += 10) {
    const edge = shaderBand(rl - originZ, bands);
    assert.equal(edge.b, (rl - 120) / 10, `RL ${rl} sits on an edge`);
    assert.equal(shaderBand(rl - originZ - 0.001, bands).band, edge.band - 1, `just under RL ${rl} is the band below`);
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
