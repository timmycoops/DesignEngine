import { cn } from 'cn';
import { SAID_CLASS, type Said } from './said';

/**
 * The one line a panel says what a run, save or edit did (Two sides plan, workstream 6), in its tone's colour. Nothing
 * said, nothing drawn. `oneLine` cuts it short to a line with the whole of it in its tip, for a bar; `inline` puts it in
 * a line of other things.
 */
export function SaidLine({ said, oneLine, inline, className, slot }: { said: Said | null | undefined; oneLine?: boolean; inline?: boolean; className?: string; slot?: string }) {
  if (!said || !said.text) return null;
  const Tag = inline ? 'span' : 'div';
  return (
    <Tag role="status" data-slot={slot ?? 'said'} data-tone={said.tone} className={cn(SAID_CLASS[said.tone], oneLine && 'min-w-0 truncate', className)} title={oneLine ? said.text : undefined}>
      {said.text}
    </Tag>
  );
}
