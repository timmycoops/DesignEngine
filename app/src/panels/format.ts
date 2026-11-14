import type { ObjectKind, ObjectSummary } from '@/bridge';

const formats = new Map<number, Intl.NumberFormat>();

/** A number with thousands separators and a fixed number of decimals. */
export function fmt(value: number, decimals = 0): string {
  let format = formats.get(decimals);
  if (!format) {
    format = new Intl.NumberFormat('en-AU', { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
    formats.set(decimals, format);
  }
  return format.format(value);
}

/** Mine grid coordinates to the millimetre. */
export const coord = (value: number) => value.toFixed(3);

export const clock = (unixMs: number) => new Date(unixMs).toLocaleTimeString('en-AU', { hour12: false });

export const kindLabel: Record<ObjectKind, string> = {
  string: 'String',
  surface: 'Surface',
  solid: 'Solid',
  grid: 'Grid',
  seam_model: 'Seam model',
  face_design: 'Face design',
  workflow: 'Workflow',
};

/** Coloured by RL or its layer's colour on its own: surfaces and elevation grids. A solid goes by its layer. */
export const canColour = (o: ObjectSummary) => o.drawn && o.kind !== 'solid';
/** Seen through on its own: what canColour takes, and seam horizons. A layer's solids go by the layer. */
export const canSeeThrough = (o: ObjectSummary) => canColour(o) || o.kind === 'seam_model';
