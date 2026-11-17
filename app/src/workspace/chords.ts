// Two-letter shortcuts (decision 0080): NS is N then S. The first letter waits a moment for the second, saying so in
// the status bar, and the pair carries out its tool as a click on it would. Listened for before every other key
// handler, so the second letter is never also its own shortcut (S, snap). Never while typing in a box.
import { typing } from '../viewport/camera';
import { chordTools, type Tool } from '../tools';

/** How long the first letter waits for the second (ms). */
export const CHORD_WAIT_MS = 1500;

/** Listens on the window; returns a function that stops listening. */
export function listenForChords(run: (tool: Tool) => void, status: (text: string) => void): () => void {
  const chords = chordTools();
  const firsts = new Set([...chords.keys()].map((k) => k[0]));
  let first: string | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const forget = () => {
    first = null;
    clearTimeout(timer);
  };
  const key = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.repeat || typing(e.target)) return;
    if (e.key === 'Shift') return;
    const letter = e.key.length === 1 ? e.key.toUpperCase() : null;
    if (first) {
      const pair = first + (letter ?? '');
      forget();
      // Esc after the first letter lets it go, and does nothing else.
      if (e.key === 'Escape' || letter) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
      if (e.key === 'Escape') return status('Shortcut let go');
      const tool = letter ? chords.get(pair) : undefined;
      if (tool) return run(tool);
      if (letter) status(`${pair} isn't a shortcut`);
      return;
    }
    if (!letter || !firsts.has(letter)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    first = letter;
    timer = setTimeout(forget, CHORD_WAIT_MS);
    const next = [...chords].filter(([k]) => k[0] === letter).map(([k, t]) => `${k[1]} ${t.label}`);
    status(`${letter}… then ${next.join(', ')}`);
  };
  // Capture, on the window: ahead of the view's, the camera's and App's own keys.
  window.addEventListener('keydown', key, { capture: true });
  return () => {
    forget();
    window.removeEventListener('keydown', key, { capture: true });
  };
}
