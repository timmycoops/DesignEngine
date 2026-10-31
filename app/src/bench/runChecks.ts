// Unattended Phase 0 run (app started with --bench): loads the surfaces, then times
// Check 1 (orbit at increasing scene sizes), Check 3 (binary transfer) and Check 4 (drag with
// snapping), writes bench/out/app-results.json and exits. With --solids <zip or folder> it times that
// import through the app instead, panels showing, and writes bench/out/app-solids-results.json.
import * as THREE from 'three/webgpu';
import {
  type BenchConfig,
  buildSnapIndex,
  defaultTransport,
  exitApp,
  prepareSynthetic,
  type ProjectSummary,
  runCommand,
  type SnapHit,
  snapRay,
  stageObject,
  type Transport,
  undo,
  writeResults,
} from '../bridge';
import { loadStaged } from '../viewport/load';
import { PolylineTool } from '../viewport/PolylineTool';
import type { SectionView } from '../viewport/section';
import type { Viewport } from '../viewport/Viewport';

interface FrameSummary {
  frames: number;
  mean_fps: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  max_ms: number;
  frames_over_33ms: number;
}

const WARM_UP_FRAMES = 30;

/** Shows a summary as the app does: panels, strings, geometry and the snap index. */
type Show = (summary: ProjectSummary) => Promise<void> | undefined;

export async function runChecks(viewport: Viewport, config: BenchConfig, log: (line: string) => void, show: Show, section: () => SectionView | null): Promise<void> {
  const tool = new PolylineTool(viewport);
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {
    generated: new Date().toISOString(),
    backend: viewport.backend,
    adapter: viewport.adapter,
    canvas: { width: canvas.width, height: canvas.height, device_pixel_ratio: window.devicePixelRatio },
    user_agent: navigator.userAgent,
    default_transport: defaultTransport,
  };

  try {
    if (config.solids) {
      results.solids = await checkSolids(viewport, config, config.solids, log, show, section);
    } else {
      results.load = await loadSurfaces(viewport, config.surfaces, log);
      viewport.fitView();
      results.check1 = await checkViewport(viewport, config, log);
      results.check3 = await checkTransfer(viewport, config.transfer_cells, log);
      results.check4 = await checkDrag(viewport, tool, config.drag_updates, log);
    }
  } catch (e) {
    results.error = String(e);
    log(`error: ${String(e)}`);
  }

  const path = await writeResults(JSON.stringify(results, null, 2));
  log(`results written to ${path}`);
  await exitApp();
}

