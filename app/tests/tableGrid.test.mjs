// Any table as a grid (Tables and scripts plan, decision 1): opening and closing groups, sorting, cells and CSV. Run
// with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { calcForAi, cellText, changedWords, csvColumns, gridCsv, groupsKept, likelyKey, nextSort, pastedLines, readEntry, starterScript, toggleOpen } from '../src/panels/tableGrid.ts';

test('a group closed closes every group under it', () => {
  let open = toggleOpen([], ['3']);
  open = toggleOpen(open, ['3', 'B1']);
  open = toggleOpen(open, ['4']);
  assert.deepEqual(open, [['3'], ['3', 'B1'], ['4']]);
  assert.deepEqual(toggleOpen(open, ['3']), [['4']]);
  assert.deepEqual(toggleOpen(open, ['3', 'B1']), [['3'], ['4']]);
});

test('a header sorts up, then down, then back to the groups own order', () => {
  let sort = nextSort(null, 'volume');
  assert.deepEqual(sort, { column: 'volume', descending: false });
  sort = nextSort(sort, 'volume');
  assert.deepEqual(sort, { column: 'volume', descending: true });
  assert.equal(nextSort(sort, 'volume'), null);
  assert.deepEqual(nextSort(sort, 'rd'), { column: 'rd', descending: false });
});

test('cells show to their unit, words as they are, nothing as blank', () => {
  assert.equal(cellText(1234.56, 'm3'), '1,234.6');
  assert.equal(cellText(1.4567, 't/m3'), '1.457');
  assert.equal(cellText(12.345, '%'), '12.35');
  assert.equal(cellText(5, ''), '5');
  assert.equal(cellText(0.25, ''), '0.250');
  assert.equal(cellText('U', ''), 'U');
  assert.equal(cellText(null, 'm3'), '');
});

test('the grid as CSV keeps exact values and says each row', () => {
  const grid = {
    columns: [{ name: 'Insitu\\Coal\\volume', label: 'volume', folders: ['Insitu', 'Coal'], unit: 'm3', text: false, add_up: { by: 'sum' } }],
    every: [],
    groups: ['strip'],
    levels: ['strip', 'seam'],
    leaves: 2,
    cut: 0,
    rows: [
      { kind: 'total', path: [], label: 'All', depth: 0, leaves: 2, open: true, row: null, words: null, cells: [1000.125] },
      { kind: 'group', path: ['3'], label: '3', depth: 1, leaves: 2, open: true, row: null, words: null, cells: [1000.125] },
      { kind: 'leaf', path: ['3'], label: 'U', depth: 2, leaves: 1, open: false, row: 0, words: ['3', 'U'], cells: [null] },
    ],
  };
  assert.equal(gridCsv(grid), 'row,depth,label,leaves,Insitu\\Coal\\volume (m3)\r\ntotal,0,All,2,1000.125\r\ngroup,1,3,2,1000.125\r\nleaf,2,3 > U,1,');
});

test('group-by columns the table no longer has drop out', () => {
  assert.deepEqual(groupsKept(['strip', 'pit', 'seam'], ['strip', 'block', 'seam']), ['strip', 'seam']);
});

test('a typed value is read for its column: blank is nothing, words stay words, a number must be one', () => {
  assert.deepEqual(readEntry('  ', true), { value: null });
  assert.deepEqual(readEntry('1,234.5', true), { value: 1234.5 });
  assert.deepEqual(readEntry('-0.25', true), { value: -0.25 });
  assert.deepEqual(readEntry('soon', true), { error: "soon isn't a number" });
  assert.deepEqual(readEntry(' dragline first ', false), { value: 'dragline first' });
  assert.deepEqual(readEntry('12', false), { value: '12' });
});

test('a pasted column is a value a line, a spreadsheet row cut to its first cell', () => {
  assert.deepEqual(pastedLines('1.4\r\n1.5\r\n'), ['1.4', '1.5']);
  assert.deepEqual(pastedLines('a\tb\nc\td'), ['a', 'c']);
  assert.deepEqual(pastedLines('one'), ['one']);
});

test("what's changed since a table's run is said in a line", () => {
  assert.equal(changedWords({ blocks: 0, others: [], gone: 0 }), '');
  assert.equal(changedWords({ blocks: 2, others: [], gone: 0 }), "2 blocks changed since this table's run");
  assert.equal(changedWords({ blocks: 1, others: ['Reserves assumptions'], gone: 3 }), "1 block, Reserves assumptions and 3 inputs deleted changed since this table's run");
});

test("a CSV's columns are its header's names, units left off, a quoted comma kept", () => {
  assert.deepEqual(csvColumns('\uFEFFblock,seam,rd (t/m3),"note, long"\r\nB1,U,1.4,x\r\n'), ['block', 'seam', 'rd', 'note, long']);
  assert.deepEqual(csvColumns('\n\nstrip,zone\n3,east'), ['strip', 'zone']);
  assert.deepEqual(likelyKey(['Block', 'seam', 'rd'], ['pit', 'strip', 'block', 'seam', 'bench']), ['Block', 'seam']);
});

test('a new calculated column starts with a formula that sets it on the row', () => {
  assert.match(starterScript('strip_ratio'), /\n    row\.strip_ratio = 0\n$/);
  assert.match(starterScript('ROM ratio'), /\n    row\["ROM ratio"\] = 0\n$/);
  assert.match(starterScript('x'), /^def formula\(row, p\):/);
});

test('Copy for AI carries the names, the rule and the script, never a value', () => {
  const names = { fields: [['Insitu\\Coal\\tonnes', 't']], labels: ['strip', 'seam'], parameters: [['price', '$/t', false]] };
  const text = calcForAi('# How formulas are written', names, { name: 'strip_ratio', unit: 'm3/t', script: 'def formula(row):\n    row.strip_ratio = 1\n', adds: { by: 'from_inputs' } }, 'write');
  assert.ok(text.startsWith('# How formulas are written'));
  assert.ok(text.includes('Columns (numbers): Insitu\\Coal\\tonnes (t)'));
  assert.ok(text.includes('Words: strip, seam'));
  assert.ok(text.includes('Parameter columns: price ($/t)'));
  assert.ok(text.includes("`strip_ratio` in m3/t, worked again on a group's own inputs"));
  assert.ok(text.includes('row.strip_ratio = 1'));
});
