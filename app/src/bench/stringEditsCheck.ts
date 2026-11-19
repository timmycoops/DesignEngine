// `--string-edits`: the String tools plan's workstream 1, driven as Tim would. Three graded toes picked by a click and
// two Ctrl-clicks are trimmed back to a pit limit by Trim's button, a click on the limit and a click on the side to keep;
// one Ctrl+Z puts all three back. Extend runs them on to a line by one click on it, at their own grade. Esc in the middle
// of a trim lets it go and changes nothing. Keep both splits them at the limit. Split at a click puts a vertex in where
// it's clicked, and Join puts the two back as one, ends within its tolerance sharing a vertex. Round corners (workstream
// 2) rounds a dog-leg's corner at a radius typed, every arc vertex that far from the centre; Ctrl+Z puts it back. Move
// and copy (workstream 3), from the Solids row: a toe moved by two snapped clicks and by distances typed, one rotated a
// quarter turn about a vertex clicked, and a mirrored copy of the dog-leg across the pit limit, picked once made; each
// put back by Ctrl+Z. Then
// the layers-only tree (Filtering plan, workstream 0): a row a layer saying what's on it, none a string; a right click on
// the dog-leg in the view hides it from its menu, its layer's row says so, and Show all hidden brings it back. A layer's
// filter (decision 0088, workstream 3): Filter by attributes… on the toes' right-click, `length: >170` typed keeps the
// dog-leg alone, the toes left out are passed over by a click, and Isolate, its undoing, and Clear filter put it back.
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

  // Move and copy (workstream 3), from the Solids row, or its More where the row folds.
  const movePanel = () => document.querySelector<HTMLElement>('[data-tool-panel="move"]');
  const moveButton = () => [...(movePanel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].at(-1) ?? null;
  const useMove = async (tool: string) => {
    await key('Escape');
    document.querySelector<HTMLButtonElement>('[role="tab"][data-stage="solids"]')?.click();
    await wait(200);
    const onRow = document.querySelector<HTMLButtonElement>(`[data-slot="ribbon"] button[data-tool="move:${tool}"]:not([data-slot="row-more"])`);
    if (onRow) onRow.click();
    else {
      document.querySelector<HTMLButtonElement>('[data-slot="row-more"]')?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerType: 'mouse', ctrlKey: false }));
      await settle();
      document.querySelector<HTMLElement>(`[role="menuitem"][data-tool="move:${tool}"]`)?.click();
    }
    await wait(500);
  };
  const typeInto = (label: string, value: string) => {
    const box = movePanel()?.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`) ?? null;
    if (!box) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(box, value);
    box.dispatchEvent(new Event('input', { bubbles: true }));
    box.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  const moved = (a: Vec3[], b: Vec3[], by: Vec3) => a.length === b.length && a.every((p, k) => near(b[k][0], p[0] + by[0]) && near(b[k][1], p[1] + by[1]) && near(b[k][2], p[2] + by[2]));
  const rel = (ps: Vec3[]) => ps.map((p) => [p[0] - E, p[1] - N, p[2]]);

  // Move by two clicks: Toe 1 picked, then its first vertex and the pit limit's south end, both snapped. RLs kept.
  await useMove('move');
  await click([E + 20, N + toeY[0], 102]);
  const toe1 = (await points(toes[0])) as Vec3[];
  const moveSays = moveButton()?.textContent?.trim() ?? '';
  moveButton()?.click();
  await settle();
  const askedFrom = footer().includes('Click a point on Toe 1 to move it from');
  await click([E, N + toeY[0], 100]);
  await click([E + 60, N - 50, 100]);
  await wait(800);
  const toe1Moved = (await points(toes[0])) as Vec3[];
  expect(
    'Move by two clicks, snapped to a vertex of Toe 1 and the end of the pit limit: every vertex 60 m east and 50 m south, RLs kept',
    moveSays === 'Move Toe 1' && askedFrom && moved(toe1, toe1Moved, [60, -50, 0]),
    { button: moveSays, asked: askedFrom, now: rel(toe1Moved) },
  );
  await key('z', true);
  await wait(500);
  expect('one Ctrl+Z puts Toe 1 back', moved(toe1, (await points(toes[0])) as Vec3[], [0, 0, 0]));

  // Move by distances typed: Toe 2, 100.5 m east, 200.25 m south and 3 m up.
  await click([E + 20, N + toeY[1], 102]);
  [...(movePanel()?.querySelectorAll<HTMLButtonElement>('[role="group"][aria-label="Moved by two clicks or by distances typed"] button') ?? [])].find((b) => b.textContent?.trim() === 'Distances')?.click();
  await settle();
  const typedIn = typeInto('Distance east, west negative', '100.5') && typeInto('Distance north, south negative', '-200.25') && typeInto('Distance up, down negative', '3');
  await settle();
  const toe2 = (await points(toes[1])) as Vec3[];
  moveButton()?.click();
  await wait(800);
  const toe2Moved = (await points(toes[1])) as Vec3[];
  expect('Move by distances typed moves Toe 2 by exactly 100.5 m east, 200.25 m south and 3 m up', typedIn && moved(toe2, toe2Moved, [100.5, -200.25, 3]), { typed: typedIn, now: rel(toe2Moved) });
  await key('z', true);
  await wait(500);

  // Rotate: Toe 3 (run on to 150 m by Extend) a quarter turn anticlockwise about its own first vertex, clicked: its end
  // goes from 150 m east of that vertex to 150 m north of it, at its own RL.
  await useMove('rotate');
  await click([E + 20, N + toeY[2], 102]);
  const toe3Before = ((await points(toes[2])) as Vec3[]).at(-1) ?? [0, 0, 0];
  moveButton()?.click();
  await settle();
  await click([E, N + toeY[2], 100]);
  await wait(800);
  const toe3End = ((await points(toes[2])) as Vec3[]).at(-1) ?? [0, 0, 0];
  expect(
    "Rotate 90° about Toe 3's first vertex, clicked: its end as far north of it as it was east, at its own RL",
    near(toe3End[0], E) && near(toe3End[1], N + toeY[2] + (toe3Before[0] - E)) && near(toe3End[2], toe3Before[2]) && toe3Before[0] - E > 100,
    { before: rel([toe3Before]), after: rel([toe3End]) },
  );
  await key('z', true);
  await wait(500);

  // Mirror copies: the dog-leg across the pit limit (two clicks on its ends, snapped); a copy named for it, picked,
  // as far east of the limit as the dog-leg is west of it; the dog-leg itself where it was.
  await useMove('mirror');
  await click([E - 30, N + 170, 100]);
  const copies = movePanel()?.querySelector<HTMLButtonElement>('[aria-label="Mirror copies, leaving the originals"]');
  copies?.click();
  await settle();
  const mirrorSays = moveButton()?.textContent?.trim() ?? '';
  const legBefore = (await points(dogleg)) as Vec3[];
  moveButton()?.click();
  await settle();
  await click([E + 60, N - 50, 100]);
  await click([E + 60, N + 150, 100]);
  await wait(800);
  const copy = (await projectSummary()).objects.find((o) => o.kind === 'string' && o.name === 'Dog-leg copy');
  const copyPoints = copy ? ((await points(copy.id)) as Vec3[]) : [];
  const mirrored = copyPoints.length === 3 && legBefore.every((p, k) => near(copyPoints[k][0], 2 * (E + 60) - p[0]) && near(copyPoints[k][1], p[1]) && near(copyPoints[k][2], p[2]));
  expect(
    'Mirror with Copies ticked: a Dog-leg copy across the pit limit, picked once made, the dog-leg where it was',
    mirrorSays === 'Copy and mirror Dog-leg' && mirrored && !!copy && selection().length === 1 && selection()[0] === copy.id && moved(legBefore, (await points(dogleg)) as Vec3[], [0, 0, 0]),
    { button: mirrorSays, copy: rel(copyPoints), picked: selection().length },
  );
  copies?.click();
  await key('z', true);
  await wait(500);
  expect('one Ctrl+Z takes the copy away', !(await projectSummary()).objects.some((o) => o.name === 'Dog-leg copy'));
  await key('Escape');
  void limit;
  void ramp;

  // The layers-only tree: a row a layer, what's on it by kind, no row for a string.
  const treeRows = () => [...document.querySelectorAll<HTMLElement>('[data-slot="tree"] [data-slot="tree-row"]')];
  const rowOf = (layer: string) => treeRows().find((r) => r.dataset.layer === layer) ?? null;
  const kindsOn = (layer: string) => rowOf(layer)?.querySelector('[data-slot="layer-kinds"]')?.textContent ?? '';
  const now = await projectSummary();
  const layerRowsOnly = treeRows().length === now.layers.length && !treeRows().some((r) => /Toe 1|Dog-leg|Pit limit/.test(r.textContent ?? ''));
  expect(
    'the tree has a row a layer, saying what is on it by kind, and none for a string',
    layerRowsOnly && kindsOn('toes') === '4 strings' && kindsOn('limits') === '2 strings' && kindsOn('surfaces') === '1 surface',
    { rows: treeRows().length, layers: now.layers.length, toes: kindsOn('toes'), limits: kindsOn('limits'), surfaces: kindsOn('surfaces') },
  );
  // A right click on the dog-leg: picked, its menu opens on it, and Hide hides it.
  const rightClick = async (at: Vec3) => {
    const [x, y] = screen(at);
    for (const type of ['pointerdown', 'pointerup']) {
      canvas.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, button: 2, buttons: type === 'pointerup' ? 0 : 2, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true }));
    }
    for (let k = 0; k < 30 && !document.querySelector('[data-slot="view-menu"]'); k++) await wait(100);
  };
  const menu = () => document.querySelector<HTMLElement>('[data-slot="view-menu"]');
  const item = (starts: string) => [...(menu()?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])].find((m) => m.textContent?.trim().startsWith(starts)) ?? null;
  await key('Escape');
  await rightClick([E - 30, N + 170, 100]);
  const menuSays = menu()?.textContent ?? '';
  const pickedByIt = selection().length === 1 && selection()[0] === dogleg;
  item('Hide')?.click();
  await wait(800);
  const hidden = (await projectSummary()).objects.find((o) => o.id === dogleg)?.hidden ?? false;
  await settle();
  const rowSays = rowOf('toes')?.textContent ?? '';
  expect(
    'a right click on the dog-leg picks it and opens its menu, and Hide hides it: its layer reads 3 of 4',
    pickedByIt && menuSays.startsWith('Dog-leg') && hidden && rowSays.includes('3 of 4'),
    { picked: pickedByIt, menu: menuSays.slice(0, 60), hidden, row: rowSays },
  );
  // A right click on nothing: Show all hidden brings it back.
  await rightClick([E + 250, N - 80, 100]);
  const showAll = item('Show all hidden');
  const offered = showAll?.textContent?.trim() ?? '';
  showAll?.click();
  await wait(800);
  const back2 = !((await projectSummary()).objects.find((o) => o.id === dogleg)?.hidden ?? true);
  expect('a right click on nothing offers Show all hidden (1), which brings the dog-leg back', offered === 'Show all hidden (1)' && back2, { offered, back: back2 });

  // A layer's filter, from the tree's right-click.
  const treeMenu = async (layer: string, item: string) => {
    const row = rowOf(layer);
    const box = row?.getBoundingClientRect();
    if (!row || !box) return false;
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: box.left + box.width / 2, clientY: box.top + box.height / 2, button: 2 }));
    await wait(300);
    const found = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((m) => m.textContent?.trim() === item);
    found?.click();
    await wait(500);
    return !!found;
  };
  const typeFilter = async (text: string) => {
    const input = document.querySelector<HTMLInputElement>('[data-slot="layer-filter-bar"] input');
    if (!input) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(800);
    return true;
  };
  const filteredOut = async () => (await projectSummary()).objects.filter((o) => o.filtered).map((o) => o.name).sort();
  const opened = await treeMenu('toes', 'Filter by attributes…');
  const typed = await typeFilter('length: >170');
  const out = await filteredOut();
  await settle();
  const toesSay = rowOf('toes')?.querySelector('[data-slot="layer-showing"]')?.textContent ?? '';
  const funnel = !!rowOf('toes')?.querySelector('[data-slot="layer-filter"]');
  expect(
    "Filter by attributes… on the toes, length: >170 typed: the dog-leg (180 m) alone kept, the toes run on to 151 m left out, its row 1 of 4 with a funnel",
    opened && typed && JSON.stringify(out) === JSON.stringify(['Toe 1', 'Toe 2', 'Toe 3']) && toesSay === '1 of 4' && funnel,
    { opened, typed, out, row: toesSay, funnel },
  );
  // A click where Toe 1 lies picks nothing of it: it isn't drawn or picked.
  await key('Escape');
  await click([E + 20, N + toeY[0], 102]);
  const pickedToe = selection().includes(toes[0]);
  expect('a click on a toe the filter leaves out picks nothing of it', !pickedToe, selection().length);
  // Isolate the toes: the other layers go off; again, they come back.
  const visibleLayers = async () => (await projectSummary()).layers.filter((l) => l.visible).map((l) => l.name).sort();
  const before = await visibleLayers();
  await treeMenu('toes', 'Isolate');
  const alone = await visibleLayers();
  await treeMenu('toes', 'Show what was shown');
  const after = await visibleLayers();
  expect('Isolate shows the toes alone, and Show what was shown puts the rest back', JSON.stringify(alone) === JSON.stringify(['toes']) && JSON.stringify(after) === JSON.stringify(before), { before, alone, after });
  // Clear filter: every toe back.
  await treeMenu('toes', 'Clear filter');
  const cleared = await filteredOut();
  expect('Clear filter on the right-click puts every toe back', cleared.length === 0 && !rowOf('toes')?.querySelector('[data-slot="layer-filter"]'), cleared);

  results.failures = failures;
  return results;
}
