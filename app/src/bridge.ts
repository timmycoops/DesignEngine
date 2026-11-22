// Typed wrappers around the Tauri commands in app/src-tauri/src/commands.rs, and decoding of the
// compact render buffers described in app/src-tauri/src/render.rs.
import { invoke } from '@tauri-apps/api/core';
import { type LogPart, LogStore, ProjectStore } from './projectStore';
import { running, waitingOn } from './running';
import type { LicenceStatus } from './licence';

export type Vec3 = [number, number, number];

interface ChunkMeta {
  vertex_start: number;
  vertex_count: number;
  index_start: number;
  index_count: number;
  min: Vec3;
  max: Vec3;
  /** The walls two solids share (decision 0097): drawn only when a solid of the tile is hidden or it's seen through. */
  shared?: boolean;
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
  /** A second number per node to colour by (seam thickness); `flag` marks a node with an issue. */
  value?: { name: string; min: number; max: number; step: number; null: number; flag: number };
  tiles?: TileMeta[];
  /** A layer's solids: their ids, in the order each vertex numbers its solid (`ChunkGeometry.solids`). */
  solids?: string[];
}

/** One draw call's worth of geometry: f32 positions in scene-local coordinates, u16 indices. */
export interface ChunkGeometry {
  positions: Float32Array;
  indices: Uint16Array;
  min: Vec3;
  max: Vec3;
  /** Quantised values per vertex, when the buffer carries them (see `MeshHeader.value`). */
  values?: Uint16Array;
  /** A layer's solids: which solid each vertex is of, by its place in `MeshHeader.solids`. */
  solids?: Float32Array;
  /** The walls two solids share: inside while both are drawn (decision 0097). */
  shared?: boolean;
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
  // A layer's solids: after the indices, padded to 4 bytes, the solid each vertex is of.
  const after = Math.ceil((header.vertices * 12 + indexTotal * 2) / 4) * 4;
  const solids = header.solids ? new Float32Array(buffer, after, header.vertices) : null;
  return chunks.map((c) => ({
    positions: positions.subarray(c.vertex_start * 3, (c.vertex_start + c.vertex_count) * 3),
    indices: indices.subarray(c.index_start, c.index_start + c.index_count),
    min: c.min,
    max: c.max,
    solids: solids?.subarray(c.vertex_start, c.vertex_start + c.vertex_count),
    shared: c.shared,
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
  // Values follow the elevations in the same tile order; copied, because the buffer is released.
  const values = header.value ? new Uint16Array(buffer, header.vertices * 2, header.vertices) : null;
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
    const tileValues = values?.slice(t.offset, t.offset + t.cols * t.rows);
    return { positions, indices, min: t.min, max: t.max, values: tileValues };
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
export const stageLayerSolids = (layer: string, tile: [number, number]) => invoke<StagedMesh>('stage_layer_solids', { layer, tile });
/** The coarser versions of a drawn surface's chunks, made after it's drawn (decision 0097); decode with `decodeLevels`. */
export const stageLevels = (id: string) => invoke<StagedLevels>('stage_levels', { id });
/** The coarser versions of a tile of a layer's solids, block walls kept as they are (decision 0097, workstream 4). */
export const stageLayerLevels = (layer: string, tile: [number, number]) => invoke<StagedLevels>('stage_layer_levels', { layer, tile });
export type StagedLevels = Omit<StagedMesh, 'kind'> & { kind: 'levels' };
/** The selected solids on show as one mesh, to light up over their layers'. */
/** The box round these objects together, world coordinates, for zoom to selection; null if none has a place. */
export const objectsBounds = (objects: string[]) => invoke<[Vec3, Vec3] | null>('objects_bounds', { objects });
/** The solids among these holding less than `volume` m³, in the order given: small pieces to look at and delete by hand. */
export const solidsUnder = (objects: string[], volume: number) => invoke<string[]>('solids_under', { objects, volume });
/** What slicing these solids by these faces would refuse, found before the cut: a warning for each face that doesn't pass right through a solid. */
export const checkSlice = (solids: string[], faces: string[]) => invoke<Note[]>('check_slice', { solids, faces });
/** What cutting these solids by these block polygons would refuse, found before the cut: a warning for each part of a solid outside them all. */
export const checkCutByPolygons = (solids: string[], polygons: string[]) => invoke<Note[]>('check_cut_by_polygons', { solids, polygons });
/** What cutting these solids by a seam model would refuse, found before the cut: a warning for each part of a solid past the edge of its grids or over a hole in them. */
export const checkCutBySeams = (solids: string[], model: string) => invoke<Note[]>('check_cut_by_seams', { solids, model });
/**
 * Everything on show lying wholly inside a box on the viewport: the camera's view-projection matrix in the scene's
 * local frame (column-major) and the box's corners in normalised device coordinates.
 */
export const objectsInBox = (matrix: number[], min: [number, number], max: [number, number]) => invoke<string[]>('objects_in_box', { matrix, min, max });

export type SectionKind = 'surface' | 'solid' | 'seam' | 'string' | 'selected';

/** Where a section cuts one layer's objects of one kind. */
export interface SectionGroup {
  layer: string;
  kind: SectionKind;
  /** The surface or seam model the lines belong to, or the selected object. */
  object: string | null;
  /** A seam horizon's material in lower case; empty for anything else. */
  material: string;
  segments: number;
  /** What its closed loops fill, and the loops that could not be filled. */
  triangles: number;
  unfilled: number;
}

export interface SectionLines {
  groups: SectionGroup[];
  /** Six floats a segment, scene-local, group after group. */
  positions: Float32Array;
  /** Nine floats a triangle, scene-local, group after group. */
  fills: Float32Array;
  /** Distance along the section and RLs of everything cut (m). */
  bounds: { along: [number, number]; rl: [number, number] } | null;
  ms: number;
}

/** The section through two plan points (mine grid): lines and fills as `f32`, then a JSON header and its length. */
export async function sectionLines(a: [number, number], b: [number, number], width: number, fill: boolean, selected: string | null): Promise<SectionLines> {
  const buffer = await invoke<ArrayBuffer>('section_lines', { a, b, width, fill, selected });
  const length = new DataView(buffer).getUint32(buffer.byteLength - 4, true);
  const start = buffer.byteLength - 4 - length;
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, start, length)));
  const floats = new Float32Array(buffer, 0, start / 4);
  const lineFloats = (header.groups as SectionGroup[]).reduce((sum, group) => sum + group.segments * 6, 0);
  return { ...header, positions: floats.subarray(0, lineFloats), fills: floats.subarray(lineFloats) };
}

export const prepareSynthetic = (cells: number, kind: 'triangulation' | 'grid') =>
  invoke<StagedMesh>('prepare_synthetic', { cells, kind });

// The project, as the engine session holds it (app/src-tauri/src/commands.rs).

export interface Layer {
  id: string;
  name: string;
  colour: [number, number, number];
  visible: boolean;
}

export type ObjectKind = 'string' | 'surface' | 'solid' | 'grid' | 'seam_model' | 'face_design' | 'workflow' | 'parameter_table' | 'dataflow' | 'table';

export interface ObjectSummary {
  id: string;
  revision: number;
  name: string;
  /** The layer's id (see `Layer.id`), not its name. */
  layer: string;
  kind: ObjectKind;
  /**
   * The attributes the app reads by name (`ROW_ATTRIBUTES` in changes.rs, and the `road_top` ones), not all of them:
   * `objectDetails` and `objectsAttributes` have the rest.
   */
  attributes: Record<string, boolean | number | string>;
  /** Drawn as a surface: surfaces, solids and elevation grids. */
  drawn: boolean;
  /** Hidden on its own: not drawn, snapped to or cut by a section. */
  hidden: boolean;
  /** Left out by a layer's filter (decision 0088): drawn, snapped to and cut as hidden, but not hidden on its own. */
  filtered: boolean;
  /** Changes only when the geometry does. */
  geometry_key: string;
  /** String vertices, offset from the scene origin. Strings only: other rows leave it out. */
  vertices_local?: Vec3[] | null;
  closed?: boolean | null;
}

export interface ProjectSummary {
  path: string | null;
  can_undo: boolean;
  can_redo: boolean;
  origin: Vec3 | null;
  layers: Layer[];
  objects: ObjectSummary[];
  /** The layers holding solids, each drawn as one mesh. */
  solid_layers: SolidLayer[];
  /** How objects and layers look. */
  display: Display;
  /** Who else had the project file open when it was opened: a save to it is refused. */
  locked_by: string | null;
  /** The run log as it stands: which session's, and how many entries. */
  log: LogMark;
}

export interface LogMark {
  session: number;
  len: number;
}

/**
 * What the shell sends after a change (`changes.rs`): the objects added or changed and those gone since what it sent as
 * `base`, or the whole project when `base` is null. `ProjectStore` takes it and gives back the whole project.
 */
export interface ProjectChanges extends Omit<ProjectSummary, 'objects'> {
  base: number | null;
  epoch: number;
  objects: ObjectSummary[];
  removed: string[];
  /** Every object's id in order, when it isn't what was held, less what's gone, with the new at the end. */
  order: string[] | null;
}

/** In bands by RL, or one colour, the layer's. */
export type Colouring = 'by_rl' | 'layer';

/** How one object looks: colour and how see-through it is (1 is solid). */
export interface Look {
  colouring: Colouring;
  opacity: number;
}

/** The bands of colour by RL. */
export interface Contours {
  /** Metres between band edges; null picks one from the RL range. */
  interval: number | null;
  /** Thin lines at the band edges: off unless turned on. */
  edge_lines: boolean;
  /** One colour range across everything on show, rather than each object's own. */
  shared: boolean;
}

/** Display state saved with the project: only looks that differ from the default are listed. */
export interface Display {
  objects?: Record<string, Look>;
  /** A layer's opacity, which multiplies its objects'. */
  layers?: Record<string, number>;
  contours: Contours;
  /** A layer's filter by attributes and measures, by layer id (decision 0088): what doesn't fit isn't drawn. */
  filters?: Record<string, Record<string, string>>;
  /** A reserves table's leaves filter carried to the view (decision 0090). */
  reserves?: ReservesFilter;
}

/** A reserves table's filter in the view (decision 0090): the solids on its leaves `when` leaves out, and with `coal_only` its waste, aren't drawn. */
export interface ReservesFilter {
  table: string;
  when?: Record<string, string>;
  coal_only?: boolean;
}

/** Not drawn, snapped to or cut by a section: hidden on its own, or left out by a layer's filter. */
export const concealed = (o: { hidden: boolean; filtered?: boolean }) => o.hidden || !!o.filtered;

export const DEFAULT_LOOK: Look = { colouring: 'by_rl', opacity: 1 };
export const lookOf = (display: Display | undefined, id: string): Look => display?.objects?.[id] ?? DEFAULT_LOOK;
export const layerOpacityOf = (display: Display | undefined, id: string): number => display?.layers?.[id] ?? 1;

/** `display` with these objects' looks changed as the engine changes them (null leaves that part), for showing at once. */
export function withLooks(display: Display, ids: string[], colouring: Colouring | null, opacity: number | null): Display {
  const objects = { ...display.objects };
  for (const id of ids) {
    const was = objects[id] ?? DEFAULT_LOOK;
    const look = { colouring: colouring ?? was.colouring, opacity: opacity === null ? was.opacity : Math.min(Math.max(opacity, 0), 1) };
    if (look.colouring === DEFAULT_LOOK.colouring && look.opacity === DEFAULT_LOOK.opacity) delete objects[id];
    else objects[id] = look;
  }
  return { ...display, objects };
}

/** `display` with a layer's opacity changed, for showing at once. */
export function withLayerOpacity(display: Display, layer: string, opacity: number): Display {
  const layers = { ...display.layers };
  if (opacity >= 1) delete layers[layer];
  else layers[layer] = Math.max(opacity, 0);
  return { ...display, layers };
}

/** A layer's solids as the viewport draws them. */
/** A tile of a layer's solids, drawn as one mesh (decision 0060, workstream 4). */
export interface SolidLayer {
  layer: string;
  /** Column and row of the 1,000 m square holding the middle of each solid's plan. */
  tile: [number, number];
  /** Changes only when the geometry of a solid in the tile does, or one comes or goes. */
  key: string;
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
  /** What the run reported, each line a note (decision 0059). */
  notes: Note[];
  /** The notes' sentences, in order. */
  messages: string[];
  status: 'ok' | { failed: string };
  /** The kind of refusal, such as `refused.invalid`, when the run was refused. */
  refused?: string;
}

/** How much a note matters. */
export type Severity = 'info' | 'warning' | 'blocking';

/** Where a note is, in mine grid coordinates. */
export type Place =
  | { kind: 'point'; at: [number, number, number] }
  | { kind: 'plan'; at: [number, number] }
  | { kind: 'stretch'; from: [number, number, number]; to: [number, number, number] };

/** One line of what a run reports: its severity, a fixed code for what happened, the sentence, and where. */
export interface Note {
  severity: Severity;
  /** A fixed name, such as `wall.no_height`; none for a line not yet given one. */
  code?: string;
  text: string;
  /** The object it's about. */
  object?: string;
  place?: Place;
  value?: { value: number; unit: string };
}

export interface RunReply {
  record: RunRecord;
  summary: ProjectSummary;
}

export interface SaveReply {
  /** The `.zdp` file written: a project opened from an older `.zdproj` folder saves beside it. */
  path: string;
  /** Nothing had changed since the last save, so nothing was written. */
  unchanged: boolean;
  entries: number;
  /** Entries encoded this time; the rest were kept from the last save. */
  encoded: number;
  /** The file's size. */
  bytes: number;
  ms: number;
}

const held = new ProjectStore();

/** The project after a reply's changes, asking for the whole of it if they don't follow on from what the app holds. */
async function summaryAfter(changes: ProjectChanges): Promise<ProjectSummary> {
  return held.take(changes) ?? held.take(await invoke<ProjectChanges>('project_summary'))!;
}

/** The whole project, sent whole. */
export const projectSummary = async () => held.take(await invoke<ProjectChanges>('project_summary'))!;

/** Runs an engine command by name; rejects with the error text if the command fails. */
/**
 * One run at a time (decision 0060): it reads the project off the shell's lock, so the view answers while it goes, and
 * while it goes nothing else may change the project. The shell refuses that too; this says so before asking.
 */
export const runCommand = async (name: string, params: Record<string, unknown>): Promise<RunReply> => {
  if (!running.start(name, performance.now())) throw new Error(waitingOn() ?? 'Waiting on a run');
  try {
    const reply = await invoke<{ record: RunRecord; changes: ProjectChanges }>('run_command', { name, params });
    return { record: reply.record, summary: await summaryAfter(reply.changes) };
  } finally {
    running.end();
  }
};

/** A workflow run's record and the project after it (decision 0065). */
export interface WorkflowReply {
  record: WorkflowRecord;
  summary: ProjectSummary;
}

/**
 * Runs a workflow from step `from` (from 1) to its end, a pause or the first refusal; one undo. One run at a time, as
 * any command: the status line names each step's command as it goes, and Cancel stops it.
 */
export const runWorkflow = async (workflow: string, from: number, workflowCase = 'Base'): Promise<WorkflowReply> => {
  if (!running.start('workflow', performance.now())) throw new Error(waitingOn() ?? 'Waiting on a run');
  try {
    const reply = await invoke<{ record: WorkflowRecord; changes: ProjectChanges }>('run_workflow', { workflow, from, case: workflowCase });
    return { record: reply.record, summary: await summaryAfter(reply.changes) };
  } finally {
    running.end();
  }
};

/** Every case of a workflow, Base first, as one undo; `from` carries on after a pause: that case from that step, then the rest. */
export const runAllCases = async (workflow: string, from?: { case: string; step: number }): Promise<{ records: WorkflowRecord[]; summary: ProjectSummary }> => {
  if (!running.start('workflow', performance.now())) throw new Error(waitingOn() ?? 'Waiting on a run');
  try {
    const reply = await invoke<{ records: WorkflowRecord[]; changes: ProjectChanges }>('run_all_cases', { workflow, from: from ?? null });
    return { records: reply.records, summary: await summaryAfter(reply.changes) };
  } finally {
    running.end();
  }
};

/** Captures runs from the run log as a new workflow, each object id a rule; saved by a `workflow.save` run. */
export const captureWorkflow = async (name: string, runs: string[]): Promise<RunReply> => {
  idle();
  const reply = await invoke<{ record: RunRecord; changes: ProjectChanges }>('capture_workflow', { name, runs });
  return { record: reply.record, summary: await summaryAfter(reply.changes) };
};

/** Adds a tool panel's command and settings to a workflow as its last step, its objects turned into rules. */
export const addWorkflowStep = async (workflow: string, command: string, params: Record<string, unknown>, note?: string, at?: number): Promise<RunReply> => {
  idle();
  const reply = await invoke<{ record: RunRecord; changes: ProjectChanges }>('add_workflow_step', { workflow, at: at ?? null, command, params, note: note ?? null });
  return { record: reply.record, summary: await summaryAfter(reply.changes) };
};

/** A workflow report, an HTML page in the reports folder beside the project (decision 0068). */
export interface ReportFile {
  path: string;
  name: string;
  modified_unix_ms: number;
}

/** Writes a workflow's report, every case from its latest run, and opens it in the browser. */
export const writeWorkflowReport = (workflow: string): Promise<ReportFile> =>
  invoke<ReportFile>('write_workflow_report', { workflow, utcOffsetMinutes: -new Date().getTimezoneOffset() });

/** The reports written for a workflow, newest first; none while the project isn't saved. */
export const workflowReports = (workflow: string): Promise<ReportFile[]> => invoke<ReportFile[]>('workflow_reports', { workflow });

/** Opens one of them in the browser. */
export const openWorkflowReport = (path: string): Promise<void> => invoke<void>('open_workflow_report', { path });

/** Throws, saying what it's waiting on, while a run goes: a change to the project waits until it ends. */
function idle() {
  const waiting = waitingOn();
  if (waiting) throw new Error(waiting);
}

/** How far the run going has got, if one is. */
export interface RunProgress {
  command: string;
  seconds: number;
  done: number;
  /** 0 until the run says what it's counting. */
  total: number;
  of: string;
  cancelled: boolean;
}

export const runProgress = () => invoke<RunProgress | null>('run_progress');
/** Asks the run going to stop at the next solid, strip or file; it applies nothing. */
export const cancelRun = () => invoke<boolean>('cancel_run');

export const undo = async () => (idle(), summaryAfter(await invoke<ProjectChanges>('undo')));
export const redo = async () => (idle(), summaryAfter(await invoke<ProjectChanges>('redo')));
export const newProject = async () => (idle(), summaryAfter(await invoke<ProjectChanges>('new_project')));
export const openProject = async (path: string) => (idle(), summaryAfter(await invoke<ProjectChanges>('open_project', { path })));

/** Every attribute of each object, in the order given (a row carries only those the app reads by name). */
export const objectsAttributes = (objects: string[]) => invoke<Record<string, boolean | number | string>[]>('objects_attributes', { objects });

/** Each solid's own volume and every attribute, in the order asked, for the block table; null volume if it's gone. */
export const solidsTable = (objects: string[]) => invoke<{ volume: number | null; attributes: Record<string, boolean | number | string> }[]>('solids_table', { objects });

/** A column of a grid (Tables and scripts plan, decision 1): its full name, last part, folders, unit and rule. */
export interface GridColumn {
  name: string;
  label: string;
  folders: string[];
  unit: string;
  text: boolean;
  add_up: AddUp | null;
  /** Written by a run, or entered by a person: only an entered column's cells are typed in (decision 2). */
  kind: 'written' | 'entered';
}

/** One row of a grid: the total, a group's subtotal, or a leaf (a row of the table). */
export interface GridRow {
  kind: 'total' | 'group' | 'leaf';
  path: string[];
  label: string;
  depth: number;
  leaves: number;
  open: boolean;
  row: number | null;
  /** A leaf's words for the table's levels, top down. */
  words: string[] | null;
  cells: (number | string | null)[];
}

/** A table as a grid: the columns shown, every column, what it can be grouped by, and its rows. */
export interface Grid {
  columns: GridColumn[];
  every: GridColumn[];
  groups: string[];
  levels: string[];
  rows: GridRow[];
  leaves: number;
  /** Leaf rows left out past the limit. */
  cut: number;
}

export type GridSource = { of: 'table'; table: string } | { of: 'objects'; objects: string[]; density: string | null };

export interface GridAsk {
  when?: Record<string, string>;
  group?: string[];
  open?: string[][];
  columns?: string[];
  sort?: { column: string; descending: boolean } | null;
  limit?: number;
}

/** What a reserves table was made from that has changed since its run (Tables and scripts plan, decision 4). */
export interface ChangedSince {
  blocks: number;
  others: string[];
  gone: number;
}

export const reservesChanged = (object: string) => invoke<ChangedSince>('reserves_changed', { object });

/** What a CSV updates (Tables and scripts plan, decision 5): objects' attributes, or a table's entered columns. */
export type RecordsTarget = { of: 'objects'; objects: string[] } | { of: 'table'; table: string };

export interface UpdateRecordsParams {
  target: RecordsTarget;
  csv: string;
  key: string[];
  mode: 'update' | 'replace';
}

/** What a CSV matches and would change, before anything is written. */
export interface RecordsReport {
  columns: string[];
  rows: number;
  matched: number;
  targets: number;
  file_only: string[];
  file_only_count: number;
  target_only: number;
  changes: { record: string; column: string; old: string | null; new: string | null }[];
  changed: number;
  refused: string[];
}

export const recordsPreview = (params: UpdateRecordsParams) => invoke<RecordsReport>('records_preview', { params });

/** Any table as a grid, grouped and added up by each column's own rule (Tables and scripts plan, decision 1). */
export const tableGrid = (source: GridSource, ask: GridAsk) => invoke<Grid>('table_grid', { source, ask });

/** Every block of a reserves table's case at one step (Tables and scripts plan, decision 10), as a grid. */
export interface StepGrid {
  /** The step the units are after: `as input` for none. */
  step: string;
  /** The levels above the seam, which make a column. */
  upper: string[];
  /** Block columns run to the step. */
  columns: number;
  /** Columns that stopped before the step, each with why. */
  stopped: string[];
  grid: Grid;
}

/** The units after `step` steps (0, the columns as input) for every block of the table's case, run on what's saved. */
export const reservesStepGrid = (object: string, step: number, ask: GridAsk) => invoke<StepGrid>('reserves_step_grid', { object, step, ask });

/** A node of a reserves table's tree of levels (decision 0075): its level values, label, leaves under it and children. */
export interface ReservesNode {
  path: string[];
  label: string;
  leaves: number;
  children: ReservesNode[];
}

/** One column of a node's breakdown: the node's total (depth 0), a child (1), or an opened child's child (2, …). */
export interface BreakdownColumn {
  label: string;
  path: string[];
  depth: number;
  leaves: number;
  opens: boolean;
  open: boolean;
}

/** One field of a breakdown, a value per column added up by its rule; a field of words has words instead. */
export interface BreakdownField {
  column: string;
  folders: string[];
  name: string;
  unit: string;
  values: (number | null)[];
  words: (string | null)[] | null;
}

export interface Breakdown {
  levels: string[];
  node: string[];
  columns: BreakdownColumn[];
  fields: BreakdownField[];
  /** Leaves past the first 500 not shown, broken down to its leaves. */
  hidden: number;
}

export const reservesTree = (object: string, when: Record<string, string> = {}) => invoke<ReservesNode>('reserves_tree', { object, when });
/** A reserves table's leaves as attributes, a leaf a row: what the filter box counts to tick (decision 0088). */
export const reservesFilterRows = (object: string) => invoke<Record<string, boolean | number | string>[]>('reserves_filter_rows', { object });
/**
 * A reserves table's node with its fields down the side and across its children, each in `open` opened; or, with
 * `leaves`, every leaf under it in stack order with no subtotals.
 */
export const reservesBreakdown = (object: string, node: string[], open: string[][], leaves = false, when: Record<string, string> = {}) => invoke<Breakdown>('reserves_breakdown', { object, node, open, leaves, when });
/** The solids a reserves table's node was made from, to select. */
/** A node's solids (decisions 0075, 0090): those on the table's leaves under it, and with `when` on the leaves it keeps. */
export const reservesSolids = (object: string, node: string[], when?: Record<string, string>) => invoke<string[]>('reserves_solids', { object, node, when });
/** A reserves field's value for each block solid of the table's case, its leaf's or its block's (decision 0075, point 7). */
export const reservesFieldValues = (object: string, column: string, byBlock: boolean) => invoke<[string, number][]>('reserves_field_values', { object, column, byBlock });
/** Where a solid sits in a reserves table, or null if it isn't in it. */
export const reservesNodeOf = (object: string, solid: string) => invoke<string[] | null>('reserves_node_of', { object, solid });

/** A unit of a block's column after a step (decision 0075, 5b): its id, kept through the steps, its labels and values. */
export interface ColumnUnit {
  id: number;
  horizon: string;
  material: string;
  seam: string;
  bench: string | null;
  /** For a joined unit, its benches and their shares. */
  joined: [string, number][];
  became: string | null;
  section: string | null;
  rom: string | null;
  /** A value a field; null where it has none. */
  values: Record<string, number | null>;
}

/** A parameter a built-in step read for a unit (decision 0081): what it found, and the table (its place in `tables`) and row. */
export interface ParamRead {
  unit: number;
  column: string;
  value: number | null;
  text: string | null;
  table: number | null;
  row: number | null;
}

/**
 * The column after one step, and what the step said of it; with, for the notebook (decision 0081), what the step is: its
 * rule in words (built in) or its filter and script (a formula), the units it took, and the parameters it read.
 */
export interface ColumnStep {
  step: string;
  units: ColumnUnit[];
  said: string[];
  kind: 'input' | 'built_in' | 'formula';
  rule: string;
  when: Record<string, string>;
  source: string | null;
  fits: number[];
  reads: ParamRead[];
  /** A built-in step's arithmetic, unit by unit (decision 0082); none for a formula or the column as input. */
  workings: Working[];
  /** What the step wrote to the reserves table for this column, a folder each (decision 0084); the column as input writes Imported. */
  writes: StepWrite[];
  /** The fields of the units the step reads (decision 0084): a built-in step's as declared, a formula's off its source. */
  fields_read: string[];
}

/** What a step wrote to one folder of the reserves table (decision 0084): the units its pieces came from, or went onto, and the volume. */
export interface StepWrite {
  folder: string;
  units: number[];
  volume: number;
}

/**
 * A line of a built-in step's arithmetic for a unit (decision 0082): what was worked out or tested, the numbers that went
 * in, what it came to (none for a test) and what followed.
 */
export interface Working {
  unit: number;
  what: string;
  sum: string;
  value: number | null;
  measure: string;
  then: string | null;
}

/** A field of a block's column, in the dataflow's order, with its unit and kind. */
export interface ColumnField {
  name: string;
  unit: string;
  kind: string;
}

/** A reserves table's block column run again step by step, with what has changed since the table was made. */
export interface ColumnSteps {
  column: string[];
  case: string;
  dataflow: string;
  /** The saved dataflow's id: where its formulas are edited. */
  dataflow_id: string;
  fields: ColumnField[];
  /** The parameter tables read, in order, by name: what a read's `table` counts in. */
  tables: string[];
  /** Each of those tables' ids, where it's an object of the project: where a setting read is edited (decision 0082, point 7). */
  table_ids: (string | null)[];
  /** The column's block solids by name, top down: what the column as input is made of (decision 0084). */
  blocks: string[];
  /** The seam model whose stack orders the column; null where the run gave its own stack. */
  model: string | null;
  /** How many horizons the stack holds. */
  horizons: number;
  /** Run on anything given as edited, not as saved. */
  edited: boolean;
  /** What it was run on as edited, by name: the dataflow and the parameter tables it reads (decision 0082). */
  unsaved: string[];
  /** The edited dataflow's worked examples that don't hold, each with its step and test: run anyway (decision 0081). */
  failing: string[];
  steps: ColumnStep[];
  changed: string[];
}

/**
 * Open its column: the block column a reserves table's node sits in, run again through its dataflow, every step; with
 * `dataflow`, through that dataflow as edited, not the one saved (decision 0081).
 */
export const reservesColumn = (object: string, node: string[], dataflow: Dataflow | null = null, tables: [string, ParameterTable][] = []) =>
  invoke<ColumnSteps>('reserves_column', { object, node, dataflow, tables: tables.length ? tables : null });
/** A notebook cell's formula traced on one unit (by id), the column run up to it: `step` 1 is the first after the column as input. */
/**
 * The settings to run a reserves table's blocks under `only` again into it (decision 0085): its own run's, read from the
 * log; [] every block, a column's path its block, the path less its last word its strip. Run them with `reserves.run`.
 */
export const reservesRerunParams = (object: string, only: string[]) => invoke<Record<string, unknown>>('reserves_rerun_params', { object, only });
export const reservesColumnTrace = (object: string, node: string[], step: number, unit: number, dataflow: Dataflow | null = null, tables: [string, ParameterTable][] = []) =>
  invoke<FormulaTrace>('reserves_column_trace', { object, node, step, unit, dataflow, tables: tables.length ? tables : null });

/** One folder a seam model reads its grids from, with how many of them are there (decision 0058, stage 4). */
export interface FolderCheck {
  folder: string;
  files: number;
  missing: number;
}

/** A seam model's grid files checked where they are now. */
export interface ModelLinks {
  object: string;
  name: string;
  folders: FolderCheck[];
  /** Files whose content differs from when the model was built. */
  changed: string[];
}

export interface LinksReply {
  /** What opening did: models relinked to grid folders found beside the project, and folders found neither way. */
  notes: string[];
  models: ModelLinks[];
}

/** Every seam model's grids checked where they are now. */
export const checkLinks = () => invoke<LinksReply>('check_links');
/** Saves to `path` (a `.zdp` file), or to the file last opened or saved. */
export const saveProject = (path?: string) => invoke<SaveReply>('save_project', { path: path ?? null });
export const setLayerVisible = async (layer: string, visible: boolean) =>
  (idle(), summaryAfter(await invoke<ProjectChanges>('set_layer_visible', { layer, visible })));
export const setObjectsVisible = async (objects: string[], visible: boolean) =>
  (idle(), summaryAfter(await invoke<ProjectChanges>('set_objects_visible', { objects, visible })));
/** A layer's filter (decision 0088): attribute or measure name to the list it may hold; {} takes it away. Display state. */
export const setLayerFilter = async (layer: string, when: Record<string, string>) =>
  (idle(), summaryAfter(await invoke<ProjectChanges>('set_layer_filter', { layer, when })));
/** A reserves table's leaves filter in the view (decision 0090); null takes it away. Display state. */
export const setReservesFilter = async (filter: ReservesFilter | null) =>
  (idle(), summaryAfter(await invoke<ProjectChanges>('set_reserves_filter', { filter })));
/** Every attribute of what's on a layer and the layers under it, an object a row: what the filter box counts. */
export const layerAttributes = (layer: string) => invoke<Record<string, boolean | number | string>[]>('layer_attributes', { layer });
/** What one input of a workflow step finds before its filter, as the case sees it, an object a row with its measures. */
export const workflowInputRows = (workflow: string, step: number, input: string, caseName?: string) =>
  invoke<Record<string, boolean | number | string>[]>('workflow_input_rows', { workflow, step, input, case: caseName });
/** Colours objects by RL or their layer, sets their opacity, or both (null leaves it). Display state, not a run. */
export const setLooks = async (objects: string[], colouring: Colouring | null, opacity: number | null) =>
  (idle(), invoke<Display>('set_looks', { objects, colouring, opacity }));
export const setLayerOpacity = async (layer: string, opacity: number) => (idle(), invoke<Display>('set_layer_opacity', { layer, opacity }));
export const setContours = async (contours: Contours) => (idle(), invoke<Display>('set_contours', { contours }));

/** A layer of a Deswik drawing, named as Deswik shows it (levels joined by `\`), and its size unzipped. */
export interface DeswikLayer {
  name: string;
  bytes: number;
}

/** The layers of a Deswik `.dcf`, read from its zip's directory alone (decision 0036). */
export const deswikLayers = (path: string) => invoke<DeswikLayer[]>('deswik_layers', { path });

/** What a Spry project holds to import, listed without unpacking any geometry (decision 0039). */
export interface SpryContents {
  databaseVersion: number;
  /** A table's Solid fields, named by table, folders and field joined by `\`, with how many nodes have a solid. */
  solidFields: { name: string; solids: number; bytes: number }[];
  /** Design layers, folders and layer joined by `\`, with their size as stored. */
  layers: { name: string; bytes: number }[];
}

export const spryContents = (path: string) => invoke<SpryContents>('spry_contents', { path });

/** The session's run log, oldest first: every run (failed ones too), undo, redo, save and open. */
export type LogEntry =
  | ({ event: 'run' } & RunRecord)
  | { event: 'undo' | 'redo'; run: string; at_unix_ms: number }
  | { event: 'saved' | 'opened'; path: string; at_unix_ms: number }
  | ({ event: 'workflow' } & WorkflowRecord);

/** One step of a workflow run (decision 0065). */
export interface StepRun {
  step: number;
  /** Where it is in the order, from 1. */
  position: number;
  command?: string;
  outcome: 'ran' | 'off' | 'paused' | 'refused';
  /** Its command's run in the log. */
  run?: string;
  refused?: string;
  /** What the check before a cut found. */
  checked?: Note[];
  ms: number;
}

/** A workflow run, in the run log after its steps' own runs. */
export type WorkflowRecord = {
  run: string;
  workflow: string;
  name: string;
  /** Base, or one of its cases. */
  case: string;
  from: number;
  steps: StepRun[];
  started_unix_ms: number;
  duration_ms: number;
} & ({ status: 'finished' } | { status: 'paused' | 'refused'; at: number });

const log = new LogStore();

/** The whole run log, fetching only the entries the app hasn't got. */
export const runLog = async () => log.take(await invoke<LogPart>('run_log', { session: log.session, from: log.entries.length }));

/** Validity flags in Seequent Evo vocabulary, with the counts behind them. */
export interface MeshValidity {
  closed: boolean;
  manifold: boolean;
  consistent_winding: boolean;
  non_degenerate: boolean;
  /** Null until a crossing check has run. */
  non_self_intersecting: boolean | null;
  boundary_edges: number;
  non_manifold_edges: number;
  inconsistent_edges: number;
  degenerate_triangles: number;
}

/**
 * A crossing check: pairs of triangles that pass through or fold back onto each other. By the solid
 * rules (decision 0014), faces within the tolerance facing opposite ways and apart are thin instead.
 */
export interface Crossings {
  check: { by: 'search' | 'solid_search'; tolerance: number } | { by: 'lattice' };
  pairs: number;
  /** Where the first pairs cross (up to 100), in world coordinates. */
  at: Vec3[];
  /** Thin pairs (solid rules only): knife edges or hairline gaps, kept as they are. */
  thin: number;
  /** Where the first thin pairs are (up to 100). */
  thin_at: Vec3[];
}

/** A grid file a seam model reads in place, with its fingerprint from when the model was built. */
export interface GridLink {
  file: string;
  fingerprint: { bytes: number; modified_unix_ms: number; checksum: number };
}

export interface Horizon {
  code: string;
  parent: string | null;
  material: string;
  /** Stacked, in order, for solids and reserves. */
  working: boolean;
  roof: GridLink;
  floor: GridLink;
  /** Quality, density and thickness grids by variable code. */
  grids: Record<string, GridLink>;
}

export type IssueKind = 'floor_above_roof' | 'crossing' | 'outside_parent' | 'unpaired' | 'no_density' | 'holes';

/** A problem found when the stack was checked; the grids are never changed to hide it. */
export interface StackIssue {
  kind: IssueKind;
  horizon: string;
  /** The horizon above for a crossing, the parent for outside_parent. */
  other: string | null;
  nodes: number;
  /** Largest overlap in metres; 0 where the issue is not a distance. */
  worst: number;
  /** Easting and northing of the worst node. */
  at: [number, number] | null;
  blocking: boolean;
}

export type WallSide = 'left' | 'right';

export type StepTarget = { kind: 'surface'; id: string } | { kind: 'rl'; rl: number };

/** A step's batter or width: a number typed on the row, or a rule's name. */
export type StepValue = number | string;

/** `condition` is how far the line may move in plan when it's cleaned (m); null leaves it as it is. */
export type ProfileStep =
  /** `ramps` names the ramps that climb this wall, lowest first. `turns` names those of them that turn back before it, on
   * a flat pad across the berm below: each is on the wall before too. */
  | { step: 'project'; to: StepTarget; batter: StepValue; condition: number | null; ramps?: string[]; turns?: string[] }
  | { step: 'berm'; on: StepTarget | null; width: StepValue; register: boolean; condition: number | null; min_bench?: number | null; /** On, off, or left out for the berm to join only boxes within a road's width (0047). */ join?: boolean };

/** The way a road runs along its string. */
export type RampHeading = 'along' | 'back';

/** Which end of a road is where its string crosses. With its end there, the run moves its start along the toe until it comes out there.
 * `line`: it has no string, and carries on the road of its name from where the design below left it on each line. */
export type RampFixed = 'start' | 'end' | 'line';

/** How a road gets past a berm between two walls it climbs: after it, crossing the full berm level and straight across
 * to start up the next wall from its toe, or through it at its grade, with the berm narrowing to nothing at the road. */
export type RampBerm = 'after' | 'through';

/** A road up the walls that name it, on every strip its string crosses. */
export interface Ramp {
  /** What a wall row calls it: a letter, usually. */
  name: string;
  /** A string drawn across the strips. Each line it crosses gets a road. A ramp from the line has none. */
  string?: string | null;
  fixed: RampFixed;
  heading: RampHeading;
  /** Square to the road, in metres. */
  width: number;
  /** Rise over plan run on the inside edge, the pit side, in percent. */
  grade: number;
  /** After the berm if it's left out, as designs saved before it could be set. */
  berm?: RampBerm;
  /** Where it turns back with no box past its top (a flat of 0), how far on from the turn, along the road, the toe of the
   * wall above joins back to the berm's line, in metres. 50 m if it's left out, as designs saved before it could be set. */
  turn_join?: number;
  /** Where it turns back, how far the flat box it turns on runs past its top, in line with it and on the floor on a grid,
   * in metres: the road, the berm and the next road wide. The next road climbs the other way from its near end, straight
   * across the berm from the top, and the wall above goes round the box. Its width if it's left out, as designs saved
   * before it could be set; 0 turns at the top with no box. */
  turn_flat?: number | null;
  /** Where it turns back on a box, the angle in plan between the box's far end and the strip, in degrees, 30 to 90: 90,
   * square across, if it's left out, as designs saved before it could be set. Less flares the far end out past the box,
   * from its back corner to the berm's line past it. */
  turn_end?: number | null;
}

/** One step of the manual builder, as `face.build_step` takes it (decision 0048): a wall from the line, with the roads on
 * it climbing it but for those in `stop`, and new ones in `ramps`; or a berm, with the roads on the line crossing it,
 * turning on it (`turns`) or ending (`stop`), and `join` to make their boxes one pad: on, off, or left out for the berm to
 * join only boxes within a road's width of each other. */
export type BuildOp =
  | { op: 'project'; to: StepTarget; batter: number; condition?: number | null; ramps?: Ramp[]; stop?: string[] }
  | { op: 'berm'; width: number; on?: StepTarget | null; condition?: number | null; turns?: string[]; join?: boolean | null; stop?: string[] };

/** How a step built a line, as the line keeps it in its `build_record` attribute. */
export interface BuildRecord {
  /** The line it was built from, what that lies on, and that line's revision then. */
  from: string;
  from_on: StepTarget;
  from_revision?: number;
  /** The revision the line it made had then (1 if it doesn't say): a later one means it's been edited by hand since. */
  made_revision?: number;
  side: WallSide;
  step: BuildOp;
  run_on?: [number, number];
  condition?: number | null;
}

export type FaceDirection = 'up' | 'down';

export interface FaceProfile {
  name: string;
  direction: FaceDirection;
  /** What a toe registers to: a surface or grid it follows, or an RL it lies flat on. */
  register: StepTarget;
  condition: number | null;
  steps: ProfileStep[];
  ramps?: Ramp[];
}

export interface FaceLine {
  /** One string to run on; or, in its place, `layer`. */
  string?: string;
  /** Every string on this layer (its id) as it stands at each run. */
  layer?: string;
  profile: string;
  side: WallSide;
  /** Past the string's start and end, in metres. */
  run_on: [number, number];
  /** Lines of a step design redrawn by hand (decision 0046), one to a row. */
  edits?: LineEdit[];
}

/** A line redrawn by hand: the line row `row` (from 1) ends on, a wall's crest or a berm's line, is this string. The row
 * re-forms to meet it and the rows above carry on from it. */
export interface LineEdit {
  row: number;
  string: string;
}

/** Azimuths clockwise from `from` to `to`, in degrees from grid north, and their value. */
export interface AzimuthSector {
  from: number;
  to: number;
  value: number;
}

export interface HorizonValue {
  surface: string;
  value: number;
}

/** How a rule gives its value: one value for each edge of the line a step works from. */
export type RuleValue =
  | { type: 'fixed'; value: number }
  | { type: 'fixed_list'; values: number[] }
  | { type: 'inside_polygon'; polygon: string; inside: number; outside: number }
  | { type: 'inside_solid'; solid: string; inside: number; outside: number }
  | { type: 'segment_azimuth'; sectors: AzimuthSector[]; otherwise: number }
  | { type: 'by_horizon'; values: HorizonValue[]; otherwise: number };

export interface WallRule {
  name: string;
  value: RuleValue;
}

export interface FaceDesign {
  layer: string;
  lines: FaceLine[];
  profiles: FaceProfile[];
  rules: WallRule[];
}

export interface SeamModel {
  folder: string;
  density: string;
  tolerance: number;
  lattice: { origin: [number, number]; cell_size: [number, number]; cols: number; rows: number };
  /** An RL that means the seam is absent. */
  no_seam: number | null;
  /** Top of the stack first. */
  horizons: Horizon[];
  issues: StackIssue[];
}

/** What a solid is made of: closed shells wound outward (pieces) and inward (cavities), volumes in m³. */
export interface Pieces {
  pieces: number;
  cavities: number;
  smallest_piece: number;
  largest_piece: number;
  cavity_volume: number;
}

export type Measures =
  | { kind: 'string'; points: Vec3[]; closed: boolean; length: number; plan_length: number; plan_area: number | null }
  | { kind: 'surface'; vertices: number; triangles: number; area: number; validity: MeshValidity; crossings: Crossings | null }
  | { kind: 'solid'; vertices: number; triangles: number; area: number; volume: number; pieces: Pieces; validity: MeshValidity; crossings: Crossings | null }
  | {
      kind: 'grid';
      origin: [number, number];
      cell_size: [number, number];
      cols: number;
      rows: number;
      nulls: number;
      z_range: [number, number] | null;
    }
  | { kind: 'seam_model'; model: SeamModel }
  | { kind: 'face_design'; design: FaceDesign }
  | { kind: 'workflow'; workflow: Workflow }
  | { kind: 'parameter_table'; parameters: ParameterTable; csv: string }
  | { kind: 'dataflow'; dataflow: Dataflow }
  | { kind: 'table'; rows: number; columns: { name: string; unit?: string; text?: boolean; add_up?: AddUp }[]; figures: { writes: WriteFigures[] } };

/** How a field adds up over units or rows (decision 0070, point 4). */
export type AddUp = { by: 'sum' | 'volume' | 'max' | 'min' } | { by: 'mass'; density: string } | { by: 'ratio'; of: string; per: string };

/** A reserves dataflow (decisions 0070, 0072): its fields, where each value comes from, its steps and its tables. */
export interface Dataflow {
  /** The reserves table's levels top down (decision 0074): those above `seam` make a column. */
  levels?: { attribute: string; default?: string }[];
  fields: { name: string; kind: string; add_up: AddUp; from: string; attribute?: string; column?: string }[];
  steps: DataflowStep[];
  parameters: string[];
}

/** A value a formula's worked example gives or expects. */
export type ParamCell = number | string;

/** A formula's worked example (decision 0077): a unit's fields and labels, its block and parameters, the ROM section's
 * coal where it's in one, and the fields the formula must leave it with. */
export interface FormulaTest {
  name: string;
  unit?: Record<string, ParamCell>;
  block?: Record<string, ParamCell>;
  parameters?: Record<string, ParamCell>;
  rom_coal?: Record<string, ParamCell>;
  /** The fields it must leave the unit with; `null`, with no value. */
  expect?: Record<string, number | null>;
  /** Words it must stop the block with. */
  stops?: string;
}

/** A dataflow step: a built-in by name, or a formula step with its filter, Starlark and worked examples (0077). */
export interface DataflowStep {
  step: string;
  on?: boolean;
  settings?: Record<string, string>;
  formula?: { when?: Record<string, string>; script: string; tests?: FormulaTest[] };
}

/** A formula as edited, checked (decision 0077, 7d). */
export interface FormulaReport {
  stage: string | null;
  problems: { message: string; line: number | null }[];
  tests: { name: string; ok: boolean; why: string | null }[];
  names: { fields: [string, string][]; labels: string[]; parameters: [string, string, boolean][] };
}

/** A formula traced on a picked solid's unit (decision 0077, 7d). */
export interface FormulaTrace {
  block: string;
  unit: string;
  fits: boolean;
  before: [string, number | null][];
  lines: { line: number; text: string; values: [string, string][] }[];
  reads: [string, string][];
  sets: [string, string][];
  said: string[];
  stopped: { message: string; line: number | null } | null;
}

/** Rows of a results table added up by each column's rule. */
export interface ResRow {
  label: string;
  material: string;
  rows: number;
  values: (number | null)[];
}

export interface WriteFigures {
  /** The top folder: Imported, Insitu, Loss. */
  write: string;
  columns: [string, string][];
  /** A row a seam and the folder below the top (material or kind of loss), top down. */
  seams: ResRow[];
  totals: ResRow[];
  benches: ResRow[];
}

/** Assumptions any tool reads, rows read top down, the first that fits and holds a value giving it (decision 0072). */
export interface ParameterTable {
  filters: string[];
  columns: { name: string; unit?: string; text?: boolean; note?: string }[];
  rows: { when?: Record<string, string>; values: Record<string, number | string> }[];
}

/** Where a picked solid takes its values from (decision 0075, 5d): each table's rows it fits, each column's table and row. */
export interface Picked {
  fits: number[][];
  gives: Record<string, { table: number; row: number }>;
}

/** The tables read with a parameter table, in order (its dataflow's, or it alone), and where a picked solid's values come from. */
export interface ParametersPicked {
  tables: [string, string][];
  dataflow: string | null;
  picked: Picked | null;
}

export const parametersPicked = (table: string, solid: string | null) => invoke<ParametersPicked>('parameters_picked', { table, solid });
/** The engine's check of a parameter table as edited, before it's saved: resolves, or rejects with why. */
export const parametersCheck = (content: ParameterTable) => invoke<null>('parameters_check', { content });
/** The engine's check of a dataflow as edited, before it's saved. */
export const dataflowCheck = (content: Dataflow) => invoke<null>('dataflow_check', { content });
/** A formula step of a dataflow as edited, checked: where it sits, every problem with its line, its worked examples. */
export const formulaCheck = (content: Dataflow, index: number) => invoke<FormulaReport>('formula_check', { content, index });
/** A formula step of a dataflow as edited, traced on a picked solid's unit; nothing kept. */
export const formulaTrace = (content: Dataflow, step: string, solid: string) => invoke<FormulaTrace>('formula_trace', { content, step, solid });
/** How to write a formula: the panel's help and what Copy for AI hands over. */
export const formulaReference = () => invoke<string>('formula_reference');

/** Saved steps run again in one go (decision 0065), and its cases (decision 0066). */
export interface Workflow {
  steps: WorkflowStep[];
  cases?: Case[];
}

/** A case: the steps with some settings changed, on its own layers, tagged `case`. */
export interface Case {
  name: string;
  /** What it changes, by step id (as text) and setting name. */
  settings?: Record<string, Record<string, unknown>>;
  /** Each step's last run in this case, by step id (as text). */
  last?: Record<string, LastStepRun>;
}

export interface WorkflowStep {
  id: number;
  note?: string;
  /** Left out when on. */
  on?: boolean;
  action: { kind: 'command'; command: string; settings: Record<string, unknown>; inputs: Record<string, InputRule> } | { kind: 'pause' };
  last?: LastStepRun;
}

/** How a step finds an input each time it runs. */
export interface InputRule {
  from: { step: number } | { layer: string } | { named: string } | { picked: string[] };
  kind?: ObjectKind;
  where?: Record<string, string | number | boolean>;
  many: boolean;
}

export interface LastStepRun {
  run: string;
  workflow_run?: string;
  at_unix_ms: number;
  ms: number;
  made: string[];
  revised?: string[];
  deleted?: number;
  warnings?: number;
  refused?: boolean;
}

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

/** One horizon of a seam model, staged as its roof coloured by thickness. */
export interface StagedHorizon {
  staged: StagedMesh;
  horizon: string;
  /** The thickest coal on the horizon (m): the top of the colour ramp. */
  thickness_max: number;
  coal_nodes: number;
  nodes: number;
  /** Nodes marked for an issue beyond the model's tolerance. */
  flagged_nodes: number;
  /** The worst node of each blocking issue on this horizon, scene-local; `issue` indexes the model's issues. */
  markers: { issue: number; position: Vec3 }[];
  /** Grid files changed since the model was built. */
  changed: string[];
  /** Every horizon, top first, to choose from, with the grids it can be coloured by. */
  horizons: { code: string; material: string; working: boolean; variables: string[] }[];
  /** The grid the horizon is coloured by, by its code; null for its thickness. */
  variable: string | null;
  /** The values the colours span. */
  value_range: [number, number];
}

/**
 * Reads a seam model's grids in place and stages one horizon (the first working one if none is named), coloured by its
 * thickness or by one of its grids (decision 0061, workstream 6).
 */
export const stageSeamHorizon = (id: string, horizon: string | null, variable: string | null = null) =>
  invoke<StagedHorizon>('stage_seam_horizon', { id, horizon, variable });

/** Each block of one seam of a model with its value of an attribute: `[id, value]`. */
export const blockValues = (model: string, horizon: string, seamAttribute: string, attribute: string) =>
  invoke<[string, number][]>('block_values', { model, horizon, seamAttribute, attribute });

export interface SnapIndex {
  ms: number;
  surfaces: number;
  triangles: number;
}

/** The last snap index of what's on show, for the benches: how long a project just drawn couldn't snap. */
export let lastSnapIndex: SnapIndex | null = null;
/** With `shown`, only what's on show; else that first, then what's hidden. */
export const buildSnapIndex = (shown = false) =>
  invoke<SnapIndex>('build_snap_index', { shown }).then((index) => {
    if (shown) lastSnapIndex = index;
    return index;
  });

export interface SnapHit {
  /** Offset from the scene origin, for drawing. */
  local: Vec3;
  /** Mine grid coordinates, for commands. */
  world: Vec3;
  /** The surface or grid hit; null for a solid, which is searched with the rest of its layer. */
  object: string | null;
  /**
   * What was snapped to: a string's vertex, segment or crossing, a surface (the default), or the level
   * working plane when nothing is under the cursor.
   */
  kind?: 'vertex' | 'segment' | 'intersection' | 'surface' | 'plane';
  /** For the status line, e.g. "vertex 3 of toe". */
  label?: string;
}

export const snapRay = (origin: Vec3, direction: Vec3) => invoke<SnapHit | null>('snap_ray', { origin, direction });
/**
 * Over empty space, the nearest thing to the cursor (decision 0044): rings of rays from one origin, nearest the cursor
 * first. The first ring with a hit gives its number and its hit nearest the camera.
 */
export const snapRays = (origin: Vec3, rings: Vec3[][]) => invoke<[number, SnapHit] | null>('snap_rays', { origin, rings });

/** Where a cursor at a distance along the section and an RL snaps to the last cut, if anywhere near. */
export const sectionPick = (along: number, rl: number, reach: number) => invoke<string | null>('section_pick', { along, rl, reach });
export const sectionSnap = (along: number, rl: number, vertexReach: number, segmentReach: number, ignoring: [number, number] | null) =>
  invoke<SnapHit | null>('section_snap', { along, rl, vertexReach, segmentReach, ignoring });

export interface BenchConfig {
  surfaces: string[];
  /** A zip or folder of solids: time this import and nothing else. */
  solids: string | null;
  /** Strings to load into the same scene first. */
  strings: string[];
  seam: string | null;
  /** Grids to step a wall up on the section, bottom first. */
  walls: string[];
  results_path: string;
  scale_targets: number[];
  transfer_cells: number[];
  orbit_seconds: number;
  drag_updates: number;
  /** With `--opacity`, every surface is drawn this see-through, to time stacked see-through surfaces. */
  opacity: number | null;
  /** With `--pictures <folder>`, the surfaces are drawn in their looks and saved there as pictures. */
  pictures: string | null;
  /** With `--webgl`, the viewport runs on three's WebGL2 backend, as it does without WebGPU. */
  webgl: boolean;
  /** With `--pictures` and `--palettes`, a few views are drawn on every RL ramp, and on one sheet. */
  palettes: boolean;
  /** With `--design-lines`, a step design's own line and a copy of it on top: what a click picks, and Delete on each. */
  design_lines: boolean;
  /** With `--select`, selecting several and zooming to them. */
  select: boolean;
  /** With `--shell`, panels minimised and brought back, and the toolbar's groups. */
  shell: boolean;
  /** With `--workflow`, the Run log's Capture and the Workflow panel driven as Tim would (decision 0065). */
  workflow: boolean;
  /** A project with a parameter table, a dataflow and a results table, for Properties' views of them (decision 0072). */
  dataflow: string | null;
  /** With `--builder`, the Builder panel driven as Tim would, from a toe up, with an edit between steps (decision 0048). */
  builder: boolean;
  /** With `--project`, Project on the Strings tab driven as Tim would, through the panel and clicks in the view (decision 0079). */
  project: boolean;
  /** With `--measure`, the distance measure on the view bar driven as Tim would, by its key, its button and clicks (decision 0079). */
  measure: boolean;
  /** With `--clicks`, how the view takes clicks and Esc driven as Tim would: NS, right-click to finish, Esc a step a press (decision 0080). */
  clicks: boolean;
  /** With `--string-edits`, Trim, Extend, Split at a click and Join driven as Tim would, by picks and clicks (String tools plan, workstream 1). */
  string_edits: boolean;
  /** With `--ramp`, a ramp made by hand from the Walls row: Set grade, Offset, Project and a surface between the strings (decision 0079). */
  ramp: boolean;
  /** With `--ramp-start`, Ramp from a start driven as Tim would: placed, moved, undone, and to a surface (decision 0079). */
  ramp_start: boolean;
  /** With `--cut`, the Cut solids panel driven as Tim would: a pit between two surfaces, sliced into strips, cut at an RL (decision 0049). */
  cut: boolean;
  /** With `--section-guide`, the dashed line from a section's first point to the cursor, on a made-up topography. */
  section_guide: boolean;
  /** With `--lod <project>`, a big project opened and orbited at Full and at 1 px (decision 0097). */
  lod: string | null;
}

/** Saves a PNG data URL of the viewport as `<name>.png` in the `--pictures` folder. Bench only. */
export const writePicture = (name: string, dataUrl: string) => invoke<string>('write_picture', { name, dataUrl });

export const benchConfig = () => invoke<BenchConfig | null>('bench_config');

export const writeResults = (text: string) => invoke<string>('write_results', { text });

export const exitApp = () => invoke<void>('exit_app');

/** Whether closing, or starting or opening another project, would lose work: runs since the last save or open, or a run going. */
export const projectUnsaved = () => invoke<boolean>('project_unsaved');

/** Where this copy's licence stands (decision 0098). */
export const licenceStatus = () => invoke<LicenceStatus>('licence_status');

/** Puts a licence file on this machine and applies it; refused with the reason if it isn't one this version trusts. */
export const loadLicence = (path: string) => invoke<LicenceStatus>('load_licence', { path });

/** Every command's module by name. */
export const commandModules = () => invoke<Record<string, string>>('command_modules');
