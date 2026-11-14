// The workflow canvas (decision 0067): a workflow laid out as a graph, and a wire drawn changing an input. Run with
// `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { rewire, workflowGraph } from '../src/panels/workflowGraph.ts';

const command = (id, command, inputs, extra = {}) => ({ id, action: { kind: 'command', command, settings: {}, inputs }, ...extra });
const many = (from, rest = {}) => ({ from, many: true, ...rest });

// A toe offset, faces from it, a pause, strips sliced by the faces, blocks cut from the strips, and a step on objects as
// picked after the pause.
const flow = {
  steps: [
    command(1, 'string.offset', { string: many({ layer: '0 toe' }, { kind: 'polyline' }) }),
    command(2, 'face.design', { lines: many({ step: 1 }), design: many({ named: 'Faces' }) }),
    { id: 3, action: { kind: 'pause' } },
    command(4, 'solid.slice_by_faces', { solid: many({ named: 'Pit' }), faces: many({ step: 2 }) }),
    command(5, 'solid.cut_by_polygons', { solids: many({ step: 4 }), polygons: many({ layer: 'blocks' }, { kind: 'polyline', where: { strip: 3 } }) }),
    command(6, 'object.delete', { objects: many({ picked: ['a', 'b'] }) }),
  ],
};

test('steps stand right of what they read, the pause between the steps before and after it, inputs at the left', () => {
  const graph = workflowGraph(flow);
  const at = (id) => graph.nodes.find((n) => n.id === id);
  assert.deepEqual(
    ['step:1', 'step:2', 'step:3', 'step:4', 'step:5', 'step:6'].map((id) => [at(id).column, at(id).row]),
    [[1, 0], [2, 0], [3, 0], [4, 0], [5, 0], [4, 1]],
    'step 6 reads nothing from a step, so it stands just right of the pause, under step 4',
  );
  assert.equal(at('step:3').kind, 'pause');
  assert.equal(graph.columns, 6);
  assert.deepEqual(
    graph.nodes.filter((n) => n.column === 0).map((n) => [n.id, n.row, n.label]),
    [['layer:0 toe', 0, '0 toe'], ['named:Faces', 1, 'Faces'], ['named:Pit', 2, 'Pit'], ['layer:blocks', 3, 'blocks'], ['picked:6:objects', 4, '2 picked']],
  );
  assert.equal(graph.edges.length, 8, 'a wire an input');
  const polygons = graph.edges.find((e) => e.input === 'polygons');
  assert.deepEqual([polygons.source, polygons.target, polygons.outside, polygons.detail], ['layer:blocks', 'step:5', true, 'polylines where strip is 3']);
  const faces = graph.edges.find((e) => e.input === 'faces');
  assert.deepEqual([faces.source, faces.outside], ['step:2', false]);
  assert.deepEqual(at('step:5').inputs, ['solids', 'polygons']);
});

test('a wire from a step no longer there starts at an input node saying so', () => {
  const gone = { steps: [command(7, 'solid.interrogate', { solids: many({ step: 99 }) })] };
  const graph = workflowGraph(gone);
  assert.deepEqual(graph.nodes.map((n) => [n.id, n.kind, n.column]), [['gone:7:solids', 'gone', 0], ['step:7', 'step', 1]]);
});

test('a wire drawn from an earlier step, a layer or a named object changes the input, keeping its kind and filter', () => {
  const fromStep = rewire(flow, 'step:1', 'step:5', 'polygons');
  assert.deepEqual(fromStep.steps[4].action.inputs.polygons, { from: { step: 1 }, many: true, kind: 'polyline', where: { strip: 3 } });
  assert.deepEqual(fromStep.steps[3], flow.steps[3], 'other steps as they were');
  assert.deepEqual(rewire(flow, 'layer:strips', 'step:5', 'solids').steps[4].action.inputs.solids.from, { layer: 'strips' });
  assert.deepEqual(rewire(flow, 'named:Faces 2', 'step:2', 'design').steps[1].action.inputs.design.from, { named: 'Faces 2' });
  assert.equal(rewire(flow, 'step:2', 'step:4', 'faces'), flow, 'the same wire changes nothing');
});

test('a wire from a later step, a pause or objects as picked is refused, in words', () => {
  assert.equal(rewire(flow, 'step:5', 'step:4', 'solid'), 'Step 5 comes after step 4: a step reads only what a step before it made');
  assert.equal(rewire(flow, 'step:4', 'step:4', 'solid'), 'Step 4 comes after step 4: a step reads only what a step before it made');
  assert.equal(rewire(flow, 'step:3', 'step:4', 'solid'), 'A pause makes nothing to read');
  assert.match(rewire(flow, 'picked:6:objects', 'step:5', 'solids'), /^Objects as picked/);
  assert.match(rewire(flow, 'step:1', 'step:3', 'x'), /^Wires go onto/);
  assert.match(rewire(flow, 'step:1', 'step:5', 'nothing'), /^Wires go onto/);
});
