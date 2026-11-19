import { concealed } from '@/bridge';
import { ArrowLeftRight, Check, MousePointerClick } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { StepTarget } from '@/bridge';
import { Button } from '@/design-system/ui/button';
import { Checkbox } from '@/design-system/ui/checkbox';
import { Input } from '@/design-system/ui/input';
import { Choice, Field, NumberInput, NumberOrBlank, resultParams, type RunReport, Select, TargetInput, type ToolAction, ToolPanel, ToolSection, useResultAs } from '@/design-system/kit';
import { choiceWords } from '@/tools';
import { nearestString, sideOf } from './stringSide';
import { useWorkspace } from '@/workspace/context';

/**
 * Strings (decision 0076, workstream 7): the commands on a string, out of Properties and onto the kit. Each tool on the
 * Strings tab opens this panel set to it. A tool works on every string picked, as one run and one undo (decision 0078):
 * offset, grade, RL, drape, smooth, filter, reverse and close. Split works on one string, the last picked. Join joins
 * the open strings picked end to end into the one picked last, which keeps its name and the way it runs; the others
 * are deleted. Number across, Polygons from lines, Project and Surface between strings work on several as they did.
 * Vertex numbers are as Properties lists them, from 1, and are for one string. Grade, RL, drape, split, smooth, filter,
 * reverse and close write a new revision of each string; offset, polygons and project make new objects. Ramp from a start places a ramp's polygon by two clicks in the view, along the string picked last or
 * straight, and Move it places the last one again.
 *
 * Trim and Extend (String tools plan, workstream 1) work on the strings picked, then a click on the cutting string, or
 * the string to run on to, and for Trim a click on the side to keep, which holds for every string. Split splits at a
 * click on the string, or at a vertex by number. Join joins ends within a tolerance as one vertex.
 *
 * Polygons (workstream 6): Clip keeps the parts of the strings picked inside a closed string clicked, or outside it.
 * Union, overlap or difference makes new closed strings of the closed strings picked: their union, or, with a closed
 * string clicked, their overlap with it or what's left of them less it.
 *
 * Vertices (workstream 4): with vertices picked in the view by Alt and a box, Set grade, Set elevation and Drape act on
 * them alone, the button saying how many, and add no workflow step. Densify adds a vertex every so many metres, the
 * line unchanged; Vertices at crossings adds one on each string picked wherever another of them crosses it.
 *
 * While the Workflow panel is adding steps, the button reads Add to the workflow and adds the tool and its settings as a
 * step instead of running it (decision 0078, workstream 4): the strings picked are found by rule each time it runs, the
 * strings of a layer by their layer. Number across asks for its line across first. Ramp from a start adds no step.
 */

type Mode =
  | 'offset'
  | 'number_across'
  | 'polygons_from_lines'
  | 'set_grade'
  | 'set_elevation'
  | 'drape'
  | 'project'
  | 'split'
  | 'trim'
  | 'extend'
  | 'clip'
  | 'polygon_boolean'
  | 'join'
  | 'fillet'
  | 'smooth'
  | 'filter'
  | 'densify'
  | 'vertices_at_crossings'
  | 'reverse'
  | 'set_closed'
  | 'surface_between'
  | 'ramp_polygon';

/** In the order the Strings tab has them. */
const MODES: readonly Mode[] = ['offset', 'number_across', 'polygons_from_lines', 'set_grade', 'set_elevation', 'drape', 'project', 'split', 'trim', 'extend', 'clip', 'polygon_boolean', 'join', 'fillet', 'smooth', 'filter', 'densify', 'vertices_at_crossings', 'reverse', 'set_closed', 'surface_between', 'ramp_polygon'];

/**
 * The tools that change the strings themselves, so offer in place or as new on a layer (decision 0091). The rest make
 * something new anyway, or set a number on them.
 */
const CHANGES: readonly Mode[] = ['set_grade', 'set_elevation', 'drape', 'split', 'trim', 'extend', 'clip', 'join', 'fillet', 'smooth', 'filter', 'densify', 'vertices_at_crossings', 'reverse', 'set_closed'];

/** The tools that act on the vertices picked alone, when some are (String tools plan, workstream 4). */
const ON_VERTICES: readonly Mode[] = ['set_grade', 'set_elevation', 'drape'];

/** How far from a string a click on it may land and still be it, in plan (m). Clicks snap, so on it they're on it. */
const CLICK_ON_A_STRING = 2;
/** Each tool's name, and the line it puts at the panel's head, from the one tool list. */
const { labels: MODE_LABELS, lines: MODE_LINES } = choiceWords('strings', MODES);

/** The tools that work on one string, the last picked: they work at a vertex. */
const ONE_STRING: readonly Mode[] = ['split'];

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
/** The first three names and how many more, for under a count. */
const firstNames = (names: string[]) => (names.length > 3 ? `${names.slice(0, 3).join(', ')} and ${names.length - 3} more` : names.join(', '));

