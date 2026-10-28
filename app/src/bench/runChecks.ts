// Unattended Phase 0 run (app started with --bench): loads the surfaces, then times
// Check 1 (orbit at increasing scene sizes), Check 3 (binary transfer) and Check 4 (drag with
// snapping), writes bench/out/app-results.json and exits.
import * as THREE from 'three/webgpu';
import {
  type BenchConfig,
  buildSnapIndex,
  decodeMesh,
  exitApp,
  fetchStaged,
  openSurface,
  prepareSynthetic,
  snapRay,
  writeResults,
} from '../bridge';
import type { PolylineTool } from '../viewport/PolylineTool';
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

export async function runChecks(
  viewport: Viewport,
  tool: PolylineTool,
  config: BenchConfig,
  log: (line: string) => void,
): Promise<void> {
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {
    generated: new Date().toISOString(),
    backend: viewport.backend,
    adapter: viewport.adapter,
    canvas: { width: canvas.width, height: canvas.height, device_pixel_ratio: window.devicePixelRatio },
    user_agent: navigator.userAgent,
  };

  try {
    results.load = await loadSurfaces(viewport, config.surfaces, log);
    viewport.fitView();
    results.check1 = await checkViewport(viewport, config, log);
    results.check3 = await checkTransfer(viewport, config.transfer_cells, log);
    results.check4 = await checkDrag(viewport, tool, config.drag_updates, log);
  } catch (e) {
    results.error = String(e);
    log(`error: ${String(e)}`);
  }

  const path = await writeResults(JSON.stringify(results, null, 2));
  log(`results written to ${path}`);
  await exitApp();
}

async function loadSurfaces(viewport: Viewport, paths: string[], log: (line: string) => void) {
  const loads = [];
  for (const path of paths) {
    log(`loading ${path}`);
    const start = performance.now();
    const decoded = await openSurface(path);
    const invoked = performance.now();
    viewport.addMesh(decoded);
    await viewport.nextFrame();
    loads.push({
      name: decoded.header.name,
      triangles: decoded.header.triangles,
      vertices: decoded.header.vertices,
      bytes: decoded.bytes,
      rust: decoded.header.timings,
      invoke_ms: invoked - start,
      first_frame_ms: performance.now() - start,
      import_log: decoded.header.log,
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

async function checkTransfer(viewport: Viewport, cellSizes: number[], log: (line: string) => void) {
  const rows = [];
  for (const cells of cellSizes) {
    const staged = await prepareSynthetic(cells);
    log(`check 3: transferring ${staged.triangles.toLocaleString()} triangles (${(staged.bytes / 1e6).toFixed(0)} MB)`);
    const start = performance.now();
    const buffer = await fetchStaged(staged.handle);
    const received = performance.now();
    const mesh = viewport.addMesh(decodeMesh(buffer));
    const built = performance.now();
    const frame = await viewport.nextFrame();
    rows.push({
      cells,
      triangles: staged.triangles,
      bytes: staged.bytes,
      prepare_ms: staged.prepare_ms,
      ipc_ms: received - start,
      decode_and_geometry_ms: built - received,
      upload_and_submit_ms: frame.submitted - built,
      request_to_frame_submitted_ms: frame.submitted - start,
      request_to_gpu_done_ms: frame.gpuDone - start,
    });
    viewport.removeMesh(mesh);
    await viewport.nextFrame();
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
