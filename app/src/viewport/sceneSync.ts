// Keeps the drawn surfaces (surfaces, solids, elevation grids, seam horizons) in line with the
// project summary. Geometry is fetched only when an object's geometry key changes, so a rename or a
// layer toggle never refetches a large mesh. A seam model shows one horizon at a time, its roof
// coloured by thickness; choosing another horizon reloads that model alone. A layer's solids draw as
// one mesh, so a pit's tens of thousands of them are a few chunks; any change to one, hiding it included,
// reloads the layer, and the old mesh stays up until the new one is ready. Anything else hidden on its
// own is only switched off.
import * as THREE from 'three/webgpu';
import { type ObjectSummary, type ProjectSummary, type StagedHorizon, stageLayerSolids, stageObject, stageSeamHorizon } from '../bridge';
import { colourOf, loadStaged } from './load';
import type { Viewport } from './Viewport';

interface Shown {
  key: string;
  group: THREE.Group;
  /** A seam model's horizon, as loaded. */
  seam?: SeamShown;
}

/** The `shown` key of a layer's solids. */
const solidsOf = (layer: string) => `solids:${layer}`;

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
  /** The newest summary asked for: any older one still waiting is skipped. */
  private latest: ProjectSummary | null = null;
  /** The horizon shown for each seam model: the first working one until another is chosen. */
  private horizons = new Map<string, string>();
  private seams: SeamShown[] = [];
  /** The seam models on show changed: one loaded or hidden, or a new name or horizon. */
  onSeams?: (showing: SeamShown[]) => void;
  onError?: (message: string) => void;

  constructor(private viewport: Viewport) {}

  /** Applies summaries one at a time, in order. Resolves to the number of surfaces newly loaded. */
  apply(summary: ProjectSummary): Promise<number> {
    this.latest = summary;
    this.queue = this.queue.then(() => (summary === this.latest ? this.applyNow(summary) : 0)).catch((e) => {
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
    const wanted = new Map(summary.objects.filter((o) => (o.drawn && o.kind !== 'solid') || seam(o.kind)).map((o) => [o.id, o]));
    const solids = new Map(summary.solid_layers.map((s) => [solidsOf(s.layer), s.key]));
    const keyOf = (id: string) => {
      if (solids.has(id)) return solids.get(id);
      const o = wanted.get(id);
      if (!o) return undefined;
      return seam(o.kind) ? `${o.geometry_key}#${this.horizons.get(id) ?? ''}` : o.geometry_key;
    };
    const on = (o: ObjectSummary) => (layers.get(o.layer)?.visible ?? true) && !o.hidden;
    for (const [id, shown] of this.shown) {
      if (keyOf(id) !== shown.key && !solids.has(id)) {
        this.viewport.removeMesh(shown.group);
        this.shown.delete(id);
        if (!wanted.has(id)) this.horizons.delete(id);
      }
    }
    let loaded = 0;
    for (const object of wanted.values()) {
      const layer = layers.get(object.layer);
      const shown = this.shown.get(object.id);
      if (shown) {
        shown.group.visible = on(object);
        continue;
      }
      // Loaded when first shown: a model hidden in a saved project shouldn't read its grids.
      if (object.hidden) continue;
      let group: THREE.Group;
      let view: SeamShown | undefined;
      if (seam(object.kind)) {
        try {
          const result = await stageSeamHorizon(object.id, this.horizons.get(object.id) ?? null);
          this.horizons.set(object.id, result.horizon);
          group = (await loadStaged(this.viewport, result.staged)).group;
          view = { id: object.id, name: object.name, result };
        } catch (e) {
          // Grids moved or unreadable: say so, and don't retry until something changes.
          this.onError?.(`${object.name}: ${String(e)}`);
          group = new THREE.Group();
        }
      } else {
        const staged = await stageObject(object.id);
        group = (await loadStaged(this.viewport, staged, { colour: layer ? colourOf(layer.colour) : undefined })).group;
      }
      group.visible = on(object);
      this.shown.set(object.id, { key: keyOf(object.id)!, group, seam: view });
      loaded++;
    }
    for (const { layer: id, key } of summary.solid_layers) {
      const layer = layers.get(id);
      const shown = this.shown.get(solidsOf(id));
      if (shown?.key === key) {
        shown.group.visible = layer?.visible ?? true;
        continue;
      }
      const staged = await stageLayerSolids(id);
      const { group } = await loadStaged(this.viewport, staged, { colour: layer ? colourOf(layer.colour) : undefined });
      group.visible = layer?.visible ?? true;
      if (shown) this.viewport.removeMesh(shown.group);
      this.shown.set(solidsOf(id), { key, group });
      loaded++;
    }
    this.reportSeams(summary);
    return loaded;
  }

  /** Tells the app which seam models are on show, under their current names, if that has changed. */
  private reportSeams(summary: ProjectSummary): void {
    const showing = summary.objects.flatMap((o) => {
      const shown = this.shown.get(o.id);
      return shown?.seam && shown.group.visible ? [{ ...shown.seam, name: o.name }] : [];
    });
    const same = showing.length === this.seams.length && showing.every((s, k) => s.name === this.seams[k].name && s.result === this.seams[k].result);
    if (same) return;
    this.seams = showing;
    this.onSeams?.(showing);
  }
}
