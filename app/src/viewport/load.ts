// Fetches a staged render buffer, decodes it and adds it to the viewport, timing each step.
import { decodeMesh, fetchMesh, type StagedMesh, type Transport } from '../bridge';
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
  options: { transport?: Transport; colour?: number; over?: boolean } = {},
): Promise<{ group: THREE.Group; timings: LoadTimings }> {
  const start = performance.now();
  const fetched = await fetchMesh(staged.handle, options.transport);
  const received = performance.now();
  const decoded = decodeMesh(fetched.buffer);
  // Grid tiles are rebuilt into fresh arrays, so a grid's shared buffer can go straight away;
  // triangulation chunks view the buffer until the surface is removed.
  const grid = decoded.header.kind === 'grid';
  if (grid) fetched.release();
  const group = viewport.addMesh(decoded, grid ? undefined : fetched.release, options.colour, options.over);
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

/** What is selected shows in this, wherever it is drawn. */
export const SELECTED = 0xffd23f;

/** A layer's `[r, g, b]` as a three.js colour number. */
export function colourOf(rgb: [number, number, number]): number {
  return (rgb[0] << 16) | (rgb[1] << 8) | rgb[2];
}
