import type { DataflowStep, FormulaReport } from '../bridge';

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

/** What Copy for AI hands over: how to write one, the dataflow's names (never a value), this formula, and a request. */
export function forAi(reference: string, report: FormulaReport | null, step: DataflowStep, ask: 'write' | 'explain'): string {
  const names = report?.names;
  const fields = names ? names.fields.map(([n, u]) => (u ? `${n} (${u})` : n)).join(', ') : '';
  const params = names ? names.parameters.map(([n, u, words]) => `${n}${words ? ' (words)' : u ? ` (${u})` : ''}`).join(', ') : '';
  const where = whenText(step.formula?.when) || 'every unit';
  const lines = [
    reference.trim(),
    '',
    '## This dataflow',
    `Fields: ${fields}`,
    `Labels: ${names?.labels.join(', ') ?? 'seam, material, bench, became'}`,
    `Parameter columns: ${params}`,
    '',
    '## This formula',
    `Step \`${step.step}\`, after \`${report?.stage ?? '?'}\`, on units where ${where}.`,
    '```python',
    (step.formula?.script ?? '').trimEnd(),
    '```',
    '',
  ];
  if (ask === 'explain') {
    lines.push('Explain in plain words what this formula does, line by line, for a mine planner who doesn\'t program. Say what each name is and where its value comes from.');
  } else {
    lines.push(
      '## What I want',
      '<Say what the formula should do, or paste a Spry snippet or an Excel formula here.>',
      '',
      'Write the whole formula, using only the names above, then its worked examples as JSON: a list of {"name", "unit": {field: value}, "parameters": {column: value}, "expect": {field: value}}. Use made-up numbers, never a client\'s.',
    );
  }
  return lines.join('\n');
}

