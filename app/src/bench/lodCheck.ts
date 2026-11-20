// `--lod <project>`: a big project drawn with fewer triangles far off (decision 0097, workstream 5). It opens the
// project as Tim would, waits for every big surface and tile of solids to have its coarser versions, then orbits it as
// Phase 0 did, at Full and at 1 px, counting the frames and the triangles drawn. Then, at three distances, the picture
// at 1 px against Full, and a pick at the middle of the view at each. Made-up data only: counts and timings.
import * as THREE from 'three/webgpu';
import { openProject, type ProjectSummary } from '../bridge';
import type { Viewport } from '../viewport/Viewport';
import { orbit } from './runChecks';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
/** What the view thins: a surface or tile of this many triangles or more (`LEVELS_FROM_TRIANGLES`). */
const THINNED_FROM = 100_000;

export async function checkLod(viewport: Viewport, path: string, log: (line: string) => void, show: Show) {
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`lod: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };
  const settle = async () => {
    for (let i = 0; i < 4; i++) await viewport.nextFrame();
  };

  const opened = performance.now();
  await show(await openProject(path));
  await settle();
  const drawnAt = performance.now();
  results.open_to_first_frame_s = Number(((drawnAt - opened) / 1000).toFixed(1));
  const big = () => viewport.content.children.filter((g) => g.visible && (g.userData.triangles as number) >= THINNED_FROM);
  for (let k = 0; k < 1200 && big().some((g) => !g.userData.detail); k++) await wait(250);
  const groups = big();
  results.versions_after_drawn_s = Number(((performance.now() - drawnAt) / 1000).toFixed(1));
  expect('every big surface and tile of solids on show gets its versions', groups.length > 0 && groups.every((g) => !!g.userData.detail), { big: groups.length, with_versions: groups.filter((g) => g.userData.detail).length });
  results.held_triangles = viewport.triangleCount;

  // Frames and triangles drawn round the orbit, at Full and at 1 px.
  const orbitAt = async (pixels: number) => {
    viewport.setDetail(pixels, false);
    const drawn: number[] = [];
    const count = () => drawn.push(viewport.drawnTriangleCount);
    viewport.beforeRender.add(count);
    const frames = await orbit(viewport, 8);
    viewport.beforeRender.delete(count);
    const mean = drawn.reduce((a, b) => a + b, 0) / Math.max(1, drawn.length);
    return { ...frames, drawn_mean: Math.round(mean), drawn_max: Math.max(...drawn) };
  };
  const full = await orbitAt(0);
  const thinned = await orbitAt(1);
  results.orbit_full = full;
  results.orbit_1px = thinned;
  expect('round the orbit at 1 px, far fewer triangles drawn and more frames', thinned.drawn_mean < full.drawn_mean / 2 && thinned.mean_fps > full.mean_fps, { fps: [Number(full.mean_fps.toFixed(1)), Number(thinned.mean_fps.toFixed(1))], drawn: [full.drawn_mean, thinned.drawn_mean] });

  // The picture and a pick at three distances: everything, a quarter of the way across, and close in.
  const snapshot = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
    viewport.draw();
    const picture = new Image();
    picture.src = canvas.toDataURL('image/png');
    await picture.decode();
    const pixels = Object.assign(document.createElement('canvas'), { width: canvas.width, height: canvas.height }).getContext('2d')!;
    pixels.drawImage(picture, 0, 0);
    return (x: number, y: number) => Array.from(pixels.getImageData(Math.round(x * canvas.width), Math.round(y * canvas.height), 1, 1).data.slice(0, 3));
  };
  const rect = canvas.getBoundingClientRect();
  const centre = viewport.bounds.getCenter(new THREE.Vector3());
  const radius = viewport.bounds.getSize(new THREE.Vector3()).length() / 2;
  const views: Record<string, unknown> = {};
  for (const [name, distance] of [['everything', 0], ['a quarter across', radius / 2], ['close in', 300]] as const) {
    viewport.setDetail(1, false);
    if (distance === 0) viewport.fitView();
    else viewport.focusOn([centre.x, centre.y, centre.z], distance);
    await settle();
    const coarse = await snapshot();
    const drawn = viewport.drawnTriangleCount;
    const coarsePick = await viewport.pickPoint?.(rect.left + rect.width / 2, rect.top + rect.height / 2);
    viewport.setDetail(0, false);
    await settle();
    const whole = await snapshot();
    const fullPick = await viewport.pickPoint?.(rect.left + rect.width / 2, rect.top + rect.height / 2);
    let [looked, differ] = [0, 0];
    for (let i = 1; i < 80; i++) {
      for (let j = 1; j < 50; j++) {
        const [a, b] = [coarse(i / 80, j / 50), whole(i / 80, j / 50)];
        looked++;
        if (Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]) > 24) differ++;
      }
    }
    const samePick = !!coarsePick && !!fullPick && coarsePick.every((v, k) => v === fullPick[k]);
    views[name] = { drawn_at_1px: drawn, drawn_at_full: viewport.drawnTriangleCount, points: looked, differ, same_pick: samePick };
    expect(`${name}: the picture at 1 px is the full picture near enough, and a pick lands where it does at Full`, differ / looked < 0.02 && samePick, views[name]);
  }
  results.views = views;
  viewport.setDetail(1, false);
  results.failures = failures;
  return results;
}
