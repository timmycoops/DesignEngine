// Typed wrappers around the Tauri commands in app/src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';

export type Vec3 = [number, number, number];

export interface MeshHeader {
  id: number;
  name: string;
  log: string;
  vertices: number;
  triangles: number;
  origin: Vec3;
  bounds_local?: { min: Vec3; max: Vec3 };
  timings: Record<string, number>;
}

export interface DecodedMesh {
  header: MeshHeader;
  bytes: number;
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
}

/** Views over a render buffer: f32 positions, f32 normals, u32 indices, JSON header, u32 header length. */
export function decodeMesh(buffer: ArrayBuffer): DecodedMesh {
  const view = new DataView(buffer);
  const headerLength = view.getUint32(buffer.byteLength - 4, true);
  const headerStart = buffer.byteLength - 4 - headerLength;
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, headerStart, headerLength))) as MeshHeader;
  const positions = new Float32Array(buffer, 0, header.vertices * 3);
  const normals = new Float32Array(buffer, positions.byteLength, header.vertices * 3);
  const indices = new Uint32Array(buffer, positions.byteLength * 2, header.triangles * 3);
  return { header, bytes: buffer.byteLength, positions, normals, indices };
}

export const openSurface = (path: string) => invoke<ArrayBuffer>('open_surface', { path }).then(decodeMesh);

export const clearSurfaces = () => invoke<void>('clear_surfaces');

export interface StagedMesh {
  handle: number;
  triangles: number;
  bytes: number;
  prepare_ms: number;
}

export const prepareSynthetic = (cells: number) => invoke<StagedMesh>('prepare_synthetic', { cells });

export const fetchStaged = (handle: number) => invoke<ArrayBuffer>('fetch_staged', { handle });

export interface SnapIndex {
  ms: number;
  surfaces: number;
  triangles: number;
}

export const buildSnapIndex = () => invoke<SnapIndex>('build_snap_index');

export interface SnapHit {
  local: Vec3;
  world: Vec3;
  surface: number;
}

export const snapRay = (origin: Vec3, direction: Vec3) => invoke<SnapHit | null>('snap_ray', { origin, direction });

export interface BenchConfig {
  surfaces: string[];
  results_path: string;
  scale_targets: number[];
  transfer_cells: number[];
  orbit_seconds: number;
  drag_updates: number;
}

export const benchConfig = () => invoke<BenchConfig | null>('bench_config');

export const writeResults = (text: string) => invoke<string>('write_results', { text });

export const exitApp = () => invoke<void>('exit_app');
