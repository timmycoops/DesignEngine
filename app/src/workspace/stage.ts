/**
 * The stage whose tools the ribbon shows, remembered for each project on this machine (decision 0076): by the project's
 * path, with one for a project not saved yet. It's how the window looks, like the layout, so it stays out of the project
 * file. Kept free of React, so the tests can read it.
 */
import { STAGES, type Stage } from '../tools.ts';

type StageId = Exclude<Stage, 'always'>;

const KEY = 'zd.ribbon.stage.v1';
/** The most projects remembered; the oldest go first. */
const MOST = 50;

const isStage = (value: unknown): value is StageId => STAGES.some((s) => s.id === value);

/** The remembered stages, a project's path to its stage, newest last. */
function remembered(storage: Pick<Storage, 'getItem'>): [string, StageId][] {
  try {
    const stored: unknown = JSON.parse(storage.getItem(KEY) ?? '[]');
    return Array.isArray(stored) ? stored.filter((e): e is [string, StageId] => Array.isArray(e) && typeof e[0] === 'string' && isStage(e[1])) : [];
  } catch {
    return [];
  }
}

/** The stage a project was last left on: Project for one never opened here. */
export function stageOf(path: string | null, storage: Pick<Storage, 'getItem'> = localStorage): StageId {
  return remembered(storage).find(([p]) => p === (path ?? ''))?.[1] ?? 'project';
}

export function rememberStage(path: string | null, stage: StageId, storage: Pick<Storage, 'getItem' | 'setItem'> = localStorage): void {
  try {
    const others = remembered(storage).filter(([p]) => p !== (path ?? ''));
    storage.setItem(KEY, JSON.stringify([...others, [path ?? '', stage]].slice(-MOST)));
  } catch {
    // Storage unavailable: the stage just isn't remembered.
  }
}
