// Typed wrappers around the Tauri commands in app/src-tauri/src/commands.rs, and decoding of the
// compact render buffers described in app/src-tauri/src/render.rs.
import { invoke } from '@tauri-apps/api/core';

export type Vec3 = [number, number, number];

interface ChunkMeta {
  vertex_start: number;
  vertex_count: number;
  index_start: number;
  index_count: number;
  min: Vec3;
  max: Vec3;
}

interface TileMeta {
  col: number;
  row: number;
  cols: number;
  rows: number;
  offset: number;
  nulls: boolean;
  triangles: number;
  min: Vec3;
  max: Vec3;
}

export interface MeshHeader {
  id: number;
  name: string;
  kind: 'triangulation' | 'grid';
  vertices: number;
  triangles: number;
  bounds_local?: { min: Vec3; max: Vec3 };
  timings: Record<string, number>;
  chunks?: ChunkMeta[];
  lattice?: { origin: [number, number]; cell_size: [number, number] };
  z?: { min: number; step: number; null: number };
  tiles?: TileMeta[];
}

/** One draw call's worth of geometry: f32 positions in scene-local coordinates, u16 indices. */
export interface ChunkGeometry {
  positions: Float32Array;
  indices: Uint16Array;
  min: Vec3;
  max: Vec3;
}

export interface DecodedMesh {
  header: MeshHeader;
  bytes: number;
  chunks: ChunkGeometry[];
}

/** Views over a render buffer (arrays first, then the JSON header, then its u32 length). */
export function decodeMesh(buffer: ArrayBuffer): DecodedMesh {
  const view = new DataView(buffer);
  const headerLength = view.getUint32(buffer.byteLength - 4, true);
  const headerStart = buffer.byteLength - 4 - headerLength;
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, headerStart, headerLength))) as MeshHeader;
  const chunks = header.kind === 'grid' ? decodeGrid(buffer, header) : decodeTriangulation(buffer, header);
  return { header, bytes: buffer.byteLength, chunks };
}

/** Chunk views straight onto the buffer: no copying. */
function decodeTriangulation(buffer: ArrayBuffer, header: MeshHeader): ChunkGeometry[] {
  const chunks = header.chunks ?? [];
  const indexTotal = chunks.reduce((n, c) => n + c.index_count, 0);
  const positions = new Float32Array(buffer, 0, header.vertices * 3);
  const indices = new Uint16Array(buffer, header.vertices * 12, indexTotal);
  return chunks.map((c) => ({
    positions: positions.subarray(c.vertex_start * 3, (c.vertex_start + c.vertex_count) * 3),
    indices: indices.subarray(c.index_start, c.index_start + c.index_count),
    min: c.min,
    max: c.max,
  }));
}

/**
 * Rebuilds grid tiles: x and y from the lattice, z from the quantised elevations, and triangles by
 * the same rule as zd_geom::triangulate_grid (cell corners SW, SE, NE, NW; two triangles for four
 * live corners, one for three). Tiles without nulls share one index array per tile size.
 */
function decodeGrid(buffer: ArrayBuffer, header: MeshHeader): ChunkGeometry[] {
  const { lattice, z: zq, tiles = [] } = header;
  if (!lattice || !zq) throw new Error('grid buffer without lattice');
  const z = new Uint16Array(buffer, 0, header.vertices);
  const [ox, oy] = lattice.origin;
  const [dx, dy] = lattice.cell_size;
  const fullIndices = new Map<string, Uint16Array>();
  let triangles = 0;
  const chunks = tiles.map((t) => {
    const positions = new Float32Array(t.cols * t.rows * 3);
    for (let r = 0, i = 0; r < t.rows; r++) {
      const y = oy + (t.row + r) * dy;
      for (let c = 0; c < t.cols; c++, i++) {
        const q = z[t.offset + i];
        positions[3 * i] = ox + (t.col + c) * dx;
        positions[3 * i + 1] = y;
        positions[3 * i + 2] = q === zq.null ? 0 : zq.min + q * zq.step;
      }
    }
    let indices: Uint16Array;
    if (t.nulls) {
      indices = tileIndices(t.cols, t.rows, (i) => z[t.offset + i] !== zq.null);
    } else {
      const key = `${t.cols}x${t.rows}`;
      indices = fullIndices.get(key) ?? tileIndices(t.cols, t.rows, () => true);
      fullIndices.set(key, indices);
    }
    triangles += indices.length / 3;
    return { positions, indices, min: t.min, max: t.max };
  });
  if (triangles !== header.triangles) {
    throw new Error(`grid decode made ${triangles} triangles but the engine made ${header.triangles}`);
  }
  return chunks;
}

