// What a tool works on (decision 0108, decision 6): one choice at the foot of the view, the solids picked, every solid
// on the first picked one's layer, or every solid on show. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { READ_BY, solidsWorkedOn, WORKS_ON } from '../src/panels/worksOn.ts';

const solid = (id, layer, hidden = false) => ({ id, kind: 'solid', layer, name: id, hidden });
const objects = [solid('a', 'L1'), solid('b', 'L1'), solid('c', 'L2'), solid('d', 'L3'), solid('e', 'L1', true), { id: 's', kind: 'string', layer: 'L1', name: 's' }];
const layers = [
  { id: 'L1', visible: true },
  { id: 'L2', visible: true },
  { id: 'L3', visible: false },
];
const concealed = (o) => !!o.hidden;
const ids = (list) => list.map((o) => o.id).join();

test('picked takes the solids picked, never a string picked with them', () => {
  const got = solidsWorkedOn(objects, layers, ['b', 'c', 's'], 'picked', concealed);
  assert.equal(ids(got.chosen), 'b,c');
});

test("its layer takes every solid on the first picked one's layer, hidden ones too", () => {
  const got = solidsWorkedOn(objects, layers, ['b', 'c'], 'layer', concealed);
  assert.equal(ids(got.chosen), 'a,b,e');
  assert.equal(ids(solidsWorkedOn(objects, layers, [], 'layer', concealed).chosen), '', 'nothing picked, no layer');
});

test('on show takes the solids on layers shown, but not one hidden on its own', () => {
  const got = solidsWorkedOn(objects, layers, [], 'visible', concealed);
  assert.equal(ids(got.chosen), 'a,b,c');
});

test('each choice says how many it takes, whichever is in use', () => {
  const got = solidsWorkedOn(objects, layers, ['c'], 'picked', concealed);
  assert.deepEqual(got.labels, { picked: 'Picked (1)', layer: 'Its layer (1)', visible: 'On show (3)' });
  assert.deepEqual(WORKS_ON, ['picked', 'layer', 'visible']);
  assert.deepEqual(READ_BY, ['cut', 'reserves'], 'Cut solids and Interrogate read it');
});
