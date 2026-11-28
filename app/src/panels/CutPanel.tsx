import { Combine, Grid3x3, Layers, Rows3, Slice } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { concealed, checkCutByPolygons, checkCutBySeams, checkSlice, type Note, type ObjectSummary, type StepTarget, solidsTable } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { type Asker, AskSteps, Choice, Field, NoteLine, NumberInput, type RunReport, Select, TargetInput, TargetOptions, TextInput, type ToolAction, ToolPanel, ToolSection, useAsking } from '@/design-system/kit';
import { choiceWords } from '@/tools';
import { useWorkspace } from '@/workspace/context';
import { BenchRulesSection } from './BenchRulesSection';
import { type Attributes, type BenchRule, everything, rulesParam } from './benchRules';
import { byStrip, facesLine } from './strips';

/**
 * Cut solids (decision 0049, UI plan): one panel for every cut. Cut solids by a surface, a grid or an RL, keeping either
 * side; slice them into strips by faces, each face told apart by its strip number, a solid a strip adding up to the
 * solid; cut them by a seam model's horizons in one run, each piece carrying its seam, material and horizon, or into
 * blocks by polygons, each piece taking its polygon's name (decision 0062); cut them at bench levels by bench rules, each
 * piece taking its bench (decision 0071); or make the large solid between two surfaces
 * inside a closed string. The solids, faces and polygons stay as they are, and each run is one undo. Made from the kit,
 * as every tool panel is.
 *
 * Each asks for its inputs in turn (decision 0092). A cut asks for the solids (those picked when it opens fill it the
 * first time; what's on show needs none), holds them, then for a cut by a surface the surface or grid to cut by unless
 * one is set, then for the button or a right click, as the rest is the panel's: faces, the seam model, bench rules,
 * polygons. Between surfaces asks for the closed string around it, the top and the bottom, then makes it. It runs, and
 * asks again; Esc steps back a prompt a press. The button always runs with what the panel has.
 */

type Mode = 'strips' | 'seams' | 'benches' | 'polygons' | 'cut' | 'between';

const MODES: readonly Mode[] = ['strips', 'seams', 'benches', 'polygons', 'cut', 'between'];
/** Each choice's name, and the line it puts at the panel's head, from the one tool list. */
const { lines: MODE_LINES } = choiceWords('cut', MODES);

