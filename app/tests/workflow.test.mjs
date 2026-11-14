// Workflows (decision 0065): how a step's input rules read in the Workflow panel, and ticking runs in the Run log to
// capture them. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { BASE, caseValue, onlyLayers, readSetting, ruleWords, settingRows, stepsOfCase, tickRuns, tidyLayers, withSetting } from '../src/panels/workflow.ts';

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

// Cases (decision 0066): a toe offset and block lines, Base at 100 m and a case at 50 m.
const cased = {
  steps: [
    { id: 1, action: { kind: 'command', command: 'string.offset', settings: { distance: 60, times: 5, layer: '1 toes', side: 'left' }, inputs: {} }, last: { made: ['t'] } },
    { id: 2, action: { kind: 'pause' } },
    { id: 3, action: { kind: 'command', command: 'string.offset', settings: { distance: 100, times: 15, points: [[0, 0, 0]], closed: false }, inputs: {} }, last: { made: ['b'] } },
  ],
  cases: [{ name: '50m', settings: { 3: { distance: 50, times: 30 } }, last: { 3: { made: ['b50'] } } }],
};

test('the settings grid has a row a plain setting, and a case reads as Base but for what it changes', () => {
  const rows = settingRows(cased);
  assert.deepEqual(rows.map((r) => `${r.position} ${r.field}`), ['1 distance', '1 times', '1 layer', '1 side', '3 distance', '3 times', '3 closed'], 'no lists of points');
  const distance = rows.find((r) => r.step === 3 && r.field === 'distance');
  assert.equal(caseValue(cased, BASE, distance), 100);
  assert.equal(caseValue(cased, '50m', distance), 50);
  assert.equal(caseValue(cased, '50m', rows[0]), undefined, 'as Base');
  assert.deepEqual(stepsOfCase(cased, '50m').map((s) => s.last?.made), [undefined, undefined, ['b50']]);
  assert.deepEqual(stepsOfCase(cased, BASE).map((s) => s.last?.made), [['t'], undefined, ['b']]);
});

test('a cell typed changes Base or the case, an empty case cell reads as Base again, and text that is not a value is refused', () => {
  const rows = settingRows(cased);
  const distance = rows.find((r) => r.step === 3 && r.field === 'distance');
  const base = withSetting(cased, BASE, distance, '120');
  assert.equal(base.steps[2].action.settings.distance, 120);
  assert.deepEqual(base.cases, cased.cases, 'the case keeps its own');
  const width = withSetting(cased, '50m', rows[0], '30');
  assert.deepEqual(width.cases[0].settings, { 1: { distance: 30 }, 3: { distance: 50, times: 30 } });
  const back = withSetting(withSetting(cased, '50m', distance, ''), '50m', rows.find((r) => r.step === 3 && r.field === 'times'), ' ');
  assert.deepEqual(back.cases[0].settings, {}, 'both changes taken away');
  assert.equal(withSetting(cased, BASE, distance, 'wide'), null);
  assert.equal(readSetting('true', false), true);
  assert.equal(readSetting('maybe', false), null);
  assert.equal(readSetting('2 faces', 'x'), '2 faces');
});
