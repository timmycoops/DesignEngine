// The Reserves case tab's units table (Reserves case plan, decision 12): pieces of ground before and after a step,
// groups whose subtotals are the block's, a piece that leaves its group listed there in red, the field sets, a piece's
// journey and the field map, on a made-up block at rom_loss. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { addUp, changedFields, fieldMap, fieldsOfSet, groupName, journeyOf, lastWriter, piecesOf, rowChanged, unitGroups } from '../src/panels/caseUnits.ts';

const fields = [
  { name: 'volume', unit: 'm3', kind: 'volume', add_up: { by: 'sum' } },
  { name: 'thickness', unit: 'm', kind: 'thickness', add_up: { by: 'sum' } },
  { name: 'rd', unit: 't/m3', kind: 'density', add_up: { by: 'volume' } },
  { name: 'ash', unit: '%', kind: 'quality', add_up: { by: 'mass', density: 'rd' } },
  { name: 'ash_cutoff', unit: '%', kind: 'setting', add_up: { by: 'min' } },
];
const unit = (id, horizon, material, volume, more = {}) => ({
  id,
  horizon,
  material,
  seam: 'U',
  bench: null,
  joined: [],
  became: null,
  section: null,
  rom: null,
  ...more,
  values: { volume, thickness: volume / 1000, rd: material === 'coal' ? 1.4 : 2.3, ash: material === 'coal' ? 10 : null, ash_cutoff: 35, ...(more.values ?? {}) },
});

// B5 before rom_loss: OB, U coal 6,507 m³ in section 2, U-L waste. After: U keeps its id, less its losses, and four
// pieces split off (roof 834, edge 50, other 65, floor 512 m³) sent to the waste beside it.
const before = [unit(1, 'OB', 'waste', 50_000, { section: '1 waste A' }), unit(2, 'U', 'coal', 6_507, { section: '2 coal U' }), unit(3, 'U-L', 'waste', 30_000, { section: '3 waste L' })];
const loss = (id, kind, v, to) => unit(id, 'U', 'coal', v, { section: '2 coal U', became: `loss (${kind})`, rom: to });
const after = [
  before[0],
  loss(10, 'roof', 834, '1 waste A'),
  loss(11, 'edge', 50, '1 waste A'),
  loss(12, 'other', 65, '1 waste A'),
  unit(2, 'U', 'coal', 6_507 - 834 - 50 - 65 - 512, { section: '2 coal U' }),
  loss(13, 'floor', 512, '3 waste L'),
  before[2],
];
const steps = [
  { step: 'as input', units: before, fields_read: [] },
  { step: 'rom_loss', units: after, fields_read: ['thickness', 'volume'] },
];

const close = (a, b) => Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(b));

test("a block's pieces: U kept its id, four pieces split off it, and they add to the 6,507 m³ it was", () => {
  const rows = piecesOf(before, after);
  assert.deepEqual(
    rows.map((r) => [r.how, r.after?.id ?? null, r.before?.id ?? null]),
    [
      ['kept', 1, 1],
      ['kept', 2, 2],
      ['split', 10, null],
      ['split', 11, null],
      ['split', 12, null],
      ['split', 13, null],
      ['kept', 3, 3],
    ],
  );
  const of = rows.filter((r) => r.before?.id === 2 || r.other?.id === 2);
  assert.ok(close(of.reduce((s, r) => s + r.after.values.volume, 0), 6_507));
});

test('a join: the benches go into the joined unit, which is listed joined', () => {
  const b1 = unit(5, 'U', 'coal', 300, { bench: '1' });
  const b2 = unit(6, 'U', 'coal', 200, { bench: '2' });
  const joined = unit(7, 'U', 'coal', 500, { joined: [['1', 0.6], ['2', 0.4]] });
  assert.deepEqual(
    piecesOf([b1, b2], [joined]).map((r) => [r.how, r.before?.id ?? null, r.after?.id ?? null, r.other?.id ?? null]),
    [
      ['joined', null, 7, 5],
      ['gone', 5, null, 7],
      ['gone', 6, null, 7],
    ],
  );
});

