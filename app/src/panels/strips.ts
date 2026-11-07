/**
 * Faces told apart by their strip number (decision 0049): the attribute a line drawn across the strips sets, which a
 * face keeps from its toe. What the Cut solids panel says about the faces it's given, before it slices by them.
 */

/** What the panel needs of an object. */
export interface Named {
  id: string;
  name: string;
  attributes: Record<string, boolean | number | string>;
}

/** A face's strip number, if it has one: a number, or text that reads as one. */
export function stripNumber(o: Named, attribute = 'strip'): number | null {
  const value = o.attributes[attribute];
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'string' && value.trim() !== '') {
    const n = Number(value.trim());
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

/** Faces by strip number, lowest first, and those with no number. */
export interface ByStrip {
  strips: { number: number; faces: Named[] }[];
  unnumbered: Named[];
}

export function byStrip(faces: Named[], attribute = 'strip'): ByStrip {
  const groups = new Map<number, Named[]>();
  const unnumbered: Named[] = [];
  for (const face of faces) {
    const n = stripNumber(face, attribute);
    if (n === null) unnumbered.push(face);
    else groups.set(n, [...(groups.get(n) ?? []), face]);
  }
  const strips = [...groups.entries()].sort((a, b) => a[0] - b[0]).map(([number, faces]) => ({ number, faces }));
  return { strips, unnumbered };
}

/** Numbers as a short run: "1–15", or "1–4, 6, 9–10" where some are missing. */
export function numberRuns(numbers: number[]): string {
  const sorted = [...numbers].sort((a, b) => a - b);
  const runs: string[] = [];
  for (let i = 0; i < sorted.length; ) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j += 1;
    runs.push(j > i ? `${sorted[i]}–${sorted[j]}` : `${sorted[i]}`);
    i = j + 1;
  }
  return runs.join(', ');
}

/** The faces in a line: how many, on how many strips, and which numbers. */
export function facesLine(by: ByStrip): string {
  const faces = by.strips.reduce((n, s) => n + s.faces.length, 0);
  if (faces === 0) return 'No faces with a strip number';
  const strips = by.strips.length;
  const numbers = numberRuns(by.strips.map((s) => s.number));
  const joined = faces > strips ? `, ${faces - strips === 1 ? 'one strip has' : 'some strips have'} more than one face, joined as they're sliced` : '';
  return `${faces} face${faces === 1 ? '' : 's'} on ${strips} strip${strips === 1 ? '' : 's'}: ${numbers}${joined}`;
}
