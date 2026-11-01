// The section view: a thin, exact slice through two plan points, in place of the plan view. The engine
// cuts every visible layer and sends the lines as binary; this draws them with real line weights and
// steps the plane along. A section is view state: no command, nothing to undo.
import { LineSegments2 } from 'three/addons/lines/webgpu/LineSegments2.js';
import { LineSegmentsGeometry } from 'three/addons/lines/LineSegmentsGeometry.js';
import * as THREE from 'three/webgpu';
import { type ProjectSummary, type SectionLines, sectionLines, sectionPick, sectionSnap, type SectionKind, type SnapHit, type Vec3 } from '../bridge';
import { colourOf, SELECTED } from './load';
import { SectionOverview } from './overview';
import { type GridLines, type SectionFrame, SectionGrid } from './sectionGrid';
import type { SectionTools } from './strings';
import type { Viewport } from './Viewport';

/** Strings this close to the plane (m) draw flat on it: a string crosses a thin slice, not lies in it. */
const STRING_WIDTH = 1;

/** Line weights by kind (px), until a layer sets its own. */
const WEIGHTS: Record<SectionKind, number> = { surface: 2.5, seam: 1.5, solid: 1.5, string: 2, selected: 3 };

/** Each kind's own lines draw over the next: strings, then seams, surfaces and solids. */
const ORDER: Record<SectionKind, number> = { solid: 10, surface: 11, seam: 12, string: 13, selected: 14 };

/** How much of a fill shows: enough to read the layer, with its cut lines standing out in the same colour. */
const FILL_OPACITY = 0.3;
/** Fills draw behind every line. */
const FILL_ORDER = 5;

/** Seam horizons by material, until colours can be set: coal stands out and waste sits back as context. */
const MATERIAL_COLOURS: Record<string, number> = { coal: 0xf2b134, waste: 0x8d8579 };

export type PlanPoint = [number, number];

export class SectionView {
  /** The section's ends in mine grid coordinates; null when no section is open. */
  private ends: { a: PlanPoint; b: PlanPoint } | null = null;
  private summary: ProjectSummary | null = null;
  private group = new THREE.Group();
  private lines = new Map<string, LineSegments2>();
  private fills = new Map<string, THREE.Mesh>();
  private filling = true;
  private selected: string | null = null;
  private weights = new Map<string, number>();
  private fetching: Promise<void> | null = null;
  private fetchAgain = false;
  private overview: SectionOverview;
  private grid: SectionGrid;
  /** Metres moved square to the section since it opened. */
  private moved = 0;
  /** Closed loops the last cut could not fill. */
  unfilled = 0;
  /** Tells of closed loops left unfilled, when that count changes. */
  onUnfilled?: (count: number) => void;
  /** What the plan map shows beyond the surfaces. */
  mapShows: THREE.Object3D[] = [];
  private increment = 25;
  /** The last section: request to frame submitted, and the engine's share of that (ms). */
  lastMs = 0;
  lastEngineMs = 0;

  constructor(private viewport: Viewport) {
    viewport.overlay.add(this.group);
    const host = viewport.renderer.domElement.parentElement as HTMLElement;
    this.overview = new SectionOverview(host, (target, x, y, half) =>
      viewport.planPicture(target, x, y, half, { show: this.mapShows, hide: [this.group] }),
    );
    // In whole steps, so a drag lands on the same sections the keys do.
    this.overview.onPointAt = (offset) => {
      const steps = Math.round(offset / this.stepSize);
      if (steps !== 0) void this.step(steps * this.stepSize);
    };
    this.grid = new SectionGrid(host);
    viewport.cameraMoved.add(() => this.drawGrid());
  }

  /** The RL grid's canvas, for checks. */
  get gridCanvas(): HTMLCanvasElement {
    return this.grid.canvasElement;
  }

  /** The grid lines drawn, for checks. */
  get gridLines(): GridLines {
    return this.grid.lines;
  }

  /** How long the grid's last drawing took (ms). */
  get gridDrawMs(): number {
    return this.grid.lastDrawMs;
  }

  /** The last cut as the engine sent it, for checks. */
  lastCut: SectionLines | null = null;

