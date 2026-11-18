// Bench rules (decision 0071): matching as the engine matches, the horizon ticks, the levels row and CSV, on made-up seam
// pieces. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { catchAll, comparison, everything, fits, horizonState, listFits, horizonsOf, range, readWhen, ruleFor, rulesFromCsv, rulesParam, rulesToCsv, setHorizonBenched, taken, whenText } from '../src/panels/benchRules.ts';

const piece = (strip, horizon, material) => ({ strip, horizon, material, seam: material === 'coal' ? horizon : 'U' });
// Two strips cut by seams, top down: OB, U, U-M, M.
const pieces = [1, 7].flatMap((s) => [piece(s, 'OB', 'waste'), piece(s, 'U', 'coal'), piece(s, 'U-M', 'waste'), piece(s, 'M', 'coal')]);
const rule = (when, benched = true, extra = {}) => ({ ...everything(), when, benched, ...extra });

test('filters take words, numbers and ranges, as the engine does', () => {
  assert.deepEqual(range('1-5'), [1, 5]);
  assert.deepEqual(range('-20--5'), [-20, -5]);
  assert.equal(range('U-M'), null);
  const um = piece(3, 'U-M', 'waste');
  assert.ok(fits(rule({ horizon: 'U-M, M-L' }), um), 'U-M is a word');
  assert.ok(fits(rule({ Horizon: 'u-m' }), um), 'case aside');
  assert.ok(fits(rule({ strip: '1-5' }), um));
  assert.ok(!fits(rule({ strip: '4-5' }), um));
  assert.ok(fits(rule({ strip: '', material: '*' }), um), 'blank and * are any');
  assert.ok(!fits(rule({ block: 'B1' }), um), "an attribute it hasn't doesn't fit");
  assert.ok(fits(rule({ strip: '10-15' }), { strip: '12' }), 'text that reads as a number');
});

test('the first rule that fits wins, and what each takes is counted', () => {
  const rules = [rule({ horizon: 'U-M' }, false), rule({ material: 'coal', strip: '1-5' }, false), rule({ material: 'coal' }), everything()];
  assert.equal(ruleFor(rules, piece(1, 'U-M', 'waste')), 0);
  assert.equal(ruleFor(rules, piece(1, 'U', 'coal')), 1);
  assert.equal(ruleFor(rules, piece(7, 'U', 'coal')), 2);
  assert.equal(ruleFor(rules, piece(7, 'OB', 'waste')), 3);
  assert.deepEqual(taken(rules, pieces), { byRule: [2, 2, 2, 2], none: 0 });
  assert.deepEqual(taken(rules.slice(0, 2), pieces), { byRule: [2, 2], none: 4 });
  assert.equal(catchAll(rules), 3);
  assert.equal(catchAll(rules.slice(0, 3)), -1);
});

test('horizon ticks keep one rule at the top listing the horizons left whole', () => {
  assert.deepEqual(horizonsOf(pieces), ['OB', 'U', 'U-M', 'M']);
  let rules = [everything()];
  assert.equal(horizonState(rules, pieces, 'U-M').benched, 'all');
  rules = setHorizonBenched(rules, 'U-M', false);
  rules = setHorizonBenched(rules, 'M', false);
  assert.equal(rules.length, 2);
  assert.deepEqual(rules[0].when, { horizon: 'U-M, M' });
  assert.equal(rules[0].benched, false);
  assert.deepEqual(horizonState(rules, pieces, 'M'), { benched: 'none', byTicks: true, rules: [0] });
  rules = setHorizonBenched(rules, 'U-M', true);
  assert.deepEqual(rules[0].when, { horizon: 'M' });
  rules = setHorizonBenched(rules, 'M', true);
  assert.deepEqual(rules, [everything()], 'the ticks rule goes once it lists nothing');
  // Coal on strips 1-5 left whole by a rule of the user's own: U is benched on strip 7 only, and the ticks don't decide it.
  const own = [rule({ material: 'coal', strip: '1-5' }, false), everything()];
  assert.deepEqual(horizonState(own, pieces, 'U'), { benched: 'some', byTicks: false, rules: [0] });
});

