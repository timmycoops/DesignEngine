// The tools beside the workflow canvas (decision 0067): where a dropped tool's step goes, what's picked for it, and what
// the canvas says. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { CANVAS_TOOLS, dropWords, madeByStep, placeOfDrop, toolByKey } from '../src/panels/workflowTools.ts';

const last = (made, revised = []) => ({ run: 'r', at_unix_ms: 0, ms: 1, made, revised });
const flow = {
  steps: [
    { id: 1, action: { kind: 'command', command: 'string.offset', settings: {}, inputs: {} }, last: last(['a', 'b']) },
    { id: 4, action: { kind: 'pause' } },
    { id: 2, action: { kind: 'command', command: 'solid.slice_by_faces', settings: {}, inputs: {} }, last: last(['c'], ['d', 'c']) },
  ],
};

test('a tool dropped on a step goes in after it; on nothing, at the end', () => {
  assert.equal(placeOfDrop(flow, 1), 1);
  assert.equal(placeOfDrop(flow, 4), 2, 'step ids are not places: the pause with id 4 is second');
  assert.equal(placeOfDrop(flow, 2), 3);
  assert.equal(placeOfDrop(flow, null), 3);
  assert.equal(placeOfDrop(flow, 99), 3, 'a step no longer there: the end');
  assert.equal(placeOfDrop({ steps: [] }, null), 0);
});

test("what's picked is what the step made or revised, once each, while it's still there", () => {
  const there = (id) => id !== 'b';
  assert.deepEqual(madeByStep(flow.steps[0], there), ['a']);
  assert.deepEqual(madeByStep(flow.steps[2], there), ['c', 'd']);
  assert.deepEqual(madeByStep(flow.steps[1], there), [], 'a pause makes nothing');
  assert.deepEqual(madeByStep(undefined, there), []);
});

test('the canvas says where the step goes and what is picked', () => {
  const cut = toolByKey('cut:polygons');
  assert.equal(cut.panel, 'cut');
  assert.equal(cut.mode, 'polygons');
  assert.equal(dropWords(flow, 'Toe to blocks', cut, 1, 2), "Set up By polygons in Cut solids, then Add to Toe to blocks: it goes in as step 2; what step 1 made is picked (2)");
  assert.equal(dropWords(flow, 'Toe to blocks', cut, 3, 0), 'Set up By polygons in Cut solids, then Add to Toe to blocks: it goes in as step 4, at the end');
});

test('every tool is one of what a panel that adds steps does, each key once', () => {
  const keys = CANVAS_TOOLS.map((t) => t.key);
  assert.equal(new Set(keys).size, keys.length);
  for (const tool of CANVAS_TOOLS) assert.ok(['builder', 'cut', 'reserves'].includes(tool.panel), tool.key);
  assert.deepEqual(
    CANVAS_TOOLS.filter((t) => t.panel === 'cut').map((t) => t.mode).sort(),
    ['between', 'cut', 'polygons', 'seams', 'strips'],
    "every one of Cut solids' choices",
  );
});
