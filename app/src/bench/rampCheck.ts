// `--ramp`: a ramp made by hand from polylines alone (decision 0079), driven as Tim would from the Walls row: the
// road's inside edge graded with Set grade, its outside edge by Offset, the cut's crest and the fill's toe by Project,
// and one surface between the four with Surface between strings. Its area is known: three parallelograms.
import * as THREE from 'three/webgpu';
import { objectDetails, projectSummary, type ProjectSummary, runCommand, type Vec3 } from '../bridge';
import type { StringLayer } from '../viewport/strings';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;
const LENGTH = 200;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The area of a parallelogram on two sides (m²). */
function parallelogram(u: Vec3, v: Vec3): number {
  return Math.hypot(u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]);
}

export async function checkRamp(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null, selection: () => string[]) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`ramp: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log("ramp: a road's inside edge, 200 m long at RL 100");
  const edge = (await runCommand('string.create', { name: 'Inside edge', layer: 'ramp', closed: false, points: [[E, N, 100], [E + LENGTH, N, 100]] })).record.outputs[0].id;
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
  const press = (group: string, option: string) => {
    const found = [...document.querySelectorAll(`[aria-label="${group}"] button`)].find((b) => b.textContent?.trim() === option) as HTMLElement | undefined;
    found?.click();
    return !!found;
  };
  /** A tool on the Walls row, clicked. */
  const fromWalls = async (tool: string) => {
    document.querySelector<HTMLButtonElement>('[role="tab"][data-stage="walls"]')?.click();
    await wait(200);
    const onRow = document.querySelector<HTMLButtonElement>(`[data-slot="ribbon"] [data-tool="${tool}"]`);
    onRow?.click();
    await wait(500);
    return !!onRow;
  };
  /** The panel's one button, by how its words start, pressed and waited for. */
  const run = async (starts: string) => {
    const go = buttons().find((b) => !!panel()?.contains(b) && b.textContent?.trim().startsWith(starts));
    if (!go || go.disabled) return { ran: false, words: go?.textContent?.trim() ?? null };
    go.click();
    await wait(150);
    for (let k = 0; k < 100 && buttons().some((b) => !!panel()?.contains(b) && b.textContent?.trim() === 'Working…'); k++) await wait(50);
    await wait(600);
    summary = await projectSummary();
    await show(summary);
    await wait(400);
    return { ran: true, words: go.textContent?.trim() ?? '' };
  };
  const named = (name: string) => summary.objects.find((o) => o.name === name);
  /** A string's vertices about the origin, to the millimetre. */
  const at = (name: string) => (named(name)?.vertices_local ?? []).map((p) => [p[0] + origin[0] - E, p[1] + origin[1] - N, p[2] + origin[2]].map((c) => Math.round(c * 1000) / 1000));
  const same = (a: number[][], b: number[][]) => a.length === b.length && a.every((p, i) => p.every((c, k) => Math.abs(c - b[i][k]) < 1e-3));

  window.dispatchEvent(new KeyboardEvent('keydown', { key: 't', bubbles: true }));
  await settle();
  viewport.fitView();
  await settle();

  // The road's inside edge, graded at 10 % from RL 100.
  expect('Set grade, Offset, Project and Surface between are on the Walls row', (await fromWalls('strings:offset')) && (await fromWalls('strings:project')) && (await fromWalls('strings:surface_between')) && (await fromWalls('strings:set_grade')));
  layer.select(edge);
  await wait(500);
  const graded = typeInto('Grade', '10') && (await wait(300), true) && (await run('Grade Inside edge')).ran;
  expect('Set grade puts the edge at 10 %: RL 100 to 120', graded && same(at('Inside edge'), [[0, 0, 100], [LENGTH, 0, 120]]), at('Inside edge'));

  // Its outside edge, 30 m to the right at the same heights.
  await fromWalls('strings:offset');
  const offset = typeInto('Offset distance', '30') && press('The side to offset to, looking along the string', 'Right') && (await wait(300), true) && (await run('Offset Inside edge')).ran;
  const outside = 'Inside edge offset 30 m right';
  expect('Offset makes the outside edge 30 m out at the same heights', offset && same(at(outside), [[0, -30, 100], [LENGTH, -30, 120]]), at(outside));

  // The cut's crest from the inside edge, 15 m up at 45° on the left; the fill's toe from the outside edge, 10 m down on the right.
  await fromWalls('strings:project');
  layer.select(edge);
  await wait(500);
  const SIDE = 'The side the new string comes out on, looking along the string';
  const up = typeInto('Height above or below each string', '15') && typeInto('Batter', '45') && press(SIDE, 'Left') && press('Up from the strings, or down', 'Up') && (await wait(300), true) && (await run('Project Inside edge up 15 m')).ran;
  expect("Project makes the cut's crest 15 m out and 15 m up", up && same(at('Inside edge up 15 m'), [[0, 15, 115], [LENGTH, 15, 135]]), at('Inside edge up 15 m'));
  layer.select(named(outside)?.id ?? null);
  await wait(500);
  const down = typeInto('Height above or below each string', '10') && press(SIDE, 'Right') && press('Up from the strings, or down', 'Down') && (await wait(300), true) && (await run(`Project ${outside} down 10 m`)).ran;
  expect("and the fill's toe 10 m out and 10 m down", down && same(at(`${outside} down 10 m`), [[0, -40, 90], [LENGTH, -40, 110]]), at(`${outside} down 10 m`));

  // One surface between the four, picked by a box.
  await fromWalls('strings:surface_between');
  expect('with one string picked it asks for another, its button off', panelText().includes('and one more to pick') && (await run('Make a surface')).ran === false, panelText().slice(0, 200));
  viewport.fitView();
  await settle();
  const [from, to] = [screen([E - 20, N - 55, 100]), screen([E + LENGTH + 20, N + 30, 100])];
  pointer('pointerdown', from[0], from[1], true);
  for (let k = 1; k <= 4; k++) pointer('pointermove', from[0] + ((to[0] - from[0]) * k) / 4, from[1] + ((to[1] - from[1]) * k) / 4, true);
  pointer('pointerup', to[0], to[1], true);
  await settle();
  expect('a box picks the four strings', selection().length === 4, selection().length);
  typeInto('Name of the surface', 'Ramp 1');
  await wait(300);
  const made = await run('Make a surface between 4 strings');
  const ramp = named('Ramp 1');
  const details = ramp ? await objectDetails(ramp.id) : null;
  const area = details?.measures.kind === 'surface' ? details.measures.area : null;
  const along: Vec3 = [LENGTH, 0, 20];
  const want = parallelogram(along, [0, 15, 15]) + parallelogram(along, [0, 30, 0]) + parallelogram(along, [0, 10, 10]);
  expect('one surface across the four: the cut batter, the road and the fill batter', made.ran && ramp?.kind === 'surface' && area !== null && Math.abs(area - want) < 1e-6, { made, area, want });
  expect('six triangles on the strings own eight vertices', details?.measures.kind === 'surface' && details.measures.triangles === 6 && details.measures.vertices === 8, details?.measures.kind === 'surface' ? [details.measures.triangles, details.measures.vertices] : null);
  expect('the surface is picked', selection().length === 1 && selection()[0] === ramp?.id, selection().length);
  expect('the run says the order across', panelText().includes("in order across 'Inside edge up 15 m', 'Inside edge', 'Inside edge offset 30 m right', 'Inside edge offset 30 m right down 10 m'"), panelText().slice(-320));

  results.failures = failures;
  return results;
}
