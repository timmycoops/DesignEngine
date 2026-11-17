// The column notebook (decision 0081): the focus unit and following it through a join, what a cell caused, a read in
// words, a formula's lines from its trace, on a made-up column. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { cellOut, cellSummaries, codeLines, defaultFocus, focusAt, readsOnce, readWords, shortValue, summaryWords, whenWords } from '../src/panels/notebook.ts';

const fields = [
  { name: 'volume', unit: 'm3', kind: 'volume' },
  { name: 'rd', unit: 't/m3', kind: 'density' },
  { name: 'rd_is', unit: 't/m3', kind: 'density' },
];
const unit = (id, horizon, material, bench, values, more = {}) => ({ id, horizon, material, seam: 'U', bench, joined: [], became: null, section: null, rom: null, values, ...more });
const cell = (step, units, more = {}) => ({ step, units, said: [], kind: 'built_in', rule: '', when: {}, source: null, fits: [], reads: [], ...more });

// As input: OB and U's benches 130 and 120. densities: rd_is set on both benches. join_benches: the benches go, one U.
// small_volumes: nothing.
const ob = (rd_is = null) => unit(1, 'OB', 'waste', null, { volume: 50000, rd: 2.3, rd_is });
const steps = [
  cell('as input', [ob(), unit(2, 'U', 'coal', '130', { volume: 10000, rd: 1.4, rd_is: null }), unit(3, 'U', 'coal', '120', { volume: 10000, rd: 1.6, rd_is: null })], { kind: 'input' }),
  cell('densities', [ob(), unit(2, 'U', 'coal', '130', { volume: 10000, rd: 1.4, rd_is: 1.37 }), unit(3, 'U', 'coal', '120', { volume: 10000, rd: 1.6, rd_is: 1.55 })], { kind: 'formula', when: { material: 'coal' }, source: 'def formula(unit):\n    if unit.material != "coal":\n        return\n    unit.rd_is = in_situ_rd(unit.rd)\n' }),
  cell('join_benches', [ob(), unit(4, 'U', 'coal', null, { volume: 20000, rd: 1.5, rd_is: 1.46 }, { joined: [['130', 0.5], ['120', 0.5]] })]),
  cell('small_volumes', [ob(), unit(4, 'U', 'coal', null, { volume: 20000, rd: 1.5, rd_is: 1.46 }, { joined: [['130', 0.5], ['120', 0.5]] })], {
    reads: [
      { unit: 1, column: 'min_volume', value: null, text: null, table: null, row: null },
      { unit: 4, column: 'min_volume', value: 100, text: null, table: 0, row: 2 },
    ],
  }),
];

test('the focus is the picked bench, else the first coal, and follows it into the join', () => {
  assert.equal(defaultFocus(steps, { seam: 'U', bench: '120' }).id, 3);
  assert.equal(defaultFocus(steps, null).id, 2, 'the first coal, not OB');
  const f = defaultFocus(steps, { seam: 'U', bench: '120' });
  assert.equal(focusAt(steps[1].units, f).id, 3, 'itself while it is there');
  assert.equal(focusAt(steps[2].units, f).id, 4, 'then the joined unit that holds its bench');
  // A loss cut from the joined unit holds the same benches and comes first in the column: the unit itself is followed.
  const loss = unit(5, 'U', 'coal', null, { volume: 300, rd: 1.5, rd_is: 1.46 }, { joined: [['130', 0.5], ['120', 0.5]], became: 'loss (roof)' });
  assert.equal(focusAt([loss, ...steps[2].units], f).id, 4, 'not the loss piece cut from it');
});

test('a cell shows only the units it touched and the fields it changed', () => {
  const out = cellOut(fields, steps, 1);
  assert.deepEqual(out.units.map((u) => [u.label, u.isNew]), [['U 130', false], ['U 120', false]], 'OB untouched');
  assert.deepEqual(out.rows.map((r) => r.field.name), ['rd_is']);
  assert.deepEqual(out.rows[0].cells.map((c) => [c.before, c.value, c.changed]), [[null, 1.37, true], [null, 1.55, true]]);
  assert.deepEqual([out.volumeIn, out.volumeOut], [70000, 70000]);
  const join = cellOut(fields, steps, 2);
  assert.deepEqual(join.units.map((u) => [u.label, u.isNew]), [['U (2 benches)', true]]);
  assert.deepEqual(join.gone, ['U 130', 'U 120']);
  assert.deepEqual(join.rows.map((r) => r.field.name), ['volume', 'rd', 'rd_is'], 'a new unit shows every field it has');
  assert.equal(cellOut(fields, steps, 0).units.length, 0, 'the column as input caused nothing');
});

test('quiet steps fold, the rest say what they did', () => {
  const s = cellSummaries(steps);
  assert.deepEqual(s.map((x) => x.quiet), [false, false, false, true]);
  assert.equal(summaryWords(s[1]), '2 values changed');
  assert.equal(summaryWords(s[2]), '1 unit made · 2 taken away');
  assert.equal(summaryWords(s[3]), 'nothing changed in this column');
});

test('a read says its value and the table and column it came from, or that no row gives it', () => {
  const [none, given] = steps[3].reads;
  assert.deepEqual(readWords(given, ['assumptions'], 'm3'), { name: 'min_volume', value: '100.0', from: 'assumptions, column 3' });
  assert.deepEqual(readWords(none, ['assumptions']), { name: 'min_volume', value: 'none', from: 'no row gives it' });
  assert.equal(whenWords({ material: 'coal' }), 'material coal');
  assert.equal(whenWords({}), 'every unit');
});

test("a formula's lines are marked by the trace, a loop's line by its last run", () => {
  const trace = { block: '', unit: '', fits: true, before: [], reads: [], sets: [], said: [], stopped: null, lines: [
    { line: 2, text: '', values: [] },
    { line: 4, text: '', values: [['rd_is', '1.1']] },
    { line: 4, text: '', values: [['rd_is', '1.37']] },
  ] };
  const lines = codeLines(steps[1].source, trace);
  assert.deepEqual(lines.slice(0, 4).map((l) => [l.n, l.ran]), [[1, 0], [2, 1], [3, 0], [4, 2]]);
  assert.deepEqual(lines[3].values, [['rd_is', '1.37']]);
  assert.equal(codeLines(steps[1].source, null).every((l) => l.ran === 0), true, 'nothing ran without a trace');
});

test("what a formula read shows each name once, long decimals cut", () => {
  assert.deepEqual(readsOnce([['unit.rd', '1.4140115813035563'], ['unit.mad', '3.5'], ['unit.rd', '1.4140115813035563'], ['unit.material', '"coal"']]), [
    ['unit.rd', '1.414'],
    ['unit.mad', '3.5'],
    ['unit.material', '"coal"'],
  ]);
  assert.equal(shortValue('1.38722939824051480'), '1.3872');
  assert.equal(shortValue('2.3'), '2.3', 'short numbers as they are');
  assert.equal(shortValue('[]'), '[]');
});
