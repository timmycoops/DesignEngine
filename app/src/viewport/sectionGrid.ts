// The section's RL grid: RL lines, and distance along the section from A, on a canvas over the viewport.
// Everything is read from the section camera each frame, so a line sits exactly where that RL and
// distance are drawn. The spacing follows the zoom, the same both ways so the grid stays square on a
// true-scale section, and the labels stay on the screen's edges.
import { SECTION, SHADE, STEPS, TEXT } from './overview';

/** The least gap between grid lines on screen (CSS px): wide enough for a distance label. */
const LEAST_GAP = 64;
/** The band along the top that holds the distances (CSS px). */
const TOP_BAND = 24;
/** How close a distance label may come to the A or B tag (CSS px). */
const TAG_CLEAR = 36;
/** The height of the bottom left corner the plan map takes (CSS px). */
const MAP_CORNER = 264;

const LINE = 'rgba(255, 255, 255, 0.1)';

/** Where the section camera is looking. */
export interface SectionFrame {
  /** Distance along the section from A at the view's centre (m). */
  along: number;
  /** RL at the view's centre (m). */
  rl: number;
  /** Distance from A to B (m). */
  length: number;
  /** The width and height of the view (m). */
  across: number;
  up: number;
}

/** The lines drawn, on screen in CSS px from the viewport's corner. */
export interface GridLines {
  spacing: number;
  along: { value: number; x: number }[];
  rl: { value: number; y: number }[];
}

/** The smallest 1, 2 or 5 times a power of ten that is at least `least`. */
export function gridSpacing(least: number): number {
  const power = 10 ** Math.floor(Math.log10(least));
  return [1, 2, 5, 10].map((m) => m * power).find((spacing) => spacing >= least) ?? 10 * power;
}

/** The multiples of `spacing` from `low` to `high`. */
export function multiplesBetween(low: number, high: number, spacing: number): number[] {
  const multiples: number[] = [];
  // Adding zero turns -0 into 0, so it never reads as "-0".
  for (let k = Math.ceil(low / spacing); k * spacing <= high; k++) multiples.push(k * spacing + 0);
  return multiples;
}

export class SectionGrid {
  private canvas = Object.assign(document.createElement('canvas'), { className: 'section-grid', hidden: true });
  /** What is drawn, to draw only when the view has moved. */
  private drawn = '';
  /** The lines drawn, for checks. */
  lines: GridLines = { spacing: 1, along: [], rl: [] };
  /** How long the last drawing took (ms). */
  lastDrawMs = 0;

  constructor(private host: HTMLElement) {
    host.append(this.canvas);
  }

  get canvasElement(): HTMLCanvasElement {
    return this.canvas;
  }

  get shown(): boolean {
    return !this.canvas.hidden;
  }

  show(): void {
    this.drawn = '';
    this.canvas.hidden = false;
  }

  hide(): void {
    this.canvas.hidden = true;
  }

  draw(frame: SectionFrame): void {
    const { clientWidth: w, clientHeight: h } = this.host;
    const key = [...Object.values(frame), w, h].join();
    if (key === this.drawn || w === 0 || h === 0) return;
    this.drawn = key;
    const started = performance.now();
    const ratio = window.devicePixelRatio;
    const [width, height] = [Math.round(w * ratio), Math.round(h * ratio)];
    if (this.canvas.width !== width || this.canvas.height !== height) Object.assign(this.canvas, { width, height });
    const context = this.canvas.getContext('2d');
    if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, w, h);

    const [across, up] = [frame.across / w, frame.up / h];
    const toX = (along: number) => w / 2 + (along - frame.along) / across;
    const toY = (rl: number) => h / 2 - (rl - frame.rl) / up;
    const spacing = gridSpacing(LEAST_GAP * Math.max(across, up));
    const along = multiplesBetween(frame.along - frame.across / 2, frame.along + frame.across / 2, spacing).map((value) => ({ value, x: toX(value) }));
    const rl = multiplesBetween(frame.rl - frame.up / 2, frame.rl + frame.up / 2, spacing).map((value) => ({ value, y: toY(value) }));
    this.lines = { spacing, along, rl };

    // On a whole device pixel, so a line stays one pixel wide wherever it falls.
    const sharp = (at: number) => (Math.round(at * ratio) + 0.5) / ratio;
    const stroke = (colour: string, draw: () => void) => {
      context.strokeStyle = colour;
      context.lineWidth = 1 / ratio;
      context.beginPath();
      draw();
      context.stroke();
    };
    const upright = (x: number) => {
      context.moveTo(sharp(x), 0);
      context.lineTo(sharp(x), h);
    };
    stroke(LINE, () => {
      for (const { x } of along) upright(x);
      for (const { y } of rl) {
        context.moveTo(0, sharp(y));
        context.lineTo(w, sharp(y));
      }
    });
    const tags = [
      { text: 'A', x: toX(0) },
      { text: 'B', x: toX(frame.length) },
    ];
    stroke(STEPS, () => tags.forEach(({ x }) => upright(x)));

    context.font = '11px system-ui, sans-serif';
    context.textBaseline = 'middle';
    const label = (text: string, x: number, y: number, align: CanvasTextAlign, colour: string) => {
      const width = context.measureText(text).width;
      const left = align === 'left' ? x : align === 'right' ? x - width : x - width / 2;
      context.fillStyle = SHADE;
      context.fillRect(left - 4, y - 8, width + 8, 16);
      context.textAlign = align;
      context.fillStyle = colour;
      context.fillText(text, x, y);
    };
    const digits = Math.max(0, -Math.floor(Math.log10(spacing)));
    const reading = (value: number) => value.toLocaleString('en-AU', { minimumFractionDigits: digits, maximumFractionDigits: digits });
    for (const { value, y } of rl) {
      if (y < TOP_BAND) continue;
      if (y < h - MAP_CORNER) label(`RL ${reading(value)}`, 8, y, 'left', TEXT);
      label(`RL ${reading(value)}`, w - 8, y, 'right', TEXT);
    }
    for (const { value, x } of along) {
      if (tags.every((tag) => Math.abs(tag.x - x) > TAG_CLEAR)) label(`${reading(value)} m`, x, TOP_BAND / 2, 'center', TEXT);
    }
    for (const { text, x } of tags) label(text, x, TOP_BAND / 2, 'center', SECTION);
    this.lastDrawMs = performance.now() - started;
  }
}
