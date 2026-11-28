import { Check } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { concealed, type ObjectSummary } from '@/bridge';
import { type Asker, AskSteps, Choice, Field, NumberInput, NumberOrBlank, resultParams, type RunReport, TextInput, type ToolAction, ToolPanel, ToolSection, useAsking, useResultAs } from '@/design-system/kit';
import { nearestString } from './stringSide';
import { choiceWords } from '@/tools';
import { useWorkspace } from '@/workspace/context';

/**
 * Surfaces (Surface tools plan, decision 5): a panel of its own, not inside Face design, for the tools that work on
 * surfaces and grids. Its tools are on the Walls row; Compare is on the Reserves row too.
 *
 * - Compare (workstream 1, decision 0093): the cut and fill between two surfaces or grids, exact, with a column check
 *   alongside, inside a closed string or wherever both are; a thickness grid if switched on, off by default. It asks in
 *   turn (decision 0092): the surface compared against, the one compared, then a closed string or a right click for
 *   wherever both are; it runs, and asks again. Esc steps back a prompt a press. Two surfaces picked when it opens fill
 *   the first two steps. The button runs with what the panel has.
 * - Merge (workstream 2, decision 0094): the bottom of the surfaces, grids and solids picked (dug out) or their top
 *   (laid on), a solid counting by its floor or its top, as a grid at a cell size or an exact surface. It asks what to
 *   merge (what's picked already counts) and runs on a right click or the button, then asks again.
 * - Trim in plan (workstream 3, decision 0095): the strings, surfaces, solids and grids picked, cut by a closed string
 *   in plan, keeping the inside or the outside, each by its own kind's rule, in place or as new on a layer (0091). It
 *   asks what to trim (what's picked already counts), then for a click on the closed string, runs, and asks again.
 * - Triangulate (workstream 4, decision 0096): one surface through the strings picked, each segment kept as an edge,
 *   bounded by the outermost closed strings or, with none closed, the hull with no edge along it over the longest set.
 *   It asks for the strings (what's picked already counts) and runs on a right click or the button, then asks again.
 *
 * While the Workflow panel is adding steps, the button adds the tool as a step instead (decision 0078).
 */

type Mode = 'compare' | 'merge' | 'trim' | 'triangulate';

const MODES: readonly Mode[] = ['compare', 'merge', 'trim', 'triangulate'];

const MERGES = new Set(['surface', 'grid', 'solid']);
const TRIMS = new Set(['string', 'surface', 'grid', 'solid']);

/** A click this close to a string in plan (m) is on it. */
const CLICK_ON_A_STRING = 2;
const { lines: MODE_LINES } = choiceWords('surfaces', MODES);

const SURFACES = new Set(['surface', 'grid']);

