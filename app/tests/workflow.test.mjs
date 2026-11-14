// Workflows (decision 0065): how a step's input rules read in the Workflow panel, and ticking runs in the Run log to
// capture them. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { onlyLayers, ruleWords, tickRuns, tidyLayers } from '../src/panels/workflow.ts';

const workflow = {
  steps: [
    { id: 4, action: { kind: 'command', command: 'solid.slice_by_faces', settings: {}, inputs: {} } },
    { id: 9, action: { kind: 'pause' } },
    { id: 2, action: { kind: 'command', command: 'solid.cut_by_seams', settings: {}, inputs: {} } },
  ],
};

test('a rule reads as the engine says it, a step by where it is now', () => {
  assert.equal(ruleWords(workflow, { from: { step: 2 }, many: true }), 'the objects step 3 made');
  assert.equal(ruleWords(workflow, { from: { step: 4 }, kind: 'solid', where: { material: 'coal' }, many: true }), 'the solids step 1 made where material is coal');
  assert.equal(ruleWords(workflow, { from: { step: 7 }, many: true }), 'the objects a step no longer there made');
  assert.equal(ruleWords(workflow, { from: { layer: 'block lines' }, kind: 'string', many: true }), "the strings on layer 'block lines'");
  assert.equal(ruleWords(workflow, { from: { named: 'Seam model' }, kind: 'seam_model', many: false }), "the seam model named 'Seam model'");
  assert.equal(ruleWords(workflow, { from: { picked: ['a'] }, many: false }), '1 object as picked');
  assert.equal(ruleWords(workflow, { from: { picked: ['a', 'b'] }, kind: 'solid', many: true }), '2 solids as picked');
});

test('a click ticks one run, a Shift-click every run between, never one that can not be captured', () => {
  const tickable = new Set([1, 2, 3, 5, 6]);
  let ticked = tickRuns(new Set(), tickable, 2, null, false);
  assert.deepEqual([...ticked], [2]);
  ticked = tickRuns(ticked, tickable, 6, 2, true);
  assert.deepEqual([...ticked].sort(), [2, 3, 5, 6], 'run 4 was refused or undone');
  ticked = tickRuns(ticked, tickable, 3, 6, true);
  assert.deepEqual([...ticked].sort(), [2], 'unticking runs back the same way');
  assert.deepEqual([...tickRuns(new Set(), tickable, 4, null, false)], []);
});

// Toes drawn by hand and offset on one layer, faces, a pit, strips and blocks each on their own, and the grids.
const objects = [
  { id: 'base', layer: 'toes' }, { id: 'toe2', layer: 'toes' },
  { id: 'face1', layer: 'faces' }, { id: 'face2', layer: 'faces' },
  { id: 'pit', layer: 'pit' }, { id: 'strip1', layer: 'strips' },
  { id: 'block1', layer: 'blocks' }, { id: 'block2', layer: 'blocks' },
  { id: 'topo', layer: 'grids' },
];
const made = [['toe2'], ['face1', 'face2'], ['pit'], ['strip1'], ['block1', 'block2']];

test('a tidy run shows its result and hides what only the steps in between made, never what was drawn by hand', () => {
  const { show, hide } = tidyLayers(made, ['block1', 'block2'], objects);
  assert.deepEqual(show, ['blocks']);
  assert.deepEqual(hide.sort(), ['faces', 'pit', 'strips'], 'toes holds the hand-drawn base toe, the grids no step made');
  // Paused after the strips: the strips are the result.
  assert.deepEqual(tidyLayers(made, ['strip1'], objects), { show: ['strips'], hide: ['faces', 'pit', 'blocks'] });
});

test('only a step shows its layers and turns off every other layer on show', () => {
  const layers = ['toes', 'faces', 'pit', 'strips', 'blocks', 'grids'].map((id) => ({ id, visible: id !== 'pit' }));
  assert.deepEqual(onlyLayers(['face1', 'face2'], objects, layers), { show: ['faces'], hide: ['toes', 'strips', 'blocks', 'grids'] });
  assert.deepEqual(onlyLayers(['gone'], objects, layers).show, [], 'what the step made is gone: nothing to show');
});
