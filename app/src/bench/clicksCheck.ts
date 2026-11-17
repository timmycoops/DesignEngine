// `--clicks`: how the view takes clicks and Esc (decision 0080), driven as Tim would. NS (N then S) starts a new
// string without turning snap off; left clicks add points and a right click finishes it; a right drag pans and makes
// nothing; Esc takes back one thing a press and says so: the points, then Draw back to Select, then the selection, and
// from a box on a tool panel too. A right click ends a measure, and the next click starts another. Delete deletes
// what's selected in one undo, or the vertex under the cursor on the selected string.
import * as THREE from 'three/webgpu';
import { projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { StringLayer } from '../viewport/strings';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkClicks(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null, selection: () => string[]) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`clicks: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('clicks: a topography at RL 100');
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E - 200, N - 200, 100], [E + 600, N - 200, 100], [E + 600, N + 600, 100], [E - 200, N + 600, 100]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const summary = await projectSummary();
  await show(summary);
  await wait(1500);
  const origin = summary.origin ?? [0, 0, 0];
  const screen = (p: Vec3) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
  const pointer = (type: string, x: number, y: number, button: 0 | 2) =>
    canvas.dispatchEvent(
      new PointerEvent(type, { clientX: x, clientY: y, button, buttons: type === 'pointerup' || (type === 'pointermove' && button === 0) ? 0 : button === 2 ? 2 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true }),
    );
  const settle = async () => {
    await wait(400);
    await viewport.nextFrame();
  };
  const click = async (at: Vec3, button: 0 | 2 = 0) => {
    const [x, y] = screen(at);
    pointer('pointerdown', x, y, button);
    pointer('pointerup', x, y, button);
    await settle();
  };
  /** A key pressed where the focus is, as the keyboard sends it. */
  const key = async (name: string, on: Element = document.activeElement ?? document.body, ctrlKey = false) => {
    on.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ctrlKey }));
    await settle();
  };
  const footer = () => document.querySelector('footer')?.textContent ?? '';
  const lit = (label: string) => document.querySelector(`[data-slot="view-bar"] [aria-label="${label}"]`)?.getAttribute('aria-pressed') === 'true';
  const strings = async () => (await projectSummary()).objects.filter((o) => o.kind === 'string');

  await key('t');
  viewport.fitView();
  await settle();

  // NS: N waits for the second letter, and S then is the shortcut's, not snap's.
  const snapBefore = lit('Snap');
  await key('n');
  expect('N says it waits for the next letter', footer().includes('N… then S Draw string'), footer().slice(0, 60));
  await key('s');
  expect('NS starts drawing a new string', lit('Draw string') && footer().startsWith('Draw:'), footer().slice(0, 60));
  expect('and the S did not turn snap over', lit('Snap') === snapBefore);
  await key('n');
  await key('Escape');
  expect('Esc after N lets it go, and Draw stays on', footer().includes('Shortcut let go') && lit('Draw string'), footer().slice(0, 40));

  // Left adds, right finishes.
  const had = new Set((await strings()).map((o) => o.id));
  const before = had.size;
  await click([E, N, 100]);
  await click([E + 200, N, 100]);
  await click([E + 200, N + 200, 100]);
  expect('three clicks, three points, and right-click is offered', footer().startsWith('3 vertices') && footer().includes('right-click'), footer().slice(0, 80));
  await click([E + 200, N + 200, 100], 2);
  const made = await strings();
  const string = made.find((o) => !had.has(o.id));
  expect('a right click finishes the string', made.length === before + 1 && string?.vertices_local?.length === 3, { strings: made.length, vertices: string?.vertices_local?.length });
  expect('still drawing, for the next one', lit('Draw string'));

  // A right drag pans: no string, the points kept.
  await click([E, N + 300, 100]);
  await click([E + 200, N + 300, 100]);
  const [rx, ry] = screen([E + 300, N + 300, 100]);
  pointer('pointerdown', rx, ry, 2);
  pointer('pointermove', rx + 60, ry + 40, 2);
  pointer('pointerup', rx + 60, ry + 40, 2);
  await settle();
  expect('a right drag pans and makes nothing', (await strings()).length === before + 1);

  // Esc, a step a press.
  await key('Escape');
  expect('Esc abandons the points', footer().includes('String abandoned') && lit('Draw string'), footer().slice(0, 40));
  expect('nothing made from them', (await strings()).length === before + 1);
  await key('Escape');
  expect('Esc again: back to Select', lit('Select') && !lit('Draw string') && footer().startsWith('Select:'), footer().slice(0, 40));
  if (string) layer.select(string.id);
  await settle();
  expect('the string just made, picked', !!string && selection().includes(string.id), selection());
  await key('Escape');
  expect('Esc again: the selection clears, and it says so', selection().length === 0 && footer().includes('Nothing selected'), footer().slice(0, 40));

  // From a box on a tool panel: Esc leaves the box and clears the selection.
  if (string) layer.select(string.id);
  await settle();
  document.querySelector<HTMLButtonElement>('[role="tab"][data-stage="strings"]')?.click();
  await wait(200);
  document.querySelector<HTMLButtonElement>('[data-slot="ribbon"] [data-tool="strings:offset"]')?.click();
  await wait(500);
  const box = document.querySelector<HTMLInputElement>('[data-tool-panel="strings"] input');
  box?.focus();
  await settle();
  expect('a box on the Strings panel has the cursor, a string picked', !!box && document.activeElement === box && selection().length === 1, { box: !!box, picked: selection().length });
  if (box) await key('Escape', box);
  expect('Esc in it leaves the box and clears the selection', document.activeElement !== box && selection().length === 0 && footer().includes('Nothing selected'), {
    picked: selection().length,
    footer: footer().slice(0, 40),
  });

  // Delete: what's selected, in one undo; over a handle of the selected string, that vertex.
  const make = async (name: string, points: Vec3[]) => (await runCommand('string.create', { name, layer: 'design', closed: false, points })).record.outputs[0].id as string;
  const one = await make('Delete one', [[E, N + 400, 100], [E + 100, N + 400, 100]]);
  const two = await make('Delete two', [[E, N + 450, 100], [E + 100, N + 450, 100]]);
  const three = await make('Delete a vertex', [[E + 300, N + 450, 100], [E + 350, N + 450, 100], [E + 400, N + 450, 100]]);
  await show(await projectSummary());
  await settle();
  const alive = async () => new Set((await strings()).map((o) => o.id));
  layer.select(one);
  await settle();
  const [cx, cy] = screen([E + 50, N + 450, 100]);
  canvas.dispatchEvent(new PointerEvent('pointerdown', { clientX: cx, clientY: cy, button: 0, buttons: 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true, ctrlKey: true }));
  canvas.dispatchEvent(new PointerEvent('pointerup', { clientX: cx, clientY: cy, button: 0, buttons: 0, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true, ctrlKey: true }));
  await settle();
  expect('two strings picked, the second by Ctrl+click', selection().length === 2 && selection().includes(one) && selection().includes(two), selection().length);
  await key('Delete', document.body);
  await wait(400);
  let now = await alive();
  expect('Delete deletes both, and says so', !now.has(one) && !now.has(two) && selection().length === 0 && footer().includes('Deleted 2 objects'), footer().slice(0, 60));
  await key('z', document.body, true);
  await wait(400);
  now = await alive();
  expect('one Ctrl+Z brings both back', now.has(one) && now.has(two));
  await key('Delete', document.body);
  expect('Delete with nothing selected says so', footer().includes('Nothing selected to delete'), footer().slice(0, 40));
  layer.select(three);
  await settle();
  const [vx, vy] = screen([E + 350, N + 450, 100]);
  pointer('pointermove', vx, vy, 0);
  await settle();
  await key('Delete', document.body);
  await wait(400);
  const left = (await strings()).find((o) => o.id === three);
  expect('Delete over a handle takes that vertex, not the string', left?.vertices_local?.length === 2, left?.vertices_local?.length);
  canvas.dispatchEvent(new PointerEvent('pointerleave', { bubbles: false }));
  await key('Delete', document.body);
  await wait(400);
  expect('off the view, Delete takes the string', !(await alive()).has(three));

  // A right click ends a measure; the next click starts another.
  await key('m');
  await click([E, N, 100]);
  await click([E + 300, N + 400, 100]);
  await click([E + 300, N + 400, 100], 2);
  expect('a right click ends the measure, its reading kept', layer.measured.length === 2 && footer().includes('plan 500.00 m') && footer().includes('Click to start another'), footer().slice(0, 100));
  await click([E + 100, N, 100]);
  expect('the next click starts another', layer.measured.length === 1, layer.measured.length);
  await key('Escape');
  await key('Escape');
  expect('Esc, Esc: cleared, then back to Select', lit('Select') && !lit('Measure distance'));

  results.failures = failures;
  return results;
}
