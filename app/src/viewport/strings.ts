// Strings in the viewport, and editing them through engine commands. A layer's strings draw as one
// set of lines, so thousands of contours are one draw, and the selected string draws over them.
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
// drawing, else the RL at the centre of the view. With snapping off (S, or the toolbar), every point
// goes straight on that plane.
//
// With a section open the section plane is the working plane, and points snap to its cut lines. The
// strings stay ordinary 3D strings, and stepping between clicks draws one across several sections.
// Only what lies within the section's width can be picked, shown a handle or dragged, and a dragged
// vertex lands on the plane.
import * as THREE from 'three/webgpu';
import type { Layer, ProjectSummary, SnapHit, Vec3 } from '../bridge';
import { colourOf, SELECTED } from './load';
import { PointMark } from './markers';
import type { Viewport } from './Viewport';

export type Mode = 'select' | 'draw' | 'section';

export interface StringActions {
  /** Runs an engine command; resolves true if it succeeded. */
  run(name: string, params: Record<string, unknown>): Promise<boolean>;
  snap(clientX: number, clientY: number): Promise<SnapHit | null>;
  /** The surface straight below a scene-local plan position, if any. */
  snapDown(x: number, y: number): Promise<SnapHit | null>;
  status(text: string): void;
  /** Layer that new strings go on. */
  layer(): string;
  /** The second point of a section was picked. */
  section(a: Vec3, b: Vec3): void;
  /** A click that met no string: selects what's under it, or nothing. */
  pick(clientX: number, clientY: number): void;
  /** A Ctrl-click: adds a string to the marks or takes it off. With null, clears them. */
  mark(id: string | null): void;
}

interface Shown {
  layer: string;
  visible: boolean;
  points: THREE.Vector3[];
  closed: boolean;
  revision: number;
  name: string;
}

/** What the string tools work on while a section is open. */
export interface SectionTools {
  /** Strings this close to the plane show on the section (m). */
  width: number;
  /** How far a scene-local point lies off the plane (m). */
  offset(p: THREE.Vector3): number;
  /** The point on the plane under the cursor. */
  onPlane(x: number, y: number): SnapHit | null;
  /** The cut line near the cursor, if any, leaving out the segments of a vertex being dragged. */
  snap(x: number, y: number, vertexPx: number, segmentPx: number, skip?: THREE.Vector3): Promise<SnapHit | null>;
  /** What a click picks on the section, within `px` of the cursor: an object's id. */
  pick(x: number, y: number, px: number): Promise<string | null>;
}

/**
 * A stretch of a string's segments with a sphere round it. A pick tests the spheres against the ray
 * through the cursor and looks closely only at the runs it meets, so a project of a million vertices
 * of contours costs a pick a few thousand vertices, not a million.
 */
interface Run {
  id: string;
  shown: Shown;
  /** Segment `i` joins vertices `i` and `i + 1`; these are `first` to `first + count - 1`. */
  first: number;
  count: number;
  centre: THREE.Vector3;
  radius: number;
}

/** Segments in a run. */
const RUN_SEGMENTS = 64;

/** A string's segments as runs. */
function runsOf(id: string, shown: Shown): Run[] {
  const { points, closed } = shown;
  const segments = closed ? points.length : points.length - 1;
  const runs: Run[] = [];
  for (let first = 0; first < segments; first += RUN_SEGMENTS) {
    const count = Math.min(RUN_SEGMENTS, segments - first);
    const box = new THREE.Box3();
    for (let i = first; i <= first + count; i++) box.expandByPoint(points[i % points.length]);
    const centre = box.getCenter(new THREE.Vector3());
    runs.push({ id, shown, first, count, centre, radius: box.getSize(new THREE.Vector3()).length() / 2 });
  }
  return runs;
}