/** A zip or folder of solids by the app's own path: import, draw, snap index, orbit, snap, undo. */
async function checkSolids(viewport: Viewport, config: BenchConfig, path: string, log: (line: string) => void, show: Show, section: () => SectionView | null) {
  const orbitSeconds = config.orbit_seconds;
  for (const surface of config.surfaces) await show((await runCommand('surface.import', { path: surface, layer: 'surfaces' })).summary);
  for (const strings of config.strings) await show((await runCommand('string.import', { path: strings })).summary);
  log(`solids: importing ${path}`);
  const start = performance.now();
  const reply = await runCommand('solid.import_many', { paths: [path], layer: 'solids', repair: true });
  const imported = performance.now();
  await show(reply.summary);
  const ready = performance.now();
  await viewport.nextFrame();
  const painted = performance.now();
  // Only the rows on screen are in the page, and the far end of the list is a scroll away.
  const rowsInPage = document.querySelectorAll('[data-slot="table-row"]').length;
  const list = document.querySelector('table.table-fixed')?.parentElement;
  list?.scrollTo(0, list.scrollHeight);
  await viewport.nextFrame();
  await viewport.nextFrame();
  const lastRows = list?.querySelectorAll('[data-slot="table-row"]');
  const scrolledTo = lastRows?.[lastRows.length - 1]?.textContent;
  const triangles = viewport.triangleCount;
  for (let i = 0; i < WARM_UP_FRAMES; i++) await viewport.nextFrame();
  log(`solids: orbiting ${triangles.toLocaleString()} triangles`);
  const orbited = await orbit(viewport, orbitSeconds);
  viewport.fitView();
  const snaps: number[] = [];
  for (let i = 0; i < 200; i++) {
    const ray = viewport.rayFromNdc(-0.6 + (i % 20) * 0.06, -0.3 + Math.floor(i / 20) * 0.06);
    const t = performance.now();
    await snapRay(ray.origin, ray.direction);
    snaps.push(performance.now() - t);
  }
  snaps.sort((a, b) => a - b);
  const pick = await checkPickTime(viewport);
  const camera = await checkCamera(viewport);
  const snapSwitch = await checkSnapSwitch(viewport);
  const stringPick = await checkStringPick(viewport, show);
  const sectionCheck = await checkSection(viewport, section(), reply.summary);
  const undoStart = performance.now();
  await show(await undo());
  return {
    solids: reply.summary.objects.length,
    triangles,
    engine_ms: reply.record.duration_ms,
    import_to_summary_ms: imported - start,
    draw_and_snap_index_ms: ready - imported,
    total_to_ready_ms: ready - start,
    total_to_painted_ms: painted - start,
    table_rows_in_page: rowsInPage,
    last_solid_in_last_row: scrolledTo?.includes(reply.summary.objects[reply.summary.objects.length - 1].name) ?? false,
    orbit: orbited,
    pick,
    camera,
    snap_switch: snapSwitch,
    string_pick: stringPick,
    section: sectionCheck,
    snap_p50_ms: snaps[100],
    snap_p95_ms: snaps[190],
    undo_ms: performance.now() - undoStart,
    messages: reply.record.messages.slice(0, 2),
  };
}

/** What a press or a wheel tick costs: the time to pick, across the view, with everything loaded. */
async function checkPickTime(viewport: Viewport) {
  const box = viewport.renderer.domElement.getBoundingClientRect();
  const times: number[] = [];
  for (let k = 0; k < 200; k++) {
    const [x, y] = [box.left + box.width * (0.1 + (k % 20) * 0.04), box.top + box.height * (0.1 + Math.floor(k / 20) * 0.08)];
    const start = performance.now();
    await viewport.pickPoint?.(x, y);
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  return { p50_ms: times[100], p95_ms: times[190], max_ms: times[199] };
}

/**
 * Drags and wheels the real canvas with a point of the scene under the cursor: that point must stay
 * under the cursor (px), while the view turns and closes in on it (m).
 */
async function checkCamera(viewport: Viewport) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 4; i++) await viewport.nextFrame();
  };
  let found: { x: number; y: number; point: THREE.Vector3 } | null = null;
  for (let k = 0; k < 40 && !found; k++) {
    const [x, y] = [box.left + box.width * (0.25 + (k % 8) * 0.07), box.top + box.height * (0.3 + Math.floor(k / 8) * 0.1)];
    const hit = await viewport.pickPoint?.(x, y);
    if (hit) {
      // Start exactly on the point: a pick can land on a string a few pixels from the cursor.
      const point = new THREE.Vector3(...hit);
      const [px, py] = viewport.projectToClient(point);
      found = { x: px, y: py, point };
    }
  }
  if (!found) return { error: 'nothing under the cursor' };
  const { x, y, point } = found;
  const drift = () => {
    const [sx, sy] = viewport.projectToClient(point);
    return Math.hypot(sx - x, sy - y);
  };
  const send = (type: string, dx: number, dy: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x + dx, clientY: y + dy, pointerId: 1, pointerType: 'mouse' }));

  const startDistance = viewport.camera.position.distanceTo(point);
  const startPosition = viewport.camera.position.clone();
  send('pointerdown', 0, 0);
  await settle();
  for (let k = 1; k <= 12; k++) {
    send('pointermove', 10 * k, 5 * k);
    await viewport.nextFrame();
  }
  const pivotMark = viewport.pivotMark.position?.distanceTo(point);
  send('pointerup', 120, 60);
  await settle();
  const turned = {
    drift_px: drift(),
    camera_moved_m: viewport.camera.position.distanceTo(startPosition),
    pivot_mark_off_point_m: pivotMark ?? null,
    pivot_mark_gone_after: viewport.pivotMark.position === null,
  };

  const turnedDistance = viewport.camera.position.distanceTo(point);
  for (let k = 0; k < 12; k++) {
    canvas.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: x, clientY: y, deltaY: -120 }));
    await viewport.nextFrame();
  }
  await settle();
  return { start_distance_m: startDistance, turned, zoom: { drift_px: drift(), from_m: turnedDistance, to_m: viewport.camera.position.distanceTo(point) } };
}

