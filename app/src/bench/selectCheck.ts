// `--select`: selecting several objects and zooming to them (decision 0050), driven as Tim would, through pointer and
// key events on the canvas and the window: a Shift-drag box in 3D and in plan, Ctrl+Shift to add a box, Ctrl-click on
// solids and a string, F and Shift+F, Esc. It reads back what the app selected, where the camera went, and the colour
// the selected solids are drawn in. New projects colour by RL with no contour lines, which it checks first. Then the
// view keys: T into plan and out, Z held with a left drag tilting out of plan into 3D about the point under the cursor,
// 2 for a section, and E to zoom to everything.
import * as THREE from 'three/webgpu';
import { objectsBounds, projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;
/** The cubes' side and how far apart they stand, west to east (m). */
const SIDE = 20;
const SPACING = 100;
const CUBES = 6;
/** The gold of what's picked (`SELECTED` in load.ts). */
const GOLD: [number, number, number] = [0xff, 0xd2, 0x3f];

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** A cube's eight corners, world coordinates. */
function cornersOf(k: number): Vec3[] {
  const [x, y] = [E + k * SPACING, N];
  return Array.from({ length: 8 }, (_, i) => [x + (i & 1) * SIDE, y + ((i >> 1) & 1) * SIDE, 100 + ((i >> 2) & 1) * SIDE] as Vec3);
}

export async function checkSelect(viewport: Viewport, log: (line: string) => void, show: Show, selection: () => string[]) {
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`select: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('select: a topography, six cubes in a row and two strings');
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E - 200, N - 300, 100], [E + 800, N - 300, 100], [E + 800, N + 300, 100], [E - 200, N + 300, 100]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const faces = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
  const cubes: string[] = [];
  for (let k = 0; k < CUBES; k++) cubes.push((await runCommand('solid.from_mesh', { name: `Block ${k + 1}`, layer: 'blocks', vertices: cornersOf(k), triangles: faces })).record.outputs[0].id);
  const lineAt = (y: number): Vec3[] => [[E, N + y, 130], [E + (CUBES - 1) * SPACING + SIDE, N + y, 130]];
  const strings: string[] = [];
  for (const [k, y] of [80, 160].entries()) strings.push((await runCommand('string.create', { name: `Toe ${k + 1}`, layer: 'toes', closed: false, points: lineAt(y) })).record.outputs[0].id);
  const summary = await projectSummary();
  await show(summary);
  await wait(1500);
  const origin = summary.origin ?? [0, 0, 0];
  const local = (p: Vec3) => new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]);
  const screen = (p: Vec3) => viewport.projectToClient(local(p));

  expect('a new project has no contour lines', summary.display.contours.edge_lines === false, summary.display.contours);

  const pointer = (type: string, x: number, y: number, keys: { shift?: boolean; ctrl?: boolean }) =>
    canvas.dispatchEvent(
      new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true, shiftKey: !!keys.shift, ctrlKey: !!keys.ctrl }),
    );
  const settle = async () => {
    await wait(400);
    await viewport.nextFrame();
  };
  /** A Shift-drag from one corner to the other, through a few moves. How long until the selection came back. */
  const drag = async (from: [number, number], to: [number, number], ctrl = false) => {
    const started = performance.now();
    const before = selection();
    pointer('pointerdown', from[0], from[1], { shift: true, ctrl });
    for (let k = 1; k <= 4; k++) pointer('pointermove', from[0] + ((to[0] - from[0]) * k) / 4, from[1] + ((to[1] - from[1]) * k) / 4, { shift: true, ctrl });
    pointer('pointerup', to[0], to[1], { shift: true, ctrl });
    for (let k = 0; k < 100 && selection() === before; k++) await wait(5);
    const ms = Math.round(performance.now() - started);
    await settle();
    return ms;
  };
  const click = async (at: [number, number], ctrl = false) => {
    pointer('pointerdown', at[0], at[1], { ctrl });
    pointer('pointerup', at[0], at[1], { ctrl });
    await settle();
  };
  const key = async (name: string, shift = false) => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: name, shiftKey: shift, bubbles: true }));
    await settle();
  };
  /** The toolbar's plan button says Plan or 3D. */
  const toolbarSays = (word: string) => [...document.querySelectorAll('header button')].some((b) => b.textContent?.trim() === word);
  /** The screen box round some cubes, a few pixels out. */
  const around = (ks: number[]): [[number, number], [number, number]] => {
    const points = ks.flatMap((k) => cornersOf(k).map(screen));
    const xs = points.map((p) => p[0]);
    const ys = points.map((p) => p[1]);
    return [
      [Math.min(...xs) - 6, Math.min(...ys) - 6],
      [Math.max(...xs) + 6, Math.max(...ys) + 6],
    ];
  };
  const topOf = (k: number): Vec3 => [E + k * SPACING + SIDE / 2, N + SIDE / 2, 100 + SIDE];
  const same = (got: string[], want: string[]) => got.length === want.length && want.every((id) => got.includes(id));
  const names = (ids: string[]) => ids.map((id) => (cubes.includes(id) ? `Block ${cubes.indexOf(id) + 1}` : strings.includes(id) ? `Toe ${strings.indexOf(id) + 1}` : id));

  // 3D, from the usual south-east.
  viewport.fitView();
  await settle();
  const boxMs = await drag(...around([0, 1]));
  expect('a box round two blocks in 3D selects those two alone', same(selection(), [cubes[0], cubes[1]]), { selected: names(selection()), ms: boxMs });
  await drag(...around([2]), true);
  expect('Ctrl+Shift adds the blocks in a second box', same(selection(), cubes.slice(0, 3)), names(selection()));
  await click(screen(topOf(3)), true);
  await click(screen(topOf(0)), true);
  expect('Ctrl-click adds a block, and takes one off again', same(selection(), [cubes[1], cubes[2], cubes[3]]), names(selection()));
  const [a, b] = lineAt(160);
  await click(screen([(a[0] + b[0]) / 2 + 10, a[1], a[2]]), true);
  expect('Ctrl-click adds a string', same(selection(), [cubes[1], cubes[2], cubes[3], strings[1]]), names(selection()));
  expect('Properties shows them together', document.body.textContent?.includes('4 selected') ?? false);

  // The selected blocks draw gold over their layer; the others don't.
  for (let i = 0; i < 20; i++) await viewport.nextFrame();
  viewport.draw();
  const picture = new Image();
  picture.src = canvas.toDataURL('image/png');
  await picture.decode();
  const pixels = Object.assign(document.createElement('canvas'), { width: canvas.width, height: canvas.height }).getContext('2d')!;
  pixels.drawImage(picture, 0, 0);
  const rect = canvas.getBoundingClientRect();
  const colourAt = (p: [number, number]) => {
    const data = pixels.getImageData(Math.round(((p[0] - rect.left) * canvas.width) / rect.width), Math.round(((p[1] - rect.top) * canvas.height) / rect.height), 1, 1).data;
    return [data[0], data[1], data[2]];
  };
  const fromGold = (c: number[]) => Math.hypot(c[0] - GOLD[0], c[1] - GOLD[1], c[2] - GOLD[2]);
  const lit = [1, 2, 3].map((k) => colourAt(screen(topOf(k))));
  const unlit = [0, 4, 5].map((k) => colourAt(screen(topOf(k))));
  expect(
    'the selected blocks draw nearer gold than the others',
    Math.max(...lit.map(fromGold)) < Math.min(...unlit.map(fromGold)),
    { selected: lit, others: unlit },
  );

  // F frames what's selected; Shift+F everything.
  const ids = selection();
  const box = await objectsBounds(ids);
  await key('f');
  const target = viewport.controls.target;
  const centre = box ? local([(box[0][0] + box[1][0]) / 2, (box[0][1] + box[1][1]) / 2, (box[0][2] + box[1][2]) / 2]) : null;
  const corners = box ? [0, 1, 2, 3, 4, 5, 6, 7].map((i) => local([box[i & 1][0], box[(i >> 1) & 1][1], box[(i >> 2) & 1][2]]).project(viewport.camera)) : [];
  expect('F looks at the middle of what is selected', !!centre && target.distanceTo(centre) < 1e-6, centre && target.distanceTo(centre));
  expect(
    'F has all of it on screen',
    corners.length === 8 && corners.every((c) => Math.abs(c.x) <= 1 && Math.abs(c.y) <= 1),
    corners.map((c) => [Number(c.x.toFixed(3)), Number(c.y.toFixed(3))]),
  );
  await key('F', true);
  const everything = viewport.bounds.getCenter(new THREE.Vector3());
  expect('Shift+F looks at the middle of everything', target.distanceTo(everything) < 1e-6, target.distanceTo(everything));
  await key('Escape');
  expect('Esc clears the selection', selection().length === 0, names(selection()));

  // Plan: a box round the last two blocks, then F.
  viewport.setPlanView(true);
  viewport.fitView();
  await settle();
  await drag(...around([4, 5]));
  expect('a box in plan selects the blocks inside it', same(selection(), [cubes[4], cubes[5]]), names(selection()));
  await key('f');
  const planCentre = local([E + 4.5 * SPACING + SIDE / 2, N + SIDE / 2, 110]);
  expect('F in plan centres on them', Math.hypot(target.x - planCentre.x, target.y - planCentre.y) < 1e-6, [target.x - planCentre.x, target.y - planCentre.y]);
  const everythingBox = await drag(...around([0, 1, 2, 3, 4, 5]));
  expect('a box round every block in plan takes all six and not the topography under them', same(selection(), cubes), { selected: names(selection()), ms: everythingBox });
  viewport.setPlanView(false);
  await key('Escape');

  // T flips plan and 3D, and the toolbar follows.
  viewport.fitView();
  await settle();
  await key('t');
  expect('T goes into plan, and the toolbar says so', viewport.planView && toolbarSays('Plan'), { plan: viewport.planView });
  await key('t');
  expect('T again comes back to 3D', !viewport.planView && toolbarSays('3D'), { plan: viewport.planView });

  // Z held, a left drag in plan: out of plan into 3D, turning about what's under the cursor, which stays put on screen.
  await key('t');
  viewport.fitView();
  await settle();
  const press = screen(topOf(2));
  const under = await viewport.pickPoint?.(press[0], press[1]);
  const before = selection();
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', bubbles: true }));
  pointer('pointerdown', press[0], press[1], {});
  await wait(100);
  for (let k = 1; k <= 10; k++) pointer('pointermove', press[0] + 8 * k, press[1] - 15 * k, {});
  await wait(100);
  pointer('pointerup', press[0] + 80, press[1] - 150, {});
  window.dispatchEvent(new KeyboardEvent('keyup', { key: 'z', bubbles: true }));
  await settle();
  const forward = viewport.camera.getWorldDirection(new THREE.Vector3());
  const tilt = THREE.MathUtils.radToDeg(forward.angleTo(new THREE.Vector3(0, 0, -1)));
  expect('Z and a drag in plan tilt out of it into 3D, and the toolbar says 3D', !viewport.planView && toolbarSays('3D') && tilt > 10, { tilt_from_straight_down_deg: Number(tilt.toFixed(1)) });
  const pivot = under ? viewport.projectToClient(new THREE.Vector3(...under)) : null;
  const moved = pivot ? Math.hypot(pivot[0] - press[0], pivot[1] - press[1]) : null;
  expect('the point under the cursor stays put through the tilt (px)', moved !== null && moved < 2, moved === null ? null : Number(moved.toFixed(3)));
  expect('Z and a drag select nothing', same(selection(), before), names(selection()));
  // Let go of Z: in plan a left drag pans again.
  await key('t');
  const panFrom = viewport.controls.target.clone();
  pointer('pointerdown', press[0], press[1], {});
  for (let k = 1; k <= 4; k++) pointer('pointermove', press[0] + 20 * k, press[1], {});
  pointer('pointerup', press[0] + 80, press[1], {});
  await settle();
  expect('without Z, a left drag in plan pans and stays in plan', viewport.planView && viewport.controls.target.distanceTo(panFrom) > 1, Number(viewport.controls.target.distanceTo(panFrom).toFixed(2)));
  viewport.setPlanView(false);

  // 2 starts a section's picking and stops it; with two points picked a section opens, and 2 closes it.
  const footer = () => document.querySelector('footer')?.textContent ?? '';
  viewport.fitView();
  await settle();
  await key('2');
  expect('2 starts picking a section', footer().includes('Section: pick the first point'), footer().slice(0, 60));
  await key('2');
  expect('2 again stops picking', footer().startsWith('Select:'), footer().slice(0, 60));
  await key('2');
  await click(screen(topOf(0)));
  await click(screen(topOf(5)));
  for (let k = 0; k < 40 && !toolbarSays('Fill'); k++) await wait(100);
  expect('two points picked open a section', toolbarSays('Fill'));
  await key('2');
  expect('2 closes the section', !toolbarSays('Fill'));

  // E zooms to everything, from wherever the view is.
  await settle();
  const all = viewport.bounds.getCenter(new THREE.Vector3());
  viewport.focusOn([all.x + 300, all.y - 200, all.z], 100);
  await settle();
  const away = viewport.controls.target.distanceTo(all);
  await key('e');
  expect('E zooms to everything', away > 100 && viewport.controls.target.distanceTo(all) < 1e-6, { before_m: Number(away.toFixed(1)), after_m: viewport.controls.target.distanceTo(all) });

  results.failures = failures;
  return results;
}
