import { Combine, Slice } from 'lucide-react';
import { useMemo, useState } from 'react';
import type { ObjectSummary, StepTarget } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { Choice, Field, type RunReport, Select, TargetInput, TargetOptions, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { byStrip, facesLine } from './strips';

/**
 * Cut solids (decision 0049, UI plan): one panel for every cut. Cut solids by a surface, a grid or an RL, keeping either
 * side; slice them into strips by faces, each face told apart by its strip number, a solid a strip adding up to the
 * solid; or make the large solid between two surfaces inside a closed string. The solids and faces stay as they are,
 * and each run is one undo. Made from the kit, as every tool panel is.
 */

type Mode = 'strips' | 'cut' | 'between';
type Scope = 'picked' | 'layer' | 'visible';

const MODES: readonly Mode[] = ['strips', 'cut', 'between'];
const MODE_LABELS: Record<Mode, string> = { strips: 'Into strips', cut: 'By a surface', between: 'Between surfaces' };
const SCOPES: readonly Scope[] = ['picked', 'layer', 'visible'];

export function CutPanel() {
  const { summary, selected, selection, runRecord, clearRefusal, select } = useWorkspace();
  const [mode, setMode] = useState<Mode>('strips');
  const [scope, setScope] = useState<Scope>('picked');
  const [by, setBy] = useState<StepTarget | null>(null);
  const [keep, setKeep] = useState<'below' | 'above' | 'both'>('both');
  const [facesLayer, setFacesLayer] = useState<string | null>(null);
  const [top, setTop] = useState<string | null>(null);
  const [bottom, setBottom] = useState<string | null>(null);
  const [limit, setLimit] = useState<string | null>(null);
  const [name, setName] = useState('PIT');
  const [outputs, setOutputs] = useState<Record<Mode, string>>({ strips: 'strips', cut: 'cut', between: 'pit' });
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  const layers = summary?.layers ?? [];
  const layerName = (id: string) => layers.find((l) => l.id === id)?.name ?? '';

  // The solids: those selected, all on the layer of the first of them, or all on show.
  const solids = objects.filter((o) => o.kind === 'solid');
  const pickedIds = new Set(selection);
  const picked = solids.filter((o) => pickedIds.has(o.id));
  const onLayer = picked.length > 0 ? solids.filter((o) => o.layer === picked[0].layer) : [];
  const shown = new Set(layers.filter((l) => l.visible).map((l) => l.id));
  const visible = solids.filter((o) => shown.has(o.layer) && !o.hidden);
  const chosen = scope === 'picked' ? picked : scope === 'layer' ? onLayer : visible;
  const scopeLabels: Record<Scope, string> = { picked: `Picked (${picked.length})`, layer: `Its layer (${onLayer.length})`, visible: `On show (${visible.length})` };

  const surfaces = useMemo(() => objects.filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid')), [objects]);

  // The faces: the surfaces on a layer, told apart by strip number. The layer with the most numbered faces comes first.
  const faceLayers = useMemo(() => {
    const counts = new Map<string, number>();
    for (const o of objects) if (o.kind === 'surface' && o.attributes.strip !== undefined) counts.set(o.layer, (counts.get(o.layer) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([id, count]) => ({ id, count }));
  }, [objects]);
  const facesOn = facesLayer ?? faceLayers[0]?.id ?? null;
  const faces = objects.filter((o) => o.kind === 'surface' && o.layer === facesOn);
  const strips = byStrip(faces);

  const strings = objects.filter((o) => o.kind === 'string');
  const closed = strings.filter((o) => o.closed);
  const pickedString = strings.find((o) => o.id === selected);
  const around = strings.find((o) => o.id === limit) ?? pickedString ?? closed[0] ?? null;
  const topOf = surfaces.find((o) => o.id === top) ?? null;
  const bottomOf = surfaces.find((o) => o.id === bottom) ?? null;

  const output = outputs[mode].trim();
  const setOutput = (text: string) => setOutputs((was) => ({ ...was, [mode]: text }));

  const runIt = async (command: string, params: Record<string, unknown>, pick?: (made: ObjectSummary[]) => string | null) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, params);
    setBusy(false);
    if (!outcome.record) return setReport({ notes: [], error: outcome.error });
    setReport({ notes: outcome.record.notes });
    const made = outcome.record.outputs.filter((o) => o.change === 'created').map((o) => o.id);
    const next = pick?.(objects.filter((o) => made.includes(o.id)));
    if (next) select(next);
    else if (made.length === 1) select(made[0]);
  };

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
  const action: ToolAction = (() => {
    switch (mode) {
      case 'strips':
        return {
          label: `Slice ${plural(chosen.length, 'solid')} into strips`,
          icon: <Slice className="size-3.5" />,
          disabled: chosen.length === 0 || strips.strips.length === 0 || strips.unnumbered.length > 0 || output === '',
          busy,
          busyLabel: 'Slicing…',
          title: 'Each strip is what lies in front of its face of what the strips before it left, so they add up to the solid',
          run: () => runIt('solid.slice_by_faces', { solids: chosen.map((o) => o.id), faces: faces.map((o) => o.id), layer: output }),
        };
      case 'cut':
        return {
          label: `Cut ${plural(chosen.length, 'solid')}`,
          disabled: chosen.length === 0 || by === null || output === '',
          busy,
          busyLabel: 'Cutting…',
          run: () => runIt('solid.cut_by_surface', { solids: chosen.map((o) => o.id), ...(by?.kind === 'rl' ? { rl: by.rl } : { surface: by?.id }), keep, layer: output }),
        };
      case 'between':
        return {
          label: 'Make the solid',
          disabled: !around || !topOf || !bottomOf || topOf.id === bottomOf.id || output === '',
          busy,
          busyLabel: 'Making…',
          run: () => runIt('solid.between_surfaces', { top: topOf?.id, bottom: bottomOf?.id, string: around?.id, layer: output, ...(name.trim() ? { name: name.trim() } : {}) }),
        };
    }
  })();

  const solidsSection = (
    <ToolSection title="Solids">
      <Field label="Take" hint={chosen.length === 0 ? 'Pick a solid in the view or the tree, or take those on show' : chosen.length === 1 ? chosen[0].name : `${chosen.length} solids, each on its own`}>
        <Choice value={scope} options={SCOPES} labels={scopeLabels} onChange={setScope} label="Which solids" />
      </Field>
    </ToolSection>
  );

  const outputSection = (
    <ToolSection title="Output">
      <Field label="To layer">
        <Input className="h-7 w-40 px-1.5 text-xs" value={outputs[mode]} aria-label="Layer for what's made" onChange={(e) => setOutput(e.target.value)} />
      </Field>
      {mode === 'between' && (
        <Field label="Name">
          <Input className="h-7 w-40 px-1.5 text-xs" value={name} aria-label="Name of the solid" onChange={(e) => setName(e.target.value)} />
        </Field>
      )}
    </ToolSection>
  );

  return (
    <ToolPanel
      title="Cut solids"
      panel="cut"
      summary="Slice solids into strips by faces, cut them by a surface, grid or RL, or make one between two surfaces. What's given stays as it is."
      result={report}
      action={action}
    >
      <ToolSection>
        <Choice value={mode} options={MODES} labels={MODE_LABELS} onChange={setMode} label="What to do" />
      </ToolSection>

      {mode !== 'between' && solidsSection}

      {mode === 'strips' && (
        <ToolSection
          title="Faces"
          aside={
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-xs"
              disabled={busy || faces.length === 0 || strips.unnumbered.length > 0}
              title="Make one face for each strip number on a layer of its own, to look at or export: up and down, or several runs, joined"
              onClick={() => void runIt('face.join', { faces: faces.map((o) => o.id), layer: `${layerName(facesOn ?? '')} joined` })}
            >
              <Combine className="size-3.5" /> Join a face a strip
            </Button>
          }
        >
          <Field label="On layer" hint={facesLine(strips)}>
            {faceLayers.length === 0 ? (
              <span className="text-muted-foreground">No faces with a strip number yet: number the toes with a line across, then build</span>
            ) : (
              <Select value={facesOn ?? ''} aria-label="Layer of faces" onChange={(e) => setFacesLayer(e.target.value)}>
                {faceLayers.map((l) => (
                  <option key={l.id} value={l.id}>
                    {layerName(l.id)} ({l.count})
                  </option>
                ))}
              </Select>
            )}
          </Field>
          {strips.unnumbered.length > 0 && (
            <div className="text-warning">
              {plural(strips.unnumbered.length, 'face')} on it {strips.unnumbered.length === 1 ? 'has' : 'have'} no strip number: {strips.unnumbered.slice(0, 3).map((o) => o.name).join(', ')}
              {strips.unnumbered.length > 3 ? '…' : ''}
            </div>
          )}
          <div className="text-muted-foreground">Strip n is the solid in front of face n, back to the face before. What's behind the last face is kept too, as the next strip, to delete if it isn't one.</div>
        </ToolSection>
      )}

      {mode === 'cut' && (
        <>
          <ToolSection title="Cut by">
            <Field label="Surface">
              <TargetInput value={by} surfaces={surfaces} onChange={setBy} label="Surface, grid or RL to cut by" none="Choose…" />
            </Field>
            <Field label="Keep">
              <Choice value={keep} options={['below', 'above', 'both'] as const} labels={{ below: 'Below', above: 'Above', both: 'Both' }} onChange={setKeep} label="Which side to keep" />
            </Field>
          </ToolSection>
        </>
      )}

      {mode === 'between' && (
        <ToolSection title="Between">
          <Field label="Inside" hint={around && !around.closed ? "This string isn't closed: it's taken as closed from its last point to its first" : undefined}>
            {strings.length === 0 ? (
              <span className="text-muted-foreground">Draw a closed string round the solid first</span>
            ) : (
              <Select value={around?.id ?? ''} aria-label="The closed string around it" onChange={(e) => setLimit(e.target.value)}>
                {[...closed, ...strings.filter((o) => !o.closed)].map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                    {o.closed ? '' : ' (open)'}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Top">
            <Select value={topOf?.id ?? ''} aria-label="The surface or grid on top" onChange={(e) => setTop(e.target.value || null)}>
              <option value="">Choose…</option>
              <TargetOptions surfaces={surfaces} />
            </Select>
          </Field>
          <Field label="Bottom">
            <Select value={bottomOf?.id ?? ''} aria-label="The surface or grid underneath" onChange={(e) => setBottom(e.target.value || null)}>
              <option value="">Choose…</option>
              <TargetOptions surfaces={surfaces} />
            </Select>
          </Field>
        </ToolSection>
      )}

      {outputSection}
    </ToolPanel>
  );
}