  /** The scene-local point at a distance along the section from A and an RL. */
  pointOn(along: number, rl: number): THREE.Vector3 {
    const { a, b } = this.ends ?? { a: [0, 0], b: [1, 0] };
    const o = this.summary?.origin ?? [0, 0, 0];
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return new THREE.Vector3(a[0] + ((b[0] - a[0]) / length) * along - o[0], a[1] + ((b[1] - a[1]) / length) * along - o[1], rl - o[2]);
  }

  /** Whether both ends of the section are on the overview map. */
  get overviewOnMap(): boolean {
    return this.overview.sectionOnMap;
  }

  /** How long the overview map's last picture took (ms). */
  get overviewPictureMs(): number {
    return this.overview.lastPictureMs;
  }

  /** The overview map, for checks. */
  get overviewCanvas(): HTMLCanvasElement {
    return this.overview.canvasElement;
  }

  /** The increment of a step and of a drag on the map (m). */
  get stepSize(): number {
    return this.increment;
  }

  set stepSize(distance: number) {
    this.increment = distance;
    this.drawOverview();
  }

  /** Metres moved square to the section since it opened: positive is away from the viewer. */
  get offset(): number {
    return this.moved;
  }

  get open(): boolean {
    return this.ends !== null;
  }

  /** Whether solids fill between their cut lines. */
  get filled(): boolean {
    return this.filling;
  }

  set filled(on: boolean) {
    this.filling = on;
    void this.refresh();
  }

  /** Draws an object's cut lines over the rest, or with null none. */
  setSelected(id: string | null): void {
    this.selected = id;
    void this.refresh();
  }

  /** The kinds' weight for a layer: its own, or null while it uses the defaults. */
  weightOf(layer: string): number | null {
    return this.weights.get(layer) ?? null;
  }

  /** Sets a layer's line weight (px); null goes back to the defaults. Display only. */
  setWeight(layer: string, px: number | null): void {
    if (px === null) this.weights.delete(layer);
    else this.weights.set(layer, px);
    this.reweigh();
  }

  /** Opens the section through two plan points (mine grid), framing the stretch between them. */
  async show(a: PlanPoint, b: PlanPoint, summary: ProjectSummary): Promise<void> {
    this.grid.hide();
    this.ends = { a, b };
    this.summary = summary;
    const lines = await sectionLines(a, b, STRING_WIDTH, this.filling, this.selected);
    if (!this.ends) return;
    this.draw(lines);
    const o = summary.origin ?? [0, 0, 0];
    const [from, to]: PlanPoint[] = [
      [a[0] - o[0], a[1] - o[1]],
      [b[0] - o[0], b[1] - o[1]],
    ];
    const length = Math.hypot(to[0] - from[0], to[1] - from[1]);
    this.moved = 0;
    this.overview.frameOn(from, to);
    this.viewport.showSection(from, to, [0, length], rlBetween(lines.positions, from, to, length));
    this.group.visible = true;
    this.overview.show();
    this.drawOverview();
    this.grid.show();
  }

  /** The view as the grid and the cursor read it: distances from A, RLs in mine grid. */
  private frame(): SectionFrame | null {
    if (!this.ends) return null;
    const { a, b } = this.ends;
    const o = this.summary?.origin ?? [0, 0, 0];
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const camera = this.viewport.sectionCamera;
    const { x, y, z } = camera.position;
    return {
      along: ((x + o[0] - a[0]) * (b[0] - a[0]) + (y + o[1] - a[1]) * (b[1] - a[1])) / length,
      rl: z + o[2],
      length,
      across: (camera.right - camera.left) / camera.zoom,
      up: (camera.top - camera.bottom) / camera.zoom,
    };
  }

  private drawGrid(): void {
    const frame = this.frame();
    if (frame && this.grid.shown) this.grid.draw(frame);
  }

  /** The cursor as a distance along the section and an RL. */
  private cursor(x: number, y: number, frame: SectionFrame): [number, number] {
    const [nx, ny] = this.viewport.clientToNdc(x, y);
    return [frame.along + (nx * frame.across) / 2, frame.rl + (ny * frame.up) / 2];
  }