/** Draws through the real toolbar, canvas and key: a click snaps, S, then a click lands on the plane. */
async function checkSnapSwitch(viewport: Viewport) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  const button = (label: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(label));
  const screen = () => document.body.innerText;
  const key = (name: string) => window.dispatchEvent(new KeyboardEvent('keydown', { key: name }));
  const click = async (x: number, y: number) => {
    for (const type of ['pointerdown', 'pointerup']) {
      canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
    }
    await settle();
  };
  const [x, y] = [box.left + box.width * 0.5, box.top + box.height * 0.5];
  viewport.fitView();
  await settle();
  button('Draw string')?.click();
  await settle();
  const hover = async (hx: number, hy: number) => {
    canvas.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, buttons: 0, clientX: hx, clientY: hy, pointerId: 1, pointerType: 'mouse' }));
    await settle();
    return document.querySelector<HTMLElement>('.snap-label')?.textContent ?? '';
  };
  const overSurface = await hover(x, y);
  await click(x, y);
  const first = screen().includes('1 vertices') && !screen().includes('1 vertices (on the working plane');
  key('s');
  await settle();
  const switchedOff = screen().includes('Snap off');
  const overPlane = await hover(x + 60, y + 30);
  await click(x + 60, y + 30);
  const second = screen().includes('2 vertices (on the working plane at RL');
  key('Escape');
  key('s');
  await settle();
  const switchedOn = screen().includes('Snap on');
  button('Select')?.click();
  await settle();
  return {
    first_click_snapped: first,
    s_turned_it_off: switchedOff,
    second_click_on_the_plane: second,
    s_turned_it_on: switchedOn,
    label_over_a_solid: overSurface.replace(/[-\d.]+$/, '<RL>'),
    label_snap_off: overPlane.replace(/[-\d.]+$/, '<RL>'),
  };
}

/**
 * A real string held 50 m above the surface, so a pick that only sees surfaces lands elsewhere:
 * turning from its vertex or its segment must pivot on the string, and hovering must name it.
 */
async function checkStringPick(viewport: Viewport, show: Show) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  viewport.fitView();
  await settle();
  // The solids have gaps: the first row of the view with three surface points along it.
  let hits: SnapHit[] = [];
  for (let down = 0.25; down < 0.8 && hits.length < 3; down += 0.05) {
    hits = [];
    for (let across = 0.2; across < 0.85 && hits.length < 3; across += 0.1) {
      const ray = viewport.rayFromNdc(...viewport.clientToNdc(box.left + box.width * across, box.top + box.height * down));
      const hit = await snapRay(ray.origin, ray.direction);
      if (hit) hits.push(hit);
    }
  }
  if (hits.length < 3) return { error: 'the string needs three points on a surface' };
  const reply = await runCommand('string.create', { name: 'bench string', layer: 'design', points: hits.map((h) => [h.world[0], h.world[1], h.world[2] + 50]) });
  await show(reply.summary);
  await settle();

  const points = hits.map((h) => new THREE.Vector3(h.local[0], h.local[1], h.local[2] + 50));
  const middle = points[0].clone().lerp(points[1], 0.5);
  const send = (type: string, x: number, y: number, buttons: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  /** Close in on `p`, so the string stands clear of anything dense behind it. */
  const closeIn = async (p: THREE.Vector3) => {
    viewport.focusOn([p.x, p.y, p.z], 300);
    await settle();
  };
  /** How far the rotation point lands from `p` when a drag starts on it. */
  const turnFrom = async (p: THREE.Vector3) => {
    await closeIn(p);
    const [x, y] = viewport.projectToClient(p);
    send('pointerdown', x, y, 1);
    await settle();
    send('pointermove', x + 12, y + 6, 1);
    await settle();
    const off = viewport.pivotMark.position?.distanceTo(p) ?? null;
    send('pointerup', x + 12, y + 6, 0);
    await settle();
    return off;
  };
  const label = async (p: THREE.Vector3) => {
    await closeIn(p);
    const [x, y] = viewport.projectToClient(p);
    send('pointermove', x, y, 0);
    await settle();
    return document.querySelector<HTMLElement>('.snap-label')?.textContent?.replace(/[-\d.]+$/, '<RL>') ?? '';
  };
  const fromVertex = await turnFrom(points[1]);
  const fromSegment = await turnFrom(middle);
  const button = (name: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(name));
  button('Draw string')?.click();
  await settle();
  const overVertex = await label(points[1]);
  const overSegment = await label(middle);
  button('Select')?.click();
  await settle();
  await show(await undo());
  return { turn_from_vertex_off_m: fromVertex, turn_from_segment_off_m: fromSegment, label_over_vertex: overVertex, label_over_segment: overSegment };
}

