// Any table as a grid (Tables and scripts plan, decision 1): opening and closing groups, sorting, cells and CSV. Run
// with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { cellText, gridCsv, groupsKept, nextSort, toggleOpen } from '../src/panels/tableGrid.ts';

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
