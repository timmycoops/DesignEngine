// Keeps the drawn surfaces (surfaces, solids, elevation grids, seam horizons) in line with the
// project summary. Geometry is fetched only when an object's geometry key changes, so a rename or a
// layer toggle never refetches a large mesh. A seam model shows one horizon at a time, its roof
// coloured by thickness; choosing another horizon reloads that model alone. A layer's solids draw as
// one mesh, so a pit's tens of thousands of them are a few chunks; any change to one, hiding it included,
// reloads the layer, and the old mesh stays up until the new one is ready. Anything else hidden on its
// own is only switched off. How each one looks (by RL or its layer's colour, and how see-through) is
// uniforms on its material, so a change of look never refetches anything.
import * as THREE from 'three/webgpu';
import { type Contours, layerOpacityOf, lookOf, type ObjectSummary, type ProjectSummary, type StagedHorizon, stageLayerSolids, stageObject, stageSeamHorizon } from '../bridge';
import { colourOf, loadStaged, SELECTED } from './load';
import { type Bands, bandsFor } from './rlColour';
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

/** What's coloured by RL on show, for the legend: each object's bands, or one set shared by all. */
export interface RlShown {
  shared: boolean;
  /** Under each object's current name. */
  objects: { id: string; name: string; bands: Bands }[];
}

export const DEFAULT_CONTOURS: Contours = { interval: null, edge_lines: false, shared: false };

