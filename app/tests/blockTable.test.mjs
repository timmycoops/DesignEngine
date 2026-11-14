// The block table (decision 0069): columns, the filter, the totals and CSV, on made-up blocks. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { attributeColumns, csvOf, densityColumns, shownRows, totalsOf } from '../src/panels/blockTable.ts';

const row = (name, volume, attributes) => ({ id: name, name, layer: 'blocks', volume, attributes });
// Two coal blocks of 72,000 m³ at RD 1.4 and 1.5, an overburden block and a sliver of 30 m³.
const rows = [
  row('B1_cu', 72000, { strip: 5, block: 'B1', seam: 'cu', material: 'coal', rd: 1.4, ash: 10 }),
  row('B1_cl', 72000, { strip: 5, block: 'B1', seam: 'cl', material: 'coal', rd: 1.5, ash: 20 }),
  row('B1_OB', 180000, { strip: 5, block: 'B1', seam: 'cu', material: 'waste' }),
  row('B2_OB "east"', 30, { strip: 6, block: 'B2', material: 'waste', note: 'a, b' }),
];

test("columns put the cut's own attributes first, then the rest by name, each marked numeric or not", () => {
  const columns = attributeColumns(rows);
  assert.deepEqual(columns.map((c) => c.key), ['strip', 'block', 'seam', 'material', 'ash', 'note', 'rd']);
  assert.deepEqual(columns.filter((c) => c.numeric).map((c) => c.key), ['strip', 'ash', 'rd']);
  assert.deepEqual(densityColumns(columns), ['rd']);
  assert.deepEqual(densityColumns([{ key: 'in_situ_density', numeric: true }, { key: 'RD', numeric: true }, { key: 'ash', numeric: true }]), ['RD', 'in_situ_density']);
});

test('the filter keeps rows with every word, and those under a volume', () => {
  assert.deepEqual(shownRows(rows, { words: 'coal b1', under: 0 }).map((r) => r.name), ['B1_cu', 'B1_cl']);
  assert.deepEqual(shownRows(rows, { words: '', under: 50 }).map((r) => r.name), ['B2_OB "east"']);
  assert.deepEqual(shownRows(rows, { words: 'waste', under: 100000 }).map((r) => r.name), ['B2_OB "east"']);
  assert.equal(shownRows(rows, { words: '  ', under: 0 }).length, 4);
});

test('totals: volume of every row shown, tonnes by the density over the rows that have one', () => {
  const coal = totalsOf(rows.slice(0, 2), 'rd');
  assert.equal(coal.count, 2);
  assert.equal(coal.volume, 144000);
  assert.ok(Math.abs(coal.tonnes - 208800) < 1e-9, String(coal.tonnes));
  const all = totalsOf(rows, 'rd');
  assert.equal(all.volume, 324030);
  assert.equal(all.weighed, 2);
  assert.equal(totalsOf(rows, null).tonnes, null);
  assert.equal(totalsOf(rows.slice(2), 'rd').tonnes, null, 'no row with a density: no tonnes');
});

test('CSV holds exact values, quoted where they need it', () => {
  const csv = csvOf(rows.slice(3), [{ key: 'strip' }, { key: 'note' }, { key: 'rd' }]).split('\r\n');
  assert.equal(csv[0], 'name,layer,volume_m3,strip,note,rd');
  assert.equal(csv[1], '"B2_OB ""east""",blocks,30,6,"a, b",');
});
