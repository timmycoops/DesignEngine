/**
 * The app's look, light or dark (Two sides plan, decision E and workstream 1): light by default, dark as a setting
 * saved on this machine, like the view controls. The panels follow the `dark` class on the root element; the view's
 * own colours, which three.js and the canvases draw, are read from `view` when they're used, so a change of look
 * carries through without opening the project again.
 */

export type Look = 'light' | 'dark';

export interface ViewColours {
  /** The view's ground. */
  background: number;
  /** A line or handle with no colour of its own: dark on light, white on dark. */
  ink: number;
  /** What's picked: the gold, a deep amber on light so a thin line still reads on the pale ground. */
  selected: number;
  /** A string as it's being drawn or moved. */
  preview: number;
  /** The section guide's dashes, and the line under them that keeps them apart from what's behind. */
  sectionGuide: number;
  sectionGuideUnder: number;
  /** The section's grid lines, as CSS. */
  gridLine: string;
  /** Where a snap lands, by what it landed on. */
  snap: { vertex: number; segment: number; intersection: number; surface: number; plane: number };
}

const PALETTES: Record<Look, ViewColours> = {
  light: {
    background: 0xe8ecf1,
    ink: 0x27313d,
    selected: 0xc28400,
    preview: 0x0a7fc2,
    sectionGuide: 0x27313d,
    sectionGuideUnder: 0xf4f6f8,
    gridLine: 'rgba(30, 40, 55, 0.13)',
    snap: { vertex: 0x14904a, segment: 0x0a7fc2, intersection: 0xc0187a, surface: 0x27313d, plane: 0xd9730d },
  },
  dark: {
    background: 0x161e29,
    ink: 0xffffff,
    selected: 0xffd23f,
    preview: 0x66d9ff,
    sectionGuide: 0xe9ecef,
    sectionGuideUnder: 0x161e29,
    gridLine: 'rgba(255, 255, 255, 0.1)',
    snap: { vertex: 0x7dfca0, segment: 0x66d9ff, intersection: 0xff7ad9, surface: 0xffffff, plane: 0xffb347 },
  },
};

const KEY = 'zd.look';

function saved(): Look {
  try {
    return localStorage.getItem(KEY) === 'dark' ? 'dark' : 'light';
  } catch {
    return 'light';
  }
}

let current: Look = saved();

/** The view's colours for the look in use. Read when drawing, never copied into a constant. */
export const view: ViewColours = { ...PALETTES[current], snap: { ...PALETTES[current].snap } };

const listeners = new Set<(look: Look) => void>();

/** The look in use. */
export const lookNow = (): Look => current;

/** Puts the root element's class in step with the look; called once before the app draws, and on each change. */
export function applyLookClass(root?: HTMLElement): void {
  if (!root && typeof document === 'undefined') return;
  root ??= document.documentElement;
  root.classList.toggle('dark', current === 'dark');
  root.style.colorScheme = current;
}

/** Changes the look, saves it on this machine, and tells whoever draws in it. */
export function setLook(look: Look): void {
  if (look === current) return;
  current = look;
  Object.assign(view, PALETTES[look], { snap: { ...PALETTES[look].snap } });
  try {
    localStorage.setItem(KEY, look);
  } catch {
    // A machine that won't keep it still changes now; it starts light next time.
  }
  applyLookClass();
  for (const listener of listeners) listener(look);
}

/** Called after each change of look; gives back the way to stop. */
export function onLook(listener: (look: Look) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The palettes, for the tests. */
export const palettes = PALETTES;
