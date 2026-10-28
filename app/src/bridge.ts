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

/** Encodes a drawn object's geometry for fetching. */
export const stageObject = (id: string) => invoke<StagedMesh>('stage_object', { id });

export const prepareSynthetic = (cells: number, kind: 'triangulation' | 'grid') =>
  invoke<StagedMesh>('prepare_synthetic', { cells, kind });

// The project, as the engine session holds it (app/src-tauri/src/commands.rs).

export interface Layer {
  id: string;
  name: string;
  colour: [number, number, number];
  visible: boolean;
}

export type ObjectKind = 'string' | 'surface' | 'solid' | 'grid';

export interface ObjectSummary {
  id: string;
  revision: number;
  name: string;
  layer: string;
  kind: ObjectKind;
  attributes: Record<string, boolean | number | string>;
  /** Drawn as a surface: surfaces, solids and elevation grids. */
  drawn: boolean;
  /** Changes only when the geometry does. */
  geometry_key: string;
  bounds_local: [Vec3, Vec3] | null;
  /** String vertices, offset from the scene origin. */
  vertices_local: Vec3[] | null;
  closed: boolean | null;
}

export interface ProjectSummary {
  path: string | null;
  can_undo: boolean;
  can_redo: boolean;
  origin: Vec3 | null;
  layers: Layer[];
  objects: ObjectSummary[];
}

export interface RunRecord {
  run: string;
  command: string;
  params: unknown;
  inputs: { id: string; revision: number }[];
  outputs: { id: string; revision: number; change: 'created' | 'revised' | 'deleted'; name: string }[];
  layers_created: string[];
  started_unix_ms: number;
  duration_ms: number;
  messages: string[];
  status: 'ok' | { failed: string };
}

export interface RunReply {
  record: RunRecord;
  summary: ProjectSummary;
}

export interface SaveReply {
  path: string;
  object_files_written: number;
  object_files_reused: number;
  files_deleted: number;
  bytes_written: number;
  ms: number;
}

export const projectSummary = () => invoke<ProjectSummary>('project_summary');

/** Runs an engine command by name; rejects with the error text if the command fails. */
export const runCommand = (name: string, params: Record<string, unknown>) => invoke<RunReply>('run_command', { name, params });

export const undo = () => invoke<ProjectSummary>('undo');
export const redo = () => invoke<ProjectSummary>('redo');
export const newProject = () => invoke<ProjectSummary>('new_project');
export const openProject = (path: string) => invoke<ProjectSummary>('open_project', { path });
/** Saves to `path`, or to the folder last opened or saved. */
export const saveProject = (path?: string) => invoke<SaveReply>('save_project', { path: path ?? null });
export const setLayerVisible = (layer: string, visible: boolean) =>
  invoke<ProjectSummary>('set_layer_visible', { layer, visible });

/** The session's run log, oldest first: every run (failed ones too), undo, redo, save and open. */
export type LogEntry =
  | ({ event: 'run' } & RunRecord)
  | { event: 'undo' | 'redo'; run: string; at_unix_ms: number }
  | { event: 'saved' | 'opened'; path: string; at_unix_ms: number };

export const runLog = () => invoke<LogEntry[]>('run_log');

/** Validity flags in Seequent Evo vocabulary, with the counts behind them. */
export interface MeshValidity {
  closed: boolean;
  manifold: boolean;
  consistent_winding: boolean;
  non_degenerate: boolean;
  /** Not computed yet. */
  non_self_intersecting: boolean | null;
  boundary_edges: number;
  non_manifold_edges: number;
  inconsistent_edges: number;
  degenerate_triangles: number;
}

export type Measures =
  | { kind: 'string'; points: Vec3[]; closed: boolean; length: number; plan_length: number; plan_area: number | null }
  | { kind: 'surface'; vertices: number; triangles: number; area: number; validity: MeshValidity }
  | { kind: 'solid'; vertices: number; triangles: number; area: number; volume: number; validity: MeshValidity }
  | {
      kind: 'grid';
      origin: [number, number];
      cell_size: [number, number];
      cols: number;
      rows: number;
      nulls: number;
      z_range: [number, number] | null;
    };

/** One revision of an object and the run that wrote it. */
export interface RevisionSummary {
  revision: number;
  run: string;
  /** Null when the run is not in this session's log. */
  command: string | null;
  at_unix_ms: number | null;
}

/** The current revision of one object, in world coordinates. */
export interface ObjectDetails {
  id: string;
  name: string;
  layer: string;
  attributes: Record<string, boolean | number | string>;
  bounds: [Vec3, Vec3] | null;
  /** Oldest first; the last entry is the current revision. */
  history: RevisionSummary[];
  measures: Measures;
}

export const objectDetails = (id: string) => invoke<ObjectDetails>('object_details', { id });

export interface SnapIndex {
  ms: number;
  surfaces: number;
  triangles: number;
}

export const buildSnapIndex = () => invoke<SnapIndex>('build_snap_index');

export interface SnapHit {
  /** Offset from the scene origin, for drawing. */
  local: Vec3;
  /** Mine grid coordinates, for commands. */
  world: Vec3;
  object: string;
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
