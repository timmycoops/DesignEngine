// `--measure`: the distance measure on the view bar (decision 0079), driven as Tim would, by the M key, the bar's
// button and clicks in the view. A level topography at RL 100 and a string from one corner of a 3-4-5 triangle to
// the other, 50 m higher: its two ends clicked read plan 500 m, slope 502.49 m, RL +50, 10 % and a bearing of 036.9°.
// A click on gives a running total, Backspace takes a point off, Esc clears, and nothing is made.
import * as THREE from 'three/webgpu';
import { projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { StringLayer } from '../viewport/strings';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkMeasure(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`measure: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('measure: a topography at RL 100 and a string up a 3-4-5 triangle');
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E - 200, N - 200, 100], [E + 600, N - 200, 100], [E + 600, N + 600, 100], [E - 200, N + 600, 100]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const [from, to]: Vec3[] = [[E, N, 100], [E + 300, N + 400, 150]];
  await runCommand('string.create', { name: 'Ramp', layer: 'design', closed: false, points: [from, to] });
  const summary = await projectSummary();
  await show(summary);
  await wait(1500);
  const before = summary.objects.length;
  const origin = summary.origin ?? [0, 0, 0];
  const screen = (p: Vec3) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
  const pointer = (type: string, x: number, y: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' || type === 'pointermove' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true }));
  const settle = async () => {
    await wait(400);
    await viewport.nextFrame();
  };
  const click = async (at: Vec3) => {
    const [x, y] = screen(at);
    pointer('pointerdown', x, y);
    pointer('pointerup', x, y);
    await settle();
  };
  const key = async (name: string) => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true }));
    await settle();
  };
  const footer = () => document.querySelector('footer')?.textContent ?? '';
  const button = () => document.querySelector<HTMLButtonElement>('[data-slot="view-bar"] [aria-label="Measure distance"]');
  const lit = () => button()?.getAttribute('aria-pressed') === 'true';

  // Plan view, everything in it.
  await key('t');
  viewport.fitView();
  await settle();

  expect('Measure is on the view bar, not lit', !!button() && !lit());
  await key('m');
  expect('M starts measuring: the button lights and the status line says how', lit() && footer().includes('Measure: click two points'), footer().slice(0, 90));

  // The string's two ends, snapped to its vertices.
  await click(from);
  expect('the first click says where it starts', footer().includes('Measure: from RL 100.00'), footer().slice(0, 90));
  const [hx, hy] = screen([E + 150, N + 200, 125]);
  pointer('pointermove', hx, hy);
  await settle();
  const label = layer.cursorLabel;
  expect('moving on reads the leg out beside the cursor', !!label && /m plan · .* m slope · RL /.test(label), label);
  await click(to);
  const said = 'plan 500.00 m · slope 502.49 m · RL +50.00 m · 10.0 % up (1 in 10.0, 5.7°) · bearing 036.9°';
  expect('the two ends read the 3-4-5 leg exactly', footer().includes(said), footer().slice(0, 140));
  const points = layer.measured;
  expect('from the points as the engine has them, not the screen', points.length === 2 && points[0].every((c, i) => c === from[i]) && points[1].every((c, i) => c === to[i]), points);

  // A click on, on the topography 300 m east of the top: a running total.
  await click([E + 600, N + 400, 100]);
  expect('a click on adds a leg and a total', footer().includes('Total over 2 legs: plan 800.00 m') && footer().includes('RL +0.00 m'), footer().slice(0, 200));
  await key('Backspace');
  expect('Backspace takes the last point off', layer.measured.length === 2 && footer().includes(said), footer().slice(0, 140));
  await key('Escape');
  expect('Esc clears it', layer.measured.length === 0 && footer().includes('Measure: click the first point'), footer().slice(0, 60));

  // From the bar's button too, and nothing was made.
  await key('m');
  expect('M again stops measuring', !lit() && footer().startsWith('Select:'), footer().slice(0, 40));
  button()?.click();
  await settle();
  expect('the button starts it', lit());
  button()?.click();
  await settle();
  expect('and stops it', !lit());
  const after = (await projectSummary()).objects.length;
  expect('measuring makes no object', after === before, { before, after });

  results.failures = failures;
  return results;
}
