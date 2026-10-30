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
  snapRay,
  stageObject,
  type Transport,
  undo,
  writeResults,
} from '../bridge';
import { loadStaged } from '../viewport/load';
import { PolylineTool } from '../viewport/PolylineTool';
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

export async function runChecks(viewport: Viewport, config: BenchConfig, log: (line: string) => void, show: Show): Promise<void> {
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
      results.solids = await checkSolids(viewport, config.solids, config.orbit_seconds, log, show);
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
async function checkSolids(viewport: Viewport, path: string, orbitSeconds: number, log: (line: string) => void, show: Show) {
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
    snap_p50_ms: snaps[100],
    snap_p95_ms: snaps[190],
    undo_ms: performance.now() - undoStart,
    messages: reply.record.messages.slice(0, 2),
  };
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
