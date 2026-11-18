// The layer tree's rows (Filtering plan, L1 and L4): a row a layer, what's on it by kind, never a row an object. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { IN_TREE, kindsOf, layerEntries } from '../src/panels/layerTree.ts';

const layer = (id) => ({ id, name: id, colour: [200, 200, 200], visible: true });
let n = 0;
const object = (kind, layerId, extra = {}) => ({ id: `o${++n}`, name: `${kind} ${n}`, kind, layer: layerId, revision: 1, hidden: false, drawn: kind !== 'grid' || !extra.quality, attributes: {}, ...extra });

test('a layer counts what is on it by kind, in the tree order', () => {
  const on = [
    ...Array.from({ length: 60 }, () => object('string', 'faces')),
    ...Array.from({ length: 12 }, () => object('surface', 'faces')),
    object('solid', 'faces'),
  ];
  assert.equal(kindsOf(on), '60 strings · 12 surfaces · 1 solid');
  assert.equal(kindsOf([object('grid', 'g'), object('grid', 'g', { quality: true, drawn: false }), object('grid', 'g', { quality: true, drawn: false }), object('seam_model', 'g')]), '1 grid · 2 quality grids · 1 seam model');
  assert.equal(kindsOf([]), '');
});

test('what is not drawn is not in the tree, and a layer of nothing else has no row', () => {
  const layers = ['faces', 'parameters', 'dataflows', 'face designs', 'empty', 'mixed'].map(layer);
  const objects = [
    object('string', 'faces', { hidden: true }),
    object('string', 'faces'),
    object('surface', 'faces'),
    object('parameter_table', 'parameters'),
    object('dataflow', 'dataflows'),
    object('face_design', 'face designs'),
    object('workflow', 'face designs'),
    object('table', 'mixed'),
    object('solid', 'mixed'),
  ];
  assert.deepEqual(objects.filter((o) => !IN_TREE(o)).map((o) => o.kind), ['parameter_table', 'dataflow', 'face_design', 'workflow', 'table']);
  const picked = new Set([objects[1].id, objects[2].id]);
  const rows = layerEntries(layers, objects, picked);
  assert.deepEqual(rows.map((r) => r.layer.id), ['faces', 'empty', 'mixed'], 'a layer holding only tables, dataflows or designs has no row; an empty one keeps its row to draw on');
  const faces = rows[0];
  assert.deepEqual([faces.objects.length, faces.hidden, faces.picked, faces.all], [3, 1, 2, 3]);
  const mixed = rows[2];
  assert.deepEqual([mixed.objects.length, mixed.all], [1, 2], 'a table on a layer of solids is not counted in the row, but its delete takes it');
});

test("a layer's row counts what's hidden and what a filter leaves out apart, and carries its filter", () => {
  const layers = [layer('blocks'), layer('other')];
  const objects = [
    object('solid', 'blocks', { hidden: true }),
    object('solid', 'blocks', { filtered: true }),
    object('solid', 'blocks', { hidden: true, filtered: true }),
    object('solid', 'blocks'),
    object('solid', 'other'),
  ];
  const rows = layerEntries(layers, objects, new Set(), { blocks: { volume: '<500' } });
  assert.deepEqual([rows[0].hidden, rows[0].concealed, rows[0].filter], [2, 3, { volume: '<500' }], 'two hidden, three not showing');
  assert.deepEqual([rows[1].concealed, rows[1].filter], [0, null]);
});
