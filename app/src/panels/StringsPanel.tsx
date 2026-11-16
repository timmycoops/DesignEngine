import { ArrowLeftRight, Check } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { Choice, Field, NumberInput, NumberOrBlank, type RunReport, Select, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { choiceWords } from '@/tools';
import { useWorkspace } from '@/workspace/context';

/**
 * Strings (decision 0076, workstream 7): the commands on a string, out of Properties and onto the kit. Each tool on the
 * Strings tab opens this panel set to it. A tool works on the string picked, the last one picked where several are;
 * Number across and Polygons from lines work on several. Vertex numbers are as Properties lists them, from 1. Grade,
 * RL, drape, split, smooth, filter, reverse and close write a new revision of the string; offset and polygons make new
 * objects; join takes the other string into this one. One undo each.
 */

type Mode = 'offset' | 'number_across' | 'polygons_from_lines' | 'set_grade' | 'set_elevation' | 'drape' | 'split' | 'join' | 'smooth' | 'filter' | 'reverse' | 'set_closed';

/** In the order the Strings tab has them. */
const MODES: readonly Mode[] = ['offset', 'number_across', 'polygons_from_lines', 'set_grade', 'set_elevation', 'drape', 'split', 'join', 'smooth', 'filter', 'reverse', 'set_closed'];
/** Each tool's name, and the line it puts at the panel's head, from the one tool list. */
const { labels: MODE_LABELS, lines: MODE_LINES } = choiceWords('strings', MODES);

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

export function StringsPanel() {
  const { summary, selected, selection, runRecord, clearRefusal, setSelection, pickLine, toolAsk, toolMode } = useWorkspace();
  const [mode, setMode] = useState<Mode>('offset');
  // A tool on the Strings tab, or found by Find a command, asks for one of what this does.
  useEffect(() => {
    if (toolAsk?.panel === 'strings' && MODES.includes(toolAsk.mode as Mode)) setMode(toolAsk.mode as Mode);
  }, [toolAsk]);
  // The ribbon lights the tool the panel is set to.
  useEffect(() => {
    toolMode('strings', mode);
    return () => toolMode('strings', null);
  }, [toolMode, mode]);

  const [distance, setDistance] = useState<number | null>(null);
  const [times, setTimes] = useState(1);
  const [side, setSide] = useState<'left' | 'right'>('right');
  const [corners, setCorners] = useState<'mitre' | 'round'>('mitre');
  const [numberName, setNumberName] = useState('Strip #');
  const [start, setStart] = useState<number | null>(null);
  const [step, setStep] = useState(1);
  const [polygonName, setPolygonName] = useState('B#');
  const [polygonLayer, setPolygonLayer] = useState('block polygons');
  const [closeWithin, setCloseWithin] = useState(1);
  const [grade, setGrade] = useState<number | null>(null);
  const [from, setFrom] = useState(1);
  const [to, setTo] = useState<number | null>(null);
  const [startRl, setStartRl] = useState<number | null>(null);
  const [level, setLevel] = useState<number | null>(null);
  const [surface, setSurface] = useState<string | null>(null);
  const [crossings, setCrossings] = useState(true);
  const [splitAt, setSplitAt] = useState<number | null>(null);
  const [joinWith, setJoinWith] = useState<string | null>(null);
  const [spacing, setSpacing] = useState(5);
  const [tolerance, setTolerance] = useState<number | null>(null);
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  const strings = objects.filter((o) => o.kind === 'string');
  /** The string a tool works on: the last picked, if it's a string. */
  const string = strings.find((o) => o.id === selected) ?? null;
  const pickedIds = new Set(selection);
  const picked = strings.filter((o) => pickedIds.has(o.id));
  /** What Number across numbers: the strings picked, where several are; else what's on the string's layer that a face run didn't make. */
  const onItsLayer = string ? strings.filter((o) => !o.hidden && o.layer === string.layer && o.attributes.face_design === undefined) : [];
  const toNumber = picked.length > 1 ? picked : onItsLayer;
  const surfaces = objects.filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid'));
  const target = surfaces.find((s) => s.id === surface) ?? surfaces[0] ?? null;
  const others = string ? strings.filter((o) => o.id !== string.id && !o.closed) : [];
  const partner = others.find((o) => o.id === joinWith) ?? others[0] ?? null;

  const runIt = async (command: string, params: Record<string, unknown>) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, params);
    setBusy(false);
    setReport(outcome.record ? { notes: outcome.record.notes } : { notes: [], error: outcome.error });
    return outcome;
  };
  const numberAcross = async () => {
    const line = await pickLine('Click where the numbering starts. Esc cancels', 'Click across the strings to where it ends. Esc cancels');
    if (line) await runIt('string.number_across', { strings: toNumber.map((o) => o.id), from: line[0], to: line[1], start, step, name: numberName });
  };
  const makePolygons = async () => {
    const outcome = await runIt('string.polygons_from_lines', { strings: picked.map((o) => o.id), layer: polygonLayer.trim(), name: polygonName, tolerance: closeWithin / 1000 });
    const made = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (made.length > 0) setSelection(made);
  };

  const gradeParams = () => {
    const params: Record<string, unknown> = { string: string?.id, from: Math.round(from) - 1, grade };
    if (to !== null) params.to = Math.round(to) - 1;
    if (startRl !== null) params.rl = startRl;
    return params;
  };

  /** The one button: what it says, what it runs, and whether it has what it needs. */
  const does = ((): { label: string; run: () => unknown; ready: boolean; title?: string } => {
    const id = string?.id;
    const on = string?.name ?? 'the string';
    switch (mode) {
      case 'offset':
        return { label: `Offset ${on}${times > 1 ? ` ${times} times` : ''}`, run: () => runIt('string.offset', { string: id, distance, side, corners, times }), ready: !!string && distance !== null && distance > 0 && Number.isInteger(times) };
      case 'number_across':
        return { label: `Draw across ${plural(toNumber.length, 'string')}`, run: numberAcross, ready: toNumber.length > 0 && start !== null && step !== 0 && !!numberName.trim(), title: 'Two clicks in the view, across the strings: each string the line crosses takes the next number' };
      case 'polygons_from_lines':
        return { label: `Make polygons from ${plural(picked.length, 'line')}`, run: makePolygons, ready: picked.length > 0 && !!polygonName.trim() && !!polygonLayer.trim() };
      case 'set_grade':
        return { label: `Grade ${on}`, run: () => runIt('string.set_grade', gradeParams()), ready: !!string && grade !== null && Number.isInteger(from) && (to === null || Number.isInteger(to)) };
      case 'set_elevation':
        return { label: `Set ${on} to RL ${level ?? '…'}`, run: () => runIt('string.set_elevation', { string: id, rl: level }), ready: !!string && level !== null };
      case 'drape':
        return { label: `Drape ${on}`, run: () => runIt('string.drape', { string: id, surface: target?.id, crossings }), ready: !!string && !!target };
      case 'split':
        return { label: `Split ${on}`, run: () => runIt('string.split', { string: id, at: Math.round(splitAt ?? 1) - 1 }), ready: !!string && splitAt !== null && Number.isInteger(splitAt) };
      case 'join':
        return { label: `Join ${partner?.name ?? 'a string'} onto ${on}`, run: () => runIt('string.join', { string: id, other: partner?.id }), ready: !!string && !string.closed && !!partner };
      case 'smooth':
        return { label: `Smooth ${on}`, run: () => runIt('string.smooth', { string: id, spacing }), ready: !!string };
      case 'filter':
        return { label: `Filter ${on}`, run: () => runIt('string.filter', { string: id, tolerance }), ready: !!string && tolerance !== null };
      case 'reverse':
        return { label: `Reverse ${on}`, run: () => runIt('string.reverse', { string: id }), ready: !!string };
      case 'set_closed':
        return { label: `${string?.closed ? 'Open' : 'Close'} ${on}`, run: () => runIt('string.set_closed', { string: id, closed: !string?.closed }), ready: !!string };
    }
  })();
  const action: ToolAction = {
    label: does.label,
    icon: mode === 'reverse' ? <ArrowLeftRight className="size-3.5" /> : <Check className="size-3.5" />,
    disabled: !does.ready,
    busy,
    title: does.title,
    run: does.run,
  };

  const several = mode === 'number_across' || mode === 'polygons_from_lines';
  return (
    <ToolPanel title="Strings" panel="strings" summary={MODE_LINES[mode]} result={report} action={action}>
      <ToolSection>
        <Field label="Tool">
          <Select value={mode} aria-label="String tool" onChange={(e) => setMode(e.target.value as Mode)}>
            {MODES.map((m) => (
              <option key={m} value={m}>
                {MODE_LABELS[m]}
              </option>
            ))}
          </Select>
        </Field>
      </ToolSection>

      <ToolSection title={several ? 'Picked' : 'String'}>
        {mode === 'number_across' ? (
          <Field label="Number" hint={picked.length > 1 ? 'The strings picked' : string ? "What's on its layer that a face run didn't make. Pick several strings to number those alone" : 'Pick a string in the view or the tree: its layer is numbered. Or pick several'}>
            <span data-slot="strings-on">{plural(toNumber.length, 'string')}</span>
          </Field>
        ) : mode === 'polygons_from_lines' ? (
          <Field label="Lines" hint="The lines across the strips and the lines closing them: pick them all, by a box or from the tree">
            <span data-slot="strings-on">{plural(picked.length, 'line')} picked</span>
          </Field>
        ) : (
          <Field label="On" hint={string ? (picked.length > 1 ? `The last of the ${picked.length} picked` : undefined) : 'Pick a string in the view or the tree'}>
            <span data-slot="strings-on" className={string ? undefined : 'text-muted-foreground'}>
              {string ? `${string.name}${string.closed ? ' · closed' : ''}` : 'No string picked'}
            </span>
          </Field>
        )}
      </ToolSection>

      {mode === 'offset' && (
        <ToolSection title="Settings">
          <Field label="Distance" hint="Horizontal, square to the string">
            <NumberOrBlank value={distance} onChange={setDistance} label="Offset distance" unit="m" range={{ min: 0, minIncluded: false }} blank="" />
          </Field>
          <Field label="Times" hint="How many strings, each the distance further out">
            <NumberInput value={times} onChange={setTimes} label="How many offsets" range={{ min: 1 }} />
          </Field>
          <Field label="Side" hint="Looking along the string">
            <Choice value={side} options={['left', 'right'] as const} labels={{ left: 'Left', right: 'Right' }} onChange={setSide} label="The side to offset to, looking along the string" />
          </Field>
          <Field label="Corners">
            <Choice value={corners} options={['mitre', 'round'] as const} labels={{ mitre: 'Mitre', round: 'Round' }} onChange={setCorners} label="How outside corners are made" />
          </Field>
        </ToolSection>
      )}

      {mode === 'number_across' && (
        <ToolSection title="Settings">
          <Field label="Name" hint="Each string's name, with its number in place of #">
            <Input className="h-7 w-40 px-1.5 text-xs" value={numberName} aria-label="Name of each string, # for its number" onChange={(e) => setNumberName(e.target.value)} />
          </Field>
          <Field label="From">
            <NumberOrBlank value={start} onChange={setStart} label="The first number" blank="" />
          </Field>
          <Field label="Step" hint="Added each time; negative to count down">
            <NumberInput value={step} onChange={setStep} label="Added for each string" />
          </Field>
        </ToolSection>
      )}

      {mode === 'polygons_from_lines' && (
        <ToolSection title="Settings">
          <Field label="Name" hint="Each polygon's name, numbered west to east in place of #">
            <Input className="h-7 w-40 px-1.5 text-xs" value={polygonName} aria-label="Name of each polygon, # for its number" onChange={(e) => setPolygonName(e.target.value)} />
          </Field>
          <Field label="To layer">
            <Input className="h-7 w-40 px-1.5 text-xs" value={polygonLayer} aria-label="Layer for the polygons" onChange={(e) => setPolygonLayer(e.target.value)} />
          </Field>
          <Field label="Join within" hint="An end this close to another line is joined to it, and said">
            <NumberInput value={closeWithin} onChange={setCloseWithin} label="How close an end is joined to a line" unit="mm" range={{ min: 0 }} />
          </Field>
        </ToolSection>
      )}

      {mode === 'set_grade' && (
        <ToolSection title="Settings">
          <Field label="Grade" hint="Positive uphill along the string">
            <NumberOrBlank value={grade} onChange={setGrade} label="Grade" unit="%" blank="" />
          </Field>
          <Field label="From vertex">
            <NumberInput value={from} onChange={setFrom} label="Start vertex" range={{ min: 1 }} />
          </Field>
          <Field label="To vertex">
            <NumberOrBlank value={to} onChange={setTo} label="End vertex" range={{ min: 1 }} blank="end" />
          </Field>
          <Field label="At RL" hint="The RL at the start vertex">
            <NumberOrBlank value={startRl} onChange={setStartRl} label="RL at the start vertex" blank="its own" />
          </Field>
        </ToolSection>
      )}

      {mode === 'set_elevation' && (
        <ToolSection title="Settings">
          <Field label="RL" hint="Every vertex to this RL">
            <NumberOrBlank value={level} onChange={setLevel} label="RL for every vertex" blank="" />
          </Field>
        </ToolSection>
      )}

      {mode === 'drape' && (
        <ToolSection title="Settings">
          <Field label="Onto">
            {surfaces.length === 0 ? (
              <span className="text-muted-foreground">No grid or surface on show</span>
            ) : (
              <Select value={target?.id ?? ''} aria-label="Grid or surface to drape onto" onChange={(e) => setSurface(e.target.value)}>
                {surfaces.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Follow" hint="Adds vertices where the string crosses the surface's triangle edges">
            <Checkbox checked={crossings} aria-label="Follow the surface between vertices" onCheckedChange={(c) => setCrossings(c === true)} />
          </Field>
        </ToolSection>
      )}

      {mode === 'split' && (
        <ToolSection title="Settings">
          <Field label="At vertex" hint="Split here; a closed string opens here">
            <NumberOrBlank value={splitAt} onChange={setSplitAt} label="Vertex to split at" range={{ min: 1 }} blank="" />
          </Field>
        </ToolSection>
      )}

      {mode === 'join' && (
        <ToolSection title="Settings">
          <Field label="Join on" hint="Joined at the nearest ends; that string is deleted, and undo brings it back">
            {string?.closed ? (
              <span className="text-muted-foreground">Open the string first</span>
            ) : others.length === 0 ? (
              <span className="text-muted-foreground">No other open string</span>
            ) : (
              <Select value={partner?.id ?? ''} aria-label="String to join on" onChange={(e) => setJoinWith(e.target.value)}>
                {others.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </ToolSection>
      )}

      {mode === 'smooth' && (
        <ToolSection title="Settings">
          <Field label="Every" hint="The longest step along the curve, in plan">
            <NumberInput value={spacing} onChange={setSpacing} label="Longest step along the curve" unit="m" range={{ min: 0, minIncluded: false }} />
          </Field>
        </ToolSection>
      )}

      {mode === 'filter' && (
        <ToolSection title="Settings">
          <Field label="Within" hint="Vertices within this 3D distance of the line are taken out">
            <NumberOrBlank value={tolerance} onChange={setTolerance} label="Distance from the line" unit="m" range={{ min: 0 }} blank="" />
          </Field>
        </ToolSection>
      )}
    </ToolPanel>
  );
}
