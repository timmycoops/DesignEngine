// The distance measure's numbers and words (decision 0079): run with `pnpm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { cursorLabel, gradeWords, legBetween, legLabel, legWords, measureTotals, measureWords } from '../src/viewport/measure.ts';

// Mine grid coordinates, so nothing passes only near the origin.
const E = 650_000;
const N = 7_500_000;
const near = (a, b, tolerance = 1e-9) => assert.ok(Math.abs(a - b) <= tolerance, `${a} against ${b}`);

test('a 3-4-5 leg rising 1 in 10 measures exactly', () => {
  // 300 m east and 400 m north is 500 m in plan on a bearing of atan(3/4); 50 m up is 10 %.
  const leg = legBetween([E, N, 100], [E + 300, N + 400, 150]);
  near(leg.plan, 500);
  near(leg.slope, Math.sqrt(500 * 500 + 50 * 50));
  near(leg.rise, 50);
  near(leg.grade, 10);
  near(leg.bearing, (Math.atan2(3, 4) * 180) / Math.PI);
  assert.equal(legWords(leg), 'plan 500.00 m · slope 502.49 m · RL +50.00 m · 10.0 % up (1 in 10.0, 5.7°) · bearing 036.9°');
  assert.equal(legLabel(leg), '500.00 m plan · 502.49 m slope · RL +50.00 · 10.0 % · 036.9°');
});

test('a millimetre shows at mine grid eastings and northings', () => {
  const leg = legBetween([E + 0.123, N + 0.456, 87.001], [E + 0.124, N + 0.456, 87.001]);
  near(leg.plan, 0.001, 1e-9);
  near(leg.bearing, 90, 1e-3);
});

test('bearings run clockwise from grid north, 0 to 360', () => {
  const to = (dx, dy) => legBetween([E, N, 0], [E + dx, N + dy, 0]).bearing;
  near(to(0, 10), 0);
  near(to(10, 0), 90);
  near(to(0, -10), 180);
  near(to(-10, 0), 270);
  near(to(-10, 10), 315);
  assert.ok(legWords(legBetween([E, N, 0], [E - 0.001, N + 100, 0])).endsWith('bearing 000.0°'), 'just west of north rounds to 000.0, not 360.0');
});

test('level, down, and straight up or down say so', () => {
  assert.equal(gradeWords(legBetween([E, N, 100], [E + 50, N, 100])), 'level');
  assert.equal(gradeWords(legBetween([E, N, 100], [E + 100, N, 90])), '10.0 % down (1 in 10.0, 5.7°)');
  const drop = legBetween([E, N, 100], [E, N, 85]);
  assert.equal(drop.grade, null);
  assert.equal(drop.bearing, null);
  assert.equal(legWords(drop), 'plan 0.00 m · slope 15.00 m · RL −15.00 m · vertical');
  assert.equal(legLabel(drop), '0.00 m plan · 15.00 m slope · RL −15.00');
});

test('clicks on add up along the points, with the rise from the first to the last', () => {
  // Along a bench, up a 10 % ramp, and back along the next bench.
  const points = [[E, N, 100], [E + 200, N, 100], [E + 200, N + 100, 110], [E, N + 100, 110]];
  const total = measureTotals(points);
  assert.equal(total.legs, 3);
  near(total.plan, 500);
  near(total.slope, 400 + Math.sqrt(100 * 100 + 10 * 10));
  near(total.rise, 10);
  assert.equal(measureWords([]), 'Measure: click the first point');
  assert.equal(measureWords(points.slice(0, 1)), 'Measure: from RL 100.00; click the next point. Esc clears');
  assert.ok(measureWords(points.slice(0, 2)).startsWith('Measure: plan 200.00 m · slope 200.00 m · RL +0.00 m · level · bearing 090.0°.'));
  assert.equal(measureWords(points), 'Measure: plan 200.00 m · slope 200.00 m · RL +0.00 m · level · bearing 270.0°. Total over 3 legs: plan 500.00 m · slope 500.50 m · RL +10.00 m');
});

test('beside the cursor: the leg to it, and from the second leg on the running total to it under it', () => {
  const points = [[E, N, 100], [E + 200, N, 100], [E + 200, N + 100, 110]];
  // One point down: the leg alone.
  assert.equal(cursorLabel(points.slice(0, 1), points[1]), '200.00 m plan · 200.00 m slope · RL +0.00 · 090.0°');
  // Two down, the cursor on the third: the leg up the ramp, and the total along both legs to the cursor.
  assert.equal(cursorLabel(points.slice(0, 2), points[2]), '100.00 m plan · 100.50 m slope · RL +10.00 · 10.0 % · 000.0°\nTotal 2 legs: 300.00 m plan · 300.50 m slope · RL +10.00');
  // Three down, the cursor back along the next bench: 3 legs, 500 m in plan.
  assert.ok(cursorLabel(points, [E, N + 100, 110]).endsWith('\nTotal 3 legs: 500.00 m plan · 500.50 m slope · RL +10.00'));
});