/** Picks a section through the real toolbar and canvas, then steps it: the gate's 50 ms a step. */
async function checkSection(viewport: Viewport, section: SectionView | null, summary: ProjectSummary) {
  if (!section) return { error: 'no section view' };
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  const button = (name: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(name));
  const click = async (x: number, y: number) => {
    for (const type of ['pointerdown', 'pointerup']) {
      canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
    }
    await settle();
  };
  viewport.fitView();
  await settle();
  button('Section')?.click();
  await settle();
  await click(box.left + box.width * 0.3, box.top + box.height * 0.5);
  await click(box.left + box.width * 0.7, box.top + box.height * 0.5);
  await settle();
  await settle();
  const opened = section.open && viewport.sectionView;
  const wide = picture(viewport);
  const [times, engine]: number[][] = [[], []];
  for (let k = 0; k < 40; k++) {
    await section.step(k < 20 ? 25 : -25);
    times.push(section.lastMs);
    engine.push(section.lastEngineMs);
  }
  times.sort((a, b) => a - b);
  engine.sort((a, b) => a - b);
  const frames = await orbitless(viewport);

  // The overview: a drag across it moves the section in whole steps.
  const map = section.overviewCanvas;
  const shown = !map.hidden;
  const atStart = section.offset;
  const mapBox = map.getBoundingClientRect();
  const drag = (type: string, x: number, y: number) =>
    map.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  const [dragX, dragY] = [mapBox.left + mapBox.width * 0.5, mapBox.top + mapBox.height * 0.2];
  drag('pointerdown', dragX, dragY);
  await settle();
  const dragged = section.offset - atStart;
  const whole = dragged !== 0 && dragged % section.stepSize === 0;
  // Moving the wrong way, or the map sliding, would carry it on from here.
  drag('pointermove', dragX, dragY);
  await settle();
  const stayed = section.offset - atStart === dragged;
  drag('pointerup', dragX, dragY);
  await settle();

  // A section as a planner would pick one: 1.5 km across the first solid cut, not the whole view.
  const o = summary.origin ?? [0, 0, 0];
  const across = section.firstSolidPoint();
  let close = '';
  if (across) {
    const [x, y] = [across.x + o[0], across.y + o[1]];
    await section.show([x - 750, y - 200], [x + 750, y + 200], summary);
    await settle();
    close = picture(viewport);
    await section.step(125);
    await settle();
  }
  // Step lines stand apart at 200 m on this map, and a long move walks the section off it.
  section.stepSize = 200;
  const overview = map.toDataURL('image/png');
  await section.step(2_000);
  await settle();
  const walked = map.toDataURL('image/png');
  section.stepSize = 25;
  button('Section')?.click();
  await settle();
  return { opened, closed: !section.open && !viewport.sectionView, step_p50_ms: times[20], step_max_ms: times[39], engine_p50_ms: engine[20], fps: frames, overview_shown: shown, overview_drag_m: dragged, overview_whole_steps: whole, overview_stayed: stayed, wide, close, overview, walked };
}

