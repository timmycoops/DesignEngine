import { Check, Minus, X } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { cn } from 'cn';
import { Badge } from '../ui/badge';

/**
 * What a panel shows and doesn't edit (Two sides plan, workstream 6), as Properties shows an object: titled parts with a
 * hairline under each, rows of a label and its value lined up, a flag that holds or doesn't, and a slider for a share.
 */

/** A titled part of what's shown: Object, Look, Attributes, History. */
export function ShownSection({ title, children, className }: { title: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cn('border-b py-2', className)}>
      <h3 className="px-3 pb-1 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">{title}</h3>
      {children}
    </section>
  );
}

/** A label and its value, the labels of a part lined up and the value cut short to its line. */
export function ShownRow({ label, children, className }: { label: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cn('grid grid-cols-[7.5rem_1fr] items-center gap-2 px-3 py-0.5 text-xs', className)}>
      <span className="truncate text-muted-foreground">{label}</span>
      <span className="min-w-0 truncate tabular-nums">{children}</span>
    </div>
  );
}

/** Whether a check holds: a tick in green, a cross in red, a dash in grey where it wasn't checked; why in its tip. */
export function FlagBadge({ label, value, detail }: { label: string; value: boolean | null; detail?: string }) {
  const Icon = value === null ? Minus : value ? Check : X;
  const tone = value === null ? 'text-muted-foreground' : value ? 'text-ok' : 'text-destructive';
  return (
    <Badge variant="outline" className="gap-1 font-normal" title={detail}>
      <Icon className={cn('size-3', tone)} />
      {label}
    </Badge>
  );
}

/**
 * A share from `min` to 100 %, in steps of `step`, as a row of its own: it shows as it moves and is kept when it's let
 * go, so a drag is one change, not one a step.
 */
export function PercentSlider({ value, label, onChange, min = 5, step = 5 }: { value: number; label: string; onChange: (share: number, commit: boolean) => void; min?: number; step?: number }) {
  const [moving, setMoving] = useState<number | null>(null);
  const shown = moving ?? value;
  const commit = () => {
    if (moving !== null) onChange(moving, true);
    setMoving(null);
  };
  return (
    <div className="grid grid-cols-[7.5rem_1fr_2.5rem] items-center gap-2 px-3 py-0.5 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <input
        type="range"
        min={min}
        max={100}
        step={step}
        value={Math.round(shown * 100)}
        aria-label={label}
        className="h-4 min-w-0 accent-primary"
        onChange={(e) => {
          const next = Number(e.target.value) / 100;
          setMoving(next);
          onChange(next, false);
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <span className="text-right tabular-nums">{Math.round(shown * 100)}%</span>
    </div>
  );
}
