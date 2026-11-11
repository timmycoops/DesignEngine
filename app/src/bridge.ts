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
  /** A second number per node to colour by (seam thickness); `flag` marks a node with an issue. */
  value?: { name: string; min: number; max: number; step: number; null: number; flag: number };
  tiles?: TileMeta[];
}

/** One draw call's worth of geometry: f32 positions in scene-local coordinates, u16 indices. */
export interface ChunkGeometry {
  positions: Float32Array;
  indices: Uint16Array;
  min: Vec3;
  max: Vec3;
  /** Quantised values per vertex, when the buffer carries them (see `MeshHeader.value`). */
  values?: Uint16Array;
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
export const stageLayerSolids = (layer: string) => invoke<StagedMesh>('stage_layer_solids', { layer });
/** The selected solids on show as one mesh, to light up over their layers'. */
export const stageSolids = (objects: string[]) => invoke<StagedMesh>('stage_solids', { objects });
/** The box round these objects together, world coordinates, for zoom to selection; null if none has a place. */
export const objectsBounds = (objects: string[]) => invoke<[Vec3, Vec3] | null>('objects_bounds', { objects });
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

export type ObjectKind = 'string' | 'surface' | 'solid' | 'grid' | 'seam_model' | 'face_design';

export interface ObjectSummary {
  id: string;
  revision: number;
  name: string;
  /** The layer's id (see `Layer.id`), not its name. */
  layer: string;
  kind: ObjectKind;
  attributes: Record<string, boolean | number | string>;
  /** Drawn as a surface: surfaces, solids and elevation grids. */
  drawn: boolean;
  /** Hidden on its own: not drawn, snapped to or cut by a section. */
  hidden: boolean;
  /** Changes only when the geometry does. */
  geometry_key: string;
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
  /** The layers holding solids, each drawn as one mesh. */
  solid_layers: SolidLayer[];
  /** How objects and layers look. */
  display: Display;
  /** Who else had the project file open when it was opened: a save to it is refused. */
  locked_by: string | null;
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
}

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
export interface SolidLayer {
  layer: string;
  /** Changes only when the geometry of a solid on the layer does. */
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

export const projectSummary = () => invoke<ProjectSummary>('project_summary');

/** Runs an engine command by name; rejects with the error text if the command fails. */
export const runCommand = (name: string, params: Record<string, unknown>) => invoke<RunReply>('run_command', { name, params });

export const undo = () => invoke<ProjectSummary>('undo');
export const redo = () => invoke<ProjectSummary>('redo');
export const newProject = () => invoke<ProjectSummary>('new_project');
export const openProject = (path: string) => invoke<ProjectSummary>('open_project', { path });

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
export const setLayerVisible = (layer: string, visible: boolean) =>
  invoke<ProjectSummary>('set_layer_visible', { layer, visible });
export const setObjectsVisible = (objects: string[], visible: boolean) =>
  invoke<ProjectSummary>('set_objects_visible', { objects, visible });
/** Colours objects by RL or their layer, sets their opacity, or both (null leaves it). Display state, not a run. */
export const setLooks = (objects: string[], colouring: Colouring | null, opacity: number | null) =>
  invoke<Display>('set_looks', { objects, colouring, opacity });
export const setLayerOpacity = (layer: string, opacity: number) => invoke<Display>('set_layer_opacity', { layer, opacity });
export const setContours = (contours: Contours) => invoke<Display>('set_contours', { contours });

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
  | { event: 'saved' | 'opened'; path: string; at_unix_ms: number };

export const runLog = () => invoke<LogEntry[]>('run_log');

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

export type IssueKind = 'floor_above_roof' | 'crossing' | 'outside_parent' | 'unpaired' | 'no_density';

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
  | { kind: 'face_design'; design: FaceDesign };

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
  /** Every horizon, top first, to choose from. */
  horizons: { code: string; material: string; working: boolean }[];
}

/** Reads a seam model's grids in place and stages one horizon (the first working one if none is named). */
export const stageSeamHorizon = (id: string, horizon: string | null) => invoke<StagedHorizon>('stage_seam_horizon', { id, horizon });

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
  /** With `--builder`, the Builder panel driven as Tim would, from a toe up, with an edit between steps (decision 0048). */
  builder: boolean;
  /** With `--cut`, the Cut solids panel driven as Tim would: a pit between two surfaces, sliced into strips, cut at an RL (decision 0049). */
  cut: boolean;
}

/** Saves a PNG data URL of the viewport as `<name>.png` in the `--pictures` folder. Bench only. */
export const writePicture = (name: string, dataUrl: string) => invoke<string>('write_picture', { name, dataUrl });

export const benchConfig = () => invoke<BenchConfig | null>('bench_config');

export const writeResults = (text: string) => invoke<string>('write_results', { text });

export const exitApp = () => invoke<void>('exit_app');
