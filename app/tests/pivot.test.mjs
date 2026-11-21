// The fan of rays round the cursor and the last-pivot check (decision 0044). Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { FAN_RAYS, FAN_RINGS_PX, fanRings, inView } from '../src/viewport/pivot.ts';

test('the fan is rings of rays at their distances from the cursor, nearest first', () => {
  const rings = fanRings(100, 200);
  assert.equal(rings.length, FAN_RINGS_PX.length);
  rings.forEach((ring, i) => {
    assert.equal(ring.length, FAN_RAYS[i]);
    for (const [x, y] of ring) assert.ok(Math.abs(Math.hypot(x - 100, y - 200) - FAN_RINGS_PX[i]) < 1e-9);
  });
  // The first ring's first ray goes straight right; the second ring's is turned half a step.
  assert.deepEqual(rings[0][0], [108, 200]);
  const [x, y] = rings[1][0];
  assert.ok(Math.abs(Math.atan2(y - 200, x - 100) - Math.PI / FAN_RAYS[1]) < 1e-12);
  // The last ring's rays are no more than about 50 px apart.
  const last = rings[rings.length - 1];
  assert.ok(Math.hypot(last[0][0] - last[1][0], last[0][1] - last[1][1]) < 51);
});

test('a last pivot is used only while it is in view', () => {
  assert.ok(inView({ x: 0.2, y: -0.9, z: 0.99 }));
  assert.ok(!inView({ x: 1.2, y: 0, z: 0.5 }), 'off the side');
  assert.ok(!inView({ x: 0, y: 0, z: 1.5 }), 'behind the camera');
});
