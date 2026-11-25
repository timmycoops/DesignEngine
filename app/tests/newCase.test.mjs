// A way in to a case (Reserves case plan, decision 13): a free name for a copy, the blocks a new case runs on by layer
// and filter, and a headline figure's change against Base. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { againstBase, blocksFitting, freeName, headlineWords } from '../src/panels/newCase.ts';

test('a free name: as asked, else numbered', () => {
  assert.equal(freeName('Reserves Tight loss', ['Reserves Base']), 'Reserves Tight loss');
  assert.equal(freeName('Reserves Base', ['Reserves Base']), 'Reserves Base 2');
  assert.equal(freeName('reserves base', ['Reserves Base', 'Reserves base 2']), 'reserves base 3');
});

test('the blocks a new case runs on: on the layer, the filter fitting', () => {
  const solids = [
    { id: 'a', layer: 'blocks' },
    { id: 'b', layer: 'blocks' },
    { id: 'c', layer: 'other' },
    { id: 'd', layer: 'blocks' },
  ];
  const attributes = [{ strip: 1, seam: 'U' }, { strip: 7, seam: 'U' }, { strip: 1, seam: 'U' }, { strip: 3, seam: 'L' }];
  assert.deepEqual(blocksFitting(solids, attributes, 'blocks', {}), ['a', 'b', 'd']);
  assert.deepEqual(blocksFitting(solids, attributes, 'blocks', { strip: '1-5' }), ['a', 'd']);
  assert.deepEqual(blocksFitting(solids, attributes, 'blocks', { strip: '1-5', seam: 'U' }), ['a']);
});

test('a figure against Base', () => {
  assert.equal(againstBase(9_106_369, 9_234_489, false), '−1.4 %');
  assert.equal(againstBase(110, 100, false), '+10.0 %');
  assert.equal(againstBase(100, 100, true), '');
  assert.equal(againstBase(null, 100, false), '');
  assert.equal(againstBase(100.01, 100, false), '±0.0 %');
  assert.equal(headlineWords(7.6123, 2), '7.61');
  assert.equal(headlineWords(9234489.4, 0), '9,234,489');
  assert.equal(headlineWords(null, 0), '—');
});
