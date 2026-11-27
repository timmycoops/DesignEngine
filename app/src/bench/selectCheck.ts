// `--select`: selecting several objects and zooming to them (decision 0050), driven as Tim would, through pointer and
// key events on the canvas and the window: a Shift-drag box in 3D and in plan, Ctrl+Shift to add a box, Ctrl-click on
// solids and a string, F and Shift+F, Esc. It reads back what the app selected, where the camera went, and the colour
// the selected solids are drawn in. New projects colour by RL with no contour lines, which it checks first. Then the
// view keys: T into plan and out, Z held with a left drag tilting out of plan into 3D about the point under the cursor,
// 2 for a section, and E to zoom to everything.
import * as THREE from 'three/webgpu';
import { objectsBounds, projectSummary, type ProjectSummary, runCommand, setObjectsVisible, type Vec3 } from '../bridge';
import { view } from '../viewport/look';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;
/** The cubes' side and how far apart they stand, west to east (m). */
const SIDE = 20;
const SPACING = 100;
const CUBES = 6;
/** The gold of what's picked, in the look in use (`view.selected`, decision 0108). */
const gold = (): [number, number, number] => [(view.selected >> 16) & 255, (view.selected >> 8) & 255, view.selected & 255];

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
  /** The view bar's plan button says Plan or 3D. */
  const toolbarSays = (word: string) => [...document.querySelectorAll('[data-slot="view-bar"] button')].some((b) => b.textContent?.trim() === word);
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
  // Properties brought forward: a bench run before this one may have left another tab in front, as the layout is saved.
  const propertiesTab = [...document.querySelectorAll<HTMLElement>('.dv-tab')].find((t) => t.textContent?.trim() === 'Properties');
  for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) propertiesTab?.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, pointerType: 'mouse' }));
  await settle();
  expect('Properties shows them together', document.body.textContent?.includes('4 selected') ?? false, { said: (document.body.textContent ?? '').match(/\d+ selected/g)?.slice(0, 4) ?? [], tabs: [...document.querySelectorAll('.dv-tab')].map((t) => t.textContent?.trim()).slice(0, 30), active: [...document.querySelectorAll('.dv-tab.dv-active-tab')].map((t) => t.textContent?.trim()) });

  // The selected blocks draw gold over their layer; the others don't.
  const snapshot = async () => {
    for (let i = 0; i < 20; i++) await viewport.nextFrame();
    viewport.draw();
    const picture = new Image();
    picture.src = canvas.toDataURL('image/png');
    await picture.decode();
    const pixels = Object.assign(document.createElement('canvas'), { width: canvas.width, height: canvas.height }).getContext('2d')!;
    pixels.drawImage(picture, 0, 0);
    const rect = canvas.getBoundingClientRect();
    return (p: [number, number]) => {
      const data = pixels.getImageData(Math.round(((p[0] - rect.left) * canvas.width) / rect.width), Math.round(((p[1] - rect.top) * canvas.height) / rect.height), 1, 1).data;
      return [data[0], data[1], data[2]];
    };
  };
  const colourAt = await snapshot();
  const GOLD = gold();
  const fromGold = (c: number[]) => Math.hypot(c[0] - GOLD[0], c[1] - GOLD[1], c[2] - GOLD[2]);
  const lit = [1, 2, 3].map((k) => colourAt(screen(topOf(k))));
  const unlit = [0, 4, 5].map((k) => colourAt(screen(topOf(k))));
  expect(
    'the selected blocks draw nearer gold than the others',
    Math.max(...lit.map(fromGold)) < Math.min(...unlit.map(fromGold)),
    { selected: lit, others: unlit },
  );

  // Hiding a selected block and another changes the layer's table, not its mesh (decision 0060): both go, gold and all,
  // and the rest stay as they were.
  const triangles = viewport.triangleCount;
  const hideStart = performance.now();
  await show(await setObjectsVisible([cubes[2], cubes[5]], false));
  await viewport.nextFrame();
  const hideMs = performance.now() - hideStart;
  const hidden = await snapshot();
  const changedBy = (k: number, before: number[]) => Math.hypot(...hidden(screen(topOf(k))).map((c, i) => c - before[i]));
  expect("hiding two blocks keeps the layer's mesh", viewport.triangleCount === triangles, { before: triangles, after: viewport.triangleCount, ms: Math.round(hideMs) });
  expect(
    'the hidden blocks, one of them selected, are gone and the others stand as they were',
    changedBy(2, lit[1]) > 30 && changedBy(5, unlit[2]) > 30 && [1, 3].every((k, j) => changedBy(k, lit[[0, 2][j]]) < 6) && changedBy(0, unlit[0]) < 6,
    { hidden: [2, 5].map((k) => hidden(screen(topOf(k)))), kept: [0, 1, 3].map((k) => hidden(screen(topOf(k)))) },
  );
  await show(await setObjectsVisible([cubes[2], cubes[5]], true));
  const shownAgain = await snapshot();
  expect('shown again, they come back', Math.hypot(...shownAgain(screen(topOf(2))).map((c, i) => c - lit[1][i])) < 6);

  // Blocks coloured by their value's band, as against a seam grid's contours (decision 0061): two blocks on bands 0 and 1
  // of 2 through their tile's table, nothing else changed; then back.
  // The blocks may lie in more than one plan tile: each tile's table takes the bands of its own.
  const tables = viewport.content.children.map((g) => g.userData.flags as import('../viewport/solidFlags').SolidFlags | null | undefined).filter((f): f is import('../viewport/solidFlags').SolidFlags => !!f);
  const setBands = (band: (k: number) => number | null, count: number) => {
    for (const flags of tables) {
      flags.begin();
      for (const [k, id] of cubes.entries()) flags.put(id, false, false, band(k));
      flags.end();
      flags.bandCount.value = count;
    }
  };
  if (tables.length > 0) {
    const before = await snapshot();
    const [b0, b4, b5] = [before(screen(topOf(0))), before(screen(topOf(4))), before(screen(topOf(5)))];
    setBands((k) => (k === 0 ? 0 : k === 4 ? 1 : null), 2);
    const banded = await snapshot();
    const [c0, c4, c5] = [banded(screen(topOf(0))), banded(screen(topOf(4))), banded(screen(topOf(5)))];
    const apart = (a: number[], b: number[]) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    expect(
      "blocks on two bands take the bands' colours, the rest their own",
      apart(c0, b0) > 20 && apart(c4, b4) > 20 && apart(c0, c4) > 20 && apart(c5, b5) < 6,
      { band0: c0, band1: c4, unbanded: c5 },
    );
    setBands(() => null, 1);
  } else {
    expect("blocks on two bands take the bands' colours, the rest their own", false, 'no table on the blocks');
  }

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
  expect('T goes into plan, and the view bar says so', viewport.planView && toolbarSays('Plan'), { plan: viewport.planView });
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
  expect('Z and a drag in plan tilt out of it into 3D, and the view bar says 3D', !viewport.planView && toolbarSays('3D') && tilt > 10, { tilt_from_straight_down_deg: Number(tilt.toFixed(1)) });
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

  // A block sunk into the ground, its top exactly on the topography, as the overburden's top is: the block shows over
  // its whole top, not patches of it and of the ground.
  const [x0, x1, y0, y1] = [E + 620, E + 700, N - 110, N - 40];
  const sunk = Array.from({ length: 8 }, (_, i) => [i & 1 ? x1 : x0, (i >> 1) & 1 ? y1 : y0, (i >> 2) & 1 ? 100 : 80] as Vec3);
  await runCommand('solid.from_mesh', { name: 'Sunk block', layer: 'sunk', vertices: sunk, triangles: faces });
  await show(await projectSummary());
  viewport.focusOn(local([(x0 + x1) / 2, (y0 + y1) / 2, 100]).toArray() as Vec3, 160);
  await settle();
  const ground = await snapshot();
  const onTop: number[][] = [];
  for (let i = 1; i < 8; i++) for (let j = 1; j < 8; j++) onTop.push(ground(screen([x0 + ((x1 - x0) * i) / 8, y0 + ((y1 - y0) * j) / 8, 100])));
  const topo = [ground(screen([E + 760, N - 75, 100])), ground(screen([E + 560, N - 75, 100]))];
  const apart = (a: number[], b: number[]) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
  const mean = [0, 1, 2].map((c) => onTop.reduce((s, p) => s + p[c], 0) / onTop.length);
  const nearestGround = Math.min(...onTop.map((p) => Math.min(...topo.map((t) => apart(p, t)))));
  const spread = Math.max(...onTop.map((p) => apart(p, mean)));
  expect(
    "a block whose top lies on the topography shows over all of its top, none of the ground's",
    apart(topo[0], topo[1]) < 6 && nearestGround > 20 && spread < 6,
    { block: mean.map(Math.round), ground: topo, nearest_to_ground: Math.round(nearestGround), spread: Number(spread.toFixed(1)), points: onTop.length },
  );

  // Two blocks stacked on one layer, the upper one's floor split on the other diagonal from the lower one's roof (so both
  // are drawn, as seam-cut blocks' are), their long south wall seen from far off and low: along the join the wall shows,
  // never the floor or roof inside it (Tim, 21 November: dashes along the seams on the large mine's block walls).
  {
    const [sx0, sx1, sy0, sy1] = [E + 2600, E + 3000, N - 1500, N - 1460];
    const box = (z0: number, z1: number) => Array.from({ length: 8 }, (_, i) => [i & 1 ? sx1 : sx0, (i >> 1) & 1 ? sy1 : sy0, (i >> 2) & 1 ? z1 : z0] as Vec3);
    const otherFloor = [[0, 2, 1], [1, 2, 3], ...faces.slice(2)];
    await runCommand('solid.from_mesh', { name: 'Lower block', layer: 'stack', vertices: box(0, 60), triangles: faces });
    await runCommand('solid.from_mesh', { name: 'Upper block', layer: 'stack', vertices: box(60, 100), triangles: otherFloor });
    await show(await projectSummary());
    const target = local([(sx0 + sx1) / 2, sy0, 60]);
    viewport.camera.position.copy(target.clone().add(new THREE.Vector3(-0.8, -1, 0.05).normalize().multiplyScalar(3000)));
    viewport.controls.target.copy(target);
    viewport.controls.update();
    await settle();
    const wall = await snapshot();
    const apart = (a: number[], b: number[]) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    let [looked, through] = [0, 0];
    for (let x = sx0 + 20; x <= sx1 - 20; x += 5) {
      const [below, on, above] = [57, 60, 63].map((z) => wall(screen([x, sy0, z])));
      looked++;
      if (apart(on, below) > 20 && apart(on, above) > 20) through++;
    }
    expect('a wall of stacked blocks seen far off and low shows the wall along the join, not the floor or roof inside it', looked > 0 && through <= 1, { looked, through });
  }

  // Drawn far off with fewer triangles (decision 0097): a rolling surface of 178,802 triangles, big enough for coarser
  // versions. Far off it draws fewer than it holds, and the picture is the full one's; close in, the chunk under the
  // camera is full detail; a pick lands where it does at full detail; Full draws every triangle.
  const [gx, gy, side] = [E + 1500, N + 600, 300];
  const vertices: Vec3[] = [];
  for (let r = 0; r < side; r++) for (let c = 0; c < side; c++) vertices.push([gx + c, gy + r, 95 + 6 * Math.sin(c / 23) * Math.cos(r / 31) + 2 * Math.sin((c + r) / 11)]);
  const rollingTriangles: [number, number, number][] = [];
  for (let r = 0; r < side - 1; r++) {
    for (let c = 0; c < side - 1; c++) {
      const at = (cc: number, rr: number) => rr * side + cc;
      rollingTriangles.push([at(c, r), at(c + 1, r), at(c + 1, r + 1)], [at(c, r), at(c + 1, r + 1), at(c, r + 1)]);
    }
  }
  await runCommand('surface.from_mesh', { name: 'Rolling ground', layer: 'ground', vertices, triangles: rollingTriangles });
  await show(await projectSummary());
  const rolling = viewport.content.children.find((g) => g.name === 'Rolling ground');
  type Detail = { full: THREE.Mesh; levels: THREE.Mesh[]; errors: number[]; triangles: number[] }[];
  for (let k = 0; k < 100 && !rolling?.userData.detail; k++) await wait(100);
  const detail = rolling?.userData.detail as Detail | undefined;
  expect('a big surface gets coarser versions once drawn', !!detail && detail.every((c) => c.levels.length >= 3), detail && { chunks: detail.length, versions: detail.map((c) => c.levels.length) });
  if (detail) {
    viewport.setDetail(1, false);
    viewport.fitView();
    await settle();
    const held = viewport.triangleCount;
    const drawnFar = viewport.drawnTriangleCount;
    const coarse = detail.filter((c) => !c.full.visible).length;
    // Which version each chunk is drawn with: 0 the full chunk, k its k-th coarser version.
    const shown = () => detail.map((c) => (c.full.visible ? 0 : c.levels.findIndex((l) => l.visible) + 1));
    const farShown = shown();
    expect('zoomed to everything, it draws far fewer triangles than it holds', drawnFar < held / 3 && coarse > 0, { held, drawn: drawnFar, chunks_coarser: coarse, of: detail.length });
    const status = document.body.textContent?.match(/\d+ fps · [\d.,]+ (?:M )?drawn of [\d.,]+(?: M)?/)?.[0] ?? null;
    expect('the status bar says drawn of held', status !== null, status);
    const coarsePicture = await snapshot();
    viewport.setDetail(0, false);
    await settle();
    const fullPicture = await snapshot();
    expect('at Full every chunk is drawn whole', detail.every((c) => c.full.visible && c.levels.every((l) => !l.visible)) && viewport.drawnTriangleCount >= drawnFar);
    const rect = canvas.getBoundingClientRect();
    let [looked, differ] = [0, 0];
    for (let i = 1; i < 60; i++) {
      for (let j = 1; j < 40; j++) {
        const at: [number, number] = [rect.left + (rect.width * i) / 60, rect.top + (rect.height * j) / 40];
        const [a, b] = [coarsePicture(at), fullPicture(at)];
        looked++;
        if (Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) > 24) differ++;
      }
    }
    expect('the picture at 1 px is the full picture, near enough', differ / looked < 0.02, { points: looked, differ });
    const middle = local([gx + side / 2, gy + side / 2, 95]);
    viewport.focusOn(middle.toArray() as Vec3, 30);
    viewport.setDetail(1, false);
    await settle();
    // Close in, the chunk under the view is drawn finer than far off, and out by at most the pixel.
    const k = detail.findIndex((c) => c.full.geometry.boundingBox!.containsPoint(new THREE.Vector3(middle.x, middle.y, c.full.geometry.boundingBox!.min.z)));
    const closeShown = shown();
    const distance = k < 0 ? 0 : detail[k].full.geometry.boundingBox!.distanceToPoint(viewport.camera.position);
    const perMetre = canvas.height / (2 * distance * Math.tan(THREE.MathUtils.degToRad(viewport.fieldOfView / 2)));
    const outBy = k < 0 || closeShown[k] === 0 ? 0 : detail[k].errors[closeShown[k] - 1] * perMetre;
    expect(
      'close in, the chunk under the view is drawn finer, out by at most a pixel',
      k >= 0 && closeShown[k] < farShown[k] && outBy <= 1,
      { far: farShown, close: closeShown, distance_m: Math.round(distance), out_by_px: Number(outBy.toFixed(2)) },
    );
    viewport.fitView();
    await settle();
    const centre: [number, number] = [rect.left + rect.width / 2, rect.top + rect.height / 2];
    const coarsePick = await viewport.pickPoint?.(centre[0], centre[1]);
    viewport.setDetail(0, false);
    await settle();
    const fullPick = await viewport.pickPoint?.(centre[0], centre[1]);
    expect('a pick lands on the same point whatever is drawn', !!coarsePick && !!fullPick && coarsePick.every((v, k) => v === fullPick[k]), { coarse: coarsePick, full: fullPick });
    viewport.setDetail(1, false);
  }

  // The pivot (decision 0044, 2): a string drawn deep under the topography shows under the cursor, as strings draw over
  // everything, but the view turns about the ground there, the nearer; a string on the ground is turned about itself.
  const deep = (await runCommand('string.create', { name: 'Deep toe', layer: 'toes', closed: false, points: [[E + 300, N - 100, 20], [E + 500, N - 100, 20]] })).record.outputs[0].id;
  const onGround = (await runCommand('string.create', { name: 'Ground line', layer: 'toes', closed: false, points: [[E + 300, N - 50, 100], [E + 500, N - 50, 100]] })).record.outputs[0].id;
  await show(await projectSummary());
  viewport.focusOn(local([E + 400, N - 75, 100]).toArray() as Vec3, 400);
  await settle();
  const deepAt = screen([E + 410, N - 100, 20]);
  const groundAt = screen([E + 410, N - 50, 100]);
  const underDeep = await viewport.pickPoint?.(deepAt[0], deepAt[1]);
  const underGround = await viewport.pickPoint?.(groundAt[0], groundAt[1]);
  expect(
    'a press over a string deep under the ground turns about the ground; over a string on the ground, about the string',
    !!underDeep && Math.abs(underDeep[2] + origin[2] - 100) < 1e-6 && !!underGround && Math.abs(underGround[1] + origin[1] - (N - 50)) < 1e-6 && Math.abs(underGround[2] + origin[2] - 100) < 1e-6,
    { deep: underDeep && Number((underDeep[2] + origin[2]).toFixed(3)), ground: underGround && [Number((underGround[1] + origin[1] - N).toFixed(3)), Number((underGround[2] + origin[2]).toFixed(3))] },
  );

  // Strings hidden behind what's in front of them, by default (Tim, 21 November: seeing strings and polygons through the
  // solids was confusing): the deep toe doesn't show through the ground, the line on the ground does, and a click over
  // the deep toe picks what's in front of it. With Over everything, the deep toe shows through.
  const beside = (at: [number, number]) => [at[0] + 14, at[1]] as [number, number];
  const showsAt = async () => {
    const picture = await snapshot();
    const apart = (a: number[], b: number[]) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    return { deep: apart(picture(deepAt), picture(beside(deepAt))) > 20, ground: apart(picture(groundAt), picture(beside(groundAt))) > 20 };
  };
  viewport.setStringsOver(false, false);
  const behind = await showsAt();
  await click(deepAt);
  const clickedDeep = selection().includes(deep);
  await key('Escape');
  viewport.setStringsOver(true, false);
  const over = await showsAt();
  viewport.setStringsOver(false, false);
  expect(
    'strings are hidden behind the ground by default, a string on the ground shows, and a click over a hidden one picks what is in front; over everything, the deep one shows',
    !behind.deep && behind.ground && !clickedDeep && over.deep && over.ground,
    { behind, clicked_the_hidden_toe: clickedDeep, over },
  );
  void onGround;

  // Blocks (decision 0097, workstream 4): four blocks touching in a square, each a rolling top over a flat floor, 234,240
  // triangles in one tile. The walls two share are left out while all four are drawn, and drawn once one is hidden; far
  // off the tops thin and the picture stays the full one's; a picked block's highlight is its full detail.
  const [bx, by, cells] = [E + 2000, N + 600, 120];
  const blockIds: string[] = [];
  for (const [i, j] of [[0, 0], [1, 0], [0, 1], [1, 1]]) {
    const [c0, r0] = [i * cells, j * cells];
    const n = cells + 1;
    const corner = (c: number, r: number, floor: boolean): Vec3 => [bx + c0 + c, by + r0 + r, floor ? 80 : 100 + 4 * Math.sin((c0 + c) / 9) * Math.cos((r0 + r) / 11) + 1.5 * Math.sin((c0 + c + r0 + r) / 5)];
    const at = (c: number, r: number, floor: boolean) => (floor ? n * n : 0) + r * n + c;
    const vertices: Vec3[] = [];
    for (const floor of [false, true]) for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) vertices.push(corner(c, r, floor));
    const faces: [number, number, number][] = [];
    for (let r = 0; r < cells; r++) {
      for (let c = 0; c < cells; c++) {
        faces.push([at(c, r, false), at(c + 1, r, false), at(c + 1, r + 1, false)], [at(c, r, false), at(c + 1, r + 1, false), at(c, r + 1, false)]);
        faces.push([at(c, r, true), at(c + 1, r + 1, true), at(c + 1, r, true)], [at(c, r, true), at(c, r + 1, true), at(c + 1, r + 1, true)]);
      }
    }
    // The border anticlockwise from above; each side's diagonal runs from its lower-numbered end's top, so two blocks
    // sharing a side have the same two triangles there.
    const ring: [number, number][] = [];
    for (let k = 0; k < cells; k++) ring.push([k, 0]);
    for (let k = 0; k < cells; k++) ring.push([cells, k]);
    for (let k = cells; k > 0; k--) ring.push([k, cells]);
    for (let k = cells; k > 0; k--) ring.push([0, k]);
    for (let k = 0; k < ring.length; k++) {
      const [p, q] = [ring[k], ring[(k + 1) % ring.length]];
      const [tp, tq, fp, fq] = [at(p[0], p[1], false), at(q[0], q[1], false), at(p[0], p[1], true), at(q[0], q[1], true)];
      const lower = p[0] + c0 + 1e4 * (p[1] + r0) < q[0] + c0 + 1e4 * (q[1] + r0);
      if (lower) faces.push([tp, fp, fq], [tp, fq, tq]);
      else faces.push([tp, fp, tq], [tq, fp, fq]);
    }
    blockIds.push((await runCommand('solid.from_mesh', { name: `Rolling block ${blockIds.length + 1}`, layer: 'rolling blocks', vertices, triangles: faces })).record.outputs[0].id);
  }
  await show(await projectSummary());
  const tileOf = () => viewport.content.children.find((g) => g.name === 'rolling blocks') as THREE.Group | undefined;
  for (let k = 0; k < 150 && !tileOf()?.userData.detail; k++) await wait(100);
  const tile = tileOf();
  const wallMeshes = (tile?.children ?? []).filter((c) => c.userData.shared);
  type BlockDetail = { full: THREE.Mesh; levels: THREE.Mesh[] }[];
  const tileDetail = tile?.userData.detail as BlockDetail | undefined;
  expect('a tile of blocks sends the walls they share apart, and gets coarser versions', wallMeshes.length > 0 && !!tileDetail && tileDetail.some((c) => c.levels.length >= 2), { shared_chunks: wallMeshes.length, versions: tileDetail?.map((c) => c.levels.length) });
  if (tile && tileDetail) {
    viewport.setDetail(0, false);
    viewport.focusOn(local([bx + cells, by + cells, 100]).toArray() as Vec3, 900);
    await settle();
    const fullDrawn = viewport.drawnTriangleCount;
    expect('with every block drawn, the walls they share are not', wallMeshes.every((m) => !m.visible));
    await show(await setObjectsVisible([blockIds[0]], false));
    await settle();
    const hiddenDrawn = viewport.drawnTriangleCount;
    expect('one block hidden, the walls they share are drawn, so its neighbours show their sides', wallMeshes.every((m) => m.visible) && hiddenDrawn > fullDrawn, { before: fullDrawn, with_one_hidden: hiddenDrawn });
    await show(await setObjectsVisible([blockIds[0]], true));
    await settle();
    expect('shown again, the shared walls go', wallMeshes.every((m) => !m.visible));
    const full = await snapshot();
    viewport.setDetail(1, false);
    await settle();
    const thinned = viewport.drawnTriangleCount;
    const coarse = await snapshot();
    const rect = canvas.getBoundingClientRect();
    let [looked, differ] = [0, 0];
    for (let i = 1; i < 60; i++) {
      for (let j = 1; j < 40; j++) {
        const at: [number, number] = [rect.left + (rect.width * i) / 60, rect.top + (rect.height * j) / 40];
        const [a, b] = [coarse(at), full(at)];
        looked++;
        if (Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) > 24) differ++;
      }
    }
    expect('the blocks far off draw fewer triangles, the picture the full one, near enough', thinned < fullDrawn / 2 && differ / looked < 0.02, { full: fullDrawn, at_1_px: thinned, points: looked, differ });
    await click(screen([bx + cells * 1.5, by + cells * 0.5, 100]));
    await settle();
    const highlights = viewport.content.children.filter((g) => g.name === 'rolling blocks selected');
    const full_chunks = tile.children.filter((c) => !c.userData.level).length;
    expect(
      "a picked block's highlight is drawn from full detail and the shared walls, not a coarser version",
      selection().includes(blockIds[1]) && highlights.length === 1 && highlights[0].children.length === full_chunks,
      { picked: selection().map((id) => blockIds.indexOf(id)), highlight_meshes: highlights.map((g) => g.children.length), full_chunks },
    );
    await key('Escape');
  }

  results.failures = failures;
  return results;
}
