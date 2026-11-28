import { Dialog as DialogPrimitive } from 'radix-ui';
import type { ReactNode } from 'react';
import { cn } from 'cn';

/**
 * A dialog over the app, in one frame for all of them (Two sides plan, workstream 6): its title, a line of what it's
 * for, what it shows, and its buttons at the foot, the one that goes on last, with `aside` before them (what's picked). Esc or a click outside closes it as its
 * way out does.
 */
export function KitDialog({ open, onClose, title, line, children, foot, aside, width = 640 }: { open: boolean; onClose: () => void; title: ReactNode; line?: ReactNode; children: ReactNode; foot: ReactNode; aside?: ReactNode; width?: number }) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <DialogPrimitive.Content
          data-slot="kit-dialog"
          className="fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] -translate-x-1/2 -translate-y-1/2 flex-col gap-3 rounded-lg border bg-card p-4 text-sm shadow-lg"
          style={{ width: `min(${width}px, 92vw)` }}
          {...(line ? {} : { 'aria-describedby': undefined })}
        >
          <div>
            <DialogPrimitive.Title className="font-semibold">{title}</DialogPrimitive.Title>
            {line && <DialogPrimitive.Description className="text-xs text-muted-foreground">{line}</DialogPrimitive.Description>}
          </div>
          {children}
          <div className="flex items-center justify-between gap-2">
            <span className="min-w-0 text-xs text-muted-foreground">{aside}</span>
            <span className="flex gap-2">{foot}</span>
          </div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}

/** A dialog's list, scrolling inside it when long. */
export function DialogList({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('min-h-0 flex-1 overflow-auto rounded border text-xs', className)}>{children}</div>;
}
