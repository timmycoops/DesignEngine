// The Dataflow view's strip (decision 0084): a step's writes under their top folders, short volumes, the inputs and what
// the picked step read lit, on a made-up column. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { beforeAfter, inputsOf, matchUnits, stripSteps, tablesRead, volumeShort, writeTip, writesByFolder } from '../src/panels/dataflowView.ts';

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

// Before and After (decision 0084, points 4 and 5): OB 50,000 m³ over U 10,000 m³; rom_dilution reads thickness and
// volume, cuts 500 m³ of roof dilution off OB's bottom as a new unit and takes nothing away; join_benches joins U's
// two benches into one; rom reads and changes nothing.
const v = (id, horizon, material, values, more = {}) => ({ ...unit(id), horizon, material, seam: horizon, values, ...more });
const small = {
  ...column,
  fields: [
    { name: 'volume', unit: 'm3', kind: 'volume' },
    { name: 'thickness', unit: 'm', kind: 'thickness' },
    { name: 'rd', unit: 't/m3', kind: 'density' },
    { name: 'ash', unit: '%', kind: 'quality' },
  ],
  steps: [
    step('as input', [], { kind: 'input' }),
    step('rom_dilution', [], { fields_read: ['thickness', 'volume'] }),
    step('join_benches', [], { fields_read: ['volume', 'thickness'] }),
    step('rom', [], { fields_read: [] }),
  ],
};
const ob = v(1, 'OB', 'waste', { volume: 50000, thickness: 10, rd: 2.3, ash: null });
const u130 = v(2, 'U', 'coal', { volume: 6000, thickness: 3, rd: 1.4, ash: 12 }, { bench: '130' });
const u120 = v(3, 'U', 'coal', { volume: 4000, thickness: 2, rd: 1.5, ash: 14 }, { bench: '120' });
const obCut = { ...ob, values: { ...ob.values, volume: 49500, thickness: 9.9 } };
const dil = v(4, 'OB', 'waste', { volume: 500, thickness: 0.1, rd: 2.3, ash: null }, { became: 'dilution (roof)' });
const joined = v(5, 'U', 'coal', { volume: 10000, thickness: 5, rd: 1.44, ash: 12.8 }, { joined: [['130', 0.6], ['120', 0.4]] });
small.steps[0].units = [ob, u130, u120];
small.steps[1].units = [obCut, dil, u130, u120];
small.steps[2].units = [obCut, dil, joined];
small.steps[3].units = [obCut, dil, joined];

test('Before and After: the fields the step reads and changes first, a unit a row, what changed and what it made', () => {
  const ba = beforeAfter(small, 1, false);
  assert.deepEqual(ba.fields.map((f) => [f.field.name, f.read, f.wrote]), [['volume', true, true], ['thickness', true, true], ['rd', false, true]], 'rd only on the unit it made');
  assert.equal(ba.more, 1, 'All fields adds ash');
  assert.deepEqual(ba.before.map((r) => [r.label, r.mark]), [['OB', null], ['U 130', null], ['U 120', null]]);
  assert.deepEqual(ba.after.map((r) => [r.label, r.mark]), [['OB', null], ['OB', 'new'], ['U 130', null], ['U 120', null]]);
  assert.deepEqual(ba.after[0].cells.map((c) => [c.value, c.changed]), [[49500, true], [9.9, true], [2.3, false]]);
  assert.deepEqual(ba.after[1].cells.map((c) => c.changed), [false, false, false], 'a new unit has nothing to change from');
  assert.deepEqual(ba.volume, { before: 60000, after: 60000 });
  assert.deepEqual(beforeAfter(small, 1, true).fields.map((f) => f.field.name), ['volume', 'thickness', 'rd', 'ash'], 'the rest after');
});

test('a join takes its benches away on Before and makes one unit on After', () => {
  const ba = beforeAfter(small, 2, false);
  assert.deepEqual(ba.before.map((r) => r.mark), [null, null, 'gone', 'gone']);
  assert.deepEqual(ba.after.map((r) => [r.label, r.mark]), [['OB', null], ['OB', null], ['U (2 benches)', 'new']]);
  assert.deepEqual(ba.fields.map((f) => f.field.name), ['volume', 'thickness', 'rd', 'ash'], 'every field the joined unit has');
});

