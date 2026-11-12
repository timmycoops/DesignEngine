// What the app holds of the project, kept up from the shell's changes (the App at scale plan, workstream 1): taken in
// turn, the changes give what the whole project would. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { LogStore, ProjectStore } from '../src/projectStore.ts';

const row = (id, revision = 1, extra = {}) => ({ id, revision, name: id, layer: 'L', kind: 'string', attributes: {}, drawn: false, hidden: false, geometry_key: '0', vertices_local: [], closed: false, ...extra });

const reply = (base, epoch, objects, removed = [], order = null) => ({
  base,
  epoch,
  objects,
  removed,
  order,
  path: null,
  can_undo: base !== null,
  can_redo: false,
  origin: [0, 0, 0],
  layers: [],
  solid_layers: [],
  display: {},
  locked_by: null,
  log: { session: 1, len: epoch },
});

const ids = (summary) => summary.objects.map((o) => o.id);

test('a create, a revision, a delete and an undo, taken in turn, give the whole project', () => {
  const store = new ProjectStore();
  let p = store.take(reply(null, 1, [row('a'), row('b'), row('c')]));
  assert.deepEqual(ids(p), ['a', 'b', 'c']);
  const b = p.objects[1];

  p = store.take(reply(1, 2, [row('d')]));
  assert.deepEqual(ids(p), ['a', 'b', 'c', 'd'], 'a new object goes at the end');
  assert.equal(p.objects[1], b, "an object that didn't change keeps its row");

  p = store.take(reply(2, 3, [row('a', 2, { name: 'a2' })]));
  assert.deepEqual(ids(p), ['a', 'b', 'c', 'd'], 'a revision keeps its place');
  assert.equal(p.objects[0].name, 'a2');

  p = store.take(reply(3, 4, [], ['a']));
  assert.deepEqual(ids(p), ['b', 'c', 'd']);

  // Undone, it goes back to its place, which the shell sends as the order.
  p = store.take(reply(4, 5, [row('a', 2, { name: 'a2' })], [], ['a', 'b', 'c', 'd']));
  assert.deepEqual(ids(p), ['a', 'b', 'c', 'd']);
  assert.equal(p.can_undo, true);
  assert.equal('removed' in p || 'epoch' in p || 'order' in p, false, 'the panels get the project, not the change');

  p = store.take(reply(5, 6, [row('b', 1, { hidden: true })]));
  assert.equal(p.objects[1].hidden, true, 'hiding is a change to the row');
});

test("changes that don't follow on from what the app holds aren't taken: it asks for the whole project", () => {
  const store = new ProjectStore();
  assert.equal(store.take(reply(7, 8, [row('a')])), null, 'nothing held yet');
  store.take(reply(null, 1, [row('a')]));
  assert.equal(store.take(reply(5, 6, [row('b')])), null, 'a reply that crossed another');
  assert.deepEqual(ids(store.take(reply(1, 2, [row('b')]))), ['a', 'b'], 'what it held stands');
  assert.deepEqual(ids(store.take(reply(null, 9, [row('z')]))), ['z'], 'the whole project replaces what it held');
});

test('the run log is fetched from where the app has got to, and a new session starts it again', () => {
  const log = new LogStore();
  const e = (n) => ({ event: 'undo', run: String(n), at_unix_ms: n });
  assert.deepEqual(log.take({ session: 1, from: 0, entries: [e(1), e(2)] }), [e(1), e(2)]);
  const held = log.take({ session: 1, from: 2, entries: [] });
  assert.equal(held, log.take({ session: 1, from: 2, entries: [] }), 'nothing new, the same list');
  assert.deepEqual(log.take({ session: 1, from: 2, entries: [e(3)] }), [e(1), e(2), e(3)]);
  // A fetch that crossed another starts where it says.
  assert.deepEqual(log.take({ session: 1, from: 2, entries: [e(3), e(4)] }), [e(1), e(2), e(3), e(4)]);
  assert.deepEqual(log.take({ session: 2, from: 0, entries: [e(9)] }), [e(9)], 'a project opened since');
});
