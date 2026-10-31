// The section view: a thin, exact slice through two plan points, in place of the plan view. The engine
// cuts every visible layer and sends the lines as binary; this draws them with real line weights and
// steps the plane along. A section is view state: no command, nothing to undo.
import { LineSegments2 } from 'three/addons/lines/webgpu/LineSegments2.js';
import { LineSegmentsGeometry } from 'three/addons/lines/LineSegmentsGeometry.js';
import * as THREE from 'three/webgpu';
import { type ProjectSummary, type SectionLines, sectionLines, type SectionKind, type Vec3 } from '../bridge';
import { colourOf } from './load';
import type { Viewport } from './Viewport';

/** Strings this close to the plane (m) draw flat on it: a string crosses a thin slice, not lies in it. */
const STRING_WIDTH = 1;

/** Line weights by kind (px), until a layer sets its own. */
const WEIGHTS: Record<SectionKind, number> = { surface: 2.5, seam: 1.5, solid: 1.5, string: 2 };

/** Each kind's own lines draw over the next: strings, then seams, surfaces and solids. */
const ORDER: Record<SectionKind, number> = { solid: 10, surface: 11, seam: 12, string: 13 };

export type PlanPoint = [number, number];

export class SectionView {
  /** The section's ends in mine grid coordinates; null when no section is open. */
  private ends: { a: PlanPoint; b: PlanPoint } | null = null;
  private summary: ProjectSummary | null = null;
  private group = new THREE.Group();
  private lines = new Map<string, LineSegments2>();
  private weights = new Map<string, number>();
  private fetching: Promise<void> | null = null;
  private fetchAgain = false;
  /** The last section: request to frame submitted, and the engine's share of that (ms). */
  lastMs = 0;
  lastEngineMs = 0;

  constructor(private viewport: Viewport) {
    viewport.overlay.add(this.group);
  }

  get open(): boolean {
    return this.ends !== null;
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
    this.ends = { a, b };
    this.summary = summary;
    const lines = await sectionLines(a, b, STRING_WIDTH);
    if (!this.ends) return;
    this.draw(lines);
    const o = summary.origin ?? [0, 0, 0];
    const [from, to]: PlanPoint[] = [
      [a[0] - o[0], a[1] - o[1]],
      [b[0] - o[0], b[1] - o[1]],
    ];
    const length = Math.hypot(to[0] - from[0], to[1] - from[1]);
    this.viewport.showSection(from, to, [0, length], rlBetween(lines.positions, from, to, length));
    this.group.visible = true;
  }

  /** Moves the plane `distance` square to itself, away from the viewer when positive. */
  async step(distance: number): Promise<void> {
    if (!this.ends) return;
    const { a, b } = this.ends;
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const [nx, ny] = [(-(b[1] - a[1]) / length) * distance, ((b[0] - a[0]) / length) * distance];
    this.ends = { a: [a[0] + nx, a[1] + ny], b: [b[0] + nx, b[1] + ny] };
    this.viewport.moveSection(nx, ny);
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
      const lines = await sectionLines(ends.a, ends.b, STRING_WIDTH);
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
    this.viewport.closeSection();
  }

  private draw({ groups, positions }: SectionLines): void {
    const layers = new Map(this.summary?.layers.map((l) => [l.id, l]));
    const seen = new Set<string>();
    let from = 0;
    for (const { layer, kind, segments } of groups) {
      const key = `${layer} ${kind}`;
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
      const colour = layers.get(layer)?.colour;
      (line.material as THREE.Line2NodeMaterial).color.setHex(colour ? colourOf(colour) : 0xffffff);
      line.visible = true;
    }
    for (const [key, line] of this.lines) if (!seen.has(key)) line.visible = false;
    this.reweigh();
  }

  private reweigh(): void {
    for (const line of this.lines.values()) {
      const { layer, kind } = line.userData as { layer: string; kind: SectionKind };
      (line.material as THREE.Line2NodeMaterial).linewidth = this.weights.get(layer) ?? WEIGHTS[kind];
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
