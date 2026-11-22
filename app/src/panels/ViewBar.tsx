import { createContext, useContext, useEffect, useState } from 'react';
import { Aperture, Box, Cone, Cuboid, Expand, Focus, Magnet, Map as MapIcon, MousePointer2, PaintBucket, PenLine, Ruler, Scissors, StepBack, StepForward } from 'lucide-react';
import { ToolButton } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from '@/design-system/ui/dropdown-menu';
import { Input } from '@/design-system/ui/input';
import { Separator } from '@/design-system/ui/separator';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/design-system/ui/tooltip';
import type { Mode } from '@/viewport/strings';
import { DETAIL_CHOICES, DETAIL_DEFAULT, FIELD_OF_VIEW_DEFAULT } from '@/viewport/Viewport';
import { VIEW_CONTROLS_CHANGED, viewKeyLabel } from '@/viewport/viewControls';

/** Fields of view offered on the view bar, in degrees. */
const FIELD_OF_VIEW_CHOICES = [2, 4, 6, 10, 15, 20, 30, 45];

/** What the view bar shows and does. App owns the state, as it does the keys that do the same (T, P, F, E, 2, [, ], S). */
export interface ViewTools {
  mode: Mode;
  setMode: (mode: Mode) => void;
  snapping: boolean;
  toggleSnapping: () => void;
  /** Looking straight down, north up. */
  plan: boolean;
  togglePlan: () => void;
  /** The 3D view has no perspective: a metre the same size near and far. */
  orthographic: boolean;
  toggleOrthographic: () => void;
  /** Something is selected to zoom to. */
  hasSelection: boolean;
  zoomToSelection: () => void;
  zoomToExtents: () => void;
  /** The 3D field of view, in degrees. */
  fieldOfView: number;
  setFieldOfView: (degrees: number) => void;
  /** Pixels far-off chunks may be drawn out by (decision 0097); 0 is always full detail. */
  detail: number;
  setDetail: (pixels: number) => void;
  /** Strings and polygons drawn over everything; by default they're hidden behind the solids and surfaces in front. */
  stringsOver: boolean;
  setStringsOver: (on: boolean) => void;
  sectionOpen: boolean;
  /** Starts picking a section's two points, or stops; closes an open one. */
  toggleSection: () => void;
  /** Steps an open section square to itself: 1 on, -1 back. */
  stepSection: (direction: number) => void;
  /** Metres to step, as typed. */
  sectionStep: string;
  setSectionStep: (text: string) => void;
  sectionFill: boolean;
  setSectionFill: (on: boolean) => void;
  /** Opens View controls: Spry's mouse layout or ours, and the view's keys. */
  openViewControls: () => void;
}

/** The view's keys as set (0044), named for the tips, and drawn again when they change. */
function useViewKeys() {
  const [, setChanged] = useState(0);
  useEffect(() => {
    const changed = () => setChanged((n) => n + 1);
    window.addEventListener(VIEW_CONTROLS_CHANGED, changed);
    return () => window.removeEventListener(VIEW_CONTROLS_CHANGED, changed);
  }, []);
  return { turn: viewKeyLabel('turn'), selection: viewKeyLabel('selection'), extents: viewKeyLabel('extents'), plan: viewKeyLabel('plan'), orthographic: viewKeyLabel('orthographic') };
}

export const ViewToolsContext = createContext<ViewTools | null>(null);

/**
 * The view's own tools, on the view (decision 0076): Select, Draw, Measure and Snap, 3D or Plan, perspective or
 * orthographic, the zooms, the field of
 * view and Section, with the section's step and fill beside it while one is open. Every one has a key, said in its tip.
 */
