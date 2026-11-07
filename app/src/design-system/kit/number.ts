/**
 * Reading a number as it's typed (UI plan: one way of working, item 4). A box takes what parses and is in range; anything
 * else it holds as typed and says why it won't take it, so a half-typed value never reaches a command.
 */

export interface Range {
  /** The smallest value taken, if there is one. */
  min?: number;
  /** The largest value taken, if there is one. */
  max?: number;
  /** Whether the smallest is itself taken (true by default). A width or a batter of 0 isn't. */
  minIncluded?: boolean;
}

export type Read = { value: number } | { why: string };

/** The number in `text`, with its thousands commas and spaces allowed, or why it isn't one the box takes. */
export function readNumber(text: string, range: Range = {}): Read {
  const plain = text.trim().replace(/[,\s]/g, '');
  if (plain === '') return { why: 'Type a number' };
  const value = Number(plain);
  if (!Number.isFinite(value)) return { why: `${text.trim()} isn't a number` };
  const { min, max, minIncluded = true } = range;
  if (min !== undefined && (minIncluded ? value < min : value <= min)) return { why: minIncluded ? `At least ${min}` : `More than ${min}` };
  if (max !== undefined && value > max) return { why: `At most ${max}` };
  return { value };
}

/** A number as a box shows it: as it is, with no trailing zeros and no thousands commas, so it reads back the same. */
export function showNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : String(Number(value.toFixed(6)));
}
