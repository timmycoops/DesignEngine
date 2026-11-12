import { useEffect, useRef, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { Checkbox } from '@/design-system/ui/checkbox';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from '@/design-system/ui/dropdown-menu';
import { css, ISSUE, NO_COAL, rampCss } from '@/viewport/ramp';
import { type Bands, INTERVALS, rlColour, rlRampCss } from '@/viewport/rlColour';
import { DEFAULT_CONTOURS, type SeamShown } from '@/viewport/sceneSync';
import { valueBands } from '@/viewport/bands';
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

/** Colour by RL on show, then every seam model on show, nearest the top the first in the project. */
function Legends() {
  const { seamViews, rlShown, sectionOpen } = useWorkspace();
  if (seamViews.length === 0 && rlShown.objects.length === 0) return null;
  return (
    <div className={`absolute left-3 z-10 flex max-h-[calc(100%-1.5rem)] w-64 flex-col gap-2 overflow-auto ${sectionOpen ? 'bottom-[264px]' : 'bottom-3'}`}>
      {rlShown.objects.length > 0 && <RlLegend />}
      {seamViews.map((view) => (
        <SeamLegend key={view.id} seamView={view} />
      ))}
    </div>
  );
}

/** One band of colour per band of RL, while there are few enough to see; past that the smooth ramp. */
function bandsCss(bands: Bands): string {
  if (bands.count > 40) return rlRampCss();
  const stops = Array.from({ length: bands.count }, (_, k) => {
    const colour = css(rlColour((k + 0.5) / bands.count).map(Math.round) as [number, number, number]);
    return `${colour} ${((100 * k) / bands.count).toFixed(2)}% ${((100 * (k + 1)) / bands.count).toFixed(2)}%`;
  });
  return `linear-gradient(to right, ${stops.join(', ')})`;
}

const MOST_LISTED = 6;

/**
 * What's coloured by RL: the interval (picked from the range, or set), the ramp, the RLs it spans, and
 * switches for the contour lines and for one colour range across every surface.
 */
function RlLegend() {
  const { rlShown, summary, setContours } = useWorkspace();
  const contours = summary?.display.contours ?? DEFAULT_CONTOURS;
  const { objects } = rlShown;
  const intervals = new Set(objects.map((o) => o.bands.interval));
  const one = rlShown.shared || objects.length === 1 ? objects[0].bands : null;
  const label = contours.interval === null ? `Auto${intervals.size === 1 ? ` · ${fmt(objects[0].bands.interval)} m` : ''}` : `${fmt(contours.interval)} m`;
  const span = (b: Bands) => `${fmt(b.low)} to ${fmt(b.high)}`;
  return (
    <div className="shrink-0 rounded-md border bg-card/90 p-2 text-xs shadow-md backdrop-blur-sm">
      <div className="flex items-center gap-1">
        <span className="font-medium">Coloured by RL</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="ml-auto h-6 gap-1 px-2 text-xs" title="Metres between contours">
              {label} <ChevronDown className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuRadioGroup
              value={contours.interval === null ? 'auto' : String(contours.interval)}
              onValueChange={(v) => setContours({ ...contours, interval: v === 'auto' ? null : Number(v) })}
            >
              <DropdownMenuRadioItem value="auto">Auto, from the RL range</DropdownMenuRadioItem>
              {INTERVALS.map((m) => (
                <DropdownMenuRadioItem key={m} value={String(m)}>
                  {m} m
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {one ? (
        <>
          <div className="mt-1 h-2.5 rounded-sm" style={{ background: bandsCss(one) }} />
          <div className="flex justify-between text-muted-foreground tabular-nums">
            <span>{fmt(one.low)}</span>
            <span>{fmt(one.high)} RL</span>
          </div>
        </>
      ) : (
        <>
          <div className="mt-1 h-2.5 rounded-sm" style={{ background: rlRampCss() }} />
          <p className="text-muted-foreground">Each over its own RLs:</p>
          <ul className="text-muted-foreground tabular-nums">
            {objects.slice(0, MOST_LISTED).map((o) => (
              <li key={o.id} className="flex gap-2">
                <span className="min-w-0 flex-1 truncate" title={o.name}>
                  {o.name}
                </span>
                <span>{span(o.bands)}</span>
                {intervals.size > 1 && <span>· {fmt(o.bands.interval)} m</span>}
              </li>
            ))}
            {objects.length > MOST_LISTED && <li>and {objects.length - MOST_LISTED} more</li>}
          </ul>
        </>
      )}
      <div className="mt-1.5 flex flex-col gap-1">
        <label className="flex items-center gap-1.5">
          <Checkbox checked={contours.edge_lines} onCheckedChange={(c) => setContours({ ...contours, edge_lines: c === true })} />
          Contour lines
        </label>
        <label className="flex items-center gap-1.5" title="One colour range across everything coloured by RL, so an RL is the same colour on every surface">
          <Checkbox checked={contours.shared} onCheckedChange={(c) => setContours({ ...contours, shared: c === true })} />
          Same colours on every surface
        </label>
      </div>
    </div>
  );
}

/**
 * A seam model's horizon on show: which one (and a picker for the others), what it's coloured by, its thickness ramp or
 * one of its grids' bands, and its issues. Coloured by a grid, the seam's blocks can take their own values' colours on the
 * same bands, to check them by eye against the contours (decision 0061, workstream 6).
 */
function SeamLegend({ seamView }: { seamView: SeamShown }) {
  const { showSeamHorizon, select, colourSeamBlocks, blocksColouredBy } = useWorkspace();
  const r = seamView.result;
  const here = r.horizons.find((h) => h.code === r.horizon);
  const material = (here?.material ?? 'coal').toLowerCase();
  const variables = here?.variables ?? [];
  const bands = r.variable ? valueBands(r.value_range[0], r.value_range[1]) : null;
  const blocksOn = blocksColouredBy === `${seamView.id}#${r.horizon}#${r.variable}`;
  return (
    <div className="shrink-0 rounded-md border bg-card/90 p-2 text-xs shadow-md backdrop-blur-sm">
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
              <DropdownMenuItem key={h.code} onSelect={() => showSeamHorizon(seamView.id, h.code, h.variables.includes(r.variable ?? '') ? r.variable : null)}>
                {h.code}
                {!h.working && <span className="text-muted-foreground">parent</span>}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <div className="mt-1 flex items-center gap-1">
        <span className="text-muted-foreground">Roof coloured by</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" className="h-6 gap-1 px-2 text-xs" aria-label="What the horizon is coloured by">
              {r.variable ?? 'thickness'} <ChevronDown className="size-3" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onSelect={() => showSeamHorizon(seamView.id, r.horizon, null)}>thickness</DropdownMenuItem>
            {variables.map((v) => (
              <DropdownMenuItem key={v} onSelect={() => showSeamHorizon(seamView.id, r.horizon, v)}>
                {v}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {bands ? (
        <>
          <div className="mt-1 flex h-2.5 overflow-hidden rounded-sm">
            {Array.from({ length: bands.count }, (_, k) => (
              <div key={k} className="flex-1" style={{ background: css(rlColour((k + 0.5) / bands.count).map(Math.round) as [number, number, number]) }} />
            ))}
          </div>
          <div className="flex justify-between text-muted-foreground tabular-nums">
            <span>{fmt(bands.low, 3)}</span>
            <span>every {fmt(bands.interval, 3)}</span>
            <span>{fmt(bands.high, 3)}</span>
          </div>
          <label className="mt-1 flex items-center gap-1.5" title="Each block whose seam is this horizon, coloured by its own interrogated value on these bands: a block that doesn't match the contours round it stands out">
            <Checkbox checked={blocksOn} onCheckedChange={(on) => colourSeamBlocks(on === true ? seamView : null)} aria-label="Colour this seam's blocks by their value" />
            Colour {r.horizon}'s blocks by their {r.variable}
          </label>
        </>
      ) : (
        <>
          <div className="mt-1 h-2.5 rounded-sm" style={{ background: rampCss() }} />
          <div className="flex justify-between text-muted-foreground tabular-nums">
            <span>0 m</span>
            <span>{fmt(r.thickness_max, 2)} m</span>
          </div>
        </>
      )}
      <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5">
        <Swatch colour={css(NO_COAL)} label={`No ${material} · ${fmt((100 * (r.nodes - r.coal_nodes)) / Math.max(r.nodes, 1), 1)}%`} />
        {!r.variable && <Swatch colour={css(ISSUE)} label={`Issue · ${fmt(r.flagged_nodes)} nodes`} />}
      </div>
      {r.changed.length > 0 && (
        <p className="mt-1 text-destructive" title={r.changed.join('\n')}>
          {r.changed.length} grid file{r.changed.length === 1 ? '' : 's'} changed since the model was built: rebuild it.
        </p>
      )}
    </div>
  );
}

/** While drawing: type a vertex instead of clicking one. */
function TypedEntry() {
  const { drawing, typeVertex } = useWorkspace();
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);
  if (!drawing) return null;
  const submit = async () => {
    const problem = await typeVertex(text);
    setError(problem);
    if (!problem) setText('');
  };
  return (
    <div className="absolute bottom-3 left-1/2 w-[28rem] max-w-[calc(100%-1.5rem)] -translate-x-1/2 rounded-md border bg-card/90 p-2 text-xs shadow-md backdrop-blur-sm">
      <Input
        className="h-7 font-mono text-xs"
        value={text}
        placeholder="E N [RL]   or   @distance<bearing [RL | grade%]"
        aria-label="Type a vertex"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') void submit();
          if (e.key === 'Escape') e.currentTarget.blur();
        }}
      />
      <p className={`mt-1 ${error ? 'text-destructive' : 'text-muted-foreground'}`}>
        {error ?? 'Enter adds the vertex; Enter on an empty entry finishes the string. Bearings are grid, clockwise from north.'}
      </p>
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
      <Legends />
      <TypedEntry />
    </div>
  );
}
