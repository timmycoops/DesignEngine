// The filter box's logic (Filtering plan, workstream 2; decision 0088): values counted to tick, ticks written into the
// filter as words, and a filter that's more than ticks turned into the ticks of what it keeps. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { accept, isOn, isTicked, measuresOf, readWhen, suggest, tick, valuesOf, whenFits, whenText } from '../src/filters.ts';

const rows = [
  { seam: 'U', strip: 3, material: 'coal', volume: 120 },
  { seam: 'u', strip: 10, material: 'Coal', volume: 480 },
  { seam: 'M', strip: 9, material: 'waste' },
  { Seam: 'L', strip: 3 },
];

test('suggestions: names where a name is typed, the cuts first, those named elsewhere left out', () => {
  const values = valuesOf(rows);
  const at = (text, caret = text.length, names) => suggest(text, caret, values, names);
  const empty = at('');
  assert.deepEqual(empty.items.map((i) => i.label), ['strip', 'seam', 'material', 'volume']);
  assert.deepEqual(empty.items[0], { insert: 'strip: ', label: 'strip', detail: '3 to 10' });
  assert.equal(empty.items[1].detail, '3 values');
  assert.deepEqual(at('se').items.map((i) => i.label), ['seam']);
  assert.deepEqual(at('al').items.map((i) => i.label), ['material'], 'one that holds what is typed');
  const second = at('seam: U; s');
  assert.deepEqual(second.items.map((i) => i.label), ['strip'], 'seam is named already');
  assert.deepEqual([second.from, second.to], [9, 10]);
  assert.deepEqual(accept('seam: U; s', second, second.items[0]), { text: 'seam: U; strip: ', caret: 16 });
  // A name changed in front of its colon: only the name goes in.
  const before = at('sx: 3', 1);
  assert.deepEqual([before.from, before.to, before.items[0].insert], [0, 2, 'strip']);
  assert.equal(accept('sx: 3', before, before.items[0]).text, 'strip: 3');
  // A table's fields, typed and never ticked, offered too; nothing that isn't there.
  assert.deepEqual(at('ro', 2, ['ROM\\Coal\\ash']).items, [{ insert: 'ROM\\Coal\\ash: ', label: 'ROM\\Coal\\ash', detail: 'type it' }]);
  // A layer's measures by what's on it; one an attribute already names isn't offered twice.
  assert.deepEqual(measuresOf(['string']).map((m) => m.key), ['length', 'area', 'rl_min', 'rl_max']);
  assert.deepEqual(measuresOf(['solid', 'grid']).map((m) => m.key), ['volume', 'rl_min', 'rl_max']);
  assert.deepEqual(at('vol', 3, measuresOf(['solid'])).items.map((i) => `${i.label} ${i.detail}`), ['volume 120 to 480'], 'the attribute wins');
  assert.equal(at('zz').hint, 'Nothing here is called zz');
});

test('suggestions: values after the colon, counted, those in the list already left out, a not kept', () => {
  const values = valuesOf(rows);
  const at = (text, caret = text.length, names) => suggest(text, caret, values, names);
  const seam = at('seam: ');
  assert.deepEqual(seam.items.map((i) => `${i.label} ${i.detail}`), ['L 1', 'M 1', 'U 2']);
  assert.deepEqual([seam.from, seam.to], [6, 6]);
  assert.deepEqual(at('seam: M, ').items.map((i) => i.label), ['L', 'U'], 'M is in the list');
  const typed = at('seam: M, u');
  assert.deepEqual(typed.items.map((i) => i.insert), ['U']);
  assert.deepEqual(accept('seam: M, u', typed, typed.items[0]), { text: 'seam: M, U', caret: 10 });
  assert.deepEqual(at('Seam: !').items.map((i) => i.insert), ['!L', '!M', '!U'], 'the not kept, the name read case aside');
  // In the middle: the item the caret is in is replaced up to its comma, the rest kept.
  const middle = at('seam: m, L; strip: 3', 7);
  assert.deepEqual([middle.from, middle.to, middle.items.map((i) => i.insert)], [6, 7, ['M']]);
  assert.equal(accept('seam: m, L; strip: 3', middle, middle.items[0]).text, 'seam: M, L; strip: 3');
  // Numbers: in order, with their range and the ways to type one; a range or comparison typed offers nothing more.
  const strip = at('strip: ');
  assert.deepEqual(strip.items.map((i) => i.label), ['3', '9', '10']);
  assert.match(strip.hint, /^3 to 10: tick one, or type a range \(1-5\)/);
  assert.deepEqual(at('volume: <2').items, []);
  assert.match(at('strip: 3-5').hint, /^3 to 10/);
  assert.match(at('ROM\\Coal\\ash: ', undefined, ['ROM\\Coal\\ash']).hint, /^Type a range/);
  assert.equal(at('depth: ').hint, 'Nothing here is called depth');
});

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
