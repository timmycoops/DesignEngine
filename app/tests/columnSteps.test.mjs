// Open its column (decision 0075, 5b): a block's column after a step set beside the step before, units matched by id,
// on a made-up column. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { stepActivity, stepGrid, unitWords, valueText } from '../src/panels/columnSteps.ts';

const fields = [
  { name: 'volume', unit: 'm3', kind: 'volume' },
  { name: 'rd', unit: 't/m3', kind: 'density' },
  { name: 'ash', unit: '', kind: 'quality' },
  { name: 'ash_cutoff', unit: '', kind: 'setting' },
];
const unit = (id, horizon, bench, values, more = {}) => ({ id, horizon, material: horizon === 'OB' ? 'waste' : 'coal', seam: 'U', bench, joined: [], became: null, section: null, rom: null, values, ...more });

// As input: OB and U's benches 18 and 16. Initialise: nothing changes but the cut-off set on U's benches. Join: the
// benches go, one U of both comes, at RD 1.5.
const steps = [
  { step: 'as input', said: [], units: [unit(1, 'OB', null, { volume: 50000, rd: 2.3, ash: null, ash_cutoff: null }), unit(2, 'U', '18', { volume: 10000, rd: 1.4, ash: 10, ash_cutoff: null }), unit(3, 'U', '16', { volume: 10000, rd: 1.6, ash: 10, ash_cutoff: null })] },
  { step: 'initialise', said: [], units: [unit(1, 'OB', null, { volume: 50000, rd: 2.3, ash: null, ash_cutoff: null }), unit(2, 'U', '18', { volume: 10000, rd: 1.4, ash: 10, ash_cutoff: 30 }), unit(3, 'U', '16', { volume: 10000, rd: 1.6, ash: 10, ash_cutoff: 30 })] },
  {
    step: 'join_benches',
    said: [],
    units: [unit(1, 'OB', null, { volume: 50000, rd: 2.3, ash: null, ash_cutoff: null }), unit(4, 'U', null, { volume: 20000, rd: 1.5, ash: 10, ash_cutoff: 30 }, { joined: [['18', 0.5], ['16', 0.5]] })],
  },
];

test('the column as input has nothing before it: nothing changed, nothing new', () => {
  const g = stepGrid(fields, steps, 0, 'all', null);
  assert.deepEqual(g.units.map((u) => u.label), ['OB', 'U 18', 'U 16']);
  assert.equal(g.changes, 0);
  assert.ok(g.units.every((u) => !u.isNew));
  assert.deepEqual(g.gone, []);
});

test('a value a step sets is changed, with what it was', () => {
  const g = stepGrid(fields, steps, 1, 'all', null);
  const cutoff = g.rows.find((r) => r.field.name === 'ash_cutoff');
  assert.deepEqual(cutoff.cells.map((c) => c.changed), [false, true, true]);
  assert.deepEqual(cutoff.cells.map((c) => c.before), [null, null, null]);
  assert.equal(g.changes, 2);
  // Only what changed: the cut-off alone.
  assert.deepEqual(stepGrid(fields, steps, 1, 'changed', null).rows.map((r) => r.field.name), ['ash_cutoff']);
  // Main fields leave out RD, unless a step changed it.
  assert.deepEqual(stepGrid(fields, steps, 1, 'main', null).rows.map((r) => r.field.name), ['volume', 'ash', 'ash_cutoff']);
});

test('a join takes the benches away and makes one new unit, matched by id', () => {
  const g = stepGrid(fields, steps, 2, 'all', null);
  assert.deepEqual(g.units.map((u) => [u.label, u.isNew]), [['OB', false], ['U (2 benches)', true]]);
  assert.deepEqual(g.gone, ['U 18', 'U 16']);
  const volume = g.rows.find((r) => r.field.name === 'volume');
  assert.equal(volume.cells[1].before, undefined, 'a new unit has no before');
  assert.equal(g.changes, 0, 'OB is as it was');
  // A new unit's values count as the step's doing, so its fields show under Only what changed.
  assert.deepEqual(stepGrid(fields, steps, 2, 'changed', null).rows.map((r) => r.field.name), ['volume', 'rd', 'ash', 'ash_cutoff']);
});

test('the bench picked marks its unit, and the joined unit that holds it', () => {
  assert.deepEqual(stepGrid(fields, steps, 1, 'all', { seam: 'U', bench: '16' }).units.map((u) => u.picked), [false, false, true]);
  assert.deepEqual(stepGrid(fields, steps, 2, 'all', { seam: 'U', bench: '16' }).units.map((u) => u.picked), [false, true]);
  assert.deepEqual(stepGrid(fields, steps, 0, 'all', { seam: 'U' }).units.map((u) => u.picked), [true, true, true], 'OB is on U, as its waste is above it');
});

test('each step says how much it did', () => {
  assert.deepEqual(stepActivity(steps), [
    { changes: 0, made: 0, gone: 0 },
    { changes: 2, made: 0, gone: 0 },
    { changes: 0, made: 1, gone: 2 },
  ]);
});

test('words and values', () => {
  assert.equal(unitWords(steps[0].units[1]), 'U 18');
  assert.equal(valueText(10481.25, 'm3'), '10,481.3');
  assert.equal(valueText(1.5, 't/m3'), '1.500');
  assert.equal(valueText(null, 'm3'), '');
});
