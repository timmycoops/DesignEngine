// The Balance view (decision 0086): the 4c column's dilution balance laid out a row a stage, the moves signed as they
// change each part, every check holding. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { balanceRows, hasBalance, mineChecks } from '../src/panels/reservesBalance.ts';

const f = (part, name, value) => ({ column: `Summary\\Balance\\${part}\\${name}`, folders: ['Summary', 'Balance', part], name, unit: 'm3', values: [value], words: null });
// The 4c column, dilution only: coal 25,000 and waste 95,000 in; 1,760 to coal; 19,500 not mined.
const fields = [
  f('Coal', 'imported', 25000), f('Coal', 'insitu', 25000), f('Coal', 'to_coal', 1760), f('Coal', 'rom', 26760), f('Coal', 'scheduled', 26760),
  f('Coal', 'unaccounted_insitu', 0), f('Coal', 'unaccounted_rom', 0), f('Coal', 'unaccounted_scheduled', 0),
  f('Waste', 'imported', 95000), f('Waste', 'insitu', 95000), f('Waste', 'to_coal', 1760), f('Waste', 'not_mined', 19500), f('Waste', 'rom', 73740), f('Waste', 'scheduled', 73740),
  f('Waste', 'unaccounted_insitu', 0), f('Waste', 'unaccounted_rom', 0), f('Waste', 'unaccounted_scheduled', 0),
  f('Total', 'imported', 120000), f('Total', 'insitu', 120000), f('Total', 'not_mined', 19500), f('Total', 'rom', 100500), f('Total', 'scheduled', 100500),
  f('Total', 'unaccounted_insitu', 0), f('Total', 'unaccounted_rom', 0), f('Total', 'unaccounted_scheduled', 0),
];

test('a row a stage, the moves between them signed, a check after each stage', () => {
  const rows = balanceRows(fields);
  assert.deepEqual(rows.map((r) => r.key), ['imported', 'insitu', 'unaccounted_insitu', 'to_coal', 'not_mined', 'rom', 'unaccounted_rom', 'scheduled', 'unaccounted_scheduled'], 'no small volume, to waste or from outside here');
  const toCoal = rows.find((r) => r.key === 'to_coal');
  assert.deepEqual([toCoal.coal, toCoal.waste, toCoal.total], [1760, -1760, null], 'coal up, waste down, the total the same');
  const unmined = rows.find((r) => r.key === 'not_mined');
  assert.deepEqual([unmined.coal, unmined.waste, unmined.total], [null, -19500, -19500]);
  assert.ok(rows.filter((r) => r.kind === 'check').every((r) => r.holds));
  // Each stage follows from the one before by the moves between: the view adds up as the engine checked.
  const rom = rows.find((r) => r.key === 'rom');
  assert.equal(25000 + toCoal.coal, rom.coal);
  assert.equal(95000 + toCoal.waste + unmined.waste, rom.waste);
});

test("a check that doesn't hold is said; a table made before the balance has none", () => {
  const off = fields.map((x) => (x.column === 'Summary\\Balance\\Coal\\unaccounted_rom' ? { ...x, values: [5] } : x));
  assert.equal(balanceRows(off).find((r) => r.key === 'unaccounted_rom').holds, false, '5 m³ of 120,000 is past 1e-6');
  assert.ok(hasBalance(fields) && !hasBalance([{ column: 'ROM\\Coal\\volume', values: [1] }]));
});

// A mine's own checks (decision 0101, workstream 3): the waste processes 100 m³ short on one leaf, pre-strip missing on
// two waste leaves, a coal check holding.
test("the mine's checks: each one's difference and its leaves out or missing", () => {
  const c = (name, part, value, unit = '') => ({ column: `Summary\\Checks\\${name}\\${part}`, folders: ['Summary', 'Checks', name], name: part, unit, values: [value], words: null });
  const checks = mineChecks([
    ...fields,
    c('waste processes', 'difference', -100, 'm3'),
    c('waste processes', 'leaves out', 1),
    c('prestrip on waste', 'leaves missing', 2),
    c('coal mined', 'difference', 0, 't'),
    c('coal mined', 'leaves out', 0),
  ]);
  assert.deepEqual(checks.map((x) => [x.name, x.kind, x.difference, x.unit, x.leaves, x.holds]), [
    ['waste processes', 'adds', -100, 'm3', 1, false],
    ['prestrip on waste', 'required', null, '', 2, false],
    ['coal mined', 'adds', 0, 't', 0, true],
  ]);
  assert.deepEqual(mineChecks(fields), [], 'none without a check step');
});
