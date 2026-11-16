// `--project`: Project on the Strings tab (decision 0079), driven as Tim would, through the ribbon, the panel's own
// controls and clicks in the view. Three toes at RL 100 under a topography at RL 140: the side set by a click in the
// view; all three picked by a box and projected up by a height in one run, the new strings picked ready for the next
// step; those projected on to the topography with their walls kept; one undo a run.
import * as THREE from 'three/webgpu';
import { projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { StringLayer } from '../viewport/strings';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;
/** The toes run east, this long and this far apart going north (m). */
const LENGTH = 400;
const APART = 120;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkProject(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null, selection: () => string[]) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`project: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('project: three toes at RL 100 under a topography at RL 140');
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E - 300, N - 300, 140], [E + 700, N - 300, 140], [E + 700, N + 600, 140], [E - 300, N + 600, 140]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const toes: string[] = [];
  for (let k = 0; k < 3; k++) toes.push((await runCommand('string.create', { name: `Toe ${k + 1}`, layer: 'toes', closed: false, points: [[E, N + k * APART, 100], [E + LENGTH, N + k * APART, 100]] })).record.outputs[0].id);
  let summary = await projectSummary();
  await show(summary);
  await wait(1500);
  const origin = summary.origin ?? [0, 0, 0];
  const screen = (p: Vec3) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
  const pointer = (type: string, x: number, y: number, shift = false) =>
    canvas.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true, shiftKey: shift }));
  const settle = async () => {
    await wait(400);
    await viewport.nextFrame();
  };
  const buttons = () => [...document.querySelectorAll('button')];
  const panel = () => document.querySelector('[data-tool-panel="strings"]');
  const panelText = () => panel()?.textContent ?? '';
  const typeInto = (label: string, value: string) => {
    const input = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
    if (!input) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  };
  const choose = (label: string, value: string) => {
    const select = document.querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`);
    if (!select) return false;
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  /** One of a choice's options, pressed. */
  const press = (group: string, option: string) => {
    const found = [...document.querySelectorAll(`[aria-label="${group}"] button, [role="radiogroup"][aria-label="${group}"] [role="radio"]`)].find((b) => b.textContent?.trim() === option) as HTMLElement | undefined;
    found?.click();
    return !!found;
  };
  /** The option of a choice that's on. */
  const chosen = (group: string) => [...document.querySelectorAll(`[aria-label="${group}"] button, [aria-label="${group}"] [role="radio"]`)].find((b) => b.getAttribute('aria-pressed') === 'true' || b.getAttribute('aria-checked') === 'true' || b.getAttribute('data-state') === 'on')?.textContent?.trim() ?? null;
  /** The panel's one button, by how its words start, pressed and waited for. */
  const run = async (starts: string) => {
    const go = buttons().find((b) => !!panel()?.contains(b) && b.textContent?.trim().startsWith(starts));
    if (!go || go.disabled) return { ran: false, words: go?.textContent?.trim() ?? null };
    const words = go.textContent?.trim() ?? '';
    const started = performance.now();
    go.click();
    await wait(150);
    for (let k = 0; k < 100 && buttons().some((b) => !!panel()?.contains(b) && b.textContent?.trim() === 'Working…'); k++) await wait(50);
    await wait(600);
    return { ran: true, words, ms: Math.round(performance.now() - started) };
  };
  const strings = async () => (await projectSummary()).objects.filter((o) => o.kind === 'string');
  const northings = (o: { vertices_local?: Vec3[] | null }) => [...new Set((o.vertices_local ?? []).map((p) => Math.round((p[1] + origin[1] - N) * 1000) / 1000))];
  const rls = (o: { vertices_local?: Vec3[] | null }) => [...new Set((o.vertices_local ?? []).map((p) => Math.round((p[2] + origin[2]) * 1000) / 1000))];

  // From the ribbon: the Strings tab, then Project on its row.
  document.querySelector<HTMLButtonElement>('[role="tab"][data-stage="strings"]')?.click();
  await wait(200);
  const onRow = document.querySelector<HTMLButtonElement>('[data-slot="ribbon"] [data-tool="strings:project"]');
  expect('Project is on the Strings row', !!onRow);
  onRow?.click();
  await wait(500);
  expect('it opens the Strings panel set to Project', panelText().includes('Projects strings at a batter, up or down, to a target.'), panelText().slice(0, 120));
  expect('with nothing picked it says so, and its button is off', panelText().includes('No string picked') && (await run('Project')).ran === false);

  // One toe picked, and the side set by a click in the view: north of a toe running east is its left, south its right.
  key('t');
  await settle();
  viewport.fitView();
  await settle();
  layer.select(toes[0]);
  await wait(500);
  expect('a picked toe is named on the panel', panelText().includes('Toe 1'));
  const clickSide = async (at: Vec3) => {
    buttons().find((b) => b.textContent?.trim() === 'Click it')?.click();
    await wait(300);
    const [x, y] = screen(at);
    pointer('pointerdown', x, y);
    pointer('pointerup', x, y);
    await settle();
  };
  const SIDE = 'The side the new string comes out on, looking along the string';
  await clickSide([E + LENGTH / 2, N - 40, 100]);
  const south = chosen(SIDE);
  await clickSide([E + LENGTH / 2, N + 40, 100]);
  const north = chosen(SIDE);
  expect('a click south of the toe sets Right, and north of it Left', south === 'Right' && north === 'Left', { south, north });
  expect('the click leaves the toe picked', selection().length === 1 && selection()[0] === toes[0], selection().length);

  // All three picked by a box, projected up 20 m at 45° in one run.
  const corner = (p: Vec3) => screen(p);
  const [from, to] = [corner([E - 30, N - 30, 100]), corner([E + LENGTH + 30, N + 2 * APART + 30, 100])];
  pointer('pointerdown', from[0], from[1], true);
  for (let k = 1; k <= 4; k++) pointer('pointermove', from[0] + ((to[0] - from[0]) * k) / 4, from[1] + ((to[1] - from[1]) * k) / 4, true);
  pointer('pointerup', to[0], to[1], true);
  await settle();
  expect('a box picks the three toes', selection().length === 3 && toes.every((id) => selection().includes(id)), selection().length);
  expect('the panel says how many', panelText().includes('3 strings'), panelText().slice(0, 160));
  const typed = typeInto('Height above or below each string', '20') && typeInto('Batter', '45');
  await wait(300);
  const up = await run('Project 3 strings up 20 m');
  summary = await projectSummary();
  await show(summary);
  await wait(500);
  const made = (await strings()).filter((o) => o.name.endsWith(' up 20 m'));
  expect('one run makes a string for each toe', typed && up.ran && made.length === 3, { typed, up, made: made.map((o) => o.name) });
  expect(
    'each stands 20 m out on the left at RL 120',
    made.length === 3 && made.every((o, k) => northings(o).length === 1 && Math.abs(northings(o)[0] - (k * APART + 20)) < 1e-3 && rls(o).length === 1 && rls(o)[0] === 120),
    made.map((o) => [northings(o), rls(o)]),
  );
  expect('the new strings are picked, ready for the next step', selection().length === 3 && made.every((o) => selection().includes(o.id)), selection().length);
  expect('the run says what it did', panelText().includes('projected 3 strings up at 45° up 20 m, on the left'), panelText().slice(-200));

  // On from those to the topography, walls kept.
  const toTopography = choose('What the strings are projected to', summary.objects.find((o) => o.name === 'Topography')?.id ?? '');
  await wait(300);
  document.querySelector<HTMLElement>('[aria-label="Keep the wall as a surface"]')?.click();
  await wait(300);
  const on = await run('Project 3 strings up to Topography');
  summary = await projectSummary();
  await show(summary);
  await wait(500);
  const tops = summary.objects.filter((o) => o.kind === 'string' && o.name.endsWith(' to Topography'));
  const walls = summary.objects.filter((o) => o.kind === 'surface' && o.name.endsWith(' to Topography wall'));
  expect('from the new strings to a surface, with a wall each', toTopography && on.ran && tops.length === 3 && walls.length === 3, { toTopography, on, tops: tops.length, walls: walls.length });
  expect(
    'each stands 40 m out at RL 140',
    tops.length === 3 && tops.every((o) => northings(o).length === 1 && Math.abs((northings(o)[0] % APART) - 40) < 1e-3 && rls(o).length === 1 && rls(o)[0] === 140),
    tops.map((o) => [northings(o), rls(o)]),
  );
  expect('only the strings are picked, not their walls', selection().length === 3 && tops.every((o) => selection().includes(o.id)), selection().length);

  // Down past the toes' RL from where they are can't be done going up: refused, and nothing made.
  const before = (await projectSummary()).objects.length;
  choose('What the strings are projected to', 'rl');
  await wait(300);
  typeInto('What the strings are projected to: RL', '90');
  await wait(300);
  const refused = await run('Project 3 strings up to RL 90');
  expect("a run that can't be done is refused, with why, and makes nothing", refused.ran && (await projectSummary()).objects.length === before && panelText().includes("3 of 3 strings can't be projected"), { refused, said: panelText().slice(-220) });
  press('Up from the strings, or down', 'Down');
  await wait(300);
  const down = await run('Project 3 strings down to RL 90');
  summary = await projectSummary();
  await show(summary);
  await wait(500);
  const floors = summary.objects.filter((o) => o.kind === 'string' && o.name.endsWith(' to RL 90'));
  expect('going down it runs: 50 m out at RL 90', down.ran && floors.length === 3 && floors.every((o) => rls(o).length === 1 && rls(o)[0] === 90 && Math.abs((northings(o)[0] % APART) - 90) < 1e-3), { down, floors: floors.map((o) => [northings(o), rls(o)]) });

  // One undo a run.
  key('z', true);
  await wait(800);
  expect('one undo takes back a whole run', (await projectSummary()).objects.length === before, { before, after: (await projectSummary()).objects.length });

  results.failures = failures;
  return results;

  function key(name: string, ctrl = false) {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: name, ctrlKey: ctrl, bubbles: true }));
  }
}
