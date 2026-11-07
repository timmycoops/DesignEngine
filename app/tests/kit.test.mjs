// The kit's number boxes and run results (UI plan): what a box takes as it's typed, how it shows a value, and which of a
// run's messages read as warnings. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readNumber, showNumber } from '../src/design-system/kit/number.ts';
import { warns } from '../src/design-system/kit/words.ts';

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

test('the messages that warn are the ones a planner should look at', () => {
  for (const line of [
    'road A: stopped at 650100.0, 7450100.0: its top is only 12.0 m along the crest',
    'the wall has no height along 1 stretch(es), the first from 651054.6, 7450405.4',
    '2 pairs of the wall\'s triangles cross',
    'Built crest 1 had been edited by hand: it\'s built again, and undo brings your edit back',
    "road C: its string doesn't cross the line, so it made no road",
  ]) {
    assert.ok(warns(line), line);
  }
  for (const line of ['Built step 2: a berm 10 m wide on RL 120.0, from Built crest 1', 'road A: crosses the berm to climb on from its line', 'road A: turns back on the berm, for the wall above', '6 steps built again from Built crest 5 as it is now']) {
    assert.ok(!warns(line), line);
  }
});
