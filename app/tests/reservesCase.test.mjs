// The Reserves case tab (Reserves case plan, decisions 8, 10 and 11): the steps by stage, the block map, the whole case
// against the last run, and a step's settings by seam and ply, on made-up steps and blocks. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { blockMap, caseRows, changeWords, mapMeasure, scriptSettings, settingsGrid, shadeOf, stagesOf, stepSettings, suppliedFor, withCell } from '../src/panels/reservesCase.ts';

const s = (step, kind = 'built_in', dataflow) => ({ step, kind, ...(dataflow ? { dataflow } : {}) });

test('the steps go by stage as the run puts them: formulas with the next built-in step, after rom in ROM, an add-on its own', () => {
  const steps = [
    s('as input', 'input'),
    s('small_volumes'),
    s('initialise'),
    s('fill_size', 'formula'),
    s('limits', 'formula'),
    s('in_situ'),
    s('join_benches'),
    s('discard', 'formula'),
    s('working_sections'),
    s('rom_loss'),
    s('rom'),
    s('rom_qualities', 'formula'),
    s('split', 'allocation', 'wa'),
    s('place', 'formula', 'wa'),
    s('final'),
  ];
  assert.deepEqual(stagesOf(steps, { wa: 'Waste allocation' }), [
    { name: 'In situ', steps: [1, 2, 3, 4, 5, 6] },
    { name: 'Working sections', steps: [7, 8] },
    { name: 'ROM', steps: [9, 10, 11] },
    { name: 'Waste allocation (add-on)', steps: [12, 13] },
    { name: 'Final', steps: [14] },
  ]);
});

const did = (path, moved, written = moved, stopped = null) => ({ path, stopped, moved, written, coal_before: 0, coal_after: 0 });

test('the block map is a row a strip, blocks in their natural order, shaded by what the step moved', () => {
  const blocks = [did(['Pit', '10', 'B2'], 0), did(['Pit', '2', 'B10'], 300), did(['Pit', '2', 'B9'], 600), did(['Pit', '10', 'B1'], 0, 0, 'no rd')];
  const map = blockMap(blocks);
  assert.deepEqual(
    map.map((r) => [r.strip, r.blocks.map((b) => b.path[2])]),
    [
      ['Pit 2', ['B9', 'B10']],
      ['Pit 10', ['B1', 'B2']],
    ],
  );
  assert.equal(mapMeasure(blocks), 'moved');
  assert.equal(shadeOf(blocks[1], blocks, 'moved'), 0.5);
  assert.equal(shadeOf(blocks[3], blocks, 'moved'), 0, 'a stopped block has no shade');
  // Nothing moved: by what it wrote.
  assert.equal(mapMeasure([did(['P', '1', 'B1'], 0, 5)]), 'written');
});

const totals = (said, written) => ({ said, written, table: 't', step: 10, name: 'rom_loss', steps: [], columns: 90, stopped: [], fields: [], units: [], balance: {}, ran_from: null, ran: [], kept_mb: 0 });
const folder = (group, volume, tonnes) => ({ group, count: 1, values: { volume }, tonnes });

test('the whole case at the step: what it said it moved, then coal in tonnes and waste in BCM, last run against now', () => {
  const saved = totals([{ what: 'roof loss', units: 24, volume: 166169 }, { what: 'a check', units: 2, volume: null }], [folder('Insitu\\Coal', 6e6, 9166575), folder('Loss\\Roof', 9e4, 1e5)]);
  const now = totals([{ what: 'roof loss', units: 24, volume: 249253 }], [folder('Insitu\\Coal', 6e6, 9166575)]);
  const rows = caseRows(saved, now);
  assert.deepEqual(
    rows.map((r) => [r.label, r.unit, r.last, r.now]),
    [
      ['Roof loss', 'm3', 166169, 249253],
      ['In situ coal', 't', 9166575, 9166575],
    ],
  );
  assert.equal(changeWords(rows[0]), '+83,084 (+50.0 %)');
  assert.equal(changeWords(rows[1]), '—');
  assert.deepEqual(
    caseRows(saved, null).map((r) => [r.label, r.last, r.now]),
    [
      ['Roof loss', 166169, 166169],
      ['In situ coal', 9166575, 9166575],
    ],
    'no edits: now is the last run',
  );
});