test('a step that reads and changes nothing shows the volume; the column as input has no Before', () => {
  const quiet = beforeAfter(small, 3, false);
  assert.deepEqual(quiet.fields.map((f) => [f.field.name, f.read, f.wrote]), [['volume', false, false]]);
  assert.equal(quiet.more, 3);
  const input = beforeAfter(small, 0, false);
  assert.deepEqual([input.before.length, input.after.length, input.volume.before, input.volume.after], [0, 3, null, 60000]);
  assert.ok(input.after.every((r) => r.mark === null && r.cells.every((c) => !c.changed)));
});

// What an unsaved edit moved against the saved run (decision 0082, point 6): roof dilution made 0.10 m from 0.05 m cuts
// 1,000 m³ off OB, not 500 m³, so OB's cut and the dilution move; U doesn't. The edit also gives the dilution unit a new
// id (ids are handed out as a run goes), so units are matched by what they are.
const savedDil = { ...dil, values: { ...dil.values } };
const liveObCut = { ...ob, values: { ...ob.values, volume: 49000, thickness: 9.8 } };
const liveDil = { ...dil, id: 9, values: { ...dil.values, volume: 1000, thickness: 0.2 } };
const savedRun = {
  ...small,
  steps: [
    { ...small.steps[0] },
    { ...small.steps[1], writes: [{ folder: 'Dilution\\Roof', units: [4], volume: 500 }] },
    small.steps[2],
    small.steps[3],
  ],
};
const liveRun = {
  ...small,
  steps: [
    { ...small.steps[0] },
    { ...small.steps[1], units: [liveObCut, liveDil, u130, u120], writes: [{ folder: 'Dilution\\Roof', units: [9], volume: 1000 }] },
    { ...small.steps[2], units: [liveObCut, liveDil, joined] },
    { ...small.steps[3], units: [liveObCut, liveDil, joined] },
  ],
};
savedRun.steps[1].units = [obCut, savedDil, u130, u120];

test('units are matched across runs by what they are, not their ids', () => {
  const m = matchUnits(liveRun.steps[1].units, savedRun.steps[1].units);
  assert.deepEqual([...m.entries()].map(([id, u]) => [id, u.id]), [[1, 1], [9, 4], [2, 2], [3, 3]]);
});

test('run on an edit, a value it moved carries the saved one; one it left alone carries none', () => {
  const ba = beforeAfter(liveRun, 1, false, savedRun);
  assert.deepEqual(ba.after[0].cells.map((c) => [c.value, c.was]), [[49000, 49500], [9.8, 9.9], [2.3, undefined]], 'OB cut by 1,000 m³, not 500');
  assert.deepEqual(ba.after[1].cells.map((c) => c.was), [500, 0.1, undefined], 'the dilution');
  assert.ok(ba.after.slice(2).every((r) => r.cells.every((c) => c.was === undefined) && !r.added), "U's benches didn't move");
  assert.ok(ba.before.every((r) => r.cells.every((c) => c.was === undefined)), 'what came in is as saved');
  assert.ok(beforeAfter(liveRun, 1, false).after.every((r) => r.cells.every((c) => c.was === undefined)), 'nothing to set it against: nothing moved');
  const strip = stripSteps(liveRun.steps, [false, false, false, true], savedRun.steps);
  assert.deepEqual(strip[1].writes.map((w) => [w.folder, w.volume, w.was]), [['Dilution', 1000, 500]]);
  assert.equal(stripSteps(savedRun.steps, [false, false, false, true], savedRun.steps)[1].writes[0].was, undefined);
});

test('a unit the saved run has nothing like is marked as made by the edit', () => {
  const extra = v(7, 'M', 'coal', { volume: 10, thickness: 0.1, rd: 1.4, ash: 20 });
  const run = { ...liveRun, steps: liveRun.steps.map((s, i) => (i === 1 ? { ...s, units: [...s.units, extra] } : s)) };
  const ba = beforeAfter(run, 1, false, savedRun);
  const row = ba.after.find((r) => r.unit.id === 7);
  assert.ok(row.added && row.cells[0].was === null);
});