/** What the viewport shows now, as a PNG data URL: drawn again and copied before the frame is gone. */
function picture(viewport: Viewport): string {
  const source = viewport.renderer.domElement;
  const copy = Object.assign(document.createElement('canvas'), { width: source.width, height: source.height });
  viewport.renderer.render(viewport.scene, viewport.camera);
  copy.getContext('2d')?.drawImage(source, 0, 0);
  return copy.toDataURL('image/png');
}

/** Frames per second over two seconds, the view left alone. */
async function orbitless(viewport: Viewport): Promise<number> {
  const start = performance.now();
  let frames = 0;
  while (performance.now() - start < 2000) {
    await viewport.nextFrame();
    frames++;
  }
  return (frames * 1000) / (performance.now() - start);
}

async function loadSurfaces(viewport: Viewport, paths: string[], log: (line: string) => void) {
  const loads = [];
  for (const path of paths) {
    log(`loading ${path}`);
    // The same path as the app: an engine import command, then the geometry fetched to draw.
    const start = performance.now();
    const reply = await runCommand('surface.import', { path, layer: 'bench' });
    const staged = await stageObject(reply.record.outputs[0].id);
    const opened = performance.now();
    const { timings } = await loadStaged(viewport, staged);
    loads.push({
      name: staged.name,
      kind: staged.kind,
      triangles: staged.triangles,
      bytes: staged.bytes,
      import_ms: reply.record.duration_ms,
      encode_ms: staged.timings.encode,
      import_and_encode_ms: opened - start,
      ...timings,
      total_to_frame_submitted_ms: opened - start + timings.to_frame_submitted_ms,
      import_log: reply.record.messages,
    });
  }
  return loads;
}

async function checkViewport(viewport: Viewport, config: BenchConfig, log: (line: string) => void) {
  const base = viewport.baseTriangleCount;
  const steps = [1, ...config.scale_targets.filter((t) => t > base).map((t) => Math.ceil(t / base))];
  const rows = [];
  for (const copies of steps) {
    const triangles = viewport.setCopies(copies);
    viewport.fitView();
    for (let i = 0; i < WARM_UP_FRAMES; i++) await viewport.nextFrame();
    log(`check 1: orbiting ${triangles.toLocaleString()} triangles (${copies} copies)`);
    rows.push({ copies, triangles, ...(await orbit(viewport, config.orbit_seconds)) });
  }
  viewport.setCopies(1);
  viewport.fitView();
  return rows;
}

/** One full turn around the scene while zooming in and out twice; returns frame-interval stats. */
async function orbit(viewport: Viewport, seconds: number): Promise<FrameSummary> {
  const box = viewport.bounds;
  const centre = box.getCenter(new THREE.Vector3());
  const radius = box.getSize(new THREE.Vector3()).length() / 2;
  const elevation = THREE.MathUtils.degToRad(35);
  const intervals: number[] = [];
  viewport.controls.enabled = false;
  await new Promise<void>((resolve) => {
    const start = performance.now();
    let last = -1;
    const hook = (now: number) => {
      const t = (now - start) / 1000;
      if (t > seconds) {
        viewport.beforeRender.delete(hook);
        resolve();
        return;
      }
      if (last >= 0) intervals.push(now - last);
      last = now;
      const angle = (t / seconds) * Math.PI * 2;
      const r = radius * 1.9 * (1 - 0.45 * Math.sin((t / seconds) * Math.PI * 4));
      viewport.camera.position.set(
        centre.x + r * Math.cos(angle) * Math.cos(elevation),
        centre.y + r * Math.sin(angle) * Math.cos(elevation),
        centre.z + r * Math.sin(elevation),
      );
      viewport.controls.target.copy(centre);
    };
    viewport.beforeRender.add(hook);
  });
  viewport.controls.enabled = true;
  return summarise(intervals);
}

/**
 * Each size in both layouts (general triangulation; grid elevations) over both transports
 * (WebView2 shared buffer; raw bytes over IPC). The mesh is generated and encoded in Rust
 * first, so only the hand-over, decode and first frame are timed.
 */