test("a joined unit's pieces: split off it with its joins, they're its pieces, not new ground", () => {
  const joins = [['1', 0.6], ['2', 0.4]];
  const u = unit(4, 'U', 'coal', 1_292.5, { joined: joins, section: '2 coal U' });
  const pieces = [unit(4, 'U', 'coal', 833.8, { joined: joins, section: '2 coal U' }), unit(20, 'U', 'coal', 458.7, { joined: joins, section: '2 coal U', became: 'loss (roof)', rom: '1 waste A' })];
  const rows = piecesOf([u], pieces);
  assert.deepEqual(
    rows.map((r) => [r.how, r.after.id, r.other?.id ?? null]),
    [
      ['kept', 4, null],
      ['split', 20, 4],
    ],
  );
  assert.ok(close(rows.reduce((s, r) => s + r.after.values.volume, 0), 1_292.5));
  assert.deepEqual(unitGroups(rows, 'section', fields)[0].rows.map((r) => r.left), [false, true], 'the loss piece listed red where it left');
});

test("by working section, the groups' subtotals are the block's before and after; pieces that left are listed where they were", () => {
  const rows = piecesOf(before, after);
  const groups = unitGroups(rows, 'section', fields);
  assert.deepEqual(
    groups.map((g) => g.name),
    ['1 waste A', '2 coal U', '3 waste L'],
  );
  const all = (side) => addUp(fields, side);
  for (const side of ['before', 'after']) {
    const sum = groups.reduce((s, g) => s + (g[side].volume ?? 0), 0);
    assert.ok(close(sum, all(side === 'before' ? before : after).volume), side);
  }
  const coal = groups[1];
  assert.deepEqual(
    coal.rows.map((r) => [r.row.after.id, r.left]),
    [
      [2, false],
      [10, true],
      [11, true],
      [12, true],
      [13, true],
    ],
    'the loss pieces listed red in the section they left',
  );
  assert.deepEqual(coal.counts, [1, 1]);
  assert.ok(close(coal.before.volume, 6_507) && close(coal.after.volume, 5_046));
  assert.ok(close(groups[0].after.volume, 50_000 + 834 + 50 + 65));
  // Qualities by mass, as the engine adds them: the coal's 10 %; where the waste weighs and has no ash, none, never a
  // made-up figure. The cut-off is the smallest.
  assert.equal(coal.after.ash, 10);
  assert.equal(groups[0].after.ash, null);
  assert.equal(addUp(fields, before).ash_cutoff, 35);
  assert.equal(groupName(after[1], 'material'), 'coal');
  assert.equal(unitGroups(rows, 'none', fields).length, 1);
});

test('the field sets: This step what it reads and changes with thickness and volume; the rest by kind; a planner’s by name', () => {
  const rows = piecesOf(before, after);
  assert.deepEqual(changedFields(rows, fields), ['volume', 'thickness']);
  assert.deepEqual(fieldsOfSet('This step', fields, steps[1], rows), ['volume', 'thickness']);
  assert.deepEqual(fieldsOfSet('Quantities', fields, null, rows), ['volume', 'thickness', 'rd']);
  assert.deepEqual(fieldsOfSet('Qualities', fields, null, rows), ['ash']);
  assert.deepEqual(fieldsOfSet('Class and audit', fields, null, rows), ['ash_cutoff']);
  assert.deepEqual(fieldsOfSet('Haul', fields, null, rows, [{ name: 'Haul', fields: ['rd', 'nothing', 'volume'] }]), ['rd', 'volume']);
  assert.equal(rowChanged(rows[0], ['volume']), false);
  assert.equal(rowChanged(rows[1], ['volume']), true);
  assert.equal(rowChanged(rows[2], []), true, 'a piece split off changed');
});

test("a piece's journey, the last step to write each field, and the field map", () => {
  assert.deepEqual(journeyOf(steps, 1, 2), [
    { step: 0, name: 'as input', what: 'read from the block, 6,507 m³' },
    { step: 1, name: 'rom_loss', what: '6,507 → 5,046 m³' },
  ]);
  assert.deepEqual(journeyOf(steps, 1, 10), [{ step: 1, name: 'rom_loss', what: 'made here, 834 m³, loss (roof)' }]);
  assert.equal(lastWriter(steps, 1, 2, 'volume'), 'rom_loss');
  assert.equal(lastWriter(steps, 1, 2, 'rd'), null, 'as read');
  const map = fieldMap(steps, fields);
  assert.deepEqual([...map[0].reads], ['thickness', 'volume']);
  assert.deepEqual([...map[0].writes].sort(), ['thickness', 'volume']);
});