const PICK_PX = 8;
const HANDLE_PX = 10;
/** Handles closer than this on screen to the last one drawn are skipped (they can still be grabbed). */
const HANDLE_SPACING_PX = 12;
const CLICK_SLOP_PX = 4;
const PREVIEW = 0x66d9ff;
const MAX_HANDLES = 8192;
const SNAP_MARK_PX = 7;
/** A snap marker's colour says what the point will land on. */
const SNAP_COLOURS: Record<NonNullable<SnapHit['kind']>, number> = {
  vertex: 0x7dfca0,
  segment: 0x66d9ff,
  intersection: 0xff7ad9,
  surface: 0xffffff,
  plane: 0xffb347,
};

export class StringLayer {
  mode: Mode = 'select';
  /** A panel is waiting on the next click for a point. */
  private wanted: ((point: Vec3 | null) => void) | null = null;
  /** Where a pick landed, in local coordinates, and where a line being picked starts from. */
  private lastPick = new THREE.Vector3();
  private pickFrom: THREE.Vector3 | null = null;
  selected: string | null = null;
  onSelect?: (id: string | null) => void;

  private shown = new Map<string, Shown>();
  private runs: Run[] = [];
  private group = new THREE.Group();
  /** Each layer's strings as one set of lines. */
  private layerLines = new Map<string, THREE.LineSegments>();
  /** The selected string, over its layer's lines. */
  private highlight: THREE.Line;
  private markedLines: THREE.LineSegments;
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
  private snapping = true;
  /** A section is showing: the plane to work on. The strings draw through it, as its cut lines. */
  private section: SectionTools | null = null;
  private snapMark: PointMark;
  private snapLabel: HTMLElement;
  private hovering: { x: number; y: number } | null = null;
  private hoverInFlight = false;
  /** Surfaces and grids by id, to name what a point snaps to. */
  private surfaceNames = new Map<string, string>();
  /** Snapping was switched on or off. */
  onSnapping?: (on: boolean) => void;
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
    this.highlight = new THREE.Line(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: SELECTED, depthTest: false }));
    this.highlight.renderOrder = 11;
    this.highlight.frustumCulled = false;
    this.highlight.visible = false;
    this.markedLines = new THREE.LineSegments(new THREE.BufferGeometry(), new THREE.LineBasicMaterial({ color: SELECTED, depthTest: false }));
    this.markedLines.renderOrder = 11;
    this.markedLines.frustumCulled = false;
    this.markedLines.visible = false;
    this.handles = new THREE.InstancedMesh(new THREE.OctahedronGeometry(1), new THREE.MeshBasicMaterial({ color: 0xffffff, depthTest: false }), MAX_HANDLES);
    this.handles.count = 0;
    this.handles.renderOrder = 13;
    this.handles.frustumCulled = false;
    viewport.overlay.add(this.group, this.highlight, this.markedLines, this.preview, this.handles);
    viewport.beforeRender.add(() => this.placeHandles());
    this.snapMark = new PointMark(viewport, SNAP_MARK_PX);
    this.snapLabel = Object.assign(document.createElement('div'), { className: 'snap-label', hidden: true });
    viewport.renderer.domElement.parentElement?.append(this.snapLabel);
  }

  /** The next click, snapped, in world coordinates. Esc, or asking again, gives null. */
  pickPoint(prompt: string): Promise<Vec3 | null> {
    this.finishPick(null);
    this.actions.status(prompt);
    return new Promise((resolve) => {
      this.wanted = resolve;
    });
  }

  /** Two clicks, with a line following the cursor from the first. Null if either is cancelled. */
  async pickLine(first: string, second: string): Promise<[Vec3, Vec3] | null> {
    const from = await this.pickPoint(first);
    if (!from) return null;
    this.pickFrom = this.lastPick.clone();
    const to = await this.pickPoint(second);
    this.pickFrom = null;
    this.preview.visible = false;
    return to && [from, to];
  }

  private finishPick(point: Vec3 | null): void {
    const wanted = this.wanted;
    this.wanted = null;
    this.clearSnapMark();
    wanted?.(point);
  }

  setMode(mode: Mode): void {
    this.finishPick(null);
    this.mode = mode;
    this.pending = [];
    this.clearSnapMark();
    this.refreshPreview();
    this.placeHandles();
  }

  /** The strings as drawn, for the section's plan map. */
  get lines(): THREE.Object3D {
    return this.group;
  }

  /** Works on a section's plane while one is open, and on the view again when it closes. */
  setSection(section: SectionTools | null): void {
    this.section = section;
    this.group.visible = !section;
    this.clearSnapMark();
    this.recolour();
    this.placeHandles();
  }

  /** Whether a scene-local point shows on the section, if one is open. */
  private onSection(p: THREE.Vector3): boolean {
    return !this.section || Math.abs(this.section.offset(p)) <= this.section.width;
  }

  /** The part of a segment that shows on the section, if one is open. */
  private shownPart(a: THREE.Vector3, b: THREE.Vector3): [THREE.Vector3, THREE.Vector3] | null {
    if (!this.section) return [a, b];
    const { width } = this.section;
    const [da, db] = [this.section.offset(a), this.section.offset(b)];
    if (da === db) return Math.abs(da) <= width ? [a, b] : null;
    const [enter, leave] = [(-width - da) / (db - da), (width - da) / (db - da)];
    const [from, to] = [Math.max(0, Math.min(enter, leave)), Math.min(1, Math.max(enter, leave))];
    return from < to ? [a.clone().lerp(b, from), a.clone().lerp(b, to)] : null;
  }

  /** Works mid-string or mid-drag too: the next point takes the new setting. */
  toggleSnapping(): void {
    this.snapping = !this.snapping;
    this.onSnapping?.(this.snapping);
    this.clearSnapMark();
    this.actions.status(this.snapping ? 'Snap on: strings, then surfaces, then the working plane' : 'Snap off: points go on the working plane');
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
    this.surfaceNames = new Map(summary.objects.filter((o) => o.drawn).map((o) => [o.id, o.name]));
    const everyString = summary.objects.filter((o) => o.kind === 'string' && o.vertices_local);
    this.names = Math.max(this.names, everyString.length);
    const strings = everyString.filter((o) => !o.hidden);
    // Layers whose lines must be made again; an edit on one leaves the contours on another alone.
    const changed = new Set<string>();
    const live = new Set(strings.map((s) => s.id));
    for (const [id, shown] of this.shown) {
      if (!live.has(id)) {
        changed.add(shown.layer);
        this.shown.delete(id);
      }
    }
    for (const s of strings) {
      const visible = this.layers.get(s.layer)?.visible ?? true;
      const existing = this.shown.get(s.id);
      if (existing && existing.revision === s.revision) {
        existing.visible = visible;
        continue;
      }
      if (existing) changed.add(existing.layer);
      changed.add(s.layer);
      const points = (s.vertices_local ?? []).map((p) => new THREE.Vector3(...p));
      this.shown.set(s.id, { layer: s.layer, visible, points, closed: !!s.closed, revision: s.revision, name: s.name });
    }
    for (const layer of changed) this.drawLayer(layer);
    if (changed.size > 0) this.runs = [...this.shown].flatMap(([id, shown]) => runsOf(id, shown));
    for (const [id, lines] of this.layerLines) {
      const layer = this.layers.get(id);
      lines.visible = layer?.visible ?? true;
      (lines.material as THREE.LineBasicMaterial).color.setHex(layer ? colourOf(layer.colour) : 0xffffff);
    }
    if (this.selected && !this.shown.has(this.selected)) this.select(null);
    this.recolour();
    this.placeHandles();
  }

  /** Draws the marked strings over everything, as a selected one is. */
  setMarked(ids: string[]): void {
    const strings = ids.flatMap((id) => this.shown.get(id) ?? []).filter((s) => s.visible);
    const points = strings.flatMap((s) => {
      const line = s.closed ? [...s.points, s.points[0]] : s.points;
      return line.slice(1).flatMap((p, k) => [line[k], p]);
    });
    this.markedLines.geometry.dispose();
    this.markedLines.geometry = new THREE.BufferGeometry().setFromPoints(points);
    this.markedLines.visible = points.length > 0 && !this.section;
  }

  /** Makes a layer's lines again from its strings. */
  private drawLayer(id: string): void {
    const old = this.layerLines.get(id);
    if (old) {
      this.group.remove(old);
      old.geometry.dispose();
      (old.material as THREE.Material).dispose();
      this.layerLines.delete(id);
    }
    const strings = [...this.shown.values()].filter((s) => s.layer === id);
    const segments = strings.reduce((n, s) => n + (s.closed ? s.points.length : s.points.length - 1), 0);
    if (segments <= 0) return;
    const positions = new Float32Array(segments * 6);
    let at = 0;
    for (const { points, closed } of strings) {
      const n = closed ? points.length : points.length - 1;
      for (let i = 0; i < n; i++) {
        const [a, b] = [points[i], points[(i + 1) % points.length]];
        positions.set([a.x, a.y, a.z, b.x, b.y, b.z], at);
        at += 6;
      }
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    const lines = new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ depthTest: false }));
    lines.renderOrder = 10;
    lines.frustumCulled = false;
    this.group.add(lines);
    this.layerLines.set(id, lines);
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
    const left = () => this.clearSnapMark();
    canvas.addEventListener('pointermove', move);
    canvas.addEventListener('pointerup', up);
    canvas.addEventListener('pointerleave', left);
    canvas.addEventListener('dblclick', dbl);
    window.addEventListener('keydown', key);
    return () => {
      canvas.removeEventListener('pointerdown', down, { capture: true });
      canvas.removeEventListener('pointermove', move);
      canvas.removeEventListener('pointerup', up);
      canvas.removeEventListener('pointerleave', left);
      canvas.removeEventListener('dblclick', dbl);
      window.removeEventListener('keydown', key);
    };
  }

  private pointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    if (this.mode === 'select' && this.selected && !this.wanted) {
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
    // A held button is the view turning or panning, not a point being placed.
    if (e.buttons !== 0) return this.clearSnapMark();
    if (this.mode !== 'select' || this.wanted) {
      this.hovering = { x: e.clientX, y: e.clientY };
      void this.pumpHover();
    }
    if (this.mode === 'select' && this.selected) {
      const index = this.handleAt(this.selected, e.clientX, e.clientY);
      this.hovered = index >= 0 ? { id: this.selected, index } : null;
    }
  }

  /** What the next click would land on, shown as the cursor moves. As in `pumpDrag`, one at a time. */
  private async pumpHover(): Promise<void> {
    if (this.hoverInFlight || !this.hovering) return;
    this.hoverInFlight = true;
    const at = this.hovering;
    this.hovering = null;
    const hit = await this.snapAt(at.x, at.y);
    this.hoverInFlight = false;
    if (this.mode !== 'select' || this.wanted) this.showSnap(hit, at);
    if (this.wanted && this.pickFrom && hit) this.showPreview([this.pickFrom, new THREE.Vector3(...hit.local)]);
    void this.pumpHover();
  }

  private showSnap(hit: SnapHit | null, at: { x: number; y: number }): void {
    if (!hit?.kind) return this.clearSnapMark();
    this.snapMark.show(new THREE.Vector3(...hit.local), SNAP_COLOURS[hit.kind]);
    const box = this.viewport.renderer.domElement.getBoundingClientRect();
    this.snapLabel.textContent = this.describeSnap(hit);
    this.snapLabel.style.transform = `translate(${at.x - box.left + 14}px, ${at.y - box.top + 14}px)`;
    this.snapLabel.hidden = false;
  }

  private clearSnapMark(): void {
    this.hovering = null;
    this.snapMark.hide();
    this.snapLabel.hidden = true;
  }

  /** What a point lands on and at what RL, for the label beside the cursor. */
  private describeSnap(hit: SnapHit): string {
    if (hit.kind === 'plane') return hit.label ?? 'The working plane';
    const name = hit.kind === 'surface' ? ((hit.object && this.surfaceNames.get(hit.object)) ?? 'Solid') : hit.label;
    return `${name} · RL ${hit.world[2].toFixed(2)}`;
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
      this.showSnap(hit, target);
      this.drag.hit = hit;
      const shown = this.shown.get(this.drag.id);
      if (shown) {
        const points = shown.points.map((p) => p.clone());
        const { index } = this.drag;
        points[index].set(...hit.local);
        const n = points.length;
        // On a section only its neighbours: the rest of a string may lie far off the plane.
        const nearby = [shown.closed || index > 0 ? points[(index + n - 1) % n] : null, points[index], shown.closed || index < n - 1 ? points[(index + 1) % n] : null];
        this.showPreview(this.section ? nearby.filter((p) => p !== null) : shown.closed ? [...points, points[0]] : points);
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
      this.clearSnapMark();
      if (hit) await this.actions.run('string.move_vertex', { string: id, index, to: hit.world });
      return;
    }
    const down = this.down;
    this.down = null;
    if (!down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > CLICK_SLOP_PX) return;

    if (this.wanted) {
      const hit = await this.snapAt(e.clientX, e.clientY);
      if (!hit) return this.actions.status('No point there: tilt the view down or use plan view');
      this.lastPick.set(...hit.local);
      return this.finishPick(hit.world);
    }
    if (this.mode === 'section') {
      const hit = await this.snapAt(e.clientX, e.clientY);
      if (!hit) return this.actions.status('No point there: tilt the view down or use plan view');
      this.pending.push(hit);
      this.placeHandles();
      if (this.pending.length < 2) return this.actions.status('Section: pick the second point');
      const [a, b] = this.pending;
      this.pending = [];
      if (a.world[0] === b.world[0] && a.world[1] === b.world[1]) return this.actions.status('Section: those are the same point in plan; pick the first point again');
      return this.actions.section(a.world, b.world);
    }
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
    const string = this.stringAt(e.clientX, e.clientY);
    if (e.ctrlKey || e.metaKey) {
      if (string) this.actions.mark(string);
      return;
    }
    this.actions.mark(null);
    if (string) this.select(string);
    else this.actions.pick(e.clientX, e.clientY);
  }

  private keyDown(e: KeyboardEvent): void {
    if (e.target instanceof HTMLInputElement) return;
    if (e.key.toLowerCase() === 's' && !e.ctrlKey && !e.metaKey && !e.altKey) return this.toggleSnapping();
    if (e.key === 'Escape' && this.wanted) return this.finishPick(null);
    if (this.mode !== 'select') {
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
    if (e.key === 'Escape') {
      this.actions.mark(null);
      this.select(null);
    }
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
    if (this.mode !== 'select' && this.pending.length > 0) {
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
    const shown = this.selected ? this.shown.get(this.selected) : undefined;
    this.highlight.visible = !!shown?.visible && !this.section;
    if (!shown) return;
    this.highlight.geometry.dispose();
    this.highlight.geometry = new THREE.BufferGeometry().setFromPoints(shown.closed ? [...shown.points, shown.points[0]] : shown.points);
  }

  /** Handles for the selected string's vertices, or for the vertices being drawn. */
  private placeHandles(): void {
    const points =
      this.mode !== 'select'
        ? this.pending.map((h) => new THREE.Vector3(...h.local))
        : this.selected
          ? (this.shown.get(this.selected)?.points ?? [])
          : [];
    // On a section the selected string lights up only if it lies on the plane, which a step can change.
    const lying = this.section && this.mode === 'select' && this.selected ? this.shown.get(this.selected) : undefined;
    if (lying) this.highlight.visible = lying.visible && lying.points.every((p) => this.onSection(p));
    // A densely draped string would bury itself in handles: draw one only where it stands clear of
    // the last one drawn, plus both ends and the one under the cursor.
    const hovered = this.hovered?.id === this.selected ? this.hovered.index : -1;
    const camera = this.viewport.camera;
    const { clientWidth: w, clientHeight: h } = this.viewport.renderer.domElement;
    const at = new THREE.Vector3();
    let last: [number, number] | null = null;
    let n = 0;
    for (let i = 0; i < points.length && n < MAX_HANDLES; i++) {
      if (this.mode === 'select' && !this.onSection(points[i])) continue;
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

  /** Scene-local points to client pixels, for many at a time: one read of the canvas, no allocation. */
  private screen(): (p: THREE.Vector3) => [number, number] {
    const box = this.viewport.renderer.domElement.getBoundingClientRect();
    const camera = this.viewport.camera;
    const at = new THREE.Vector3();
    return (p) => {
      at.copy(p).project(camera);
      return [box.left + ((at.x + 1) / 2) * box.width, box.top + ((1 - at.y) / 2) * box.height];
    };
  }

  /** The visible runs whose spheres come within `reachPx` of the ray through the cursor. */
  private runsNear(x: number, y: number, reachPx: number): Run[] {
    const viewport = this.viewport;
    const { origin: o, direction: d } = viewport.rayFromNdc(...viewport.clientToNdc(x, y));
    // What a pixel measures: the same everywhere in plan, growing with distance in 3D.
    const eye = viewport.camera.position;
    const flat = viewport.flatView ? viewport.pixelSize(eye) : 0;
    const perMetre = viewport.flatView ? 0 : viewport.pixelSize(new THREE.Vector3(eye.x + d[0], eye.y + d[1], eye.z + d[2]));
    return this.runs.filter(({ shown, centre, radius }) => {
      if (!shown.visible) return false;
      const [wx, wy, wz] = [centre.x - o[0], centre.y - o[1], centre.z - o[2]];
      const along = wx * d[0] + wy * d[1] + wz * d[2];
      const [px, py, pz] = [wx - along * d[0], wy - along * d[1], wz - along * d[2]];
      const reach = radius + reachPx * (flat + perMetre * (Math.abs(along) + radius));
      return px * px + py * py + pz * pz <= reach * reach;
    });
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
    if (this.section) return this.snapOnSection(this.section, x, y, skip);
    const workingPlane = () => {
      const dragged = skip ? this.shown.get(skip.id)?.points[skip.index] : undefined;
      const last = this.pending[this.pending.length - 1];
      return this.onPlane(x, y, dragged ? dragged.z : last ? last.local[2] : this.viewport.controls.target.z);
    };
    if (!this.snapping) return workingPlane();
    const onString = this.snapToStrings(x, y, skip);
    if (onString) return onString;
    const surface = await this.actions.snap(x, y);
    if (surface) return { ...surface, kind: 'surface' };
    return workingPlane();
  }

  /** On a section: a vertex of the string being drawn, else the cut lines, else the plane itself. */
  private async snapOnSection(section: SectionTools, x: number, y: number, skip?: { id: string; index: number }): Promise<SnapHit | null> {
    if (!this.snapping) return section.onPlane(x, y);
    const screen = this.screen();
    const drawn = this.pending.find((hit) => {
      const [sx, sy] = screen(new THREE.Vector3(...hit.local));
      return Math.hypot(sx - x, sy - y) <= HANDLE_PX;
    });
    if (drawn) return drawn;
    const dragged = skip ? this.shown.get(skip.id)?.points[skip.index] : undefined;
    return (await section.snap(x, y, HANDLE_PX, PICK_PX, dragged)) ?? section.onPlane(x, y);
  }

  /** What a drag or the wheel works on at a screen point: a string near it, else the surface under it. */
  async pick(x: number, y: number): Promise<Vec3 | null> {
    return (this.snapToStrings(x, y) ?? (await this.actions.snap(x, y)))?.local ?? null;
  }

  /** A string's vertex, crossing or segment near the cursor, in that order, or null. */
  private snapToStrings(x: number, y: number, skip?: { id: string; index: number }): SnapHit | null {
    const screen = this.screen();
    const runs = this.runsNear(x, y, HANDLE_PX);
    let vertex: SnapHit | null = null;
    let best = HANDLE_PX * HANDLE_PX;
    const consider = (id: string, name: string, points: THREE.Vector3[], i: number) => {
      if (skip?.id === id && skip.index === i) return;
      const [sx, sy] = screen(points[i]);
      const d = (sx - x) ** 2 + (sy - y) ** 2;
      if (d <= best) {
        best = d;
        vertex = this.stringHit(id, points[i], 'vertex', `vertex ${i + 1} of ${name}`);
      }
    };
    for (const { id, shown, first, count } of runs) {
      for (let i = first; i <= first + count; i++) consider(id, shown.name, shown.points, i % shown.points.length);
    }
    if (this.mode !== 'select') {
      const drawn = this.pending.map((h) => new THREE.Vector3(...h.local));
      drawn.forEach((_, i) => consider('', 'this string', drawn, i));
    }
    if (vertex) {
      // Drawn vertices keep the exact coordinates they were snapped with.
      const v: SnapHit = vertex;
      return v.object === '' ? (this.pending.find((h) => h.local.every((c, k) => c === v.local[k])) ?? v) : v;
    }

    // Segments near the cursor: the nearest is the segment snap, and pairs of them may cross. A
    // crossing within the handle distance has both its segments within that distance too.
    const near: { id: string; index: number; d: number; a: THREE.Vector3; b: THREE.Vector3; name: string }[] = [];
    for (const { id, shown, first, count } of runs) {
      for (let i = first; i < first + count; i++) {
        const j = (i + 1) % shown.points.length;
        if (skip?.id === id && (skip.index === i || skip.index === j)) continue;
        const d = distanceToSegment([x, y], screen(shown.points[i]), screen(shown.points[j]));
        if (d <= HANDLE_PX) near.push({ id, index: i, d, a: shown.points[i], b: shown.points[j], name: shown.name });
      }
    }
    const crossing = this.crossingNear(near.slice().sort((p, q) => p.d - q.d).slice(0, 64), x, y);
    if (crossing) return crossing;
    const segment = near.reduce<(typeof near)[number] | null>((best, s) => (s.d <= PICK_PX && (!best || s.d < best.d) ? s : best), null);
    if (segment) {
      const hit = this.pointOnSegment(segment.id, segment.index, x, y);
      if (hit) return hit;
    }
    return null;
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
        const [sx, sy] = this.viewport.projectToClient(point);
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
      if (numbers.length < 2 || numbers.length > 3 || numbers.some((v) => !Number.isFinite(v))) return 'Type E N or E N RL, e.g. 650100 7450200 95';
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
    const screen = this.screen();
    let best = -1;
    let bestDist = HANDLE_PX * HANDLE_PX;
    shown.points.forEach((p, i) => {
      if (!this.onSection(p)) return;
      const [sx, sy] = screen(p);
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
    const screen = this.screen();
    let best = -1;
    let bestDist = PICK_PX;
    for (let i = 0; i < n; i++) {
      const part = this.shownPart(shown.points[i], shown.points[(i + 1) % shown.points.length]);
      if (!part) continue;
      const d = distanceToSegment([x, y], screen(part[0]), screen(part[1]));
      if (d <= bestDist) {
        best = i;
        bestDist = d;
      }
    }
    return best;
  }

  private stringAt(x: number, y: number): string | null {
    const screen = this.screen();
    let best: string | null = null;
    let bestDist = PICK_PX;
    for (const { id, shown, first, count } of this.runsNear(x, y, PICK_PX)) {
      for (let i = first; i < first + count; i++) {
        const part = this.shownPart(shown.points[i], shown.points[(i + 1) % shown.points.length]);
        if (!part) continue;
        const d = distanceToSegment([x, y], screen(part[0]), screen(part[1]));
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
