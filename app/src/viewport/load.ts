// Fetches a staged render buffer, decodes it and adds it to the viewport, timing each step.
import { decodeMesh, fetchMesh, type StagedLevels, type StagedMesh, type Transport } from '../bridge';
import { decodeLevels } from './levels';
import type { Viewport } from './Viewport';
import type * as THREE from 'three/webgpu';

export interface LoadTimings {
  transport: Transport;
  fetch_ms: number;
  decode_ms: number;
  /** From the start of the fetch to the frame that first draws the surface being submitted. */
  to_frame_submitted_ms: number;
  /** As above, to the GPU's work-done callback (which Chromium reports a few frames late). */
  to_gpu_done_ms: number;
}

export async function loadStaged(
  viewport: Viewport,
  staged: StagedMesh,
  options: { transport?: Transport; colour?: number; over?: boolean; byRl?: boolean } = {},
): Promise<{ group: THREE.Group; timings: LoadTimings }> {
  const start = performance.now();
  const fetched = await fetchMesh(staged.handle, options.transport);
  const received = performance.now();
  const decoded = decodeMesh(fetched.buffer);
  // Grid tiles are rebuilt into fresh arrays, so a grid's shared buffer can go straight away;
  // triangulation chunks view the buffer until the surface is removed.
  const grid = decoded.header.kind === 'grid';
  if (grid) fetched.release();
  const group = viewport.addMesh(decoded, grid ? undefined : fetched.release, options);
  const decodedAt = performance.now();
  const frame = await viewport.nextFrame();
  return {
    group,
    timings: {
      transport: fetched.transport,
      fetch_ms: received - start,
      decode_ms: decodedAt - received,
      to_frame_submitted_ms: frame.submitted - start,
      to_gpu_done_ms: frame.gpuDone - start,
    },
  };
}

/** A surface this big or bigger gets coarser versions to draw far off (decision 0097); a smaller one costs nothing. */
export const LEVELS_FROM_TRIANGLES = 100_000;

/** The surfaces waiting for their versions, one made at a time, so they don't fight over the shell's cores. */
let levelsQueue: Promise<unknown> = Promise.resolve();

/**
 * Asks for a drawn surface's coarser versions and puts them on when they come (decision 0097), after it's drawn at full
 * detail, so the view never waits on them. Resolves to how many versions went on: none if the surface is too small,
 * has gone, or was sent again meanwhile, or the shell has none for it.
 */
export function loadLevels(viewport: Viewport, group: THREE.Group, stage: () => Promise<StagedLevels>, triangles: number): Promise<number> {
  if (triangles < LEVELS_FROM_TRIANGLES) return Promise.resolve(0);
  const next = levelsQueue.then(async () => {
    if (!group.parent) return 0;
    try {
      const staged = await stage();
      const fetched = await fetchMesh(staged.handle);
      return viewport.attachLevels(group, decodeLevels(fetched.buffer), fetched.release);
    } catch {
      return 0;
    }
  });
  levelsQueue = next;
  return next;
}


/** A layer's `[r, g, b]` as a three.js colour number. */
export function colourOf(rgb: [number, number, number]): number {
  return (rgb[0] << 16) | (rgb[1] << 8) | rgb[2];
}
