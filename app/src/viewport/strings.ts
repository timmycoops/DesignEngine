// Strings in the viewport, and editing them through engine commands.
//
// Draw mode: each click adds a vertex; Enter or a double-click finishes the string
// (`string.create`), Backspace drops the last vertex, Escape abandons it. Select mode: click a
// string to select it; drag one of its handles to move that vertex (`string.move_vertex`, one
// command per drag, with a snapped preview while dragging); Alt+click on the selected string inserts
// a vertex on it (`string.insert_vertex`); Delete removes the vertex under the cursor
// (`string.delete_vertex`). Every edit is a command, so undo covers it.
//
// Snapping, in order: a string vertex near the cursor (its exact coordinates), the crossing of two
// strings, the nearest point on a string segment, the surface under the cursor, and failing all of
// those a level working plane: at the dragged vertex's RL while dragging, the last vertex's while
// drawing, else the RL at the centre of the view.
import * as THREE from 'three/webgpu';
import type { Layer, ProjectSummary, SnapHit, Vec3 } from '../bridge';
import { colourOf } from './load';
import type { Viewport } from './Viewport';

export type Mode = 'select' | 'draw';

export interface StringActions {
  /** Runs an engine command; resolves true if it succeeded. */
  run(name: string, params: Record<string, unknown>): Promise<boolean>;
  snap(clientX: number, clientY: number): Promise<SnapHit | null>;
  /** The surface straight below a scene-local plan position, if any. */
  snapDown(x: number, y: number): Promise<SnapHit | null>;
  status(text: string): void;
  /** Layer that new strings go on. */
  layer(): string;
}

interface Shown {
  line: THREE.Line;
  points: THREE.Vector3[];
  closed: boolean;
  revision: number;
  name: string;
}

const PICK_PX = 8;
const HANDLE_PX = 10;
/** Handles closer than this on screen to the last one drawn are skipped (they can still be grabbed). */
const HANDLE_SPACING_PX = 12;
/** Segments this close to the cursor are tested for crossings. */
const CROSSING_REACH_PX = 40;
const CLICK_SLOP_PX = 4;
const SELECTED = 0xffd23f;
const PREVIEW = 0x66d9ff;
const MAX_HANDLES = 8192;

export class StringLayer {
  mode: Mode = 'select';
  selected: string | null = null;
  onSelect?: (id: string | null) => void;

  private shown = new Map<string, Shown>();
  private group = new THREE.Group();
  private preview: THREE.Line;
  private handles: THREE.InstancedMesh;
  private dummy = new THREE.Object3D();
  private pending: SnapHit[] = [];
  private drag: { id: string; index: number; hit: SnapHit | null } | null = null;
  private hovered: { id: string; index: number } | null = null;
  private down: { x: number; y: number } | null = null;
  private latest: { x: number; y: number } | null = null;
  private inFlight = false;
  private layers = new Map<string, Layer>();
  private names = 0;
  /** The scene origin: world = origin + local, exactly, for points the shell sent. */
  private origin: Vec3 = [0, 0, 0];

