import { Check } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ObjectSummary } from '@/bridge';
import { Input } from '@/design-system/ui/input';
import { type Asker, AskSteps, Choice, Field, NumberInput, NumberOrBlank, type RunReport, Select, type ToolAction, ToolPanel, ToolSection, useAsking } from '@/design-system/kit';
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
 *
 * While the Workflow panel is adding steps, the button adds the tool as a step instead (decision 0078).
 */

type Mode = 'compare' | 'merge';

const MODES: readonly Mode[] = ['compare', 'merge'];

const MERGES = new Set(['surface', 'grid', 'solid']);
const { labels: MODE_LABELS, lines: MODE_LINES } = choiceWords('surfaces', MODES);

const SURFACES = new Set(['surface', 'grid']);

export function SurfacesPanel() {
  const { summary, selection, runRecord, clearRefusal, toolAsk, toolMode, setSelection, addingTo, panelsShown } = useWorkspace();
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

  /** Compare and Merge ask in turn (decision 0092). The loops outlive a render, so they read the panel through `latest`. */
  const asking = useAsking();
  const latest = useRef({ surfaces, closedStrings, compareParams, runIt, toMerge, mergeParams });
  latest.current = { surfaces, closedStrings, compareParams, runIt, toMerge, mergeParams };
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
  const loop = mode === 'merge' ? mergeLoop : compareLoop;
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
  const action: ToolAction = mode === 'merge' ? {
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
      <ToolSection>
        <Field label="Tool">
          <Select value={mode} aria-label="Surfaces tool" onChange={(e) => setMode(e.target.value as Mode)}>
            {MODES.map((m) => (
              <option key={m} value={m}>
                {MODE_LABELS[m]}
              </option>
            ))}
          </Select>
        </Field>
      </ToolSection>

      {mode === 'merge' && (
        <AskSteps steps={[{ label: 'The surfaces, grids and solids to merge', value: toMerge.length > 0 ? `${toMerge.length} picked` : undefined }, { label: `The ${which}, merged` }]} at={asking.at} />
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
            <Input className="h-7 px-1.5 text-xs" value={mergeName} placeholder={keep === 'bottom' ? 'Merged bottom' : 'Merged top'} aria-label="Name for what's merged" onChange={(e) => setMergeName(e.target.value)} />
          </Field>
          <Field label="Layer">
            <Input className="h-7 px-1.5 text-xs" value={mergeLayer} aria-label="Layer for what's merged" onChange={(e) => setMergeLayer(e.target.value)} />
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
            <Input className="h-7 px-1.5 text-xs" value={thicknessLayer} aria-label="Layer for the thickness grid" onChange={(e) => setThicknessLayer(e.target.value)} />
          </Field>
        )}
      </ToolSection>}
    </ToolPanel>
  );
}
