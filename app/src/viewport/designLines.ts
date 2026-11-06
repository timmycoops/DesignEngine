// Which string a click picks, and the lines a face design makes. A design's own toes, crests and road edges are
// made again each time it runs, so an edit to one is lost at the next run; Edit from here (✎) copies a row's line
// to a string of the planner's own, on top of the design's. Where they lie on each other, a click keeps the line
// already selected, then takes one of the planner's own over the design's, so the copy is the one edited. Plain
// numbers and strings, no three.js, so tests/designLines.test.mjs runs it under Node.

/** A string near the cursor: how far off on screen, and whether a face design made it. */
export interface NearLine {
  id: string;
  /** Pixels from the cursor to the nearest of its segments. */
  distance: number;
  madeByDesign: boolean;
}

/** Strings this much further off than the nearest still count as under the cursor with it, in pixels. */
export const TIE_PX = 2;

/**
 * The string a click picks from those near it: the nearest within `pickPx`, unless others lie as near, to
 * `TIE_PX`. Then the selected one if it's among them, so a click on the line you're editing keeps it; else the
 * nearest of your own over a design's.
 */
export function lineToPick(near: NearLine[], selected: string | null, pickPx: number): string | null {
  const within = near.filter((n) => n.distance <= pickPx).sort((a, b) => a.distance - b.distance);
  if (within.length === 0) return null;
  const tied = within.filter((n) => n.distance <= within[0].distance + TIE_PX);
  if (selected && tied.some((n) => n.id === selected)) return selected;
  return (tied.find((n) => !n.madeByDesign) ?? tied[0]).id;
}

/** What a design-made string carries (zd-commands `face.rs`): the design's id, and for wall lines their row. */
export interface DesignLine {
  name: string;
  /** The `face_design` attribute: the id of the design that made it. */
  design: string;
  /** The `step` attribute: the wall's row for its toe and crest, the last row for the next toe. */
  step: number | null;
}

/**
 * The row whose ✎ redraws this line by hand: the line a row ends on. A wall's crest ends its own row, its toe the
 * row before it (the berm under it), and the next toe the design's last row. Null for a line no row ends on: the
 * first wall's toe (the design's string) and road edges.
 */
export function rowEndingOn(line: DesignLine): number | null {
  if (line.step === null) return null;
  if (/ next toe$/.test(line.name) || / crest \d+$/.test(line.name)) return line.step;
  if (/ toe \d+$/.test(line.name)) return line.step > 1 ? line.step - 1 : null;
  return null;
}

/** Why an edit to a design's own line is refused, and what to do instead. */
export function designLineMessage(line: DesignLine, designName: string | undefined): string {
  const by = designName ? `the design ${designName}` : 'a face design';
  const row = rowEndingOn(line);
  const instead = row !== null
    ? `In Face design, ✎ on row ${row} gives you a line of your own to change.`
    : 'Change the design and run it again instead.';
  return `${line.name} is made by ${by} and made again each time it runs, so an edit to it would be lost. ${instead}`;
}
