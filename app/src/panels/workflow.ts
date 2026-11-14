/**
 * Workflows in words (decision 0065): how a step's input rule reads in the Workflow panel, and how ticking runs in the
 * Run log works, a Shift-click ticking every run between.
 */
import type { InputRule, LastStepRun, Workflow, WorkflowStep } from '../bridge';

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

/** The name of the case a workflow's steps are as they stand (decision 0066). */
export const BASE = 'Base';

/** A step as one case sees it: its last run that case's own. */
export function stepsOfCase(workflow: Workflow, name: string): WorkflowStep[] {
  if (name === BASE) return workflow.steps;
  const last = workflow.cases?.find((c) => c.name === name)?.last ?? {};
  return workflow.steps.map((s) => ({ ...s, last: last[String(s.id)] as LastStepRun | undefined }));
}

/** A row of the settings grid: one plain setting of one step. */
export interface SettingRow {
  step: number;
  /** Where the step is, from 1. */
  position: number;
  command: string;
  field: string;
  /** Base's value. */
  base: string | number | boolean;
}

/** Every plain setting (a number, text, or true or false) of every command step, in order: the grid's rows. */
export function settingRows(workflow: Workflow): SettingRow[] {
  const rows: SettingRow[] = [];
  workflow.steps.forEach((step, at) => {
    if (step.action.kind !== 'command') return;
    for (const [field, value] of Object.entries(step.action.settings ?? {})) {
      if (typeof value === 'number' || typeof value === 'string' || typeof value === 'boolean') rows.push({ step: step.id, position: at + 1, command: step.action.command, field, base: value });
    }
  });
  return rows;
}

/** What a case has in a cell: its own value, or undefined where it reads as Base. */
export function caseValue(workflow: Workflow, name: string, row: SettingRow): unknown {
  if (name === BASE) return row.base;
  return workflow.cases?.find((c) => c.name === name)?.settings?.[String(row.step)]?.[row.field];
}

/** A cell's text read as the setting's own type: null where it isn't one. */
export function readSetting(text: string, like: string | number | boolean): string | number | boolean | null {
  if (typeof like === 'number') {
    const n = Number(text.trim());
    return text.trim() !== '' && Number.isFinite(n) ? n : null;
  }
  if (typeof like === 'boolean') return text.trim() === 'true' ? true : text.trim() === 'false' ? false : null;
  return text;
}

/**
 * The workflow with one cell of the settings grid changed: Base's changes the step's own setting; a case's sets what the
 * case changes, and an empty cell takes the change away, so the case reads as Base again. Null where the text isn't a
 * value of the setting's type.
 */
export function withSetting(workflow: Workflow, name: string, row: SettingRow, text: string): Workflow | null {
  if (name !== BASE && text.trim() === '') {
    const cases = (workflow.cases ?? []).map((c) => {
      if (c.name !== name) return c;
      const settings = { ...(c.settings ?? {}) };
      const changes = { ...(settings[String(row.step)] ?? {}) };
      delete changes[row.field];
      if (Object.keys(changes).length === 0) delete settings[String(row.step)];
      else settings[String(row.step)] = changes;
      return { ...c, settings };
    });
    return { ...workflow, cases };
  }
  const value = readSetting(text, row.base);
  if (value === null) return null;
  if (name === BASE) {
    const steps = workflow.steps.map((s) => (s.id === row.step && s.action.kind === 'command' ? { ...s, action: { ...s.action, settings: { ...s.action.settings, [row.field]: value } } } : s));
    return { ...workflow, steps };
  }
  const cases = (workflow.cases ?? []).map((c) =>
    c.name !== name ? c : { ...c, settings: { ...(c.settings ?? {}), [String(row.step)]: { ...(c.settings?.[String(row.step)] ?? {}), [row.field]: value } } },
  );
  return { ...workflow, cases };
}
