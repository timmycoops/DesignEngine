// `--section-guide`: the dashed line a two-point section draws from its first point to the cursor, driven as Tim would,
// through the toolbar's Section button and pointer and key events on the canvas, in plan over a made-up topography that
// climbs east so it colours by RL. Before the first click there's no line; after it the line runs from that point to
// the point under the cursor and follows it; Esc clears it; the second click opens the section and clears it. With
// `--pictures <folder>` it saves the view with the line showing, as `section-guide.png`.
import type * as THREE from 'three/webgpu';
import { projectSummary, type ProjectSummary, runCommand, writePicture } from '../bridge';
import type { SectionView } from '../viewport/section';
import type { StringLayer } from '../viewport/strings';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const E = 650_000;
const N = 7_450_000;
/** How near the line's far end must come to the cursor on screen (px). */
const CURSOR_PX = 1.5;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkSectionGuide(
  viewport: Viewport,
  log: (line: string) => void,
  show: Show,
  section: () => SectionView | null,
  stringLayer: () => StringLayer | null,
  pictures: boolean,
) {
  const strings = stringLayer();
  if (!strings) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`section guide: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };

  log('section guide: a topography 1 km by 600 m, RL 50 in the west to RL 150 in the east');
  await runCommand('surface.from_mesh', {
    name: 'Topography',
    layer: 'surfaces',
    vertices: [[E, N, 50], [E + 1000, N, 150], [E + 1000, N + 600, 150], [E, N + 600, 50]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  const summary = await projectSummary();
  await show(summary);
  await wait(1500);
  viewport.setPlanView(true);
  viewport.fitView();

  const pointer = (type: string, [x, y]: [number, number]) =>
    canvas.dispatchEvent(
      new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerdown' ? 1 : 0, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true }),
    );
  const settle = async () => {
    await wait(300);
    for (let i = 0; i < 4; i++) await viewport.nextFrame();
  };
  const click = async (at: [number, number]) => {
    pointer('pointerdown', at);
    pointer('pointerup', at);
    await settle();
  };
  const move = async (at: [number, number]) => {
    pointer('pointermove', at);
    await settle();
  };
  const key = async (name: string) => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: name, cancelable: true, bubbles: true }));
    await settle();
  };
  const box = canvas.getBoundingClientRect();
  const at = (fx: number, fy: number): [number, number] => [box.left + box.width * fx, box.top + box.height * fy];
  const [first, cursor, later, second] = [at(0.3, 0.6), at(0.65, 0.35), at(0.55, 0.7), at(0.7, 0.4)];
  /** How far the line's far end lies from a cursor position on screen, and its near end from the first click (px). */
  const offScreen = (target: [number, number], start: [number, number]) => {
    const ends = strings.sectionGuideEnds;
    if (!ends) return null;
    const [a, b] = ends.map((p: THREE.Vector3) => viewport.projectToClient(p));
    return { start_px: Math.hypot(a[0] - start[0], a[1] - start[1]), cursor_px: Math.hypot(b[0] - target[0], b[1] - target[1]) };
  };

  [...document.querySelectorAll('button')].find((b) => b.textContent?.includes('Section'))?.click();
  await settle();
  expect('the Section button puts the view in section mode', strings.mode === 'section', strings.mode);
  await move(cursor);
  expect('no line before the first point', strings.sectionGuideEnds === null);

  await click(first);
  await move(cursor);
  const aimed = offScreen(cursor, first);
  expect('after the first point a line runs from it to the cursor', !!aimed && aimed.start_px < CURSOR_PX && aimed.cursor_px < CURSOR_PX, aimed);
  expect('the line is dashed', strings.sectionGuideDashed);
  if (pictures) {
    const copy = Object.assign(document.createElement('canvas'), { width: canvas.width, height: canvas.height });
    viewport.renderer.render(viewport.scene, viewport.camera);
    copy.getContext('2d')?.drawImage(canvas, 0, 0);
    const path = await writePicture('section-guide', copy.toDataURL('image/png'));
    log(`section guide: picture ${path}`);
    results.picture = path;
  }
  await move(later);
  const followed = offScreen(later, first);
  expect('the line follows the cursor', !!followed && followed.start_px < CURSOR_PX && followed.cursor_px < CURSOR_PX, followed);

  await key('Escape');
  expect('Esc clears the line', strings.sectionGuideEnds === null);
  await move(cursor);
  expect('after Esc no line until a first point again', strings.sectionGuideEnds === null);

  await click(first);
  await move(second);
  expect('a first point again draws the line again', strings.sectionGuideEnds !== null);
  await click(second);
  for (let i = 0; i < 200 && !section()?.open; i++) await viewport.nextFrame();
  await settle();
  expect('the second point opens the section', section()?.open === true);
  expect('the second point clears the line', strings.sectionGuideEnds === null);
  [...document.querySelectorAll('button')].find((b) => b.textContent?.includes('Section'))?.click();
  await settle();
  expect('the section closes', section()?.open === false);

  results.failures = failures;
  return results;
}
