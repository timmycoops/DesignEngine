
/**
 * The formula editor's words (decision 0077, 7d): a filter as typed and read back, a new formula's first lines, and what
 * Copy for AI and Explain hand an AI: how formulas are written, the dataflow's names and the formula, never a value.
 * Kept apart from the panel, with no runtime imports, so the tests run it on its own.
 */

/** A filter in the bench rules' words, as typed: `material: coal; seam: U, M`. */
export const whenText = (when: Record<string, string> | undefined) =>
  Object.entries(when ?? {})
    .map(([k, v]) => `${k}: ${v}`)
    .join('; ');

export function readWhen(text: string): Record<string, string> {
  const when: Record<string, string> = {};
  for (const part of text.split(';')) {
    const [k, ...v] = part.split(':');
    if (k?.trim() && v.join(':').trim()) when[k.trim()] = v.join(':').trim();
  }
  return when;
}

/** A new formula's first words. */
export const FORMULA_TEMPLATE = `# What this formula does, in a line or two.
def formula(unit, p):
    if unit.material.lower() != "coal":
        return
    # unit.<field> = ...
`;


