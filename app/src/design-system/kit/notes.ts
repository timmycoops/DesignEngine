/**
 * How a run's notes read to the panels that show them (decision 0059): each by its severity, never by its words. A note
 * says how much it matters, and where it is when it can.
 */
import type { Note } from '../../bridge';

/** How a line shows: the first says what the run made, information after it is muted, and warnings and blocks stand out. */
export type Tone = 'lead' | 'muted' | 'warning' | 'blocking';

export function noteTone(note: Note, index: number): Tone {
  if (note.severity === 'blocking') return 'blocking';
  if (note.severity === 'warning') return 'warning';
  return index === 0 ? 'lead' : 'muted';
}

/** Whether Show has somewhere to go: a place, or the object a warning is about. */
export function canShow(note: Note): boolean {
  return note.place !== undefined || (note.object !== undefined && note.severity !== 'info');
}

/** A line the app says itself, as a run's information is said. */
export function plainNote(text: string): Note {
  return { severity: 'info', text };
}

/** How many of a run's notes warn or block, in words for the status line; empty when none do. */
export function lookAt(notes: Note[]): string {
  const blocking = notes.filter((n) => n.severity === 'blocking').length;
  const warnings = notes.filter((n) => n.severity === 'warning').length;
  const parts = [];
  if (blocking > 0) parts.push(`${blocking} blocking`);
  if (warnings > 0) parts.push(`${warnings} warning${warnings === 1 ? '' : 's'}`);
  return parts.join(', ');
}
