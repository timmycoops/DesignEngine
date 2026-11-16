/**
 * The distance measure on the view (decision 0079): what lies between two clicked points, worked out from their world
 * coordinates as the engine gave them (f64 mine grid eastings, northings and RLs), never from what the screen draws.
 * Nothing is kept: it reads out and goes. Kept free of three and React, so the tests can read it.
 */
type Point = readonly [number, number, number];

/** One leg, from a point to the next. */
export interface Leg {
  /** Horizontal distance (m). */
  plan: number;
  /** Distance in space (m). */
  slope: number;
  /** RL at the end less RL at the start (m): up is positive. */
  rise: number;
  /** Rise over plan distance (%), up positive; null straight up or down, or between the same point twice. */
  grade: number | null;
  /** Grid bearing from the start to the end, degrees clockwise from north, 0 to 360; null with no plan distance. */
  bearing: number | null;
}

export function legBetween(a: Point, b: Point): Leg {
  const [dx, dy, rise] = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
  const plan = Math.hypot(dx, dy);
  const bearing = plan > 0 ? ((Math.atan2(dx, dy) * 180) / Math.PI + 360) % 360 : null;
  return { plan, slope: Math.hypot(plan, rise), rise, grade: plan > 0 ? (rise / plan) * 100 : null, bearing };
}

/** Every leg added up: along the points in plan and in space, and the last RL less the first. */
export function measureTotals(points: readonly Point[]): { legs: number; plan: number; slope: number; rise: number } {
  let [plan, slope] = [0, 0];
  for (let i = 1; i < points.length; i++) {
    const leg = legBetween(points[i - 1], points[i]);
    plan += leg.plan;
    slope += leg.slope;
  }
  return { legs: Math.max(0, points.length - 1), plan, slope, rise: points.length > 1 ? points[points.length - 1][2] - points[0][2] : 0 };
}

const metres = (value: number) => value.toFixed(2);
const signed = (value: number) => `${value >= 0 ? '+' : '−'}${Math.abs(value).toFixed(2)}`;
/** A bearing as three figures and a tenth: 045.0°. */
const bearingWords = (degrees: number) => {
  const tenth = (Math.round(degrees * 10) / 10) % 360;
  return `${tenth.toFixed(1).padStart(5, '0')}°`;
};

/** A leg's grade in words: level, vertical, or its per cent with 1 in n and the angle from horizontal. */
export function gradeWords(leg: Leg): string {
  if (leg.grade === null) return leg.rise === 0 ? 'no distance' : 'vertical';
  // Level to the hundredth of a per cent the words would show.
  if (Math.abs(leg.grade) < 0.005) return 'level';
  const angle = (Math.atan2(Math.abs(leg.rise), leg.plan) * 180) / Math.PI;
  return `${Math.abs(leg.grade).toFixed(1)} % ${leg.rise > 0 ? 'up' : 'down'} (1 in ${(leg.plan / Math.abs(leg.rise)).toFixed(1)}, ${angle.toFixed(1)}°)`;
}

/** A leg in full, for the status line. */
export function legWords(leg: Leg): string {
  const parts = [`plan ${metres(leg.plan)} m`, `slope ${metres(leg.slope)} m`, `RL ${signed(leg.rise)} m`, gradeWords(leg)];
  if (leg.bearing !== null) parts.push(`bearing ${bearingWords(leg.bearing)}`);
  return parts.join(' · ');
}

/** A leg in short, for the label beside the cursor. */
export function legLabel(leg: Leg): string {
  const parts = [`${metres(leg.plan)} m plan`, `${metres(leg.slope)} m slope`, `RL ${signed(leg.rise)}`];
  if (leg.grade !== null && Math.abs(leg.grade) >= 0.005) parts.push(`${Math.abs(leg.grade).toFixed(1)} %`);
  if (leg.bearing !== null) parts.push(bearingWords(leg.bearing));
  return parts.join(' · ');
}

/** What the status line says of the points clicked so far. */
export function measureWords(points: readonly Point[]): string {
  if (points.length === 0) return 'Measure: click the first point';
  if (points.length === 1) return `Measure: from RL ${points[0][2].toFixed(2)}; click the next point. Esc clears`;
  const last = legWords(legBetween(points[points.length - 2], points[points.length - 1]));
  if (points.length === 2) return `Measure: ${last}. Click on for a running total; Backspace takes a point off, Esc clears`;
  const total = measureTotals(points);
  return `Measure: ${last}. Total over ${total.legs} legs: plan ${metres(total.plan)} m · slope ${metres(total.slope)} m · RL ${signed(total.rise)} m`;
}
