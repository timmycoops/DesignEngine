// How the mouse and keys move the view (decision 0044, section 7): Spry's mouse layout or ours, and the view's keys,
// each changeable. Saved on this machine, as the panel layout is, not in the project. Kept free of React.
import { TOOLS } from '../tools';

/** Ours: left drag turns, right drag pans, middle drag dollies. Spry's: left is a click or a box, middle (or Space) pans, middle and left (or Z) turn. */
export type MouseLayout = 'ours' | 'spry';

/** The view's keys: two held while a left drag moves the view, four pressed. */
export type ViewKey = 'turn' | 'pan' | 'selection' | 'extents' | 'plan' | 'orthographic';

export const VIEW_KEYS: readonly ViewKey[] = ['turn', 'pan', 'selection', 'extents', 'plan', 'orthographic'];

export interface ViewControls {
  layout: MouseLayout;
  /** Each key as `KeyboardEvent.key` gives it, lower case: ' ' is Space. */
  keys: Record<ViewKey, string>;
}

export const VIEW_KEY_NAMES: Record<ViewKey, string> = {
  turn: 'Hold to turn about the cursor',
  pan: 'Hold to pan',
  selection: "Zoom to what's selected",
  extents: 'Zoom to everything',
  plan: 'Plan or 3D',
  orthographic: 'Orthographic or perspective',
};

export const DEFAULT_VIEW_CONTROLS: ViewControls = {
  layout: 'ours',
  keys: { turn: 'z', pan: ' ', selection: 'f', extents: 'e', plan: 't', orthographic: 'p' },
};

/** The tools in the list whose shortcut is one of the view's keys. */
export const TOOL_VIEW_KEYS: Readonly<Record<string, ViewKey>> = {
  'view.zoom_selection': 'selection',
  'view.zoom_all': 'extents',
  'view.plan': 'plan',
  'view.orthographic': 'orthographic',
};

const STORE_KEY = 'zd.view.controls';
/** Sent on the window when the controls change, so the view bar's tips and the tool list's keys follow. */
export const VIEW_CONTROLS_CHANGED = 'zd-view-controls-changed';

/** Keys a view key can't be, whatever the tool list says: the ones that finish, cancel, delete or step a section. */
const FIXED_KEYS: Record<string, string> = {
  enter: 'Enter finishes',
  escape: 'Esc takes back a step',
  backspace: 'Backspace takes back a point',
  delete: 'Delete deletes what is selected',
  tab: 'Tab moves between boxes',
  '[': '[ steps a section back',
  ']': '] steps a section on',
};

let current: ViewControls | null = null;

function read(): ViewControls {
  try {
    const saved = JSON.parse(globalThis.localStorage?.getItem(STORE_KEY) ?? 'null') as Partial<ViewControls> | null;
    if (!saved) return structuredClone(DEFAULT_VIEW_CONTROLS);
    const layout = saved.layout === 'spry' ? 'spry' : 'ours';
    const keys = { ...DEFAULT_VIEW_CONTROLS.keys };
    for (const which of VIEW_KEYS) {
      const key = saved.keys?.[which];
      if (typeof key === 'string' && key.length > 0) keys[which] = key;
    }
    // A saved set that clashes (a tool took a key since) falls back to the defaults for the keys.
    const clash = VIEW_KEYS.some((which) => keyClash(which, keys[which], { layout, keys }) !== null);
    return { layout, keys: clash ? { ...DEFAULT_VIEW_CONTROLS.keys } : keys };
  } catch {
    return structuredClone(DEFAULT_VIEW_CONTROLS);
  }
}

/** The controls in use. */
export function viewControls(): ViewControls {
  current ??= read();
  return current;
}

/** Sets the controls, saves them on this machine, and tells the window. */
export function setViewControls(next: ViewControls): void {
  current = { layout: next.layout, keys: { ...next.keys } };
  try {
    globalThis.localStorage?.setItem(STORE_KEY, JSON.stringify(current));
  } catch {
    // Not saved: it still holds for this run.
  }
  globalThis.dispatchEvent?.(new Event(VIEW_CONTROLS_CHANGED));
}

/** A key as people read it: Space, or the letter in capitals. */
export function keyLabel(key: string): string {
  return key === ' ' ? 'Space' : key.length === 1 ? key.toUpperCase() : key[0].toUpperCase() + key.slice(1);
}

/** The key set for `which`, as people read it. */
export function viewKeyLabel(which: ViewKey): string {
  return keyLabel(viewControls().keys[which]);
}

/** A tool's shortcut as it stands: a view key as set, else the tool list's own. */
export function toolShortcut(tool: { key: string; shortcut?: string }): string | undefined {
  const which = TOOL_VIEW_KEYS[tool.key];
  return which ? viewKeyLabel(which) : tool.shortcut;
}

/** A key press is `which`: the key alone, with no Ctrl, Alt or Meta (Shift is the caller's to judge). */
export function isViewKey(e: KeyboardEvent, which: ViewKey): boolean {
  return !e.ctrlKey && !e.metaKey && !e.altKey && e.key.toLowerCase() === viewControls().keys[which];
}

/**
 * Why `key` can't be the key for `which` in `controls`, or null if it can: it's another view key, a key with its own
 * job (Enter, Esc, Backspace, Delete, Tab, [ and ]), a tool's shortcut, or the first letter of a two-letter one (0080).
 */
export function keyClash(which: ViewKey, key: string, controls: ViewControls): string | null {
  const k = key.toLowerCase();
  if (k.length !== 1 && !(k in FIXED_KEYS)) return `${keyLabel(key)} can't be a view key: a letter, a digit, Space or a mark`;
  if (k in FIXED_KEYS) return `${keyLabel(key)} is taken: ${FIXED_KEYS[k]}`;
  for (const other of VIEW_KEYS) {
    if (other !== which && controls.keys[other] === k) return `${keyLabel(key)} is taken: ${VIEW_KEY_NAMES[other]}`;
  }
  for (const tool of TOOLS) {
    if (!tool.shortcut || TOOL_VIEW_KEYS[tool.key]) continue;
    const shortcut = tool.shortcut.toLowerCase();
    if (shortcut === k) return `${keyLabel(key)} is taken: ${tool.label}`;
    if (/^[a-z]{2}$/.test(shortcut) && shortcut[0] === k) return `${keyLabel(key)} is taken: it starts ${tool.shortcut}, ${tool.label}`;
  }
  return null;
}
