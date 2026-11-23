// The reserves table's fields as rows (decision 0075): rows by folder, words for fields, CSV and where a picked leaf
// shows, on a made-up breakdown. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { breakdownCsv, cellText, decimals, densityOfTonnes, fieldLabel, fieldRows, isDetail, asPutBack, isolation, nodeForLeaf } from '../src/panels/reservesTable.ts';

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

// Waste allocation plan, decision 5: schedule fields sit under Scheduled, a section a process, after its waste, each
// section read out with its folders; a words column there shows its words.
test('schedule fields show under Scheduled, a section a process', () => {
  const node = {
    ...b,
    fields: [
      field('Scheduled\\Waste\\volume', 'm3', [50000, 50000, null]),
      field('Scheduled\\Waste\\Dragline\\Prime Volume', 'm3', [40000, 40000, 0]),
      field('Scheduled\\Waste\\Dragline\\Total Volume', 'm3', [48000, 48000, 0]),
      field('Scheduled\\Waste\\Drill and Blast\\Volume', 'm3', [10000, 0, 10000]),
      field('Scheduled\\Waste\\Dragline\\Method', '', [null, null, null], ['Strip', 'Strip', '']),
    ],
  };
  const rows = fieldRows(node, false, new Set());
  assert.deepEqual(rows.map((r) => `${r.kind}:${r.label}`), ['folder:Scheduled', 'folder:Waste', 'field:volume', 'folder:Waste › Dragline', 'field:Prime Volume', 'field:Total Volume', 'field:Method', 'folder:Waste › Drill and Blast', 'field:Volume']);
  const prime = rows.find((r) => r.kind === 'field' && r.key === 'Scheduled\\Waste\\Dragline\\Prime Volume');
  assert.equal(prime.kind === 'field' && cellText(prime.field, 0), '40,000.0');
  const method = rows.find((r) => r.kind === 'field' && r.label === 'Method');
  assert.equal(method.kind === 'field' && cellText(method.field, 1), 'Strip');
});

// App review brief, 17 Nov, item 2: under each folder, Main fields shows the density its tonnes are on, so volume × it is
// the tonnes shown (B1's coal, from the brief: lab RD 1.444 under Imported, in situ 1.415 under Insitu, and the ROM
// density under ROM), never the air-dried tonnes a cubic metre, which an older table called rd_adt.
test('Main fields shows beside each folder its own density, the one its tonnes are on', () => {
  const v = 1267.8;
  const node = {
    ...b,
    fields: [
      field('Imported\\Coal\\volume', 'm3', [v]),
      field('Imported\\Coal\\rd', 't/m3', [1.444]),
      field('Imported\\Coal\\rd_adt', 't/m3', [1.444]),
      field('Imported\\Coal\\tonnes', 't', [v * 1.444]),
      field('Insitu\\Coal\\volume', 'm3', [v]),
      field('Insitu\\Coal\\rd', 't/m3', [1.444]),
      field('Insitu\\Coal\\rd_is', 't/m3', [1.415]),
      field('Insitu\\Coal\\rd_adt', 't/m3', [1.349]),
      field('Insitu\\Coal\\tonnes', 't', [v * 1.415]),
      field('Insitu\\Coal\\tonnes_ad', 't', [v * 1.349]),
      field('ROM\\Coal\\volume', 'm3', [v]),
      field('ROM\\Coal\\rd_is', 't/m3', [1.415]),
      field('ROM\\Coal\\rd_rom', 't/m3', [1.39]),
      field('ROM\\Coal\\tonnes', 't', [v * 1.39]),
      field('Insitu\\Waste\\volume', 'm3', [500]),
    ],
  };
  const rows = fieldRows(node, false, new Set()).filter((r) => r.kind === 'field');
  const shown = rows.map((r) => r.key);
  assert.deepEqual(
    shown.filter((k) => /\\(rd|rd_is|rd_rom|rd_adt|tonnes_ad)$/.test(k)),
    ['Imported\\Coal\\rd', 'Insitu\\Coal\\rd_is', 'ROM\\Coal\\rd_rom'],
    'one density a folder, the one its tonnes are on; no rd_adt or air-dried tonnes in Main fields',
  );
  for (const folder of ['Imported\\Coal', 'Insitu\\Coal', 'ROM\\Coal']) {
    const value = (name) => node.fields.find((f) => f.column === `${folder}\\${name}`)?.values[0];
    const density = rows.find((r) => r.key.startsWith(`${folder}\\rd`)).field.values[0];
    assert.ok(Math.abs(value('volume') * density - value('tonnes')) <= 1e-9 * value('tonnes'), folder);
  }
  assert.equal(densityOfTonnes(['Insitu', 'Waste'], ['volume']), null, 'no tonnes, no density shown');
  assert.equal(fieldLabel('rd_adt'), 'tonnes, air dried, per m³', 'an older table reads as it is named now');
  assert.equal(fieldLabel('tonnes_ad_per_m3'), 'tonnes, air dried, per m³');
  assert.ok(isDetail('rd_adt') && isDetail('tonnes_ad_per_m3'));
  assert.ok(fieldRows(node, true, new Set()).some((r) => r.key === 'Insitu\\Coal\\rd_adt'), 'Every field still shows it');
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

test('Show then Isolate: Isolate starts from the screen as Show found it, so the layers Show put back go off again', () => {
  const objects = [
    { id: 'a1', layer: 'blocks', kind: 'solid', hidden: false },
    { id: 'b1', layer: 'blocks', kind: 'solid', hidden: false },
    { id: 't1', layer: 'toes', kind: 'string', hidden: false },
  ];
  const before = [
    { id: 'blocks', visible: true },
    { id: 'toes', visible: true },
    { id: 'grids', visible: false },
  ];
  // Show: blocks alone. The screen now has toes off; what Show keeps to put back is `before`.
  const duringShow = [
    { id: 'blocks', visible: true },
    { id: 'toes', visible: false },
    { id: 'grids', visible: false },
  ];
  const start = asPutBack(objects, duringShow, { layers: before, changed: null });
  assert.deepEqual(start.layers, before, 'the toes come back on with Show put back');
  const iso = isolation(['a1'], start.objects, start.layers);
  assert.deepEqual(iso.hideLayers, ['toes'], 'so Isolate turns them off again');
  assert.deepEqual(iso.hide, ['b1']);

  // Isolate, then Isolate another node: b1 was hidden by the first, and is shown again before the second works out its own.
  const afterIso = objects.map((o) => (o.id === 'b1' ? { ...o, hidden: true } : o));
  const again = asPutBack(afterIso, duringShow, { layers: before, changed: iso });
  assert.equal(again.objects.find((o) => o.id === 'b1').hidden, false);
  assert.deepEqual(isolation(['b1'], again.objects, again.layers).hide, ['a1'], 'the second hides a1, and b1 is its own');
  assert.equal(asPutBack(objects, before, null).layers, before, 'nothing alone: the screen as it is');
});
