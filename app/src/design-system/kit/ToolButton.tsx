import type { ComponentProps, ReactNode } from 'react';
import { cn } from 'cn';
import { Button } from '../ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip';

/**
 * A button on a bar of tools (the toolbar, the view bar): what it does and its key in a tip, and lit in the picked gold
 * while its tool or mode is on. One that's on or off says so to a screen reader too.
 */
export function ToolButton({ tip, active, className, children, ...props }: { tip: string; active?: boolean; children: ReactNode } & ComponentProps<typeof Button>) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="sm" aria-pressed={active} className={cn(active && 'bg-selection text-primary', className)} {...props}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{tip}</TooltipContent>
    </Tooltip>
  );
}
