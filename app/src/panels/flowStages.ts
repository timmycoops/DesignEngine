import type { DataflowState, FlowGroup, FlowStep } from '@/bridge';
import type { Stage } from './reservesCase.ts';

/**
 * The flow across the Reserves case tab (the Build Plan's "Reserves case plan", decision 10, workstream 6): the case's
 * quantities carried through the dataflow by stage. Coal and waste are two lanes; coal a stage takes (a loss, thin coal,
 * high ash) leaves the coal lane red, and rock it lets into ROM (dilution) leaves the waste lane amber. Coal is counted
 * in tonnes where the units weigh, waste in BCM.
 */

const isCoal = (g: FlowGroup) => g.material === 'coal';

/** What became of a group, in words: `loss (roof)`, `thinloss (thin)`. */
export const becameWords = (g: FlowGroup) => (g.became ? (g.detail ? `${g.became} (${g.detail})` : g.became) : '');

/** Coal and waste still in play after a step, and what has left each lane so far, by what became of it. */
export interface Lanes {
  coal: number;
  coalTonnes: number | null;
  waste: number;
  /** Coal that became something, by what (`loss (roof)`): volume and tonnes. */
  out: Map<string, { volume: number; tonnes: number | null }>;
  /** Rock let in, by what (`dilution (roof)`): volume. */
  letIn: Map<string, number>;
}

const addTonnes = (a: number | null, b: number | null) => (a === null ? b : b === null ? a : a + b);

/** The lanes after one step. */
export function lanesAt(f: FlowStep): Lanes {
  const lanes: Lanes = { coal: 0, coalTonnes: null, waste: 0, out: new Map(), letIn: new Map() };
  for (const g of f.groups) {
    if (!g.became) {
      if (isCoal(g)) {
        lanes.coal += g.volume;
        lanes.coalTonnes = addTonnes(lanes.coalTonnes, g.tonnes);
      } else lanes.waste += g.volume;
    } else if (isCoal(g)) {
      const was = lanes.out.get(becameWords(g)) ?? { volume: 0, tonnes: null };
      lanes.out.set(becameWords(g), { volume: was.volume + g.volume, tonnes: addTonnes(was.tonnes, g.tonnes) });
    } else lanes.letIn.set(becameWords(g), (lanes.letIn.get(becameWords(g)) ?? 0) + g.volume);
  }
  return lanes;
}

/** Something a stage moved: what, its volume and tonnes. */
export interface Moved {
  what: string;
  volume: number;
  tonnes: number | null;
}

/** A stage of the flow: its steps, the lanes after it, and what it took off the coal lane or let in from the waste. */
export interface FlowStage {
  name: string;
  steps: number[];
  coal: number;
  coalTonnes: number | null;
  waste: number;
  /** Coal it took, net: the coal lane before it less after it (m³, t). */
  lost: number;
  lostTonnes: number | null;
  /** What it took, by what became of it, each that grew. */
  lostBy: Moved[];
  /** Rock it let in, net (m³), and by what. */
  letIn: number;
  letInBy: Moved[];
  /** Blocks stopped by its end. */
  stopped: number;
}

const grew = <T>(before: Map<string, T>, after: Map<string, T>, volume: (t: T) => number, tonnes: (t: T) => number | null): Moved[] =>
  [...after.entries()]
    .map(([what, now]) => {
      const was = before.get(what);
      const t0 = was === undefined ? 0 : tonnes(was);
      const t1 = tonnes(now);
      return { what, volume: volume(now) - (was === undefined ? 0 : volume(was)), tonnes: t1 === null ? null : t1 - (t0 ?? 0) };
    })
    .filter((m) => m.volume > 1e-9)
    .sort((a, b) => b.volume - a.volume);

/**
 * The flow by stage: the blocks as read first, then each stage of the dataflow (`stages`, by step number), each with the
 * lanes after its last step and what it took or let in against the step before its first. A stage whose steps haven't a
 * flow yet is left out.
 */
export function flowStages(flow: FlowStep[], stages: Stage[]): FlowStage[] {
  const at = new Map(flow.map((f) => [f.step, f]));
  const blocks = at.get(0);
  if (!blocks) return [];
  const all: Stage[] = [{ name: 'Blocks', steps: [0] }, ...stages];
  const out: FlowStage[] = [];
  for (const s of all) {
    const first = s.steps[0];
    const end = at.get(s.steps[s.steps.length - 1]);
    const start = first === 0 ? null : at.get(first - 1);
    if (!end || (first > 0 && !start)) continue;
    const a = lanesAt(end);
    const b = start ? lanesAt(start) : null;
    const lostBy = b ? grew(b.out, a.out, (x) => x.volume, (x) => x.tonnes) : [];
    const letInBy = b ? grew(b.letIn, a.letIn, (x) => x, () => null) : [];
    // What left the coal lane in tonnes is what the groups that became something gained, net: a change of density on
    // the coal (its in situ basis) moves the lane's tonnes but takes no coal.
    const outTonnes = (l: Lanes) => [...l.out.values()].reduce<number | null>((x, y) => (x === null || y.tonnes === null ? null : x + y.tonnes), 0);
    const [ta, tb] = [outTonnes(a), b ? outTonnes(b) : null];
    const lostTonnes = b && ta !== null && tb !== null ? ta - tb : null;
    out.push({
      name: s.name,
      steps: s.steps,
      coal: a.coal,
      coalTonnes: a.coalTonnes,
      waste: a.waste,
      lost: b ? b.coal - a.coal : 0,
      lostTonnes,
      lostBy,
      letIn: b ? [...a.letIn.values()].reduce((x, y) => x + y, 0) - [...b.letIn.values()].reduce((x, y) => x + y, 0) : 0,
      letInBy,
      stopped: end.stopped,
    });
  }
  return out;
}

/** A dataflow's range in words: `strip 1-6`, or every row (a row dataflow) or the case's blocks (the reserves one). */
export function rangeWords(d: Pick<DataflowState, 'range' | 'reserves'>): string {
  const range = Object.entries(d.range ?? {});
  if (range.length === 0) return d.reserves ? "the case's blocks" : 'every row';
  return range.map(([k, v]) => `${k} ${v}`).join(', ');
}

/** The dataflows to run for Run what's out of date or Run all, in run order. */
export function toRun(dataflows: DataflowState[], all: boolean): DataflowState[] {
  return all ? dataflows : dataflows.filter((d) => !d.up_to_date);
}
