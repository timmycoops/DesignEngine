// `--lod <project>`: a big project drawn with fewer triangles far off (decision 0097, workstream 5). It opens the
// project as Tim would, waits for every big surface and tile of solids to have its coarser versions, then orbits it as
// Phase 0 did, at Full and at 1 px, counting the frames and the triangles drawn. Then, at three distances, the picture
// at 1 px against Full, and a pick at the middle of the view at each; and the orbit's pivot: picks at 15 points, and a press
// timed to its gold mark (decision 0044). Made-up data only: counts and timings.
import * as THREE from 'three/webgpu';
import { benchConfig, openProject, type ProjectSummary, snapRay } from '../bridge';
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
  // Picks for the pivot at 15 points across the view: how long each takes, and how far the point picked lies in depth
  // from the surface or solid under the cursor (a string snapped instead lies elsewhere).
  const picks = async () => {
    const rect = canvas.getBoundingClientRect();
    const out: { ms: number; off_surface_m: number | null; behind_m: number | null }[] = [];
    const rayMs: number[] = [];
    for (let i = 1; i <= 5; i++) {
      for (let j = 1; j <= 3; j++) {
        const [x, y] = [rect.left + (rect.width * i) / 6, rect.top + (rect.height * j) / 4];
        const t0 = performance.now();
        const hit = await viewport.pickPoint?.(x, y);
        const ms = performance.now() - t0;
        const [nx, ny] = viewport.clientToNdc(x, y);
        const ray = viewport.rayFromNdc(nx, ny);
        const t1 = performance.now();
        const under = await snapRay(ray.origin, ray.direction);
        rayMs.push(performance.now() - t1);
        const off = hit && under ? Math.hypot(hit[0] - under.local[0], hit[1] - under.local[1], hit[2] - under.local[2]) : null;
        const eye = viewport.camera.position;
        const away = (p: number[]) => Math.hypot(p[0] - eye.x, p[1] - eye.y, p[2] - eye.z);
        const behind = hit && under ? away(hit) - away(under.local) : null;
        out.push({ ms: Number(ms.toFixed(1)), off_surface_m: off === null ? null : Number(off.toFixed(1)), behind_m: behind === null ? null : Number(behind.toFixed(1)) });
      }
    }
    const ms = out.map((p) => p.ms).sort((a, b) => a - b);
    rayMs.sort((a, b) => a - b);
    const plain: number[] = [];
    for (let k = 0; k < 9; k++) {
      const t2 = performance.now();
      await benchConfig();
      plain.push(performance.now() - t2);
    }
    plain.sort((a, b) => a - b);
    return { plain_invoke_p50_ms: Number(plain[4].toFixed(1)), ray_p50_ms: Number(rayMs[7].toFixed(1)), p50_ms: ms[Math.floor(ms.length / 2)], max_ms: ms[ms.length - 1], on_a_string_off_the_surface: out.filter((p) => (p.off_surface_m ?? 0) > 5).length, behind_what_is_under_it: out.filter((p) => (p.behind_m ?? 0) > 1).length, worst_behind_m: Math.max(...out.map((p) => p.behind_m ?? 0)), of: out.length, worst_off_m: Math.max(...out.map((p) => p.off_surface_m ?? 0)) };
  };
  const cpu = () => {
    const times: number[] = [];
    for (let k = 0; k < 10; k++) {
      const t0 = performance.now();
      viewport.draw();
      times.push(performance.now() - t0);
    }
    times.sort((a, b) => a - b);
    return Number(times[5].toFixed(1));
  };
  results.picks_while_versions_are_made = await picks();
  results.draw_cpu_ms_before_versions = cpu();
  let before = 0;
  viewport.scene.traverse(() => before++);
  results.objects_before_versions = before;
  const big = () => viewport.content.children.filter((g) => g.visible && (g.userData.triangles as number) >= THINNED_FROM);
  for (let k = 0; k < 1200 && big().some((g) => !g.userData.detail); k++) await wait(250);
  const groups = big();
  results.versions_after_drawn_s = Number(((performance.now() - drawnAt) / 1000).toFixed(1));
  expect('every big surface and tile of solids on show gets its versions', groups.length > 0 && groups.every((g) => !!g.userData.detail), { big: groups.length, with_versions: groups.filter((g) => g.userData.detail).length });
  results.held_triangles = viewport.triangleCount;
  viewport.fitView();
  await settle();
  results.picks_after = await picks();
  // A press to turn, as Tim would: from the press to the gold mark showing where it will turn, at 15 points.
  const presses = async () => {
    const rect = canvas.getBoundingClientRect();
    const mark = viewport.pivotMark as unknown as { show: (...a: unknown[]) => void };
    const show = mark.show.bind(mark);
    let shownAt = 0;
    mark.show = (...a: unknown[]) => {
      shownAt = performance.now();
      show(...a);
    };
    const times: number[] = [];
    for (let i = 1; i <= 5; i++) {
      for (let j = 1; j <= 3; j++) {
        const [x, y] = [rect.left + (rect.width * i) / 6, rect.top + (rect.height * j) / 4];
        shownAt = 0;
        const t0 = performance.now();
        canvas.dispatchEvent(new PointerEvent('pointerdown', { clientX: x, clientY: y, button: 0, buttons: 1, pointerId: 7, bubbles: true, pointerType: 'mouse' }));
        for (let k = 0; k < 400 && shownAt === 0; k++) await wait(1);
        times.push(shownAt ? shownAt - t0 : 1e9);
        canvas.dispatchEvent(new PointerEvent('pointerup', { clientX: x, clientY: y, button: 0, buttons: 0, pointerId: 7, bubbles: true, pointerType: 'mouse' }));
        await settle();
      }
    }
    mark.show = show;
    times.sort((a, b) => a - b);
    return { p50_ms: Number(times[7].toFixed(1)), max_ms: Number(times[14].toFixed(1)) };
  };
  results.press_to_pivot = await presses();
  const after = results.picks_after as { behind_what_is_under_it: number };
  const press = results.press_to_pivot as { p50_ms: number };
  expect('a press turns about what is nearest the camera under the cursor, never a string far behind it', after.behind_what_is_under_it === 0, after);
  expect('the turn knows its pivot soon after the press, drawing held while it waits', press.p50_ms < 20, press);
  let meshes = 0;
  viewport.scene.traverse(() => meshes++);
  results.draw_cpu_ms = { objects: meshes, at_1px: (viewport.setDetail(1, false), cpu()), at_full: (viewport.setDetail(0, false), cpu()) };
  viewport.setDetail(1, false);
  const detail = viewport.content.children.map((g) => g.userData.detail as { levels: unknown[] }[] | undefined).filter((d) => !!d);
  results.chunks = { with_versions: detail.reduce((n, d) => n + d!.length, 0), version_meshes: detail.reduce((n, d) => n + d!.reduce((m, c) => m + c.levels.length, 0), 0) };

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