export function StringsPanel() {
  const { summary, selected, selection, runRecord, clearRefusal, setSelection, pickLine, pickPoint, toolAsk, toolMode, vertexPicks } = useWorkspace();
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
  /** Where Project goes to: an RL, a grid or a surface, or null for a height above or below each string. */
  const [projectTo, setProjectTo] = useState<StepTarget | null>(null);
  const [projectHeight, setProjectHeight] = useState(10);
  const [batter, setBatter] = useState(65);
  const [direction, setDirection] = useState<'up' | 'down'>('up');
  const [keepWall, setKeepWall] = useState(false);
  const [surfaceName, setSurfaceName] = useState('');
  /** Ramp from a start: what it follows, its road, the RL it starts at (blank: where it's clicked) and what it runs to. */
  const [rampFollows, setRampFollows] = useState<'line' | 'direction'>('line');
  const [rampWidth, setRampWidth] = useState(50);
  const [rampGrade, setRampGrade] = useState(10);
  const [rampStartRl, setRampStartRl] = useState<number | null>(null);
  const [rampTo, setRampTo] = useState<StepTarget | null>(null);
  /** The ramp placed last, and the line it runs along, for Move it. */
  const [lastRamp, setLastRamp] = useState<{ id: string; line: string | null } | null>(null);
  const [surfaceLayer, setSurfaceLayer] = useState('');
  /** What the last Project made, until the project's summary has it. */
  const [made, setMade] = useState<string[]>([]);
  const [splitAt, setSplitAt] = useState<number | null>(null);
  const [splitBy, setSplitBy] = useState<'click' | 'vertex'>('click');
  /** Trim keeps the side clicked, or both sides, split. */
  const [keep, setKeep] = useState<'side' | 'both'>('side');
  const [reach, setReach] = useState(100);
  /** Clip keeps the parts inside the closed string clicked, or outside. */
  const [clipKeep, setClipKeep] = useState<'inside' | 'outside'>('inside');
  /** Union, overlap or difference: which, and what the strings made are called and where they go. */
  const [operation, setOperation] = useState<'union' | 'overlap' | 'difference'>('union');
  const [opName, setOpName] = useState('');
  const [opLayer, setOpLayer] = useState('polygons');
  /** Join: ends this close share a vertex (mm). */
  const [joinWithin, setJoinWithin] = useState(10);
  const [spacing, setSpacing] = useState(5);
  const [closeOrOpen, setCloseOrOpen] = useState<'close' | 'open'>('close');
  const [tolerance, setTolerance] = useState<number | null>(null);
  // Round corners (String tools plan, workstream 2): the radius, the corners it takes, and the arcs' vertex spacing.
  const [radius, setRadius] = useState<number | null>(null);
  const [cornerAngle, setCornerAngle] = useState(10);
  const [arcSpacing, setArcSpacing] = useState(2);
  /** Densify: the most a vertex is from the next, in plan. */
  const [denseSpacing, setDenseSpacing] = useState(2);
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);
  /** In place or as new on a layer, for the tools that change strings (decision 0091). */
  const resultAs = useResultAs();
  const changes = CHANGES.includes(mode);

  const objects = summary?.objects ?? [];
  const strings = objects.filter((o) => o.kind === 'string');
  /** The string a one-string tool works on, and whose vertices a setting names: the last picked, if it's a string. */
  const string = strings.find((o) => o.id === selected) ?? null;
  const pickedIds = new Set(selection);
  const picked = strings.filter((o) => pickedIds.has(o.id));
  /** What the tools that take several work on: every string picked, as one run (decision 0078). */
  const on = picked.length > 0 ? picked : string ? [string] : [];
  const ids = on.map((o) => o.id);
  const many = on.length > 1;
  const what = on.length === 1 ? on[0].name : on.length > 1 ? `${on.length} strings` : 'the string';
  /** The vertices picked in the view (Alt and a box), on strings still here: Set grade, Set elevation and Drape take them alone. */
  const picks = Object.entries(vertexPicks).filter(([id, v]) => v.length > 0 && strings.some((o) => o.id === id));
  const vertexCount = picks.reduce((n, [, v]) => n + v.length, 0);
  const onVertices = ON_VERTICES.includes(mode) && vertexCount > 0;
  const vertexWhat = vertexCount === 1 ? '1 vertex' : `${vertexCount} vertices`;
  /** A command's strings: the vertices picked and their strings, or every string worked on. */
  const worksOn = () => (onVertices ? { strings: picks.map(([id]) => id), vertices: picks.map(([, v]) => v) } : { strings: ids });
  /** Close closes the open ones or opens the closed ones; where the strings picked differ, the panel asks which. */
  const mixed = on.some((o) => o.closed) && on.some((o) => !o.closed);
  const closing = mixed ? closeOrOpen === 'close' : !on[0]?.closed;
  /** What Number across numbers: the strings picked, where several are; else what's on the string's layer that a face run didn't make. */
  const onItsLayer = string ? strings.filter((o) => !concealed(o) && o.layer === string.layer && o.attributes.face_design === undefined) : [];
  const toNumber = picked.length > 1 ? picked : onItsLayer;
  const surfaces = objects.filter((o) => o.drawn && (o.kind === 'surface' || o.kind === 'grid'));
  const target = surfaces.find((s) => s.id === surface) ?? surfaces[0] ?? null;
  // The strings Project made are picked once they're in, ready for the next step from them: a berm's offset, or the
  // next project. Its walls aren't.
  useEffect(() => {
    if (made.length === 0) return;
    const ids = new Set(made);
    const lines = (summary?.objects ?? []).filter((o) => ids.has(o.id) && o.kind === 'string').map((o) => o.id);
    if (lines.length === 0) return;
    setMade([]);
    setSelection(lines);
  }, [made, summary, setSelection]);
  /** What Project projects: every string picked. */
  const toProject = picked.length > 0 ? picked : string ? [string] : [];
  /** What Join joins end to end: the strings picked, the one picked last first, since it keeps the result. */
  const toJoin = string && on.some((o) => o.id === string.id) ? [string, ...on.filter((o) => o.id !== string.id)] : on;
  const closedToJoin = toJoin.filter((o) => o.closed).length;
  const joinParams = () => ({ strings: toJoin.map((o) => o.id), tolerance: joinWithin / 1000 });
  const joinPieces = async () => {
    const outcome = await runIt('string.join', joinParams());
    if (outcome.record && !resultAs.asNew) setSelection([toJoin[0].id]);
  };

  const runIt = async (command: string, params: Record<string, unknown>) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, { ...params, ...(changes ? resultParams(resultAs) : {}) });
    setBusy(false);
    setReport(outcome.record ? { notes: outcome.record.notes } : { notes: [], error: outcome.error });
    // As new, what's made is picked, ready for the next step.
    const made = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (changes && resultAs.asNew && made.length > 0) setSelection(made);
    return outcome;
  };
  const numberParams = () => ({ strings: toNumber.map((o) => o.id), start, step, name: numberName });
  /** Number across's settings with the line drawn across the strings, or null when Esc is pressed. */
  const numberLine = async () => {
    const line = await pickLine('Click where the numbering starts. Esc cancels', 'Click across the strings to where it ends. Esc cancels');
    return line ? { ...numberParams(), from: line[0], to: line[1] } : null;
  };
  const numberAcross = async () => {
    const params = await numberLine();
    if (params) await runIt('string.number_across', params);
  };
  const polygonParams = () => ({ strings: picked.map((o) => o.id), layer: polygonLayer.trim(), name: polygonName, tolerance: closeWithin / 1000 });
  const makePolygons = async () => {
    const outcome = await runIt('string.polygons_from_lines', polygonParams());
    const made = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (made.length > 0) setSelection(made);
  };

  const projectParams = () => ({ strings: toProject.map((o) => o.id), to: projectTo ?? { kind: 'height', height: projectHeight }, batter, side, direction, wall: keepWall });
  const project = async () => {
    const outcome = await runIt('string.project', projectParams());
    setMade(outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? []);
  };
  /** A click in the view on the side the new string goes, read against the string picked last. */
  const clickSide = async () => {
    const line = string ?? toProject.at(-1) ?? null;
    const origin = summary?.origin;
    if (!line?.vertices_local || !origin) return;
    const point = await pickPoint(`Click on the side of ${line.name} the new string goes. Esc cancels`);
    if (!point) return;
    const found = sideOf(
      line.vertices_local.map((v) => [v[0] + origin[0], v[1] + origin[1], v[2] + origin[2]] as const),
      !!line.closed,
      point,
    );
    if (found) setSide(found);
  };
  /** Two clicks in the view place a ramp: where it starts and the way it runs. With `moved`, that ramp is placed again. */
  const placeRamp = async (moved?: { id: string; line: string | null }) => {
    const along = moved ? moved.line : rampFollows === 'line' ? (string?.id ?? null) : null;
    const clicks = await pickLine(along ? 'Click on the line where the ramp starts. Esc cancels' : 'Click where the ramp starts. Esc cancels', 'Click the way the ramp runs. Esc cancels');
    if (!clicks) return;
    const start = rampStartRl === null ? clicks[0] : [clicks[0][0], clicks[0][1], rampStartRl];
    const outcome = await runIt('string.ramp_polygon', { start, towards: clicks[1], ...(along ? { line: along } : {}), width: rampWidth, grade: rampGrade, side, to: rampTo, ...(moved ? { replaces: moved.id } : {}) });
    const placed = outcome.record?.outputs.find((o) => o.change === 'created' || o.change === 'revised');
    if (placed) setLastRamp({ id: placed.id, line: along });
  };
  const rampReady = rampWidth > 0 && rampGrade > 0 && rampTo !== null && (rampTo.kind === 'rl' || surfaces.some((s) => s.id === rampTo.id));
  const placedRamp = lastRamp ? (objects.find((o) => o.id === lastRamp.id && o.kind === 'string') ?? null) : null;
  const surfaceParams = () => ({ strings: toProject.map((o) => o.id), ...(surfaceName.trim() ? { name: surfaceName.trim() } : {}), ...(surfaceLayer.trim() ? { layer: surfaceLayer.trim() } : {}) });
  const makeSurface = async () => {
    const outcome = await runIt('surface.between_strings', surfaceParams());
    const surface = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (surface.length > 0) setSelection(surface);
  };
  const projectWords = () => {
    const where = projectTo === null ? `${direction} ${projectHeight} m` : projectTo.kind === 'rl' ? `${direction} to RL ${projectTo.rl}` : `${direction} to ${surfaces.find((s) => s.id === projectTo.id)?.name ?? 'a surface'}`;
    return `Project ${toProject.length === 1 ? toProject[0].name : plural(toProject.length, 'string')} ${where}`;
  };

  /** A click on a string in the view other than those worked on: the cutting string, or the one to run on to. */
  const clickString = async (prompt: string) => {
    const point = await pickPoint(prompt);
    const origin = summary?.origin;
    if (!point || !origin) return null;
    const others = strings
      .filter((o) => !concealed(o) && o.vertices_local && !ids.includes(o.id))
      .map((o) => ({ id: o.id, name: o.name, closed: !!o.closed, vertices: (o.vertices_local ?? []).map((v) => [v[0] + origin[0], v[1] + origin[1], v[2] + origin[2]] as const) }));
    const found = nearestString(others, point, CLICK_ON_A_STRING);
    if (!found) setReport({ notes: [], error: 'No string there: click on the string itself, not one of those picked' });
    return found;
  };
  /** Trim's settings with its clicks: the cutting string, then the side to keep. Null when Esc is pressed. */
  const trimParams = async () => {
    const cutter = await clickString(`Click the string that cuts ${what}. Esc cancels`);
    if (!cutter) return null;
    if (keep === 'both') return { strings: ids, by: cutter.id, keep: 'both' };
    const at = await pickPoint(`Click on the side of ${cutter.name} to keep. Esc cancels`);
    return at ? { strings: ids, by: cutter.id, keep: 'side', side: at } : null;
  };
  const trim = async () => {
    const params = await trimParams();
    if (!params) return;
    const outcome = await runIt('string.trim', params);
    const kept = outcome.record?.outputs.filter((o) => o.change === 'created' || o.change === 'revised').map((o) => o.id) ?? [];
    if (kept.length > 0) setSelection(kept);
  };
  /** Clip's settings with its click: the closed string to clip to. Null when Esc is pressed. */
  const clipParams = async () => {
    const limit = await clickString(`Click the closed string to clip ${what} to. Esc cancels`);
    if (!limit) return null;
    if (!limit.closed) {
      setReport({ notes: [], error: `${limit.name} isn't closed, so it has no inside to clip to: close it first, or click a closed string` });
      return null;
    }
    return { strings: ids, by: limit.id, keep: clipKeep };
  };
  const clip = async () => {
    const params = await clipParams();
    if (!params) return;
    const outcome = await runIt('string.clip', params);
    const kept = outcome.record?.outputs.filter((o) => o.change === 'created' || o.change === 'revised').map((o) => o.id) ?? [];
    if (kept.length > 0) setSelection(kept);
  };
  const opBase = () => ({ strings: ids, operation, layer: opLayer.trim() || 'polygons', ...(opName.trim() ? { name: opName.trim() } : {}) });
  /** Union, overlap or difference's settings: for an overlap or a difference, a click on the closed string to work against. */
  const opParams = async () => {
    if (operation === 'union') return opBase();
    const other = await clickString(`Click the closed string to ${operation === 'overlap' ? 'overlap' : 'take away from'} ${what}. Esc cancels`);
    if (!other) return null;
    if (!other.closed) {
      setReport({ notes: [], error: `${other.name} isn't closed, so it has no inside: click a closed string` });
      return null;
    }
    return { ...opBase(), with: [other.id] };
  };
  const polygonOp = async () => {
    const params = await opParams();
    if (!params) return;
    const outcome = await runIt('string.polygon_boolean', params);
    const made = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (made.length > 0) setSelection(made);
  };
  const openPicked = on.filter((o) => !o.closed).length;
  const extendParams = async () => {
    const to = await clickString(`Click the string to run ${what} on to. Esc cancels`);
    return to ? { strings: ids, to: to.id, reach } : null;
  };
  const extend = async () => {
    const params = await extendParams();
    if (params) await runIt('string.extend', params);
  };
  const splitAtClick = async () => {
    if (!string) return;
    const point = await pickPoint(`Click on ${string.name} where it splits. Esc cancels`);
    if (point) await runIt('string.split', { string: string.id, point });
  };

  /** Several strings are each graded from their first vertex to their last; From and To are for one. */
  const gradeParams = () => {
    const params: Record<string, unknown> = { ...worksOn(), grade };
    if (!many && !onVertices) params.from = Math.round(from) - 1;
    if (!many && !onVertices && to !== null) params.to = Math.round(to) - 1;
    if (startRl !== null) params.rl = startRl;
    return params;
  };

  /** A tool that runs its command as it stands: the run, and the step it adds to a workflow (decision 0078, workstream 4). */
  const calls = (command: string, params: Record<string, unknown>) => ({ run: () => runIt(command, params), step: { command, params } });
  /** On the vertices picked: run as it stands, no workflow step, since a workflow finds strings by rule, not vertices. */
  const onPicked = (command: string, params: Record<string, unknown>) => (onVertices ? { run: () => runIt(command, params) } : calls(command, params));

  /**
   * The one button: what it says, what it runs, whether it has what it needs, and the step it adds while the Workflow
   * panel is adding. Ramp from a start adds none: it's placed by hand.
   */
  const does = ((): { label: string; run: () => unknown; ready: boolean; title?: string; step?: ToolAction['step'] } => {
    const id = string?.id;
    const one = string?.name ?? 'the string';
    const some = on.length > 0;
    switch (mode) {
      case 'offset':
        return { label: `Offset ${what}${times > 1 ? ` ${times} times` : ''}`, ...calls('string.offset', { strings: ids, distance, side, corners, times }), ready: some && distance !== null && distance > 0 && Number.isInteger(times) };
      case 'number_across':
        return {
          label: `Draw across ${plural(toNumber.length, 'string')}`,
          run: numberAcross,
          step: { command: 'string.number_across', params: numberParams(), ask: numberLine },
          ready: toNumber.length > 0 && start !== null && step !== 0 && !!numberName.trim(),
          title: 'Two clicks in the view, across the strings: each string the line crosses takes the next number',
        };
      case 'polygons_from_lines':
        return { label: `Make polygons from ${plural(picked.length, 'line')}`, run: makePolygons, step: { command: 'string.polygons_from_lines', params: polygonParams() }, ready: picked.length > 0 && !!polygonName.trim() && !!polygonLayer.trim() };
      case 'set_grade':
        return {
          label: `Grade ${onVertices ? vertexWhat : what}`,
          ...onPicked('string.set_grade', gradeParams()),
          ready: (onVertices || some) && grade !== null && (onVertices || many || (Number.isInteger(from) && (to === null || Number.isInteger(to)))),
        };
      case 'set_elevation':
        return { label: `Set ${onVertices ? vertexWhat : what} to RL ${level ?? '…'}`, ...onPicked('string.set_elevation', { ...worksOn(), rl: level }), ready: (onVertices || some) && level !== null };
      case 'drape':
        return { label: `Drape ${onVertices ? vertexWhat : what}`, ...onPicked('string.drape', { ...worksOn(), surface: target?.id, ...(onVertices ? {} : { crossings }) }), ready: (onVertices || some) && !!target };
      case 'project':
        return {
          label: projectWords(),
          run: project,
          step: { command: 'string.project', params: projectParams() },
          ready: toProject.length > 0 && batter > 0 && batter < 90 && (projectTo === null ? projectHeight > 0 : projectTo.kind === 'rl' || surfaces.some((s) => s.id === projectTo.id)),
          title: 'Each string picked gets a new string where a wall from it at the batter comes out. The strings stay as they are',
        };
      case 'split':
        return splitBy === 'click'
          ? { label: `Split ${one} at a click`, run: splitAtClick, ready: !!string, title: 'A click on the string: a vertex goes in there at its height, or it splits at a vertex within a millimetre' }
          : { label: `Split ${one}`, ...calls('string.split', { string: id, at: Math.round(splitAt ?? 1) - 1 }), ready: !!string && splitAt !== null && Number.isInteger(splitAt) };
      case 'trim':
        return {
          label: keep === 'both' ? `Split ${what} at a string` : `Trim ${what}`,
          run: trim,
          step: { command: 'string.trim', params: { strings: ids, keep }, ask: trimParams },
          ready: some,
          title: keep === 'both' ? 'A click on the string that cuts them: each is split where it crosses' : 'A click on the string that cuts them, then one on the side to keep, for every string picked',
        };
      case 'extend':
        return {
          label: `Run ${what} on`,
          run: extend,
          step: { command: 'string.extend', params: { strings: ids, reach }, ask: extendParams },
          ready: some && reach > 0,
          title: 'A click on the string to run them on to: each runs on from its nearer end, at its own grade',
        };
      case 'clip':
        return {
          label: `Clip ${what}`,
          run: clip,
          step: { command: 'string.clip', params: { strings: ids, keep: clipKeep }, ask: clipParams },
          ready: some,
          title: `A click on the closed string to clip to: each string keeps its parts ${clipKeep} it, cut at its own heights`,
        };
      case 'polygon_boolean':
        return {
          label: operation === 'union' ? `Union of ${what}` : operation === 'overlap' ? `Overlap ${what} with a polygon` : `Take a polygon from ${what}`,
          run: polygonOp,
          step: { command: 'string.polygon_boolean', params: opBase(), ...(operation === 'union' ? {} : { ask: opParams }) },
          ready: some && openPicked === 0,
          title:
            operation === 'union'
              ? 'One set of closed strings in plan, everywhere inside any of them. The strings picked stay as they are'
              : 'A click on the closed string to work against. The strings picked, and it, stay as they are',
        };
      case 'join':
        return {
          label: toJoin.length > 1 ? `Join ${toJoin.length} strings into one` : 'Join the strings picked',
          run: joinPieces,
          step: { command: 'string.join', params: joinParams() },
          ready: toJoin.length > 1 && closedToJoin === 0,
          title: 'The nearest ends are joined first, then the next nearest. Ends that meet share a vertex; a gap is bridged by a segment and said with its size',
        };
      case 'smooth':
        return { label: `Smooth ${what}`, ...calls('string.smooth', { strings: ids, spacing }), ready: some };
      case 'fillet':
        return { label: `Round the corners of ${what}`, ...calls('string.fillet', { strings: ids, radius, angle: cornerAngle, spacing: arcSpacing }), ready: some && radius !== null };
      case 'filter':
        return { label: `Filter ${what}`, ...calls('string.filter', { strings: ids, tolerance }), ready: some && tolerance !== null };
      case 'densify':
        return { label: `Add vertices to ${what}`, ...calls('string.densify', { strings: ids, spacing: denseSpacing }), ready: some && denseSpacing > 0, title: 'Each segment split into equal parts no longer than the spacing; the line stays exactly where it was' };
      case 'vertices_at_crossings':
        return {
          label: `Add vertices where ${what} cross`,
          ...calls('string.vertices_at_crossings', { strings: ids }),
          ready: on.length > 1,
          title: 'A vertex on each string picked wherever another of them crosses it, at its own RL there',
        };
      case 'reverse':
        return { label: `Reverse ${what}`, ...calls('string.reverse', { strings: ids }), ready: some };
      case 'set_closed':
        return { label: `${closing ? 'Close' : 'Open'} ${what}`, ...calls('string.set_closed', { strings: ids, closed: closing }), ready: some };
      case 'ramp_polygon':
        return {
          label: rampFollows === 'line' ? `Place a ramp along ${string?.name ?? 'a line'}` : 'Place a ramp from a start',
          run: () => placeRamp(),
          ready: rampReady && (rampFollows === 'direction' || !!string),
          title: 'Two clicks in the view: where the ramp starts, and the way it runs',
        };
      case 'surface_between':
        return {
          label: `Make a surface between ${plural(toProject.length, 'string')}`,
          run: makeSurface,
          step: { command: 'surface.between_strings', params: surfaceParams() },
          ready: toProject.length >= 2,
          title: 'One surface across the strings picked, in order across whatever order they were picked in. The strings stay as they are',
        };
    }
  })();
  const action: ToolAction = {
    label: does.label,
    icon: mode === 'reverse' ? <ArrowLeftRight className="size-3.5" /> : <Check className="size-3.5" />,
    disabled: !does.ready,
    busy,
    title: does.title,
    run: does.run,
    step: does.step,
    result: changes ? resultAs : undefined,
  };

  const several = mode === 'number_across' || mode === 'polygons_from_lines' || mode === 'project' || mode === 'surface_between' || mode === 'join';
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
          <Field label="Number" hint={picked.length > 1 ? 'The strings picked' : string ? "What's on its layer that a face run didn't make. Pick several strings to number those alone" : 'Pick a string in the view or the Objects table: its layer is numbered. Or pick several'}>
            <span data-slot="strings-on">{plural(toNumber.length, 'string')}</span>
          </Field>
        ) : mode === 'polygons_from_lines' ? (
          <Field label="Lines" hint="The lines across the strips and the lines closing them: pick them all, by a box, Ctrl-click or the Objects table">
            <span data-slot="strings-on">{plural(picked.length, 'line')} picked</span>
          </Field>
        ) : mode === 'ramp_polygon' ? (
          <>
            <Field label="Follows" hint="Its inside edge runs along the line picked, from where you click on it, or straight from the start the way you click">
              <Choice value={rampFollows} options={['line', 'direction'] as const} labels={{ line: 'A line', direction: 'A direction' }} onChange={setRampFollows} label="What the ramp follows" />
            </Field>
            {rampFollows === 'line' && (
              <Field label="Along" hint={string ? undefined : 'Pick the line in the view or the Objects table: a toe or a crest'}>
                <span data-slot="strings-on" className={string ? undefined : 'text-muted-foreground'}>
                  {string ? string.name : 'No line picked'}
                </span>
              </Field>
            )}
          </>
        ) : mode === 'surface_between' ? (
          <Field label="Between" hint={toProject.length >= 2 ? 'In order across, whatever order they were picked in' : "Pick two strings or more: a road's two edges, with the crest and toe of its batters if it has them"}>
            <span data-slot="strings-on" className={toProject.length >= 2 ? undefined : 'text-muted-foreground'}>
              {toProject.length >= 2 ? plural(toProject.length, 'string') : toProject.length === 1 ? `${toProject[0].name}, and one more to pick` : 'No string picked'}
            </span>
          </Field>
        ) : mode === 'project' ? (
          <Field label="Project" hint={toProject.length > 0 ? 'Every string picked, each on its own, in one run' : 'Pick the strings in the view or the Objects table: by a box, or with Ctrl held'}>
            <span data-slot="strings-on" className={toProject.length > 0 ? undefined : 'text-muted-foreground'}>
              {toProject.length === 1 ? toProject[0].name : toProject.length > 0 ? plural(toProject.length, 'string') : 'No string picked'}
            </span>
          </Field>
        ) : mode === 'join' ? (
          <Field
            label="Join"
            hint={
              toJoin.length < 2
                ? 'Pick two open strings or more, by a box or with Ctrl held: the pieces of a toe, say'
                : closedToJoin > 0
                  ? `${closedToJoin} of them ${closedToJoin === 1 ? 'is' : 'are'} closed, with no ends to join: open ${closedToJoin === 1 ? 'it' : 'them'} first, or leave ${closedToJoin === 1 ? 'it' : 'them'} out`
                  : `End to end into ${toJoin[0].name}, the one picked last: it keeps its name and the way it runs, and the others are deleted`
            }
          >
            <span className="flex min-w-0 flex-col">
              <span data-slot="strings-on" className={toJoin.length > 1 ? undefined : 'text-muted-foreground'}>
                {toJoin.length > 1 ? plural(toJoin.length, 'string') : toJoin.length === 1 ? `${toJoin[0].name}, and one more to pick` : 'No string picked'}
              </span>
              {toJoin.length > 1 && (
                <span data-slot="strings-on-names" className="truncate text-muted-foreground" title={toJoin.map((o) => o.name).join(', ')}>
                  {firstNames(toJoin.map((o) => o.name))}
                </span>
              )}
            </span>
          </Field>
        ) : onVertices ? (
          <Field label="On" hint="Only the vertices picked change, the rest as they are. Esc in the view lets them go and the tool works on the strings whole">
            <span data-slot="strings-on">
              {vertexWhat} picked on {plural(picks.length, 'string')}
            </span>
          </Field>
        ) : ONE_STRING.includes(mode) ? (
          <Field label="On" hint={string ? (picked.length > 1 ? `The last of the ${picked.length} picked: this tool works on one string` : undefined) : 'Pick a string in the view or the Objects table'}>
            <span data-slot="strings-on" className={string ? undefined : 'text-muted-foreground'}>
              {string ? `${string.name}${string.closed ? ' · closed' : ''}` : 'No string picked'}
            </span>
          </Field>
        ) : (
          <Field label="On" hint={many ? 'Every string picked, as one run and one undo' : on.length === 1 ? undefined : 'Pick a string in the view or the Objects table, or several by a box or Ctrl-click'}>
            <span className="flex min-w-0 flex-col">
              <span data-slot="strings-on" className={on.length > 0 ? undefined : 'text-muted-foreground'}>
                {on.length === 0 ? 'No string picked' : many ? `${on.length} strings` : `${on[0].name}${on[0].closed ? ' · closed' : ''}`}
              </span>
              {many && (
                <span data-slot="strings-on-names" className="truncate text-muted-foreground" title={on.map((o) => o.name).join(', ')}>
                  {firstNames(on.map((o) => o.name))}
                </span>
              )}
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
          <Field label="Side" hint={many ? "Looking along each string. One that runs the other way to the first goes to the other side, and the run says so" : 'Looking along the string'}>
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
          {onVertices ? (
            <Field label="Along" hint="Measured in plan along each string from its first vertex picked, through the vertices between">
              <span data-slot="strings-grade-along" className="text-muted-foreground">
                From the first vertex picked
              </span>
            </Field>
          ) : many ? (
            <Field label="Along" hint="From and To vertex are for one string">
              <span data-slot="strings-grade-along" className="text-muted-foreground">
                Each string, first vertex to last
              </span>
            </Field>
          ) : (
            <>
              <Field label="From vertex">
                <NumberInput value={from} onChange={setFrom} label="Start vertex" range={{ min: 1 }} />
              </Field>
              <Field label="To vertex">
                <NumberOrBlank value={to} onChange={setTo} label="End vertex" range={{ min: 1 }} blank="end" />
              </Field>
            </>
          )}
          <Field label="At RL" hint={onVertices ? "The RL at each string's first vertex picked" : many ? "The RL at each string's first vertex" : 'The RL at the start vertex'}>
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
          {!onVertices && (
            <Field label="Follow" hint="Adds vertices where the string crosses the surface's triangle edges">
              <Checkbox checked={crossings} aria-label="Follow the surface between vertices" onCheckedChange={(c) => setCrossings(c === true)} />
            </Field>
          )}
        </ToolSection>
      )}

      {mode === 'project' && (
        <ToolSection title="Settings">
          <Field label="To" hint="An RL, a grid or a surface, or a height above or below each string, all the way along it">
            <TargetInput value={projectTo} surfaces={surfaces} onChange={setProjectTo} label="What the strings are projected to" none="A height" />
            {projectTo === null && <NumberInput value={projectHeight} onChange={setProjectHeight} label="Height above or below each string" unit="m" range={{ min: 0, minIncluded: false }} />}
          </Field>
          <Field label="Going">
            <Choice value={direction} options={['up', 'down'] as const} labels={{ up: 'Up', down: 'Down' }} onChange={setDirection} label="Up from the strings, or down" />
          </Field>
          <Field label="Batter" hint="From horizontal, square to the string in plan">
            <NumberInput value={batter} onChange={setBatter} label="Batter" unit="°" range={{ min: 0, max: 90, minIncluded: false }} />
          </Field>
          <Field label="Side" hint="Where the new string comes out, looking along each string, whether it goes up or down">
            <Choice value={side} options={['left', 'right'] as const} labels={{ left: 'Left', right: 'Right' }} onChange={setSide} label="The side the new string comes out on, looking along the string" />
            <Button variant="outline" size="sm" className="h-7 gap-1 px-2 text-xs" disabled={toProject.length === 0} title="Click in the view on the side the new string goes: left or right is set from the string picked last" onClick={() => void clickSide()}>
              <MousePointerClick className="size-3.5" /> Click it
            </Button>
          </Field>
          <Field label="Wall" hint="Keeps the wall between each string and its new one, as a surface">
            <Checkbox checked={keepWall} aria-label="Keep the wall as a surface" onCheckedChange={(c) => setKeepWall(c === true)} />
          </Field>
        </ToolSection>
      )}

      {mode === 'ramp_polygon' && (
        <ToolSection title="Settings">
          <Field label="Width">
            <NumberInput value={rampWidth} onChange={setRampWidth} label="Road width" unit="m" range={{ min: 0, minIncluded: false }} />
          </Field>
          <Field label="Grade" hint="Along its inside edge. What it runs to says whether it climbs or falls">
            <NumberInput value={rampGrade} onChange={setRampGrade} label="Ramp grade" unit="%" range={{ min: 0, minIncluded: false }} />
          </Field>
          <Field label="Side" hint="The side of its inside edge the road lies on, looking the way it runs">
            <Choice value={side} options={['left', 'right'] as const} labels={{ left: 'Left', right: 'Right' }} onChange={setSide} label="The side the road lies on, looking the way it runs" />
          </Field>
          <Field label="From RL" hint="The RL it starts at. Left blank, the RL of where you click: on the line, or on a surface">
            <NumberOrBlank value={rampStartRl} onChange={setRampStartRl} label="RL the ramp starts at" blank="clicked" />
          </Field>
          <Field label="To" hint="It ends where its inside edge reaches the RL, or meets the grid or surface">
            <TargetInput value={rampTo} surfaces={surfaces} onChange={setRampTo} label="What the ramp runs to" none="Choose…" />
          </Field>
        </ToolSection>
      )}

      {mode === 'ramp_polygon' && placedRamp && lastRamp && (
        <ToolSection title="Placed">
          <Field label={placedRamp.name} hint="Move it places it again from a new start with the settings above: the same polygon, moved. One undo puts it back">
            <Button variant="outline" size="sm" className="h-7 gap-1 px-2 text-xs" disabled={!rampReady || busy} title="Two clicks in the view: its new start, and the way it runs" onClick={() => void placeRamp(lastRamp)}>
              <MousePointerClick className="size-3.5" /> Move it
            </Button>
          </Field>
        </ToolSection>
      )}

      {mode === 'surface_between' && (
        <ToolSection title="Settings">
          <Field label="Name" hint="Left blank, it's named for the strings at either side">
            <Input className="h-7 w-40 px-1.5 text-xs" value={surfaceName} aria-label="Name of the surface" onChange={(e) => setSurfaceName(e.target.value)} />
          </Field>
          <Field label="To layer" hint="Left blank, the layer of the string picked first">
            <Input className="h-7 w-40 px-1.5 text-xs" value={surfaceLayer} aria-label="Layer for the surface" onChange={(e) => setSurfaceLayer(e.target.value)} />
          </Field>
        </ToolSection>
      )}

      {mode === 'split' && (
        <ToolSection title="Settings">
          <Field label="At" hint="A click on the string, or a vertex by its number; a closed string opens there">
            <Choice value={splitBy} options={['click', 'vertex'] as const} labels={{ click: 'A click', vertex: 'A vertex' }} onChange={setSplitBy} label="Where the string splits" />
          </Field>
          {splitBy === 'vertex' && (
            <Field label="At vertex">
              <NumberOrBlank value={splitAt} onChange={setSplitAt} label="Vertex to split at" range={{ min: 1 }} blank="" />
            </Field>
          )}
        </ToolSection>
      )}

      {mode === 'trim' && (
        <ToolSection title="Settings">
          <Field label="Keep" hint={keep === 'side' ? 'The side of the cutting string you click, inside or outside a closed one, for every string picked' : 'Every part: each string is split where the cutting string crosses it'}>
            <Choice value={keep} options={['side', 'both'] as const} labels={{ side: 'The side clicked', both: 'Both, split' }} onChange={setKeep} label="What the trim keeps" />
          </Field>
        </ToolSection>
      )}

      {mode === 'clip' && (
        <ToolSection title="Settings">
          <Field label="Keep" hint={clipKeep === 'inside' ? 'The parts inside the closed string clicked, such as toes inside a pit limit' : 'The parts outside the closed string clicked'}>
            <Choice value={clipKeep} options={['inside', 'outside'] as const} labels={{ inside: 'Inside', outside: 'Outside' }} onChange={setClipKeep} label="What the clip keeps" />
          </Field>
        </ToolSection>
      )}

      {mode === 'polygon_boolean' && (
        <ToolSection title="Settings">
          {openPicked > 0 && (
            <Field label="Open" hint="Only closed strings have an inside: close these first, or leave them out">
              <span className="text-warning">
                {plural(openPicked, 'string')} picked {openPicked === 1 ? 'is' : 'are'} open
              </span>
            </Field>
          )}
          <Field
            label="Make"
            hint={
              operation === 'union'
                ? 'Everywhere inside any of the strings picked, as one set: those that touch or overlap become one'
                : operation === 'overlap'
                  ? 'Where the strings picked and a closed string clicked overlap'
                  : 'The strings picked less a closed string clicked'
            }
          >
            <Choice value={operation} options={['union', 'overlap', 'difference'] as const} labels={{ union: 'Union', overlap: 'Overlap', difference: 'Difference' }} onChange={setOperation} label="What to make of the polygons" />
          </Field>
          <Field label="Name" hint="A second part is numbered on, and a hole named for its part">
            <Input className="h-7 px-1.5 text-xs" value={opName} placeholder={operation === 'union' ? 'Union' : operation === 'overlap' ? 'Overlap' : 'Difference'} aria-label="Name for the polygons made" onChange={(e) => setOpName(e.target.value)} />
          </Field>
          <Field label="Layer">
            <Input className="h-7 px-1.5 text-xs" value={opLayer} aria-label="Layer for the polygons made" onChange={(e) => setOpLayer(e.target.value)} />
          </Field>
        </ToolSection>
      )}

      {mode === 'extend' && (
        <ToolSection title="Settings">
          <Field label="Reach" hint="The furthest an end runs on, in plan. One that meets nothing within it stops the run, and is named">
            <NumberInput value={reach} onChange={setReach} label="Furthest an end runs on" unit="m" range={{ min: 0, minIncluded: false }} />
          </Field>
        </ToolSection>
      )}

      {mode === 'join' && (
        <ToolSection title="Settings">
          <Field label="Ends within" hint="Ends this close share one vertex; further apart, a segment bridges them and it's said. Pieces making a ring close">
            <NumberInput value={joinWithin} onChange={setJoinWithin} label="How close ends share a vertex" unit="mm" range={{ min: 0 }} />
          </Field>
        </ToolSection>
      )}

      {mode === 'set_closed' && mixed && (
        <ToolSection title="Settings">
          <Field label="Do" hint="Some of the strings picked are open and some closed: the ones already so are left">
            <Choice value={closeOrOpen} options={['close', 'open'] as const} labels={{ close: 'Close', open: 'Open' }} onChange={setCloseOrOpen} label="Close the strings picked, or open them" />
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

      {mode === 'fillet' && (
        <ToolSection title="Settings">
          <Field label="Radius" hint="Each corner's radius in plan; a corner whose legs are too short takes the biggest that fits, and the run says which">
            <NumberOrBlank value={radius} onChange={setRadius} label="Corner radius" unit="m" range={{ min: 0, minIncluded: false }} blank="" />
          </Field>
          <Field label="Corners over" hint="Only corners turning more than this are rounded">
            <NumberInput value={cornerAngle} onChange={setCornerAngle} label="Corners turning more than" unit="°" range={{ min: 0, max: 179 }} />
          </Field>
          <Field label="Every" hint="A vertex every so many metres along each arc, its heights graded evenly from the legs'">
            <NumberInput value={arcSpacing} onChange={setArcSpacing} label="Metres between vertices along an arc" unit="m" range={{ min: 0, minIncluded: false }} />
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

      {mode === 'densify' && (
        <ToolSection title="Settings">
          <Field label="Every" hint="The most a vertex is from the next, in plan: each segment split into equal parts, the line as it was">
            <NumberInput value={denseSpacing} onChange={setDenseSpacing} label="Vertex spacing" unit="m" range={{ min: 0, minIncluded: false }} />
          </Field>
        </ToolSection>
      )}
    </ToolPanel>
  );
}