const read = (unit, column, table, row) => ({ unit, column, value: 0.1, text: null, table, row });

test("a step's settings: a built-in step's as read, a formula's off its source", () => {
  assert.deepEqual(scriptSettings('def formula(u, p):\n    x = p.roof_loss * u.p.dip  # p.not_this\n    y = getattr(p, "ash_default", None)\n    z = getattr(p, name)\n'), ['roof_loss', 'dip', 'ash_default']);
  const builtIn = { step: 'rom_loss', kind: 'built_in', reads: [read(2, 'roof_loss', 0, 0), read(3, 'roof_loss', 1, 1), read(2, 'floor_loss', 1, 1)], source: null };
  assert.deepEqual(stepSettings(builtIn), ['roof_loss', 'floor_loss']);
  assert.deepEqual([...suppliedFor(builtIn, 2)], [
    ['roof_loss', { table: 0, row: 0 }],
    ['floor_loss', { table: 1, row: 1 }],
  ]);
  assert.equal(suppliedFor(builtIn, null).size, 0);
});

test('settings by seam and ply: every row of each table holding them, in reading order, case aside', () => {
  const column = {
    tables: ['Base own', 'Mining factors', 'Rates'],
    table_ids: ['own', 'mf', 'r'],
    steps: [{}, { step: 'rom_loss', kind: 'built_in', reads: [read(2, 'roof_loss', 0, 0), read(2, 'floor_loss', 1, 1)], source: null }],
  };
  const own = { filters: [], columns: [{ name: 'Roof_Loss' }], rows: [{ values: { Roof_Loss: 0.15 } }] };
  const factors = { filters: ['material', 'seam'], columns: [{ name: 'roof_loss' }, { name: 'floor_loss' }], rows: [{ when: { seam: 'M' }, values: { floor_loss: 0.15 } }, { when: { material: 'coal' }, values: { roof_loss: 0.1, floor_loss: 0.1 } }] };
  const rates = { filters: [], columns: [{ name: 'bcm_per_hour' }], rows: [{ values: { bcm_per_hour: 900 } }] };
  const grid = settingsGrid(column, 1, [own, factors, rates]);
  assert.deepEqual(grid.columns, ['roof_loss', 'floor_loss']);
  assert.deepEqual(
    grid.rows.map((r) => [r.tableName, r.row, r.fits, r.values.roof_loss, r.values.floor_loss]),
    [
      ['Base own', 0, 'every block', 0.15, null],
      ['Mining factors', 0, 'seam M', null, 0.15],
      ['Mining factors', 1, 'material coal', 0.1, 0.1],
    ],
  );
});

test('a cell typed sets a number, cleared passes on, a new column added; words stay words', () => {
  const t = { filters: ['material'], columns: [{ name: 'roof_loss' }, { name: 'ws_split', text: true }], rows: [{ when: { material: 'coal' }, values: { roof_loss: 0.1 } }] };
  assert.deepEqual(withCell(t, 0, 'Roof_Loss', '0.15').rows[0].values, { roof_loss: 0.15 });
  assert.deepEqual(withCell(t, 0, 'roof_loss', ' ').rows[0].values, {});
  assert.deepEqual(withCell(t, 0, 'ws_split', '12').rows[0].values, { roof_loss: 0.1, ws_split: '12' });
  const added = withCell(t, 0, 'edge_loss', '0.05');
  assert.deepEqual(added.columns.map((c) => c.name), ['roof_loss', 'ws_split', 'edge_loss']);
  assert.deepEqual(added.rows[0].values, { roof_loss: 0.1, edge_loss: 0.05 });
  assert.deepEqual(t.rows[0].values, { roof_loss: 0.1 }, 'the table given is left as it was');
});
