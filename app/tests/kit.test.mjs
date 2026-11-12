// The kit's number boxes and run results (UI plan): what a box takes as it's typed, how it shows a value, and how a run's
// notes show. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readNumber, showNumber } from '../src/design-system/kit/number.ts';
import { canShow, lookAt, noteTone, plainNote } from '../src/design-system/kit/notes.ts';

test('a box takes numbers in range, with thousands commas, and says why it will not take the rest', () => {
  assert.deepEqual(readNumber('65'), { value: 65 });
  assert.deepEqual(readNumber(' 1,250.5 '), { value: 1250.5 });
  assert.deepEqual(readNumber('-3'), { value: -3 });
  assert.deepEqual(readNumber(''), { why: 'Type a number' });
  assert.deepEqual(readNumber('6-'), { why: "6- isn't a number" });
  assert.deepEqual(readNumber('0', { min: 0, minIncluded: false }), { why: 'More than 0' });
  assert.deepEqual(readNumber('0', { min: 0 }), { value: 0 });
  assert.deepEqual(readNumber('91', { min: 0, max: 90 }), { why: 'At most 90' });
  assert.deepEqual(readNumber('90', { min: 0, max: 90 }), { value: 90 });
});

test('a value shows as it reads back, with no float noise', () => {
  assert.equal(showNumber(65), '65');
  assert.equal(showNumber(0.1 + 0.2), '0.3');
  assert.equal(showNumber(1250.5), '1250.5');
  assert.equal(readNumber(showNumber(0.1 + 0.2)).value, 0.3);
});

test('a note shows by its severity, never by its words', () => {
  const stopped = { severity: 'warning', code: 'ramp.stopped', text: 'road A: crosses the berm, then stopped', place: { kind: 'point', at: [650100, 7450100, 120] } };
  const crossing = { severity: 'blocking', code: 'seam.crossing', text: 'BLOCKING: cl roof above the floor of cu', place: { kind: 'plan', at: [500000, 7000000] } };
  // Words the old list took for warnings, in a line that says nothing is wrong.
  const clean = { severity: 'info', text: '3 solids made, checked by the solid rules: none crosses itself' };
  assert.equal(noteTone(stopped, 1), 'warning');
  assert.equal(noteTone(crossing, 0), 'blocking', 'a block stands out, first or not');
  assert.equal(noteTone(clean, 1), 'muted');
  assert.equal(noteTone(clean, 0), 'lead', 'the first line says what the run made');
});

test('Show goes where a note has a place, or to what a warning is about', () => {
  assert.ok(canShow({ severity: 'info', text: 'road A: 120.0 m', place: { kind: 'point', at: [0, 0, 0] } }));
  assert.ok(canShow({ severity: 'warning', text: 'strip 7: Nothing is built on this string', object: 'id' }));
  assert.ok(!canShow({ severity: 'info', text: 'strip 1: 2500000.000 m3', object: 'id' }), 'information about an object has nothing to show');
  assert.ok(!canShow(plainNote('Took out step 3')));
});

test('the status line counts what to look at', () => {
  const warning = { severity: 'warning', text: 'w' };
  assert.equal(lookAt([plainNote('made'), warning, warning, { severity: 'blocking', text: 'b' }]), '1 blocking, 2 warnings');
  assert.equal(lookAt([plainNote('made'), warning]), '1 warning');
  assert.equal(lookAt([plainNote('made')]), '');
});
