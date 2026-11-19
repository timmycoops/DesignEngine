import { Check } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { ObjectSummary } from '@/bridge';
import { Choice, Field, NumberInput, resultParams, type RunReport, Select, type ToolAction, ToolPanel, ToolSection, useResultAs } from '@/design-system/kit';
import { choiceWords } from '@/tools';
import { useWorkspace } from '@/workspace/context';
import { countedWords, moveBetween } from './moveWords';

/**
 * Move and copy (String tools plan, workstream 3): what's picked moved, rotated in plan or mirrored in plan as a whole,
 * in one run and one undo. Strings, surfaces, solids and grids; a grid only moves, its rows running east and north. Move
 * goes by two clicks in the view, from a point to where it goes, or by an easting, northing and RL typed. Rotate turns
 * about a point clicked, by an angle typed, anticlockwise. Mirror goes across a line clicked. Each is in place, or as
 * new on a layer (decision 0091), which is what Copy was: a copy named for its original with " copy", on the layer given
 * or its original's, the originals left as they were, picked once made, ready to move on.
 *
 * While the Workflow panel is adding steps, the button adds the tool as a step instead of running it (decision 0078):
 * the objects picked are found by rule each time it runs, and the clicks are asked for as the step is added.
 */

type Mode = 'move' | 'rotate' | 'mirror';

const MODES: readonly Mode[] = ['move', 'rotate', 'mirror'];
const { labels: MODE_LABELS, lines: MODE_LINES } = choiceWords('move', MODES);

/** What moves whole: what's drawn. */
const MOVES = new Set(['string', 'surface', 'solid', 'grid']);

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