export class SceneSync {
  private shown = new Map<string, Shown>();
  private queue: Promise<number> = Promise.resolve(0);
  /** The newest summary asked for: any older one still waiting is skipped. */
  private latest: ProjectSummary | null = null;
  /** The horizon shown for each seam model: the first working one until another is chosen. */
  private horizons = new Map<string, string>();
  private seams: SeamShown[] = [];
  private selected: string | null = null;
  /** The object lit up: its own surface, or for a solid a copy over its layer's mesh. */
  private lit: { id: string; key: string; group: THREE.Group; copy: boolean } | null = null;
  /** The seam models on show changed: one loaded or hidden, or a new name or horizon. */
  onSeams?: (showing: SeamShown[]) => void;
  /** What's coloured by RL on show changed, or its bands did. */
  onRl?: (shown: RlShown) => void;
  private rl = '';
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
    return this.latest ? this.apply(this.latest) : Promise.resolve(0);
  }

  /** Lights up the selected object, or with null nothing. */
  select(id: string | null): Promise<number> {
    this.selected = id;
    return this.latest ? this.apply(this.latest) : Promise.resolve(0);
  }

  /** The nearest seam horizon on show at a point on screen, and how far along the ray it is. */
  seamAt(x: number, y: number): { id: string; distance: number } | null {
    let nearest: { id: string; distance: number } | null = null;
    for (const [id, shown] of this.shown) {
      if (!shown.seam || !shown.group.visible) continue;
      const distance = this.viewport.distanceTo(shown.group, x, y);
      if (distance !== null && (!nearest || distance < nearest.distance)) nearest = { id, distance };
    }
    return nearest;
  }

  get count(): number {
    return this.shown.size;
  }

  private async applyNow(summary: ProjectSummary): Promise<number> {
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
    // Before anything loads, so a surface's first frame has its bands on round RLs.
    this.viewport.originZ = summary.origin?.[2] ?? 0;
    for (const [id, shown] of this.shown) {
      if (keyOf(id) !== shown.key && !solids.has(id)) {
        this.viewport.removeMesh(shown.group);
        this.shown.delete(id);
        if (!wanted.has(id)) this.horizons.delete(id);
      }
    }
    let loaded = 0;
    for (const object of wanted.values()) {
      // A newer summary is waiting: it is applied next, so stop loading this one's surfaces.
      if (summary !== this.latest) return loaded;
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
        let staged: Awaited<ReturnType<typeof stageObject>>;
        try {
          staged = await stageObject(object.id);
        } catch (e) {
          // Deleted by a run since this summary was taken (a repair replacing a surface while
          // the import's were still loading): the summary that run returns says so.
          if (String(e).includes('no such object')) continue;
          throw e;
        }
        const byRl = lookOf(summary.display, object.id).colouring === 'by_rl';
        group = (await loadStaged(this.viewport, staged, { colour: layer ? colourOf(layer.colour) : undefined, byRl })).group;
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
      const { group } = await loadStaged(this.viewport, staged, { colour: layer ? colourOf(layer.colour) : undefined, byRl: false });
      group.visible = layer?.visible ?? true;
      if (shown) this.viewport.removeMesh(shown.group);
      this.shown.set(solidsOf(id), { key, group });
      loaded++;
    }
    this.reportSeams(summary);
    this.restyle(summary);
    await this.light(summary, on).catch((e) => this.onError?.(String(e)));
    return loaded;
  }

  /**
   * Gives everything drawn its look: colour by RL in its bands or its layer's colour, and its opacity,
   * its own times its layer's. Bands are over each object's own RLs, or over everything coloured by RL
   * on show when they're shared, so an RL is one colour on every surface.
   */
  private restyle(summary: ProjectSummary): void {
    const { display } = summary;
    const contours = display?.contours ?? DEFAULT_CONTOURS;
    const originZ = this.viewport.originZ;
    const layers = new Map(summary.layers.map((l) => [l.id, l]));
    const surfaces = summary.objects.flatMap((object) => {
      const shown = this.shown.get(object.id);
      return shown && !shown.seam ? [{ object, group: shown.group, range: this.viewport.zRange(shown.group) }] : [];
    });
    const byRl = surfaces.filter((s) => s.group.visible && s.range && lookOf(display, s.object.id).colouring === 'by_rl');
    const shared: [number, number] | null =
      contours.shared && byRl.length > 0 ? [Math.min(...byRl.map((s) => s.range![0])), Math.max(...byRl.map((s) => s.range![1]))] : null;
    const bandsOf = (range: [number, number]) => bandsFor(range[0] + originZ, range[1] + originZ, originZ, contours.interval);
    for (const { object, group, range } of surfaces) {
      const look = lookOf(display, object.id);
      const layer = layers.get(object.layer);
      this.viewport.setLook(group, {
        colour: layer ? colourOf(layer.colour) : 0xffffff,
        byRl: look.colouring === 'by_rl',
        bands: range ? bandsOf(shared ?? range) : undefined,
        lines: contours.edge_lines,
        opacity: look.opacity * layerOpacityOf(display, object.layer),
      });
    }
    for (const [id, shown] of this.shown) {
      const seam = shown.seam && summary.objects.find((o) => o.id === id);
      if (seam) this.viewport.setLook(shown.group, { colour: 0xffffff, byRl: false, lines: false, opacity: lookOf(display, id).opacity * layerOpacityOf(display, seam.layer) });
    }
    for (const { layer: id } of summary.solid_layers) {
      const shown = this.shown.get(solidsOf(id));
      const layer = layers.get(id);
      if (shown) this.viewport.setLook(shown.group, { colour: layer ? colourOf(layer.colour) : 0xffffff, byRl: false, lines: false, opacity: layerOpacityOf(display, id) });
    }
    const report: RlShown = { shared: shared !== null, objects: byRl.map((s) => ({ id: s.object.id, name: s.object.name, bands: bandsOf(shared ?? s.range!) })) };
    const key = JSON.stringify(report);
    if (key !== this.rl) {
      this.rl = key;
      this.onRl?.(report);
    }
  }

  /** Brings the highlight in line with what is selected and how it is drawn now. */
  private async light(summary: ProjectSummary, on: (o: ObjectSummary) => boolean): Promise<void> {
    const object = this.selected ? summary.objects.find((o) => o.id === this.selected) : undefined;
    const lit = this.lit;
    if (lit && lit.copy && object?.id === lit.id && object.geometry_key === lit.key) {
      lit.group.visible = on(object);
      return;
    }
    if (lit && !lit.copy && object?.id === lit.id && this.shown.get(lit.id)?.group === lit.group) return;
    if (lit) {
      if (lit.copy) this.viewport.removeMesh(lit.group);
      else this.viewport.setLit(lit.group, null);
      this.lit = null;
    }
    if (!object) return;
    if (object.kind === 'solid') {
      if (!on(object)) return;
      const staged = await stageObject(object.id);
      const { group } = await loadStaged(this.viewport, staged, { colour: SELECTED, over: true });
      this.lit = { id: object.id, key: object.geometry_key, group, copy: true };
      return;
    }
    const group = this.shown.get(object.id)?.group;
    if (group) {
      this.viewport.setLit(group, SELECTED);
      this.lit = { id: object.id, key: object.geometry_key, group, copy: false };
    }
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
