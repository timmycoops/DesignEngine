// The kit everywhere (Two sides plan, workstream 6): every panel takes its boxes, ticks, buttons and dialogs from the
// kit, and says what a run did with the kit's one line. Run with `pnpm test`.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';
import { ok, quiet, refused, SAID_CLASS, saidOf, saidOfError, warning } from '../src/design-system/kit/said.ts';

const DIRS = ['src/panels', 'src/workspace'];
const files = DIRS.flatMap((dir) =>
  readdirSync(new URL(`../${dir}/`, import.meta.url))
    .filter((f) => f.endsWith('.tsx'))
    .map((f) => ({ name: `${dir}/${f}`, text: readFileSync(new URL(`../${dir}/${f}`, import.meta.url), 'utf8') })),
);

/** Where a plain box is kept, and why: a file picked, a colour, or a tick a shift-click runs along. */
const OWN_BOXES = { 'src/panels/RunLogPanel.tsx': 'a shift-click ticks every run between' };

test('a panel takes its word boxes and ticks from the kit, never shadcn or a plain box', () => {
  const found = [];
  for (const { name, text } of files) {
    if (/from '[^']*ui\/(input|checkbox)'/.test(text)) found.push(`${name}: imports a shadcn box`);
    for (const m of text.matchAll(/<input\s[^>]*/g)) {
      if (/type="(file|color|range)"/.test(m[0])) continue;
      if (OWN_BOXES[name]) continue;
      found.push(`${name}: a plain <input>`);
    }
    if (/<select\b/.test(text)) found.push(`${name}: a system <select>`);
  }
  assert.deepEqual(found, []);
});

test("a panel has none of its own look-alikes of the kit's parts", () => {
  const LOOK_ALIKES = /^function (NumberBox|SmallButton|SmallInput|IconButton|Choice|Heading|Labelled|Section|Field|Flag|OpacitySlider|SurfaceOptions)\b/m;
  const found = files.filter(({ text }) => LOOK_ALIKES.test(text)).map(({ name, text }) => `${name}: ${text.match(LOOK_ALIKES)[1]}`);
  assert.deepEqual(found, []);
});

test("a dialog is the kit's frame", () => {
  const found = files.filter(({ text }) => /<Dialog\.(Root|Content)\b/.test(text)).map(({ name }) => name);
  assert.deepEqual(found, []);
});

test('what a run said is drawn by the one line, never by a panel picking its own colour', () => {
  const found = files.filter(({ text }) => /[sS]aid\.(refused|error)\b|[sS]aid\.tone ===|\{\w*[sS]aid && <(span|div|p)\b/.test(text)).map(({ name }) => name);
  assert.deepEqual(found, []);
});

test("a reply says it was refused in red and what it did in green, a thrown error refused", () => {
  assert.deepEqual(saidOf({ error: 'No solids picked' }), { text: 'No solids picked', tone: 'refused' });
  assert.deepEqual(saidOf({ record: { messages: ['3 solids made'] } }), { text: '3 solids made', tone: 'ok' });
  assert.deepEqual(saidOf({ record: { messages: ['3 solids made'] } }, 'Saved'), { text: 'Saved', tone: 'ok' });
  assert.deepEqual(saidOf({}), { text: 'Done', tone: 'ok' });
  assert.deepEqual(saidOfError(new Error('the file is in use')), { text: 'the file is in use', tone: 'refused' });
  assert.deepEqual(saidOfError('timed out'), { text: 'timed out', tone: 'refused' });
  assert.deepEqual([ok('a'), warning('b'), refused('c'), quiet('d')].map((s) => s.tone), ['ok', 'warning', 'refused', 'quiet']);
  // Each tone in the theme's one colour for it.
  assert.deepEqual(SAID_CLASS, { ok: 'text-ok', warning: 'text-warning', refused: 'text-destructive', quiet: 'text-muted-foreground' });
});
