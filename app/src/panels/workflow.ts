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

/** A rule's filter as the filter box has it (decision 0088): each value as its text, a number or true as it reads. */
export function whereOf(rule: InputRule): Record<string, string> {
  return Object.fromEntries(Object.entries(rule.where ?? {}).map(([key, value]) => [key, String(value)]));
}

/**
 * The workflow with one step input's filter set (decision 0088, workstream 5): a value whose text is as saved keeps the
 * type it was saved as (a number captured stays that value), the rest are filter text; an empty filter takes `where`
 * away. A step's filter is the step's own, so every case reads it.
 */
export function withWhere(workflow: Workflow, step: number, input: string, when: Record<string, string>): Workflow {
  const steps = workflow.steps.map((s) => {
    if (s.id !== step || s.action.kind !== 'command' || !s.action.inputs[input]) return s;
    const rule = s.action.inputs[input];
    const where = Object.fromEntries(
      Object.entries(when).map(([key, text]) => {
        const was = rule.where?.[key];
        return [key, was !== undefined && String(was) === text ? was : text];
      }),
    );
    const next: InputRule = { ...rule, where };
    if (Object.keys(where).length === 0) delete next.where;
    return { ...s, action: { ...s.action, inputs: { ...s.action.inputs, [input]: next } } };
  });
  return { ...workflow, steps };
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

/** What the view draws: a run's result is the last of these it made, never a table or a setting it wrote. */
const DRAWN = new Set(['string', 'surface', 'solid', 'grid', 'seam_model']);

/**
 * A run's result, for Tidy after a run (app review brief, 17 Nov, item 4): what the last step that made something drawn
 * made, walking back from the end. A reserves table at the end, or a delete just before a pause, leaves the blocks or
 * the strips before it on show, not an empty view. `made` is each step's, in the order they ran.
 */
export function tidyResult(made: string[][], objects: { id: string; kind: string }[]): string[] {
  const kindOf = new Map(objects.map((o) => [o.id, o.kind]));
  for (let k = made.length - 1; k >= 0; k--) {
    const drawn = made[k].filter((id) => DRAWN.has(kindOf.get(id) ?? ''));
    if (drawn.length > 0) return drawn;
  }
  return [];
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

/** One plain setting of one step, as it shows under the step. */
export interface SettingRow {
  step: number;
  /** Where the step is, from 1. */
  position: number;
  command: string;
  field: string;
  /** Base's value. */
  base: string | number | boolean;
}

/** Every plain setting (a number, text, or true or false) of every command step, in order. */
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

/** One step's plain settings, as rows: what shows under the step in the Workflow panel. */
export function settingsOfStep(workflow: Workflow, step: number): SettingRow[] {
  return settingRows(workflow).filter((r) => r.step === step);
}

/** How many settings of each step a case changes, by step id: none for Base, which is the steps as they stand. */
export function changesOfCase(workflow: Workflow, name: string): Map<number, number> {
  const changes = new Map<number, number>();
  if (name === BASE) return changes;
  for (const [step, set] of Object.entries(workflow.cases?.find((c) => c.name === name)?.settings ?? {})) {
    const n = Object.keys(set ?? {}).length;
    if (n > 0) changes.set(Number(step), n);
  }
  return changes;
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
 * The workflow with one setting changed under its step: Base's changes the step's own setting; a case's sets what the
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
