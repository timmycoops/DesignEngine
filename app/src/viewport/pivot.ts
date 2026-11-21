// What the view turns about when nothing is under the cursor (decision 0044): the nearest thing to it on screen, found
// by a fan of rays in rings round the cursor; with nothing within the last ring, the last point turned about if it's
// still in view; else the view's centre.

/** The rings' distances from the cursor, in pixels, nearest first. */
export const FAN_RINGS_PX = [8, 16, 32, 64, 128, 256];
/** Rays in each ring: rays about 50 px apart at most, so a corner of ground within the last ring isn't missed between two. */
export const FAN_RAYS = [8, 8, 8, 8, 16, 32];

/**
 * The screen points of the fan round `(x, y)`, ring by ring, nearest first. Each ring is turned half a step, so a thin
 * edge between two rays of one ring is caught by the next.
 */
export function fanRings(x: number, y: number): [number, number][][] {
  return FAN_RINGS_PX.map((r, ring) =>
    Array.from({ length: FAN_RAYS[ring] }, (_, i) => {
      const a = ((i + (ring % 2) / 2) * 2 * Math.PI) / FAN_RAYS[ring];
      return [x + r * Math.cos(a), y + r * Math.sin(a)] as [number, number];
    }),
  );
}

/** A point projected to normalised device coordinates is in view: in front of the camera and inside the picture. */
export function inView(ndc: { x: number; y: number; z: number }): boolean {
  return Math.abs(ndc.x) <= 1 && Math.abs(ndc.y) <= 1 && ndc.z > -1 && ndc.z < 1;
}
