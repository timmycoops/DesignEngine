// The Tables side's list (decision 0108, workstream 4): the items down the chain, what each object opens as, and the
// case the top row runs following what's opened. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { caseOpen, itemNow, listOf, OBJECTS, onItem, openItem, resetItems, SECTIONS, sectionOf, setCase, setShowing, showingNow } from '../src/workspace/tablesSide.ts';
import { isItem, PANELS } from '../src/workspace/panels.ts';

const objects = [
  { id: 'blk', name: 'Pit blocks', kind: 'solid' },
  { id: 'base', name: 'Reserves Base', kind: 'table' },
  { id: 'csv', name: 'Spry export', kind: 'table' },
  { id: 'own', name: 'Objects formulas', kind: 'table', objects_own: true },
  { id: 'rom', name: 'Import to ROM', kind: 'dataflow' },
  { id: 'lib', name: 'shared', kind: 'script' },
  { id: 'res', name: 'Reserves assumptions', kind: 'parameter_table' },
  { id: 'c50', name: 'Reserves 50m', kind: 'table' },
];
const cases = [
  { table: 'base', name: 'Reserves Base', base: true },
  { table: 'c50', name: 'Reserves 50m', base: false },
];

test('the list runs down the chain: inputs, cases, dataflows, parameters, results, schedule', () => {
  assert.deepEqual(
    SECTIONS.map((s) => s.title),
    ['Inputs', 'Cases', 'Dataflows', 'Parameters', 'Results', 'Schedule'],
  );
  const rows = listOf(objects, cases);
  assert.deepEqual(
    rows.map((r) => `${r.section}:${r.id}`),
    ['inputs:objects', 'inputs:csv', 'case:base', 'case:c50', 'dataflows:rom', 'dataflows:lib', 'params:res', 'results:base', 'results:c50', 'schedule:null'],
  );
  // The order follows the sections', so the list draws each section's rows together.
  const order = SECTIONS.map((s) => s.id);
  assert.ok(rows.every((r, n) => n === 0 || order.indexOf(rows[n - 1].section) <= order.indexOf(r.section)));
});

test("a table that's no case is an input; the Objects table's own formulas are never listed; Base is marked", () => {
  const rows = listOf(objects, cases);
  assert.equal(rows.find((r) => r.id === 'csv').section, 'inputs');
  assert.ok(!rows.some((r) => r.id === 'own'));
  assert.equal(rows.find((r) => r.section === 'case' && r.id === 'base').note, 'Base');
  assert.equal(rows.find((r) => r.section === 'schedule').note, 'not built');
  assert.equal(rows[0].id, OBJECTS, 'the Objects table first, from Design');
});

test('an object opens in its section by its kind', () => {
  assert.equal(sectionOf('dataflow', false), 'dataflows');
  assert.equal(sectionOf('script', false), 'dataflows');
  assert.equal(sectionOf('parameter_table', false), 'params');
  assert.equal(sectionOf('table', true), 'case');
  assert.equal(sectionOf('table', false), 'inputs');
  assert.equal(sectionOf('solid', false), null);
});

test('opening a case or a result makes it the case the top row runs; a section opened bare finds its last item', () => {
  resetItems();
  let told = 0;
  const stop = onItem(() => told++);
  openItem('case', 'c50');
  assert.deepEqual(itemNow(), { section: 'case', id: 'c50' });
  assert.equal(caseOpen(), 'c50');
  openItem('params', 'res');
  assert.equal(caseOpen(), 'c50', 'parameters leave the case as it was');
  openItem('results', 'base');
  assert.equal(caseOpen(), 'base');
  openItem('case');
  assert.deepEqual(itemNow(), { section: 'case', id: 'c50' }, 'Cases asked for bare: the case it showed last');
  // The top row's case picked: the case open follows it.
  setCase('base');
  assert.deepEqual(itemNow(), { section: 'case', id: 'base' });
  openItem('params');
  setCase('c50');
  assert.deepEqual(itemNow(), { section: 'params', id: 'res' }, 'a case picked on the top row leaves the parameters open');
  assert.equal(caseOpen(), 'c50');
  stop();
  const was = told;
  openItem('inputs', OBJECTS);
  assert.equal(told, was, 'no longer told once stopped');
  assert.ok(was >= 6);
  resetItems();
});

test("each of the list's sections is an item for Find a command, opened in the Tables panel, never a tab of its own", () => {
  assert.deepEqual(
    PANELS.filter((p) => isItem(p.id)).map((p) => p.id),
    SECTIONS.map((s) => s.id),
  );
  assert.ok(!isItem('tables') && !isItem('restable'));
  assert.deepEqual(
    PANELS.filter((p) => isItem(p.id)).map((p) => p.title),
    SECTIONS.map((s) => s.title),
  );
});

test('what the window shows is told to the Tables side, so a pick goes to the view only with both shown (0109)', () => {
  assert.equal(showingNow(), 'design');
  setShowing('both');
  assert.equal(showingNow(), 'both');
  setShowing('tables');
  assert.equal(showingNow(), 'tables');
  setShowing('design');
});
