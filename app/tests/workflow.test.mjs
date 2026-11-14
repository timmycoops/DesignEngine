// Workflows (decision 0065): how a step's input rules read in the Workflow panel, and ticking runs in the Run log to
// capture them. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { ruleWords, tickRuns } from '../src/panels/workflow.ts';

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
