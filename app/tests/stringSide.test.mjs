// Which side of a string a click is on (decision 0079): run with `pnpm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';

import { sideOf } from '../src/panels/stringSide.ts';

const E = 650_000;
const N = 7_500_000;
const at = (x, y) => [E + x, N + y, 100];

test('a click north of a string running east is on its left', () => {
  const east = [at(0, 0), at(200, 0)];
  assert.equal(sideOf(east, false, at(50, 30)), 'left');
  assert.equal(sideOf(east, false, at(50, -30)), 'right');
  assert.equal(sideOf([...east].reverse(), false, at(50, 30)), 'right', 'reversed, the sides swap');
});

test('beside a bent string it goes by the stretch the click is beside', () => {
  // East, then north: inside the corner is on the left of both stretches.
  const bent = [at(0, 0), at(200, 0), at(200, 150)];
  assert.equal(sideOf(bent, false, at(150, 50)), 'left');
  assert.equal(sideOf(bent, false, at(250, 100)), 'right');
  // Outside the corner, past it both ways, the nearest point is the corner itself: still the right.
  assert.equal(sideOf(bent, false, at(230, -30)), 'right');
});

test('inside a closed string drawn anticlockwise is its left', () => {
  const pit = [at(0, 0), at(100, 0), at(100, 100), at(0, 100)];
  assert.equal(sideOf(pit, true, at(50, 50)), 'left');
  assert.equal(sideOf(pit, true, at(-20, 50)), 'right', 'beside the stretch that closes it');
});

test("where it can't be told there's no side", () => {
  assert.equal(sideOf([at(0, 0)], false, at(5, 5)), null);
  assert.equal(sideOf([at(0, 0), at(200, 0)], false, at(100, 0)), null, 'on the string');
});

test('a click on a string finds it, the nearest in plan within the reach, and nothing past it', async () => {
  const { nearestString } = await import('../src/panels/stringSide.ts');
  const line = (id, y) => ({ id, closed: false, vertices: [[650000, 7450000 + y, 100], [650100, 7450000 + y, 110]] });
  const strings = [line('a', 0), line('b', 10)];
  assert.equal(nearestString(strings, [650050, 7450000.5, 0], 2)?.id, 'a');
  assert.equal(nearestString(strings, [650050, 7450009, 0], 2)?.id, 'b');
  assert.equal(nearestString(strings, [650050, 7450005, 0], 2), null, 'five metres off both is neither');
  const ring = { id: 'ring', closed: true, vertices: [[0, 0, 0], [10, 0, 0], [10, 10, 0]] };
  assert.equal(nearestString([ring], [5, 5.2, 0], 1)?.id, 'ring', 'its closing side counts');
});