  constructor(
    private viewport: Viewport,
    private actions: StringActions,
  ) {
    this.preview = new THREE.Line(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: PREVIEW, depthTest: false }));
    this.preview.renderOrder = 12;
    this.preview.frustumCulled = false;
    this.preview.visible = false;
    this.handles = new THREE.InstancedMesh(new THREE.OctahedronGeometry(1), new THREE.MeshBasicMaterial({ color: 0xffffff, depthTest: false }), MAX_HANDLES);
    this.handles.count = 0;
    this.handles.renderOrder = 13;
    this.handles.frustumCulled = false;
    viewport.overlay.add(this.group, this.preview, this.handles);
    viewport.beforeRender.add(() => this.placeHandles());
  }

  setMode(mode: Mode): void {
    this.mode = mode;
    this.pending = [];
    this.refreshPreview();
    this.placeHandles();
  }

  select(id: string | null): void {
    this.selected = id && this.shown.has(id) ? id : null;
    this.recolour();
    this.placeHandles();
    this.onSelect?.(this.selected);
  }

  /** Brings the drawn strings in line with the project summary. */
  sync(summary: ProjectSummary): void {
    this.layers = new Map(summary.layers.map((l) => [l.id, l]));
    this.origin = summary.origin ?? [0, 0, 0];
    const strings = summary.objects.filter((o) => o.kind === 'string' && o.vertices_local);
    this.names = Math.max(this.names, strings.length);
    const live = new Set(strings.map((s) => s.id));
    for (const [id, shown] of this.shown) {
      if (!live.has(id)) {
        this.group.remove(shown.line);
        shown.line.geometry.dispose();
        this.shown.delete(id);
      }
    }
    for (const s of strings) {
      const visible = this.layers.get(s.layer)?.visible ?? true;
      const existing = this.shown.get(s.id);
      if (existing && existing.revision === s.revision) {
        existing.line.visible = visible;
        existing.line.userData.layer = s.layer;
        continue;
      }
      if (existing) {
        this.group.remove(existing.line);
        existing.line.geometry.dispose();
      }
      const points = (s.vertices_local ?? []).map((p) => new THREE.Vector3(...p));
      const ring = s.closed ? [...points, points[0]] : points;
      const line = new THREE.Line(new THREE.BufferGeometry().setFromPoints(ring), new THREE.LineBasicMaterial({ depthTest: false }));
      line.renderOrder = 10;
      line.frustumCulled = false;
      line.visible = visible;
      line.userData.layer = s.layer;
      this.group.add(line);
      this.shown.set(s.id, { line, points, closed: !!s.closed, revision: s.revision, name: s.name });
    }
    if (this.selected && !this.shown.has(this.selected)) this.select(null);
    this.recolour();
    this.placeHandles();
  }

  /** Listens on the canvas; returns a function that stops listening. */
  attach(): () => void {
    const canvas = this.viewport.renderer.domElement;
    const down = (e: PointerEvent) => this.pointerDown(e);
    const move = (e: PointerEvent) => this.pointerMove(e);
    const up = (e: PointerEvent) => void this.pointerUp(e);
    const dbl = () => void this.finishDrawing();
    const key = (e: KeyboardEvent) => this.keyDown(e);
    // Capture phase runs before OrbitControls' own listener on the canvas.
    canvas.addEventListener('pointerdown', down, { capture: true });
    canvas.addEventListener('pointermove', move);
    canvas.addEventListener('pointerup', up);
    canvas.addEventListener('dblclick', dbl);
    window.addEventListener('keydown', key);
    return () => {
      canvas.removeEventListener('pointerdown', down, { capture: true });
      canvas.removeEventListener('pointermove', move);
      canvas.removeEventListener('pointerup', up);
      canvas.removeEventListener('dblclick', dbl);
      window.removeEventListener('keydown', key);
    };
  }

  private pointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    if (this.mode === 'select' && this.selected) {
      const index = this.handleAt(this.selected, e.clientX, e.clientY);
      if (index >= 0) {
        this.drag = { id: this.selected, index, hit: null };
        this.viewport.controls.enabled = false;
        this.viewport.renderer.domElement.setPointerCapture(e.pointerId);
        e.stopImmediatePropagation();
        return;
      }
    }
    this.down = { x: e.clientX, y: e.clientY };
  }

  private pointerMove(e: PointerEvent): void {
    if (this.drag) {
      this.latest = { x: e.clientX, y: e.clientY };
      void this.pumpDrag();
      return;
    }
    if (this.mode === 'select' && this.selected) {
      const index = this.handleAt(this.selected, e.clientX, e.clientY);
      this.hovered = index >= 0 ? { id: this.selected, index } : null;
    }
  }

  /** One snap request in flight; moves that arrive meanwhile collapse to the latest. */
  private async pumpDrag(): Promise<void> {
    if (this.inFlight || !this.latest || !this.drag) return;
    this.inFlight = true;
    const target = this.latest;
    this.latest = null;
    const hit = await this.snapAt(target.x, target.y, this.drag);
    if (hit && this.drag) {
      this.reportSnap(hit);
      this.drag.hit = hit;
      const shown = this.shown.get(this.drag.id);
      if (shown) {
        const points = shown.points.map((p) => p.clone());
        points[this.drag.index].set(...hit.local);
        this.showPreview(shown.closed ? [...points, points[0]] : points);
      }
    }
    this.inFlight = false;
    void this.pumpDrag();
  }

  private async pointerUp(e: PointerEvent): Promise<void> {
    if (this.drag) {
      const { id, index, hit } = this.drag;
      this.drag = null;
      this.latest = null;
      this.viewport.controls.enabled = true;
      this.viewport.renderer.domElement.releasePointerCapture(e.pointerId);
      this.preview.visible = false;
      if (hit) await this.actions.run('string.move_vertex', { string: id, index, to: hit.world });
      return;
    }
    const down = this.down;
    this.down = null;
    if (!down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > CLICK_SLOP_PX) return;

    if (this.mode === 'draw') {
      const hit = await this.snapAt(e.clientX, e.clientY);
      if (!hit) return this.actions.status('No point there: the view looks along the working plane; tilt it down or use plan view');
      this.pending.push(hit);
      this.refreshPreview();
      this.placeHandles();
      const snapped = hit.kind && hit.kind !== 'surface' ? ` (on ${hit.label})` : '';
      this.actions.status(`${this.pending.length} vertices${snapped}; Enter or double-click to finish, Esc to abandon`);
      return;
    }
    if (e.altKey && this.selected) {
      // On the segment itself, so the string keeps its shape.
      const segment = this.segmentAt(this.selected, e.clientX, e.clientY);
      const hit = segment < 0 ? null : this.pointOnSegment(this.selected, segment, e.clientX, e.clientY);
      if (hit) await this.actions.run('string.insert_vertex', { string: this.selected, index: segment + 1, point: hit.world });
      return;
    }
    this.select(this.stringAt(e.clientX, e.clientY));
  }

  private keyDown(e: KeyboardEvent): void {
    if (e.target instanceof HTMLInputElement) return;
    if (this.mode === 'draw') {
      if (e.key === 'Enter') void this.finishDrawing();
      if (e.key === 'Escape') {
        this.pending = [];
        this.refreshPreview();
        this.placeHandles();
        this.actions.status('String abandoned');
      }
      if (e.key === 'Backspace' && this.pending.length > 0) {
        this.pending.pop();
        this.refreshPreview();
        this.placeHandles();
      }
      return;
    }
    if (e.key === 'Escape') this.select(null);
    if ((e.key === 'Delete' || e.key === 'Backspace') && this.hovered) {
      const { id, index } = this.hovered;
      this.hovered = null;
      void this.actions.run('string.delete_vertex', { string: id, index });
    }
  }

  private async finishDrawing(): Promise<void> {
    if (this.mode !== 'draw') return;
    // A double-click also lands two clicks on the same spot: drop repeated vertices.
    const points: Vec3[] = [];
    for (const hit of this.pending) {
      const last = points[points.length - 1];
      if (!last || last.some((c, i) => c !== hit.world[i])) points.push(hit.world);
    }
    if (points.length < 2) return this.actions.status('A string needs at least two vertices');
    this.names++;
    const ok = await this.actions.run('string.create', { name: `string ${this.names}`, layer: this.actions.layer(), points });
    if (ok) {
      this.pending = [];
      this.refreshPreview();
      this.placeHandles();
    }
  }

  private refreshPreview(): void {
    if (this.mode === 'draw' && this.pending.length > 0) {
      this.showPreview(this.pending.map((h) => new THREE.Vector3(...h.local)));
    } else {
      this.preview.visible = false;
    }
  }

  private showPreview(points: THREE.Vector3[]): void {
    this.preview.geometry.dispose();
    this.preview.geometry = new THREE.BufferGeometry().setFromPoints(points.length > 1 ? points : [points[0], points[0]]);
    this.preview.visible = true;
  }

  private recolour(): void {
    for (const [id, shown] of this.shown) {
      const layer = this.layers.get(shown.line.userData.layer as string);
      const colour = id === this.selected ? SELECTED : layer ? colourOf(layer.colour) : 0xffffff;
      (shown.line.material as THREE.LineBasicMaterial).color.setHex(colour);
    }
  }

  /** Handles for the selected string's vertices, or for the vertices being drawn. */
  private placeHandles(): void {
    const points =
      this.mode === 'draw'
        ? this.pending.map((h) => new THREE.Vector3(...h.local))
        : this.selected
          ? (this.shown.get(this.selected)?.points ?? [])
          : [];
    // A densely draped string would bury itself in handles: draw one only where it stands clear of
    // the last one drawn, plus both ends and the one under the cursor.
    const hovered = this.hovered?.id === this.selected ? this.hovered.index : -1;
    const camera = this.viewport.camera;
    const { clientWidth: w, clientHeight: h } = this.viewport.renderer.domElement;
    const at = new THREE.Vector3();
    let last: [number, number] | null = null;
    let n = 0;
    for (let i = 0; i < points.length && n < MAX_HANDLES; i++) {
      at.copy(points[i]).project(camera);
      const s: [number, number] = [((at.x + 1) / 2) * w, ((1 - at.y) / 2) * h];
      const clear = !last || Math.hypot(s[0] - last[0], s[1] - last[1]) >= HANDLE_SPACING_PX;
      if (!clear && i !== points.length - 1 && i !== hovered) continue;
      last = s;
      this.dummy.position.copy(points[i]);
      this.dummy.scale.setScalar(this.viewport.pixelSize(points[i]) * 6);
      this.dummy.updateMatrix();
      this.handles.setMatrixAt(n++, this.dummy.matrix);
    }
    this.handles.count = n;
    this.handles.instanceMatrix.needsUpdate = true;
  }

  private screen(p: THREE.Vector3): [number, number] {
    return this.viewport.projectToClient(p);
  }

  /** A snap from a string point the viewport already holds (scene-local). */
  private stringHit(id: string, local: THREE.Vector3, kind: 'vertex' | 'segment' | 'intersection', label: string): SnapHit {
    const l: Vec3 = [local.x, local.y, local.z];
    const [ox, oy, oz] = this.origin;
    return { local: l, world: [ox + l[0], oy + l[1], oz + l[2]], object: id, kind, label };
  }

  /**
   * Snaps the cursor: a visible string's vertex (or one being drawn) within the handle distance,
   * else the nearest point on a visible string's segment within the pick distance, else the
   * surface under the cursor. `skip` leaves out a vertex being dragged and its two segments.
   */
  private async snapAt(x: number, y: number, skip?: { id: string; index: number }): Promise<SnapHit | null> {
    let vertex: SnapHit | null = null;
    let best = HANDLE_PX * HANDLE_PX;
    const consider = (id: string, name: string, points: THREE.Vector3[]) =>
      points.forEach((p, i) => {
        if (skip?.id === id && skip.index === i) return;
        const [sx, sy] = this.screen(p);
        const d = (sx - x) ** 2 + (sy - y) ** 2;
        if (d <= best) {
          best = d;
          vertex = this.stringHit(id, p, 'vertex', `vertex ${i + 1} of ${name}`);
        }
      });
    for (const [id, shown] of this.shown) if (shown.line.visible) consider(id, shown.name, shown.points);
    if (this.mode === 'draw') consider('', 'this string', this.pending.map((h) => new THREE.Vector3(...h.local)));
    if (vertex) {
      // Drawn vertices keep the exact coordinates they were snapped with.
      const v: SnapHit = vertex;
      return v.object === '' ? (this.pending.find((h) => h.local.every((c, k) => c === v.local[k])) ?? v) : v;
    }

    // Segments near the cursor: the nearest is the segment snap, and pairs of them may cross.
    const near: { id: string; index: number; d: number; a: THREE.Vector3; b: THREE.Vector3; name: string }[] = [];
    for (const [id, shown] of this.shown) {
      if (!shown.line.visible) continue;
      const n = shown.closed ? shown.points.length : shown.points.length - 1;
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % shown.points.length;
        if (skip?.id === id && (skip.index === i || skip.index === j)) continue;
        const d = distanceToSegment([x, y], this.screen(shown.points[i]), this.screen(shown.points[j]));
        if (d <= CROSSING_REACH_PX) near.push({ id, index: i, d, a: shown.points[i], b: shown.points[j], name: shown.name });
      }
    }
    const crossing = this.crossingNear(near.slice().sort((p, q) => p.d - q.d).slice(0, 64), x, y);
    if (crossing) return crossing;
    const segment = near.reduce<(typeof near)[number] | null>((best, s) => (s.d <= PICK_PX && (!best || s.d < best.d) ? s : best), null);
    if (segment) {
      const hit = this.pointOnSegment(segment.id, segment.index, x, y);
      if (hit) return hit;
    }
    const surface = await this.actions.snap(x, y);
    if (surface) return { ...surface, kind: 'surface' };
    const dragged = skip ? this.shown.get(skip.id)?.points[skip.index] : undefined;
    const last = this.pending[this.pending.length - 1];
    const plane = dragged ? dragged.z : last ? last.local[2] : this.viewport.controls.target.z;
    return this.onPlane(x, y, plane);
  }

  /** Where the pick ray through the cursor meets the level plane at scene-local height `z`. */
  private onPlane(x: number, y: number, z: number): SnapHit | null {
    const ray = this.viewport.rayFromNdc(...this.viewport.clientToNdc(x, y));
    const [o, d] = [ray.origin, ray.direction];
    if (Math.abs(d[2]) < 1e-9) return null;
    const t = (z - o[2]) / d[2];
    // Behind the eye, or so far off towards the horizon that the point would mean nothing.
    if (t <= 0 || t > 1e6) return null;
    const local = new THREE.Vector3(o[0] + t * d[0], o[1] + t * d[1], z);
    const hit = this.stringHit('', local, 'vertex', '');
    return { ...hit, kind: 'plane', label: `the working plane at RL ${hit.world[2].toFixed(2)}` };
  }

  /** The point on a segment of a string nearest the pick ray through the cursor. */
  private pointOnSegment(id: string, index: number, x: number, y: number): SnapHit | null {
    const shown = this.shown.get(id);
    if (!shown) return null;
    const a = shown.points[index];
    const b = shown.points[(index + 1) % shown.points.length];
    const ray = this.viewport.rayFromNdc(...this.viewport.clientToNdc(x, y));
    const o = new THREE.Vector3(...ray.origin);
    const d = new THREE.Vector3(...ray.direction);
    // Closest approach of the ray o + s d and the segment a + t (b - a), with t kept on the segment.
    const u = b.clone().sub(a);
    const w = a.clone().sub(o);
    const [uu, ud, dd, uw, dw] = [u.dot(u), u.dot(d), d.dot(d), u.dot(w), d.dot(w)];
    const denom = uu * dd - ud * ud;
    const t = denom > 1e-12 ? Math.max(0, Math.min(1, (ud * dw - dd * uw) / denom)) : 0;
    return this.stringHit(id, a.clone().addScaledVector(u, t), 'segment', `segment ${index + 1} of ${shown.name}`);
  }

  /**
   * Where two of the given segments cross in plan, if that crossing is within the handle distance
   * of the cursor; the nearest such crossing. Its RL is the first segment's there.
   */
  private crossingNear(segments: { id: string; index: number; a: THREE.Vector3; b: THREE.Vector3; name: string }[], x: number, y: number): SnapHit | null {
    let best: SnapHit | null = null;
    let bestDist = HANDLE_PX * HANDLE_PX;
    for (let i = 0; i < segments.length; i++) {
      for (let j = i + 1; j < segments.length; j++) {
        const [p, q] = [segments[i], segments[j]];
        // Neighbouring segments of one string meet at a vertex, which the vertex snap already offers.
        if (p.id === q.id && Math.abs(p.index - q.index) <= 1) continue;
        const r = [p.b.x - p.a.x, p.b.y - p.a.y];
        const s = [q.b.x - q.a.x, q.b.y - q.a.y];
        const denom = r[0] * s[1] - r[1] * s[0];
        if (Math.abs(denom) < 1e-12) continue;
        const w = [q.a.x - p.a.x, q.a.y - p.a.y];
        const t = (w[0] * s[1] - w[1] * s[0]) / denom;
        const u = (w[0] * r[1] - w[1] * r[0]) / denom;
        if (t < 0 || t > 1 || u < 0 || u > 1) continue;
        const point = p.a.clone().lerp(p.b, t);
        const [sx, sy] = this.screen(point);
        const d = (sx - x) ** 2 + (sy - y) ** 2;
        if (d <= bestDist) {
          bestDist = d;
          best = this.stringHit(p.id, point, 'intersection', `the crossing of ${p.name} and ${q.name}`);
        }
      }
    }
    return best;
  }

  /**
   * Adds a typed vertex while drawing: `E N [RL]` in mine grid coordinates, or `@distance<bearing`
   * from the last vertex (plan distance, grid bearing in degrees clockwise from north) with an
   * optional RL or `grade%`. Without an RL the vertex takes the surface below it, else the last
   * vertex's RL. An empty entry finishes the string. Returns an error to show, or null.
   */
  async addTyped(text: string): Promise<string | null> {
    if (this.mode !== 'draw') return 'Switch to Draw string first';
    const entry = text.trim();
    if (entry === '') {
      await this.finishDrawing();
      return null;
    }
    const last = this.pending[this.pending.length - 1];
    let world: Vec3;
    let rl: number | null = null;
    if (entry.startsWith('@')) {
      const m = /^@\s*([-+]?\d*\.?\d+)\s*<\s*([-+]?\d*\.?\d+)\s*(?:[ ,]\s*([-+]?\d*\.?\d+)\s*(%?))?$/.exec(entry);
      if (!m) return 'Type @distance<bearing, with an optional RL or grade%, e.g. @50<90 or @50<90 -10%';
      if (!last) return 'Place a first vertex before typing a bearing and distance';
      const [distance, bearing] = [Number(m[1]), (Number(m[2]) * Math.PI) / 180];
      world = [last.world[0] + distance * Math.sin(bearing), last.world[1] + distance * Math.cos(bearing), last.world[2]];
      if (m[3] !== undefined) rl = m[4] === '%' ? last.world[2] + (Number(m[3]) / 100) * distance : Number(m[3]);
    } else {
      const numbers = entry.split(/[\s,]+/).map(Number);
      if (numbers.length < 2 || numbers.length > 3 || numbers.some((v) => !Number.isFinite(v))) return 'Type E N or E N RL, e.g. 553100 7489100 295';
      world = [numbers[0], numbers[1], last?.world[2] ?? NaN];
      if (numbers.length === 3) rl = numbers[2];
    }
    const [ox, oy, oz] = this.origin;
    if (rl === null) {
      const below = await this.actions.snapDown(world[0] - ox, world[1] - oy);
      rl = below ? below.world[2] : world[2];
    }
    if (!Number.isFinite(rl)) return 'No surface below that point and no vertex to take an RL from: type the RL too';
    world[2] = rl;
    this.pending.push({ local: [world[0] - ox, world[1] - oy, world[2] - oz], world, object: '', kind: 'surface', label: 'typed' });
    this.refreshPreview();
    this.placeHandles();
    this.actions.status(`${this.pending.length} vertices; type the next, or Enter on an empty entry to finish`);
    return null;
  }

  private reportSnap(hit: SnapHit): void {
    if (hit.kind && hit.kind !== 'surface') this.actions.status(`Snapped to ${hit.label}`);
  }

  private handleAt(id: string, x: number, y: number): number {
    const shown = this.shown.get(id);
    if (!shown) return -1;
    let best = -1;
    let bestDist = HANDLE_PX * HANDLE_PX;
    shown.points.forEach((p, i) => {
      const [sx, sy] = this.screen(p);
      const d = (sx - x) ** 2 + (sy - y) ** 2;
      if (d <= bestDist) {
        best = i;
        bestDist = d;
      }
    });
    return best;
  }

  /** Index of the segment of `id` nearest the cursor (within the pick distance), or -1. */
  private segmentAt(id: string, x: number, y: number): number {
    const shown = this.shown.get(id);
    if (!shown) return -1;
    const n = shown.closed ? shown.points.length : shown.points.length - 1;
    let best = -1;
    let bestDist = PICK_PX;
    for (let i = 0; i < n; i++) {
      const d = distanceToSegment([x, y], this.screen(shown.points[i]), this.screen(shown.points[(i + 1) % shown.points.length]));
      if (d <= bestDist) {
        best = i;
        bestDist = d;
      }
    }
    return best;
  }

  private stringAt(x: number, y: number): string | null {
    let best: string | null = null;
    let bestDist = PICK_PX;
    for (const [id, shown] of this.shown) {
      if (!shown.line.visible) continue;
      const n = shown.closed ? shown.points.length : shown.points.length - 1;
      for (let i = 0; i < n; i++) {
        const d = distanceToSegment([x, y], this.screen(shown.points[i]), this.screen(shown.points[(i + 1) % shown.points.length]));
        if (d <= bestDist) {
          best = id;
          bestDist = d;
        }
      }
    }
    return best;
  }
}

function distanceToSegment(p: [number, number], a: [number, number], b: [number, number]): number {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const length = dx * dx + dy * dy;
  const t = length > 0 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length)) : 0;
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}
