// The filter box's logic (Filtering plan, workstream 2; decision 0088): values counted to tick, ticks written into the
// filter as words, and a filter that's more than ticks turned into the ticks of what it keeps. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { isOn, isTicked, readWhen, tick, valuesOf, whenFits, whenText } from '../src/filters.ts';

const rows = [
  { seam: 'U', strip: 3, material: 'coal', volume: 120 },
  { seam: 'u', strip: 10, material: 'Coal', volume: 480 },
  { seam: 'M', strip: 9, material: 'waste' },
  { Seam: 'L', strip: 3 },
];

test('values are counted an attribute at a time, the cuts first, case aside', () => {
  const v = valuesOf(rows);
  assert.deepEqual(v.map((a) => a.key), ['strip', 'seam', 'material', 'volume']);
  const seam = v.find((a) => a.key === 'seam');
  assert.deepEqual(seam.values, [{ value: 'L', count: 1 }, { value: 'M', count: 1 }, { value: 'U', count: 2 }], 'Seam and seam are one, U and u one');
  const strip = v.find((a) => a.key === 'strip');
  assert.deepEqual([strip.numeric, strip.min, strip.max, strip.values.map((x) => x.value)], [true, 3, 10, ['3', '9', '10']], 'numbers in order: 9 before 10');
  assert.deepEqual([v.find((a) => a.key === 'material').numeric, v.find((a) => a.key === 'volume').rows], [false, 2]);
});

test('a tick adds or takes off a word, and the last off leaves the attribute out', () => {
  let when = {};
  when = tick(when, 'seam', 'U', true, ['L', 'M', 'U']);
  when = tick(when, 'seam', 'M', true, ['L', 'M', 'U']);
  assert.equal(whenText(when), 'seam: U, M');
  assert.ok(isOn(when, 'seam', 'u') && !isOn(when, 'seam', 'L') && !isOn(when, 'strip', '3'));
  assert.equal(rows.filter((r) => whenFits(when, r)).length, 3, 'U, u and M; L is out');
  when = tick(when, 'seam', 'u', false, ['L', 'M', 'U']);
  when = tick(when, 'seam', 'M', false, ['L', 'M', 'U']);
  assert.deepEqual(when, {}, 'nothing ticked is anything');
});

test('a filter that is more than ticks becomes the ticks of what it keeps', () => {
  assert.ok(isTicked('U, M, 3') && !isTicked('1-5') && !isTicked('<10') && !isTicked('!WASTE') && !isTicked('*'));
  const read = readWhen('strip: 1-9; material: !waste');
  assert.ok('when' in read);
  const strips = ['3', '9', '10'];
  assert.ok(isOn(read.when, 'strip', '3') && isOn(read.when, 'strip', '9') && !isOn(read.when, 'strip', '10'));
  // Ticking 10 on keeps 3 and 9, which the range kept.
  assert.equal(tick(read.when, 'strip', '10', true, strips).strip, '3, 9, 10');
  // Ticking 9 off keeps 3.
  assert.equal(tick(read.when, 'strip', '9', false, strips).strip, '3');
  // A not keeps every value but the one it names: ticking coal off leaves nothing of the material kept, so it's left out.
  assert.equal(tick(read.when, 'material', 'coal', false, ['coal']).material, undefined);
});