function tileIndices(cols: number, rows: number, live: (node: number) => boolean): Uint16Array {
  const out = new Uint16Array((cols - 1) * (rows - 1) * 6);
  let n = 0;
  const ring = [0, 0, 0, 0];
  for (let r = 0; r < rows - 1; r++) {
    for (let c = 0; c < cols - 1; c++) {
      const sw = r * cols + c;
      let k = 0;
      for (const v of [sw, sw + 1, sw + cols + 1, sw + cols]) if (live(v)) ring[k++] = v;
      if (k >= 3) {
        out[n++] = ring[0];
        out[n++] = ring[1];
        out[n++] = ring[2];
      }
      if (k === 4) {
        out[n++] = ring[0];
        out[n++] = ring[2];
        out[n++] = ring[3];
      }
    }
  }
  return n === out.length ? out : out.slice(0, n);
}

// WebView2 delivers shared buffers as page events; match them to requests by handle.
interface SharedBufferEvent {
  getBuffer(): ArrayBuffer;
  additionalData: { handle?: number } | null;
}

interface WebView2Bridge {
  addEventListener(type: 'sharedbufferreceived', listener: (e: SharedBufferEvent) => void): void;
  releaseBuffer(buffer: ArrayBuffer): void;
}

const webview2 = (window as unknown as { chrome?: { webview?: WebView2Bridge } }).chrome?.webview;
const awaitingBuffers = new Map<number, (buffer: ArrayBuffer) => void>();
webview2?.addEventListener('sharedbufferreceived', (e) => {
  const handle = e.additionalData?.handle;
  const resolve = handle === undefined ? undefined : awaitingBuffers.get(handle);
  if (handle !== undefined && resolve) {
    awaitingBuffers.delete(handle);
    resolve(e.getBuffer());
  } else {
    webview2.releaseBuffer(e.getBuffer());
  }
});

export type Transport = 'shared' | 'ipc';
export const defaultTransport: Transport = webview2 ? 'shared' : 'ipc';

export interface FetchedBuffer {
  buffer: ArrayBuffer;
  transport: Transport;
  /** Frees a shared buffer's memory; call once nothing views the buffer. No-op for IPC. */
  release: () => void;
}

export async function fetchMesh(handle: number, transport: Transport = defaultTransport): Promise<FetchedBuffer> {
  if (transport === 'shared') {
    if (!webview2) throw new Error('shared buffers need WebView2');
    const received = new Promise<ArrayBuffer>((resolve) => awaitingBuffers.set(handle, resolve));
    await invoke<void>('fetch_mesh_shared', { handle });
    const buffer = await received;
    return { buffer, transport, release: () => webview2.releaseBuffer(buffer) };
  }
  const buffer = await invoke<ArrayBuffer>('fetch_mesh_ipc', { handle });
  return { buffer, transport, release: () => {} };
}

export interface StagedMesh {
  handle: number;
  name: string;
  kind: 'triangulation' | 'grid';
  triangles: number;
  bytes: number;
  log: string;
  timings: Record<string, number>;
}

export const openSurface = (path: string) => invoke<StagedMesh>('open_surface', { path });

export const prepareSynthetic = (cells: number, kind: 'triangulation' | 'grid') =>
  invoke<StagedMesh>('prepare_synthetic', { cells, kind });

export const clearSurfaces = () => invoke<void>('clear_surfaces');

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
