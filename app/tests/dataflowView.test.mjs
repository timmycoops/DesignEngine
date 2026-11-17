// The Dataflow view's strip (decision 0084): a step's writes under their top folders, short volumes, the inputs and what
// the picked step read lit, on a made-up column. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { inputsOf, stripSteps, tablesRead, volumeShort, writeTip, writesByFolder } from '../src/panels/dataflowView.ts';

const unit = (id) => ({ id, horizon: 'U', material: 'coal', seam: 'U', bench: null, joined: [], became: null, section: null, rom: null, values: {} });
const step = (name, units, more = {}) => ({ step: name, units: units.map(unit), said: [], kind: 'built_in', rule: '', when: {}, source: null, fits: [], reads: [], workings: [], writes: [], ...more });
const read = (u, column, table, row) => ({ unit: u, column, value: 0.05, text: null, table, row });

const column = {
  column: ['Pit', '1', 'B1'],
  case: 'Base',
  dataflow: 'Reserves',
  dataflow_id: 'f',
  fields: [],
  tables: ['Mining factors', 'Seam rules'],
  blocks: ['1_B1_OB', '1_B1_A', '1_B1_A-B'],
  model: null,
  horizons: 5,
  edited: false,
  failing: [],
  changed: [],
  steps: [
    step('as input', [1, 2, 3], { kind: 'input', writes: [{ folder: 'Imported\\Waste', units: [1, 3], volume: 95000 }, { folder: 'Imported\\Coal', units: [2], volume: 25000 }] }),
    step('working_sections', [1, 2, 3], { reads: [read(2, 'min_parting', 1, 0), read(3, 'min_parting', 1, 2)] }),
    step('rom_dilution', [1, 2, 3, 4], {
      reads: [read(2, 'roof_dilution', 0, 2), read(4, 'roof_dilution', 0, 1)],
      writes: [
        { folder: 'Dilution\\Roof', units: [2, 4], volume: 500 },
        { folder: 'Dilution\\Floor', units: [2, 4], volume: 1000 },
      ],
    }),
    step('densities', [1, 2, 3, 4], { kind: 'formula', reads: [] }),
  ],
};

test('writes go under their top folder, volumes added up, in the order first written', () => {
  const byFolder = writesByFolder(column.steps[0].writes);
  assert.deepEqual(byFolder.map((w) => [w.folder, w.volume]), [['Imported', 120000]]);
  const dilution = writesByFolder(column.steps[2].writes);
  assert.deepEqual(dilution.map((w) => [w.folder, w.volume, w.parts.length]), [['Dilution', 1500, 2]]);
  assert.equal(writeTip(dilution[0]), 'Dilution\\Roof 500.0 m³ (2 units)\nDilution\\Floor 1,000.0 m³ (2 units)');
});

test('the strip: the column as input is Imported, each step with its units passed on and its writes', () => {
  const s = stripSteps(column.steps, [false, true, false, false]);
  assert.deepEqual(s.map((x) => [x.name, x.units, x.quiet, x.writes.length]), [['Imported', 3, false, 1], ['working_sections', 3, true, 0], ['rom_dilution', 4, false, 1], ['densities', 4, false, 0]]);
  assert.equal(s[3].kind, 'formula');
});

test('volumes short enough for a tag', () => {
  assert.deepEqual([412300, 1100, 350, 9.94, 0, -19500, 2_500_000].map(volumeShort), ['412.3k', '1.1k', '350', '9.9', '0', '−19.5k', '2.5M']);
});

test('nothing lit with no step picked; the blocks and the stack when the column as input is', () => {
  const none = inputsOf(column, null, null);
  assert.deepEqual(none.map((c) => [c.key, c.label, c.lit]), [['blocks', '3 block solids', false], ['model', 'Stack of 5 horizons', false], ['table:0', 'Mining factors', false], ['table:1', 'Seam rules', false]]);
  const input = inputsOf(column, 0, null);
  assert.deepEqual(input.filter((c) => c.lit).map((c) => c.key), ['blocks', 'model']);
  assert.ok(input[0].title.includes('1_B1_OB, 1_B1_A, 1_B1_A-B'));
  assert.equal(inputsOf({ ...column, model: 'PIT X seams' }, null, null)[1].label, 'PIT X seams');
});

test('a built-in step lights the table and the rows it read: for the unit followed where it read any, else every unit', () => {
  const forUnit = inputsOf(column, 2, 2);
  assert.deepEqual(forUnit.filter((c) => c.lit).map((c) => [c.key, c.label, c.rows]), [['table:0', 'Mining factors · column 3', [3]]]);
  const every = inputsOf(column, 2, 1);
  assert.deepEqual(every.filter((c) => c.lit).map((c) => c.rows), [[2, 3]], 'unit 1 read nothing: every unit counted');
  assert.deepEqual(inputsOf(column, 1, null).filter((c) => c.lit).map((c) => [c.key, c.rows]), [['table:1', [1, 3]]]);
  assert.equal(inputsOf(column, 3, 2).filter((c) => c.lit).length, 0, 'a formula lights nothing: its reads are in its trace');
});

test('the tables a step read, for the graph to wire', () => {
  assert.deepEqual(column.steps.map(tablesRead), [[], [1], [0], []]);
});
