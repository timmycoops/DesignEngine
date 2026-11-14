import type { ComponentProps, ReactNode } from 'react';
import { cn } from 'cn';
import { rampColour } from '@/viewport/rampColours';

/** A road's letter in its own colour, as the viewport draws it. */
export function RoadBadge({ name }: { name: string }) {
  return (
    <span className="inline-block shrink-0 rounded px-1 text-[10px] font-semibold text-white" style={{ background: rampColour(name) }}>
      {name}
    </span>
  );
}

/** A word on a line's state, in the colour that means it (UI plan, item 8). */
export function StatusBadge({ tone, children, title }: { tone: 'edited' | 'warning' | 'ok' | 'refused'; children: ReactNode; title?: string }) {
  const tones = { edited: 'bg-edited/20 text-edited', warning: 'bg-warning/20 text-warning', ok: 'bg-ok/20 text-ok', refused: 'bg-destructive/20 text-destructive' };
  return (
    <span className={cn('inline-block shrink-0 rounded px-1 text-[10px]', tones[tone])} title={title}>
      {children}
    </span>
  );
}

/** A small button that's only an icon, its label in its tooltip. */
export function IconButton({ label, className, children, ...props }: { label: string; children: ReactNode } & ComponentProps<'button'>) {
  return (
    <button type="button" className={cn('inline-flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground [&_svg]:size-3.5', className)} title={label} aria-label={label} {...props}>
      {children}
    </button>
  );
}