export function ViewBar() {
  const view = useContext(ViewToolsContext);
  const keys = useViewKeys();
  if (!view) return null;
  const { mode, plan, snapping, sectionOpen } = view;
  return (
    <div
      role="toolbar"
      aria-label="View tools"
      data-slot="view-bar"
      data-section={sectionOpen ? 'open' : 'closed'}
      className="view-bar flex flex-wrap items-center gap-0.5 rounded-md border bg-card/90 p-0.5 shadow-md backdrop-blur-sm"
    >
      <ToolButton size="icon-sm" tip="Select and edit strings" aria-label="Select" active={mode === 'select'} onClick={() => view.setMode('select')}>
        <MousePointer2 />
      </ToolButton>
      <ToolButton
        size="icon-sm"
        tip={sectionOpen ? 'Draw a string on the section (NS): snaps to crossings, vertices and lines of the cut; right-click finishes' : 'Draw a string (NS): snaps to string vertices and segments, then surfaces; right-click finishes'}
        aria-label="Draw string"
        active={mode === 'draw'}
        onClick={() => view.setMode('draw')}
      >
        <PenLine />
      </ToolButton>
      <ToolButton
        size="icon-sm"
        tip={mode === 'measure' ? 'Stop measuring (M)' : 'Measure (M): the distance, RL difference, grade and bearing between clicks; click on for a running total'}
        aria-label="Measure distance"
        active={mode === 'measure'}
        onClick={() => view.setMode(mode === 'measure' ? 'select' : 'measure')}
      >
        <Ruler />
      </ToolButton>
      <ToolButton
        size="icon-sm"
        tip={snapping ? `Snap is on: to ${sectionOpen ? 'the cut lines' : 'strings, then surfaces'} (S turns it off)` : 'Snap is off: points go on the working plane (S turns it on)'}
        aria-label="Snap"
        active={snapping}
        onClick={view.toggleSnapping}
      >
        <Magnet />
      </ToolButton>
      <Separator orientation="vertical" className="mx-0.5 h-5" />
      <ToolButton
        tip={plan ? `Back to the 3D view (${keys.plan}). Or hold ${keys.turn} and drag to tilt up out of plan` : `Plan view (${keys.plan}): straight down, north up; the wheel zooms where the cursor is`}
        active={plan}
        className="px-2"
        onClick={view.togglePlan}
      >
        {plan ? <MapIcon /> : <Box />} {plan ? 'Plan' : '3D'}
      </ToolButton>
      <ToolButton
        tip={view.orthographic ? `Orthographic (${keys.orthographic}): no perspective, a metre the same size near and far. Press for perspective` : `Perspective (${keys.orthographic}). Press for orthographic: no perspective, a metre the same size near and far`}
        active={view.orthographic}
        disabled={plan || sectionOpen}
        className="px-2"
        aria-label={view.orthographic ? 'Orthographic' : 'Perspective'}
        onClick={view.toggleOrthographic}
      >
        {view.orthographic ? <Cuboid /> : <Cone />} {view.orthographic ? 'Ortho' : 'Persp'}
      </ToolButton>
      <ToolButton size="icon-sm" tip={`Zoom to what's selected (${keys.selection}), from the way you're looking`} aria-label="Zoom to what's selected" disabled={!view.hasSelection || sectionOpen} onClick={view.zoomToSelection}>
        <Focus />
      </ToolButton>
      <ToolButton size="icon-sm" tip={`Zoom to everything (${keys.extents}, or Shift+${keys.selection})`} aria-label="Zoom to everything" disabled={sectionOpen} onClick={view.zoomToExtents}>
        <Expand />
      </ToolButton>
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="px-2 tabular-nums" aria-label="Field of view">
                <Aperture /> {view.fieldOfView}°
              </Button>
            </DropdownMenuTrigger>
          </TooltipTrigger>
          <TooltipContent>3D field of view, how much detail is drawn far off, whether strings show through solids, and the mouse layout and keys. Saved on this machine</TooltipContent>
        </Tooltip>
        <DropdownMenuContent align="start">
          {FIELD_OF_VIEW_CHOICES.map((degrees) => (
            <DropdownMenuItem key={degrees} onSelect={() => view.setFieldOfView(degrees)}>
              <span className="w-8 tabular-nums">{degrees}°</span>
              {degrees === FIELD_OF_VIEW_DEFAULT ? 'default' : degrees === 45 ? 'old view' : ''}
              {degrees === view.fieldOfView ? ' ✓' : ''}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">Detail far off: drawn out by at most</DropdownMenuLabel>
          {DETAIL_CHOICES.map((pixels) => (
            <DropdownMenuItem key={`detail-${pixels}`} onSelect={() => view.setDetail(pixels)}>
              <span className="w-8 tabular-nums">{pixels === 0 ? 'Full' : `${pixels} px`}</span>
              {pixels === DETAIL_DEFAULT ? 'default' : pixels === 0 ? 'every triangle, always' : 'faster, softer far off'}
              {pixels === view.detail ? ' ✓' : ''}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">Strings and polygons</DropdownMenuLabel>
          <DropdownMenuItem onSelect={() => view.setStringsOver(false)}>
            Hidden behind solids and surfaces{view.stringsOver ? '' : ' ✓'}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => view.setStringsOver(true)}>
            Over everything{view.stringsOver ? ' ✓' : ''}
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={view.openViewControls}>View controls: mouse layout and keys…</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <Separator orientation="vertical" className="mx-0.5 h-5" />
      <ToolButton tip={sectionOpen ? 'Close the section (2)' : 'Section (2): pick two points for a true vertical slice'} active={sectionOpen || mode === 'section'} className="px-2" onClick={view.toggleSection}>
        <Scissors /> Section
      </ToolButton>
      {sectionOpen && (
        <>
          <ToolButton size="icon-sm" tip="Step back ([)" aria-label="Step back" onClick={() => view.stepSection(-1)}>
            <StepBack />
          </ToolButton>
          <Input
            className="h-7 w-14 px-1.5 text-xs tabular-nums"
            value={view.sectionStep}
            onChange={(e) => view.setSectionStep(e.target.value)}
            title="Metres to step, square to the section"
            aria-label="Section step in metres"
          />
          <ToolButton size="icon-sm" tip="Step on (])" aria-label="Step on" onClick={() => view.stepSection(1)}>
            <StepForward />
          </ToolButton>
          <ToolButton tip="Fill solids between their cut lines" active={view.sectionFill} className="px-2" onClick={() => view.setSectionFill(!view.sectionFill)}>
            <PaintBucket /> Fill
          </ToolButton>
        </>
      )}
    </div>
  );
}
