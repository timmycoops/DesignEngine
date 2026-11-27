// The app's look (Two sides plan, workstream 1): light by default, dark as a setting, the view's colours read from
// the look in use, and what the view draws readable on its ground in both. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { lookNow, onLook, palettes, setLook, view } from '../src/viewport/look.ts';

/** WCAG relative luminance of a 0xRRGGBB colour. */
function luminance(colour) {
  const lin = (c) => {
    const v = c / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin((colour >> 16) & 255) + 0.7152 * lin((colour >> 8) & 255) + 0.0722 * lin(colour & 255);
}
const contrast = (a, b) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

test('the app starts light when nothing is saved on the machine', () => {
  assert.equal(lookNow(), 'light');
  assert.equal(view.background, palettes.light.background);
  assert.equal(view.ink, palettes.light.ink);
});

test('both looks give every colour the view draws', () => {
  const keys = (p) => [...Object.keys(p), ...Object.keys(p.snap).map((k) => `snap.${k}`)].sort();
  assert.deepEqual(keys(palettes.light), keys(palettes.dark));
  assert.notEqual(palettes.light.background, palettes.dark.background);
  assert.notEqual(palettes.light.ink, palettes.dark.ink);
});

test('what the view draws reads on its ground in both looks', () => {
  for (const [name, p] of Object.entries(palettes)) {
    assert.ok(contrast(p.ink, p.background) >= 7, `${name}: ink on the ground`);
    assert.ok(contrast(p.selected, p.background) >= 2.5, `${name}: the picked gold on the ground`);
    assert.ok(contrast(p.preview, p.background) >= 2.5, `${name}: a string being drawn`);
    assert.ok(contrast(p.sectionGuide, p.sectionGuideUnder) >= 7, `${name}: the section guide over its under-line`);
    for (const [kind, colour] of Object.entries(p.snap)) assert.ok(contrast(colour, p.background) >= 2.5, `${name}: snap to ${kind}`);
  }
});

test('a change of look carries through to the view and tells whoever draws, then back', () => {
  const told = [];
  const stop = onLook((look) => told.push(look));
  setLook('dark');
  assert.equal(lookNow(), 'dark');
  assert.equal(view.background, palettes.dark.background);
  assert.equal(view.snap.vertex, palettes.dark.snap.vertex);
  setLook('dark');
  setLook('light');
  stop();
  setLook('dark');
  assert.deepEqual(told, ['dark', 'light']);
  setLook('light');
  assert.equal(view.selected, palettes.light.selected);
  assert.notEqual(palettes.light.snap, view.snap, 'the palette itself is never handed out to be changed');
});
