/**
 * The side the app is on and the Design group whose tools the ribbon shows, each remembered for each project on this
 * machine (decisions 0076, 0108): by the project's path, with one for a project not saved yet. It's how the window
 * looks, like the layout, so it stays out of the project file. Kept free of React, so the tests can read it.
 */
import { type Group, type Side, STAGES } from '../tools.ts';

const STAGE_KEY = 'zd.ribbon.stage.v1';
const SIDE_KEY = 'zd.side.v1';
/** The most projects remembered; the oldest go first. */
const MOST = 50;

const isGroup = (value: unknown): value is Group => STAGES.some((s) => s.id === value);
// Both, remembered before it was taken out (decision 0109), reads as Design.
const isSide = (value: unknown): value is Side => value === 'design' || value === 'tables';

/** What's remembered under `key`, a project's path to its value, newest last; anything that isn't one dropped. */
function remembered<T>(storage: Pick<Storage, 'getItem'>, key: string, is: (v: unknown) => v is T): [string, T][] {
  try {
    const stored: unknown = JSON.parse(storage.getItem(key) ?? '[]');
    return Array.isArray(stored) ? stored.filter((e): e is [string, T] => Array.isArray(e) && typeof e[0] === 'string' && is(e[1])) : [];
  } catch {
    return [];
  }
}

function remember<T>(storage: Pick<Storage, 'getItem' | 'setItem'>, key: string, is: (v: unknown) => v is T, path: string | null, value: T): void {
  try {
    const others = remembered(storage, key, is).filter(([p]) => p !== (path ?? ''));
    storage.setItem(key, JSON.stringify([...others, [path ?? '', value]].slice(-MOST)));
  } catch {
    // Storage unavailable: it just isn't remembered.
  }
}

/** The Design group a project was last left on: Strings for one never opened here, or left on a stage there no longer is. */
export function stageOf(path: string | null, storage: Pick<Storage, 'getItem'> = localStorage): Group {
  return remembered(storage, STAGE_KEY, isGroup).find(([p]) => p === (path ?? ''))?.[1] ?? 'strings';
}

export function rememberStage(path: string | null, stage: Group, storage: Pick<Storage, 'getItem' | 'setItem'> = localStorage): void {
  remember(storage, STAGE_KEY, isGroup, path, stage);
}

/** The side a project was last left on: Design for one never opened here. */
export function sideOf(path: string | null, storage: Pick<Storage, 'getItem'> = localStorage): Side {
  return remembered(storage, SIDE_KEY, isSide).find(([p]) => p === (path ?? ''))?.[1] ?? 'design';
}

export function rememberSide(path: string | null, side: Side, storage: Pick<Storage, 'getItem' | 'setItem'> = localStorage): void {
  remember(storage, SIDE_KEY, isSide, path, side);
}
