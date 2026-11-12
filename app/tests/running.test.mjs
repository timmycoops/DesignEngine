// The one run going, app-wide (decision 0060): one at a time, and what the buttons waiting on it and the status bar say.
// Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { progressWords, running, runWords, waitingOn } from '../src/running.ts';

test('one run at a time, and what waits says what it waits on', () => {
  const told = [];
  const stop = running.subscribe(() => told.push(running.get()?.command ?? null));
  assert.equal(waitingOn(), null, 'nothing going, nothing waits');
  assert.equal(running.start('solid.slice_by_faces', 10), true);
  assert.equal(running.start('face.build_step', 20), false, 'a second run waits for the first');
  assert.equal(running.get().command, 'solid.slice_by_faces');
  assert.equal(waitingOn(), 'Waiting on slicing into strips');
  running.end();
  assert.equal(waitingOn(), null);
  assert.deepEqual(told, ['solid.slice_by_faces', null]);
  stop();
  assert.equal(runWords('string.reverse'), 'string.reverse', 'a command with no words of its own goes by its name');
});

test('the status bar says how far a run has got and how long it has been', () => {
  assert.equal(progressWords({ done: 3, total: 15, of: 'strips', seconds: 12.7 }), '3 of 15 strips · 12 s');
  assert.equal(progressWords({ done: 1240, total: 43000, of: 'solids', seconds: 75 }), '1,240 of 43,000 solids · 1 min 15 s');
  assert.equal(progressWords({ done: 0, total: 0, of: '', seconds: 2 }), '2 s', 'nothing counted yet: the time alone');
  assert.equal(progressWords({ done: 3, total: 15, of: 'strips', seconds: 4, cancelled: true }), 'cancelling · 3 of 15 strips · 4 s');
});
