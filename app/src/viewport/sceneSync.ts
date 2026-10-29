// Keeps the drawn surfaces (surfaces, solids, elevation grids, seam horizons) in line with the
// project summary. Geometry is fetched only when an object's geometry key changes, so a rename or a
// layer toggle never refetches a large mesh. A seam model shows one horizon at a time, its roof
// coloured by thickness; choosing another horizon reloads that model alone.
import * as THREE from 'three/webgpu';
import { type ProjectSummary, type StagedHorizon, stageObject, stageSeamHorizon } from '../bridge';
import { colourOf, loadStaged } from './load';
import type { Viewport } from './Viewport';

interface Shown {
  key: string;
  group: THREE.Group;
}

/** A seam model's horizon as the viewport shows it. */
export interface SeamShown {
  id: string;
  name: string;
  result: StagedHorizon;
}

export class SceneSync {
  private shown = new Map<string, Shown>();
  private queue: Promise<number> = Promise.resolve(0);
  private last: ProjectSummary | null = null;
  /** The horizon shown for each seam model: the first working one until another is chosen. */
  private horizons = new Map<string, string>();
  /** A seam model's horizon was loaded, or (null) the model is no longer drawn. */
  onSeam?: (id: string, shown: SeamShown | null) => void;
  onError?: (message: string) => void;

  constructor(private viewport: Viewport) {}

  /** Applies summaries one at a time, in order. Resolves to the number of surfaces newly loaded. */
  apply(summary: ProjectSummary): Promise<number> {
    this.queue = this.queue.then(() => this.applyNow(summary)).catch((e) => {
      console.error('scene sync failed', e);
      return 0;
    });
    return this.queue;
  }

  /** Shows another horizon of a seam model. */
  showHorizon(id: string, code: string): Promise<number> {
    this.horizons.set(id, code);
    return this.last ? this.apply(this.last) : Promise.resolve(0);
  }

  get count(): number {
    return this.shown.size;
  }

  private async applyNow(summary: ProjectSummary): Promise<number> {
    this.last = summary;
    const layers = new Map(summary.layers.map((l) => [l.id, l]));
    const seam = (kind: string) => kind === 'seam_model';
    const wanted = new Map(summary.objects.filter((o) => o.drawn || seam(o.kind)).map((o) => [o.id, o]));
    const keyOf = (id: string) => {
      const o = wanted.get(id);
      if (!o) return undefined;
      return seam(o.kind) ? `${o.geometry_key}#${this.horizons.get(id) ?? ''}` : o.geometry_key;
    };
    for (const [id, shown] of this.shown) {
      if (keyOf(id) !== shown.key) {
        this.viewport.removeMesh(shown.group);
        this.shown.delete(id);
        if (!wanted.has(id)) {
          this.horizons.delete(id);
          this.onSeam?.(id, null);
        }
      }
    }
    let loaded = 0;
    for (const object of wanted.values()) {
      const layer = layers.get(object.layer);
      const shown = this.shown.get(object.id);
      if (shown) {
        shown.group.visible = layer?.visible ?? true;
        continue;
      }
      let group: THREE.Group;
      if (seam(object.kind)) {
        try {
          const result = await stageSeamHorizon(object.id, this.horizons.get(object.id) ?? null);
          this.horizons.set(object.id, result.horizon);
          group = (await loadStaged(this.viewport, result.staged)).group;
          this.onSeam?.(object.id, { id: object.id, name: object.name, result });
        } catch (e) {
          // Grids moved or unreadable: say so, and don't retry until something changes.
          this.onError?.(`${object.name}: ${String(e)}`);
          group = new THREE.Group();
        }
      } else {
        const staged = await stageObject(object.id);
        group = (await loadStaged(this.viewport, staged, { colour: layer ? colourOf(layer.colour) : undefined })).group;
      }
      group.visible = layer?.visible ?? true;
      this.shown.set(object.id, { key: keyOf(object.id)!, group });
      loaded++;
    }
    return loaded;
  }
}