export function SurfacesPanel() {
  const { summary, selection, runRecord, clearRefusal, toolAsk, toolMode, setSelection, addingTo, panelsShown, pickPoint } = useWorkspace();
  const [mode, setMode] = useState<Mode>('compare');
  useEffect(() => {
    if (toolAsk?.panel === 'surfaces' && MODES.includes(toolAsk.mode as Mode)) setMode(toolAsk.mode as Mode);
  }, [toolAsk]);
  useEffect(() => {
    toolMode('surfaces', mode);
    return () => toolMode('surfaces', null);
  }, [toolMode, mode]);

  const [cell, setCell] = useState(2);
  const [thickness, setThickness] = useState<'off' | 'on'>('off');
  const [thicknessLayer, setThicknessLayer] = useState('thickness');
  /** What Compare has been given: the surface compared against, the one compared, and the string to compare inside. */
  const [against, setAgainst] = useState<string | null>(null);
  const [compared, setCompared] = useState<string | null>(null);
  const [inside, setInside] = useState<string | null>(null);
  /** Merge: bottom or top, a grid or an exact surface, the grid's cell (blank: 5 m, or grids alike node by node). */
  const [keep, setKeep] = useState<'bottom' | 'top'>('bottom');
  const [output, setOutput] = useState<'grid' | 'surface'>('grid');
  const [mergeCell, setMergeCell] = useState<number | null>(5);
  const [mergeName, setMergeName] = useState('');
  const [mergeLayer, setMergeLayer] = useState('merged');
  /** Trim: what it keeps, the string it last trimmed by, and in place or as new on a layer (decision 0091). */
  const [trimKeep, setTrimKeep] = useState<'inside' | 'outside'>('inside');
  const [trimBy, setTrimBy] = useState<string | null>(null);
  const resultAs = useResultAs();
  /** Triangulate: the longest hull edge kept with no closed string, and the surface's name and layer. */
  const [longest, setLongest] = useState(200);
  const [tinName, setTinName] = useState('');
  const [tinLayer, setTinLayer] = useState('triangulated');
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  const byId = (id: string | null) => (id ? objects.find((o) => o.id === id) ?? null : null);
  const inOrder = selection.map((id) => objects.find((o) => o.id === id)).filter((o): o is ObjectSummary => o !== undefined);
  const surfaces = inOrder.filter((o) => SURFACES.has(o.kind));
  const closedStrings = inOrder.filter((o) => o.kind === 'string' && o.closed);
  const a = byId(against) ?? surfaces[0] ?? null;
  const b = byId(compared) ?? (surfaces.length > 1 ? surfaces[1] : null);
  const ring = byId(inside);
  const toMerge = inOrder.filter((o) => MERGES.has(o.kind));
  const toTrim = inOrder.filter((o) => TRIMS.has(o.kind));
  const trimWhat = toTrim.length === 1 ? toTrim[0].name : `${toTrim.length} things`;
  const toTriangulate = inOrder.filter((o) => o.kind === 'string');
  const anyClosed = toTriangulate.some((o) => o.closed);

  const runIt = async (command: string, params: Record<string, unknown>) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, params);
    setBusy(false);
    setReport(outcome.record ? { notes: outcome.record.notes } : { notes: [], error: outcome.error });
    const made = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (made.length > 0) setSelection(made);
    return outcome;
  };
  const compareParams = (first: string | undefined, second: string | undefined, string: string | null | undefined) => ({
    surface: first,
    with: second,
    ...(string ? { string } : {}),
    cell,
    ...(thickness === 'on' ? { thickness: true, layer: thicknessLayer.trim() || 'thickness' } : {}),
  });

  const mergeParams = () => ({
    objects: toMerge.map((o) => o.id),
    keep,
    output,
    ...(output === 'grid' && mergeCell !== null ? { cell: mergeCell } : {}),
    layer: mergeLayer.trim() || 'merged',
    ...(mergeName.trim() ? { name: mergeName.trim() } : {}),
  });

  const tinParams = () => ({
    strings: toTriangulate.map((o) => o.id),
    longest,
    layer: tinLayer.trim() || 'triangulated',
    ...(tinName.trim() ? { name: tinName.trim() } : {}),
  });
  const trimParams = (by: string, objects: string[]) => ({ objects, by, keep: trimKeep, ...resultParams(resultAs) });
  /** A click on a closed string in the view, other than those picked to trim: what they're trimmed by. Null on Esc. */
  const clickLimit = async (prompt: string) => {
    // What's to be trimmed as the prompt opens: the click picks the string it lands on too.
    const picked = new Set(latest.current.toTrim.map((o) => o.id));
    const point = await pickPoint(prompt);
    // The project as it stands when the click lands, not when the prompt opened: a string drawn meanwhile counts.
    const now = latest.current.summary;
    const origin = now?.origin;
    if (!point || !origin) return null;
    const others = (now?.objects ?? [])
      .filter((o) => o.kind === 'string' && !concealed(o) && o.vertices_local && !picked.has(o.id))
      .map((o) => ({ id: o.id, name: o.name, closed: !!o.closed, vertices: (o.vertices_local ?? []).map((v) => [v[0] + origin[0], v[1] + origin[1], v[2] + origin[2]] as const) }));
    const found = nearestString(others, point, CLICK_ON_A_STRING);
    if (!found) setReport({ notes: [], error: 'No string there: click on the closed string itself, not one of those picked' });
    else if (!found.closed) setReport({ notes: [], error: `${found.name} isn't closed, so it has no inside to trim to: close it first, or click a closed string` });
    return found?.closed ? found : null;
  };

  /** Compare, Merge and Trim ask in turn (decision 0092). The loops outlive a render, so they read the panel through `latest`. */
  const asking = useAsking();
  const latest = useRef({ summary, surfaces, closedStrings, compareParams, runIt, toMerge, mergeParams, toTrim, trimWhat, trimParams, clickLimit, toTriangulate, tinParams });
  latest.current = { summary, surfaces, closedStrings, compareParams, runIt, toMerge, mergeParams, toTrim, trimWhat, trimParams, clickLimit, toTriangulate, tinParams };
  const tinLoop = useCallback(async (ask: Asker) => {
    let first = true;
    while (ask.alive()) {
      ask.step(0);
      // Started by the button with strings picked: the button runs with what the panel has.
      const now = first && ask.byButton && latest.current.toTriangulate.length > 0;
      first = false;
      const done = now ? 'pressed' : await ask.gather('Pick the strings to triangulate: a click, a box or Ctrl-click. Right-click or the button to make the surface; Esc stops');
      if (!done || !ask.alive()) return;
      if (latest.current.toTriangulate.length === 0) continue;
      ask.step(1);
      await latest.current.runIt('surface.triangulate', latest.current.tinParams());
    }
  }, []);
  const trimLoop = useCallback(
    async (ask: Asker) => {
      let first = true;
      while (ask.alive()) {
        ask.step(0);
        setTrimBy(null);
        const now = () => latest.current;
        if (!(first && now().toTrim.length > 0)) {
          const done = await ask.gather('Pick what to trim: strings, surfaces, solids or grids, a click, a box or Ctrl-click. Right-click when done; Esc stops');
          if (!done || !ask.alive()) return;
        }
        first = false;
        if (now().toTrim.length === 0) continue;
        ask.step(1);
        // What's picked now is what's trimmed: the click on the string picks it as well.
        const trimming = now().toTrim.map((o) => o.id);
        const limit = await now().clickLimit(`Click the closed string to trim ${now().trimWhat} by. Esc goes back`);
        if (!ask.alive()) return;
        if (!limit) continue;
        setTrimBy(limit.id);
        ask.step(2);
        const outcome = await now().runIt('object.trim', now().trimParams(limit.id, trimming));
        const kept = outcome.record?.outputs.filter((o) => o.change === 'created' || o.change === 'revised').map((o) => o.id) ?? [];
        if (kept.length > 0) setSelection(kept);
      }
    },
    [setSelection],
  );
  const mergeLoop = useCallback(async (ask: Asker) => {
    let first = true;
    while (ask.alive()) {
      ask.step(0);
      // Started by the button with what to merge picked: the button runs with what the panel has.
      const now = first && ask.byButton && latest.current.toMerge.length >= 2;
      first = false;
      const done = now ? 'pressed' : await ask.gather('Pick the surfaces, grids and solids to merge: a box or Ctrl-click. Right-click or the button to merge; Esc stops');
      if (!done || !ask.alive()) return;
      if (latest.current.toMerge.length < 2) continue;
      ask.step(1);
      await latest.current.runIt('surface.merge', latest.current.mergeParams());
    }
  }, []);
  const compareLoop = useCallback(
    async (ask: Asker) => {
      let first = true;
      while (ask.alive()) {
        ask.step(0);
        setAgainst(null);
        setCompared(null);
        setInside(null);
        const now = () => latest.current;
        let x = first && now().surfaces.length >= 2 ? now().surfaces[0] : undefined;
        let y = first && now().surfaces.length >= 2 ? now().surfaces[1] : undefined;
        const ringFirst = first ? now().closedStrings.at(-1) : undefined;
        first = false;
        if (!x || !y) {
          const done = await ask.gather('Pick the surface or grid to compare against, the topography, say. Right-click when done; Esc stops');
          if (!done || !ask.alive()) return;
          x = now().surfaces.at(-1);
          if (!x) continue;
          setAgainst(x.id);
          ask.step(1);
          setSelection([]);
          const second = await ask.gather(`Pick the one to compare with ${x.name}, the design, say. Right-click when done; Esc goes back`);
          if (!ask.alive()) return;
          if (!second) continue;
          const from = x;
          y = [...now().surfaces].reverse().find((o) => o.id !== from.id);
          if (!y) continue;
        }
        setAgainst(x.id);
        setCompared(y.id);
        ask.step(2);
        let string = ringFirst?.id;
        if (!string) {
          setSelection([]);
          const done = await ask.gather('Pick a closed string to compare inside, or right-click for wherever both are; Esc goes back');
          if (!ask.alive()) return;
          if (!done) continue;
          string = now().closedStrings.at(-1)?.id;
        }
        setInside(string ?? null);
        ask.step(3);
        await now().runIt('surface.compare', now().compareParams(x.id, y.id, string));
      }
    },
    [setSelection],
  );
  const asks = !addingTo && panelsShown.includes('surfaces');
  const loop = mode === 'merge' ? mergeLoop : mode === 'trim' ? trimLoop : mode === 'triangulate' ? tinLoop : compareLoop;
  useEffect(() => {
    if (asks) asking.start(loop);
    else asking.stop();
  }, [asks, asking.start, asking.stop, loop]);
  // Picked again from the ribbon or Find a command as this same tool, it asks afresh.
  const askedAgain = toolAsk?.panel === 'surfaces' && toolAsk.mode === mode ? toolAsk.n : null;
  const seenAsk = useRef({ key: askedAgain, mode });
  useEffect(() => {
    const was = seenAsk.current;
    seenAsk.current = { key: askedAgain, mode };
    if (askedAgain !== null && askedAgain !== was.key && mode === was.mode && asks) asking.start(loop);
  }, [askedAgain, mode, asks, asking.start, loop]);
  useEffect(() => () => asking.stop(), [asking.stop]);

  const ready = !!a && !!b && a.id !== b.id && cell > 0;
  const which = keep === 'bottom' ? 'bottom' : 'top';
  const trimmedBy = byId(trimBy);
  const tinWhat = toTriangulate.length === 1 ? toTriangulate[0].name : `${toTriangulate.length} strings`;
  const action: ToolAction = mode === 'triangulate' ? {
    label: toTriangulate.length > 0 ? `Triangulate ${tinWhat}` : 'Triangulate',
    icon: <Check className="size-3.5" />,
    disabled: asking.at === null ? false : toTriangulate.length === 0,
    busy,
    title: 'One surface through every vertex at its RL, each segment kept as an edge',
    run: () => asking.next(tinLoop),
    step: { command: 'surface.triangulate', params: tinParams() },
  } : mode === 'trim' ? {
    label: toTrim.length > 0 ? `Trim ${trimWhat} to ${trimKeep} a closed string` : 'Trim in plan',
    icon: <Check className="size-3.5" />,
    disabled: asking.at === null ? false : toTrim.length === 0,
    busy,
    title: `A click on the closed string: each keeps what's ${trimKeep} it, by its own kind's rule`,
    run: () => asking.next(trimLoop),
    result: resultAs,
    step: {
      command: 'object.trim',
      params: { objects: toTrim.map((o) => o.id), keep: trimKeep, ...resultParams(resultAs) },
      ask: async () => {
        const trimming = toTrim.map((o) => o.id);
        const limit = await clickLimit(`Click the closed string to trim ${trimWhat} by. Esc cancels`);
        return limit ? trimParams(limit.id, trimming) : null;
      },
    },
  } : mode === 'merge' ? {
    label: toMerge.length >= 2 ? `Merge the ${which} of ${toMerge.length} as ${output === 'grid' ? 'a grid' : 'an exact surface'}` : 'Merge surfaces',
    icon: <Check className="size-3.5" />,
    disabled: asking.at === null ? false : toMerge.length < 2,
    busy,
    title: output === 'surface' ? 'Exact: every crest, toe and berm where they are, joined where they meet. It takes longer' : 'A grid at the cell set: fast at any size',
    run: () => asking.next(mergeLoop),
    step: { command: 'surface.merge', params: mergeParams() },
  } : {
    label: a && b ? `Compare ${b.name} with ${a.name}` : 'Compare surfaces',
    icon: <Check className="size-3.5" />,
    disabled: asking.at === null ? false : !ready && asking.at !== 0 && asking.at !== 1 && asking.at !== 2,
    busy,
    title: 'Cut where the second is below the first, fill where it is above, each exact, with a column check alongside',
    run: () => asking.next(compareLoop),
    step: { command: 'surface.compare', params: compareParams(a?.id, b?.id, ring?.id) },
  };

  return (
    <ToolPanel title="Surfaces" panel="surfaces" summary={MODE_LINES[mode]} result={report} action={action}>

      {mode === 'merge' && (
        <AskSteps steps={[{ label: 'The surfaces, grids and solids to merge', value: toMerge.length > 0 ? `${toMerge.length} picked` : undefined }, { label: `The ${which}, merged` }]} at={asking.at} />
      )}

      {mode === 'triangulate' && (
        <AskSteps steps={[{ label: 'The strings to triangulate', value: toTriangulate.length > 0 ? tinWhat : undefined }, { label: 'The surface through them' }]} at={asking.at} />
      )}

      {mode === 'triangulate' && (
        <ToolSection title="Settings">
          <Field label="Longest edge" hint={anyClosed ? 'Not used: the outermost closed strings picked bound the surface' : 'With no closed string picked, the hull is peeled back wherever an edge along it is longer than this'}>
            <NumberInput value={longest} onChange={setLongest} label="Longest hull edge" unit="m" range={{ min: 0, minIncluded: false }} />
          </Field>
          <Field label="Name">
            <TextInput value={tinName} placeholder={toTriangulate.length === 1 ? `${toTriangulate[0].name} surface` : 'Triangulated'} label="Name for the surface" onChange={(value) => setTinName(value)} />
          </Field>
          <Field label="Layer">
            <TextInput value={tinLayer} label="Layer for the surface" onChange={(value) => setTinLayer(value)} />
          </Field>
        </ToolSection>
      )}

      {mode === 'trim' && (
        <AskSteps
          steps={[
            { label: 'What to trim', value: toTrim.length > 0 ? trimWhat : undefined },
            { label: 'The closed string to trim by', value: asking.at !== null && asking.at > 1 ? trimmedBy?.name : undefined },
            { label: `What's ${trimKeep} it, kept` },
          ]}
          at={asking.at}
        />
      )}

      {mode === 'trim' && (
        <ToolSection title="Settings">
          <Field label="Keep" hint={trimKeep === 'inside' ? "What's inside: a string cut at its own heights, a surface exactly along it, a solid vertically and closed, a grid's nodes outside emptied" : "What's outside, each by the same rules"}>
            <Choice value={trimKeep} options={['inside', 'outside'] as const} labels={{ inside: 'Inside', outside: 'Outside' }} onChange={setTrimKeep} label="What the trim keeps" />
          </Field>
        </ToolSection>
      )}

      {mode === 'compare' && <AskSteps
        steps={[
          { label: 'The surface or grid to compare against', value: a?.name },
          { label: 'The one compared', value: b?.name },
          { label: 'A closed string, or wherever both are', value: asking.at !== null && asking.at > 2 ? ring?.name ?? 'wherever both are' : undefined },
          { label: 'Cut and fill' },
        ]}
        at={asking.at}
      />}

      {mode === 'merge' && (
        <ToolSection title="Settings">
          <Field label="Keep" hint={keep === 'bottom' ? 'The lowest of them at each place: a pit shell or strip solids dug out of the topography, a solid by its floor' : 'The highest: a dump laid on, a solid by its top'}>
            <Choice value={keep} options={['bottom', 'top'] as const} labels={{ bottom: 'Bottom', top: 'Top' }} onChange={setKeep} label="Keep the bottom or the top" />
          </Field>
          <Field label="As" hint={output === 'grid' ? 'A grid at the cell set: fast at any size' : 'Exact: every crest, toe and berm where they are, joined where they meet; it takes longer'}>
            <Choice value={output} options={['grid', 'surface'] as const} labels={{ grid: 'A grid', surface: 'An exact surface' }} onChange={setOutput} label="A grid or an exact surface" />
          </Field>
          {output === 'grid' && (
            <Field label="Cell" hint="Blank: 5 m, or, every one a grid on one lattice, that lattice node by node">
              <NumberOrBlank value={mergeCell} onChange={setMergeCell} label="Grid cell" unit="m" blank="Its own" />
            </Field>
          )}
          <Field label="Name">
            <TextInput value={mergeName} placeholder={keep === 'bottom' ? 'Merged bottom' : 'Merged top'} label="Name for what's merged" onChange={(value) => setMergeName(value)} />
          </Field>
          <Field label="Layer">
            <TextInput value={mergeLayer} label="Layer for what's merged" onChange={(value) => setMergeLayer(value)} />
          </Field>
        </ToolSection>
      )}

      {mode === 'compare' && <ToolSection title="Settings">
        <Field label="Column check" hint="A 2.5D column volume at this cell runs alongside the exact one; a difference beyond 0.1 % is said">
          <NumberInput value={cell} onChange={setCell} label="Column check cell" unit="m" range={{ min: 0, minIncluded: false }} />
        </Field>
        <Field label="Thickness grid" hint={thickness === 'on' ? 'The second less the first at each column, as a grid coloured by its value' : 'Off: the volumes only'}>
          <Choice value={thickness} options={['off', 'on'] as const} labels={{ off: 'Off', on: 'Make it' }} onChange={setThickness} label="Make a thickness grid" />
        </Field>
        {thickness === 'on' && (
          <Field label="Grid layer">
            <TextInput value={thicknessLayer} label="Layer for the thickness grid" onChange={(value) => setThicknessLayer(value)} />
          </Field>
        )}
      </ToolSection>}
    </ToolPanel>
  );
}
