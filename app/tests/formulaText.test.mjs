// The formula editor's words (decision 0077, 7d): a filter typed and read back, and what Copy for AI and Explain hand
// over: how formulas are written, the dataflow's names and the formula, never a value. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { FORMULA_TEMPLATE, readWhen, whenText } from '../src/panels/formulaText.ts';

test('a filter reads as the bench rules write it and back', () => {
  const when = readWhen(' material: coal ; seam: U, M;  ; strip: 1-5');
  assert.deepEqual(when, { material: 'coal', seam: 'U, M', strip: '1-5' });
  assert.equal(whenText(when), 'material: coal; seam: U, M; strip: 1-5');
  assert.deepEqual(readWhen(''), {});
  assert.equal(whenText(undefined), '');
});

