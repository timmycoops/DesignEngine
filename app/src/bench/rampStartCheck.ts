// `--ramp-start`: Ramp from a start (decision 0079), driven as Tim would from the Walls row, through the panel and
// clicks in the view. A toe 150 m east then north at RL 100, under a topography falling 1 in 20 east from RL 130. A
// ramp 30 m wide at 10 % to RL 120 is placed along the toe by two clicks, its polygon where it should be to the
// millimetre; Move it places it again 50 m on, the same polygon; undo puts it back; and one straight from a start
// runs to the topography.
import * as THREE from 'three/webgpu';
import { projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { StringLayer } from '../viewport/strings';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkRampStart(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null, selection: () => string[]) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`ramp start: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('ramp start: a toe at RL 100 under a topography falling east from RL 130');
  const z = (x: number) => 130 - 0.05 * x;
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E - 300, N - 400, z(-300)], [E + 600, N - 400, z(600)], [E + 600, N + 500, z(600)], [E - 300, N + 500, z(-300)]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const toe = (await runCommand('string.create', { name: 'Toe', layer: 'toes', closed: false, points: [[E, N, 100], [E + 150, N, 100], [E + 150, N + 300, 100]] })).record.outputs[0].id;
  let summary = await projectSummary();
  await show(summary);
  await wait(1500);
  const origin = summary.origin ?? [0, 0, 0];
  const screen = (p: Vec3) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
  const pointer = (type: string, x: number, y: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true }));
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
  const press = (group: string, option: string) => {
    const found = [...document.querySelectorAll(`[aria-label="${group}"] button`)].find((b) => b.textContent?.trim() === option) as HTMLElement | undefined;
    found?.click();
    return !!found;
  };
  /** A button of the panel by how its words start, pressed; then the two clicks, and the run waited for. */
  const place = async (starts: string, start: Vec3, towards: Vec3) => {
    const go = buttons().find((b) => !!panel()?.contains(b) && b.textContent?.trim().startsWith(starts));
    if (!go || go.disabled) return { ran: false, words: go?.textContent?.trim() ?? null };
    const words = go.textContent?.trim() ?? '';
    go.click();
    await wait(300);
    await click(start);
    await click(towards);
    for (let k = 0; k < 100 && buttons().some((b) => !!panel()?.contains(b) && b.textContent?.trim() === 'Working…'); k++) await wait(50);
    await wait(600);
    summary = await projectSummary();
    await show(summary);
    await wait(400);
    return { ran: true, words };
  };
  const ramps = () => summary.objects.filter((o) => o.kind === 'string' && o.name.startsWith('Ramp RL'));
  /** A string's vertices about the origin, to the millimetre. */
  const at = (o: { vertices_local?: Vec3[] | null } | undefined) => (o?.vertices_local ?? []).map((p) => [p[0] + origin[0] - E, p[1] + origin[1] - N, p[2] + origin[2]].map((c) => Math.round(c * 1000) / 1000 + 0));
  const same = (a: number[][], b: number[][]) => a.length === b.length && a.every((p, i) => p.every((c, k) => Math.abs(c - b[i][k]) < 2e-3));

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', bubbles: true }));
  await settle();
  viewport.fitView();
  await settle();

  // From the Walls row.
  document.querySelector<HTMLButtonElement>('[role="tab"][data-stage="walls"]')?.click();
  await wait(200);
  const onRow = document.querySelector<HTMLButtonElement>('[data-slot="ribbon"] [data-tool="strings:ramp_polygon"]');
  expect('Ramp is on the Walls row', !!onRow);
  onRow?.click();
  await wait(500);
  expect('it opens the panel set to it', panelText().includes('Places a ramp from a start, as its polygon.'), panelText().slice(0, 120));
  expect('with no line picked and nothing to run to, its button is off', panelText().includes('No line picked') && (await place('Place a ramp', [E, N, 100], [E + 50, N, 100])).ran === false);

  // Along the toe: 30 m wide on the left at 10 % to RL 120, from 50 m along it and round the corner.
  layer.select(toe);
  await wait(500);
  const set = typeInto('Road width', '30') && typeInto('Ramp grade', '10') && press('The side the road lies on, looking the way it runs', 'Left') && choose('What the ramp runs to', 'rl');
  await wait(300);
  const toRl = typeInto('What the ramp runs to: RL', '120');
  await wait(300);
  const placed = await place('Place a ramp along Toe', [E + 50, N, 100], [E + 150, N + 200, 100]);
  const first = ramps()[0];
  const where = [[50, 0, 100], [150, 0, 110], [150, 100, 120], [120, 100, 120], [120, 30, 110], [50, 30, 100]];
  expect('two clicks place the ramp along the toe, its corner at one RL from edge to edge', set && toRl && placed.ran && ramps().length === 1 && same(at(first), where), { set, toRl, placed, at: at(first) });
  expect('it is a closed string named for its RLs', first?.closed === true && first?.name === 'Ramp RL 100 to 120', first?.name);
  expect('the run says what it placed', panelText().includes("a ramp 30 m wide at 10 % up along 'Toe', from RL 100 to RL 120: 200 m long"), panelText().slice(-200));
  expect('the toe stays picked, for the next one', selection().length === 1 && selection()[0] === toe, selection().length);

  // Move it: placed again 50 m further along, the same polygon.
  expect('the placed ramp is named on the panel with Move it', panelText().includes('Ramp RL 100 to 120') && buttons().some((b) => b.textContent?.trim() === 'Move it'));
  const moved = await place('Move it', [E + 100, N, 100], [E + 150, N + 200, 100]);
  const movedTo = [[100, 0, 100], [150, 0, 105], [150, 150, 120], [120, 150, 120], [120, 30, 105], [100, 30, 100]];
  expect('Move it places the same polygon from the new start', moved.ran && ramps().length === 1 && ramps()[0].id === first?.id && same(at(ramps()[0]), movedTo), { moved, at: at(ramps()[0]) });
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true }));
  await wait(800);
  summary = await projectSummary();
  await show(summary);
  await wait(400);
  expect('one undo puts it back where it was', ramps().length === 1 && same(at(ramps()[0]), where), at(ramps()[0]));

  // Straight from a start, to the topography: from RL 100 going east it meets 130 - x / 20 at 200 m, RL 120.
  press('What the ramp follows', 'A direction');
  await wait(300);
  const surface = choose('What the ramp runs to', summary.objects.find((o) => o.name === 'Topography')?.id ?? '') && typeInto('RL the ramp starts at', '100') && press('The side the road lies on, looking the way it runs', 'Right');
  await wait(300);
  const straight = await place('Place a ramp from a start', [E, N - 200, z(0)], [E + 100, N - 200, z(100)]);
  const second = ramps().find((o) => o.id !== first?.id);
  expect('straight from a start it runs to the surface', surface && straight.ran && same(at(second), [[0, -200, 100], [200, -200, 120], [200, -230, 120], [0, -230, 100]]), { surface, straight, at: at(second) });
  expect('on the ramps layer', summary.layers.some((l) => l.name === 'ramps' && l.id === second?.layer), second?.layer);

  results.failures = failures;
  return results;
}
