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