async function checkTransfer(viewport: Viewport, cellSizes: number[], log: (line: string) => void) {
  const transports: Transport[] = defaultTransport === 'shared' ? ['shared', 'ipc'] : ['ipc'];
  const rows = [];
  for (const cells of cellSizes) {
    for (const kind of ['triangulation', 'grid'] as const) {
      for (const transport of transports) {
        const staged = await prepareSynthetic(cells, kind);
        log(
          `check 3: ${staged.triangles.toLocaleString()} triangles as ${kind}, ` +
            `${(staged.bytes / 1e6).toFixed(0)} MB via ${transport}`,
        );
        const { group, timings } = await loadStaged(viewport, staged, { transport });
        rows.push({ cells, kind, triangles: staged.triangles, bytes: staged.bytes, encode_ms: staged.timings.encode, ...timings });
        viewport.removeMesh(group);
        await viewport.nextFrame();
      }
    }
  }
  return rows;
}

async function checkDrag(viewport: Viewport, tool: PolylineTool, updates: number, log: (line: string) => void) {
  log('check 4: building snap index');
  const index = await buildSnapIndex();
  viewport.fitView();
  await viewport.nextFrame();

  tool.clear();
  for (let i = 0; i < 6; i++) {
    const ray = viewport.rayFromNdc(-0.5 + i * 0.2, -0.1);
    const hit = await snapRay(ray.origin, ray.direction);
    if (hit) tool.add(hit.local);
  }
  if (tool.points.length < 3) throw new Error('check 4: could not place a polyline on the surface');

  log(`check 4: dragging a vertex through ${updates} snapped updates (full scene)`);
  const full = await drag(viewport, tool, updates);
  // Same drag with only the light design surface drawn, to separate render cost from snapping
  // (snapping still searches every loaded surface).
  viewport.showOnly((name) => name.startsWith('design'));
  log(`check 4: dragging a vertex through ${updates} snapped updates (design surface only drawn)`);
  const light = await drag(viewport, tool, updates);
  viewport.showOnly();
  return { snap_index: index, polyline_vertices: tool.points.length, full_scene: full, light_scene: light };
}

async function drag(viewport: Viewport, tool: PolylineTool, updates: number) {
  const vertex = 2;
  const [cx, cy] = [-0.5 + vertex * 0.2, -0.1];
  const snapMs: number[] = [];
  const submitMs: number[] = [];
  const gpuMs: number[] = [];
  let misses = 0;
  for (let i = 0; i < updates; i++) {
    const a = (i / updates) * Math.PI * 4;
    const ray = viewport.rayFromNdc(cx + 0.2 * Math.cos(a), cy + 0.2 * Math.sin(a));
    const start = performance.now();
    const hit = await snapRay(ray.origin, ray.direction);
    const snapped = performance.now();
    if (!hit) {
      misses++;
      continue;
    }
    tool.move(vertex, hit.local);
    const frame = await viewport.nextFrame();
    snapMs.push(snapped - start);
    submitMs.push(frame.submitted - start);
    gpuMs.push(frame.gpuDone - start);
  }
  return {
    drawn_triangles: viewport.triangleCount,
    updates: snapMs.length,
    misses,
    snap_round_trip: percentiles(snapMs),
    update_to_frame_submitted: percentiles(submitMs),
    update_to_gpu_done: percentiles(gpuMs),
  };
}

function summarise(intervals: number[]): FrameSummary {
  const p = percentiles(intervals);
  const mean = intervals.reduce((a, b) => a + b, 0) / Math.max(intervals.length, 1);
  return {
    frames: intervals.length,
    mean_fps: 1000 / mean,
    p50_ms: p.p50,
    p95_ms: p.p95,
    p99_ms: p.p99,
    max_ms: p.max,
    frames_over_33ms: intervals.filter((v) => v > 33.4).length,
  };
}

function percentiles(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] : NaN);
  return { p50: at(0.5), p95: at(0.95), p99: at(0.99), max: sorted.length ? sorted[sorted.length - 1] : NaN };
}
