// The Reserves case tab's flow and Open another dataflow (Reserves case plan, decisions 4, 6 and 10): the case's coal and
// waste carried through a made-up dataflow by stage, what each stage takes off the coal or lets in from the waste, and a
// dataflow's range in words. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { flowStages, lanesAt, rangeWords, toRun } from '../src/panels/flowStages.ts';

const g = (material, volume, tonnes, became = null, detail = '') => ({ material, became, detail, units: 1, volume, tonnes });
const step = (n, name, groups, stopped = 0) => ({ step: n, name, groups, stopped });

// The blocks bring 25,000 m³ of coal (35,000 t) and 95,000 m³ of waste. working_sections takes 100 m³ of thin coal;
// rom_loss 1,000 roof and 2,000 floor; rom_dilution lets 500 m³ of rock in; rom_ash_cutoff moves 300 m³ of the roof
// loss and 400 m³ of coal to high ash, so roof loss goes down and the coal lane down by 400.
const coal = (v) => g('coal', v, v * 1.4);
const flow = [
  step(0, 'as input', [coal(25_000), g('waste', 95_000, null)]),
  step(1, 'initialise', [coal(25_000), g('waste', 95_000, null)]),
  step(2, 'working_sections', [coal(24_900), g('coal', 100, 140, 'thinloss', 'thin'), g('waste', 95_000, null)]),
  step(3, 'rom_loss', [coal(21_900), g('coal', 100, 140, 'thinloss', 'thin'), g('coal', 1_000, 1_400, 'loss', 'roof'), g('coal', 2_000, 2_800, 'loss', 'floor'), g('waste', 95_000, null)]),
  step(4, 'rom_dilution', [coal(21_900), g('coal', 100, 140, 'thinloss', 'thin'), g('coal', 1_000, 1_400, 'loss', 'roof'), g('coal', 2_000, 2_800, 'loss', 'floor'), g('waste', 94_500, null), g('waste', 500, null, 'dilution', 'roof')]),
  step(5, 'rom_ash_cutoff', [
    coal(21_500),
    g('coal', 100, 140, 'thinloss', 'thin'),
    g('coal', 700, 980, 'loss', 'roof'),
    g('coal', 2_000, 2_800, 'loss', 'floor'),
    g('coal', 700, 980, 'highashloss', 'rom ash'),
    g('waste', 94_500, null),
    g('waste', 500, null, 'dilution', 'roof'),
  ]),
];
const stages = [
  { name: 'Prepare', steps: [1] },
  { name: 'Working sections', steps: [2] },
  { name: 'ROM', steps: [3, 4, 5] },
];

test('the lanes after a step: coal and waste in play, what left each', () => {
  const l = lanesAt(flow[4]);
  assert.equal(l.coal, 21_900);
  assert.ok(Math.abs(l.coalTonnes - 21_900 * 1.4) < 1e-6);
  assert.equal(l.waste, 94_500);
  assert.deepEqual([...l.out.keys()], ['thinloss (thin)', 'loss (roof)', 'loss (floor)']);
  assert.equal(l.letIn.get('dilution (roof)'), 500);
});

test('the flow by stage: the blocks, then what each stage took off the coal or let in', () => {
  const s = flowStages(flow, stages);
  assert.deepEqual(
    s.map((x) => x.name),
    ['Blocks', 'Prepare', 'Working sections', 'ROM'],
  );
  assert.equal(s[0].coal, 25_000);
  assert.equal(s[0].waste, 95_000);
  assert.equal(s[1].lost, 0);
  assert.equal(s[2].lost, 100);
  assert.deepEqual(s[2].lostBy.map((m) => [m.what, m.volume]), [['thinloss (thin)', 100]]);
  // ROM takes 3,400 m³ of coal net (25,000 − 100 − 21,500), its roof loss shown at what it grew by over the stage.
  const rom = s[3];
  assert.equal(rom.lost, 24_900 - 21_500);
  assert.ok(Math.abs(rom.lostTonnes - 3_400 * 1.4) < 1e-6);
  assert.deepEqual(
    rom.lostBy.map((m) => [m.what, m.volume]),
    [
      ['loss (floor)', 2_000],
      ['loss (roof)', 700],
      ['highashloss (rom ash)', 700],
    ],
  );
  assert.equal(rom.letIn, 500);
  assert.deepEqual(rom.letInBy.map((m) => [m.what, m.volume]), [['dilution (roof)', 500]]);
  // Every stage's lanes and what left them add back to what came in.
  for (const x of s) {
    const l = lanesAt(flow.find((f) => f.step === x.steps[x.steps.length - 1]));
    const out = [...l.out.values()].reduce((a, b) => a + b.volume, 0);
    assert.equal(l.coal + out, 25_000, x.name);
  }
});

test('a stage with no flow yet is left out', () => {
  assert.deepEqual(
    flowStages(flow.slice(0, 3), stages).map((x) => x.name),
    ['Blocks', 'Prepare', 'Working sections'],
  );
  assert.deepEqual(flowStages([], stages), []);
});

test('a range in words, and what Run what is out of date runs', () => {
  assert.equal(rangeWords({ reserves: true }), "the case's blocks");
  assert.equal(rangeWords({ reserves: false, range: {} }), 'every row');
  assert.equal(rangeWords({ reserves: false, range: { strip: '1-6', seam: 'U' } }), 'strip 1-6, seam U');
  const d = [
    { name: 'Reserves', up_to_date: true },
    { name: 'Haulage', up_to_date: false },
  ];
  assert.deepEqual(
    toRun(d, false).map((x) => x.name),
    ['Haulage'],
  );
  assert.deepEqual(
    toRun(d, true).map((x) => x.name),
    ['Reserves', 'Haulage'],
  );
});

test('a density change moves the coal lane in tonnes but takes no coal', () => {
  const dens = [
    step(0, 'as input', [g('coal', 1_000, 1_500), g('waste', 2_000, null)]),
    step(1, 'densities', [g('coal', 1_000, 1_450), g('waste', 2_000, null)]),
  ];
  const [, s] = flowStages(dens, [{ name: 'In situ', steps: [1] }]);
  assert.equal(s.coalTonnes, 1_450);
  assert.equal(s.lost, 0);
  assert.equal(s.lostTonnes, 0);
  assert.deepEqual(s.lostBy, []);
});
