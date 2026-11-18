// `--string-edits`: the String tools plan's workstream 1, driven as Tim would. Three graded toes picked by a click and
// two Ctrl-clicks are trimmed back to a pit limit by Trim's button, a click on the limit and a click on the side to keep;
// one Ctrl+Z puts all three back. Extend runs them on to a line by one click on it, at their own grade. Esc in the middle
// of a trim lets it go and changes nothing. Keep both splits them at the limit. Split at a click puts a vertex in where
// it's clicked, and Join puts the two back as one, ends within its tolerance sharing a vertex. Round corners (workstream
// 2) rounds a dog-leg's corner at a radius typed, every arc vertex that far from the centre; Ctrl+Z puts it back.
import * as THREE from 'three/webgpu';
import { objectDetails, projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkStringEdits(viewport: Viewport, log: (line: string) => void, show: Show, selection: () => string[]) {
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`string edits: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('string edits: a topography at RL 100, three toes at 10 %, a pit limit across them and a line past them');
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E - 100, N - 100, 100], [E + 300, N - 100, 100], [E + 300, N + 200, 100], [E - 100, N + 200, 100]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const make = async (name: string, points: Vec3[], layer = 'toes') => (await runCommand('string.create', { name, layer, closed: false, points })).record.outputs[0].id as string;
  const toeY = [0, 30, 60];
  const toes: string[] = [];
  for (const [k, y] of toeY.entries()) toes.push(await make(`Toe ${k + 1}`, [[E, N + y, 100], [E + 50, N + y, 105], [E + 100, N + y, 110]]));
  const limit = await make('Pit limit', [[E + 60, N - 50, 100], [E + 60, N + 150, 100]], 'limits');
  const ramp = await make('Ramp line', [[E + 150, N - 50, 100], [E + 150, N + 150, 100]], 'limits');
  const dogleg = await make('Dog-leg', [[E - 60, N + 170, 100], [E + 40, N + 170, 100], [E + 40, N + 250, 100]]);
  const summary = await projectSummary();
  await show(summary);
  await wait(1500);
  const origin = summary.origin ?? [0, 0, 0];
  const screen = (p: Vec3) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
  const settle = async () => {
    await wait(400);
    await viewport.nextFrame();
  };
  const click = async (at: Vec3, ctrlKey = false) => {
    const [x, y] = screen(at);
    for (const type of ['pointerdown', 'pointerup']) {
      canvas.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true, ctrlKey }));
    }
    await settle();
  };
  const key = async (name: string, ctrlKey = false) => {
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ctrlKey }));
    await settle();
  };
  const panel = () => document.querySelector<HTMLElement>('[data-tool-panel="strings"]');
  const goButton = () => [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].at(-1) ?? null;
  const said = () => panel()?.textContent ?? '';
  const footer = () => document.querySelector('footer')?.textContent ?? '';
  const useTool = async (key: string) => {
    document.querySelector<HTMLButtonElement>('[role="tab"][data-stage="strings"]')?.click();
    await wait(200);
    document.querySelector<HTMLButtonElement>(`[data-slot="ribbon"] [data-tool="strings:${key}"]`)?.click();
    await wait(500);
  };
  const choose = async (group: string, label: string) => {
    [...(panel()?.querySelectorAll<HTMLButtonElement>(`[role="group"][aria-label="${group}"] button`) ?? [])].find((b) => b.textContent?.trim() === label)?.click();
    await settle();
  };
  const points = async (id: string) => {
    const measures = (await objectDetails(id)).measures;
    return measures?.kind === 'string' ? measures.points : [];
  };
  const near = (a: number, b: number, within = 1e-6) => Math.abs(a - b) < within;
  const pickToes = async () => {
    await click([E + 20, N + toeY[0], 102]);
    await click([E + 20, N + toeY[1], 102], true);
    await click([E + 20, N + toeY[2], 102], true);
  };

  await key('t');
  viewport.fitView();
  await settle();

  // Trim: the toes picked, then a click on the limit and one on the side to keep.
  await useTool('trim');
  await pickToes();
  const button = goButton()?.textContent?.trim() ?? '';
  expect('three toes picked, and Trim says it trims them', selection().length === 3 && button === 'Trim 3 strings', { picked: selection().length, button });
  goButton()?.click();
  await settle();
  const askedForLimit = footer().includes('Click the string that cuts 3 strings');
  await click([E + 60, N + 120, 100]);
  const askedForSide = footer().includes('Click on the side of Pit limit to keep');
  await click([E + 10, N + 120, 100]);
  await wait(800);
  const ends = await Promise.all(toes.map(async (id) => (await points(id)).at(-1) ?? [0, 0, 0]));
  expect(
    'a click on the limit and one on the side to keep trim all three back to it, each at its own height there, RL 106',
    askedForLimit && askedForSide && ends.every((p, k) => near(p[0], E + 60) && near(p[1], N + toeY[k]) && near(p[2], 106)) && said().includes('trimmed 3 of 3 strings'),
    { asked: [askedForLimit, askedForSide], ends: ends.map((p) => [p[0] - E, p[1] - N, p[2]]) },
  );
  await key('z', true);
  await wait(500);
  const back = await Promise.all(toes.map(async (id) => (await points(id)).at(-1)?.[0] ?? 0));
  expect('one Ctrl+Z puts all three back', back.every((x) => near(x, E + 100)), back.map((x) => x - E));

  // Esc while Trim waits on its click lets it go and changes nothing.
  await pickToes();
  goButton()?.click();
  await settle();
  await key('Escape');
  const stillThere = await Promise.all(toes.map(async (id) => (await points(id)).length));
  expect('Esc while Trim waits on a click lets it go, and nothing changes', footer().includes('Pick cancelled') && stillThere.every((n) => n === 3), { footer: footer().slice(0, 40), vertices: stillThere });

  // Extend: one click on the line to run them on to.
  await useTool('extend');
  await pickToes();
  const extendButton = goButton()?.textContent?.trim() ?? '';
  goButton()?.click();
  await settle();
  await click([E + 150, N + 120, 100]);
  await wait(800);
  const runOn = await Promise.all(toes.map(async (id) => (await points(id)).at(-1) ?? [0, 0, 0]));
  expect(
    'Extend runs all three on to the line by one click on it, at their own grade: RL 115, 150 m along',
    extendButton === 'Run 3 strings on' && runOn.every((p, k) => near(p[0], E + 150) && near(p[1], N + toeY[k]) && near(p[2], 115)),
    { button: extendButton, ends: runOn.map((p) => [p[0] - E, p[1] - N, p[2]]) },
  );

  // Keep both: split where the limit crosses them.
  await useTool('trim');
  await pickToes();
  await choose('What the trim keeps', 'Both, split');
  const bothButton = goButton()?.textContent?.trim() ?? '';
  goButton()?.click();
  await settle();
  await click([E + 60, N + 120, 100]);
  await wait(800);
  const parts = (await projectSummary()).objects.filter((o) => o.kind === 'string' && / part 2$/.test(o.name));
  expect('Keep both splits each where the limit crosses it, a new part for each', bothButton === 'Split 3 strings at a string' && parts.length === 3, { button: bothButton, parts: parts.map((o) => o.name) });
  await key('z', true);
  await wait(500);

  // Split at a click, then Join the two back as one.
  await useTool('split');
  await click([E + 20, N, 102]);
  const splitButton = goButton()?.textContent?.trim() ?? '';
  goButton()?.click();
  await settle();
  await click([E + 30, N, 103]);
  await wait(800);
  const first = await points(toes[0]);
  const second = (await projectSummary()).objects.find((o) => o.kind === 'string' && o.name === 'Toe 1 part 2');
  expect(
    'Split at a click puts a vertex in where it is clicked, at the string\'s height',
    splitButton === 'Split Toe 1 at a click' && first.length === 2 && near(first[1][0], E + 30, 0.5) && near(first[1][2], 100 + (first[1][0] - E) / 10, 1e-6) && !!second,
    { button: splitButton, first: first.map((p) => [p[0] - E, p[1] - N, p[2]]), second: !!second },
  );
  if (second) {
    await useTool('join');
    // The second part first, then Toe 1 with Ctrl: the one picked last keeps the result.
    await click([E + 90, N, 109]);
    await click([E + 10, N, 101], true);
    goButton()?.click();
    await wait(800);
    const joined = await points(toes[0]);
    expect('Join puts the two back as one', joined.length === 4 && !(await projectSummary()).objects.some((o) => o.name === 'Toe 1 part 2'), joined.length);
  }
  // Round corners: the dog-leg picked by a click, a radius of 20 m typed, the button pressed; its corner at 40 m east,
  // 170 m north becomes an arc round a centre 20 m back along both legs, its ends kept; one Ctrl+Z puts it back.
  await useTool('fillet');
  await click([E - 30, N + 170, 100]);
  const radiusBox = panel()?.querySelector<HTMLInputElement>('input[aria-label="Corner radius"]') ?? null;
  if (radiusBox) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(radiusBox, '20');
    radiusBox.dispatchEvent(new Event('input', { bubbles: true }));
    radiusBox.dispatchEvent(new Event('change', { bubbles: true }));
  }
  await settle();
  const roundButton = goButton()?.textContent?.trim() ?? '';
  goButton()?.click();
  await wait(800);
  const rounded = await points(dogleg);
  const centre = [E + 20, N + 190];
  const arc = rounded.slice(1, -1);
  expect(
    'Round corners rounds the dog-leg at 20 m: every arc vertex 20 m from the centre, the ends kept',
    roundButton.startsWith('Round the corners of') && arc.length > 5 && arc.every((p) => near(Math.hypot(p[0] - centre[0], p[1] - centre[1]), 20)) && near(rounded[0][0], E - 60) && near(rounded.at(-1)?.[1] ?? 0, N + 250),
    { button: roundButton, vertices: rounded.length },
  );
  await key('z', true);
  await wait(500);
  expect('one Ctrl+Z puts the dog-leg back', (await points(dogleg)).length === 3, (await points(dogleg)).length);
  void limit;
  void ramp;

  results.failures = failures;
  return results;
}
