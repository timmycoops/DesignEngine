// The Parameters tab's grid (decision 0075, 5d): a parameter table to a grid and back, what stops a save, edits shown,
// rows and columns moved, and cells pasted from Excel. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  addColumn,
  cellEdited,
  columnEdited,
  copyText,
  copyTextTurned,
  deleteRows,
  fromGrid,
  gridEdited,
  insertRow,
  moveColumn,
  moveRow,
  paste,
  pasteTurned,
  parseClipboard,
  setCell,
  toGrid,
  newWordName,
  withAddUp,
  withFrom,
  wordFrom,
  withTable,
} from '../src/panels/parametersGrid.ts';

// As the engine's CSV test: seam U in strips 1-5 has a 30 % cut-off; U and M a waste RD of 2.4; the default row below.
const table = {
  filters: ['seam', 'strip'],
  columns: [{ name: 'ash_cutoff', unit: '%' }, { name: 'waste_rd', unit: 't/m3' }, { name: 'loader', text: true }],
  rows: [
    { when: { seam: 'U', strip: '1-5' }, values: { ash_cutoff: 30 } },
    { when: { seam: 'U, M' }, values: { waste_rd: 2.4 } },
    { values: { ash_cutoff: 35, waste_rd: 2.3, loader: 'EX3600' } },
  ],
};
const col = (g, name) => g.columns.find((c) => c.name === name).key;

test('a table reads into a grid and back unchanged', () => {
  const g = toGrid(table);
  assert.deepEqual(g.columns.map((c) => `${c.kind}:${c.name}`), ['filter:seam', 'filter:strip', 'value:ash_cutoff', 'value:waste_rd', 'value:loader']);
  assert.equal(g.rows[1].cells[col(g, 'seam')], 'U, M');
  assert.equal(g.rows[2].cells[col(g, 'waste_rd')], '2.3');
  const back = fromGrid(g);
  assert.deepEqual(back.problems, []);
  assert.deepEqual(back.table, table);
  assert.equal(gridEdited(g, 3, g.columns.map((c) => c.key)), false);
});

test('a word in a number column, a column with no name or named twice, stop the save; blank rows are left out', () => {
  let g = toGrid(table);
  g = setCell(g, g.rows[0].key, col(g, 'ash_cutoff'), '3O');
  g = insertRow(g);
  g = addColumn(g, 'value');
  g = { ...g, columns: g.columns.map((c) => (c.name === 'value' ? { ...c, name: 'Ash_Cutoff' } : c)) };
  const { problems, blankRows } = fromGrid(g);
  assert.deepEqual(problems.map((p) => p.why), ['Ash_Cutoff is named twice', "ash_cutoff holds numbers, not '3O'"]);
  assert.equal(blankRows, 1);
  // Words are fine in a words column, and a number in one stays a word.
  const h = setCell(toGrid(table), toGrid(table).rows[0].key, 'x', '');
  assert.deepEqual(fromGrid(h).problems, []);
});

test('an edited cell shows as edited wherever its row and column move; a new row or column is all edited', () => {
  let g = toGrid(table);
  const saved = g.columns.map((c) => c.key);
  const ash = col(g, 'ash_cutoff');
  g = setCell(g, g.rows[2].key, ash, '36');
  g = moveRow(g, g.rows[2].key, -1);
  assert.equal(g.rows[1].saved, 2);
  assert.ok(cellEdited(g.rows[1], g.columns[2]));
  assert.ok(!cellEdited(g.rows[2], g.columns[2]), 'a row moved, not edited');
  g = moveColumn(g, col(g, 'loader'), -1);
  assert.deepEqual(g.columns.map((c) => c.name), ['seam', 'strip', 'ash_cutoff', 'loader', 'waste_rd']);
  assert.equal(moveColumn(g, col(g, 'ash_cutoff'), -1), g, 'values stay after the filters');
  g = insertRow(g, 0);
  assert.ok(!cellEdited(g.rows[0], g.columns[0]), 'a blank new cell is nothing to show');
  g = setCell(g, g.rows[0].key, col(g, 'seam'), 'L');
  assert.ok(cellEdited(g.rows[0], g.columns[0]));
  g = addColumn(g, 'filter');
  assert.deepEqual(g.columns.map((c) => c.name).slice(0, 3), ['seam', 'strip', 'attribute']);
  assert.ok(columnEdited(g.columns[2]));
  assert.ok(gridEdited(g, 3, saved));
  // Its saved table: the new row first, the default row second, as now ordered.
  g = deleteRows(g, new Set([g.rows[0].key]));
  g = { ...g, columns: g.columns.filter((c) => c.name !== 'attribute') };
  assert.deepEqual(fromGrid(g).table.rows[1].values, { ash_cutoff: 36, waste_rd: 2.3, loader: 'EX3600' });
});