export function CutPanel() {
  const { summary, selected, selection, runRecord, clearRefusal, select, toolAsk, toolMode, setSelection, addingTo, panelsShown, worksOn: scope } = useWorkspace();
  const [mode, setMode] = useState<Mode>('strips');
  // A tool dropped on the workflow canvas asks for one of what this does (decision 0067).
  useEffect(() => {
    if (toolAsk?.panel === 'cut' && MODES.includes(toolAsk.mode as Mode)) setMode(toolAsk.mode as Mode);
  }, [toolAsk]);
  // The ribbon lights the tool the panel is set to.
  useEffect(() => {
    toolMode('cut', mode);
    return () => toolMode('cut', null);
  }, [toolMode, mode]);
  const [by, setBy] = useState<StepTarget | null>(null);
  const [keep, setKeep] = useState<'below' | 'above' | 'both'>('both');
  const [facesLayer, setFacesLayer] = useState<string | null>(null);
  const [top, setTop] = useState<string | null>(null);
  const [bottom, setBottom] = useState<string | null>(null);
  const [limit, setLimit] = useState<string | null>(null);
  const [name, setName] = useState('PIT');
  const [model, setModel] = useState<string | null>(null);
  const [minThickness, setMinThickness] = useState(0.01);
  const [flapThickness, setFlapThickness] = useState(0.01);
  const [polygonsLayer, setPolygonsLayer] = useState<string | null>(null);
  const [patterns, setPatterns] = useState({ seams: '{solid} {horizon}', benches: '{solid} {bench}', polygons: '{strip}_{block}_{horizon}_{bench}' });
  const [outputs, setOutputs] = useState<Record<Mode, string>>({ strips: 'strips', seams: 'layers', benches: 'benches', polygons: 'blocks', cut: 'cut', between: 'pit' });
  const [benchRules, setBenchRules] = useState<BenchRule[]>([everything()]);
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  const layers = summary?.layers ?? [];
  const layerName = (id: string) => layers.find((l) => l.id === id)?.name ?? '';

  // The solids: those selected, all on the layer of the first of them, or all on show.
  const solids = objects.filter((o) => o.kind === 'solid');
  const pickedIds = new Set(selection);
  const selectionNow = useRef(selection);
  selectionNow.current = selection;
  const picked = solids.filter((o) => pickedIds.has(o.id));
  const onLayer = picked.length > 0 ? solids.filter((o) => o.layer === picked[0].layer) : [];
  const shown = new Set(layers.filter((l) => l.visible).map((l) => l.id));
  const visible = solids.filter((o) => shown.has(o.layer) && !concealed(o));
  /** The solids a cut asked for, held while it asks for the rest (decision 0092): what's picked may change meanwhile. */
  const [held, setHeld] = useState<string[] | null>(null);
  const chosen = held ? solids.filter((o) => held.includes(o.id)) : scope === 'picked' ? picked : scope === 'layer' ? onLayer : visible;
  const scopeLabels: Record<typeof scope, string> = { picked: `Picked (${picked.length})`, layer: `Its layer (${onLayer.length})`, visible: `On show (${visible.length})` };

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
  const named = mode === 'seams' || mode === 'benches' || mode === 'polygons';
  const pattern = named ? patterns[mode] : '';
  const setPattern = (text: string) => (named ? setPatterns((was) => ({ ...was, [mode]: text })) : undefined);

  // The solids' attributes, read for By benches so it can show what each rule takes and a tick for each horizon.
  const benchKey = mode === 'benches' && chosen.length > 0 ? chosen.map((o) => `${o.id}:${o.revision}`).join(',') : null;
  const [benchSolids, setBenchSolids] = useState<{ key: string; attributes: Attributes[] } | null>(null);
  useEffect(() => {
    if (benchKey === null) return;
    let stale = false;
    const wait = setTimeout(() => {
      solidsTable(chosen.map((o) => o.id)).then(
        (rows) => !stale && setBenchSolids({ key: benchKey, attributes: rows.map((r) => r.attributes) }),
        () => !stale && setBenchSolids(null),
      );
    }, 150);
    return () => {
      stale = true;
      clearTimeout(wait);
    };
    // The key holds every id and revision read.
  }, [benchKey]);
  const benchAttributes = benchSolids && benchSolids.key === benchKey ? benchSolids.attributes : null;
  const badRule = benchRules.findIndex((r) => r.benched && !(r.every > 0 && r.to >= r.from));

  // What a cut would refuse, checked before it each time the solids or what cuts them change: into strips, any face
  // that doesn't pass right through a solid, past its ends, above its top and below its floor; by seams, any part of a
  // solid past the edge of the model's grids or over a hole in them; by polygons, any part of a solid outside them all.
  // Each is listed, and the cut waits until it's fixed.
  const ids = (list: ObjectSummary[]) => list.map((o) => `${o.id}:${o.revision}`).join(',');
  const checkKey =
    mode === 'strips' && chosen.length > 0 && strips.strips.length > 0 && strips.unnumbered.length === 0
      ? `strips|${ids(chosen)}|${ids(faces)}`
      : mode === 'seams' && chosen.length > 0 && modelOf
        ? `seams|${ids(chosen)}|${ids([modelOf])}`
        : mode === 'polygons' && chosen.length > 0 && polygons.length > 0
          ? `polygons|${ids(chosen)}|${ids(polygons)}`
          : null;
  const [check, setCheck] = useState<{ key: string; notes?: Note[]; error?: string } | null>(null);
  useEffect(() => {
    if (checkKey === null) return;
    let stale = false;
    const solidIds = chosen.map((o) => o.id);
    const cutIds = (mode === 'strips' ? faces : mode === 'seams' ? (modelOf ? [modelOf] : []) : polygons).map((o) => o.id);
    const ask = mode === 'strips' ? checkSlice : mode === 'seams' ? (s: string[], c: string[]) => checkCutBySeams(s, c[0]) : checkCutByPolygons;
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
  // What the button runs, and the same as a step for a workflow (decision 0065).
  const calls = (command: string, params: Record<string, unknown>) => ({ run: () => runIt(command, params), step: { command, params } });
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
          ...calls('solid.slice_by_faces', { solids: chosen.map((o) => o.id), faces: faces.map((o) => o.id), layer: output }),
        };
      case 'seams':
        return {
          label: `Cut ${plural(chosen.length, 'solid')} by seams`,
          icon: <Layers className="size-3.5" />,
          disabled: chosen.length === 0 || !modelOf || shortfalls.length > 0 || output === '' || pattern.trim() === '',
          busy,
          busyLabel: 'Cutting…',
          title: "Overburden, each seam, each interburden and what's below the last floor, in one run; each piece carries its seam, material and horizon",
          hint: shortfalls.length > 0 ? 'Fix the grids or the solids first' : undefined,
          ...calls('solid.cut_by_seams', { solids: chosen.map((o) => o.id), model: modelOf?.id, layer: output, min_thickness: minThickness, flap_thickness: flapThickness, name: pattern.trim() }),
        };
      case 'benches':
        return {
          label: `Bench ${plural(chosen.length, 'solid')}`,
          icon: <Rows3 className="size-3.5" />,
          disabled: chosen.length === 0 || benchRules.length === 0 || badRule >= 0 || output === '' || pattern.trim() === '',
          busy,
          busyLabel: 'Benching…',
          title: 'Each solid cut at the levels of the first rule it fits, each piece taking its bench; a solid no rule benches carried over whole',
          hint: badRule >= 0 ? `Rule ${badRule + 1}: its levels run downward or have no interval` : undefined,
          ...calls('solid.cut_by_benches', { solids: chosen.map((o) => o.id), layer: output, rules: rulesParam(benchRules), flap_thickness: flapThickness, name: pattern.trim() }),
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
          ...calls('solid.cut_by_polygons', { solids: chosen.map((o) => o.id), polygons: polygons.map((o) => o.id), layer: output, name: pattern.trim() }),
        };
      case 'cut':
        return {
          label: `Cut ${plural(chosen.length, 'solid')}`,
          disabled: chosen.length === 0 || by === null || output === '',
          busy,
          busyLabel: 'Cutting…',
          ...calls('solid.cut_by_surface', { solids: chosen.map((o) => o.id), ...(by?.kind === 'rl' ? { rl: by.rl } : { surface: by?.id }), keep, layer: output }),
        };
      case 'between':
        return {
          label: 'Make the solid',
          disabled: !around || !topOf || !bottomOf || topOf.id === bottomOf.id || output === '',
          busy,
          busyLabel: 'Making…',
          ...calls('solid.between_surfaces', { top: topOf?.id, bottom: bottomOf?.id, string: around?.id, layer: output, ...(name.trim() ? { name: name.trim() } : {}) }),
        };
    }
  })();

  /**
   * Asking in turn (decision 0092). The loop outlives a render, so it reads the panel through `latest`: whether the
   * button would run, its run, and what the asks need. The button runs with what the panel has, whatever is asked.
   */
  const asking = useAsking();
  const latest = useRef({ mode, scope, chosen: chosen.map((o) => o.id), by, ready: !action.disabled, run: action.run, hint: action.hint, label: action.label, solids, surfaces, strings, output, name });
  latest.current = { mode, scope, chosen: chosen.map((o) => o.id), by, ready: !action.disabled, run: action.run, hint: action.hint, label: action.label, solids, surfaces, strings, output, name };
  const cutLoop = useCallback(
    async (ask: Asker) => {
      let first = true;
      /** The button pressed where the panel has all it needs: run now. */
      const goNow = async (done: 'done' | 'pressed' | false) => {
        if (done !== 'pressed' || !latest.current.ready) return false;
        await latest.current.run();
        return true;
      };
      const lastOf = (ids: () => string[], fits: (id: string) => boolean) => [...ids()].reverse().find(fits);
      while (ask.alive()) {
        ask.step(0);
        setHeld(null);
        const now = latest.current;
        if (now.mode === 'between') {
          // The closed string around it, the top, the bottom.
          const picks = () => [...latest.current.strings, ...latest.current.surfaces].map((o) => o.id);
          const pickedNow = (kinds: (id: string) => boolean) => lastOf(() => picks().filter((id) => selectionNow.current.includes(id)), kinds);
          const isString = (id: string) => latest.current.strings.some((o) => o.id === id);
          const isSurface = (id: string) => latest.current.surfaces.some((o) => o.id === id);
          let ring = first ? pickedNow(isString) : undefined;
          first = false;
          if (!ring) {
            const done = await ask.gather('Pick the closed string around the solid: a click. Right-click when done; Esc stops');
            if (!done || !ask.alive()) return;
            if (await goNow(done)) continue;
            ring = pickedNow(isString);
            if (!ring) continue;
          }
          setLimit(ring);
          ask.step(1);
          setSelection([]);
          const topDone = await ask.gather('Pick the top: a surface or grid. Right-click when done; Esc goes back');
          if (!ask.alive()) return;
          if (!topDone) continue;
          if (await goNow(topDone)) continue;
          const topId = pickedNow(isSurface);
          if (!topId) continue;
          setTop(topId);
          ask.step(2);
          setSelection([]);
          const bottomDone = await ask.gather('Pick the bottom: a surface or grid. Right-click when done; Esc goes back');
          if (!ask.alive()) return;
          if (!bottomDone) continue;
          const bottomId = pickedNow((id) => isSurface(id) && id !== topId);
          if (!bottomId) continue;
          setBottom(bottomId);
          ask.step(3);
          await runIt('solid.between_surfaces', { top: topId, bottom: bottomId, string: ring, layer: latest.current.output, ...(latest.current.name.trim() ? { name: latest.current.name.trim() } : {}) });
          continue;
        }
        // A cut: the solids, held; for a cut by a surface, the surface unless one's set; then the button or a right click.
        if (!(first && (now.scope === 'visible' || now.chosen.length > 0))) {
          const done = await ask.gather('Pick the solids to cut: a click, a box or Ctrl-click. Right-click when done; Esc stops');
          if (!done || !ask.alive()) return;
          first = false;
          if (await goNow(done)) continue;
        }
        first = false;
        if (latest.current.chosen.length === 0) continue;
        // Held while the rest is asked for, and picked again once it's run, ready for the next cut.
        const holding = latest.current.chosen;
        setHeld(holding);
        ask.step(1);
        if (latest.current.mode === 'cut' && latest.current.by === null) {
          setSelection([]);
          const done = await ask.gather('Pick the surface or grid to cut by: a click, or type an RL in the panel. Right-click when done; Esc goes back');
          if (!ask.alive()) return;
          if (!done) continue;
          if (await goNow(done)) {
            setSelection(holding);
            continue;
          }
          const surface = lastOf(() => selectionNow.current, (id) => latest.current.surfaces.some((o) => o.id === id));
          if (!surface) continue;
          setBy({ kind: 'surface', id: surface });
          ask.step(2);
        }
        const confirm = await ask.gather(`Check the settings, then ${latest.current.label} on the button, or right-click; Esc goes back`);
        if (!ask.alive()) return;
        if (!confirm) {
          setSelection(holding);
          continue;
        }
        if (latest.current.ready) {
          await latest.current.run();
          setSelection(holding);
        } else setReport({ notes: [], error: latest.current.hint ?? 'Not ready: the panel shows what it needs' });
      }
    },
    // Everything it reads changes is read through `latest` and `selectionNow`.
    [setSelection],
  );
  const asks = !addingTo && panelsShown.includes('cut');
  useEffect(() => {
    // Let go first: the loop's first steps run at once, and may hold solids straight away.
    setHeld(null);
    if (asks) asking.start(cutLoop);
    else asking.stop();
  }, [asks, mode, asking.start, asking.stop, cutLoop]);
  // Picked again from the ribbon or Find a command as this same tool, it asks afresh. Picked as another of this panel's
  // tools, the mode changes and that starts it: the ask for another mode must not restart this one first.
  const askedAgain = toolAsk?.panel === 'cut' && toolAsk.mode === mode ? toolAsk.n : null;
  const seenAsk = useRef({ key: askedAgain, mode });
  useEffect(() => {
    const was = seenAsk.current;
    seenAsk.current = { key: askedAgain, mode };
    if (askedAgain !== null && askedAgain !== was.key && mode === was.mode && asks) {
      setHeld(null);
      asking.start(cutLoop);
    }
  }, [askedAgain, mode, asks, asking.start, cutLoop]);
  useEffect(() => () => asking.stop(), [asking.stop]);
  const askSteps =
    mode === 'between'
      ? [{ label: 'The closed string around it', value: around?.name }, { label: 'The top', value: topOf?.name }, { label: 'The bottom', value: bottomOf?.name }, { label: 'The solid, made' }]
      : [
          { label: 'The solids to cut', value: chosen.length > 0 ? (chosen.length === 1 ? chosen[0].name : `${chosen.length} solids`) : undefined },
          ...(mode === 'cut' ? [{ label: 'The surface or grid to cut by', value: by?.kind === 'rl' ? `RL ${by.rl}` : surfaces.find((o) => by?.kind === 'surface' && o.id === by.id)?.name }] : []),
          { label: `${action.label}: the button, or a right click` },
        ];
  const askingAction: ToolAction = asks ? { ...action, run: () => asking.next(cutLoop), disabled: asking.at === null ? false : action.disabled } : action;

  const solidsSection = (
    <ToolSection title="Solids">
      <Field label="Take" hint={chosen.length === 0 ? 'Pick a solid in the view or the Objects table, or take those on show' : chosen.length === 1 ? chosen[0].name : `${chosen.length} solids, each on its own`}>
        <span data-slot="works-on-said" title="Set at the foot of the view, for every tool that takes solids">
          {scopeLabels[scope]} <span className="text-muted-foreground">· at the foot of the view</span>
        </span>
      </Field>
    </ToolSection>
  );

  const outputSection = (
    <ToolSection title="Output">
      <Field label="To layer" hint={mode === 'benches' ? "Where a rule names no layer of its own" : undefined}>
        <TextInput className="w-40" value={outputs[mode]} label="Layer for what's made" onChange={(value) => setOutput(value)} />
      </Field>
      {mode === 'between' && (
        <Field label="Name">
          <TextInput className="w-40" value={name} label="Name of the solid" onChange={(value) => setName(value)} />
        </Field>
      )}
      {named && (
        <Field label="Names" hint="Attributes in braces, {solid} for the solid's name; one a piece hasn't is left out with the separator before it">
          <TextInput className="w-40" value={pattern} label="Name of each piece" onChange={(value) => setPattern(value)} />
        </Field>
      )}
    </ToolSection>
  );

  return (
    <ToolPanel
      title="Cut solids"
      panel="cut"
      summary={MODE_LINES[mode]}
      more="What's given stays as it is."
      result={report}
      action={askingAction}
    >
      <AskSteps steps={askSteps} at={asking.at} />

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
          <Field label="Flaps" hint="Where a piece is thinner than this between its top and bottom, as where a berm lies on a seam's roof, it's taken off; the volume is said. 0 for none">
            <NumberInput value={flapThickness} onChange={setFlapThickness} label="Flap thickness" unit="m" range={{ min: 0 }} />
          </Field>
          {checkStatus(
            'Grids under the solids',
            `Checking the grids reach over ${theSolids}…`,
            `The grids reach over ${theSolids}, with no hole under them`,
            "The grids don't reach over every solid: fix the geology model and import the grids again, or keep the solids to where the grids are",
          )}
          <div className="text-muted-foreground">
            Its working seams' roofs and floors, read where the model links them. Waste goes with the seam below it. The grids must reach over every solid in plan with no hole under
            it; holes are never filled.
          </div>
        </ToolSection>
      )}

      {mode === 'benches' && <BenchRulesSection rules={benchRules} onChange={setBenchRules} solids={benchAttributes} flapThickness={flapThickness} onFlapThickness={setFlapThickness} />}

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
