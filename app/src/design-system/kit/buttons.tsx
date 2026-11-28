import { ChevronDown, ChevronRight } from 'lucide-react';
import type { ComponentProps, ReactNode } from 'react';
import { cn } from 'cn';
import { Button } from '../ui/button';
import { Checkbox } from '../ui/checkbox';

/**
 * A button written as words in a line, underlined under the pointer (Two sides plan, workstream 6): Open, Clear, + Add a
 * step. `muted` for an aside, `primary` for the way on, `danger` for what takes something away, `plain` in the text's
 * own colour, as a name that opens; `lit` for the one open.
 */
export function TextButton({ tone = 'muted', lit, className, children, ...props }: { tone?: 'muted' | 'primary' | 'danger' | 'plain'; lit?: boolean; children: ReactNode } & ComponentProps<'button'>) {
  const tones = {
    muted: 'text-muted-foreground hover:text-foreground',
    primary: 'text-primary',
    danger: 'text-muted-foreground hover:text-destructive',
    plain: '',
  };
  return (
    <button type="button" className={cn('inline-flex items-center gap-0.5 text-left hover:underline disabled:pointer-events-none disabled:opacity-40', tones[tone], lit && 'font-semibold text-primary', className)} {...props}>
      {children}
    </button>
  );
}

/** A line that opens what's under it and closes it again, its arrow saying which. */
export function Disclosure({ open, onToggle, children, className, slot, title, ...data }: { open: boolean; onToggle: () => void; children: ReactNode; className?: string; slot?: string; title?: string } & { [data: `data-${string}`]: string | number | undefined }) {
  return (
    <button type="button" data-slot={slot} aria-expanded={open} title={title} className={cn('inline-flex items-center gap-0.5 text-left text-muted-foreground hover:text-foreground', className)} onClick={onToggle} {...data}>
      {open ? <ChevronDown className="size-3 shrink-0" /> : <ChevronRight className="size-3 shrink-0" />}
      <span className="min-w-0">{children}</span>
    </button>
  );
}

/** A tick box, its label beside it where it has one, or in its tip and for a reader where it doesn't; `indeterminate` where some are ticked. */
export function Check({
  checked,
  onChange,
  label,
  children,
  disabled,
  title,
  className,
  slot,
  onClick,
}: {
  checked: boolean | 'indeterminate';
  onChange: (on: boolean) => void;
  label: string;
  children?: ReactNode;
  disabled?: boolean;
  title?: string;
  className?: string;
  slot?: string;
  onClick?: (e: React.MouseEvent) => void;
}) {
  const box = (
    <Checkbox className={children ? undefined : className} checked={checked} disabled={disabled} aria-label={label} title={title ?? (children ? undefined : label)} data-slot={slot} onClick={onClick} onCheckedChange={(on) => onChange(on === true)} />
  );
  if (!children) return box;
  return (
    <label className={cn('inline-flex cursor-pointer items-center gap-1.5', disabled && 'cursor-default opacity-50', className)} title={title}>
      {box}
      {children}
    </label>
  );
}

/**
 * A button in a panel, at the height of its boxes (`row` for one in a table's row or a bar): outline by default,
 * `default` for the one that goes on, `ghost` for a quiet one. A panel's last button is `ToolPanel`'s.
 */
export function PanelButton({ variant = 'outline', row, className, ...props }: { variant?: 'outline' | 'default' | 'ghost'; row?: boolean } & ComponentProps<typeof Button>) {
  return <Button variant={variant} size="sm" className={cn(row ? 'h-6' : 'h-7', 'gap-1 px-2 text-xs', className)} {...props} />;
}
