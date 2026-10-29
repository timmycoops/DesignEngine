import { useEffect, useRef } from 'react';
import { ChevronDown } from 'lucide-react';
import { Button } from '@/design-system/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/design-system/ui/dropdown-menu';
import { css, ISSUE, NO_COAL, rampCss } from '@/viewport/ramp';
import { useWorkspace } from '@/workspace/context';
import { fmt } from './format';

function Swatch({ colour, label }: { colour: string; label: string }) {
  return (
    <span className="flex items-center gap-1">
      <span className="inline-block size-2.5 rounded-sm" style={{ background: colour }} />
      {label}
    </span>
  );
}

/** The seam horizon on show: which one (and a picker for the others), the thickness ramp, and its issues. */
function SeamLegend() {
  const { seamView, showSeamHorizon, select } = useWorkspace();
  if (!seamView) return null;
  const r = seamView.result;
  return (
    <div className="absolute bottom-3 left-3 w-64 rounded-md border bg-card/90 p-2 text-xs shadow-md backdrop-blur-sm">
      <div className="flex items-center gap-1">
        <button className="min-w-0 truncate font-medium hover:underline" title="Show the model's horizons and issues" onClick={() => select(seamView.id)}>
          {seamView.name}
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="ml-auto h-6 gap-1 px-2 text-xs">
              {r.horizon} <ChevronDown className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="max-h-80 overflow-auto">
            {r.horizons.map((h) => (
              <DropdownMenuItem key={h.code} onSelect={() => showSeamHorizon(seamView.id, h.code)}>
                {h.code}
                {!h.working && <span className="text-muted-foreground">parent</span>}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <p className="mt-1 text-muted-foreground">Roof coloured by seam thickness</p>
      <div className="mt-1 h-2.5 rounded-sm" style={{ background: rampCss() }} />
      <div className="flex justify-between text-muted-foreground tabular-nums">
        <span>0 m</span>
        <span>{fmt(r.thickness_max, 2)} m</span>
      </div>
      <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5">
        <Swatch colour={css(NO_COAL)} label={`No coal · ${fmt((100 * (r.nodes - r.coal_nodes)) / Math.max(r.nodes, 1), 1)}%`} />
        <Swatch colour={css(ISSUE)} label={`Issue · ${fmt(r.flagged_nodes)} nodes`} />
      </div>
      {r.changed.length > 0 && (
        <p className="mt-1 text-destructive" title={r.changed.join('\n')}>
          {r.changed.length} grid file{r.changed.length === 1 ? '' : 's'} changed since the model was built: rebuild it.
        </p>
      )}
    </div>
  );
}

/** Mounts the app's single viewport canvas; the Viewport itself lives as long as the app. */
export function ViewportPanel() {
  const { viewportHost, viewportMounted } = useWorkspace();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current!.appendChild(viewportHost);
    viewportMounted();
    return () => viewportHost.remove();
  }, [viewportHost, viewportMounted]);
  return (
    <div className="relative h-full w-full overflow-hidden bg-[#161e29]">
      <div ref={ref} className="absolute inset-0" />
      <SeamLegend />
    </div>
  );
}
