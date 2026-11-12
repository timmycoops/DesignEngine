import { createContext, useContext } from 'react';
import { LocateFixed } from 'lucide-react';
import { cn } from 'cn';
import type { Note } from '../../bridge';
import { canShow, noteTone, type Tone } from './notes';

/** What Show does with a note: the app picks its object and takes the view to its place (decision 0059). */
export const ShowNote = createContext<((note: Note) => void) | null>(null);

const TONE: Record<Tone, string> = {
  lead: 'text-foreground',
  muted: 'text-muted-foreground',
  warning: 'text-warning',
  blocking: 'text-destructive',
};

/**
 * One line of what a run reported, coloured by how much it matters: orange for a warning, red for what blocks later use.
 * A note with a place, or a warning about an object, has Show beside it. `index` is its place in the run's lines, as the
 * first says what the run made. `text` stands in for the note's own words where a panel shortens them.
 */
export function NoteLine({ note, index, text, className }: { note: Note; index: number; text?: string; className?: string }) {
  const show = useContext(ShowNote);
  const tone = noteTone(note, index);
  return (
    <div className={cn('flex items-start gap-1', TONE[tone], className)} data-severity={note.severity} data-code={note.code}>
      <span className="min-w-0 flex-1">{text ?? note.text}</span>
      {show && canShow(note) && (
        <button
          type="button"
          className="mt-px shrink-0 rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          title={note.place ? 'Show where this is' : 'Show what this is about'}
          aria-label="Show"
          onClick={() => show(note)}
        >
          <LocateFixed className="size-3" />
        </button>
      )}
    </div>
  );
}