test('a filter is typed as attribute: values; and read back', () => {
  assert.equal(whenText({ horizon: 'U-M, M-L', strip: '1-5', block: '' }), 'horizon: U-M, M-L; strip: 1-5');
  assert.deepEqual(readWhen('horizon: U-M, M-L; strip: 1-5'), { when: { horizon: 'U-M, M-L', strip: '1-5' } });
  assert.deepEqual(readWhen('  '), { when: {} });
  assert.ok('why' in readWhen('horizon U-M'));
  assert.ok('why' in readWhen('strip: 1; strip: 2'));
});

test('rules go out as CSV, a column an attribute, and come back the same', () => {
  const rules = [rule({ horizon: 'U-M, M-L' }, false, { layer: '9 kept whole' }), rule({ material: 'coal', strip: '1-5' }, false), rule({ material: 'coal' }, true, { every: 5 }), rule({}, true, { named_by: 'top', from: -300 })];
  const csv = rulesToCsv(rules);
  assert.equal(csv.split('\n')[0], 'horizon,material,strip,benched,from,to,every,named_by,layer');
  assert.equal(csv.split('\n')[1], '"U-M, M-L",,,no,,,,,9 kept whole');
  assert.equal(csv.split('\n')[4], ',,,yes,-300,500,10,top,');
  const back = rulesFromCsv(csv);
  assert.deepEqual(back.errors, []);
  assert.deepEqual(rulesParam(back.rules), rulesParam(rules));
});

test('CSV with mistakes is refused line by line; columns may be left out', () => {
  const bad = rulesFromCsv('material,benched,every,named_by\ncoal,maybe,5,bottom\nwaste,yes,ten,up\n');
  assert.deepEqual(bad.rules, []);
  assert.deepEqual(bad.errors, ['line 2: benched is "maybe", not yes or no', 'line 3: every is "ten", not a number', 'line 3: named_by is "up", not bottom or top']);
  const short = rulesFromCsv('﻿horizon,benched\r\n# comments are skipped\r\nOB,yes\r\n');
  assert.deepEqual(short.errors, []);
  assert.deepEqual(rulesParam(short.rules), [{ when: { horizon: 'OB' }, benched: true, from: 0, to: 500, every: 10, named_by: 'bottom' }]);
  assert.deepEqual(rulesFromCsv('from,to,every\n10,0,5\n').errors, ['line 2: from RL 10 is above to RL 0']);
});

test('comparisons and not, as the engine reads them (Filtering plan, decision 1)', () => {
  assert.deepEqual([comparison('<0.3'), comparison('>= 120'), comparison('<abc'), comparison('-20--5')], [['<', 0.3], ['>=', 120], null, null]);
  assert.ok(listFits('<0.3', 0.25) && !listFits('<0.25', 0.25) && listFits('<=0.25', 0.25));
  assert.ok(listFits('>=120', '120') && !listFits('>120', '120'), 'text that reads as a number compares as one');
  assert.ok(!listFits('<5', 'U'), 'a comparison never fits a word');
  assert.ok(listFits('<abc', '<abc'), '`<abc` is a word');
  assert.ok(listFits('!waste', 'Coal') && !listFits('!waste', 'WASTE') && listFits('!waste', undefined), 'what has none is not waste');
  assert.ok(listFits('*, !WASTE', 'Coal') && !listFits('*, !WASTE', 'WASTE'));
  assert.ok(!listFits('Coal, !WASTE', undefined), 'with values it may hold, one must be there');
  assert.ok(!listFits('1-10, !4', 4) && listFits('1-10, !5-6', 4));
  const rule = { ...everything(), when: readWhen('thickness: <0.3; seam: !P').when };
  assert.ok(fits(rule, { thickness: 0.2, seam: 'U' }) && !fits(rule, { thickness: 0.2, seam: 'P' }) && !fits(rule, { thickness: 0.4, seam: 'U' }));
});
