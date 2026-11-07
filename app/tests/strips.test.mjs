// Faces told apart by their strip number (decision 0049): what the Cut solids panel says about the faces it's given.
// Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { byStrip, facesLine, numberRuns, stripNumber } from '../src/panels/strips.ts';

const face = (name, strip) => ({ id: name, name, attributes: strip === undefined ? {} : { strip } });

test('a strip number is a number, or text that reads as one', () => {
  assert.equal(stripNumber(face('a', 5)), 5);
  assert.equal(stripNumber(face('b', ' 7 ')), 7);
  assert.equal(stripNumber(face('c', 'S7')), null);
  assert.equal(stripNumber(face('d')), null);
  assert.equal(stripNumber(face('e', true)), null);
});

test('faces group by strip number, lowest first, up and down together', () => {
  const by = byStrip([face('3 up', 3), face('1 up', 1), face('3 down', 3), face('loose'), face('2 up', '2')]);
  assert.deepEqual(
    by.strips.map((s) => [s.number, s.faces.map((f) => f.name)]),
    [
      [1, ['1 up']],
      [2, ['2 up']],
      [3, ['3 up', '3 down']],
    ],
  );
  assert.deepEqual(by.unnumbered.map((f) => f.name), ['loose']);
  assert.equal(facesLine(by), "4 faces on 3 strips: 1–3, one strip has more than one face, joined as they're sliced");
});

test('numbers read as short runs', () => {
  assert.equal(numberRuns([3, 1, 2, 4, 6, 9, 10]), '1–4, 6, 9–10');
  assert.equal(numberRuns([1]), '1');
  assert.equal(facesLine(byStrip([])), 'No faces with a strip number');
});
