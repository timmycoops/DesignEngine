import { Combine, Grid3x3, Layers, Slice } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { checkCutByPolygons, checkSlice, type Note, type ObjectSummary, type StepTarget } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { Choice, Field, NoteLine, NumberInput, type RunReport, Select, TargetInput, TargetOptions, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { byStrip, facesLine } from './strips';

/**
 * Cut solids (decision 0049, UI plan): one panel for every cut. Cut solids by a surface, a grid or an RL, keeping either
 * side; slice them into strips by faces, each face told apart by its strip number, a solid a strip adding up to the
 * solid; cut them by a seam model's horizons in one run, each piece carrying its seam, material and horizon, or into
 * blocks by polygons, each piece taking its polygon's name (decision 0062); or make the large solid between two surfaces
 * inside a closed string. The solids, faces and polygons stay as they are, and each run is one undo. Made from the kit,
 * as every tool panel is.
 */

type Mode = 'strips' | 'seams' | 'polygons' | 'cut' | 'between';
type Scope = 'picked' | 'layer' | 'visible';

const MODES: readonly Mode[] = ['strips', 'seams', 'polygons', 'cut', 'between'];
const MODE_LABELS: Record<Mode, string> = { strips: 'Into strips', seams: 'By seams', polygons: 'By polygons', cut: 'By a surface', between: 'Between surfaces' };
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
  const [model, setModel] = useState<string | null>(null);
  const [minThickness, setMinThickness] = useState(0.01);
  const [polygonsLayer, setPolygonsLayer] = useState<string | null>(null);
  const [patterns, setPatterns] = useState({ seams: '{solid} {horizon}', polygons: '{strip}_{block}_{horizon}' });
  const [outputs, setOutputs] = useState<Record<Mode, string>>({ strips: 'strips', seams: 'layers', polygons: 'blocks', cut: 'cut', between: 'pit' });
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

  // The seam model: the one chosen, or the last built.
  const models = objects.filter((o) => o.kind === 'seam_model');
  const modelOf = models.find((o) => o.id === model) ?? models[models.length - 1] ?? null;

  // The block polygons: the closed strings on a layer. The layer with the most of them comes first.
  const polygonLayers = useMemo(() => {
    const counts = new Map<string, number>();
    for (const o of objects) if (o.kind === 'string' && o.closed) counts.set(o.layer, (counts.get(o.layer) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([id, count]) => ({ id, count }));
  }, [objects]);
  const polygonsOn = polygonsLayer ?? polygonLayers[0]?.id ?? null;
  const polygons = closed.filter((o) => o.layer === polygonsOn);
  const pattern = mode === 'seams' || mode === 'polygons' ? patterns[mode] : '';
  const setPattern = (text: string) => mode === 'seams' || mode === 'polygons' ? setPatterns((was) => ({ ...was, [mode]: text })) : undefined;

  // What a cut would refuse, checked before it each time the solids or what cuts them change: into strips, any face
  // that doesn't pass right through a solid, past its ends, above its top and below its floor; by polygons, any part of a
  // solid outside them all. Each is listed, and the cut waits until it's fixed.
  const ids = (list: ObjectSummary[]) => list.map((o) => `${o.id}:${o.revision}`).join(',');
  const checkKey =
    mode === 'strips' && chosen.length > 0 && strips.strips.length > 0 && strips.unnumbered.length === 0
      ? `strips|${ids(chosen)}|${ids(faces)}`
      : mode === 'polygons' && chosen.length > 0 && polygons.length > 0
        ? `polygons|${ids(chosen)}|${ids(polygons)}`
        : null;
  const [check, setCheck] = useState<{ key: string; notes?: Note[]; error?: string } | null>(null);
  useEffect(() => {
    if (checkKey === null) return;
    let stale = false;
    const solidIds = chosen.map((o) => o.id);
    const cutIds = (mode === 'strips' ? faces : polygons).map((o) => o.id);
    const ask = mode === 'strips' ? checkSlice : checkCutByPolygons;
    const wait = setTimeout(() => {
      setCheck({ key: checkKey });
      ask(solidIds, cutIds).then(
        (notes) => !stale && setCheck({ key: checkKey, notes }),
        (error: unknown) => !stale && setCheck({ key: checkKey, error: String(error) }),
      );
    }, 250);
    return () => {
      stale = true;
      clearTimeout(wait);
    };
    // The key holds the mode and every id and revision the check reads.
  }, [checkKey]);
  const checked = check && check.key === checkKey ? check : null;
  const shortfalls = checked?.notes?.filter((n) => n.severity === 'warning') ?? [];
  const theSolids = chosen.length === 1 ? 'the solid' : `the ${chosen.length} solids`;
  /** The check's state in words: checking, passed, or what's wrong, each listed. */
  const checkStatus = (label: string, checking: string, passed: string, failed: string) =>
    checkKey !== null && (
      <div className="flex flex-col gap-0.5" role="status" aria-label={label}>
        {!checked || (!checked.notes && !checked.error) ? (
          <span className="text-muted-foreground">{checking}</span>
        ) : checked.error ? (
          <span className="text-destructive">{checked.error}</span>
        ) : shortfalls.length === 0 ? (
          <span className="text-ok">{passed}</span>
        ) : (
          <>
            <span className="text-warning">{failed}</span>
            <div className="flex max-h-40 flex-col gap-0.5 overflow-auto">
              {shortfalls.map((note, k) => (
                <NoteLine key={k} note={note} index={k + 1} />
              ))}
            </div>
          </>
        )}
      </div>
    );

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
          disabled: chosen.length === 0 || strips.strips.length === 0 || strips.unnumbered.length > 0 || shortfalls.length > 0 || output === '',
          busy,
          busyLabel: 'Slicing…',
          title: 'Each strip is what lies in front of its face of what the strips before it left, so they add up to the solid',
          hint: shortfalls.length > 0 ? 'Fix the faces first' : undefined,
          run: () => runIt('solid.slice_by_faces', { solids: chosen.map((o) => o.id), faces: faces.map((o) => o.id), layer: output }),
        };
      case 'seams':
        return {
          label: `Cut ${plural(chosen.length, 'solid')} by seams`,
          icon: <Layers className="size-3.5" />,
          disabled: chosen.length === 0 || !modelOf || output === '' || pattern.trim() === '',
          busy,
          busyLabel: 'Cutting…',
          title: "Overburden, each seam, each interburden and what's below the last floor, in one run; each piece carries its seam, material and horizon",
          run: () => runIt('solid.cut_by_seams', { solids: chosen.map((o) => o.id), model: modelOf?.id, layer: output, min_thickness: minThickness, name: pattern.trim() }),
        };
      case 'polygons':
        return {
          label: `Cut ${plural(chosen.length, 'solid')} into blocks`,
          icon: <Grid3x3 className="size-3.5" />,
          disabled: chosen.length === 0 || polygons.length === 0 || shortfalls.length > 0 || output === '' || pattern.trim() === '',
          busy,
          busyLabel: 'Cutting…',
          title: 'Each solid cut vertically where each polygon sits, the piece taking its name as its block',
          hint: shortfalls.length > 0 ? 'Fix the polygons first' : undefined,
          run: () => runIt('solid.cut_by_polygons', { solids: chosen.map((o) => o.id), polygons: polygons.map((o) => o.id), layer: output, name: pattern.trim() }),
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
      {(mode === 'seams' || mode === 'polygons') && (
        <Field label="Names" hint="Attributes in braces, {solid} for the solid's name; one a piece hasn't is left out with the separator before it">
          <Input className="h-7 w-40 px-1.5 text-xs" value={pattern} aria-label="Name of each piece" onChange={(e) => setPattern(e.target.value)} />
        </Field>
      )}
    </ToolSection>
  );

  return (
    <ToolPanel
      title="Cut solids"
      panel="cut"
      summary="Slice solids into strips by faces, cut them by seams or into blocks by polygons, cut them by a surface, grid or RL, or make one between two surfaces. What's given stays as it is."
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
          {checkStatus(
            'Faces against the solids',
            `Checking the faces pass right through ${theSolids}…`,
            `Every face passes right through ${theSolids}`,
            'Not every face passes right through: carry these on past the solid, then slice',
          )}
          <div className="text-muted-foreground">
            Every face must pass right through the solid: past it at both ends, above its top and below its floor. Strip n is the solid in front of face n, back to the face before. What's behind the
            last face is kept too, as the next strip, to delete if it isn't one.
          </div>
        </ToolSection>
      )}

      {mode === 'seams' && (
        <ToolSection title="Seams">
          <Field label="Seam model">
            {models.length === 0 ? (
              <span className="text-muted-foreground">Build a seam model first: Seam model on the toolbar</span>
            ) : (
              <Select value={modelOf?.id ?? ''} aria-label="Seam model" onChange={(e) => setModel(e.target.value)}>
                {models.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Thinnest" hint="A layer is made only where it's thicker than this; the volume left out is said">
            <NumberInput value={minThickness} onChange={setMinThickness} label="Thinnest layer made" unit="m" range={{ min: 0, minIncluded: false }} />
          </Field>
          <div className="text-muted-foreground">
            Its working seams' roofs and floors, read where the model links them. Waste goes with the seam below it; what lies off the grids is kept whole on a layer of its own.
          </div>
        </ToolSection>
      )}

      {mode === 'polygons' && (
        <ToolSection title="Polygons">
          <Field label="On layer" hint={polygons.length > 0 ? `${plural(polygons.length, 'block polygon')}, each piece taking its name as its block` : undefined}>
            {polygonLayers.length === 0 ? (
              <span className="text-muted-foreground">No closed strings yet: select lines across the strips and Polygons in Properties</span>
            ) : (
              <Select value={polygonsOn ?? ''} aria-label="Layer of block polygons" onChange={(e) => setPolygonsLayer(e.target.value)}>
                {polygonLayers.map((l) => (
                  <option key={l.id} value={l.id}>
                    {layerName(l.id)} ({l.count})
                  </option>
                ))}
              </Select>
            )}
          </Field>
          {checkStatus(
            'Polygons over the solids',
            `Checking the polygons cover ${theSolids}…`,
            `The polygons cover ${theSolids}`,
            "The polygons don't cover every solid: carry them on over these, then cut",
          )}
          <div className="text-muted-foreground">
            A vertical cut where each polygon sits. The polygons must cover every solid in plan, crest to crest: a solid reaching outside them is the polygons to fix. Overlapping polygons are
            refused.
          </div>
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
