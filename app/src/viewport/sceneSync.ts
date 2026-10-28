// Keeps the drawn surfaces (surfaces, solids, elevation grids) in line with the project summary.
// Geometry is fetched only when an object's geometry key changes, so a rename or a layer toggle
// never refetches a large mesh.
import type * as THREE from 'three/webgpu';
import { type ProjectSummary, stageObject } from '../bridge';
import { colourOf, loadStaged } from './load';
import type { Viewport } from './Viewport';

interface Shown {
  key: string;
  group: THREE.Group;
}

export class SceneSync {
  private shown = new Map<string, Shown>();
  private queue: Promise<number> = Promise.resolve(0);

  constructor(private viewport: Viewport) {}

  /** Applies summaries one at a time, in order. Resolves to the number of surfaces newly loaded. */
  apply(summary: ProjectSummary): Promise<number> {
    this.queue = this.queue.then(() => this.applyNow(summary)).catch((e) => {
      console.error('scene sync failed', e);
      return 0;
    });
    return this.queue;
  }

  get count(): number {
    return this.shown.size;
  }

  private async applyNow(summary: ProjectSummary): Promise<number> {
    const layers = new Map(summary.layers.map((l) => [l.id, l]));
    const wanted = new Map(summary.objects.filter((o) => o.drawn).map((o) => [o.id, o]));
    for (const [id, shown] of this.shown) {
      if (wanted.get(id)?.geometry_key !== shown.key) {
        this.viewport.removeMesh(shown.group);
        this.shown.delete(id);
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
      const staged = await stageObject(object.id);
      const colour = layer ? colourOf(layer.colour) : undefined;
      const { group } = await loadStaged(this.viewport, staged, { colour });
      group.visible = layer?.visible ?? true;
      this.shown.set(object.id, { key: object.geometry_key, group });
      loaded++;
    }
    return loaded;
  }
}
