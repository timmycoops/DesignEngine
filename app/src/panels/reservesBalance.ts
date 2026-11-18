import type { BreakdownField } from '../bridge';

/**
 * The Balance view of a reserves table's node (decision 0086): its coal, waste and total from Imported through Insitu
 * and ROM to Scheduled, what moved between them, and each stage's check against the one before, read from the table's
 * `Summary\Balance` fields as the engine added them up for the node. Laid out here; the numbers are the engine's.
 */

export type BalanceKind = 'stage' | 'move' | 'check';

/** A row of the balance: a stage's volumes, a move's (signed as it changes each part), or a check. */
export interface BalanceRow {
  key: string;
  label: string;
  kind: BalanceKind;
  coal: number | null;
  waste: number | null;
  total: number | null;
  /** A check: whether every part holds, within 1e-6 of the node's imported volume. */
  holds?: boolean;
}

const at = (fields: BreakdownField[], column: number) => (part: string, name: string): number | null =>
  fields.find((f) => f.column === `Summary\\Balance\\${part}\\${name}`)?.values[column] ?? null;

/** Whether the table has a balance at all: a table made before decision 0086 hasn't, until it's run again. */
export const hasBalance = (fields: BreakdownField[]) => fields.some((f) => f.column.startsWith('Summary\\Balance\\'));

const neg = (x: number | null) => (x === null ? null : -x);
const any = (...xs: (number | null)[]) => xs.some((x) => x !== null && x !== 0);

/**
 * The rows for the node (`column` 0) or one of its columns across: Imported, less small volume, Insitu and its check;
 * to waste (coal down, waste up), to coal (the other way), from outside (coal and total up), not mined (waste and total
 * down); ROM and its check; Scheduled and its check. A move none of the parts made is left out; a check is always there.
 */
export function balanceRows(fields: BreakdownField[], column = 0): BalanceRow[] {
  const v = at(fields, column);
  const part = (name: string) => ({ coal: v('Coal', name), waste: v('Waste', name), total: v('Total', name) });
  const size = Math.max(1, Math.abs(v('Total', 'imported') ?? 0));
  const rows: BalanceRow[] = [];
  const stage = (key: string, label: string) => rows.push({ key, label, kind: 'stage', ...part(key) });
  const move = (key: string, label: string, coal: number | null, waste: number | null, total: number | null) => {
    if (any(coal, waste, total)) rows.push({ key, label, kind: 'move', coal, waste, total });
  };
  const check = (key: string, label: string) => {
    const p = part(key);
    if (p.coal === null && p.waste === null && p.total === null) return;
    const holds = [p.coal, p.waste, p.total].every((x) => x === null || Math.abs(x) <= 1e-6 * size);
    rows.push({ key, label, kind: 'check', ...p, holds });
  };
  stage('imported', 'Imported');
  const small = part('small_volume');
  move('small_volume', 'Small volume set aside', neg(small.coal), neg(small.waste), neg(small.total));
  if (v('Total', 'insitu') !== null || v('Coal', 'insitu') !== null) {
    stage('insitu', 'Insitu');
    check('unaccounted_insitu', 'Unaccounted, imported to insitu');
  }
  const toWaste = v('Coal', 'to_waste');
  move('to_waste', 'Coal to waste (losses, thin, high ROM ash, discarded)', neg(toWaste), toWaste, null);
  const toCoal = v('Coal', 'to_coal');
  move('to_coal', 'Waste to coal (dilution, partings)', toCoal, neg(toCoal), null);
  const outside = v('Coal', 'from_outside');
  move('from_outside', 'From outside the model (underburden)', outside, null, outside);
  const unmined = v('Waste', 'not_mined');
  move('not_mined', 'Not mined (waste below the last coal)', null, neg(unmined), neg(unmined));
  stage('rom', 'ROM');
  check('unaccounted_rom', 'Unaccounted, to ROM');
  stage('scheduled', 'Scheduled');
  check('unaccounted_scheduled', 'Unaccounted, ROM to scheduled');
  return rows;
}