test('cells pasted from Excel fill from the cell picked, adding rows; cells past the last column are counted', () => {
  const block = parseClipboard('M\t6-9\t31\r\n"L, M"\t\t\t2.25\r\n');
  assert.deepEqual(block, [['M', '6-9', '31'], ['L, M', '', '', '2.25']]);
  assert.deepEqual(parseClipboard('"a\tb"\t"say ""hi"""\nc'), [['a\tb', 'say "hi"'], ['c']]);
  const g = toGrid(table);
  const { grid, cut } = paste(g, g.rows[2].key, col(g, 'seam'), block);
  assert.equal(cut, 0);
  assert.equal(grid.rows.length, 4);
  assert.deepEqual(fromGrid(grid).table.rows.slice(2), [
    { when: { seam: 'M', strip: '6-9' }, values: { ash_cutoff: 31, waste_rd: 2.3, loader: 'EX3600' } },
    { when: { seam: 'L, M' }, values: { waste_rd: 2.25 } },
  ]);
  assert.equal(paste(g, g.rows[0].key, col(g, 'loader'), [['a', 'b', 'c']]).cut, 2);
  assert.equal(copyText(g, new Set([g.rows[1].key])), 'when seam\twhen strip\tash_cutoff (%)\twaste_rd (t/m3)\tloader\nU, M\t\t\t2.4\t');
});

test('turned, as the tab shows it: a pasted line runs across the rows for one parameter, adding rows; copy runs the same way', () => {
  const g = toGrid(table);
  // Pasted at row 3's seam: seam across rows 3 and 4, then strip, then ash_cutoff with a cell left out.
  const block = parseClipboard('M\tL, M\n6-9\n31\t\n');
  const { grid, cut } = pasteTurned(g, g.rows[2].key, col(g, 'seam'), block);
  assert.equal(cut, 0);
  assert.equal(grid.rows.length, 4);
  assert.deepEqual(fromGrid(grid).table.rows.slice(2), [
    { when: { seam: 'M', strip: '6-9' }, values: { ash_cutoff: 31, waste_rd: 2.3, loader: 'EX3600' } },
    { when: { seam: 'L, M' }, values: {} },
  ]);
  // Three lines from loader, the last parameter: two lines past it are left out.
  assert.equal(pasteTurned(g, g.rows[0].key, col(g, 'loader'), [['a'], ['b'], ['c']]).cut, 2);
  assert.equal(copyTextTurned(g, new Set([g.rows[1].key, g.rows[2].key])), 'when seam\tU, M\t\nwhen strip\t\t\nash_cutoff (%)\t\t35\nwaste_rd (t/m3)\t2.4\t2.3\nloader\t\tEX3600');
});

test('a dataflow field changes where it comes from and how it adds up', () => {
  const ash = { name: 'ash', kind: 'quality', add_up: { by: 'volume' }, from: 'attribute', attribute: 'as' };
  assert.deepEqual(withFrom(ash, 'parameter'), { name: 'ash', kind: 'quality', add_up: { by: 'volume' }, from: 'parameter', column: 'as' });
  assert.deepEqual(withFrom(ash, 'worked'), { name: 'ash', kind: 'quality', add_up: { by: 'volume' }, from: 'worked' });
  assert.deepEqual(withAddUp(ash, 'mass', ['rd', 'rd_is']).add_up, { by: 'mass', density: 'rd' });
  assert.deepEqual(withAddUp({ ...ash, add_up: { by: 'mass', density: 'rd_is' } }, 'sum', ['rd']).add_up, { by: 'sum' });
});

// Word fields (Waste allocation plan, decision 2): where the words come from, and a new one's name.
test('a word field takes its attribute or parameter column from its name; a new one is process, then process_2', () => {
  const w = { name: 'method', from: 'worked' };
  assert.deepEqual(wordFrom(w, 'attribute'), { name: 'method', from: 'attribute', attribute: 'method' });
  assert.deepEqual(wordFrom(wordFrom(w, 'attribute'), 'parameter'), { name: 'method', from: 'parameter', column: 'method' });
  assert.deepEqual(wordFrom({ name: 'method', from: 'parameter', column: 'Method' }, 'worked'), { name: 'method', from: 'worked' });
  assert.equal(newWordName([]), 'process');
  assert.equal(newWordName([{ name: 'Process' }, { name: 'process_2' }]), 'process_3');
});

// The Reserves case tab's settings (Reserves case plan, decision 11): an edit typed there shows in the grid as edited.
test("a table edited in the Reserves case tab lays on the grid as saved: its cells edited, a column it adds new", () => {
  const saved = { filters: ['material'], columns: [{ name: 'roof_loss', unit: 'm' }], rows: [{ when: { material: 'waste' }, values: {} }, { when: { material: 'coal' }, values: { roof_loss: 0.1 } }] };
  const g = toGrid(saved);
  const savedColumns = g.columns.map((c) => c.key);
  const edited = { ...saved, columns: [...saved.columns, { name: 'edge_loss' }], rows: [saved.rows[0], { when: { material: 'coal' }, values: { roof_loss: 0.15, edge_loss: 0.05 } }] };
  const laid = withTable(g, edited);
  assert.deepEqual(laid.rows.map((r) => r.saved), [0, 1], 'the saved rows kept');
  assert.equal(gridEdited(laid, 2, savedColumns), true);
  assert.deepEqual(fromGrid(laid).table.rows[1].values, { roof_loss: 0.15, edge_loss: 0.05 });
  assert.equal(laid.columns.find((c) => c.name === 'edge_loss').was, null, 'a column added is new');
  // Put back as saved: not edited. Rows that don't line up: the table as it is.
  assert.equal(gridEdited(withTable(g, saved), 2, savedColumns), false);
  assert.equal(withTable(g, { ...saved, rows: [saved.rows[1]] }).rows.length, 1);
});
