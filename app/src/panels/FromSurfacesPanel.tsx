import { Check } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { ObjectSummary } from '@/bridge';
import { Input } from '@/design-system/ui/input';
import { Field, NumberInput, NumberOrBlank, type RunReport, Select, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { choiceWords } from '@/tools';
import { useWorkspace } from '@/workspace/context';

/**
 * Strings from surfaces (String tools plan, workstream 5; decision 0083): new strings made from surfaces, grids and
 * solids, on a layer, what they're made from left as it was.
 *
 * - Where surfaces meet: two surfaces or grids picked, the line where they meet at its own RLs, such as a design's crest
 *   on the topography.
 * - Contours: on the surface or grid picked last, every so many metres between two RLs (left blank, its own range), each
 *   string carrying its RL.
 * - Outline: each surface's or grid's edge, every hole its own string; a solid's outline in plan, at its lowest RL.
 * - Section to strings: the lines the open section cuts through what's picked, or, with no section open, a section
 *   through two clicks.
 *
 * While the Workflow panel is adding steps, the button adds the tool as a step instead (decision 0078): what's picked
 * is found by rule each time it runs; a section's two points are asked for as the step is added.
 */

type Mode = 'where_surfaces_meet' | 'contours' | 'outline' | 'from_section';

const MODES: readonly Mode[] = ['where_surfaces_meet', 'contours', 'outline', 'from_section'];
const { labels: MODE_LABELS, lines: MODE_LINES } = choiceWords('from_surfaces', MODES);

const SURFACES = new Set(['surface', 'grid']);
const MESHES = new Set(['surface', 'grid', 'solid']);
const LAYERS: Record<Mode, string> = { where_surfaces_meet: 'crests', contours: 'contours', outline: 'outlines', from_section: 'section strings' };

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
const names = (on: ObjectSummary[]) => (on.length > 3 ? `${on.slice(0, 3).map((o) => o.name).join(', ')} and ${on.length - 3} more` : on.map((o) => o.name).join(', '));

export function FromSurfacesPanel() {
  const { summary, selection, selected, runRecord, clearRefusal, pickLine, toolAsk, toolMode, sectionPlane, sectionOpen } = useWorkspace();
  const [mode, setMode] = useState<Mode>('contours');
  useEffect(() => {
    if (toolAsk?.panel === 'from_surfaces' && MODES.includes(toolAsk.mode as Mode)) setMode(toolAsk.mode as Mode);
  }, [toolAsk]);
  useEffect(() => {
    toolMode('from_surfaces', mode);
    return () => toolMode('from_surfaces', null);
  }, [toolMode, mode]);

  const [every, setEvery] = useState(5);
  const [from, setFrom] = useState<number | null>(null);
  const [to, setTo] = useState<number | null>(null);
  const [layers, setLayers] = useState<Record<Mode, string>>(LAYERS);
  const layer = layers[mode].trim() || LAYERS[mode];
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  // In the order picked, so the first of two surfaces is the one named first.
  const inOrder = selection.map((id) => objects.find((o) => o.id === id)).filter((o): o is ObjectSummary => o !== undefined);
  const surfaces = inOrder.filter((o) => SURFACES.has(o.kind));
  const meshes = inOrder.filter((o) => MESHES.has(o.kind));
  const last = objects.find((o) => o.id === selected);
  /** Contours: the surface or grid picked last. */
  const contoured = last && SURFACES.has(last.kind) ? last : surfaces.at(-1);

  const runIt = async (command: string, params: Record<string, unknown>) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, params);
    setBusy(false);
    setReport(outcome.record ? { notes: outcome.record.notes } : { notes: [], error: outcome.error });
  };

  /** A section's two points: the open section's, or two clicks. Null when Esc is pressed. */
  const sectionPoints = async (): Promise<{ from: number[]; to: number[] } | null> => {
    const open = sectionPlane();
    if (open) return { from: [open[0][0], open[0][1], 0], to: [open[1][0], open[1][1], 0] };
    const line = await pickLine('Click where the section starts. Esc cancels', 'Click where it ends. Esc cancels');
    return line ? { from: [line[0][0], line[0][1], 0], to: [line[1][0], line[1][1], 0] } : null;
  };

  const contourParams = () => ({ surface: contoured?.id, every, layer, ...(from === null ? {} : { from }), ...(to === null ? {} : { to }) });
  const meetParams = () => ({ surface: surfaces[0]?.id, with: surfaces[1]?.id, layer });
  const outlineParams = () => ({ objects: meshes.map((o) => o.id), layer });

  const action: ToolAction = (() => {
    const icon = <Check className="size-3.5" />;
    switch (mode) {
      case 'where_surfaces_meet':
        return {
          label: surfaces.length === 2 ? `Where ${surfaces[0].name} meets ${surfaces[1].name}` : 'Where surfaces meet',
          icon,
          disabled: surfaces.length !== 2,
          busy,
          run: () => runIt('string.where_surfaces_meet', meetParams()),
          step: { command: 'string.where_surfaces_meet', params: meetParams() },
        };
      case 'contours':
        return {
          label: contoured ? `Contour ${contoured.name}` : 'Contours',
          icon,
          disabled: !contoured || !(every > 0) || (from !== null && to !== null && to < from),
          busy,
          run: () => runIt('string.contours', contourParams()),
          step: { command: 'string.contours', params: contourParams() },
        };
      case 'outline':
        return {
          label: meshes.length === 0 ? 'Outline' : `Outline ${meshes.length === 1 ? meshes[0].name : plural(meshes.length, 'object')}`,
          icon,
          disabled: meshes.length === 0,
          busy,
          run: () => runIt('string.outline', outlineParams()),
          step: { command: 'string.outline', params: outlineParams() },
        };
      case 'from_section': {
        const ask = async () => {
          const points = await sectionPoints();
          return points ? { ...points, objects: meshes.map((o) => o.id), layer } : null;
        };
        return {
          label: meshes.length === 0 ? 'Section to strings' : `Keep the section through ${meshes.length === 1 ? meshes[0].name : plural(meshes.length, 'object')}`,
          icon,
          disabled: meshes.length === 0,
          busy,
          title: sectionOpen ? 'The open section, where it stands now' : 'Two clicks in the view: where the section starts and ends',
          run: async () => {
            const p = await ask();
            if (p) await runIt('string.from_section', p);
          },
          step: { command: 'string.from_section', params: { objects: meshes.map((o) => o.id), layer }, ask },
        };
      }
    }
  })();

  const pickedWords = (() => {
    switch (mode) {
      case 'where_surfaces_meet':
        return surfaces.length === 2
          ? `${surfaces[0].name} and ${surfaces[1].name}`
          : surfaces.length < 2
            ? 'Pick two surfaces or grids, by a box or Ctrl-click'
            : `${plural(surfaces.length, 'surface')} picked: pick two`;
      case 'contours':
        return contoured ? contoured.name : 'Pick a surface or grid';
      default:
        return meshes.length > 0 ? names(meshes) : 'Pick surfaces, grids or solids, by a box or Ctrl-click';
    }
  })();
  const nothing = (mode === 'where_surfaces_meet' && surfaces.length !== 2) || (mode === 'contours' && !contoured) || ((mode === 'outline' || mode === 'from_section') && meshes.length === 0);

  return (
    <ToolPanel title="Strings from surfaces" panel="from_surfaces" summary={MODE_LINES[mode]} result={report} action={action}>
      <ToolSection>
        <Field label="Tool">
          <Select value={mode} aria-label="Strings from surfaces tool" onChange={(e) => setMode(e.target.value as Mode)}>
            {MODES.map((m) => (
              <option key={m} value={m}>
                {MODE_LABELS[m]}
              </option>
            ))}
          </Select>
        </Field>
      </ToolSection>

      <ToolSection title="Picked">
        <Field label="From" hint={mode === 'where_surfaces_meet' && surfaces.length === 2 ? 'The strings are named for the first picked, then the second' : undefined}>
          <span data-slot="from-surfaces-on" className={nothing ? 'text-muted-foreground' : undefined}>
            {pickedWords}
          </span>
        </Field>
      </ToolSection>

      <ToolSection title="Settings">
        {mode === 'contours' && (
          <>
            <Field label="Every">
              <NumberInput value={every} onChange={setEvery} label="Contour spacing" unit="m" />
            </Field>
            <Field label="From" hint="Blank: the lowest RL on the spacing the surface reaches">
              <NumberOrBlank value={from} onChange={setFrom} label="Lowest contour RL" unit="m" blank="Its lowest" />
            </Field>
            <Field label="To" hint={from !== null && to !== null && to < from ? 'The highest RL has to be above the lowest' : 'Blank: the highest RL on the spacing it reaches'}>
              <NumberOrBlank value={to} onChange={setTo} label="Highest contour RL" unit="m" blank="Its highest" />
            </Field>
          </>
        )}
        {mode === 'from_section' && (
          <Field label="Section" hint={sectionOpen ? 'Where the open section stands now, stepped or not' : 'No section is open: two clicks in the view place one'}>
            <span className="text-muted-foreground">{sectionOpen ? 'The open section' : 'Two clicks'}</span>
          </Field>
        )}
        <Field label="Layer">
          <Input className="h-7 px-1.5 text-xs" value={layers[mode]} aria-label="Layer for the strings made" onChange={(e) => setLayers({ ...layers, [mode]: e.target.value })} />
        </Field>
      </ToolSection>
    </ToolPanel>
  );
}
