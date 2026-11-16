import { Info } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip';

/**
 * A panel's head (decision 0076): what the panel does in one line, never its name, which is on its tab. A line too
 * long for the panel is cut short; the whole of it, and anything more to say, is in a tip on the line and on the mark at
 * its end.
 */
export function PanelHead({ line, more }: { line: string; more?: string }) {
  const whole = more ? `${line} ${more}` : line;
  return (
    <div data-slot="panel-head" className="flex h-7 shrink-0 items-center gap-1.5 border-b px-3 text-xs text-muted-foreground">
      <span className="min-w-0 flex-1 truncate" title={whole}>
        {line}
      </span>
      {more && (
        <Tooltip>
          <TooltipTrigger asChild>
            <button type="button" data-slot="panel-head-more" aria-label="More about this" className="shrink-0 rounded hover:text-foreground">
              <Info className="size-3.5" />
            </button>
          </TooltipTrigger>
          <TooltipContent side="bottom" align="end" className="max-w-72">
            {whole}
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  );
}