  /** A scene-local point as a distance along the section and an RL. */
  private flat(p: THREE.Vector3): [number, number] {
    const { a, b } = this.ends ?? { a: [0, 0], b: [1, 0] };
    const o = this.summary?.origin ?? [0, 0, 0];
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return [((p.x + o[0] - a[0]) * (b[0] - a[0]) + (p.y + o[1] - a[1]) * (b[1] - a[1])) / length, p.z + o[2]];
  }

  /** What the string tools use while this section is open. */
  readonly tools: SectionTools = {
    width: STRING_WIDTH,
    offset: (p) => {
      const { a, b } = this.ends ?? { a: [0, 0], b: [1, 0] };
      const o = this.summary?.origin ?? [0, 0, 0];
      const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
      return ((b[0] - a[0]) * (p.y + o[1] - a[1]) - (b[1] - a[1]) * (p.x + o[0] - a[0])) / length;
    },
    onPlane: (x, y) => {
      const [frame, ends] = [this.frame(), this.ends];
      if (!frame || !ends) return null;
      const [along, rl] = this.cursor(x, y, frame);
      const o = this.summary?.origin ?? [0, 0, 0];
      const { a, b } = ends;
      const world: Vec3 = [a[0] + ((b[0] - a[0]) / frame.length) * along, a[1] + ((b[1] - a[1]) / frame.length) * along, rl];
      return { local: [world[0] - o[0], world[1] - o[1], world[2] - o[2]], world, object: null, kind: 'plane', label: `the section at RL ${rl.toFixed(2)}` };
    },
    snap: async (x, y, vertexPx, segmentPx, skip): Promise<SnapHit | null> => {
      const frame = this.frame();
      if (!frame) return null;
      const [along, rl] = this.cursor(x, y, frame);
      const metres = frame.across / this.viewport.renderer.domElement.clientWidth;
      return sectionSnap(along, rl, vertexPx * metres, segmentPx * metres, skip ? this.flat(skip) : null);
    },
    pick: async (x, y, px): Promise<string | null> => {
      const frame = this.frame();
      if (!frame) return null;
      const [along, rl] = this.cursor(x, y, frame);
      return sectionPick(along, rl, (px * frame.across) / this.viewport.renderer.domElement.clientWidth);
    },
  };

  private drawOverview(): void {
    if (!this.ends || !this.summary) return;
    const o = this.summary.origin ?? [0, 0, 0];
    const { a, b } = this.ends;
    this.overview.draw([a[0] - o[0], a[1] - o[1]], [b[0] - o[0], b[1] - o[1]], this.stepSize, this.moved);
  }

  /** Moves the plane `distance` square to itself, away from the viewer when positive. */
  async step(distance: number): Promise<void> {
    if (!this.ends) return;
    const { a, b } = this.ends;
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const [nx, ny] = [(-(b[1] - a[1]) / length) * distance, ((b[0] - a[0]) / length) * distance];
    this.ends = { a: [a[0] + nx, a[1] + ny], b: [b[0] + nx, b[1] + ny] };
    this.moved += distance;
    this.viewport.moveSection(nx, ny);
    this.drawOverview();
    await this.refresh();
  }

  /** Cuts again for what the project or plane now is. Calls while one is running collapse into one. */
  refresh(summary?: ProjectSummary): Promise<void> {
    if (summary) this.summary = summary;
    if (!this.ends) return Promise.resolve();
    if (this.fetching) {
      this.fetchAgain = true;
      return this.fetching;
    }
    this.fetching = this.fetchLines().finally(() => {
      this.fetching = null;
    });
    return this.fetching;
  }

  private async fetchLines(): Promise<void> {
    do {
      this.fetchAgain = false;
      const ends = this.ends;
      if (!ends) return;
      const start = performance.now();
      const lines = await sectionLines(ends.a, ends.b, STRING_WIDTH, this.filling, this.selected);
      if (this.ends) this.draw(lines);
      const frame = await this.viewport.nextFrame();
      this.lastMs = frame.submitted - start;
      this.lastEngineMs = lines.ms;
    } while (this.fetchAgain);
  }

  /** A scene-local point on the first solid cut, if any. */
  firstSolidPoint(): THREE.Vector3 | null {
    const line = [...this.lines.values()].find((l) => l.visible && (l.userData as { kind: SectionKind }).kind === 'solid');
    const start = line?.geometry.getAttribute('instanceStart');
    return start ? new THREE.Vector3(start.getX(0), start.getY(0), start.getZ(0)) : null;
  }

