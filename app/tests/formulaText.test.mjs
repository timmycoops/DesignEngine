// The formula editor's words (decision 0077, 7d): a filter typed and read back, and what Copy for AI and Explain hand
// over: how formulas are written, the dataflow's names and the formula, never a value. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { FORMULA_TEMPLATE, forAi, readWhen, whenText } from '../src/panels/formulaText.ts';

test('a filter reads as the bench rules write it and back', () => {
  const when = readWhen(' material: coal ; seam: U, M;  ; strip: 1-5');
  assert.deepEqual(when, { material: 'coal', seam: 'U, M', strip: '1-5' });
  assert.equal(whenText(when), 'material: coal; seam: U, M; strip: 1-5');
  assert.deepEqual(readWhen(''), {});
  assert.equal(whenText(undefined), '');
});

test('Copy for AI hands over names and the formula, never a value', () => {
  const report = {
    stage: 'initialise',
    problems: [],
    tests: [],
    names: { fields: [['rd', 't/m3'], ['ash', '%'], ['discarded', '']], labels: ['seam', 'material', 'bench', 'became'], parameters: [['insitu_ash_cutoff', '%', false], ['discard', '', true]] },
  };
  const step = { step: 'discard', formula: { when: { material: 'coal' }, script: 'def formula(unit, p):\n    pass\n', tests: [{ name: 'by hand', unit: { ash: 12.3456 }, expect: { discarded: 1 } }] } };
  const write = forAi('# How to write one', report, step, 'write');
  assert.ok(write.startsWith('# How to write one'));
  assert.ok(write.includes('Fields: rd (t/m3), ash (%), discarded'));
  assert.ok(write.includes('Parameter columns: insitu_ash_cutoff (%), discard (words)'));
  assert.ok(write.includes('Step `discard`, after `initialise`, on units where material: coal.'));
  assert.ok(write.includes('def formula(unit, p):'));
  assert.ok(write.includes('## What I want') && write.includes('never a client'));
  assert.ok(!write.includes('12.3456'), 'a worked example\'s values are not handed over');
  const explain = forAi('# How', report, step, 'explain');
  assert.ok(explain.includes('Explain in plain words') && !explain.includes('## What I want'));
  assert.ok(FORMULA_TEMPLATE.includes('def formula(unit, p):'));
});
