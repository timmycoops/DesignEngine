/**
 * Workflows in words (decision 0065): how a step's input rule reads in the Workflow panel, and how ticking runs in the
 * Run log works, a Shift-click ticking every run between.
 */
import type { InputRule, Workflow } from '../bridge';

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
const kindWord = (kind: string) => kind.replace('_', ' ');

/** A rule in words, as the engine says it in a refusal. */
export function ruleWords(workflow: Workflow, rule: InputRule): string {
  const what = rule.kind ? `${kindWord(rule.kind)}s` : 'objects';
  const one = rule.kind ? kindWord(rule.kind) : 'object';
  const from = rule.from;
  let said: string;
  if ('step' in from) {
    const at = workflow.steps.findIndex((s) => s.id === from.step);
    said = at >= 0 ? `the ${what} step ${at + 1} made` : `the ${what} a step no longer there made`;
  } else if ('layer' in from) said = `the ${what} on layer '${from.layer}'`;
  else if ('named' in from) said = `the ${one} named '${from.named}'`;
  else said = `${plural(from.picked.length, one)} as picked`;
  for (const [key, value] of Object.entries(rule.where ?? {})) said += ` where ${key} is ${String(value)}`;
  return said;
}

/**
 * Runs ticked after a click on run `index`: it turns over, or with `range` every tickable run between it and the last
 * clicked turns as it does. Runs that can't be captured are never ticked.
 */
export function tickRuns(ticked: ReadonlySet<number>, tickable: ReadonlySet<number>, index: number, last: number | null, range: boolean): Set<number> {
  const next = new Set(ticked);
  const on = !ticked.has(index);
  const [lo, hi] = range && last !== null ? [Math.min(last, index), Math.max(last, index)] : [index, index];
  for (let i = lo; i <= hi; i++) {
    if (!tickable.has(i)) continue;
    if (on) next.add(i);
    else next.delete(i);
  }
  return next;
}

/** An object as the layer logic needs it: its id and its layer's id. */
export interface Placed {
  id: string;
  layer: string;
}

/** Layers to show and to hide, by id. */
export interface LayerChange {
  show: string[];
  hide: string[];
}

/**
 * The layers that tidy a workflow's run (option B): the layers holding what its last step made are shown, and the layers
 * holding only what the other steps made are hidden, so the run's inputs and its result are what's on screen. A layer
 * holding anything no step made (a toe drawn by hand, the grids) is left as it is.
 */
export function tidyLayers(made: string[][], result: string[], objects: Placed[]): LayerChange {
  const byStep = new Set(made.flat());
  const layerOf = new Map(objects.map((o) => [o.id, o.layer]));
  const shown = new Set(result.map((id) => layerOf.get(id)).filter((l): l is string => l !== undefined));
  const holdsOthers = new Set(objects.filter((o) => !byStep.has(o.id)).map((o) => o.layer));
  const stepLayers = new Set([...byStep].map((id) => layerOf.get(id)).filter((l): l is string => l !== undefined));
  const hide = [...stepLayers].filter((l) => !shown.has(l) && !holdsOthers.has(l));
  return { show: [...shown], hide };
}

/** The layers that show only what one step made (option A): its layers on, every other layer on show off. */
export function onlyLayers(made: string[], objects: Placed[], layers: { id: string; visible: boolean }[]): LayerChange {
  const layerOf = new Map(objects.map((o) => [o.id, o.layer]));
  const show = new Set(made.map((id) => layerOf.get(id)).filter((l): l is string => l !== undefined));
  return { show: [...show], hide: layers.filter((l) => l.visible && !show.has(l.id)).map((l) => l.id) };
}
