// The reserves table's fields as rows (decision 0075): rows by folder, words for fields, CSV and where a picked leaf
// shows, on a made-up breakdown. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { breakdownCsv, cellText, decimals, fieldLabel, fieldRows, isDetail, isolation, nodeForLeaf } from '../src/panels/reservesTable.ts';

const field = (column, unit, values, words = null) => {
  const parts = column.split('\\');
  return { column, folders: parts.slice(0, -1), name: parts[parts.length - 1], unit, values, words };
};
// Block B1: its total and seams U and L.
const b = {
  levels: ['pit', 'strip', 'block', 'seam', 'bench'],
  node: ['Pit', '3', 'B1'],
  columns: [
    { label: 'B1', path: ['Pit', '3', 'B1'], depth: 0, leaves: 3, opens: true, open: true },
    { label: 'U', path: ['Pit', '3', 'B1', 'U'], depth: 1, leaves: 2, opens: true, open: false },
    { label: 'L', path: ['Pit', '3', 'B1', 'L'], depth: 1, leaves: 1, opens: true, open: false },
  ],
  fields: [
    field('Imported\\Coal\\volume', 'm3', [3000, 1000, 2000]),
    field('Imported\\Coal\\rd', 't/m3', [1.45, 1.4, 1.475]),
    field('Imported\\Waste\\volume', 'm3', [9000.25, 9000.25, null]),
    field('ROM\\section', '', [null, null, null], ['U; L', 'U', 'L']),
    field('ROM\\Coal\\as_rom', '', [15.123, 16, 14.5]),
  ],
};

test('rows run by top folder then sub-folder, fields straight under a top folder first, details left out unless asked', () => {
  const rows = fieldRows(b, false, new Set());
  assert.deepEqual(rows.map((r) => `${r.kind}:${r.label}`), ['folder:Imported', 'folder:Coal', 'field:volume', 'folder:Waste', 'field:volume', 'folder:ROM', 'field:section', 'folder:Coal', 'field:as, ROM basis']);
  assert.ok(fieldRows(b, true, new Set()).some((r) => r.kind === 'field' && r.key === 'Imported\\Coal\\rd'), 'every field when asked');
  const folded = fieldRows(b, false, new Set(['Imported', 'ROM\\Coal']));
  assert.deepEqual(folded.map((r) => r.label), ['Imported', 'ROM', 'section', 'Coal']);
});

test('fields are worded and shown to their unit', () => {
  assert.equal(fieldLabel('as_is'), 'as, in situ');
  assert.equal(fieldLabel('tonnes_ad'), 'tonnes, air dried');
  assert.equal(fieldLabel('volume'), 'volume');
  assert.ok(isDetail('rd_rom') && !isDetail('thickness'));
  assert.equal(decimals('t/m3'), 3);
  assert.equal(cellText(b.fields[2], 0), '9,000.3');
  assert.equal(cellText(b.fields[2], 2), '');
  assert.equal(cellText(b.fields[3], 0), 'U; L');
  assert.equal(cellText(b.fields[4], 0), '15.12');
});

test('CSV keeps exact values, a column a node', () => {
  const csv = breakdownCsv(b).split('\n');
  assert.equal(csv[0], 'field,unit,Pit > 3 > B1,Pit > 3 > B1 > U,Pit > 3 > B1 > L');
  assert.equal(csv[3], 'Imported\\Waste\\volume,m3,9000.25,9000.25,');
  assert.equal(csv[4], 'ROM\\section,,U; L,U,L');
});

test('a picked leaf shows on its block, its seam opened', () => {
  assert.deepEqual(nodeForLeaf(b.levels, ['Pit', '3', 'B1', 'U', '110']), { node: ['Pit', '3', 'B1'], open: [['Pit', '3', 'B1', 'U']] });
  assert.deepEqual(nodeForLeaf(['strip', 'block'], ['3', 'B1']), { node: ['3'], open: [['3', 'B1']] });
});

test("a block's dip reads in degrees, its geometry with the detail fields (decision 0086)", async () => {
  const { unitLabel, isDetail } = await import('../src/panels/reservesTable.ts');
  assert.equal(unitLabel('deg'), '°');
  assert.ok(['width', 'dip', 'centroid_z'].every(isDetail) && !isDetail('thickness'));
});

test("Isolate its solids: their layers alone, the rest on them hidden, and only what it changed put back", () => {
  const objects = [
    { id: 'a1', layer: 'blocks', kind: 'solid', hidden: false },
    { id: 'a2', layer: 'blocks', kind: 'solid', hidden: true },
    { id: 'b1', layer: 'blocks', kind: 'solid', hidden: false },
    { id: 'b2', layer: 'blocks', kind: 'solid', hidden: true },
    { id: 'c1', layer: 'blocks2', kind: 'solid', hidden: false },
    { id: 't', layer: 'blocks', kind: 'table', hidden: false },
    { id: 'g', layer: 'grids', kind: 'grid', hidden: false },
  ];
  const layers = [
    { id: 'blocks', visible: true },
    { id: 'blocks2', visible: false },
    { id: 'grids', visible: true },
    { id: 'toes', visible: false },
  ];
  const got = isolation(['a1', 'a2', 'c1'], objects, layers);
  assert.deepEqual(got.showLayers, ['blocks', 'blocks2']);
  // Only layers that were on go off; a layer already off is left alone.
  assert.deepEqual(got.hideLayers, ['grids']);
  // b2 was hidden already, so it isn't in what's hidden (nor shown again when put back); the table isn't drawn.
  assert.deepEqual(got.hide, ['b1']);
  assert.deepEqual(got.unhide, ['a2']);
});
