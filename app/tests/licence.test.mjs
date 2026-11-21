// The words the app shows for its licence (decision 0098). Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { licenceStanding, licenceWarning, MODULES } from '../src/licence.ts';

const licence = { licence: 'ZD-0001', licensed_to: 'Example Coal Pty Ltd', runs_until: '2027-06-30' };
const status = (state) => ({ ...state, licence, modules: ['viewer'], machine: 'M-1111-2222-3333', path: null });

test('the status bar says nothing while a licence simply runs, and warns as it ends', () => {
  assert.equal(licenceWarning(status({ state: 'current', days_left: 200 })), null);
  assert.equal(licenceWarning(status({ state: 'ends_soon', days_left: 10 })), 'Licence ends in 10 days');
  assert.equal(licenceWarning(status({ state: 'ends_soon', days_left: 1 })), 'Licence ends in 1 day');
  assert.equal(licenceWarning(status({ state: 'ends_soon', days_left: 0 })), 'Licence ends today');
  assert.equal(licenceWarning(status({ state: 'grace', grace_left: 13 })), 'Licence ended: 13 days of grace left');
  assert.equal(licenceWarning(status({ state: 'viewer', reason: 'No licence' })), 'Viewer only: no licence for this machine');
});

test('the panel says where the licence stands, and why it is the Viewer', () => {
  assert.equal(licenceStanding(status({ state: 'current', days_left: null })), 'Running, with no end date');
  assert.equal(licenceStanding(status({ state: 'current', days_left: 200 })), 'Running until 2027-06-30');
  assert.equal(licenceStanding(status({ state: 'grace', grace_left: 2 })), 'Ended 2027-06-30; it runs on for 2 days of grace');
  assert.equal(licenceStanding(status({ state: 'viewer', reason: 'The licence is for other machines' })), 'The licence is for other machines');
});

test('the modules are the engine’s, in its order', () => {
  assert.deepEqual(MODULES, ['viewer', 'design', 'solids', 'reserves', 'python']);
});
