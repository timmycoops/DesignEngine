// Move and copy (String tools plan, workstream 3): what it says it's moving, and the move two clicks make. Run with
// `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { countedWords, moveBetween } from '../src/panels/moveWords.ts';

test('what is picked, counted by kind in words', () => {
  assert.equal(countedWords([]), 'nothing');
  assert.equal(countedWords([{ kind: 'solid' }]), '1 solid');
  assert.equal(countedWords([{ kind: 'solid' }, { kind: 'string' }, { kind: 'string' }, { kind: 'string' }]), '3 strings and 1 solid');
  assert.equal(countedWords([{ kind: 'grid' }, { kind: 'surface' }, { kind: 'string' }]), '1 string, 1 surface and 1 grid');
});

test('two clicks move by the difference, in plan unless the heights move too, to a micrometre', () => {
  const from = [650_123.4, 7_450_987.6, 104.2];
  const to = [650_223.4, 7_450_787.35, 107.2];
  assert.deepEqual(moveBetween(from, to, 'keep'), [100, -200.25, 0]);
  assert.deepEqual(moveBetween(from, to, 'move'), [100, -200.25, 3]);
  assert.deepEqual(moveBetween(from, from, 'move'), [0, 0, 0]);
});