export function MovePanel() {
  const { summary, selection, selected, runRecord, clearRefusal, setSelection, pickLine, pickPoint, toolAsk, toolMode } = useWorkspace();
  const [mode, setMode] = useState<Mode>('move');
  /** In place, or as new on a layer: a copy (decision 0091). */
  const resultAs = useResultAs();
  useEffect(() => {
    if (toolAsk?.panel === 'move' && MODES.includes(toolAsk.mode as Mode)) setMode(toolAsk.mode as Mode);
  }, [toolAsk]);
  useEffect(() => {
    toolMode('move', mode);
    return () => toolMode('move', null);
  }, [toolMode, mode]);

  /** Move and Copy: by two clicks, or by distances typed. */
  const [by, setBy] = useState<'clicks' | 'typed'>('clicks');
  /** By two clicks: heights kept, a move in plan, or moved by the clicks' RL difference too. */
  const [heights, setHeights] = useState<'keep' | 'move'>('keep');
  const [east, setEast] = useState(0);
  const [north, setNorth] = useState(0);
  const [up, setUp] = useState(0);
  const [angle, setAngle] = useState(90);
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  const pickedIds = new Set(selection);
  const picked: ObjectSummary[] = objects.filter((o) => pickedIds.has(o.id) && MOVES.has(o.kind));
  const last = objects.find((o) => o.id === selected && MOVES.has(o.kind));
  /** What's moved: everything picked that moves whole, or the one picked last. */
  const on = picked.length > 0 ? picked : last ? [last] : [];
  const ids = on.map((o) => o.id);
  const what = on.length === 1 ? on[0].name : countedWords(on);
  const grids = on.filter((o) => o.kind === 'grid').length;
  const turning = mode === 'rotate' || mode === 'mirror';
  /** A grid only moves. */
  const gridsStop = turning && grids > 0;
  const copying = resultAs.asNew;
  const command = 'object.transform';

  const runIt = async (params: Record<string, unknown>) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, { ...params, ...resultParams(resultAs) });
    setBusy(false);
    setReport(outcome.record ? { notes: outcome.record.notes } : { notes: [], error: outcome.error });
    // Copies are picked once made, ready to move on.
    const made = outcome.record?.outputs.filter((o) => o.change === 'created').map((o) => o.id) ?? [];
    if (copying && made.length > 0) setSelection(made);
    return outcome;
  };

  const typedMove = () => ({ kind: 'move', by: [east, north, up] });
  /** The transform with its clicks, or null when Esc is pressed. */
  const clicked = async (): Promise<Record<string, unknown> | null> => {
    if (mode === 'rotate') {
      const about = await pickPoint(`Click the point to turn ${what} about. Esc cancels`);
      return about ? { kind: 'rotate', about: [about[0], about[1]], angle } : null;
    }
    if (mode === 'mirror') {
      const line = await pickLine(`Click where the mirror line starts. Esc cancels`, 'Click where it ends. Esc cancels');
      return line ? { kind: 'mirror', from: [line[0][0], line[0][1]], to: [line[1][0], line[1][1]] } : null;
    }
    if (by === 'typed') return typedMove();
    const line = await pickLine(`Click a point on ${what} to move it from. Esc cancels`, 'Click where that point goes. Esc cancels');
    return line ? { kind: 'move', by: moveBetween(line[0], line[1], heights) } : null;
  };
  const params = async () => {
    const transform = await clicked();
    return transform ? { objects: ids, transform } : null;
  };
  const run = async () => {
    const p = await params();
    if (p) await runIt(p);
  };
  const needsClicks = turning || by === 'clicks';
  const typedNothing = !needsClicks && east === 0 && north === 0 && up === 0 && mode === 'move';

  const label = (() => {
    switch (mode) {
      case 'move':
        return `${copying ? 'Copy' : 'Move'} ${what}`;
      case 'rotate':
        return `${copying ? 'Copy and rotate' : 'Rotate'} ${what}`;
      case 'mirror':
        return `${copying ? 'Copy and mirror' : 'Mirror'} ${what}`;
    }
  })();
  const title =
    mode === 'rotate'
      ? 'A click in the view on the point to turn about'
      : mode === 'mirror'
        ? 'Two clicks in the view: the line to mirror across, in plan'
        : by === 'clicks'
          ? 'Two clicks in the view: a point, then where it goes. Clicks snap to vertices and surfaces'
          : undefined;
  const action: ToolAction = {
    label,
    icon: <Check className="size-3.5" />,
    disabled: on.length === 0 || gridsStop || typedNothing,
    busy,
    title,
    run,
    step: needsClicks ? { command, params: { objects: ids }, ask: params } : { command, params: { objects: ids, transform: typedMove() } },
    result: resultAs,
  };

  return (
    <ToolPanel title="Move and copy" panel="move" summary={MODE_LINES[mode]} result={report} action={action}>
      <ToolSection>
        <Field label="Tool">
          <Select value={mode} aria-label="Move tool" onChange={(e) => setMode(e.target.value as Mode)}>
            {MODES.map((m) => (
              <option key={m} value={m}>
                {MODE_LABELS[m]}
              </option>
            ))}
          </Select>
        </Field>
      </ToolSection>

      <ToolSection title="Picked">
        <Field
          label="On"
          hint={
            on.length === 0
              ? 'Pick strings, surfaces, solids or grids in the view or the Objects table, by a box or Ctrl-click'
              : gridsStop
                ? `${plural(grids, 'grid')} picked: a grid moves but doesn't rotate or mirror, its rows running east and north. Leave ${grids === 1 ? 'it' : 'them'} out`
                : on.length > 1
                  ? 'Everything picked, as one run and one undo'
                  : undefined
          }
        >
          <span data-slot="move-on" className={on.length > 0 ? undefined : 'text-muted-foreground'}>
            {on.length === 0 ? 'Nothing picked' : what}
          </span>
        </Field>
      </ToolSection>

      <ToolSection title="Settings">
        {mode === 'move' && (
          <>
            <Field label="By" hint={by === 'clicks' ? 'A point on what is picked, then where it goes: clicks snap to vertices and surfaces' : 'East, north and up are positive'}>
              <Choice value={by} options={['clicks', 'typed'] as const} labels={{ clicks: 'Two clicks', typed: 'Distances' }} onChange={setBy} label="Moved by two clicks or by distances typed" />
            </Field>
            {by === 'clicks' ? (
              <Field label="Heights" hint={heights === 'keep' ? 'Moved in plan: every RL as it was' : 'Up or down by the RL between the two clicks too'}>
                <Choice value={heights} options={['keep', 'move'] as const} labels={{ keep: 'Keep RLs', move: 'Move by the clicks' }} onChange={setHeights} label="Keep the RLs, or move them by the clicks" />
              </Field>
            ) : (
              <>
                <Field label="East">
                  <NumberInput value={east} onChange={setEast} label="Distance east, west negative" unit="m" />
                </Field>
                <Field label="North">
                  <NumberInput value={north} onChange={setNorth} label="Distance north, south negative" unit="m" />
                </Field>
                <Field label="Up" hint={typedNothing ? 'A move needs a distance: east, north or up' : undefined}>
                  <NumberInput value={up} onChange={setUp} label="Distance up, down negative" unit="m" />
                </Field>
              </>
            )}
          </>
        )}
        {mode === 'rotate' && (
          <Field label="Angle" hint="Anticlockwise seen from above; a minus angle turns clockwise. Heights stay as they are">
            <NumberInput value={angle} onChange={setAngle} label="Angle to turn, anticlockwise" unit="°" />
          </Field>
        )}
        {mode === 'mirror' && (
          <Field label="Across" hint="A line clicked in plan: two clicks. Heights stay as they are">
            <span className="text-muted-foreground">A line clicked</span>
          </Field>
        )}
      </ToolSection>
    </ToolPanel>
  );
}