  close(): void {
    this.ends = null;
    this.group.visible = false;
    this.overview.hide();
    this.grid.hide();
    this.viewport.closeSection();
  }

  private draw(lines: SectionLines): void {
    this.lastCut = lines;
    const { groups, positions, fills } = lines;
    const layers = new Map(this.summary?.layers.map((l) => [l.id, l]));
    const seen = new Set<string>();
    let [from, fillFrom, unfilled] = [0, 0, 0];
    for (const { layer, kind, object, material, segments, triangles, unfilled: left } of groups) {
      const key = kind === 'selected' ? kind : `${layer} ${kind} ${object ?? ''} ${material}`;
      seen.add(key);
      const geometry = new LineSegmentsGeometry();
      geometry.setPositions(positions.slice(from, from + segments * 6));
      from += segments * 6;
      let line = this.lines.get(key);
      if (line) {
        line.geometry.dispose();
        line.geometry = geometry;
      } else {
        line = new LineSegments2(geometry, new THREE.Line2NodeMaterial({ depthTest: false, worldUnits: false }));
        line.userData = { layer, kind };
        line.renderOrder = ORDER[kind];
        line.frustumCulled = false;
        this.group.add(line);
        this.lines.set(key, line);
      }
      const layerColour = layers.get(layer)?.colour;
      const colour = kind === 'selected' ? SELECTED : (MATERIAL_COLOURS[material] ?? (layerColour ? colourOf(layerColour) : 0xffffff));
      (line.material as THREE.Line2NodeMaterial).color.setHex(colour);
      line.visible = true;
      if (triangles > 0) this.drawFill(key, fills.slice(fillFrom, fillFrom + triangles * 9), colour);
      else if (this.fills.has(key)) (this.fills.get(key) as THREE.Mesh).visible = false;
      fillFrom += triangles * 9;
      unfilled += left;
    }
    for (const [key, line] of this.lines) if (!seen.has(key)) line.visible = false;
    for (const [key, fill] of this.fills) if (!seen.has(key)) fill.visible = false;
    if (unfilled !== this.unfilled && unfilled > 0) this.onUnfilled?.(unfilled);
    this.unfilled = unfilled;
    this.reweigh();
  }

  private drawFill(key: string, positions: Float32Array, colour: number): void {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    let fill = this.fills.get(key);
    if (fill) {
      fill.geometry.dispose();
      fill.geometry = geometry;
    } else {
      const material = new THREE.MeshBasicNodeMaterial({ transparent: true, opacity: FILL_OPACITY, side: THREE.DoubleSide, depthTest: false, depthWrite: false });
      fill = new THREE.Mesh(geometry, material);
      fill.renderOrder = FILL_ORDER;
      fill.frustumCulled = false;
      this.group.add(fill);
      this.fills.set(key, fill);
    }
    (fill.material as THREE.MeshBasicNodeMaterial).color.setHex(colour);
    fill.visible = true;
  }

  private reweigh(): void {
    for (const line of this.lines.values()) {
      const { layer, kind } = line.userData as { layer: string; kind: SectionKind };
      (line.material as THREE.Line2NodeMaterial).linewidth = (kind === 'selected' ? null : this.weights.get(layer)) ?? WEIGHTS[kind];
    }
  }
}

/** The lowest and highest scene-local RL of the lines between two points, to frame that stretch. */
function rlBetween(positions: Float32Array, from: PlanPoint, to: PlanPoint, length: number): PlanPoint {
  const [dx, dy] = [(to[0] - from[0]) / length, (to[1] - from[1]) / length];
  let [low, high] = [Infinity, -Infinity];
  for (let k = 0; k < positions.length; k += 3) {
    const along = (positions[k] - from[0]) * dx + (positions[k + 1] - from[1]) * dy;
    if (along >= 0 && along <= length) {
      low = Math.min(low, positions[k + 2]);
      high = Math.max(high, positions[k + 2]);
    }
  }
  return low <= high ? [low, high] : [0, 100];
}

/** A snap's plan position, for the section's ends. */
export const planPoint = (p: Vec3): PlanPoint => [p[0], p[1]];
